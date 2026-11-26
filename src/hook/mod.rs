//! The hook Claude Code or Codex runs on its lifecycle events when a project registers
//! `hivex hook` (#258). It reminds each session of the recovery rule, records when the session runs
//! HiveX's retrieval and denies edits, agent launches, `git commit` and `gh pr create` until it
//! has. It reads only documented hook fields, never the transcript, and lets through with a
//! warning any input it does not recognise, so a change in a host's format never locks a session
//! out.

mod registration;

pub use registration::registrations;

use regex::Regex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fmt::Write;
use std::fs;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::{Duration, SystemTime};

const REMINDER: &str = "This project uses HiveX. Before acting, whether coordinating, briefing, implementing, reviewing, researching or answering, recover what has been decided on the topic with `hivex search`, `hivex relations` and `hivex read`, and state the rules you apply with document, line and the owner's date. Remembered or summarised context is not verified: after a compaction, recover again. Until this session runs one of those commands, edits, agent launches, `git commit` and `gh pr create` are denied.";
const DENIAL: &str = "HiveX: this session has not recovered what has been decided on its topic since it started or was last compacted. Run `hivex search \"<the task's topic>\" --max-bytes 65536`, read the authorities it returns with `hivex read` and follow `hivex relations`, state the rules you apply with document, line and the owner's date, then retry.";
/// Calls that change the project or brief another agent. Codex reports its edits as
/// `apply_patch` and its agent launches as `spawn_agent`.
const GATED: &[&str] = &[
  "Edit",
  "Write",
  "NotebookEdit",
  "Agent",
  "apply_patch",
  "spawn_agent",
];
/// Claude Code runs shell commands through `PowerShell` on Windows without Git Bash.
const SHELLS: &[&str] = &["Bash", "PowerShell"];
/// Markers whose session never reported its end are removed after a week.
const EXPIRY: Duration = Duration::from_secs(7 * 24 * 60 * 60);

static RETRIEVAL: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"(?:^|[\s;&|(/\\])hivex(?:\.exe)?\s+(?:search|read|relations)\b").expect("valid")
});
static PUBLISHING: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"(?:^|[\s;&|(])(?:git(?:\s+-[cC]\s+\S+)*\s+commit|gh\s+pr\s+create)\b")
    .expect("valid")
});

/// The JSON the host reads from the hook's stdout for the event in `input`, or `Null` when the
/// hook has nothing to say and prints nothing.
pub fn handle(input: &[u8]) -> Value {
  let Ok(event) = serde_json::from_slice::<Value>(input) else {
    return warning("its input is not JSON");
  };
  let Some(session) = event["session_id"].as_str().filter(|id| !id.is_empty()) else {
    return warning("its input has no session_id");
  };
  let markers = Markers::new(session, event["agent_id"].as_str().unwrap_or_default());
  match event["hook_event_name"].as_str().unwrap_or_default() {
    "SessionStart" => start(&event, &markers),
    "SubagentStart" => context("SubagentStart"),
    "PreToolUse" => before_tool(&event, &markers),
    "PostToolUse" => after_tool(&event, &markers),
    "PostCompact" => {
      markers.forget();
      Value::Null
    }
    "SessionEnd" => {
      markers.end_session();
      Value::Null
    }
    other => warning(&format!("it does not handle the event {other:?}")),
  }
}

fn start(event: &Value, markers: &Markers) -> Value {
  // A cleared or compacted conversation no longer holds what the session recovered.
  if matches!(event["source"].as_str(), Some("clear" | "compact")) {
    markers.forget();
  }
  markers.expire();
  context("SessionStart")
}

