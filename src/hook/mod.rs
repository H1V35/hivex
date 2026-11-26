//! The hook Claude Code or Codex runs on its lifecycle events when a project registers
//! `hivex hook` (#258). It reminds each session of the recovery rule, records when the session runs
//! HiveX's retrieval and denies edits, agent and workflow launches, the MCP tools the project's
//! matcher sends it, `git commit` and `gh pr create` until it has. It reads only documented hook
//! fields, never the transcript, and lets through with a warning any input it does not recognise,
//! so a change in a host's format never locks a session out.

mod markers;
mod registration;
mod shell;

pub use registration::registrations;

use markers::Markers;
use serde::Deserialize;
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

/// The documented fields the hook reads; the rest of the input is skipped unread.
#[derive(Deserialize)]
struct Event {
  session_id: Option<String>,
  agent_id: Option<String>,
  hook_event_name: Option<String>,
  source: Option<String>,
  tool_name: Option<String>,
  tool_input: Option<Value>,
}

/// What a `PreToolUse` or `PostToolUse` call is, from its tool and input.
enum Call<'a> {
  /// A shell command, and whether a backslash escapes the next character in it.
  Shell(&'a str, bool),
  Gated,
  Reading,
  Unknown(String),
}

/// The JSON the host reads from the hook's stdout for the event in `input`, or `Null` when the
/// hook has nothing to say and prints nothing.
pub fn handle(input: &[u8]) -> Value {
  if input.len() > MAX_INPUT {
    return warning("its input is larger than it reads");
  }
  let Ok(event) = serde_json::from_slice::<Event>(input) else {
    return warning("its input is not a hook event it knows");
  };
  let Some(session) = event.session_id.as_deref().filter(|id| !id.is_empty()) else {
    return warning("its input has no session_id");
  };
  let markers = Markers::new(session, event.agent_id.as_deref().unwrap_or_default());
  match event.hook_event_name.as_deref().unwrap_or_default() {
    "SessionStart" => start(&event, &markers),
    "SubagentStart" => context("SubagentStart"),
    "PreToolUse" => before_tool(&event, &markers),
    "PostToolUse" => after_tool(&event, &markers),
    "PostCompact" => reset(&markers),
    "SessionEnd" => {
      markers.end_session();
      Value::Null
    }
    other => warning(&format!("it does not handle the event {other:?}")),
  }
}

fn start(event: &Event, markers: &Markers) -> Value {
  markers.expire();
  // A cleared or compacted conversation no longer holds what the session recovered.
  if matches!(event.source.as_deref(), Some("clear" | "compact")) && markers.forget().is_err() {
    return warning("it cannot remove the recovery recorded before the compaction");
  }
  context("SessionStart")
}

fn reset(markers: &Markers) -> Value {
  match markers.forget() {
    Ok(()) => Value::Null,
    Err(_) => warning("it cannot remove the recovery recorded before the compaction"),
  }
}

fn before_tool(event: &Event, markers: &Markers) -> Value {
  let gated = match call(event) {
    Ok(Call::Shell(command, escapes)) => shell::publishes(command, escapes),
    Ok(Call::Gated) => true,
    Ok(Call::Reading) => false,
    Ok(Call::Unknown(tool)) => return warning(&format!("it does not know the tool {tool:?}")),
    Err(reason) => return warning(reason),
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
    Err(_) => warning("it cannot use its marker directory"),
  }
}

fn after_tool(event: &Event, markers: &Markers) -> Value {
  match call(event) {
    Ok(Call::Shell(command, escapes)) if shell::recovers(command, escapes) => {
      match markers.record() {
        Ok(()) => Value::Null,
        Err(_) => warning("it cannot use its marker directory"),
      }
    }
    Ok(Call::Unknown(tool)) => warning(&format!("it does not know the tool {tool:?}")),
    Err(reason) => warning(reason),
    Ok(_) => Value::Null,
  }
}

/// Classifies the call by its tool and checks the documented input fields a decision needs.
fn call(event: &Event) -> Result<Call<'_>, &'static str> {
  let Some(tool) = event.tool_name.as_deref() else {
    return Err("its input has no tool_name");
  };
  let input = event.tool_input.as_ref().unwrap_or(&Value::Null);
  let fields = |names: &[&str]| names.iter().all(|name| input[name].is_string());
  let well_formed = match tool {
    // Claude Code runs shell commands through `PowerShell` on Windows without Git Bash.
    "Bash" | "PowerShell" => {
      let command = input["command"]
        .as_str()
        .ok_or("its shell input has no command")?;
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
    _ if tool.starts_with("mcp__") => input.is_object(),
    _ if READING.contains(&tool) => return Ok(Call::Reading),
    _ => return Ok(Call::Unknown(tool.to_owned())),
  };
  if well_formed {
    return Ok(Call::Gated);
  }
  Err("its tool input lacks the fields it checks")
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
