//! Extensions: Git repositories whose `vX.Y.Z` tags add skills and project files to the bundled
//! foundation.
use super::{Bundle, error, release};
use crate::error::Result;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const MANIFEST: &str = "hivex-extension.json";

/// An extension repository and the release a project pins.
#[derive(Clone, PartialEq, Eq)]
pub struct Pin {
  pub repository: String,
  pub version: String,
}

/// The `extensions` a configuration pins.
pub fn pins(config: &serde_json::Map<String, Value>) -> Result<Vec<Pin>> {
  let Some(entries) = config.get("extensions") else {
    return Ok(Vec::new());
  };
  let invalid = || {
    error(
      "INVALID_CONFIG",
      "extensions must list objects with a repository and a MAJOR.MINOR.PATCH version",
    )
  };
  entries
    .as_array()
    .ok_or_else(invalid)?
    .iter()
    .map(|entry| {
      let text = |key| entry.get(key).and_then(Value::as_str);
      let repository = text("repository").filter(|repository| !repository.is_empty());
      let version = text("version").filter(|version| release(version).is_some());
      let (Some(repository), Some(version)) = (repository, version) else {
        return Err(invalid());
      };
      Ok(Pin {
        repository: repository.to_owned(),
        version: version.to_owned(),
      })
    })
    .collect()
}

/// `OWNER/REPO` names a GitHub repository; anything else is a Git URL or path.
fn url(repository: &str) -> String {
  let github = repository.split('/').count() == 2
    && !repository.contains(':')
    && !repository.starts_with(['.', '/', '\\']);
  if github {
    format!("https://github.com/{repository}.git")
  } else {
    repository.to_owned()
  }
}

