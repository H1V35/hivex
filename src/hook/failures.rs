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
  let original = create(&directory)?;
  // Checking the size, rotating and appending under one lock keeps two failures at the same time
  // from taking the file past its limit or replacing the full previous file with a new one.
  let _lock = wait(open(&directory.join(LOCK))?, File::try_lock)?;
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
    file.seek(SeekFrom::End(0))?;
    return file.write_all(line.as_bytes());
  }
  drop(file);
  rotate(&directory, &original)?.write_all(line.trim_start().as_bytes())
}

/// Renames the full current file over the previous one, in one step that an interruption cannot
/// leave halfway, and opens a new current file. The directory is checked again first, so one that
/// was replaced since the hook opened its files is left as it is.
fn rotate(directory: &Path, original: &fs::Metadata) -> io::Result<File> {
  let now = fs::symlink_metadata(directory)?;
  if !(now.is_dir() && same(&now, original)) {
    return Err(io::Error::other("the record's directory changed"));
  }
  fs::rename(directory.join(CURRENT), directory.join(PREVIOUS))?;
  open(&directory.join(CURRENT))
}

/// Opens a file of the record to read and write, creating it, and refuses one that is not a
/// regular file, such as a link.
fn open(path: &Path) -> io::Result<File> {
  if fs::symlink_metadata(path).is_ok_and(|metadata| !metadata.is_file()) {
    return Err(io::Error::other("not a regular file"));
  }
  let mut options = options();
  options.read(true).write(true);
  // A missing file is created only where nothing is, not even a link.
  let file = match options.clone().create_new(true).open(path) {
    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => options.open(path)?,
    created => created?,
  };
  verify(path, file)
}

/// An existing file of the record, opened to read.
fn existing(path: &Path) -> io::Result<File> {
  verify(path, options().read(true).open(path)?)
}

/// Options that refuse a link where the path ends, on Unix, or open the link itself, on Windows,
/// and, on Unix, do not wait on a named pipe; `verify` then checks what was opened.
fn options() -> OpenOptions {
  let mut options = OpenOptions::new();
  #[cfg(unix)]
  std::os::unix::fs::OpenOptionsExt::custom_flags(
    &mut options,
    libc::O_NOFOLLOW | libc::O_NONBLOCK,
  );
  // FILE_FLAG_OPEN_REPARSE_POINT
  #[cfg(windows)]
  std::os::windows::fs::OpenOptionsExt::custom_flags(&mut options, 0x0020_0000);
  options
}

/// `file`, opened from `path`, when it is still the regular file the path names, in a directory
/// that is not a link, so a link put in either place while it opened is refused.
fn verify(path: &Path, file: File) -> io::Result<File> {
  let opened = file.metadata()?;
  let named = fs::symlink_metadata(path)?;
  let directory = fs::symlink_metadata(path.parent().unwrap_or(path))?;
  if opened.is_file() && named.is_file() && same(&opened, &named) && directory.is_dir() {
    return Ok(file);
  }
  Err(io::Error::other("not a regular file of the record"))
}

#[cfg(unix)]
fn same(opened: &fs::Metadata, named: &fs::Metadata) -> bool {
  use std::os::unix::fs::MetadataExt;
  (opened.dev(), opened.ino()) == (named.dev(), named.ino())
}

/// Windows opened a link itself, which is not a regular file, so the types already tell.
#[cfg(not(unix))]
fn same(_: &fs::Metadata, _: &fs::Metadata) -> bool {
  true
}

