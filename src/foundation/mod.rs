use crate::compatibility::{normalize_path, project_directory};
use crate::error::{HivexError, Result};
use serde_json::{Value, json};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

// Managed rules stay last so earlier project rules cannot re-include their paths. Claude's skill
// links are created on each machine, because Git cannot share them with Windows.
const IGNORE_RULES: [&str; 3] = ["/.claude/skills/hivex*", "/.hivex/", "/.reviews/"];
/// This release; `hivex.json` pins the release a project expects.
const VERSION: &str = env!("CARGO_PKG_VERSION");
const CONFIG: &str = "hivex.json";

mod extension;

use extension::Pin;

/// Bundled files under `prefix`, by their path relative to it.
macro_rules! bundled {
  ($prefix:literal: $($path:literal),* $(,)?) => {
    &[$(($path, include_bytes!(concat!($prefix, $path)))),*]
  };
}

/// Project foundation files: created when missing and never changed afterwards.
static TEMPLATE_FILES: &[(&str, &[u8])] = bundled![
  "../../skills/hivex/assets/project/":
  "AGENTS.md",
  "docs/CONTEXT.md",
  "docs/PRD.md",
  "docs/README.md",
  "docs/adr/README.md",
  "docs/guidelines/engineering.md",
  "docs/guidelines/triage-labels.md",
  "docs/procedures/independent-review.md",
  "docs/procedures/independent-review.schema.json",
  "docs/procedures/issue-tracker.md",
  "docs/procedures/self-hosted-runners.md",
  "hivex.json",
];

/// The complete skills, relative to `.agents/skills/`: `init` creates missing files and `update`
/// makes them identical to this release.
static SKILL_FILES: &[(&str, &[u8])] = bundled![
  "../../skills/":
  "hivex-design/SKILL.md",
  "hivex-design/references/visual-review.md",
  "hivex-document/SKILL.md",
  "hivex-git/SKILL.md",
  "hivex-git/assets/labels.json",
  "hivex-implement/SKILL.md",
  "hivex-review/SKILL.md",
  "hivex/SKILL.md",
  "hivex/assets/project/AGENTS.md",
  "hivex/assets/project/docs/CONTEXT.md",
  "hivex/assets/project/docs/PRD.md",
  "hivex/assets/project/docs/README.md",
  "hivex/assets/project/docs/adr/README.md",
  "hivex/assets/project/docs/guidelines/engineering.md",
  "hivex/assets/project/docs/guidelines/triage-labels.md",
  "hivex/assets/project/docs/procedures/independent-review.md",
  "hivex/assets/project/docs/procedures/independent-review.schema.json",
  "hivex/assets/project/docs/procedures/issue-tracker.md",
  "hivex/assets/project/docs/procedures/self-hosted-runners.md",
  "hivex/assets/project/hivex.json",
  "hivex/references/markdown.md",
  "hivex/templates/README.md",
  "hivex/templates/rust/docs/guidelines/rust.md",
  "hivex/templates/swift/docs/guidelines/ios.md",
  "hivex/templates/swift/docs/guidelines/swift.md",
  "hivex/templates/typescript/docs/guidelines/typescript.md",
];

/// What a project installs: the bundled foundation with its extensions applied.
struct Bundle {
  templates: Vec<(String, Vec<u8>)>,
  /// Skill files, relative to `.agents/skills/`.
  skills: Vec<(String, Vec<u8>)>,
}

impl Bundle {
  fn with_extensions(root: &Path, pins: &[Pin]) -> Result<Self> {
    let owned = |files: &[(&str, &[u8])]| -> Vec<(String, Vec<u8>)> {
      files
        .iter()
        .map(|(path, bytes)| ((*path).to_owned(), bytes.to_vec()))
        .collect()
    };
    let mut bundle = Self {
      templates: owned(TEMPLATE_FILES),
      skills: owned(SKILL_FILES),
    };
    for pin in pins {
      extension::apply(&mut bundle, &extension::fetch(root, pin)?)?;
    }
    Ok(bundle)
  }

