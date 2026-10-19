//! Extensions: Git repositories whose `vX.Y.Z` tags add skills and project files to the bundled
//! foundation.
use super::{Bundle, BundleFile, error, release};
use crate::error::Result;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt::Write;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

const MANIFEST: &str = "hivex-extension.json";
/// Written into a cached release to bind it to the repository it came from.
const SOURCE: &str = ".hivex-source";

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

/// `OWNER/REPO` names a GitHub repository, a URL is used as given and any other text is a path
/// relative to the project.
fn location(root: &Path, repository: &str) -> String {
  let url = repository.contains("://") || repository.starts_with("git@");
  let github = !url
    && repository.split('/').count() == 2
    && !repository.contains(':')
    && !repository.starts_with(['.', '/', '\\']);
  if github {
    return format!("https://github.com/{repository}.git");
  }
  if url || Path::new(repository).is_absolute() {
    return repository.to_owned();
  }
  root.join(repository).to_string_lossy().into_owned()
}

fn git(args: &[&str]) -> Result<String> {
  let output = Command::new("git")
    .args([
      "-c",
      "core.autocrlf=false",
      "-c",
      "advice.detachedHead=false",
    ])
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
      format!("git failed: {}", reason.trim()),
    ));
  }
  Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The newest `vX.Y.Z` tag of an extension.
pub fn latest(root: &Path, repository: &str) -> Result<String> {
  git(&["ls-remote", "--tags", "--refs", &location(root, repository)])?
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

/// A readable name for the repository plus a hash of where it comes from, so two repositories
/// never share a cache entry.
fn cache_key(location: &str) -> String {
  let name: String = location
    .trim_end_matches(['/', '\\'])
    .trim_end_matches(".git")
    .rsplit(['/', '\\', ':'])
    .next()
    .unwrap_or("extension")
    .chars()
    .filter(|character| character.is_ascii_alphanumeric() || "-_".contains(*character))
    .take(32)
    .collect();
  let digest = Sha256::digest(location.as_bytes());
  let hash = digest[..8]
    .iter()
    .fold(String::with_capacity(16), |mut hash, byte| {
      let _ = write!(hash, "{byte:02x}");
      hash
    });
  format!("{name}-{hash}")
}

/// A real directory at `path`, created when missing; a symbolic link or file fails.
fn real_directory(path: &Path) -> Result<()> {
  match fs::symlink_metadata(path) {
    Ok(entry) if entry.is_dir() => Ok(()),
    Ok(_) => Err(error(
      "INVALID_DESTINATION",
      format!(
        "The extension cache must be a real directory: {}",
        path.display()
      ),
    )),
    Err(_) => fs::create_dir(path).map_err(|io_error| {
      error(
        "INIT_WRITE_FAILED",
        format!("Unable to create {}: {io_error}", path.display()),
      )
    }),
  }
}

/// Remove a cache entry this process owns, never following a link out of the cache.
fn discard(path: &Path) {
  if fs::symlink_metadata(path).is_ok_and(|entry| entry.is_dir()) {
    let _ = fs::remove_dir_all(path);
  }
}

/// The extension content at the pinned release. Releases are fetched once into the ignored
/// `.hivex/extensions/`, validated before they are kept, and older releases of the same
/// repository are removed.
pub fn load(root: &Path, pin: &Pin) -> Result<Content> {
  let source = location(root, &pin.repository);
  let key = cache_key(&source);
  let cache = root.join(".hivex").join("extensions");
  real_directory(&root.join(".hivex"))?;
  real_directory(&cache)?;
  let release_directory = cache.join(format!("{key}@{}", pin.version));
  let cached = fs::symlink_metadata(&release_directory).is_ok_and(|entry| entry.is_dir())
    && fs::read_to_string(release_directory.join(SOURCE)).is_ok_and(|text| text == source);
  if !cached {
    discard(&release_directory);
    fetch(&release_directory, &source, &pin.version)?;
  }
  let content = read(&release_directory)?;
  prune(&cache, &key, &release_directory);
  Ok(content)
}

/// Fetch exactly the tag into a staging directory and keep it only once it is a valid
/// extension.
fn fetch(release_directory: &Path, source: &str, version: &str) -> Result<()> {
  let mut name = release_directory
    .file_name()
    .unwrap_or_default()
    .to_os_string();
  name.push(".partial");
  let staging = release_directory.with_file_name(name);
  discard(&staging);
  let tag = format!("refs/tags/v{version}");
  let staged = staging.to_string_lossy().into_owned();
  let fetched = git(&["init", "--quiet", &staged])
    .and_then(|_| {
      git(&[
        "-C",
        &staged,
        "fetch",
        "--quiet",
        "--depth",
        "1",
        "--no-tags",
        source,
        &format!("+{tag}:{tag}"),
      ])
    })
    .and_then(|_| git(&["-C", &staged, "checkout", "--quiet", "--detach", &tag]))
    .and_then(|_| read(&staging).map(|_| ()))
    .and_then(|()| {
      fs::write(staging.join(SOURCE), source)
        .and_then(|()| fs::rename(&staging, release_directory))
        .map_err(|io_error| {
          error(
            "INIT_WRITE_FAILED",
            format!("Unable to cache {source}: {io_error}"),
          )
        })
    });
  if fetched.is_err() {
    discard(&staging);
  }
  fetched
}

/// Remove other cached releases of the same repository.
fn prune(cache: &Path, key: &str, keep: &Path) {
  let Ok(entries) = fs::read_dir(cache) else {
    return;
  };
  let prefix = format!("{key}@");
  for entry in entries.flatten() {
    let path = entry.path();
    if path != keep && entry.file_name().to_string_lossy().starts_with(&prefix) {
      discard(&path);
    }
  }
}

/// What a validated extension release contributes.
pub struct Content {
  /// Bundled skills the extension removes.
  replaces: Vec<String>,
  /// Ignore rules the extension's projects need, managed like Hivex's own.
  ignore: Vec<String>,
  skills: Vec<BundleFile>,
  templates: Vec<BundleFile>,
}

fn invalid(message: impl Into<String>) -> crate::error::HivexError {
  error("INVALID_EXTENSION", message)
}

fn read(directory: &Path) -> Result<Content> {
  let manifest = directory.join(MANIFEST);
  if !fs::symlink_metadata(&manifest).is_ok_and(|entry| entry.is_file()) {
    return Err(invalid(format!("{MANIFEST} is missing")));
  }
  let manifest: Value = fs::read(&manifest)
    .ok()
    .and_then(|bytes| serde_json::from_slice(&bytes).ok())
    .ok_or_else(|| invalid(format!("{MANIFEST} must contain valid JSON")))?;
  let manifest = manifest
    .as_object()
    .ok_or_else(|| invalid(format!("{MANIFEST} must contain an object")))?;
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
      .ok_or_else(|| invalid(format!("{MANIFEST}: {key} must list text")))
  };
  Ok(Content {
    replaces: list("replaces")?,
    ignore: list("ignore")?,
    skills: files(directory, &["skills"])?,
    templates: files(directory, &["assets", "project"])?,
  })
}

