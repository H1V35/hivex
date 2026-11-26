//! `hivex hook` as Claude Code and Codex run it. Each input is the host's documented example or is
//! built from its documented fields, cited by section: Claude Code's hooks reference
//! (<https://code.claude.com/docs/en/hooks>) and Codex's hooks page
//! (<https://learn.chatgpt.com/docs/hooks>).
use crate::support::{Project, subset};
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

/// `hivex hook` with `args`. Markers go to the project's own temporary directory and the failure
/// record to its own per-user state directory, never the user's; the process runs as no host
/// unless a test says which.
fn command(p: &Project, args: &[&str]) -> Command {
  let temporary = p.path("tmp");
  fs::create_dir_all(&temporary).unwrap();
  let mut command = p.command(&[]);
  command
    .arg("hook")
    .args(args)
    .env_remove("CLAUDE_PROJECT_DIR");
  for variable in ["TMPDIR", "TMP", "TEMP"] {
    command.env(variable, &temporary);
  }
  for variable in ["HOME", "XDG_STATE_HOME", "LOCALAPPDATA"] {
    command.env(variable, p.path("state"));
  }
  command
}

/// The hook's answer to `event`, `Null` when it prints nothing.
fn hook(p: &Project, event: &Value) -> Value {
  answer(command(p, &[]), event)
}

/// The answer when Claude Code runs the hook, which sets `CLAUDE_PROJECT_DIR`.
fn claude_hook(p: &Project, event: &Value) -> Value {
  let mut command = command(p, &[]);
  command.env("CLAUDE_PROJECT_DIR", p.path(""));
  answer(command, event)
}

/// `hivex hook --failures`.
fn failures(p: &Project) -> Value {
  let output = command(p, &["--failures"]).output().unwrap();
  assert!(output.status.success(), "{output:?}");
  serde_json::from_slice(&output.stdout).unwrap()
}

fn answer(mut command: Command, event: &Value) -> Value {
  let mut child = command
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap();
  child
    .stdin
    .take()
    .unwrap()
    .write_all(event.to_string().as_bytes())
    .unwrap();
  let output = child.wait_with_output().unwrap();
  assert!(output.status.success(), "{event}: {output:?}");
  assert!(output.stderr.is_empty(), "{event}: {output:?}");
  if output.stdout.is_empty() {
    return Value::Null;
  }
  serde_json::from_slice(&output.stdout).unwrap()
}

/// Claude Code, "Common input fields": the documented `PreToolUse` input for a Bash command.
fn claude_bash(event: &str, command: &str) -> Value {
  json!({
    "session_id": "abc123",
    "prompt_id": "550e8400-e29b-41d4-a716-446655440000",
    "transcript_path": "/home/user/.claude/projects/.../transcript.jsonl",
    "cwd": "/home/user/my-project",
    "scratchpad_dir": "/tmp/claude-1000/-home-user-my-project/abc123/scratchpad",
    "permission_mode": "default",
    "hook_event_name": event,
    "tool_name": "Bash",
    "tool_input": {
      "command": command,
      "description": "Run test suite",
      "timeout": 120_000,
      "run_in_background": false
    },
    "tool_use_id": "toolu_01ABC123..."
  })
}

/// Claude Code, "`PreToolUse` input" and its "Write" fields.
fn claude_write() -> Value {
  claude_tool(
    "Write",
    &json!({"file_path": "C:\\project\\src\\index.ts", "content": "..."}),
  )
}

fn claude_tool(tool: &str, input: &Value) -> Value {
  json!({
    "session_id": "abc123",
    "transcript_path": "/Users/.../.claude/projects/.../00893aaf-19fa-41d2-8238-13269b9b3ca0.jsonl",
    "cwd": "/Users/...",
    "permission_mode": "default",
    "hook_event_name": "PreToolUse",
    "tool_name": tool,
    "tool_input": input,
    "tool_use_id": "toolu_01ABC123..."
  })
}

/// Claude Code, "`SessionStart` input": the documented resumed session, with its `source`.
fn claude_start(source: &str) -> Value {
  json!({
    "session_id": "abc123",
    "transcript_path": "/Users/.../.claude/projects/.../00893aaf-19fa-41d2-8238-13269b9b3ca0.jsonl",
    "cwd": "/Users/...",
    "hook_event_name": "SessionStart",
    "source": source,
    "model": "claude-opus-5"
  })
}

/// Claude Code, "`UserPromptSubmit` input", built from the common input fields and `prompt`.
fn claude_prompt(prompt: &str) -> Value {
  json!({
    "session_id": "abc123",
    "transcript_path": "/Users/.../.claude/projects/.../00893aaf-19fa-41d2-8238-13269b9b3ca0.jsonl",
    "cwd": "/Users/...",
    "permission_mode": "default",
    "hook_event_name": "UserPromptSubmit",
    "prompt": prompt
  })
}