  fn skill_names(&self) -> Vec<String> {
    let mut names: Vec<String> = self
      .skills
      .iter()
      .map(|(path, _)| extension::skill_name(path).to_owned())
      .collect();
    names.dedup();
    names
  }

  /// Claude's links are per machine, so each one is ignored.
  fn ignore_rules(&self) -> Vec<String> {
    let links = self
      .skill_names()
      .into_iter()
      .filter(|name| !name.starts_with("hivex"))
      .map(|name| format!("/.claude/skills/{name}"));
    IGNORE_RULES
      .iter()
      .map(|rule| (*rule).to_owned())
      .chain(links)
      .collect()
  }
}

/// A host CLI and the idempotent steps it runs for the ponytail plugin.
type HostSteps = (&'static str, &'static [&'static [&'static str]]);

/// `init` installs ponytail only where `plugin list --json` does not show it.
const PONYTAIL_INSTALL: [HostSteps; 2] = [
  (
    "claude",
    &[
      &["plugin", "marketplace", "add", "DietrichGebert/ponytail"],
      &["plugin", "install", "ponytail@ponytail"],
    ],
  ),
  (
    "codex",
    &[
      &["plugin", "marketplace", "add", "DietrichGebert/ponytail"],
      &["plugin", "add", "ponytail@ponytail"],
    ],
  ),
];

/// `update` also refreshes the marketplace and brings ponytail to its latest release.
const PONYTAIL_UPDATE: [HostSteps; 2] = [
  (
    "claude",
    &[
      &["plugin", "marketplace", "add", "DietrichGebert/ponytail"],
      &["plugin", "marketplace", "update", "ponytail"],
      &["plugin", "install", "ponytail@ponytail"],
      &["plugin", "update", "ponytail@ponytail"],
    ],
  ),
  (
    "codex",
    &[
      &["plugin", "marketplace", "add", "DietrichGebert/ponytail"],
      &["plugin", "marketplace", "upgrade", "ponytail"],
      &["plugin", "add", "ponytail@ponytail"],
    ],
  ),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum OperationState {
  Created,
  Preserved,
  Updated,
}

impl OperationState {
  const fn planned(exists: bool) -> Self {
    if exists {
      Self::Preserved
    } else {
      Self::Created
    }
  }
}

struct FileOperation {
  absolute_path: PathBuf,
  content: Content,
  path: String,
  state: OperationState,
}

enum Content {
  Bytes(Vec<u8>),
  /// Claude's link to the named skill in `.agents/skills`.
  SkillLink(String),
  /// A former npm link to a skill, replaced by a copy.
  Unlink,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DestinationKind {
  File,
  Link,
}

struct Destination {
  absolute_path: PathBuf,
  exists: bool,
}

fn error(code: &str, message: impl Into<String>) -> HivexError {
  HivexError::new(code, message)
}

fn project_root(requested: &str) -> Result<PathBuf> {
  let directory = project_directory(requested).map_err(|mut root_error| {
    // init has always reported the reason in its message, without details.
    let details = root_error.details.take();
    if let Some(reason) = details
      .as_ref()
      .and_then(|details| details["reason"].as_str())
    {
      root_error.message = format!("{}: {reason}", root_error.message);
    }
    root_error
  })?;
  fs::canonicalize(directory).map_err(|read_error| error("INVALID_ROOT", read_error.to_string()))
}

fn destination(root: &Path, relative_path: &str, kind: DestinationKind) -> Result<Destination> {
  let absolute_path = normalize_path(&root.join(relative_path));
  let relative = absolute_path.strip_prefix(root).map_err(|_| {
    error(
      "INVALID_DESTINATION",
      format!("Initialization path escapes the project root: {relative_path}"),
    )
  })?;
  if relative.as_os_str().is_empty() {
    return Err(error(
      "INVALID_DESTINATION",
      format!("Initialization path escapes the project root: {relative_path}"),
    ));
  }

  let components: Vec<_> = relative.components().collect();
  let mut current = root.to_path_buf();
  for (index, component) in components.iter().enumerate() {
    let Component::Normal(part) = component else {
      return Err(error(
        "INVALID_DESTINATION",
        "Initialization paths must be relative project files",
      ));
    };
    current.push(part);
    let metadata = match fs::symlink_metadata(&current) {
      Ok(metadata) => metadata,
      Err(read_error) if read_error.kind() == std::io::ErrorKind::NotFound => {
        return Ok(Destination {
          absolute_path,
          exists: false,
        });
      }
      Err(read_error) => {
        return Err(error(
          "INVALID_DESTINATION",
          format!("Unable to inspect initialization path {relative_path}: {read_error}"),
        ));
      }
    };
    validate_destination_component(
      &metadata,
      relative_path,
      index == components.len() - 1,
      kind,
    )?;
  }
  Ok(Destination {
    absolute_path,
    exists: true,
  })
}

fn split_crlf_lines(text: &str) -> impl Iterator<Item = &str> {
  text
    .split('\n')
    .map(|line| line.strip_suffix('\r').unwrap_or(line))
}

fn missing_ignore_rules<'a>(text: &str, rules: &'a [String]) -> Vec<&'a String> {
  let lines: Vec<_> = split_crlf_lines(text).collect();
  // The last matching rule wins, so a later negation could re-include local state.
  let effective = lines
    .iter()
    .rposition(|line| line.starts_with('!'))
    .map_or(&lines[..], |negation| &lines[negation + 1..]);
  rules
    .iter()
    .filter(|rule| !effective.contains(&rule.as_str()))
    .collect()
}