fn before_tool(event: &Value, markers: &Markers) -> Value {
  let Some(tool) = event["tool_name"].as_str() else {
    return warning("its PreToolUse input has no tool_name");
  };
  let gated = match command(event, tool) {
    Some(Ok(command)) => PUBLISHING.is_match(command),
    Some(Err(())) => return warning("its shell input has no command"),
    None => GATED.contains(&tool),
  };
  if !gated {
    return Value::Null;
  }
  match markers.recovered() {
    Ok(true) => Value::Null,
    Ok(false) => json!({"hookSpecificOutput": {
      "hookEventName": "PreToolUse",
      "permissionDecision": "deny",
      "permissionDecisionReason": DENIAL,
    }}),
    Err(reason) => warning(reason),
  }
}

fn after_tool(event: &Value, markers: &Markers) -> Value {
  let Some(tool) = event["tool_name"].as_str() else {
    return warning("its PostToolUse input has no tool_name");
  };
  match command(event, tool) {
    Some(Ok(command)) if RETRIEVAL.is_match(command) => match markers.record() {
      Ok(()) => Value::Null,
      Err(_) => warning("it cannot write its marker directory"),
    },
    Some(Err(())) => warning("its shell input has no command"),
    _ => Value::Null,
  }
}

/// The shell command of a shell tool's call; `None` for other tools.
fn command<'a>(event: &'a Value, tool: &str) -> Option<Result<&'a str, ()>> {
  SHELLS
    .contains(&tool)
    .then(|| event["tool_input"]["command"].as_str().ok_or(()))
}

fn context(event: &str) -> Value {
  json!({"hookSpecificOutput": {"hookEventName": event, "additionalContext": REMINDER}})
}

/// Lets the call through and shows why the hook could not apply the rule.
fn warning(reason: &str) -> Value {
  json!({"systemMessage": format!(
    "HiveX hook: {reason}, so the call goes ahead. Recover with `hivex search`, `hivex relations` and `hivex read` before acting anyway."
  )})
}

/// Empty files in the user's temporary directory, one per session and agent, named by hashes of
/// their ids, recording that the agent ran HiveX's retrieval since it started or was compacted.
struct Markers {
  directory: PathBuf,
  session: String,
  own: String,
}

impl Markers {
  fn new(session: &str, agent: &str) -> Self {
    let session = digest(session);
    Self {
      // shortcut: one directory per temporary directory; a second user of a shared /tmp fails
      // open with a warning, so name it per user if that case appears.
      directory: std::env::temp_dir().join("hivex-hook"),
      own: format!("{session}-{}", digest(agent)),
      session,
    }
  }

  fn record(&self) -> std::io::Result<()> {
    fs::create_dir_all(&self.directory)?;
    fs::write(self.directory.join(&self.own), b"")
  }

  /// Whether this agent's marker exists. A directory where no marker could be written must not
  /// deny every call, so a missing marker is checked against a probe write.
  fn recovered(&self) -> Result<bool, &'static str> {
    if self.directory.join(&self.own).is_file() {
      return Ok(true);
    }
    let probe = self.directory.join(format!("{}.probe", self.own));
    fs::create_dir_all(&self.directory)
      .and_then(|()| fs::write(&probe, b""))
      .and_then(|()| fs::remove_file(&probe))
      .map(|()| false)
      .map_err(|_| "it cannot write its marker directory")
  }

  fn forget(&self) {
    let _ = fs::remove_file(self.directory.join(&self.own));
  }

  fn end_session(&self) {
    let prefix = format!("{}-", self.session);
    self.remove(|name, _| name.starts_with(&prefix));
    self.expire();
  }

  fn expire(&self) {
    self.remove(|_, age| age > EXPIRY);
  }

  fn remove(&self, condition: impl Fn(&str, Duration) -> bool) {
    let Ok(entries) = fs::read_dir(&self.directory) else {
      return;
    };
    for entry in entries.flatten() {
      let age = entry
        .metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .unwrap_or_default();
      if condition(&entry.file_name().to_string_lossy(), age) {
        let _ = fs::remove_file(entry.path());
      }
    }
  }
}

fn digest(id: &str) -> String {
  Sha256::digest(id.as_bytes())[..16]
    .iter()
    .fold(String::new(), |mut hex, byte| {
      let _ = write!(hex, "{byte:02x}");
      hex
    })
}