fn git(args: &[&str]) -> Result<String> {
  let output = Command::new("git")
    .args(args)
    // Fail instead of waiting for credentials nobody can type.
    .env("GIT_TERMINAL_PROMPT", "0")
    .stdin(Stdio::null())
    .output()
    .map_err(|run_error| {
      let message = if run_error.kind() == std::io::ErrorKind::NotFound {
        "Installing extensions needs git on PATH".to_owned()
      } else {
        format!("Unable to run git: {run_error}")
      };
      error("EXTENSION_UNAVAILABLE", message)
    })?;
  if !output.status.success() {
    let reason = String::from_utf8_lossy(&output.stderr);
    let reason = reason
      .lines()
      .find(|line| !line.trim().is_empty())
      .unwrap_or("");
    return Err(error(
      "EXTENSION_UNAVAILABLE",
      format!("git {} failed: {}", args[0], reason.trim()),
    ));
  }
  Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The newest `vX.Y.Z` tag of an extension.
pub fn latest(repository: &str) -> Result<String> {
  git(&["ls-remote", "--tags", "--refs", &url(repository)])?
    .lines()
    .filter_map(|line| {
      line
        .split_once("refs/tags/v")
        .map(|(_, version)| version.trim())
    })
    .filter(|version| release(version).is_some())
    .max_by_key(|version| release(version))
    .map(str::to_owned)
    .ok_or_else(|| {
      error(
        "EXTENSION_UNAVAILABLE",
        format!("{repository} has no vX.Y.Z release tags"),
      )
    })
}

/// The pinned release, fetched once into the project's ignored `.hivex/extensions/`.
pub fn fetch(root: &Path, pin: &Pin) -> Result<PathBuf> {
  let key: String = pin
    .repository
    .chars()
    .map(|character| {
      if character.is_ascii_alphanumeric() || "-.".contains(character) {
        character
      } else {
        '_'
      }
    })
    .collect();
  let cache = root.join(".hivex").join("extensions");
  let directory = cache.join(format!("{key}@{}", pin.version));
  if directory.join(MANIFEST).is_file() {
    return Ok(directory);
  }
  let staging = cache.join(format!("{key}@{}.partial", pin.version));
  // A previous attempt may have stopped halfway; its files are not trusted.
  let _ = fs::remove_dir_all(&staging);
  let write_error = |io_error: std::io::Error| {
    error(
      "INIT_WRITE_FAILED",
      format!("Unable to cache {key}: {io_error}"),
    )
  };
  fs::create_dir_all(&cache).map_err(write_error)?;
  let tag = format!("v{}", pin.version);
  git(&[
    "clone",
    "--quiet",
    "--depth",
    "1",
    "--branch",
    &tag,
    &url(&pin.repository),
    &staging.to_string_lossy(),
  ])?;
  fs::rename(&staging, &directory).map_err(write_error)?;
  Ok(directory)
}

/// What an extension's manifest declares.
struct Manifest {
  /// Bundled skills the extension removes.
  replaces: Vec<String>,
  /// Ignore rules the extension's projects need, managed like Hivex's own.
  ignore: Vec<String>,
}

fn manifest(directory: &Path) -> Result<Manifest> {
  let invalid = |message: &str| error("INVALID_EXTENSION", format!("{MANIFEST}: {message}"));
  let text = fs::read(directory.join(MANIFEST)).map_err(|_| invalid("missing"))?;
  let manifest: Value = serde_json::from_slice(&text).map_err(|_| invalid("invalid JSON"))?;
  let manifest = manifest
    .as_object()
    .ok_or_else(|| invalid("must be an object"))?;
  let list = |key: &str| -> Result<Vec<String>> {
    let Some(values) = manifest.get(key) else {
      return Ok(Vec::new());
    };
    values
      .as_array()
      .and_then(|values| {
        values
          .iter()
          .map(|value| value.as_str().map(str::to_owned))
          .collect()
      })
      .ok_or_else(|| invalid(&format!("{key} must list text")))
  };
  Ok(Manifest {
    replaces: list("replaces")?,
    ignore: list("ignore")?,
  })
}

/// Regular files under `directory`, by `/`-separated relative path. Symbolic links are skipped.
fn files(directory: &Path) -> Result<Vec<(String, Vec<u8>)>> {
  let mut found = Vec::new();
  let mut pending = vec![directory.to_path_buf()];
  let read_error = |io_error: std::io::Error| {
    error(
      "INVALID_EXTENSION",
      format!("Unable to read extension: {io_error}"),
    )
  };
  while let Some(current) = pending.pop() {
    let Ok(entries) = fs::read_dir(&current) else {
      continue;
    };
    for entry in entries {
      let entry = entry.map_err(read_error)?;
      let kind = entry.file_type().map_err(read_error)?;
      match (kind.is_dir(), kind.is_file()) {
        (true, _) => pending.push(entry.path()),
        (_, true) => {
          let path = entry.path();
          let relative = path
            .strip_prefix(directory)
            .expect("walk stays inside the directory");
          let relative = relative.to_string_lossy().replace('\\', "/");
          found.push((relative, fs::read(&path).map_err(read_error)?));
        }
        _ => {}
      }
    }
  }
  found.sort();
  Ok(found)
}

/// Apply an extension: its project files win over the bundled ones at the same path, and its
/// skills replace bundled skills of the same name and those its manifest names.
pub fn apply(bundle: &mut Bundle, directory: &Path) -> Result<()> {
  let Manifest {
    replaces: replaced,
    ignore,
  } = manifest(directory)?;
  let bundled = bundle.skill_names();
  if let Some(unknown) = replaced.iter().find(|name| !bundled.contains(name)) {
    return Err(error(
      "INVALID_EXTENSION",
      format!("{MANIFEST} replaces an unknown skill: {unknown}"),
    ));
  }
  let skills = files(&directory.join("skills"))?;
  let added: BTreeSet<String> = skills
    .iter()
    .map(|(path, _)| skill_name(path).to_owned())
    .collect();
  bundle.skills.retain(|(path, _)| {
    !added.contains(skill_name(path)) && !replaced.iter().any(|name| name == skill_name(path))
  });
  bundle.skills.extend(skills);
  bundle
    .ignore
    .extend(ignore.into_iter().filter(|rule| !rule.trim().is_empty()));
  for (path, bytes) in files(&directory.join("assets").join("project"))? {
    match bundle
      .templates
      .iter_mut()
      .find(|(existing, _)| *existing == path)
    {
      Some(template) => template.1 = bytes,
      None => bundle.templates.push((path, bytes)),
    }
  }
  Ok(())
}

pub fn skill_name(path: &str) -> &str {
  path.split('/').next().unwrap_or(path)
}