fn ignore_update(existing: Option<&[u8]>, rules: &[String]) -> Option<Vec<u8>> {
  let text = existing.map(String::from_utf8_lossy).unwrap_or_default();
  let missing = missing_ignore_rules(&text, rules);
  if missing.is_empty() {
    return None;
  }
  let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
  let mut bytes = existing.unwrap_or_default().to_vec();
  if !text.is_empty() && !text.ends_with('\n') {
    bytes.extend_from_slice(newline.as_bytes());
  }
  for rule in missing {
    bytes.extend_from_slice(rule.as_bytes());
    bytes.extend_from_slice(newline.as_bytes());
  }
  Some(bytes)
}

fn template_operations(root: &Path, bundle: &Bundle, pins: &[Pin]) -> Result<Vec<FileOperation>> {
  let created_at = creation_date()?;
  bundle
    .templates
    .iter()
    .map(|(path, bytes)| {
      let target = destination(root, path, DestinationKind::File)?;
      let bytes = match path.as_str() {
        _ if target.exists => bytes.clone(),
        CONFIG => pinned_config(&config_object(bytes)?, pins),
        _ if is_dated(path) => dated_template(bytes, &created_at),
        _ => bytes.clone(),
      };
      Ok(FileOperation {
        absolute_path: target.absolute_path,
        content: Content::Bytes(bytes),
        path: path.clone(),
        state: OperationState::planned(target.exists),
      })
    })
    .collect()
}

/// The configuration object, accepting the optional UTF-8 BOM that source selection accepts.
fn config_object(bytes: &[u8]) -> Result<serde_json::Map<String, Value>> {
  let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
  match serde_json::from_slice(bytes) {
    Ok(Value::Object(object)) => Ok(object),
    _ => Err(error(
      "INVALID_CONFIG",
      format!("{CONFIG} must contain a JSON object"),
    )),
  }
}

