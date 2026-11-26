//! Empty files recording that an agent ran HiveX's retrieval since it started or was compacted.
//! They live in `hivex-hook` under the user's temporary directory, one per session and agent, named
//! by hashes of their ids. On Unix the directory must be the user's own (mode 0700), so another
//! user of a shared `/tmp` can neither plant links in it nor have the hook clean up their files.

use sha2::{Digest, Sha256};
use std::fmt::Write;
use std::fs::{self, DirBuilder, Metadata, OpenOptions};
use std::io;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// Markers whose session never reported its end are removed after a week.
const EXPIRY: Duration = Duration::from_secs(7 * 24 * 60 * 60);

pub struct Markers {
  directory: PathBuf,
  session: String,
  own: String,
}

impl Markers {
  pub fn new(session: &str, agent: &str) -> Self {
    let session = digest(session);
    Self {
      directory: std::env::temp_dir().join("hivex-hook"),
      own: format!("{session}-{}", digest(agent)),
      session,
    }
  }

  pub fn record(&self) -> io::Result<()> {
    self.ready()?;
    // A new recovery restarts the marker's age; a new file is never opened through a link.
    self.forget()?;
    OpenOptions::new()
      .write(true)
      .create_new(true)
      .open(self.directory.join(&self.own))
      .map(drop)
  }

  /// Whether this agent's marker exists. Storage where no marker could be written or removed must
  /// neither deny every call nor keep an old recovery in effect, so every check first writes and
  /// removes a probe unique to this process.
  pub fn recovered(&self) -> io::Result<bool> {
    self.ready()?;
    let probe = self
      .directory
      .join(format!("{}.{}", self.own, std::process::id()));
    OpenOptions::new()
      .write(true)
      .create_new(true)
      .open(&probe)?;
    fs::remove_file(&probe)?;
    match fs::symlink_metadata(self.directory.join(&self.own)) {
      Ok(metadata) => Ok(metadata.is_file()),
      Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
      Err(error) => Err(error),
    }
  }

  /// Removes this agent's marker; a marker that cannot be removed is an error, since it would keep
  /// the recovery in effect after a compaction.
  pub fn forget(&self) -> io::Result<()> {
    self.ready()?;
    match fs::remove_file(self.directory.join(&self.own)) {
      Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
      result => result,
    }
  }

  pub fn end_session(&self) {
    let prefix = format!("{}-", self.session);
    self.remove(|name, _| name.starts_with(&prefix));
    self.expire();
  }

  pub fn expire(&self) {
    self.remove(|_, age| age > EXPIRY);
  }

  /// Removes the regular files with a marker's name that meet `condition`, and nothing else.
  fn remove(&self, condition: impl Fn(&str, Duration) -> bool) {
    if self.ready().is_err() {
      return;
    }
    let Ok(entries) = fs::read_dir(&self.directory) else {
      return;
    };
    for entry in entries.flatten() {
      let name = entry.file_name().to_string_lossy().into_owned();
      let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
        continue;
      };
      if metadata.is_file() && marker_name(&name) && condition(&name, age(&metadata)) {
        let _ = fs::remove_file(entry.path());
      }
    }
  }

  /// Creates the directory when missing and checks that it is a private directory, not a link.
  fn ready(&self) -> io::Result<()> {
    let mut builder = DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    match builder.create(&self.directory) {
      Err(error) if error.kind() != io::ErrorKind::AlreadyExists => return Err(error),
      _ => {}
    }
    let metadata = fs::symlink_metadata(&self.directory)?;
    if metadata.is_dir() && private(&metadata) {
      return Ok(());
    }
    Err(io::Error::other(
      "the marker directory is not a private directory",
    ))
  }
}

#[cfg(unix)]
fn private(metadata: &Metadata) -> bool {
  // No permission bit for the group or others, the low six bits of the mode.
  std::os::unix::fs::PermissionsExt::mode(&metadata.permissions()).trailing_zeros() >= 6
}

/// Windows gives each user a temporary directory of their own.
#[cfg(not(unix))]
fn private(_: &Metadata) -> bool {
  true
}

fn age(metadata: &Metadata) -> Duration {
  metadata
    .modified()
    .ok()
    .and_then(|modified| SystemTime::now().duration_since(modified).ok())
    .unwrap_or_default()
}

/// `<session hash>-<agent hash>`, optionally followed by a probe's `.<process id>`.
fn marker_name(name: &str) -> bool {
  let (marker, probe) = name.split_once('.').unwrap_or((name, "0"));
  let hex = |part: &str| part.len() == 32 && part.bytes().all(|byte| byte.is_ascii_hexdigit());
  marker
    .split_once('-')
    .is_some_and(|(session, agent)| hex(session) && hex(agent))
    && !probe.is_empty()
    && probe.bytes().all(|byte| byte.is_ascii_digit())
}

fn digest(id: &str) -> String {
  Sha256::digest(id.as_bytes())[..16]
    .iter()
    .fold(String::new(), |mut hex, byte| {
      let _ = write!(hex, "{byte:02x}");
      hex
    })
}
