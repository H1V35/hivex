//! Residue a project declares outside its worktrees: report each item, and move the unneeded items
//! of deterministic classes to the Trash. Nothing is deleted permanently and nothing empties the
//! Trash, which the owner empties by hand.
use crate::compatibility::normalize_path;
use crate::error::{HiveXError, Result};
use globset::GlobBuilder;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The declaration the workspace hygiene procedure describes, relative to the project root.
pub const DECLARATION: &str = "docs/procedures/workspace-hygiene.json";
const CHECK_TIMEOUT: Duration = Duration::from_secs(60);
/// Recoverable trash commands tried in order when `HIVEX_TRASH` is not set.
const TRASH_PROGRAMS: [&[&str]; 2] = [&["trash"], &["gio", "trash"]];

#[derive(Deserialize)]
struct Declaration {
  classes: Vec<Class>,
}

#[derive(Deserialize)]
struct Class {
  name: String,
  paths: Vec<String>,
  unneeded: String,
  removal: Removal,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Removal {
  Deterministic,
  Review,
}

#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
  Unneeded,
  Needed,
  Uncertain,
  /// A deterministic item containing another declared item, which moving it would take along.
  Overlaps,
  /// The filesystem root, the home folder, the project or a folder containing one of them.
  Protected,
  Trashed,
  Failed,
}