/// `config` with `version` set to this release and `extensions` to `pins` as its first fields,
/// pretty-printed.
fn pinned_config(config: &serde_json::Map<String, Value>, pins: &[Pin]) -> Vec<u8> {
  let mut object = serde_json::Map::new();
  object.insert("version".into(), VERSION.into());
  if !pins.is_empty() {
    let pins = pins
      .iter()
      .map(|pin| json!({"repository": pin.repository, "version": pin.version}))
      .collect();
    object.insert("extensions".into(), Value::Array(pins));
  }
  object.extend(
    config
      .iter()
      .filter(|(key, _)| !matches!(key.as_str(), "version" | "extensions"))
      .map(|(key, value)| (key.clone(), value.clone())),
  );
  let mut text = serde_json::to_string_pretty(&Value::Object(object)).expect("JSON serializes");
  text.push('\n');
  text.into_bytes()
}

/// `MAJOR.MINOR.PATCH` as comparable numbers.
fn release(version: &str) -> Option<[u64; 3]> {
  let mut parts = version.split('.').map(|part| part.parse().ok());
  let release = [parts.next()??, parts.next()??, parts.next()??];
  parts.next().is_none().then_some(release)
}

/// The command that installs `version`, for error messages and warnings.
pub fn install_command(version: &str) -> String {
  format!(
    "cargo install --git https://github.com/H1V35/hivex --tag v{version} --locked hivex --bin hivex --force"
  )
}

/// The release pinned in the project's `hivex.json`, if it has one.
pub fn pinned_version(root: &Path) -> Result<Option<String>> {
  let path = root.join(CONFIG);
  let Ok(text) = fs::read(&path) else {
    return Ok(None);
  };
  let config = config_object(&text)?;
  let Some(version) = config.get("version") else {
    return Ok(None);
  };
  version
    .as_str()
    .filter(|version| release(version).is_some())
    .map(|version| Some(version.to_owned()))
    .ok_or_else(|| {
      error(
        "INVALID_CONFIG",
        "hivex.json version must be MAJOR.MINOR.PATCH",
      )
    })
}

/// The warning other commands carry when the project pins another release. Source selection
/// reports an invalid configuration, so it is not repeated here.
pub fn version_warning(root: &Path) -> Option<Value> {
  let pinned = pinned_version(root).ok().flatten()?;
  (pinned != VERSION).then(|| {
    let install = install_command(&pinned);
    json!({"installed": VERSION, "pinned": pinned, "install": install})
  })
}

fn version_mismatch(pinned: &str) -> HivexError {
  error(
    "VERSION_MISMATCH",
    format!(
      "This project pins Hivex {pinned}, but {VERSION} is installed. Install it with: {}",
      install_command(pinned)
    ),
  )
  .with_details(json!({"installed": VERSION, "pinned": pinned}))
}

/// `update` moves the pin to this release, but never back to an older one, and records the
/// extension releases it installed.
fn pin_operation(root: &Path, pins: &[Pin], pins_changed: bool) -> Result<Option<FileOperation>> {
  let target = destination(root, CONFIG, DestinationKind::File)?;
  if !target.exists {
    return Ok(None);
  }
  let pinned = pinned_version(root)?;
  if let Some(pinned) = &pinned
    && release(pinned) > release(VERSION)
  {
    return Err(version_mismatch(pinned));
  }
  let config = config_object(&read_existing(&target, CONFIG)?)?;
  Ok(Some(FileOperation {
    absolute_path: target.absolute_path,
    content: Content::Bytes(pinned_config(&config, pins)),
    path: CONFIG.to_owned(),
    state: if pinned.as_deref() == Some(VERSION) && !pins_changed {
      OperationState::Preserved
    } else {
      OperationState::Updated
    },
  }))
}

/// New Markdown under `docs/` records its creation day.
fn is_dated(path: &str) -> bool {
  path.starts_with("docs/")
    && Path::new(path)
      .extension()
      .is_some_and(|extension| extension == "md")
}

/// Today's UTC date as `YYYY-MM-DD`.
fn creation_date() -> Result<String> {
  let elapsed = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_err(|_| error("INIT_DATE_FAILED", "The system clock is before 1970"))?;
  Ok(civil_date(elapsed.as_secs() / 86_400))
}

