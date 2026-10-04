use crate::compatibility::{normalize_path, project_directory};
use crate::error::{HivexError, Result};
use serde_json::{Value, json};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, symlink};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

// Managed rules stay last so earlier project rules cannot re-include their paths.
const IGNORE_RULES: [&str; 2] = ["/.hivex/", "/.reviews/"];
const SKILLS: [&str; 6] = [
  "hivex",
  "hivex-design",
  "hivex-document",
  "hivex-implement",
  "hivex-review",
  "hivex-git",
];
/// Bundled `skills/hivex/assets/project` files, by their project-relative path.
macro_rules! templates {
  ($($path:literal),* $(,)?) => {
    &[$(($path, include_bytes!(concat!("../skills/hivex/assets/project/", $path)))),*]
  };
}

static TEMPLATE_FILES: &[(&str, &[u8])] = templates![
  "AGENTS.md",
  "docs/adr/README.md",
  "docs/CONTEXT.md",
  "docs/guidelines/engineering.md",
  "docs/guidelines/triage-labels.md",
  "docs/PRD.md",
  "docs/procedures/issue-tracker.md",
  "docs/procedures/independent-review.md",
  "docs/procedures/independent-review.schema.json",
  "docs/procedures/self-hosted-runners.md",
  "docs/README.md",
  "hivex.json",
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
  content: FileContent,
  path: String,
  state: OperationState,
}

