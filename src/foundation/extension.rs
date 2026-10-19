//! Extensions: Git repositories whose `vX.Y.Z` tags add skills and project files to the bundled
//! foundation.
use super::{Bundle, BundleFile, error, release};
use crate::error::Result;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const MANIFEST: &str = "hivex-extension.json";
/// Beside each cached checkout, binding it to the repository it came from.
const SOURCE: &str = "source";
const CHECKOUT: &str = "checkout";

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
  // `user@host:path` remotes; a one-letter prefix is a Windows drive, not a host.
  let scp = !repository.contains("://")
    && repository
      .split_once(':')
      .is_some_and(|(host, _)| host.len() > 1 && !host.contains(['/', '\\']));
  let url = repository.contains("://") || scp;
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

/// Remove a cache entry this process owns. A link or file in its place is refused rather than
/// followed or left for a later step to write through.
fn clear(path: &Path) -> Result<()> {
  match fs::symlink_metadata(path) {
    Err(io_error) if io_error.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Ok(entry) if entry.is_dir() => fs::remove_dir_all(path).map_err(|io_error| {
      error(
        "INIT_WRITE_FAILED",
        format!("Unable to clear {}: {io_error}", path.display()),
      )
    }),
    _ => Err(error(
      "INVALID_DESTINATION",
      format!(
        "The extension cache holds an unexpected entry: {}",
        path.display()
      ),
    )),
  }
}

fn real_directory_exists(path: &Path) -> bool {
  fs::symlink_metadata(path).is_ok_and(|entry| entry.is_dir())
}

/// A cached release kept while its pin is in use.
pub struct Kept {
  key: String,
  directory: PathBuf,
}

/// A validated extension release and the cache entry that holds it.
pub struct Release {
  pub content: Content,
  pub kept: Kept,
}

/// The extension release a project pins, validated against the `bundled` skill names it may
/// replace. Releases are fetched once into the ignored `.hivex/extensions/` and kept only after
/// validation.
pub fn load(root: &Path, pin: &Pin, bundled: &[String]) -> Result<Release> {
  let source = location(root, &pin.repository);
  let key = cache_key(&source);
  let cache = root.join(".hivex").join("extensions");
  real_directory(&root.join(".hivex"))?;
  real_directory(&cache)?;
  let entry = cache.join(format!("{key}@{}", pin.version));
  let marker = entry.join(SOURCE);
  let cached = real_directory_exists(&entry)
    && real_directory_exists(&entry.join(CHECKOUT))
    && fs::symlink_metadata(&marker).is_ok_and(|metadata| metadata.is_file())
    && fs::read_to_string(&marker).is_ok_and(|text| text == source);
  if !cached {
    clear(&entry)?;
    fetch(&entry, &source, &pin.version, bundled)?;
  }
  let content = read(&entry.join(CHECKOUT))?;
  validate(&content, bundled)?;
  Ok(Release {
    content,
    kept: Kept {
      key,
      directory: entry,
    },
  })
}

/// Fetch exactly the tag into a staging entry and keep it only once it is a valid extension.
fn fetch(entry: &Path, source: &str, version: &str, bundled: &[String]) -> Result<()> {
  let mut name = entry.file_name().unwrap_or_default().to_os_string();
  name.push(".partial");
  let staging = entry.with_file_name(name);
  clear(&staging)?;
  let write_error = |io_error: std::io::Error| {
    error(
      "INIT_WRITE_FAILED",
      format!("Unable to cache {source}: {io_error}"),
    )
  };
  fs::create_dir(&staging).map_err(write_error)?;
  let checkout = staging.join(CHECKOUT);
  let checkout_path = checkout.to_string_lossy().into_owned();
  let tag = format!("refs/tags/v{version}");
  let fetched = git(&["init", "--quiet", &checkout_path])
    .and_then(|_| {
      git(&[
        "-C",
        &checkout_path,
        "fetch",
        "--quiet",
        "--depth",
        "1",
        "--no-tags",
        source,
        &format!("+{tag}:{tag}"),
      ])
    })
    .and_then(|_| {
      git(&[
        "-C",
        &checkout_path,
        "checkout",
        "--quiet",
        "--detach",
        &tag,
      ])
    })
    .and_then(|_| validate(&read(&checkout)?, bundled))
    .and_then(|()| {
      // The staging entry is new and owned here; the checkout cannot supply the marker.
      fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(staging.join(SOURCE))
        .and_then(|mut file| std::io::Write::write_all(&mut file, source.as_bytes()))
        .and_then(|()| fs::rename(&staging, entry))
        .map_err(write_error)
    });
  if fetched.is_err() {
    let _ = clear(&staging);
  }
  fetched
}

/// After a successful installation, remove other cached releases of the kept repositories.
pub fn prune(root: &Path, kept: &[Kept]) {
  let cache = root.join(".hivex").join("extensions");
  let Ok(entries) = fs::read_dir(&cache) else {
    return;
  };
  for entry in entries.flatten() {
    let path = entry.path();
    let name = entry.file_name().to_string_lossy().into_owned();
    let stale = kept
      .iter()
      .any(|kept| name.starts_with(&format!("{}@", kept.key)) && path != kept.directory);
    if stale && real_directory_exists(&path) {
      let _ = fs::remove_dir_all(&path);
    }
  }
}

/// What the manifest declares must make sense for this project.
fn validate(content: &Content, bundled: &[String]) -> Result<()> {
  if let Some(unknown) = content.replaces.iter().find(|name| !bundled.contains(name)) {
    return Err(invalid(format!(
      "{MANIFEST} replaces an unknown skill: {unknown}"
    )));
  }
  if let Some(rule) = content
    .ignore
    .iter()
    .find(|rule| rule.trim().is_empty() || rule.starts_with('!') || rule.contains(['\r', '\n']))
  {
    return Err(invalid(format!(
      "{MANIFEST}: ignore rules must be single nonempty lines that do not re-include paths: {rule:?}"
    )));
  }
  Ok(())
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
      Err(io_error) if io_error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
      Err(io_error) => {
        return Err(invalid(format!(
          "Unable to read {}: {io_error}",
          relative.join("/")
        )));
      }
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
pub fn apply(bundle: &mut Bundle, content: Content) {
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
  bundle.ignore.extend(content.ignore);
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
}

pub fn skill_name(path: &str) -> &str {
  path.split('/').next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
  use super::{cache_key, location};
  use std::path::Path;

  #[test]
  fn git_remotes_and_local_paths_are_told_apart() {
    let root = Path::new("project");
    let local = |path: &str| root.join(path).to_string_lossy().into_owned();
    assert_eq!(
      location(root, "owner/repo"),
      "https://github.com/owner/repo.git"
    );
    assert_eq!(
      location(root, "deploy@example.com:team/repo.git"),
      "deploy@example.com:team/repo.git"
    );
    assert_eq!(location(root, "ssh://host/repo"), "ssh://host/repo");
    assert_eq!(
      location(root, "./vendor/extension"),
      local("./vendor/extension")
    );
    // A one-letter prefix is a Windows drive, never an SSH host.
    let drive = "C:\\extensions\\demo";
    let expected = if cfg!(windows) {
      drive.to_owned()
    } else {
      local(drive)
    };
    assert_eq!(location(root, drive), expected);
  }

  #[test]
  fn repositories_with_the_same_name_get_distinct_cache_entries() {
    let first = cache_key("/opt/one/extension");
    let second = cache_key("/opt/two/extension");
    assert_ne!(first, second);
    assert!(first.starts_with("extension-"));
  }
}