/// Proleptic Gregorian date of a day count since 1970-01-01 (Hinnant's `civil_from_days`).
fn civil_date(days: u64) -> String {
  let shifted = days + 719_468;
  let era = shifted / 146_097;
  let day_of_era = shifted % 146_097;
  let year_of_era =
    (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
  let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
  let month_index = (5 * day_of_year + 2) / 153;
  let day = day_of_year - (153 * month_index + 2) / 5 + 1;
  let month = (month_index + 2) % 12 + 1;
  let year = era * 400 + year_of_era + u64::from(month <= 2);
  format!("{year:04}-{month:02}-{day:02}")
}

/// Extension files may lack frontmatter; they are installed unchanged.
fn dated_template(bytes: &[u8], created_at: &str) -> Vec<u8> {
  let Some((header, body)) = std::str::from_utf8(bytes)
    .ok()
    .and_then(|text| text.strip_prefix("---\n"))
    .and_then(|text| text.split_once("\n---\n"))
  else {
    return bytes.to_vec();
  };
  let mut metadata = header
    .lines()
    .filter(|line| {
      !["created_at:", "updated_at:", "archived_at:"]
        .iter()
        .any(|key| line.starts_with(key))
    })
    .map(str::to_owned)
    .collect::<Vec<_>>();
  let date_position = metadata
    .iter()
    .position(|line| line.starts_with("tags:") || line.starts_with("source:"))
    .unwrap_or(metadata.len());
  metadata.insert(date_position, format!("created_at: {created_at}"));
  format!("---\n{}\n---\n{body}", metadata.join("\n")).into_bytes()
}

fn ignore_operation(root: &Path, rules: &[String]) -> Result<FileOperation> {
  let relative_path = ".gitignore";
  let target = destination(root, relative_path, DestinationKind::File)?;
  let existing = if target.exists {
    Some(read_existing(&target, relative_path)?)
  } else {
    None
  };
  let (bytes, state) = match ignore_update(existing.as_deref(), rules) {
    Some(bytes) => (
      bytes,
      if target.exists {
        OperationState::Updated
      } else {
        OperationState::Created
      },
    ),
    None => (existing.unwrap_or_default(), OperationState::Preserved),
  };
  Ok(FileOperation {
    absolute_path: target.absolute_path,
    content: Content::Bytes(bytes),
    path: relative_path.to_owned(),
    state,
  })
}

fn read_existing(target: &Destination, relative_path: &str) -> Result<Vec<u8>> {
  fs::read(&target.absolute_path).map_err(|read_error| {
    error(
      "INIT_READ_FAILED",
      format!("Unable to read {relative_path}: {read_error}"),
    )
  })
}

fn skill_operations(root: &Path, bundle: &Bundle, refresh: bool) -> Result<Vec<FileOperation>> {
  let mut operations = Vec::new();
  for name in bundle.skill_names() {
    let files: Vec<_> = bundle
      .skills
      .iter()
      .filter(|(path, _)| extension::skill_name(path) == name)
      .collect();
    operations.extend(skill_directory_operations(root, &name, &files, refresh)?);
  }
  Ok(operations)
}

/// One skill's files. `init` leaves a former npm link to the skill alone; `update` replaces it
/// with a copy.
fn skill_directory_operations(
  root: &Path,
  name: &str,
  files: &[&(String, Vec<u8>)],
  refresh: bool,
) -> Result<Vec<FileOperation>> {
  let directory = format!(".agents/skills/{name}");
  let target = destination(root, &directory, DestinationKind::Link)?;
  let linked = target.exists
    && fs::symlink_metadata(&target.absolute_path).is_ok_and(|entry| entry.is_symlink());
  if !linked {
    return files
      .iter()
      .map(|(path, bytes)| file_operation(root, &format!(".agents/skills/{path}"), bytes, refresh))
      .collect();
  }
  let unlink = FileOperation {
    absolute_path: target.absolute_path,
    content: Content::Unlink,
    path: directory,
    state: if refresh {
      OperationState::Updated
    } else {
      OperationState::Preserved
    },
  };
  if !refresh {
    return Ok(vec![unlink]);
  }
  let copies = files.iter().map(|(path, bytes)| {
    let relative = format!(".agents/skills/{path}");
    FileOperation {
      absolute_path: normalize_path(&root.join(&relative)),
      content: Content::Bytes(bytes.clone()),
      path: relative,
      state: OperationState::Created,
    }
  });
  Ok(std::iter::once(unlink).chain(copies).collect())
}

fn file_operation(root: &Path, path: &str, bytes: &[u8], refresh: bool) -> Result<FileOperation> {
  let target = destination(root, path, DestinationKind::File)?;
  let current = target.exists && read_existing(&target, path)? == bytes;
  Ok(FileOperation {
    state: match (target.exists, current) {
      (false, _) => OperationState::Created,
      (true, false) if refresh => OperationState::Updated,
      _ => OperationState::Preserved,
    },
    absolute_path: target.absolute_path,
    content: Content::Bytes(bytes.to_vec()),
    path: path.to_owned(),
  })
}

/// Claude loads skills only from `.claude/skills`; an existing entry there stays as it is.
fn link_operations(root: &Path, bundle: &Bundle) -> Result<Vec<FileOperation>> {
  bundle
    .skill_names()
    .into_iter()
    .map(|name| {
      let path = format!(".claude/skills/{name}");
      let target = destination(root, &path, DestinationKind::Link)?;
      Ok(FileOperation {
        absolute_path: target.absolute_path,
        content: Content::SkillLink(name),
        path,
        state: OperationState::planned(target.exists),
      })
    })
    .collect()
}

#[cfg(unix)]
fn link_skill(link: &Path, name: &str) -> std::io::Result<()> {
  std::os::unix::fs::symlink(format!("../../.agents/skills/{name}"), link)
}

/// A directory junction needs no extra privileges, unlike a Windows symlink.
#[cfg(windows)]
fn link_skill(link: &Path, name: &str) -> std::io::Result<()> {
  let project = link
    .ancestors()
    .nth(3)
    .expect("the link lives inside the project");
  let status = std::process::Command::new("cmd")
    .args(["/C", "mklink", "/J"])
    .arg(link)
    .arg(project.join(".agents").join("skills").join(name))
    .stdout(std::process::Stdio::null())
    .status()?;
  if status.success() {
    Ok(())
  } else {
    Err(std::io::Error::other("mklink /J failed"))
  }
}

fn write_operations(operations: &[FileOperation]) -> Result<()> {
  for operation in operations {
    if operation.state != OperationState::Preserved {
      write_operation(operation).map_err(|write_error| {
        error(
          "INIT_WRITE_FAILED",
          format!("Unable to write {}: {write_error}", operation.path),
        )
      })?;
    }
  }
  Ok(())
}

fn write_operation(operation: &FileOperation) -> std::io::Result<()> {
  if let Some(parent) = operation.absolute_path.parent() {
    fs::create_dir_all(parent)?;
  }
  let bytes = match &operation.content {
    Content::SkillLink(name) => return link_skill(&operation.absolute_path, name),
    // A directory link is a file on Unix and a directory on Windows.
    Content::Unlink => {
      return fs::remove_file(&operation.absolute_path)
        .or_else(|_| fs::remove_dir(&operation.absolute_path));
    }
    Content::Bytes(bytes) => bytes,
  };
  if operation.state == OperationState::Updated {
    return fs::write(&operation.absolute_path, bytes);
  }
  // Never replace a file that appeared after planning.
  OpenOptions::new()
    .write(true)
    .create_new(true)
    .open(&operation.absolute_path)?
    .write_all(bytes)
}

fn run_quietly(program: &std::ffi::OsStr, args: &[&str]) -> std::io::Result<Output> {
  Command::new(program)
    .args(args)
    .stdin(Stdio::null())
    .stderr(Stdio::null())
    .output()
}

#[cfg(unix)]
fn run_host(host: &str, args: &[&str]) -> std::io::Result<Output> {
  run_quietly(host.as_ref(), args)
}

/// npm installs `.cmd` shims on Windows, which `Command` starts only by their full name.
#[cfg(windows)]
fn run_host(host: &str, args: &[&str]) -> std::io::Result<Output> {
  let program = std::env::var_os("PATH")
    .iter()
    .flat_map(std::env::split_paths)
    .flat_map(|directory| {
      ["exe", "cmd", "bat"].map(|extension| directory.join(host).with_extension(extension))
    })
    .find(|path| path.is_file())
    .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
  run_quietly(program.as_os_str(), args)
}

/// `None` once every step succeeded, otherwise `missing` without the host's CLI or `failed`.
fn run_steps(host: &str, steps: &[&[&str]]) -> Option<&'static str> {
  for args in steps {
    match run_host(host, args) {
      Ok(output) if output.status.success() => {}
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Some("missing"),
      _ => return Some("failed"),
    }
  }
  None
}