/// Claude Code, "`PostCompact` input".
fn claude_post_compact() -> Value {
  json!({
    "session_id": "abc123",
    "transcript_path": "/Users/.../.claude/projects/.../00893aaf-19fa-41d2-8238-13269b9b3ca0.jsonl",
    "cwd": "/Users/...",
    "hook_event_name": "PostCompact",
    "trigger": "manual",
    "compact_summary": "Summary of the compacted conversation..."
  })
}

/// Claude Code, "Common input fields": `agent_id` and `agent_type` name the subagent a hook fires in.
fn in_subagent(mut event: Value) -> Value {
  event["agent_id"] = json!("agent-abc123");
  event["agent_type"] = json!("Explore");
  event
}

fn denied(answer: &Value) -> bool {
  answer["hookSpecificOutput"]["permissionDecision"] == "deny"
}

/// The hook let the call through and said why: to the person in `systemMessage` and, on the events
/// both hosts give it for, to the agent in `additionalContext`.
fn warned(answer: &Value) -> bool {
  let Some(message) = answer["systemMessage"]
    .as_str()
    .filter(|message| message.contains("the call goes ahead"))
  else {
    return false;
  };
  answer.get("hookSpecificOutput").is_none_or(|output| {
    output.get("permissionDecision").is_none()
      && output["additionalContext"]
        .as_str()
        .is_some_and(|context| context.contains(message))
  })
}