/// Regular files under `directory/relative`, by `/`-separated relative path. The root and every
/// directory must be real directories; symbolic links inside are skipped and names that could
/// leave the tree are rejected.
fn files(directory: &Path, relative: &[&str]) -> Result<Vec<BundleFile>> {
  let mut root = directory.to_path_buf();
  for component in relative {
    root.push(component);
    match fs::symlink_metadata(&root) {
      Err(_) => return Ok(Vec::new()),
      Ok(entry) if entry.is_dir() => {}
      Ok(_) => {
        return Err(invalid(format!(
          "{} must be a directory",
          relative.join("/")
        )));
      }
    }
  }
  let mut found = Vec::new();
  let mut pending = vec![(root, String::new())];
  while let Some((current, prefix)) = pending.pop() {
    found.extend(directory_files(&current, &prefix, &mut pending)?);
  }
  found.sort_by(|left, right| left.path.cmp(&right.path));
  Ok(found)
}

/// The regular files directly in `current`, queuing its subdirectories.
fn directory_files(
  current: &Path,
  prefix: &str,
  pending: &mut Vec<(std::path::PathBuf, String)>,
) -> Result<Vec<BundleFile>> {
  let read_error =
    |io_error: std::io::Error| invalid(format!("Unable to read {prefix}: {io_error}"));
  let mut found = Vec::new();
  for entry in fs::read_dir(current).map_err(read_error)? {
    let entry = entry.map_err(read_error)?;
    let name = entry.file_name();
    let name = name
      .to_str()
      .filter(|name| !name.contains(['\\', '/', ':']) && !matches!(*name, "." | ".."))
      .ok_or_else(|| invalid(format!("Unsupported file name in {prefix}")))?;
    let path = format!("{prefix}{name}");
    let kind = entry.file_type().map_err(read_error)?;
    if kind.is_dir() {
      pending.push((entry.path(), format!("{path}/")));
    }
    if kind.is_file() {
      found.push(BundleFile {
        bytes: fs::read(entry.path()).map_err(read_error)?,
        executable: executable(&entry.path()),
        path,
      });
    }
  }
  Ok(found)
}

#[cfg(unix)]
fn executable(path: &Path) -> bool {
  use std::os::unix::fs::PermissionsExt;
  fs::metadata(path).is_ok_and(|entry| entry.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn executable(_: &Path) -> bool {
  false
}

/// Apply an extension: its project files win over the bundled ones at the same path, and its
/// skills replace bundled skills of the same name and those its manifest names.
pub fn apply(bundle: &mut Bundle, content: Content) -> Result<()> {
  let bundled = bundle.skill_names();
  if let Some(unknown) = content.replaces.iter().find(|name| !bundled.contains(name)) {
    return Err(invalid(format!(
      "{MANIFEST} replaces an unknown skill: {unknown}"
    )));
  }
  let added: BTreeSet<String> = content
    .skills
    .iter()
    .map(|file| skill_name(&file.path).to_owned())
    .collect();
  bundle.skills.retain(|file| {
    let name = skill_name(&file.path);
    !added.contains(name) && !content.replaces.iter().any(|replaced| replaced == name)
  });
  bundle.skills.extend(content.skills);
  bundle.ignore.extend(
    content
      .ignore
      .into_iter()
      .filter(|rule| !rule.trim().is_empty()),
  );
  for template in content.templates {
    match bundle
      .templates
      .iter_mut()
      .find(|existing| existing.path == template.path)
    {
      Some(existing) => *existing = template,
      None => bundle.templates.push(template),
    }
  }
  Ok(())
}

pub fn skill_name(path: &str) -> &str {
  path.split('/').next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
  use super::cache_key;

  #[test]
  fn repositories_with_the_same_name_get_distinct_cache_entries() {
    let first = cache_key("/opt/one/extension");
    let second = cache_key("/opt/two/extension");
    assert_ne!(first, second);
    assert!(first.starts_with("extension-"));
  }
}