enum FileContent {
  Bytes(Vec<u8>),
  Link(PathBuf),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DestinationKind {
  File,
  Skill,
}

struct Destination {
  absolute_path: PathBuf,
  exists: bool,
}

fn error(code: &str, message: impl Into<String>) -> HivexError {
  HivexError::new(code, message)
}

fn project_root(requested: &str) -> Result<PathBuf> {
  fs::canonicalize(project_directory(requested)?)
    .map_err(|read_error| error("INVALID_ROOT", read_error.to_string()))
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

fn missing_ignore_rules(text: &str) -> Vec<&'static str> {
  let lines: Vec<_> = split_crlf_lines(text).collect();
  // The last matching rule wins, so a later negation could re-include local state.
  let effective = lines
    .iter()
    .rposition(|line| line.starts_with('!'))
    .map_or(&lines[..], |negation| &lines[negation + 1..]);
  IGNORE_RULES
    .into_iter()
    .filter(|rule| !effective.contains(rule))
    .collect()
}

fn ignore_update(existing: Option<&[u8]>) -> Option<Vec<u8>> {
  let text = existing.map(String::from_utf8_lossy).unwrap_or_default();
  let missing = missing_ignore_rules(&text);
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

fn template_operations(root: &Path) -> Result<Vec<FileOperation>> {
  let created_at = creation_date()?;
  TEMPLATE_FILES
    .iter()
    .map(|(path, bytes)| {
      let target = destination(root, path, DestinationKind::File)?;
      Ok(FileOperation {
        absolute_path: target.absolute_path,
        content: FileContent::Bytes(if !target.exists && is_dated(path) {
          dated_template(bytes, &created_at)
        } else {
          bytes.to_vec()
        }),
        path: (*path).to_owned(),
        state: OperationState::planned(target.exists),
      })
    })
    .collect()
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

fn dated_template(bytes: &[u8], created_at: &str) -> Vec<u8> {
  let text = std::str::from_utf8(bytes).expect("bundled Markdown is UTF-8");
  let (header, body) = text
    .strip_prefix("---\n")
    .and_then(|text| text.split_once("\n---\n"))
    .expect("bundled documentation has frontmatter");
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

fn ignore_operation(root: &Path) -> Result<FileOperation> {
  let relative_path = ".gitignore";
  let target = destination(root, relative_path, DestinationKind::File)?;
  let existing = if target.exists {
    Some(fs::read(&target.absolute_path).map_err(|read_error| {
      error(
        "INIT_READ_FAILED",
        format!("Unable to read {relative_path}: {read_error}"),
      )
    })?)
  } else {
    None
  };
  let (bytes, state) = match ignore_update(existing.as_deref()) {
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
    content: FileContent::Bytes(bytes),
    path: relative_path.to_owned(),
    state,
  })
}

fn bundled_skills() -> Result<PathBuf> {
  let executable = std::env::current_exe()
    .and_then(fs::canonicalize)
    .map_err(|read_error| error("INIT_SKILLS_UNAVAILABLE", read_error.to_string()))?;
  for directory in executable.ancestors().skip(1) {
    let manifest = fs::read(directory.join("package.json"))
      .ok()
      .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    if manifest.as_ref().is_some_and(|value| {
      value["name"] == "@h1v35/hivex" && value["version"] == env!("CARGO_PKG_VERSION")
    }) {
      let skills = directory.join("skills");
      if SKILLS
        .iter()
        .all(|name| skills.join(name).join("SKILL.md").is_file())
      {
        return fs::canonicalize(skills)
          .map_err(|read_error| error("INIT_SKILLS_UNAVAILABLE", read_error.to_string()));
      }
    }
  }
  Err(error(
    "INIT_SKILLS_UNAVAILABLE",
    "Install the complete @h1v35/hivex package; its bundled skills must remain alongside the executable",
  ))
}

fn relative_link(from: &Path, target: &Path) -> PathBuf {
  let from: Vec<_> = from.components().collect();
  let target: Vec<_> = target.components().collect();
  let common = from.iter().zip(&target).take_while(|(a, b)| a == b).count();
  let mut relative = PathBuf::new();
  for _ in common..from.len() {
    relative.push("..");
  }
  for component in &target[common..] {
    relative.push(component.as_os_str());
  }
  relative
}

fn skill_operations(root: &Path) -> Result<Vec<FileOperation>> {
  let bundled = bundled_skills()?;
  // Link through the stable dependency entry, including hoisted/symlinked installs.
  let bundled = root
    .ancestors()
    .map(|directory| directory.join("node_modules/@h1v35/hivex/skills"))
    .find(|candidate| fs::canonicalize(candidate).is_ok_and(|path| path == bundled))
    .unwrap_or(bundled);
  let mut operations = Vec::new();
  for family in [".agents", ".claude"] {
    for name in SKILLS {
      let path = format!("{family}/skills/{name}");
      let target = destination(root, &path, DestinationKind::Skill)?;
      let source = if family == ".agents" {
        bundled.join(name)
      } else {
        root.join(".agents/skills").join(name)
      };
      let link = relative_link(
        target
          .absolute_path
          .parent()
          .expect("skill link has a parent"),
        &source,
      );
      operations.push(FileOperation {
        absolute_path: target.absolute_path,
        content: FileContent::Link(link),
        path,
        state: OperationState::planned(target.exists),
      });
    }
  }
  Ok(operations)
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
    FileContent::Link(target) => return symlink(target, &operation.absolute_path),
    FileContent::Bytes(bytes) => bytes,
  };
  if operation.state == OperationState::Updated {
    return fs::write(&operation.absolute_path, bytes);
  }
  // Never replace a file that appeared after planning.
  OpenOptions::new()
    .write(true)
    .create_new(true)
    .mode(0o644)
    .open(&operation.absolute_path)?
    .write_all(bytes)
}

fn report(operations: &[FileOperation]) -> Value {
  let paths = |state| {
    let mut paths: Vec<_> = operations
      .iter()
      .filter(|operation| operation.state == state)
      .map(|operation| operation.path.as_str())
      .collect();
    paths.sort_by(|left, right| {
      (left.to_ascii_lowercase(), left).cmp(&(right.to_ascii_lowercase(), right))
    });
    paths
  };
  json!({
    "command": "init",
    "created": paths(OperationState::Created),
    "modelCalls": 0,
    "preserved": paths(OperationState::Preserved),
    "updated": paths(OperationState::Updated),
  })
}

/// Prepare the missing foundation files, skill links and ignore rules of the
/// project at `root`, preserving everything that already exists.
pub fn initialize(root: &str) -> Result<Value> {
  let root = project_root(root)?;
  // Retained state must not hide a symlink or a conflicting `.hivex` entry.
  destination(&root, ".hivex/.gitignore", DestinationKind::File)?;
  let mut operations = template_operations(&root)?;
  operations.extend(skill_operations(&root)?);
  operations.push(ignore_operation(&root)?);
  write_operations(&operations)?;
  Ok(report(&operations))
}

fn validate_destination_component(
  metadata: &fs::Metadata,
  relative_path: &str,
  is_final: bool,
  kind: DestinationKind,
) -> Result<()> {
  let preserved_skill = is_final && kind == DestinationKind::Skill;
  if metadata.file_type().is_symlink() && !preserved_skill {
    return Err(error(
      "INVALID_DESTINATION",
      format!("Initialization path must not use symlinks: {relative_path}"),
    ));
  }
  let valid_leaf = if preserved_skill {
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