#[derive(Serialize)]
pub struct Item {
  pub class: String,
  /// Shown lossily when not UTF-8; operations use the path itself.
  #[serde(serialize_with = "lossy")]
  pub path: PathBuf,
  pub bytes: u64,
  pub status: Status,
  #[serde(skip)]
  identity: Option<PathBuf>,
  #[serde(skip)]
  index: usize,
  #[serde(skip)]
  removal: Removal,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassReport {
  pub name: String,
  pub removal: Removal,
  pub items: usize,
  pub bytes: u64,
  pub unneeded_bytes: u64,
}

#[derive(Default, Serialize)]
pub struct Tally {
  pub items: usize,
  pub bytes: u64,
}

pub struct Sweep {
  pub classes: Vec<ClassReport>,
  pub items: Vec<Item>,
  pub moved: Tally,
  pub failed: Tally,
}

fn lossy<S: serde::Serializer>(path: &Path, serializer: S) -> std::result::Result<S::Ok, S::Error> {
  serializer.serialize_str(&path.to_string_lossy())
}

/// The project root and the folders no item may be or contain, by their canonical paths.
struct Guard {
  root: PathBuf,
  keep: Vec<PathBuf>,
}

/// Report the residue `declaration` lists for the project at `root` and, when `trash`, move the
/// unneeded items of its deterministic classes to the Trash after checking each again.
pub fn sweep(root: &Path, declaration: &str, trash: bool) -> Result<Sweep> {
  let classes = load(&normalize_path(&root.join(declaration)))?;
  let home = std::env::home_dir();
  let guard = Guard {
    root: root.to_path_buf(),
    keep: [Some(root.to_path_buf()), home]
      .into_iter()
      .flatten()
      .filter_map(|path| fs::canonicalize(path).ok())
      .collect(),
  };
  let mut items = Vec::new();
  for (index, class) in classes.iter().enumerate() {
    items.extend(class_items(index, class, &guard)?);
  }
  mark_overlaps(&mut items);
  let reports = classes
    .iter()
    .enumerate()
    .map(|(index, class)| report(index, class, &items))
    .collect();
  let (moved, failed) = if trash {
    move_unneeded(&mut items, &classes, &guard)?
  } else {
    (Tally::default(), Tally::default())
  };
  Ok(Sweep {
    classes: reports,
    items,
    moved,
    failed,
  })
}

fn invalid(message: impl Into<String>) -> HiveXError {
  HiveXError::new("INVALID_CONFIG", message)
}

fn load(path: &Path) -> Result<Vec<Class>> {
  let shown = path.to_string_lossy();
  let bytes = fs::read(path).map_err(|error| invalid(format!("{shown}: {error}")))?;
  let declaration: Declaration =
    serde_json::from_slice(&bytes).map_err(|error| invalid(format!("{shown}: {error}")))?;
  if let Some(class) = declaration
    .classes
    .iter()
    .find(|class| class.unneeded.trim().is_empty())
  {
    return Err(invalid(format!(
      "{shown}: class '{}' needs a nonempty unneeded check",
      class.name
    )));
  }
  if let Some(pattern) = declaration
    .classes
    .iter()
    .flat_map(|class| &class.paths)
    .find(|pattern| pattern.contains("**"))
  {
    return Err(invalid(format!(
      "{shown}: '{pattern}': `**` is not supported; name each level"
    )));
  }
  Ok(declaration.classes)
}

fn class_items(index: usize, class: &Class, guard: &Guard) -> Result<Vec<Item>> {
  let mut paths = Vec::new();
  let mut unreadable = Vec::new();
  for pattern in &class.paths {
    paths.extend(expand(pattern, &guard.root, &mut unreadable)?);
  }
  // A folder a pattern cannot list may hold items of this class: it stays and is never checked.
  paths.extend(unreadable.iter().cloned());
  paths.sort();
  paths.dedup();
  let mut items: Vec<Item> = Vec::new();
  for path in paths {
    let identity = identity(&path);
    // Patterns can name one item in several ways; it is still one item.
    if items
      .iter()
      .any(|item| item.identity.is_some() && item.identity == identity)
    {
      continue;
    }
    items.push(Item {
      class: class.name.clone(),
      bytes: size(&path),
      status: if unreadable.contains(&path) {
        Status::Uncertain
      } else {
        status(&path, &class.unneeded, guard)
      },
      path,
      identity,
      index,
      removal: class.removal,
    });
  }
  Ok(items)
}

/// The paths a pattern matches. `~` and a relative pattern start at the home folder and the project
/// root, taken literally; `$NAME` and `${NAME}` expand; and `*`, `?`, `[…]` and `{…}` match within
/// one path component. Folders it cannot list go to `unreadable`.
fn expand(pattern: &str, root: &Path, unreadable: &mut Vec<PathBuf>) -> Result<Vec<PathBuf>> {
  let tilde = pattern
    .strip_prefix('~')
    .filter(|rest| rest.is_empty() || rest.starts_with(['/', '\\']));
  let (start, rest) = match (tilde, std::env::home_dir()) {
    (Some(rest), Some(home)) => (home, rest.trim_start_matches(['/', '\\'])),
    _ => (root.to_path_buf(), pattern),
  };
  let rest = PathBuf::from(variables(rest));
  let mut found = vec![if rest.has_root() {
    PathBuf::new()
  } else {
    start
  }];
  for component in rest.components() {
    found = step(found, component, unreadable)?;
  }
  // Only absence removes a match: an item that cannot be inspected is still reported.
  found.retain(|path| {
    fs::symlink_metadata(path).map_or_else(|error| error.kind() != ErrorKind::NotFound, |_| true)
  });
  Ok(found.iter().map(|path| normalize_path(path)).collect())
}

fn variables(pattern: &str) -> String {
  let variable = Regex::new(r"\$(?:\{(\w+)\}|(\w+))").expect("valid variable pattern");
  variable
    .replace_all(pattern, |found: &regex::Captures| {
      let name = found
        .get(1)
        .or_else(|| found.get(2))
        .map_or("", |m| m.as_str());
      std::env::var(name).unwrap_or_else(|_| found[0].to_owned())
    })
    .into_owned()
}

fn step(
  bases: Vec<PathBuf>,
  component: Component,
  unreadable: &mut Vec<PathBuf>,
) -> Result<Vec<PathBuf>> {
  let text = component.as_os_str().to_string_lossy();
  if !matches!(component, Component::Normal(_)) || !text.contains(['*', '?', '[', '{']) {
    return Ok(bases.into_iter().map(|base| base.join(component)).collect());
  }
  let matcher = GlobBuilder::new(&text)
    .literal_separator(true)
    .build()
    .map_err(|error| invalid(error.to_string()))?
    .compile_matcher();
  // Like the shell, a wildcard does not match hidden names unless the pattern starts with a dot.
  let hidden = text.starts_with('.');
  let mut next = Vec::new();
  for base in bases {
    let entries = match fs::read_dir(&base) {
      Ok(entries) => entries,
      Err(error) if listable(&base, &error) => {
        unreadable.push(normalize_path(&base));
        continue;
      }
      Err(_) => continue,
    };
    for name in entries.flatten().map(|entry| entry.file_name()) {
      let shown = name.to_string_lossy();
      if (hidden || !shown.starts_with('.')) && matcher.is_match(&*shown) {
        next.push(base.join(&name));
      }
    }
  }
  Ok(next)
}

/// Whether a folder that could not be listed may still exist as a folder.
fn listable(base: &Path, error: &std::io::Error) -> bool {
  match fs::metadata(base) {
    Ok(metadata) => metadata.is_dir(),
    Err(_) => error.kind() != ErrorKind::NotFound,
  }
}

/// The canonical path of the item itself, without following it when it is a link, so that aliases
/// such as another letter case on a case-insensitive volume are the same item.
fn identity(path: &Path) -> Option<PathBuf> {
  let metadata = fs::symlink_metadata(path).ok()?;
  if !metadata.file_type().is_symlink() {
    return fs::canonicalize(path).ok();
  }
  let parent = fs::canonicalize(path.parent()?).ok()?;
  Some(parent.join(stored_name(&parent, path.file_name()?)?))
}

/// A link's name as its folder stores it, which a case-insensitive volume may spell otherwise.
fn stored_name(parent: &Path, name: &std::ffi::OsStr) -> Option<std::ffi::OsString> {
  let names: Vec<_> = fs::read_dir(parent)
    .ok()?
    .flatten()
    .map(|entry| entry.file_name())
    .collect();
  if names.iter().any(|stored| stored == name) {
    return Some(name.to_owned());
  }
  let wanted = name.to_string_lossy().to_lowercase();
  names
    .into_iter()
    .find(|stored| stored.to_string_lossy().to_lowercase() == wanted)
}

/// Bytes the item occupies on disk, without following links.
fn size(path: &Path) -> u64 {
  let mut total = 0;
  let mut pending = vec![path.to_path_buf()];
  while let Some(next) = pending.pop() {
    let Ok(metadata) = fs::symlink_metadata(&next) else {
      continue;
    };
    total += used(&metadata);
    if metadata.is_dir()
      && let Ok(entries) = fs::read_dir(&next)
    {
      pending.extend(entries.flatten().map(|entry| entry.path()));
    }
  }
  total
}

#[cfg(unix)]
fn used(metadata: &fs::Metadata) -> u64 {
  use std::os::unix::fs::MetadataExt;
  metadata.blocks() * 512
}

#[cfg(not(unix))]
fn used(metadata: &fs::Metadata) -> u64 {
  metadata.len()
}

fn status(path: &Path, check: &str, guard: &Guard) -> Status {
  let Some(identity) = identity(path) else {
    return Status::Uncertain;
  };
  if identity.parent().is_none() || guard.keep.iter().any(|keep| keep.starts_with(&identity)) {
    return Status::Protected;
  }
  let mut command = shell(check);
  let Ok(mut child) = prepare(&mut command, path, &guard.root).spawn() else {
    return Status::Uncertain;
  };
  let started = Instant::now();
  let code = loop {
    match child.try_wait() {
      Ok(Some(exit)) => break exit.code(),
      Ok(None) if started.elapsed() < CHECK_TIMEOUT => {
        std::thread::sleep(Duration::from_millis(10));
      }
      _ => {
        let _ = child.kill();
        let _ = child.wait();
        break None;
      }
    }
  };
  match code {
    Some(0) => Status::Unneeded,
    Some(1) => Status::Needed,
    _ => Status::Uncertain,
  }
}

fn prepare<'a>(command: &'a mut Command, item: &Path, root: &Path) -> &'a mut Command {
  command
    .current_dir(root)
    .env("RESIDUE", item)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
}

