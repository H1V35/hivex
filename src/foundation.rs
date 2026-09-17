use crate::compatibility::trim_js_whitespace;
use crate::error::{HivexError, Result};
use serde_json::{Map, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

const IGNORE_RULES: [&str; 3] = ["!/.hivex/", "/.hivex/*", "!/.hivex/graph.json"];
const IGNORE_BLOCK: &str = "!/.hivex/\n/.hivex/*\n!/.hivex/graph.json\n";
const SKILLS: [&str; 6] = [
  "hivex",
  "hivex-design",
  "hivex-document",
  "hivex-implement",
  "hivex-review",
  "hivex-git",
];
static TEMPLATE_FILES: &[(&str, &[u8])] = &[
  (
    "AGENTS.md",
    include_bytes!("../skills/hivex/assets/project/AGENTS.md"),
  ),
  (
    "CLAUDE.md",
    include_bytes!("../skills/hivex/assets/project/CLAUDE.md"),
  ),
  (
    "docs/adr/README.md",
    include_bytes!("../skills/hivex/assets/project/docs/adr/README.md"),
  ),
  (
    "docs/CONTEXT.md",
    include_bytes!("../skills/hivex/assets/project/docs/CONTEXT.md"),
  ),
  (
    "docs/guidelines/engineering.md",
    include_bytes!("../skills/hivex/assets/project/docs/guidelines/engineering.md"),
  ),
  (
    "docs/guidelines/triage-labels.md",
    include_bytes!("../skills/hivex/assets/project/docs/guidelines/triage-labels.md"),
  ),
  (
    "docs/PRD.md",
    include_bytes!("../skills/hivex/assets/project/docs/PRD.md"),
  ),
  (
    "docs/procedures/issue-tracker.md",
    include_bytes!("../skills/hivex/assets/project/docs/procedures/issue-tracker.md"),
  ),
  (
    "docs/README.md",
    include_bytes!("../skills/hivex/assets/project/docs/README.md"),
  ),
  (
    "hivex.json",
    include_bytes!("../skills/hivex/assets/project/hivex.json"),
  ),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum OperationState {
  Created,
  Preserved,
  Updated,
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

fn normalize_path(path: &Path) -> PathBuf {
  let mut normalized = PathBuf::new();
  for component in path.components() {
    match component {
      Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
      Component::RootDir => normalized.push(std::path::MAIN_SEPARATOR_STR),
      Component::CurDir => {}
      Component::ParentDir => {
        if !normalized.pop() && !normalized.is_absolute() {
          normalized.push(component.as_os_str());
        }
      }
      Component::Normal(part) => normalized.push(part),
    }
  }
  normalized
}

fn resolve_path(path: &Path) -> std::io::Result<PathBuf> {
  if path.is_absolute() {
    Ok(normalize_path(path))
  } else {
    Ok(normalize_path(&std::env::current_dir()?.join(path)))
  }
}

fn project_root(requested: &str) -> Result<PathBuf> {
  if trim_js_whitespace(requested).is_empty() {
    return Err(error(
      "INVALID_ROOT",
      "Project root must be a non-empty path",
    ));
  }

  let root = resolve_path(Path::new(requested)).map_err(|read_error| {
    error(
      "INVALID_ROOT",
      format!("Project root is not readable: {read_error}"),
    )
  })?;
  let metadata = fs::symlink_metadata(&root).map_err(|read_error| {
    error(
      "INVALID_ROOT",
      format!("Project root is not readable: {read_error}"),
    )
  })?;
  if metadata.file_type().is_symlink() {
    return Err(error("INVALID_ROOT", "Project root must not be a symlink"));
  }
  if !metadata.is_dir() {
    return Err(error("INVALID_ROOT", "Project root must be a directory"));
  }
  fs::canonicalize(root).map_err(|read_error| error("INVALID_ROOT", read_error.to_string()))
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

fn validate_nested_ignore(root: &Path) -> Result<()> {
  let relative_path = ".hivex/.gitignore";
  let target = destination(root, relative_path, DestinationKind::File)?;
  if !target.exists {
    return Ok(());
  }
  let bytes = fs::read(&target.absolute_path).map_err(|read_error| {
    error(
      "INIT_READ_FAILED",
      format!("Unable to read {relative_path}: {read_error}"),
    )
  })?;
  let text = String::from_utf8_lossy(&bytes);
  if split_crlf_lines(&text)
    .any(|line| !trim_js_whitespace(line).is_empty() && !line.starts_with('#'))
  {
    return Err(error(
      "INIT_IGNORE_CONFLICT",
      ".hivex/.gitignore contains rules that can override snapshot visibility or local state privacy.",
    ));
  }
  Ok(())
}

fn has_final_ignore_rules(text: &str) -> bool {
  let mut lines: Vec<_> = split_crlf_lines(text).collect();
  while lines.last() == Some(&"") {
    lines.pop();
  }
  let Some(start) = lines.len().checked_sub(IGNORE_RULES.len()) else {
    return false;
  };
  lines[start..]
    .iter()
    .zip(IGNORE_RULES)
    .all(|(line, rule)| *line == rule)
}

fn ignore_update(existing: Option<&[u8]>) -> Option<Vec<u8>> {
  let Some(existing) = existing else {
    return Some(IGNORE_BLOCK.as_bytes().to_vec());
  };
  let text = String::from_utf8_lossy(existing);
  if has_final_ignore_rules(&text) {
    return None;
  }
  let separator = if existing.is_empty() || text.ends_with('\n') {
    ""
  } else {
    "\n"
  };
  let mut bytes = Vec::with_capacity(existing.len() + separator.len() + IGNORE_BLOCK.len());
  bytes.extend_from_slice(existing);
  bytes.extend_from_slice(separator.as_bytes());
  bytes.extend_from_slice(IGNORE_BLOCK.as_bytes());
  Some(bytes)
}

fn template_operations(root: &Path) -> Result<Vec<FileOperation>> {
  TEMPLATE_FILES
    .iter()
    .map(|(path, bytes)| {
      let target = destination(root, path, DestinationKind::File)?;
      Ok(FileOperation {
        absolute_path: target.absolute_path,
        content: FileContent::Bytes(bytes.to_vec()),
        path: (*path).to_owned(),
        state: if target.exists {
          OperationState::Preserved
        } else {
          OperationState::Created
        },
      })
    })
    .collect()
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
      let link = relative_link(target.absolute_path.parent().unwrap(), &source);
      operations.push(FileOperation {
        absolute_path: target.absolute_path,
        content: FileContent::Link(link),
        path,
        state: if target.exists {
          OperationState::Preserved
        } else {
          OperationState::Created
        },
      });
    }
  }
  Ok(operations)
}

fn write_operations(operations: &[FileOperation]) -> Result<()> {
  for operation in operations {
    if matches!(operation.state, OperationState::Preserved) {
      continue;
    }
    if let Some(parent) = operation.absolute_path.parent() {
      fs::create_dir_all(parent).map_err(|write_error| {
        error(
          "READ_FAILED",
          format!("Unable to create initialization directory: {write_error}"),
        )
      })?;
    }
    let bytes = match &operation.content {
      FileContent::Link(target) => {
        std::os::unix::fs::symlink(target, &operation.absolute_path).map_err(|write_error| {
          error(
            "READ_FAILED",
            format!("Unable to link {}: {write_error}", operation.path),
          )
        })?;
        continue;
      }
      FileContent::Bytes(bytes) => bytes,
    };
    if matches!(operation.state, OperationState::Created) {
      let mut options = OpenOptions::new();
      options.write(true).create_new(true);
      #[cfg(unix)]
      {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o644);
      }
      let mut file = options
        .open(&operation.absolute_path)
        .map_err(|write_error| {
          error(
            "READ_FAILED",
            format!("Unable to create {}: {write_error}", operation.path),
          )
        })?;
      file.write_all(bytes).map_err(|write_error| {
        error(
          "READ_FAILED",
          format!("Unable to write {}: {write_error}", operation.path),
        )
      })?;
    } else {
      fs::write(&operation.absolute_path, bytes).map_err(|write_error| {
        error(
          "READ_FAILED",
          format!("Unable to write {}: {write_error}", operation.path),
        )
      })?;
    }
  }
  Ok(())
}

fn sort_paths(paths: &mut [String]) {
  paths.sort_by(|left, right| {
    left
      .to_ascii_lowercase()
      .cmp(&right.to_ascii_lowercase())
      .then_with(|| left.cmp(right))
  });
}

fn report(operations: &[FileOperation]) -> Value {
  let paths = |state| {
    let mut paths: Vec<_> = operations
      .iter()
      .filter(|operation| operation.state == state)
      .map(|operation| operation.path.clone())
      .collect();
    sort_paths(&mut paths);
    Value::Array(paths.into_iter().map(Value::String).collect())
  };
  let mut report = Map::new();
  report.insert("command".to_owned(), Value::String("init".to_owned()));
  report.insert("created".to_owned(), paths(OperationState::Created));
  report.insert("modelCalls".to_owned(), Value::from(0));
  report.insert("preserved".to_owned(), paths(OperationState::Preserved));
  report.insert("updated".to_owned(), paths(OperationState::Updated));
  Value::Object(report)
}

pub fn initialize(requested: Option<&str>) -> Result<Value> {
  let root = if let Some(requested) = requested {
    project_root(requested)?
  } else {
    let current = std::env::current_dir().map_err(|read_error| {
      error(
        "INVALID_ROOT",
        format!("Project root is not readable: {read_error}"),
      )
    })?;
    project_root(&current.to_string_lossy())?
  };
  validate_nested_ignore(&root)?;
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