/// Installing again would replace Codex's copy and re-enable a plugin the owner disabled, so
/// `init` leaves an installed ponytail alone.
fn install_status(host: &str, steps: &[&[&str]]) -> &'static str {
  // shortcut: finds the plugin id in the JSON listing; parse it if a host changes that format.
  let listed =
    |output: &Output| String::from_utf8_lossy(&output.stdout).contains("\"ponytail@ponytail\"");
  match run_host(host, &["plugin", "list", "--json"]) {
    Ok(output) if output.status.success() && listed(&output) => "current",
    Ok(output) if output.status.success() => update_status(host, steps),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => "missing",
    _ => "failed",
  }
}

fn update_status(host: &str, steps: &[&[&str]]) -> &'static str {
  run_steps(host, steps).unwrap_or("current")
}

type HostStatus = fn(&str, &[&[&str]]) -> &'static str;

fn plugins(hosts: &[HostSteps], status: HostStatus) -> Value {
  hosts
    .iter()
    .map(|(host, steps)| json!({"host": host, "plugin": "ponytail", "status": status(host, steps)}))
    .collect()
}

/// What `init` or `update` did to the project's files and which extension releases it installed.
struct Outcome<'a> {
  command: &'a str,
  operations: &'a [FileOperation],
  pins: &'a [Pin],
}

