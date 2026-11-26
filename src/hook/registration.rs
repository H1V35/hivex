//! Whether each host the project uses registers `hivex hook`, and the configuration that registers
//! it when it does not. It only reads the host's project settings: HiveX never writes them.

use serde_json::{Value, json};
use std::fs;
use std::path::Path;

/// One host's project settings and the hooks that apply the recovery rule there.
struct Host {
  name: &'static str,
  directory: &'static str,
  /// Settings files that can register the hook; the first is the one to edit.
  files: &'static [&'static str],
  /// The tools `PreToolUse` must see: edits, agent launches and shell commands.
  gated: &'static str,
  /// How the host treats a failed `PreToolUse` hook, when it lets that be configured: the call
  /// must go ahead.
  on_failure: Option<&'static str>,
  shell: &'static str,
  steps: &'static str,
  documentation: &'static str,
}

const HOSTS: &[Host] = &[
  Host {
    name: "claude",
    directory: ".claude",
    files: &[".claude/settings.json", ".claude/settings.local.json"],
    gated: "Edit|Write|NotebookEdit|Agent|Bash|PowerShell",
    // Without Git Bash, Claude Code otherwise blocks the call when this hook fails.
    on_failure: Some("continue"),
    shell: "Bash|PowerShell",
    steps: "Install a HiveX release that has the hook command on PATH first, then merge configuration into .claude/settings.json, keeping its other settings and hooks. Claude Code runs project hooks once the workspace is trusted.",
    documentation: "https://code.claude.com/docs/en/hooks",
  },
  Host {
    name: "codex",
    directory: ".codex",
    files: &[".codex/hooks.json", ".codex/config.toml"],
    gated: "^(Bash|apply_patch|spawn_agent)$",
    on_failure: None,
    shell: "^Bash$",
    steps: "Install a HiveX release that has the hook command on PATH first, then merge configuration into .codex/hooks.json and review and trust the new hooks in /hooks; Codex skips hooks it has not trusted. This configuration is verified against Codex's documentation only, not in a Codex session.",
    documentation: "https://learn.chatgpt.com/docs/hooks",
  },
];

/// The host's directory belongs to the project when it holds more than HiveX's own skill copies.
fn uses(root: &Path, host: &Host) -> bool {
  fs::read_dir(root.join(host.directory)).is_ok_and(|mut entries| {
    entries.any(|entry| entry.is_ok_and(|entry| entry.file_name() != "skills"))
  })
}

/// For each host the project at `root` uses, `registered` when its settings run `hivex hook` on
/// every event of the kit, or `recommended` with the missing events, the configuration to merge
/// and the steps.
pub fn registrations(root: &Path) -> Value {
  HOSTS
    .iter()
    .filter(|host| uses(root, host))
    .map(|host| {
      let configuration = configuration(host);
      let missing: Vec<&String> = configuration["hooks"]
        .as_object()
        .into_iter()
        .flat_map(|events| events.keys())
        .filter(|event| {
          !host
            .files
            .iter()
            .any(|file| registers(&root.join(file), event))
        })
        .collect();
      if missing.is_empty() {
        return json!({"host": host.name, "status": "registered"});
      }
      json!({
        "configuration": configuration,
        "documentation": host.documentation,
        "file": host.files[0],
        "host": host.name,
        "missing": missing,
        "status": "recommended",
        "steps": host.steps,
      })
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

fn registers(path: &Path, event: &str) -> bool {
  let Ok(text) = fs::read_to_string(path) else {
    return false;
  };
  let Ok(settings) = serde_json::from_str::<Value>(&text) else {
    // shortcut: config.toml is not parsed, so `hivex hook` anywhere in it counts for every event;
    // parse it if projects register only some events there.
    return text.contains("hivex hook");
  };
  settings["hooks"][event]
    .as_array()
    .into_iter()
    .flatten()
    .flat_map(|group| group["hooks"].as_array().into_iter().flatten())
    .any(|handler| handler["command"].as_str().is_some_and(runs_hook))
}

/// `hivex hook`, also through a path to the executable.
fn runs_hook(command: &str) -> bool {
  command
    .trim()
    .strip_suffix(" hook")
    .is_some_and(|program| program.ends_with("hivex") || program.ends_with("hivex.exe"))
}
