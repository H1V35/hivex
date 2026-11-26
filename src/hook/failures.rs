//! The local record of each time the hook let a call through because it could not apply the rule
//! (#261), so a failure that repeats, such as a host changing its input format, is noticed. A line
//! holds the time, the host, the event, a stable reason code and a hash of the session, never a
//! command, path, tool input or anything else from the session. The record belongs to the user, in
//! their per-user state directory, and keeps at most two files of `LIMIT` bytes. Writing it never
//! fails the hook: a record it cannot write is skipped.

use crate::compatibility::civil_date;
use serde_json::{Map, Value, json};
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The size past which the record starts a new file and keeps the previous one.
const LIMIT: u64 = 64 * 1024;
const CURRENT: &str = "hook-failures.jsonl";
const PREVIOUS: &str = "hook-failures.1.jsonl";
/// An empty file whose lock lets one process at a time write the record.
const LOCK: &str = "hook-failures.lock";
/// A failure waits at most `ATTEMPTS` pauses, about a second, for another one writing the record,
/// and then skips its entry.
const ATTEMPTS: u32 = 200;
const PAUSE: Duration = Duration::from_millis(5);
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
  // Checking the size, rotating and appending under one lock keeps two failures at the same time
  // from taking the file past its limit or replacing the full previous file with a new one.
  let _lock = lock(&directory.join(LOCK))?;
  let line = json!({
    "time": now(),
    "host": entry.host,
    "event": entry.event,
    "reason": entry.reason,
    "session": entry.session,
  })
  .to_string()
    + "\n";
  let path = directory.join(CURRENT);
  let mut file = open(&path)?;
  let length = file.metadata()?.len();
  // A line an interrupted write left unfinished is ended first, so it does not swallow this one.
  let line = if unfinished(&mut file, length)? {
    format!("\n{line}")
  } else {
    line
  };
  if length.saturating_add(u64::try_from(line.len()).unwrap_or(u64::MAX)) <= LIMIT {
    return file.write_all(line.as_bytes());
  }
  drop(file);
  fs::rename(&path, directory.join(PREVIOUS))?;
  open(&path)?.write_all(line.trim_start().as_bytes())
}

/// Opens a file of the record to read and append, creating it, and refuses one that is not a
/// regular file, such as a link.
fn open(path: &Path) -> io::Result<File> {
  if fs::symlink_metadata(path).is_ok_and(|metadata| !metadata.is_file()) {
    return Err(io::Error::other("not a regular file"));
  }
  OpenOptions::new()
    .create(true)
    .read(true)
    .append(true)
    .open(path)
}

/// The record's lock, held until the returned file closes, or an error when another process holds
/// it for longer than a failure waits.
fn lock(path: &Path) -> io::Result<File> {
  let file = open(path)?;
  for _ in 0..ATTEMPTS {
    match file.try_lock() {
      Ok(()) => return Ok(file),
      Err(TryLockError::WouldBlock) => thread::sleep(PAUSE),
      Err(TryLockError::Error(error)) => return Err(error),
    }
  }
  Err(io::Error::other("another process is writing the record"))
}

/// Whether the file of `length` bytes ends inside a line.
fn unfinished(file: &mut File, length: u64) -> io::Result<bool> {
  let Some(last) = length.checked_sub(1) else {
    return Ok(false);
  };
  let mut byte = [0];
  file.seek(SeekFrom::Start(last))?;
  file.read_exact(&mut byte)?;
  Ok(byte != *b"\n")
}

/// `hivex hook --failures`: where the record is, how many entries it holds for each reason, the
/// newest of them, newest first, how many lines it skipped because they are not entries the hook
/// writes, and each file it could not read.
pub fn report() -> Value {
  let Some(directory) = directory() else {
    return json!({
      "command": "hook", "log": null, "total": 0, "reasons": {}, "recent": [], "skipped": 0,
      "problems": [],
    });
  };
  let mut problems = Vec::new();
  let mut files = Vec::new();
  // A directory that is a link, or not a directory, is not read.
  if fs::symlink_metadata(&directory).is_ok_and(|metadata| !metadata.is_dir()) {
    problems.push(json!({"path": directory.to_string_lossy(), "problem": "not-a-directory"}));
  } else {
    for path in [PREVIOUS, CURRENT].map(|name| directory.join(name)) {
      match read(&path) {
        Ok(file) => files.extend(file),
        Err(problem) => problems.push(json!({"path": path.to_string_lossy(), "problem": problem})),
      }
    }
  }
  // Each file's lines on their own, so one a write left unfinished never joins the next file's.
  let lines: Vec<&str> = files
    .iter()
    .flat_map(|file| file.lines())
    .filter(|line| !line.is_empty())
    .collect();
  let entries: Vec<Value> = lines.iter().filter_map(|line| entry(line)).collect();
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
    "skipped": lines.len() - entries.len(),
    "problems": problems,
  })
}

/// A file of the record, none when it does not exist. Only a regular file no larger than the record
/// keeps is read.
fn read(path: &Path) -> Result<Option<String>, &'static str> {
  let metadata = match fs::symlink_metadata(path) {
    Ok(metadata) => metadata,
    Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
    Err(_) => return Err("unreadable"),
  };
  if !metadata.is_file() {
    return Err("not-a-regular-file");
  }
  if metadata.len() > LIMIT {
    return Err("larger-than-64-kib");
  }
  let mut bytes = Vec::new();
  File::open(path)
    .and_then(|file| file.take(LIMIT).read_to_end(&mut bytes))
    .map_err(|_| "unreadable")?;
  Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

/// The entry on `line`, rebuilt from the fields the hook writes when each has the form it writes,
/// so the report never carries anything else from the file.
fn entry(line: &str) -> Option<Value> {
  let value: Value = serde_json::from_str(line).ok()?;
  let field = |name: &str, valid: fn(&str) -> bool| value[name].as_str().filter(|text| valid(text));
  let optional = |name: &str, valid: fn(&str) -> bool| match &value[name] {
    Value::Null => Some(None),
    _ => field(name, valid).map(Some),
  };
  Some(json!({
    "time": field("time", |time| time.len() == 20 && time.ends_with('Z'))?,
    "host": field("host", |host| ["claude", "codex", "unknown"].contains(&host))?,
    "event": optional("event", |event| super::EVENTS.contains(&event))?,
    "reason": field("reason", |reason| {
      (1..=64).contains(&reason.len()) && reason.bytes().all(|byte| byte.is_ascii_lowercase() || byte == b'-')
    })?,
    "session": optional("session", |session| {
      session.len() == 32 && session.bytes().all(|byte| byte.is_ascii_hexdigit())
    })?,
  }))
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
