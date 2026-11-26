//! Whether each host the project has a directory for registers `hivex hook`, and the configuration
//! that registers it when it does not. It only reads the host's project settings: HiveX never
//! writes them.

use super::shell;
use regex::Regex;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

/// One host's project settings and the hooks that apply the recovery rule there.
struct Host {
  name: &'static str,
  directory: &'static str,
  /// JSON settings files that can register the hook; the first is the one to edit.
  files: &'static [&'static str],
  /// Other settings files the host reads, which HiveX does not parse.
  unread: &'static [&'static str],
  /// For each event, the matcher values its hook must cover, each as its aliases.
  events: &'static [(&'static str, &'static [&'static [&'static str]])],
  /// The tools `PreToolUse` must see: edits, agent launches and shell commands.
  gated: &'static str,
  shell: &'static str,
  /// How the host treats a failed `PreToolUse` hook, when it lets that be configured: the call
  /// must go ahead.
  on_failure: Option<&'static str>,
  steps: &'static str,
  /// What to do when an unparsed settings file names the hook.
  unverified_steps: &'static str,
  documentation: &'static str,
  note: Option<&'static str>,
}

const STARTS: &[&[&str]] = &[&["startup"], &["resume"], &["clear"], &["compact"]];
const COMPACTIONS: &[&[&str]] = &[&["manual"], &["auto"]];

const HOSTS: &[Host] = &[
  Host {
    name: "claude",
    directory: ".claude",
    files: &[".claude/settings.json", ".claude/settings.local.json"],
    unread: &[],
    events: &[
      ("SessionStart", STARTS),
      ("SubagentStart", &[]),
      (
        "PreToolUse",
        &[
          &["Edit"],
          &["Write"],
          &["NotebookEdit"],
          &["Agent"],
          &["Workflow"],
          &["Bash"],
          &["PowerShell"],
        ],
      ),
      ("PostToolUse", &[&["Bash"], &["PowerShell"]]),
      ("PostCompact", COMPACTIONS),
      ("SessionEnd", &[]),
    ],
    gated: "Edit|Write|NotebookEdit|Agent|Workflow|Bash|PowerShell",
    shell: "Bash|PowerShell",
    // Without Git Bash, Claude Code otherwise blocks the call when this hook fails.
    on_failure: Some("continue"),
    steps: "Install a HiveX release that has the hook command on PATH first, and use Claude Code 2.1.295 or later, which reads onFailure. Then merge configuration into .claude/settings.json, keeping its other settings and hooks. Claude Code runs project hooks once the workspace is trusted.",
    unverified_steps: "",
    documentation: "https://code.claude.com/docs/en/hooks",
    note: None,
  },
  Host {
    name: "codex",
    directory: ".codex",
    files: &[".codex/hooks.json"],
    unread: &[".codex/config.toml"],
    events: &[
      ("SessionStart", STARTS),
      ("SubagentStart", &[]),
      (
        "PreToolUse",
        &[
          &["Bash"],
          &["apply_patch", "Edit", "Write"],
          &["spawn_agent", "Agent"],
        ],
      ),
      ("PostToolUse", &[&["Bash"]]),
      ("PostCompact", COMPACTIONS),
      ("SessionEnd", &[]),
    ],
    gated: "^(Bash|apply_patch|spawn_agent)$",
    shell: "^Bash$",
    on_failure: None,
    steps: "Install a HiveX release that has the hook command on PATH first. Then merge configuration into .codex/hooks.json and review and trust the new hooks in /hooks; Codex skips hooks it has not trusted. HiveX does not parse config.toml.",
    unverified_steps: "HiveX does not parse config.toml, which names hivex hook. Check that its registration covers every event and matcher in configuration and complete it there, rather than adding a second registration to .codex/hooks.json that Codex would run as well.",
    documentation: "https://learn.chatgpt.com/docs/hooks",
    note: Some(
      "Verified against Codex's documentation only, not in a Codex session. Codex documents no agent id for tool events, so a subagent shares its parent session's recovery.",
    ),
  },
];