/// The platform shell running `script`: `/bin/sh -c` on Unix and `cmd /C` on Windows.
#[cfg(unix)]
fn shell(script: &str) -> Command {
  let mut command = Command::new("/bin/sh");
  command.arg("-c").arg(script);
  command
}

#[cfg(windows)]
fn shell(script: &str) -> Command {
  use std::os::windows::process::CommandExt;
  let program = std::env::var_os("COMSPEC").unwrap_or_else(|| "cmd.exe".into());
  let mut command = Command::new(program);
  // `/S` strips the outer quotes and runs the rest as written.
  command
    .args(["/D", "/S", "/C"])
    .raw_arg(format!("\"{script}\""));
  command
}

fn mark_overlaps(items: &mut [Item]) {
  for index in 0..items.len() {
    let item = &items[index];
    if item.removal != Removal::Deterministic || item.status != Status::Unneeded {
      continue;
    }
    let Some(folder) = &item.identity else {
      continue;
    };
    // An item without an identity could not be inspected; its written path still counts.
    let overlaps = items.iter().enumerate().any(|(other, inner)| {
      other != index
        && match &inner.identity {
          Some(inner) => inner.starts_with(folder),
          None => inner.path.starts_with(folder) || inner.path.starts_with(&item.path),
        }
    });
    if overlaps {
      items[index].status = Status::Overlaps;
    }
  }
}