fn report(outcome: &Outcome, hosts: &[HostSteps], status: HostStatus) -> Value {
  let paths = |state| {
    let mut paths: Vec<_> = outcome
      .operations
      .iter()
      .filter(|operation| operation.state == state)
      .map(|operation| operation.path.as_str())
      .collect();
    paths.sort_by(|left, right| {
      (left.to_ascii_lowercase(), left).cmp(&(right.to_ascii_lowercase(), right))
    });
    paths
  };
  let extensions: Vec<Value> = outcome
    .pins
    .iter()
    .map(|pin| json!({"repository": pin.repository, "version": pin.version}))
    .collect();
  json!({
    "command": outcome.command,
    "created": paths(OperationState::Created),
    "extensions": extensions,
    "modelCalls": 0,
    "plugins": plugins(hosts, status),
    "preserved": paths(OperationState::Preserved),
    "updated": paths(OperationState::Updated),
  })
}

/// Prepare the missing foundation files, skills, Claude skill links and ignore rules of the project
/// at `root` without changing anything that exists, and install the ponytail plugin in Claude and
/// Codex. `extension` adds an extension at its newest release to a project without `hivex.json`.
pub fn initialize(root: &str, extension: Option<&str>) -> Result<Value> {
  let root = project_root(root)?;
  // Retained state must not hide a symlink or a conflicting `.hivex` entry.
  destination(&root, ".hivex/.gitignore", DestinationKind::File)?;
  if let Some(pinned) = pinned_version(&root)?.filter(|pinned| pinned != VERSION) {
    return Err(version_mismatch(&pinned));
  }
  let config = project_config(&root)?;
  let mut pins = config
    .as_ref()
    .map(extension::pins)
    .transpose()?
    .unwrap_or_default();
  if let Some(repository) = extension {
    if config.is_some() {
      return Err(error(
        "INVALID_ARGUMENT",
        "hivex.json already exists; add the extension to its extensions list",
      ));
    }
    let version = extension::latest(repository)?;
    pins.push(Pin {
      repository: repository.to_owned(),
      version,
    });
  }
  let bundle = Bundle::with_extensions(&root, &pins)?;
  let mut operations = template_operations(&root, &bundle, &pins)?;
  operations.extend(managed_operations(&root, &bundle, false)?);
  write_operations(&operations)?;
  Ok(report(
    &Outcome {
      command: "init",
      operations: &operations,
      pins: &pins,
    },
    &PONYTAIL_INSTALL,
    install_status,
  ))
}

