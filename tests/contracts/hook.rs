//! `hivex hook` as Claude Code and Codex run it. Each input is the host's documented example or is
//! built from its documented fields, cited by section: Claude Code's hooks reference
//! (<https://code.claude.com/docs/en/hooks>) and Codex's hooks page
//! (<https://learn.chatgpt.com/docs/hooks>).
use crate::support::{Project, subset};
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::process::Stdio;
use std::time::{Duration, Instant, SystemTime};

/// The hook's answer to `event`, `Null` when it prints nothing. Markers go to the project's own
/// temporary directory, never the user's.
fn hook(p: &Project, event: &Value) -> Value {
  let temporary = p.path("tmp");
  fs::create_dir_all(&temporary).unwrap();
  let mut command = p.command(&[]);
  command.arg("hook");
  for variable in ["TMPDIR", "TMP", "TEMP"] {
    command.env(variable, &temporary);
  }
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

fn warned(answer: &Value) -> bool {
  answer["systemMessage"]
    .as_str()
    .is_some_and(|message| message.contains("the call goes ahead"))
    && answer.get("hookSpecificOutput").is_none()
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
    oversize,
  ] {
    assert!(warned(&hook(&p, &event)), "{event}");
  }
  // Text that is not JSON at all.
  let mut command = p.command(&[]);
  command.arg("hook").stdin(Stdio::null());
  let output = command.output().unwrap();
  assert!(output.status.success());
  assert!(warned(&serde_json::from_slice(&output.stdout).unwrap()));
  // A marker directory the hook cannot use.
  p.write("tmp/hivex-hook", "not a directory");
  assert!(warned(&hook(&p, &claude_write())));
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
  assert!(warned(&hook(&p, &claude_post_compact())));
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
  for broken in [without, narrow, other_type, conditional, background] {
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
    json!({"type": "command", "command": "\"/opt/my tools/hivex\" hook", "timeout": 10}),
    json!({"type": "command", "command": "\"C:\\Program Files\\hivex\\hivex.exe\" hook"}),
  ] {
    let mut exec = settings.clone();
    exec["hooks"]["PostToolUse"][0]["hooks"][0] = handler;
    p.json(".claude/settings.json", &exec);
    subset(
      &p.ok(&["update"])["hooks"][0],
      &json!({"status": "registered"}),
    );
  }
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