fn report(index: usize, class: &Class, items: &[Item]) -> ClassReport {
  let own: Vec<&Item> = items.iter().filter(|item| item.index == index).collect();
  ClassReport {
    name: class.name.clone(),
    removal: class.removal,
    items: own.len(),
    bytes: own.iter().map(|item| item.bytes).sum(),
    unneeded_bytes: own
      .iter()
      .filter(|item| item.status == Status::Unneeded)
      .map(|item| item.bytes)
      .sum(),
  }
}

/// How items go to the Trash: the `HIVEX_TRASH` command line, run by the platform shell with
/// `RESIDUE` set, or the first available recoverable program.
enum Trash {
  Unresolved,
  Shell(String),
  Program(&'static [&'static str]),
}

fn trash_override() -> Result<Trash> {
  let Some(value) = std::env::var_os("HIVEX_TRASH") else {
    return Ok(Trash::Unresolved);
  };
  match value.into_string() {
    Ok(command) if !command.trim().is_empty() => Ok(Trash::Shell(command)),
    _ => Err(unavailable("HIVEX_TRASH is empty or not valid UTF-8")),
  }
}

fn unavailable(message: &str) -> HiveXError {
  HiveXError::new("TRASH_UNAVAILABLE", message)
}

fn move_unneeded(items: &mut [Item], classes: &[Class], guard: &Guard) -> Result<(Tally, Tally)> {
  let mut moved = Tally::default();
  let mut failed = Tally::default();
  let mut trash = None;
  let movable =
    |item: &&mut Item| item.removal == Removal::Deterministic && item.status == Status::Unneeded;
  for item in items.iter_mut().filter(movable) {
    // Live work may have claimed the item while the rest of the sweep ran.
    item.status = status(&item.path, &classes[item.index].unneeded, guard);
    if item.status != Status::Unneeded {
      continue;
    }
    let trash = match &mut trash {
      Some(trash) => trash,
      None => trash.insert(trash_override()?),
    };
    let gone = send(trash, &item.path, &guard.root)? && fs::symlink_metadata(&item.path).is_err();
    let tally = if gone { &mut moved } else { &mut failed };
    tally.items += 1;
    tally.bytes += item.bytes;
    item.status = if gone {
      Status::Trashed
    } else {
      Status::Failed
    };
  }
  Ok((moved, failed))
}

/// Move `path` to the Trash, resolving the program on first use; refuses when none is available.
fn send(trash: &mut Trash, path: &Path, root: &Path) -> Result<bool> {
  if let Trash::Shell(script) = trash {
    let mut command = shell(script);
    return Ok(
      prepare(&mut command, path, root)
        .status()
        .is_ok_and(|exit| exit.success()),
    );
  }
  let programs: &[&[&str]] = match trash {
    Trash::Program(program) => &[*program],
    _ => &TRASH_PROGRAMS,
  };
  for program in programs {
    let mut command = Command::new(program[0]);
    command.args(&program[1..]).arg(path);
    match prepare(&mut command, path, root).status() {
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
      result => {
        *trash = Trash::Program(program);
        return Ok(result.is_ok_and(|exit| exit.success()));
      }
    }
  }
  Err(unavailable(
    "No recoverable trash command found: install `trash` or set HIVEX_TRASH",
  ))
}
