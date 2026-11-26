//! The hook Claude Code or Codex runs on its lifecycle events when a project registers
//! `hivex hook` (#258). It reminds each session of the recovery rule, records when the session runs
//! HiveX's retrieval and denies edits, agent and workflow launches, the MCP tools the project's
//! matcher sends it, `git commit` and `gh pr create` until it has. It reads only documented hook
//! fields, never the transcript, and lets through any input it does not recognise, so a change in a
//! host's format never locks a session out; it then warns the person and the agent and records the
//! failure without its content (#261).

mod failures;
mod markers;
mod registration;
mod shell;

pub use failures::report as failures;
pub use registration::registrations;

use failures::Entry;
use markers::Markers;
use serde::Deserialize;
use serde::de::IgnoredAny;
use serde_json::{Value, json};

/// Larger inputs are let through with a warning rather than parsed: an answer must stay fast.
pub const MAX_INPUT: usize = 8 * 1024 * 1024;
const REMINDER: &str = "This project uses HiveX. Before acting, whether coordinating, briefing, implementing, reviewing, researching or answering, recover what has been decided on the topic with `hivex search`, `hivex relations` and `hivex read`, and state the rules you apply with document, line and the owner's date. Remembered or summarised context is not verified: after a compaction, recover again. Until this session runs one of those commands, edits, agent launches, the MCP tools the project gates, `git commit` and `gh pr create` are denied.";
const DENIAL: &str = "HiveX: this session has not recovered what has been decided on its topic since it started or was last compacted. Run `hivex search \"<the task's topic>\" --max-bytes 65536`, read the authorities it returns with `hivex read` and follow `hivex relations`, state the rules you apply with document, line and the owner's date, then retry.";
/// Tools that only read, which the hook lets through without a word.
const READING: &[&str] = &[
  "Read",
  "Glob",
  "Grep",
  "WebFetch",
  "WebSearch",
  "AskUserQuestion",
  "ExitPlanMode",
  "update_plan",
];

/// The documented fields the hook reads; the rest of the input is skipped unread. `turn_id` and
/// `model` only tell Codex's events apart.
#[derive(Deserialize)]
struct Event {
  session_id: Option<String>,
  agent_id: Option<String>,
  hook_event_name: Option<String>,
  source: Option<String>,
  tool_name: Option<String>,
  tool_input: Option<Value>,
  turn_id: Option<IgnoredAny>,
  model: Option<IgnoredAny>,
}

/// What a `PreToolUse` or `PostToolUse` call is, from its tool and input.
enum Call<'a> {
  /// A shell command, and whether a backslash escapes the next character in it.
  Shell(&'a str, bool),
  Gated,
  Reading,
}

/// Why the hook cannot apply the rule to an input: a stable code for the record and the words the
/// person and the agent read.
struct Failure {
  code: &'static str,
  reason: String,
}

impl Failure {
  fn new(code: &'static str, reason: impl Into<String>) -> Self {
    Self {
      code,
      reason: reason.into(),
    }
  }
}

/// The JSON the host reads from the hook's stdout for the event in `input`, or `Null` when the
/// hook has nothing to say and prints nothing.
pub fn handle(input: &[u8]) -> Value {
  let event = (input.len() <= MAX_INPUT)
    .then(|| serde_json::from_slice::<Event>(input).ok())
    .flatten();
  answer(input.len(), event.as_ref()).unwrap_or_else(|failure| fail_open(event.as_ref(), &failure))
}

fn answer(size: usize, event: Option<&Event>) -> Result<Value, Failure> {
  if size > MAX_INPUT {
    return Err(Failure::new(
      "input-too-large",
      "its input is larger than it reads",
    ));
  }
  let event = event
    .ok_or_else(|| Failure::new("not-a-hook-event", "its input is not a hook event it knows"))?;
  let session =
    session(event).ok_or_else(|| Failure::new("no-session-id", "its input has no session_id"))?;
  let markers = Markers::new(session, event.agent_id.as_deref().unwrap_or_default());
  match event.hook_event_name.as_deref().unwrap_or_default() {
    "SessionStart" => start(event, &markers),
    "SubagentStart" => Ok(context("SubagentStart")),
    "PreToolUse" => before_tool(event, &markers),
    "PostToolUse" => after_tool(event, &markers),
    "PostCompact" => markers.forget().map(|()| Value::Null).map_err(|_| kept()),
    "SessionEnd" => {
      markers.end_session();
      Ok(Value::Null)
    }
    other => Err(Failure::new(
      "unknown-event",
      format!("it does not handle the event {other:?}"),
    )),
  }
}

fn session(event: &Event) -> Option<&str> {
  event.session_id.as_deref().filter(|id| !id.is_empty())
}

fn start(event: &Event, markers: &Markers) -> Result<Value, Failure> {
  markers.expire();
  // A cleared or compacted conversation no longer holds what the session recovered.
  if matches!(event.source.as_deref(), Some("clear" | "compact")) && markers.forget().is_err() {
    return Err(kept());
  }
  Ok(context("SessionStart"))
}

fn kept() -> Failure {
  Failure::new(
    "compaction-record-kept",
    "it cannot remove the recovery recorded before the compaction",
  )
}

fn unusable() -> Failure {
  Failure::new(
    "marker-storage-unusable",
    "it cannot use its marker directory",
  )
}

