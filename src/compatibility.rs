use crate::error::{HiveXError, Result};
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
  let invalid = |message: &str| HiveXError::new("INVALID_ROOT", message);
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

/// Proleptic Gregorian date of a day count since 1970-01-01 (Hinnant's `civil_from_days`).
pub fn civil_date(days: u64) -> String {
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
