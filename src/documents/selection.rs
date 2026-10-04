//! `hivex.json` source selection and the bounded walk that finds candidate Markdown.
use super::Warning;
use super::collation::compare_paths;
use super::markdown::is_markdown_path;
use crate::compatibility::trim_js_whitespace;
use crate::error::{HivexError, Result};
use globset::{Glob, GlobBuilder, GlobMatcher};
use serde_json::{Value, json};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

pub const PROTECTED_DIRECTORIES: [&str; 3] = [".git", ".hivex", "node_modules"];
pub const GENERATED_DIRECTORIES: [&str; 4] = ["vendor", "dist", "build", "target"];
const DEFAULT_INCLUDE: [&str; 3] = ["**/*.md", "**/*.markdown", "**/*.mdown"];
const MAX_PATTERNS: usize = 64;
const MAX_CONFIG_BYTES: usize = 64 * 1024;

/// One validated relative glob. An odd number of leading `!` negates it.
struct Pattern {
  source: String,
  matcher: GlobMatcher,
  negated: bool,
}

impl Pattern {
  fn new(source: String) -> std::result::Result<Self, globset::Error> {
    let negated = source.bytes().take_while(|byte| *byte == b'!').count() % 2 == 1;
    let matcher = glob(source.trim_start_matches('!'))?.compile_matcher();
    Ok(Self {
      source,
      matcher,
      negated,
    })
  }

  fn matches(&self, path: &str) -> bool {
    self.matcher.is_match(path) != self.negated
  }

  /// Whether this pattern names the special directory `name` at `path`, which
  /// opts that directory back into discovery.
  fn selects_directory(&self, name: &str, path: &str) -> bool {
    if self.negated {
      return false;
    }
    let segments: Vec<_> = self.source.trim_start_matches('!').split('/').collect();
    segments
      .iter()
      .enumerate()
      .filter(|(_, segment)| **segment == name)
      .any(|(index, _)| {
        glob(&segments[..=index].join("/")).is_ok_and(|glob| glob.compile_matcher().is_match(path))
      })
  }
}

fn glob(pattern: &str) -> std::result::Result<Glob, globset::Error> {
  GlobBuilder::new(pattern)
    .literal_separator(true)
    .backslash_escape(false)
    .build()
}

pub struct Patterns(Vec<Pattern>);

impl Patterns {
  fn matches(&self, path: &str) -> bool {
    self.0.iter().any(|pattern| pattern.matches(path))
  }

  pub fn sources(&self) -> impl Iterator<Item = &str> {
    self.0.iter().map(|pattern| pattern.source.as_str())
  }
}

pub struct Config {
  pub include: Patterns,
  pub exclude: Patterns,
  pub archive: Patterns,
  /// Positive `exclude` patterns ending in `/**`, which prune whole directories.
  excluded_trees: Patterns,
}

pub struct Candidate {
  pub absolute_path: PathBuf,
  pub path: String,
  pub historical: bool,
}

fn invalid(message: impl Into<String>) -> HivexError {
  HivexError::new("INVALID_CONFIG", message)
}

fn invalid_because(message: &str, reason: &impl ToString) -> HivexError {
  invalid(message).with_details(json!({"reason": reason.to_string()}))
}

fn pattern(value: &Value, field: &str, index: usize) -> Result<Pattern> {
  let Some(value) = value
    .as_str()
    .filter(|value| !trim_js_whitespace(value).is_empty())
  else {
    return Err(invalid(format!(
      "{field}[{index}] must be a non-empty relative glob"
    )));
  };
  let normalized = value.replace('\\', "/");
  if normalized.starts_with('/')
    || normalized.contains('\0')
    || normalized.split('/').any(|segment| segment == "..")
  {
    return Err(invalid(format!(
      "{field}[{index}] must stay inside the project root"
    )));
  }
  Pattern::new(normalized)
    .map_err(|error| invalid_because(&format!("{field}[{index}] is not a valid glob"), &error))
}

fn patterns(value: Option<&Value>, field: &str, fallback: &[&str]) -> Result<Patterns> {
  let Some(value) = value else {
    let defaults = fallback
      .iter()
      .map(|source| Pattern::new((*source).to_owned()));
    return Ok(Patterns(
      defaults
        .collect::<std::result::Result<_, _>>()
        .expect("default globs compile"),
    ));
  };
  let Some(values) = value
    .as_array()
    .filter(|values| values.len() <= MAX_PATTERNS)
  else {
    return Err(invalid(format!(
      "{field} must contain at most {MAX_PATTERNS} relative globs"
    )));
  };
  let values = values.iter().enumerate();
  Ok(Patterns(
    values
      .map(|(index, value)| pattern(value, field, index))
      .collect::<Result<_>>()?,
  ))
}

impl Config {
  /// Read optional `hivex.json`; a missing file selects all Markdown.
  pub fn load(root: &Path) -> Result<Self> {
    let Some(text) = read_config(&root.join("hivex.json"))? else {
      return Self::from_object(&serde_json::Map::new());
    };
    let value: Value = serde_json::from_str(text.strip_prefix('\u{feff}').unwrap_or(&text))
      .map_err(|error| invalid_because("hivex.json must contain valid JSON", &error))?;
    let object = value
      .as_object()
      .ok_or_else(|| invalid("hivex.json must contain an object"))?;
    if object.contains_key("collections") {
      return Err(HivexError::new(
        "LEGACY_CONFIGURATION",
        "hivex.json uses legacy collections; replace it with include and exclude globs",
      ));
    }
    if let Some(unknown) = object
      .keys()
      .find(|key| !matches!(key.as_str(), "exclude" | "archive" | "history" | "include"))
    {
      return Err(invalid(format!(
        "hivex.json has unsupported field: {unknown}"
      )));
    }
    if object.contains_key("archive") && object.contains_key("history") {
      return Err(invalid(
        "Use archive instead of history; do not specify both fields",
      ));
    }
    Self::from_object(object)
  }