fn before_tool(event: &Event, markers: &Markers) -> Result<Value, Failure> {
  let gated = match call(event)? {
    Call::Shell(command, escapes) => shell::publishes(command, escapes),
    Call::Gated => true,
    Call::Reading => false,
  };
  if !gated {
    return Ok(Value::Null);
  }
  match markers.recovered() {
    Ok(true) => Ok(Value::Null),
    Ok(false) => Ok(json!({"hookSpecificOutput": {
      "hookEventName": "PreToolUse",
      "permissionDecision": "deny",
      "permissionDecisionReason": DENIAL,
    }})),
    Err(_) => Err(unusable()),
  }
}

fn after_tool(event: &Event, markers: &Markers) -> Result<Value, Failure> {
  match call(event)? {
    Call::Shell(command, escapes) if shell::recovers(command, escapes) => markers
      .record()
      .map(|()| Value::Null)
      .map_err(|_| unusable()),
    _ => Ok(Value::Null),
  }
}

/// Classifies the call by its tool and checks the documented input fields a decision needs.
fn call(event: &Event) -> Result<Call<'_>, Failure> {
  let Some(tool) = event.tool_name.as_deref() else {
    return Err(Failure::new("no-tool-name", "its input has no tool_name"));
  };
  let input = event.tool_input.as_ref().unwrap_or(&Value::Null);
  let fields = |names: &[&str]| names.iter().all(|name| input[name].is_string());
  let well_formed = match tool {
    // Claude Code runs shell commands through `PowerShell` on Windows without Git Bash.
    "Bash" | "PowerShell" => {
      let command = input["command"]
        .as_str()
        .ok_or_else(|| Failure::new("no-shell-command", "its shell input has no command"))?;
      return Ok(Call::Shell(command, tool == "Bash"));
    }
    "Write" => fields(&["file_path", "content"]),
    "Edit" => fields(&["file_path", "old_string", "new_string"]),
    "NotebookEdit" => fields(&["notebook_path"]),
    "Agent" => fields(&["prompt", "description"]),
    // Codex reports its edits as `apply_patch`.
    "apply_patch" => fields(&["command"]),
    // Claude Code's workflows and Codex's `spawn_agent` launch agents, and both hosts name an MCP
    // tool `mcp__<server>__<tool>`; none documents its fields. The project's matcher decides
    // which MCP tools reach the gate, so one that does may write.
    "Workflow" | "spawn_agent" => input.is_object(),
    _ if mcp(tool) => input.is_object(),
    _ if READING.contains(&tool) => return Ok(Call::Reading),
    _ => {
      return Err(Failure::new(
        "unknown-tool",
        format!("it does not know the tool {tool:?}"),
      ));
    }
  };
  if well_formed {
    return Ok(Call::Gated);
  }
  Err(Failure::new(
    "missing-tool-fields",
    "its tool input lacks the fields it checks",
  ))
}

/// Whether `tool` has the documented shape of an MCP tool's name, `mcp__<server>__<tool>`.
fn mcp(tool: &str) -> bool {
  tool
    .strip_prefix("mcp__")
    .and_then(|rest| rest.split_once("__"))
    .is_some_and(|(server, name)| !server.is_empty() && !name.is_empty())
}

fn context(event: &str) -> Value {
  json!({"hookSpecificOutput": {"hookEventName": event, "additionalContext": REMINDER}})
}

/// Lets the call through when the hook cannot apply the rule. The person reads why in
/// `systemMessage`; the agent reads it in `additionalContext` on the events both hosts document it
/// for, after the reminder at the start of a session or subagent. The failure is recorded without
/// the input's content.
fn fail_open(event: Option<&Event>, failure: &Failure) -> Value {
  let name = event.and_then(|event| event.hook_event_name.as_deref());
  failures::record(&Entry {
    host: host(event),
    event: name.filter(|name| plain(name)),
    reason: failure.code,
    session: event.and_then(session).map(markers::digest),
  });
  let message = format!(
    "HiveX hook: {}, so the call goes ahead. Recover with `hivex search`, `hivex relations` and `hivex read` before acting anyway.",
    failure.reason
  );
  let context = match name {
    Some("SessionStart" | "SubagentStart") => format!("{REMINDER}\n\n{message}"),
    Some("PreToolUse" | "PostToolUse") => message.clone(),
    _ => return json!({"systemMessage": message}),
  };
  json!({
    "systemMessage": message,
    "hookSpecificOutput": {"hookEventName": name, "additionalContext": context},
  })
}

/// The host, as its event shows it: Codex's events carry `turn_id`, or `model` outside
/// `SessionStart`, and Claude Code sets `CLAUDE_PROJECT_DIR` for the hooks it runs.
fn host(event: Option<&Event>) -> &'static str {
  let codex = event.is_some_and(|event| {
    event.turn_id.is_some()
      || (event.model.is_some() && event.hook_event_name.as_deref() != Some("SessionStart"))
  });
  match (codex, std::env::var_os("CLAUDE_PROJECT_DIR").is_some()) {
    (true, _) => "codex",
    (false, true) => "claude",
    (false, false) => "unknown",
  }
}

/// An event name the record keeps: letters only, as both hosts name their events.
fn plain(name: &str) -> bool {
  (1..=64).contains(&name.len()) && name.bytes().all(|byte| byte.is_ascii_alphabetic())
}
