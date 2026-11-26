//! The local record of each time the hook let a call through because it could not apply the rule
//! (#261), so a failure that repeats, such as a host changing its input format, is noticed. A line
//! holds the time, the host, the event, a stable reason code and a hash of the session, never a
//! command, path, tool input or anything else from the session. The record belongs to the user, in
//! their per-user state directory, and keeps at most two files of `LIMIT` bytes. Writing it never
//! fails the hook: a record it cannot write is skipped.

use crate::compatibility::civil_date;
use serde_json::{Map, Value, json};
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The size past which the record starts a new file and keeps the previous one.
const LIMIT: u64 = 64 * 1024;
const CURRENT: &str = "hook-failures.jsonl";
const PREVIOUS: &str = "hook-failures.1.jsonl";
/// How many of the newest entries `hivex hook --failures` lists.
const RECENT: usize = 20;

pub struct Entry<'a> {
  pub host: &'static str,
  pub event: Option<&'a str>,
  pub reason: &'static str,
  pub session: Option<String>,
}

/// Appends `entry` to the record, or skips it when the record cannot be written.
pub fn record(entry: &Entry) {
  let _ = append(entry);
}

fn append(entry: &Entry) -> io::Result<()> {
  let directory = directory().ok_or_else(|| io::Error::other("no per-user state directory"))?;
  create(&directory)?;
  let line = json!({
    "time": now(),
    "host": entry.host,
    "event": entry.event,
    "reason": entry.reason,
    "session": entry.session,
  })
  .to_string()
    + "\n";
  let file = directory.join(CURRENT);
  let size = u64::try_from(line.len()).unwrap_or(u64::MAX);
  match fs::symlink_metadata(&file) {
    Ok(metadata) if !metadata.is_file() => {
      return Err(io::Error::other("the record is not a file"));
    }
    Ok(metadata) if metadata.len().saturating_add(size) > LIMIT => {
      fs::rename(&file, directory.join(PREVIOUS))?;
    }
    _ => {}
  }
  OpenOptions::new()
    .create(true)
    .append(true)
    .open(&file)?
    .write_all(line.as_bytes())
}

/// `hivex hook --failures`: where the record is, how many entries it holds for each reason and the
/// newest of them, newest first.
pub fn report() -> Value {
  let Some(directory) = directory() else {
    return json!({"command": "hook", "log": null, "total": 0, "reasons": {}, "recent": []});
  };
  let entries: Vec<Value> = [PREVIOUS, CURRENT]
    .iter()
    .filter_map(|name| fs::read_to_string(directory.join(name)).ok())
    .flat_map(|text| {
      text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(Value::is_object)
        .collect::<Vec<_>>()
    })
    .collect();
  let mut reasons = Map::new();
  for reason in entries.iter().filter_map(|entry| entry["reason"].as_str()) {
    let count = reasons.entry(reason).or_insert(json!(0));
    *count = json!(count.as_u64().unwrap_or_default() + 1);
  }
  json!({
    "command": "hook",
    "log": directory.join(CURRENT).to_string_lossy(),
    "total": entries.len(),
    "reasons": reasons,
    "recent": entries.iter().rev().take(RECENT).collect::<Vec<_>>(),
  })
}

/// The current UTC time as `YYYY-MM-DDTHH:MM:SSZ`.
fn now() -> String {
  let seconds = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |elapsed| elapsed.as_secs());
  let time = seconds % 86_400;
  format!(
    "{}T{:02}:{:02}:{:02}Z",
    civil_date(seconds / 86_400),
    time / 3_600,
    time / 60 % 60,
    time % 60
  )
}

/// Each platform's per-user place for application logs and state: `%LOCALAPPDATA%` on Windows,
/// `~/Library/Logs` on macOS and `$XDG_STATE_HOME`, by default `~/.local/state`, elsewhere.
#[cfg(windows)]
fn directory() -> Option<PathBuf> {
  absolute(std::env::var_os("LOCALAPPDATA")).map(|data| data.join("hivex"))
}

#[cfg(target_os = "macos")]
fn directory() -> Option<PathBuf> {
  absolute(std::env::var_os("HOME")).map(|home| home.join("Library").join("Logs").join("hivex"))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn directory() -> Option<PathBuf> {
  absolute(std::env::var_os("XDG_STATE_HOME"))
    .or_else(|| absolute(std::env::var_os("HOME")).map(|home| home.join(".local").join("state")))
    .map(|state| state.join("hivex"))
}

/// A relative path in these variables is invalid, so it is ignored.
fn absolute(value: Option<OsString>) -> Option<PathBuf> {
  value.map(PathBuf::from).filter(|path| path.is_absolute())
}

/// Creates the directory, the user's own on Unix, and refuses one that is a link.
fn create(directory: &Path) -> io::Result<()> {
  let mut builder = fs::DirBuilder::new();
  builder.recursive(true);
  #[cfg(unix)]
  std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
  builder.create(directory)?;
  if fs::symlink_metadata(directory)?.is_dir() {
    return Ok(());
  }
  Err(io::Error::other("the record's directory is a link"))
}