/// `file` once `take` locks it, held until the file closes, or an error when another process holds
/// the lock for longer than a failure waits.
fn wait(file: File, take: fn(&File) -> Result<(), TryLockError>) -> io::Result<File> {
  for _ in 0..ATTEMPTS {
    match take(&file) {
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
  // A directory that is a link, or not a directory, is not read.
  let files = if fs::symlink_metadata(&directory).is_ok_and(|metadata| !metadata.is_dir()) {
    problems.push(json!({"path": directory.to_string_lossy(), "problem": "not-a-directory"}));
    Vec::new()
  } else {
    snapshot(&directory, &mut problems)
  };
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

/// The two files as a reader sees them under the lock, shared with other readers so no failure
/// rotates them meanwhile, naming in `problems` each it could not read.
fn snapshot(directory: &Path, problems: &mut Vec<Value>) -> Vec<String> {
  let lock = directory.join(LOCK);
  let _lock = match shared(&lock) {
    Ok(lock) => lock,
    Err(problem) => {
      problems.push(json!({"path": lock.to_string_lossy(), "problem": problem}));
      return Vec::new();
    }
  };
  let mut files = Vec::new();
  for path in [PREVIOUS, CURRENT].map(|name| directory.join(name)) {
    match read(&path) {
      Ok(file) => files.extend(file),
      Err(problem) => problems.push(json!({"path": path.to_string_lossy(), "problem": problem})),
    }
  }
  files
}

/// The lock shared with other readers, none when no failure has created it yet, which reading
/// leaves so, or the problem that kept it.
fn shared(path: &Path) -> Result<Option<File>, &'static str> {
  match existing(path) {
    Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
    Err(_) => Err("unreadable"),
    Ok(file) => wait(file, File::try_lock_shared)
      .map(Some)
      .map_err(|_| "locked"),
  }
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
  existing(path)
    .and_then(|file| file.take(LIMIT).read_to_end(&mut bytes))
    .map_err(|_| "unreadable")?;
  Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

/// The entry on `line`, rebuilt from the fields the hook writes when each has the form it writes,
/// so the report never carries anything else from the file.
fn entry(line: &str) -> Option<Value> {
  let value: Value = serde_json::from_str(line).ok()?;
  let field = |name: &str, valid: fn(&str) -> bool| value[name].as_str().filter(|text| valid(text));
  // `event` and `session` are always written, as `null` when there is none.
  let optional = |name: &str, valid: fn(&str) -> bool| match value.get(name)? {
    Value::Null => Some(None),
    _ => field(name, valid).map(Some),
  };
  Some(json!({
    "time": field("time", utc)?,
    "host": field("host", |host| ["claude", "codex", "unknown"].contains(&host))?,
    "event": optional("event", |event| super::EVENTS.contains(&event))?,
    "reason": field("reason", |reason| super::REASONS.contains(&reason))?,
    "session": optional("session", |session| {
      session.len() == 32 && session.bytes().all(|byte| byte.is_ascii_hexdigit())
    })?,
  }))
}

/// Whether `time` has the form `now` writes, `YYYY-MM-DDTHH:MM:SSZ`.
fn utc(time: &str) -> bool {
  time.len() == 20
    && time.bytes().enumerate().all(|(index, byte)| match index {
      4 | 7 => byte == b'-',
      10 => byte == b'T',
      13 | 16 => byte == b':',
      19 => byte == b'Z',
      _ => byte.is_ascii_digit(),
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

/// Creates the directory, the user's own on Unix, and refuses one that is a link; what it is, so a
/// later step can tell whether it was replaced.
fn create(directory: &Path) -> io::Result<fs::Metadata> {
  let mut builder = fs::DirBuilder::new();
  builder.recursive(true);
  #[cfg(unix)]
  std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
  builder.create(directory)?;
  let metadata = fs::symlink_metadata(directory)?;
  if metadata.is_dir() {
    return Ok(metadata);
  }
  Err(io::Error::other("the record's directory is a link"))
}

#[cfg(all(test, unix))]
mod tests {
  use super::{CURRENT, PREVIOUS, create, open, options, rotate, verify};
  use std::fs;
  use std::path::PathBuf;
  use std::process::Command;
  use std::sync::mpsc;
  use std::thread;
  use std::time::Duration;

  fn scratch() -> PathBuf {
    let directory = std::env::temp_dir().join(format!("hivex-verify-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&directory).unwrap();
    directory
  }

  /// A link where a file of the record was is not followed when opened, and one put there after it
  /// was opened is refused.
  #[test]
  fn a_file_replaced_by_a_link_is_refused() {
    let directory = scratch();
    let (path, elsewhere) = (directory.join("record"), directory.join("elsewhere"));
    fs::write(&elsewhere, "").unwrap();
    assert!(open(&path).is_ok());
    let opened = fs::File::open(&elsewhere).unwrap();
    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &path).unwrap();
    assert!(verify(&path, opened).is_err());
    assert!(options().read(true).open(&path).is_err());
    assert!(open(&path).is_err());
    fs::remove_dir_all(&directory).unwrap();
  }

  /// A directory replaced by a link before a failure rotates the record keeps the files of the
  /// directory the link points to.
  #[test]
  fn rotating_never_touches_a_directory_put_in_its_place() {
    let scratch = scratch();
    let (record, other) = (scratch.join("hivex"), scratch.join("other"));
    for directory in [&record, &other] {
      fs::create_dir(directory).unwrap();
      fs::write(directory.join(CURRENT), "current\n").unwrap();
      fs::write(directory.join(PREVIOUS), "previous\n").unwrap();
    }
    let original = create(&record).unwrap();
    assert!(rotate(&record, &original).is_ok());
    assert_eq!(
      fs::read_to_string(record.join(PREVIOUS)).unwrap(),
      "current\n"
    );
    assert_eq!(fs::read_to_string(record.join(CURRENT)).unwrap(), "");
    fs::rename(&record, scratch.join("moved")).unwrap();
    std::os::unix::fs::symlink(&other, &record).unwrap();
    assert!(rotate(&record, &original).is_err());
    assert_eq!(
      fs::read_to_string(other.join(CURRENT)).unwrap(),
      "current\n"
    );
    assert_eq!(
      fs::read_to_string(other.join(PREVIOUS)).unwrap(),
      "previous\n"
    );
    fs::remove_dir_all(&scratch).unwrap();
  }

  /// A named pipe where a file of the record was is refused at once, without waiting for a writer.
  #[test]
  fn a_pipe_in_place_of_a_file_is_refused_at_once() {
    let directory = scratch();
    let path = directory.join("record");
    assert!(
      Command::new("mkfifo")
        .arg(&path)
        .status()
        .unwrap()
        .success()
    );
    let (sender, receiver) = mpsc::channel();
    let pipe = path.clone();
    thread::spawn(move || {
      let opened = options().read(true).open(&pipe);
      sender.send(opened.and_then(|file| verify(&pipe, file)).is_err())
    });
    assert_eq!(receiver.recv_timeout(Duration::from_secs(5)), Ok(true));
    fs::remove_dir_all(&directory).unwrap();
  }
}
