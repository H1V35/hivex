use crate::error::{HivexError, Result};
use serde_json::json;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub fn trim_js_whitespace(value: &str) -> &str {
  value.trim_matches(|character: char| {
    character == '\u{feff}' || (character.is_whitespace() && character != '\u{85}')
  })
}

pub fn normalize_path(path: &Path) -> PathBuf {
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

/// The absolute normalized path of an existing project directory that is not a symlink.
pub fn project_directory(root: &str) -> Result<PathBuf> {
  let invalid = |message: &str| HivexError::new("INVALID_ROOT", message);
  if trim_js_whitespace(root).is_empty() {
    return Err(invalid("Project root must be a non-empty path"));
  }
  let requested = Path::new(root);
  let absolute = if requested.is_absolute() {
    normalize_path(requested)
  } else {
    let current = std::env::current_dir()
      .map_err(|error| invalid(&format!("Project root is not readable: {error}")))?;
    normalize_path(&current.join(requested))
  };
  let metadata = fs::symlink_metadata(&absolute).map_err(|error| {
    invalid("Project root is not readable")
      .with_details(json!({"reason": error.to_string(), "root": absolute.to_string_lossy()}))
  })?;
  if metadata.file_type().is_symlink() {
    return Err(invalid("Project root must not be a symlink"));
  }
  if !metadata.is_dir() {
    return Err(invalid("Project root must be a directory"));
  }
  Ok(absolute)
}