/// For each host whose directory the project at `root` has, `registered` when its settings run
/// `hivex hook` on every event and matcher value of the kit, or `recommended` with the missing
/// events, the configuration to merge and the steps; `unverified` instead when a settings file
/// HiveX does not parse names the hook.
pub fn registrations(root: &Path) -> Value {
  HOSTS
    .iter()
    .filter(|host| root.join(host.directory).is_dir())
    .map(|host| {
      let settings: Vec<Value> = host
        .files
        .iter()
        .filter_map(|file| fs::read(root.join(file)).ok())
        .filter_map(|bytes| serde_json::from_slice(&bytes).ok())
        .collect();
      let missing: Vec<&str> = host
        .events
        .iter()
        .filter(|(event, values)| {
          let failure = host.on_failure.filter(|_| *event == "PreToolUse");
          !registered(&settings, event, values, failure)
        })
        .map(|(event, _)| *event)
        .collect();
      let mut report = json!({"documentation": host.documentation, "host": host.name});
      if let Some(note) = host.note {
        report["note"] = json!(note);
      }
      if missing.is_empty() {
        report["status"] = json!("registered");
        return report;
      }
      let unread: Vec<&str> = host
        .unread
        .iter()
        .copied()
        .filter(|file| {
          fs::read_to_string(root.join(file)).is_ok_and(|text| text.contains("hivex hook"))
        })
        .collect();
      report["configuration"] = configuration(host);
      report["file"] = json!(host.files[0]);
      report["missing"] = json!(missing);
      report["status"] = json!(if unread.is_empty() {
        "recommended"
      } else {
        "unverified"
      });
      report["steps"] = json!(host.steps);
      if !unread.is_empty() {
        report["steps"] = json!(host.unverified_steps);
        report["unread"] = json!(unread);
      }
      report
    })
    .map(|mut report| {
      report.as_object_mut().map(serde_json::Map::sort_keys);
      report
    })
    .collect()
}

fn configuration(host: &Host) -> Value {
  let handler = json!([{"type": "command", "command": "hivex hook", "timeout": 10}]);
  let all = json!([{"hooks": handler}]);
  let mut gate = handler[0].clone();
  if let Some(on_failure) = host.on_failure {
    gate["onFailure"] = json!(on_failure);
  }
  json!({"hooks": {
    "SessionStart": all,
    "SubagentStart": all,
    "PreToolUse": [{"matcher": host.gated, "hooks": [gate]}],
    "PostToolUse": [{"matcher": host.shell, "hooks": handler}],
    "PostCompact": all,
    // Both hosts give `SessionEnd` a short budget, so it keeps the host's default timeout.
    "SessionEnd": [{"hooks": [{"type": "command", "command": "hivex hook"}]}],
  }})
}

/// Whether the matcher groups of `event` that run the hook cover every value in `values`. A handler
/// counts when it runs unconditionally and lets the call go ahead if it fails, with the host's
/// `failure` setting when the kit needs one.
fn registered(settings: &[Value], event: &str, values: &[&[&str]], failure: Option<&str>) -> bool {
  let groups: Vec<&Value> = settings
    .iter()
    .flat_map(|file| file["hooks"][event].as_array().into_iter().flatten())
    .filter(|group| {
      group["hooks"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|handler| {
          // A background hook cannot deny the call, and an `if` condition narrows it.
          handler["type"] == "command"
            && runs_hook(handler)
            && handler["async"] != true
            && handler["asyncRewake"] != true
            && handler.get("if").is_none()
            && handler["onFailure"] != "block"
            && failure.is_none_or(|failure| handler["onFailure"] == failure)
        })
    })
    .collect();
  !groups.is_empty()
    && values.iter().all(|aliases| {
      groups.iter().any(|group| {
        aliases
          .iter()
          .any(|value| covers(group["matcher"].as_str(), value))
      })
    })
}

/// Both hosts match every value with an omitted, empty or `*` matcher, and test any other matcher
/// as a regular expression; Claude Code compares a list of plain names exactly.
fn covers(matcher: Option<&str>, value: &str) -> bool {
  match matcher.unwrap_or_default() {
    "" | "*" => true,
    names
      if names
        .chars()
        .all(|c| c == '_' || c == '|' || c.is_ascii_alphanumeric()) =>
    {
      names.split('|').any(|name| name == value)
    }
    pattern => Regex::new(pattern).is_ok_and(|pattern| pattern.is_match(value)),
  }
}

/// A handler that runs `hivex hook`, also through a path to the executable, in Claude Code's exec
/// form (`command` with `args`) or in shell form, read by the handler's `shell`. Without one, Claude
/// Code uses Bash, and on Windows PowerShell when Git Bash is missing.
fn runs_hook(handler: &Value) -> bool {
  let command = handler["command"].as_str().unwrap_or_default();
  if let Some(arguments) = handler.get("args") {
    return *arguments == json!(["hook"]) && shell::program(command) == "hivex";
  }
  let bash = shell::runs_hook(command, true);
  let powershell = shell::runs_hook(command, false);
  match handler.get("shell").map(Value::as_str) {
    None => bash || (cfg!(windows) && powershell),
    Some(Some("bash")) => bash,
    Some(Some("powershell")) => powershell,
    Some(_) => false,
  }
}