#[test]
fn a_session_is_denied_edits_until_it_recovers_with_hivex() {
  for recovery in [
    "hivex search \"release procedure\" --max-bytes 65536",
    "hivex read docs/guide.md --from 1 --to 40",
    "hivex relations docs/adr/0012-project-foundation-and-workflow.md --direction both",
    "cd ../other && hivex search \"recover before acting\"",
    "rtk hivex read AGENTS.md",
    "~/.cargo/bin/hivex search topic",
  ] {
    let p = Project::new();
    let start = hook(&p, &claude_start("startup"));
    subset(
      &start,
      &json!({"hookSpecificOutput": {"hookEventName": "SessionStart"}}),
    );
    assert!(
      start["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap()
        .contains("hivex search")
    );
    let denial = hook(&p, &claude_write());
    assert!(denied(&denial), "{denial}");
    // The reason reaches the agent and names the exact command to run.
    assert!(
      denial["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .unwrap()
        .contains("`hivex search \"<the task's topic>\" --max-bytes 65536`")
    );
    assert_eq!(hook(&p, &claude_bash("PostToolUse", recovery)), Value::Null);
    assert_eq!(hook(&p, &claude_write()), Value::Null, "{recovery}");
  }
  // Claude Code, "PowerShell": the same fields as Bash, on Windows with a quoted executable path.
  let p = Project::new();
  let mut powershell = claude_bash(
    "PostToolUse",
    r#"& "C:\Program Files\HiveX\hivex.exe" search topic"#,
  );
  powershell["tool_name"] = json!("PowerShell");
  hook(&p, &powershell);
  assert_eq!(hook(&p, &claude_write()), Value::Null);
  // Only a command in command position recovers, not the same words in an argument.
  let p = Project::new();
  for text in ["echo hivex search topic", "rg -n 'hivex search' docs"] {
    hook(&p, &claude_bash("PostToolUse", text));
    assert!(denied(&hook(&p, &claude_write())), "{text}");
  }
}

#[test]
fn a_compacted_or_cleared_session_recovers_again() {
  let p = Project::new();
  let recover = claude_bash("PostToolUse", "hivex search topic");
  hook(&p, &recover);
  assert_eq!(
    hook(&p, &claude_start("resume")),
    hook(&p, &claude_start("startup"))
  );
  assert_eq!(hook(&p, &claude_write()), Value::Null);
  assert_eq!(hook(&p, &claude_post_compact()), Value::Null);
  assert!(denied(&hook(&p, &claude_write())));
  for source in ["compact", "clear"] {
    hook(&p, &recover);
    hook(&p, &claude_start(source));
    assert!(denied(&hook(&p, &claude_write())), "{source}");
  }
}

#[test]
fn reads_searches_and_other_commands_are_never_denied() {
  let p = Project::new();
  for (tool, input) in [
    ("Read", json!({"file_path": "/path/to/file.txt"})),
    ("Grep", json!({"pattern": "TODO", "path": "/path/to/dir"})),
    ("Glob", json!({"pattern": "**/*.ts"})),
    (
      "WebFetch",
      json!({"url": "https://example.com", "prompt": "Summarize"}),
    ),
  ] {
    assert_eq!(hook(&p, &claude_tool(tool, &input)), Value::Null, "{tool}");
  }
  for command in [
    "npm test",
    "git status",
    "git log --grep commit",
    "gh pr view 12",
    "hivex search topic",
    "rg -n 'before git commit' docs",
    "grep -rn \"gh pr create\" docs",
    "echo git commit",
    "cat <<'EOF' > notes.md\ngit commit -m 'not run'\nEOF\nwc -l notes.md",
    "cat <<-END\n\tgh pr create\n\tEND",
  ] {
    assert_eq!(
      hook(&p, &claude_bash("PreToolUse", command)),
      Value::Null,
      "{command}"
    );
  }
  for command in [
    "git commit -m 'docs: guide'",
    "git -C ../repo -c core.hooksPath=.githooks commit -F message.txt",
    "cd repo && gh pr create --draft --body-file body.md",
    "GIT_EDITOR=true git --no-pager commit",
    "rtk git commit -m 'docs: guide'",
    "rtk proxy gh pr create --draft",
    "cat <<EOF > notes.md\ntext\nEOF\ngit commit -F notes.md",
    "cat <<\\EOF > notes.md\ntext\nEOF\ngit commit -F notes.md",
    "cat <<'END OF NOTES' > notes.md\nEOF\nEND OF NOTES\ngit commit -F notes.md",
    "cat <<\"E\\$OF\" > notes.md\nEOF\nE$OF\ngit commit -F notes.md",
    "cat <<\"E\\\nOF\" > notes.md\ntext\nEOF\ngit commit -F notes.md",
    "cargo test\ngit commit -m 'fix: tests'",
  ] {
    assert!(
      denied(&hook(&p, &claude_bash("PreToolUse", command))),
      "{command}"
    );
  }
  for (tool, input) in [
    (
      "Edit",
      json!({"file_path": "/path/to/file.txt", "old_string": "a", "new_string": "b"}),
    ),
    (
      "NotebookEdit",
      json!({"notebook_path": "/path/to/notebook.ipynb", "new_source": "x"}),
    ),
    (
      "Agent",
      json!({"prompt": "Find all API endpoints", "description": "Find API endpoints", "subagent_type": "Explore"}),
    ),
    ("Workflow", json!({})),
  ] {
    assert!(denied(&hook(&p, &claude_tool(tool, &input))), "{tool}");
  }
}

#[test]
fn a_subagent_recovers_on_its_own() {
  let p = Project::new();
  hook(&p, &claude_bash("PostToolUse", "hivex search topic"));
  // Claude Code, "SubagentStart input".
  let start = hook(
    &p,
    &json!({
      "session_id": "abc123",
      "transcript_path": "/Users/.../.claude/projects/.../00893aaf-19fa-41d2-8238-13269b9b3ca0.jsonl",
      "cwd": "/Users/...",
      "hook_event_name": "SubagentStart",
      "agent_id": "agent-abc123",
      "agent_type": "Explore"
    }),
  );
  subset(
    &start,
    &json!({"hookSpecificOutput": {"hookEventName": "SubagentStart"}}),
  );
  assert!(denied(&hook(&p, &in_subagent(claude_write()))));
  hook(
    &p,
    &in_subagent(claude_bash("PostToolUse", "hivex read AGENTS.md")),
  );
  assert_eq!(hook(&p, &in_subagent(claude_write())), Value::Null);
  assert_eq!(hook(&p, &claude_write()), Value::Null);
  // Claude Code, "SessionEnd input": the session's markers go with it.
  hook(
    &p,
    &json!({
      "session_id": "abc123",
      "transcript_path": "/Users/.../.claude/projects/.../00893aaf-19fa-41d2-8238-13269b9b3ca0.jsonl",
      "cwd": "/Users/...",
      "hook_event_name": "SessionEnd",
      "reason": "other"
    }),
  );
  assert!(denied(&hook(&p, &claude_write())));
  assert!(denied(&hook(&p, &in_subagent(claude_write()))));
}

/// An MCP tool reaches the gate only when the project's matcher lists it, so one that does is
/// gated like an edit.
#[test]
fn an_mcp_tool_the_matcher_sends_is_gated_until_recovery() {
  let p = Project::new();
  // Claude Code, "Match MCP tools" and "`PreToolUse` input": `mcp__<server>__<tool>` with the
  // server's provenance.
  let mut claude = claude_tool(
    "mcp__memory__create_entities",
    &json!({"entities": [{"name": "decision", "entityType": "note"}]}),
  );
  claude["mcp_server"] = json!({"name": "memory", "source": "project"});
  // A plugin's server carries the plugin in its segment.
  let plugin = claude_tool("mcp__plugin_my-plugin_db__query", &json!({"sql": "update"}));
  // Codex, "PreToolUse": an MCP name like `mcp__server__tool` sends its arguments.
  let mut codex = codex_tool("PreToolUse", "mcp__server__tool", "");
  codex["tool_input"] = json!({"path": "notes.md"});
  for event in [&claude, &plugin] {
    assert!(denied(&hook(&p, event)), "{event}");
  }
  assert!(denied(&hook(&p, &codex)));
  hook(&p, &claude_bash("PostToolUse", "hivex search topic"));
  hook(&p, &codex_tool("PostToolUse", "Bash", "hivex search topic"));
  for event in [&claude, &plugin, &codex] {
    assert_eq!(hook(&p, event), Value::Null, "{event}");
  }
}

#[test]
fn markers_are_empty_and_named_only_by_hashes() {
  let p = Project::new();
  hook(
    &p,
    &claude_bash("PostToolUse", "hivex search \"secret topic\""),
  );
  let markers: Vec<_> = fs::read_dir(p.path("tmp/hivex-hook"))
    .unwrap()
    .map(|entry| entry.unwrap())
    .collect();
  assert_eq!(markers.len(), 1);
  let name = markers[0].file_name().into_string().unwrap();
  let (session, agent) = name.split_once('-').unwrap();
  for hash in [session, agent] {
    assert_eq!(hash.len(), 32, "{name}");
    assert!(hash.bytes().all(|byte| byte.is_ascii_hexdigit()), "{name}");
  }
  assert_eq!(fs::read(markers[0].path()).unwrap().len(), 0);
  // Expiry removes old markers only, never another file in the directory.
  let month = SystemTime::now() - Duration::from_secs(30 * 24 * 60 * 60);
  p.write("tmp/hivex-hook/notes.txt", "kept");
  for path in [markers[0].path(), p.path("tmp/hivex-hook/notes.txt")] {
    fs::File::options()
      .append(true)
      .open(path)
      .unwrap()
      .set_modified(month)
      .unwrap();
  }
  hook(&p, &claude_start("startup"));
  assert!(!markers[0].path().exists());
  assert!(p.path("tmp/hivex-hook/notes.txt").exists());
}

#[test]
fn an_input_it_does_not_recognise_lets_the_call_through_with_a_warning() {
  let p = Project::new();
  let mut without_session = claude_write();
  without_session
    .as_object_mut()
    .unwrap()
    .remove("session_id");
  let mut without_tool = claude_write();
  without_tool.as_object_mut().unwrap().remove("tool_name");
  let mut without_command = claude_bash("PreToolUse", "git commit");
  without_command["tool_input"] = json!({"script": "git commit"});
  let mut unknown_event = claude_write();
  unknown_event["hook_event_name"] = json!("PreToolCall");
  let mut without_input = claude_write();
  without_input.as_object_mut().unwrap().remove("tool_input");
  let unknown_tool = claude_tool("TaskCreate", &json!({"subject": "x"}));
  let mut unknown_after = unknown_tool.clone();
  unknown_after["hook_event_name"] = json!("PostToolUse");
  let partial_write = claude_tool("Write", &json!({"file_path": "/path/to/file.txt"}));
  let malformed_mcp = claude_tool("mcp__memory__create_entities", &json!("entities"));
  let incomplete_mcp = claude_tool("mcp__files", &json!({"path": "notes.md"}));
  let mut oversize = claude_write();
  oversize["tool_input"]["content"] = json!("x".repeat(9 * 1024 * 1024));
  for event in [
    json!("not an object"),
    without_session,
    without_tool,
    without_command,
    unknown_event,
    without_input,
    unknown_tool,
    unknown_after,
    partial_write,
    malformed_mcp,
    incomplete_mcp,
    oversize,
  ] {
    assert!(warned(&hook(&p, &event)), "{event}");
  }
  // Text that is not JSON at all.
  let output = command(&p, &[]).stdin(Stdio::null()).output().unwrap();
  assert!(output.status.success());
  assert!(warned(&serde_json::from_slice(&output.stdout).unwrap()));
  // A marker directory the hook cannot use.
  p.write("tmp/hivex-hook", "not a directory");
  assert!(warned(&hook(&p, &claude_write())));
  // Each with its reason in the record.
  subset(
    &failures(&p),
    &json!({"total": 14, "reasons": {
      "not-a-hook-event": 2, "no-session-id": 1, "no-tool-name": 1, "no-shell-command": 1,
      "unknown-event": 1, "missing-tool-fields": 3, "unknown-tool": 3, "input-too-large": 1,
      "marker-storage-unusable": 1
    }}),
  );
}

/// Claude Code, "`PreToolUse` decision control", "`PostToolUse` decision control" and "Add context
/// for Claude", and Codex, "`PreToolUse`", "`PostToolUse`" and "`UserPromptSubmit`":
/// `additionalContext` reaches the model without blocking the call. The event is read on its own,
/// so a malformed field or an input past the limit still names it.
#[test]
fn a_failure_on_an_event_with_a_channel_warns_the_agent_too() {
  let p = Project::new();
  let unknown_tool = claude_tool("TaskCreate", &json!({"subject": "x"}));
  let mut unknown_after = unknown_tool.clone();
  unknown_after["hook_event_name"] = json!("PostToolUse");
  let codex_unknown = codex_tool("PreToolUse", "unknown_tool", "x");
  let mut numbered_session = claude_write();
  numbered_session["session_id"] = json!(42);
  let mut oversize_after = claude_bash("PostToolUse", "hivex read docs/guide.md");
  oversize_after["tool_response"] = json!({"stdout": "x".repeat(9 * 1024 * 1024)});
  for (event, name) in [
    (unknown_tool, "PreToolUse"),
    (unknown_after, "PostToolUse"),
    (codex_unknown, "PreToolUse"),
    (numbered_session, "PreToolUse"),
    (oversize_after, "PostToolUse"),
    (claude_prompt("Fix the build"), "UserPromptSubmit"),
  ] {
    let answer = hook(&p, &event);
    assert!(warned(&answer), "{answer}");
    assert_eq!(answer["hookSpecificOutput"]["hookEventName"], name);
  }
  // Claude Code alone documents the channel for `PostToolUseFailure`, so it gets the warning only
  // when Claude Code runs the hook.
  let mut failed = claude_bash("PostToolUseFailure", "npm test");
  failed["error"] = json!("Command exited with non-zero status code 1");
  let answer = claude_hook(&p, &failed);
  assert!(warned(&answer), "{answer}");
  assert_eq!(
    answer["hookSpecificOutput"]["hookEventName"],
    "PostToolUseFailure"
  );
  // An input that names no event the hook answers tells only the person, as do `Stop`, where
  // Claude Code's context would continue the turn, and an event only another host gives a channel.
  let mut unknown_event = claude_write();
  unknown_event["hook_event_name"] = json!("PreToolCall");
  let mut stop = claude_prompt("x");
  stop["hook_event_name"] = json!("Stop");
  stop["stop_hook_active"] = json!(false);
  for answer in [
    hook(&p, &json!("not an object")),
    hook(&p, &unknown_event),
    claude_hook(&p, &stop),
    hook(&p, &failed),
  ] {
    assert!(
      warned(&answer) && answer.get("hookSpecificOutput").is_none(),
      "{answer}"
    );
  }
}

/// Claude Code and Codex, "SessionStart": the reminder still reaches a session whose compaction the
/// hook could not record, followed by the warning.
#[test]
fn a_session_that_starts_with_a_failure_still_gets_the_rule() {
  let p = Project::new();
  p.write("tmp/hivex-hook", "not a directory");
  let answer = hook(&p, &claude_start("compact"));
  assert!(warned(&answer), "{answer}");
  let context = answer["hookSpecificOutput"]["additionalContext"]
    .as_str()
    .unwrap();
  assert!(context.starts_with("This project uses HiveX."), "{context}");
}

#[test]
fn each_failure_is_recorded_without_the_inputs_content() {
  let p = Project::new();
  assert_eq!(failures(&p)["total"], 0);
  let mut without_session = claude_bash("PreToolUse", "git commit -m 'Rotate the signing key'");
  without_session
    .as_object_mut()
    .unwrap()
    .remove("session_id");
  claude_hook(
    &p,
    &claude_tool("Write", &json!({"file_path": "/private/notes/plan.md"})),
  );
  claude_hook(
    &p,
    &claude_tool("TaskCreate", &json!({"subject": "Rotate the signing key"})),
  );
  hook(&p, &without_session);
  hook(
    &p,
    &codex_tool("PreToolUse", "unknown_tool", "cat notes.md"),
  );
  hook(&p, &json!("not an object"));
  let mut renamed = claude_write();
  renamed["hook_event_name"] = json!("RotateTheSigningKey");
  hook(&p, &renamed);
  hook(
    &p,
    &claude_prompt("Rotate the signing key in /private/notes"),
  );
  let report = failures(&p);
  subset(
    &report,
    &json!({"command": "hook", "total": 7, "reasons": {
      "missing-tool-fields": 1, "unknown-tool": 2, "no-session-id": 1, "not-a-hook-event": 1,
      "unknown-event": 2
    }}),
  );
  // Newest first, each with the host its event shows and only an event the hook knows.
  let recent = report["recent"].as_array().unwrap();
  assert_eq!(recent.len(), 7);
  for (entry, expected) in recent.iter().zip([
    json!({"host": "unknown", "event": "UserPromptSubmit", "reason": "unknown-event"}),
    json!({"host": "unknown", "event": null, "reason": "unknown-event"}),
    json!({"host": "unknown", "event": null, "reason": "not-a-hook-event", "session": null}),
    json!({"host": "codex", "event": "PreToolUse", "reason": "unknown-tool"}),
    json!({"host": "unknown", "event": "PreToolUse", "reason": "no-session-id", "session": null}),
    json!({"host": "claude", "event": "PreToolUse", "reason": "unknown-tool"}),
    json!({"host": "claude", "event": "PreToolUse", "reason": "missing-tool-fields"}),
  ]) {
    subset(entry, &expected);
    let mut fields: Vec<&str> = entry
      .as_object()
      .unwrap()
      .keys()
      .map(String::as_str)
      .collect();
    fields.sort_unstable();
    assert_eq!(fields, ["event", "host", "reason", "session", "time"]);
    let time = entry["time"].as_str().unwrap();
    assert!(
      time.len() == 20 && time.ends_with('Z') && time.as_bytes()[10] == b'T',
      "{time}"
    );
    assert!(
      entry["session"]
        .as_str()
        .is_none_or(|hash| hash.len() == 32 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
    );
  }
  let log = fs::read_to_string(report["log"].as_str().unwrap()).unwrap();
  for content in [
    "/private/notes/plan.md",
    "Rotate the signing key",
    "RotateTheSigningKey",
    "/private/notes",
    "TaskCreate",
    "unknown_tool",
    "notes.md",
    "\"abc123\"",
    "thr_123",
    "/workspace",
  ] {
    assert!(!log.contains(content), "{content}: {log}");
  }
}

#[test]
fn a_record_it_cannot_write_never_stops_the_hook() {
  let p = Project::new();
  let log = PathBuf::from(failures(&p)["log"].as_str().unwrap());
  let directory = log.parent().unwrap();
  fs::create_dir_all(directory.parent().unwrap()).unwrap();
  fs::write(directory, "not a directory").unwrap();
  // `hook` asserts that it exits with success and nothing on stderr.
  assert!(warned(&hook(
    &p,
    &claude_tool("TaskCreate", &json!({"subject": "x"}))
  )));
  assert!(denied(&hook(&p, &claude_write())));
  assert_eq!(failures(&p)["total"], 0);
}

#[test]
fn the_record_keeps_at_most_two_files_of_64_kib() {
  let p = Project::new();
  let log = PathBuf::from(failures(&p)["log"].as_str().unwrap());
  let previous = log.with_file_name("hook-failures.1.jsonl");
  fs::create_dir_all(log.parent().unwrap()).unwrap();
  let mut kept = 0;
  for reason in ["unknown-tool", "unknown-event"] {
    let entry = format!(
      "{{\"time\":\"2026-11-26T00:00:00Z\",\"host\":\"unknown\",\"event\":\"PreToolUse\",\"reason\":\"{reason}\",\"session\":null}}\n"
    );
    let full = entry.repeat(64 * 1024 / entry.len());
    kept = full.lines().count();
    fs::write(&log, &full).unwrap();
    hook(&p, &json!("not an object"));
    assert_eq!(fs::read_to_string(&previous).unwrap(), full);
    assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), 1);
    assert_eq!(fs::read_dir(log.parent().unwrap()).unwrap().count(), 2);
  }
  subset(
    &failures(&p),
    &json!({"total": kept + 1, "reasons": {"unknown-event": kept, "not-a-hook-event": 1}}),
  );
}

/// A marker directory another user of a shared temporary directory could write to, or a link to
/// someone else's directory, is never used.
#[cfg(unix)]
#[test]
fn a_marker_directory_others_can_write_is_not_used() {
  use std::os::unix::fs::PermissionsExt;
  let p = Project::new();
  fs::create_dir_all(p.path("tmp/hivex-hook")).unwrap();
  fs::set_permissions(p.path("tmp/hivex-hook"), fs::Permissions::from_mode(0o777)).unwrap();
  assert!(warned(&hook(&p, &claude_write())));
  let p = Project::new();
  fs::create_dir_all(p.path("tmp")).unwrap();
  p.write("elsewhere/notes.txt", "kept");
  std::os::unix::fs::symlink(p.path("elsewhere"), p.path("tmp/hivex-hook")).unwrap();
  assert!(warned(&hook(&p, &claude_write())));
  assert!(warned(&hook(
    &p,
    &claude_bash("PostToolUse", "hivex search topic")
  )));
  assert_eq!(fs::read_dir(p.path("elsewhere")).unwrap().count(), 1);
  // A compaction that cannot remove the recovery leaves no silent authorization behind.
  let p = Project::new();
  hook(&p, &claude_bash("PostToolUse", "hivex search topic"));
  fs::set_permissions(p.path("tmp/hivex-hook"), fs::Permissions::from_mode(0o500)).unwrap();
  // Claude Code, "PostCompact": it discards `systemMessage` and has no context for the agent, so
  // the record is where that failure shows.
  let answer = hook(&p, &claude_post_compact());
  assert!(warned(&answer) && answer.get("hookSpecificOutput").is_none());
  subset(
    &failures(&p)["recent"][0],
    &json!({"event": "PostCompact", "reason": "compaction-record-kept"}),
  );
  assert!(warned(&hook(&p, &claude_write())));
  fs::set_permissions(p.path("tmp/hivex-hook"), fs::Permissions::from_mode(0o700)).unwrap();
}

/// Codex, "Common input fields" and "`PreToolUse`": `apply_patch` and Bash carry `command`.
fn codex_tool(event: &str, tool: &str, command: &str) -> Value {
  json!({
    "session_id": "thr_123",
    "transcript_path": "/workspace/.codex/rollout.jsonl",
    "cwd": "/workspace",
    "hook_event_name": event,
    "model": "gpt-5",
    "turn_id": "turn_1",
    "permission_mode": "default",
    "tool_name": tool,
    "tool_use_id": "call_1",
    "tool_input": {"command": command}
  })
}

#[test]
fn codex_sessions_follow_the_same_rule() {
  let p = Project::new();
  let patch = "*** Begin Patch\n*** Add File: hello.txt\n+hello\n*** End Patch\n";
  // Codex, "SessionStart": `source` says how the session started.
  let start = json!({
    "session_id": "thr_123",
    "transcript_path": "/workspace/.codex/rollout.jsonl",
    "cwd": "/workspace",
    "hook_event_name": "SessionStart",
    "model": "gpt-5",
    "permission_mode": "default",
    "source": "startup"
  });
  subset(
    &hook(&p, &start),
    &json!({"hookSpecificOutput": {"hookEventName": "SessionStart"}}),
  );
  assert!(denied(&hook(
    &p,
    &codex_tool("PreToolUse", "apply_patch", patch)
  )));
  assert!(denied(&hook(
    &p,
    &codex_tool("PreToolUse", "spawn_agent", "")
  )));
  assert_eq!(
    hook(&p, &codex_tool("PreToolUse", "Bash", "cargo test")),
    Value::Null
  );
  hook(&p, &codex_tool("PostToolUse", "Bash", "hivex search topic"));
  assert_eq!(
    hook(&p, &codex_tool("PreToolUse", "apply_patch", patch)),
    Value::Null
  );
  // Codex, "SessionEnd": its documented input.
  hook(
    &p,
    &json!({
      "session_id": "thr_123",
      "transcript_path": "/workspace/.codex/rollout.jsonl",
      "cwd": "/workspace",
      "hook_event_name": "SessionEnd",
      "reason": "other"
    }),
  );
  assert!(denied(&hook(
    &p,
    &codex_tool("PreToolUse", "apply_patch", patch)
  )));
}

#[test]
fn each_answer_takes_well_under_100_ms() {
  let p = Project::new();
  hook(&p, &claude_bash("PostToolUse", "hivex search topic"));
  let mut times: Vec<Duration> = (0..15)
    .map(|_| {
      let started = Instant::now();
      hook(&p, &claude_write());
      started.elapsed()
    })
    .collect();
  // A shell call whose output the host sends along with the command.
  let mut completion = claude_bash("PostToolUse", "cargo test");
  completion["tool_response"] = json!({"stdout": "ok\n".repeat(128 * 1024), "stderr": ""});
  times.extend((0..15).map(|_| {
    let started = Instant::now();
    hook(&p, &completion);
    started.elapsed()
  }));
  times.sort();
  // The median includes starting the process, as the host does on every call.
  assert!(
    times[times.len() / 2] < Duration::from_millis(100),
    "{times:?}"
  );
}

#[test]
fn init_and_update_recommend_registering_the_hook_without_changing_settings() {
  let p = Project::new();
  // `init` writes HiveX's skills into `.claude`, so Claude Code is a host from the start.
  subset(
    &p.ok(&["init"]),
    &json!({"hooks": [{
      "host": "claude",
      "status": "recommended",
      "file": ".claude/settings.json",
      "documentation": "https://code.claude.com/docs/en/hooks",
      "missing": ["SessionStart", "SubagentStart", "PreToolUse", "PostToolUse", "PostCompact", "SessionEnd"]
    }]}),
  );
  p.write(".claude/settings.json", "{\"outputStyle\": \"Concise\"}\n");
  // Codex settings in `config.toml` are not parsed, so a kit there is unverified.
  p.write(
    ".codex/config.toml",
    "[[hooks.PreToolUse.hooks]]\ncommand = \"hivex hook\"\n",
  );
  let update = p.ok(&["update"]);
  let codex = &update["hooks"][1];
  subset(
    codex,
    &json!({
      "host": "codex",
      "status": "unverified",
      "unread": [".codex/config.toml"],
      "file": ".codex/hooks.json",
      "documentation": "https://learn.chatgpt.com/docs/hooks"
    }),
  );
  assert!(
    codex["note"]
      .as_str()
      .unwrap()
      .contains("Verified against Codex's documentation only")
  );
  // It is completed in `config.toml`, not registered a second time.
  assert!(
    codex["steps"]
      .as_str()
      .unwrap()
      .contains("rather than adding a second registration")
  );
  assert!(
    update["hooks"][0]["steps"]
      .as_str()
      .unwrap()
      .contains("Claude Code 2.1.295")
  );
  assert_eq!(
    fs::read_to_string(p.path(".claude/settings.json")).unwrap(),
    "{\"outputStyle\": \"Concise\"}\n"
  );
  // The recommended configuration, merged as the steps say, registers the hook.
  let mut settings = json!({"outputStyle": "Concise"});
  settings["hooks"] = update["hooks"][0]["configuration"]["hooks"].clone();
  p.json(".claude/settings.json", &settings);
  p.json(".codex/hooks.json", &codex["configuration"]);
  let registered = p.ok(&["update"]);
  subset(
    &registered,
    &json!({"hooks": [
      {"host": "claude", "status": "registered"},
      {"host": "codex", "status": "registered", "documentation": "https://learn.chatgpt.com/docs/hooks"}
    ]}),
  );
  assert_eq!(registered["hooks"][1]["note"], codex["note"]);
  // Every state says how to gate the project's MCP writing tools.
  for report in [
    &update["hooks"][0],
    codex,
    &registered["hooks"][0],
    &registered["hooks"][1],
  ] {
    assert!(
      report["mcp"]
        .as_str()
        .unwrap()
        .contains("mcp__<server>__<tool>"),
      "{report}"
    );
  }
}

#[test]
fn only_a_registration_that_applies_the_rule_counts() {
  let p = Project::new();
  p.ok(&["init"]);
  let settings = p.ok(&["update"])["hooks"][0]["configuration"].clone();
  // A registration that cannot record recovery would deny every edit: without `PostToolUse`, with
  // a matcher that leaves out Bash, or with a handler that is not run on every call before it.
  let mut without = settings.clone();
  without["hooks"]
    .as_object_mut()
    .unwrap()
    .remove("PostToolUse");
  let mut narrow = settings.clone();
  narrow["hooks"]["PostToolUse"][0]["matcher"] = json!("PowerShell");
  let mut other_type = settings.clone();
  other_type["hooks"]["PostToolUse"][0]["hooks"][0]["type"] = json!("http");
  let mut conditional = settings.clone();
  conditional["hooks"]["PostToolUse"][0]["hooks"][0]["if"] = json!("Bash(git *)");
  let mut background = settings.clone();
  background["hooks"]["PostToolUse"][0]["hooks"][0]["async"] = json!(true);
  // PowerShell's call operator is not Bash.
  let mut wrong_shell = settings.clone();
  wrong_shell["hooks"]["PostToolUse"][0]["hooks"][0] =
    json!({"type": "command", "command": "& \"C:\\HiveX\\hivex.exe\" hook", "shell": "bash"});
  // PowerShell runs a quoted path only after `&`.
  let mut uncalled = settings.clone();
  uncalled["hooks"]["PostToolUse"][0]["hooks"][0] = json!({"type": "command", "command": "\"C:\\Program Files\\HiveX\\hivex.exe\" hook", "shell": "powershell"});
  for broken in [
    without,
    narrow,
    other_type,
    conditional,
    background,
    wrong_shell,
    uncalled,
  ] {
    p.json(".claude/settings.json", &broken);
    subset(
      &p.ok(&["update"])["hooks"][0],
      &json!({"status": "recommended", "missing": ["PostToolUse"]}),
    );
  }
  // A gate that may block the call when it fails is not the kit's.
  for failure in [json!("block"), Value::Null] {
    let mut closed = settings.clone();
    closed["hooks"]["PreToolUse"][0]["hooks"][0]["onFailure"] = failure;
    p.json(".claude/settings.json", &closed);
    subset(
      &p.ok(&["update"])["hooks"][0],
      &json!({"status": "recommended", "missing": ["PreToolUse"]}),
    );
  }
  // Claude Code's exec form registers too, and so does a quoted path to the executable.
  for handler in [
    json!({"type": "command", "command": "hivex", "args": ["hook"], "timeout": 10}),
    // Without `shell`, Windows may read a handler with PowerShell, which needs `&` here.
    json!({"type": "command", "command": "\"/opt/my tools/hivex\" hook", "shell": "bash"}),
    json!({"type": "command", "command": "\"C:\\Program Files\\hivex\\hivex.exe\" hook", "shell": "bash"}),
    json!({"type": "command", "command": "& \"C:\\HiveX\\hivex.exe\" hook", "shell": "powershell"}),
  ] {
    let mut exec = settings.clone();
    exec["hooks"]["PostToolUse"][0]["hooks"][0] = handler;
    p.json(".claude/settings.json", &exec);
    subset(
      &p.ok(&["update"])["hooks"][0],
      &json!({"status": "registered"}),
    );
  }
  // Claude Code reads a matcher of plain names, hyphens and commas as a list of exact names, so
  // one that leaves out `NotebookEdit` does not cover it however its other names are written.
  let mut omitted = settings.clone();
  omitted["hooks"]["PreToolUse"][0]["matcher"] =
    json!("Edit|Write|Agent|Workflow|Bash|PowerShell|mcp__plugin_my-plugin_db__query");
  p.json(".claude/settings.json", &omitted);
  subset(
    &p.ok(&["update"])["hooks"][0],
    &json!({"status": "recommended", "missing": ["PreToolUse"]}),
  );
  let mut commas = settings.clone();
  commas["hooks"]["PreToolUse"][0]["matcher"] =
    json!("Edit, Write, NotebookEdit, Agent, Workflow, Bash, PowerShell");
  p.json(".claude/settings.json", &commas);
  subset(
    &p.ok(&["update"])["hooks"][0],
    &json!({"status": "registered"}),
  );
  // A gate that also lists a project's MCP writing tools still registers.
  let mut mcp = settings.clone();
  mcp["hooks"]["PreToolUse"][0]["matcher"] = json!(
    "Edit|Write|NotebookEdit|Agent|Workflow|Bash|PowerShell|mcp__files__write_file|mcp__plugin_my-plugin_db__query"
  );
  p.json(".claude/settings.json", &mcp);
  subset(
    &p.ok(&["update"])["hooks"][0],
    &json!({"status": "registered"}),
  );
  // A matcher that covers every value also registers, here in the local settings file.
  p.json(".claude/settings.json", &json!({}));
  let mut local = settings.clone();
  local["hooks"]["PreToolUse"][0]["matcher"] = json!("*");
  p.json(".claude/settings.local.json", &local);
  subset(
    &p.ok(&["update"])["hooks"][0],
    &json!({"status": "registered"}),
  );
}