  fn from_object(object: &serde_json::Map<String, Value>) -> Result<Self> {
    // `history` is the compatible former name of `archive`.
    let archive = if object.contains_key("history") {
      "history"
    } else {
      "archive"
    };
    let exclude = patterns(object.get("exclude"), "exclude", &[])?;
    let excluded_trees = exclude
      .sources()
      .filter(|source| source.ends_with("/**") && !source.starts_with('!'))
      .map(|source| Pattern::new(source.to_owned()).expect("validated glob"))
      .collect();
    Ok(Self {
      exclude,
      archive: patterns(object.get(archive), archive, &[])?,
      include: patterns(object.get("include"), "include", &DEFAULT_INCLUDE)?,
      excluded_trees: Patterns(excluded_trees),
    })
  }

  /// Protected directories are never opened. Generated and hidden directories
  /// are skipped unless an include or archive pattern names them explicitly.
  fn skips_entry(&self, name: &str, path: &str) -> bool {
    if PROTECTED_DIRECTORIES.contains(&name) {
      return true;
    }
    if !GENERATED_DIRECTORIES.contains(&name) && !name.starts_with('.') {
      return false;
    }
    !self
      .include
      .0
      .iter()
      .chain(&self.archive.0)
      .any(|pattern| pattern.selects_directory(name, path))
  }

  fn skips_tree(&self, path: &str) -> bool {
    self.excluded_trees.matches(&format!("{path}/"))
  }

  /// Classify a discovered file: `None` when unselected, otherwise whether it is historical.
  fn classify(&self, path: &str) -> Option<bool> {
    if !is_markdown_path(path) || self.exclude.matches(path) {
      return None;
    }
    if self.archive.matches(path) {
      return Some(true);
    }
    self.include.matches(path).then_some(false)
  }
}

fn read_config(path: &Path) -> Result<Option<String>> {
  let metadata = match fs::symlink_metadata(path) {
    Ok(metadata) => metadata,
    Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
    Err(error) => return Err(invalid_because("Unable to read hivex.json", &error)),
  };
  if metadata.file_type().is_symlink() {
    return Err(invalid("hivex.json must not be a symlink"));
  }
  if !metadata.is_file() {
    return Err(invalid("hivex.json must be a regular file"));
  }
  let bytes =
    fs::read(path).map_err(|error| invalid_because("Unable to read hivex.json", &error))?;
  super::decode_utf8(bytes, "hivex.json", MAX_CONFIG_BYTES).map(Some)
}

pub fn relative_path(root: &Path, path: &Path) -> String {
  path
    .strip_prefix(root)
    .map(|relative| relative.to_string_lossy().replace('\\', "/"))
    .unwrap_or_default()
}

/// Walk `root` in collation order, returning selected Markdown sorted by path:
/// current sources first, then historical ones. Unreadable directories and
/// symbolic links become warnings instead of silent omissions.
pub fn discover(root: &Path, config: &Config, warnings: &mut Vec<Warning>) -> Vec<Candidate> {
  let mut candidates = Vec::new();
  walk(root, (root, config), &mut candidates, warnings);
  candidates.sort_by(|left, right| {
    left
      .historical
      .cmp(&right.historical)
      .then_with(|| compare_paths(&left.path, &right.path))
  });
  candidates
}

fn walk(
  directory: &Path,
  scope: (&Path, &Config),
  candidates: &mut Vec<Candidate>,
  warnings: &mut Vec<Warning>,
) {
  let (root, config) = scope;
  let entries = fs::read_dir(directory).and_then(Iterator::collect::<std::io::Result<Vec<_>>>);
  let mut entries = match entries {
    Ok(entries) => entries,
    Err(error) => {
      let path = relative_path(root, directory);
      warnings.push(Warning {
        path: if path.is_empty() {
          ".".to_owned()
        } else {
          path
        },
        message: format!("Unable to inspect directory: {error}"),
      });
      return;
    }
  };
  entries.sort_by(|left, right| {
    compare_paths(
      &left.file_name().to_string_lossy(),
      &right.file_name().to_string_lossy(),
    )
  });
  for entry in entries {
    let name = entry.file_name().to_string_lossy().into_owned();
    let absolute_path = directory.join(&name);
    let path = relative_path(root, &absolute_path);
    if config.skips_entry(&name, &path) {
      continue;
    }
    let Ok(file_type) = entry.file_type() else {
      continue;
    };
    if file_type.is_symlink() {
      warnings.push(Warning {
        path,
        message: "Skipped symbolic link".to_owned(),
      });
      continue;
    }
    if file_type.is_dir() && !config.skips_tree(&path) {
      walk(&absolute_path, scope, candidates, warnings);
      continue;
    }
    if let Some(historical) = file_type
      .is_file()
      .then(|| config.classify(&path))
      .flatten()
    {
      candidates.push(Candidate {
        absolute_path,
        path,
        historical,
      });
    }
  }
}