/// Bring the managed parts of the project at `root` up to this release and each extension up to
/// its newest release: the skills, Claude's skill links, the ignore rules, the pinned versions and
/// the ponytail plugin. Project files are neither created nor changed.
pub fn update(root: &str) -> Result<Value> {
  let root = project_root(root)?;
  destination(&root, ".hivex/.gitignore", DestinationKind::File)?;
  let pinned = project_config(&root)?
    .as_ref()
    .map(extension::pins)
    .transpose()?
    .unwrap_or_default();
  let pins = pinned
    .iter()
    .map(|pin| {
      let latest = extension::latest(&pin.repository)?;
      let newer = release(&latest) > release(&pin.version);
      Ok(Pin {
        repository: pin.repository.clone(),
        version: if newer { latest } else { pin.version.clone() },
      })
    })
    .collect::<Result<Vec<_>>>()?;
  let bundle = Bundle::with_extensions(&root, &pins)?;
  let mut operations = managed_operations(&root, &bundle, true)?;
  operations.extend(pin_operation(&root, &pins, pins != pinned)?);
  write_operations(&operations)?;
  Ok(report(
    &Outcome {
      command: "update",
      operations: &operations,
      pins: &pins,
    },
    &PONYTAIL_UPDATE,
    update_status,
  ))
}

/// The project's `hivex.json`, when it has one.
fn project_config(root: &Path) -> Result<Option<serde_json::Map<String, Value>>> {
  match fs::read(root.join(CONFIG)) {
    Ok(bytes) => config_object(&bytes).map(Some),
    Err(_) => Ok(None),
  }
}

fn managed_operations(root: &Path, bundle: &Bundle, refresh: bool) -> Result<Vec<FileOperation>> {
  let mut operations = skill_operations(root, bundle, refresh)?;
  // The links follow the skills so their targets exist when they are created.
  operations.extend(link_operations(root, bundle)?);
  operations.push(ignore_operation(root, &bundle.ignore_rules())?);
  Ok(operations)
}

fn validate_destination_component(
  metadata: &fs::Metadata,
  relative_path: &str,
  is_final: bool,
  kind: DestinationKind,
) -> Result<()> {
  let preserved_link = is_final && kind == DestinationKind::Link;
  if metadata.file_type().is_symlink() && !preserved_link {
    return Err(error(
      "INVALID_DESTINATION",
      format!("Initialization path must not use symlinks: {relative_path}"),
    ));
  }
  let valid_leaf = if preserved_link {
    metadata.is_dir() || metadata.file_type().is_symlink()
  } else {
    metadata.is_file()
  };
  if (!is_final && !metadata.is_dir()) || (is_final && !valid_leaf) {
    return Err(error(
      "INVALID_DESTINATION",
      format!("Initialization path has an incompatible type: {relative_path}"),
    ));
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::civil_date;

  #[test]
  fn civil_dates_follow_gregorian_leap_rules() {
    for (days, date) in [
      (0, "1970-01-01"),
      (11_016, "2000-02-29"),
      (19_782, "2024-02-29"),
      (20_730, "2026-10-04"),
      (47_541, "2100-03-01"),
    ] {
      assert_eq!(civil_date(days), date);
    }
  }
}
