use crate::support::{Project, bounded, list, subset};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

#[test]
fn installed_skills_are_self_contained() {
  let p = Project::new();
  p.ok(&["init"]);
  // Copied skills cannot rely on paths that exist only in the Hivex repository.
  p.json("hivex.json", &json!({"include":[".agents/skills/**/*.md"]}));
  subset(
    &p.ok(&["check"]),
    &json!({"status":"ready","totalFindings":0}),
  );
}

#[test]
fn update_restores_managed_entries_without_touching_project_files() {
  let p = Project::new();
  p.ok(&["init"]);
  p.write("AGENTS.md", "# My rules\n");
  p.write(".agents/skills/hivex-design/SKILL.md", "edited by hand\n");
  fs::remove_file(p.path(".agents/skills/hivex-git/SKILL.md")).unwrap();
  fs::remove_file(p.path("docs/PRD.md")).unwrap();
  p.write(".gitignore", "*.tmp\n");
  let result = p.ok(&["update"]);
  subset(
    &result,
    &json!({
      "command":"update",
      "created":[".agents/skills/hivex-git/SKILL.md"],
      "plugins":[
        {"host":"claude","plugin":"ponytail","status":"missing"},
        {"host":"codex","plugin":"ponytail","status":"missing"}
      ],
      "updated":[".agents/skills/hivex-design/SKILL.md", ".gitignore"]
    }),
  );
  assert_eq!(
    fs::read(p.path(".agents/skills/hivex-design/SKILL.md")).unwrap(),
    include_bytes!("../../skills/hivex-design/SKILL.md")
  );
  assert!(!list(&result, "preserved").contains(&json!("AGENTS.md")));
  assert_eq!(
    fs::read_to_string(p.path("AGENTS.md")).unwrap(),
    "# My rules\n"
  );
  assert!(!p.path("docs/PRD.md").exists());
  assert!(
    fs::read_to_string(p.path(".gitignore"))
      .unwrap()
      .starts_with("*.tmp\n/.claude/skills/hivex*\n")
  );
  assert_eq!(
    p.error(&["update", "extra"])["error"]["code"],
    "INVALID_ARGUMENT"
  );
}

// Symbolic links need extra privileges on Windows.
#[cfg(unix)]
#[test]
fn update_replaces_former_npm_links_with_copies() {
  let p = Project::new();
  let package = Project::new();
  package.write("hivex/SKILL.md", "# Skill from npm\n");
  fs::create_dir_all(p.path(".agents/skills")).unwrap();
  std::os::unix::fs::symlink(package.path("hivex"), p.path(".agents/skills/hivex")).unwrap();
  // `init` leaves the former link alone.
  let initialized = p.ok(&["init"]);
  assert!(list(&initialized, "preserved").contains(&json!(".agents/skills/hivex")));
  assert!(
    fs::symlink_metadata(p.path(".agents/skills/hivex"))
      .unwrap()
      .is_symlink()
  );
  let updated = p.ok(&["update"]);
  assert!(list(&updated, "updated").contains(&json!(".agents/skills/hivex")));
  assert!(list(&updated, "created").contains(&json!(".agents/skills/hivex/SKILL.md")));
  assert!(
    fs::symlink_metadata(p.path(".agents/skills/hivex"))
      .unwrap()
      .is_dir()
  );
  assert_eq!(
    fs::read(p.path(".agents/skills/hivex/SKILL.md")).unwrap(),
    include_bytes!("../../skills/hivex/SKILL.md")
  );
  // The package the link pointed to is untouched.
  assert_eq!(
    fs::read_to_string(package.path("hivex/SKILL.md")).unwrap(),
    "# Skill from npm\n"
  );
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[test]
fn the_pinned_release_is_enforced_by_init_and_update_and_reported_elsewhere() {
  let p = Project::new();
  p.ok(&["init"]);
  p.write(
    "hivex.json",
    r#"{"include":["docs/**/*.md"],"version":"0.0.1"}"#,
  );
  assert_eq!(p.error(&["init"])["error"]["code"], "VERSION_MISMATCH");
  subset(
    &p.ok(&["sources"]),
    &json!({"versionWarning":{"installed":VERSION,"pinned":"0.0.1"}}),
  );
  let updated = p.ok(&["update"]);
  assert!(list(&updated, "updated").contains(&json!("hivex.json")));
  assert_eq!(
    p.read_json("hivex.json"),
    json!({"version": VERSION, "include": ["docs/**/*.md"]})
  );
  assert!(p.ok(&["sources"]).get("versionWarning").is_none());
  p.write("hivex.json", r#"{"version":"99.0.0"}"#);
  subset(
    &p.error(&["update"]),
    &json!({"error":{"code":"VERSION_MISMATCH","details":{"installed":VERSION,"pinned":"99.0.0"}}}),
  );
  p.write("hivex.json", r#"{"version":"latest"}"#);
  assert_eq!(p.error(&["update"])["error"]["code"], "INVALID_CONFIG");
}

/// A stand-in host CLI that records its arguments, prints `listing` for `plugin list` and exits
/// with `code`.
fn fake_host(bin: &Path, name: &str, listing: &str, code: u8) {
  let log = bin.join("calls.log");
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    let script = bin.join(name);
    let body = format!(
      "#!/bin/sh\necho \"{name} $*\" >> '{}'\nif [ \"$2\" = list ]; then echo '{listing}'; fi\nexit {code}\n",
      log.display()
    );
    fs::write(&script, body).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
  }
  #[cfg(windows)]
  fs::write(
    bin.join(format!("{name}.cmd")),
    format!(
      "@echo {name} %*>>\"{}\"\r\n@if \"%2\"==\"list\" echo {listing}\r\n@exit /b {code}\r\n",
      log.display()
    ),
  )
  .unwrap();
}

/// Run `command` with the stand-in hosts in `bin` and return its plugin report and their calls.
fn with_hosts(p: &Project, bin: &Project, command: &str) -> (Value, Vec<String>) {
  let _ = fs::remove_file(bin.path("calls.log"));
  let mut process = p.command(&[command]);
  process.env("PATH", &bin.root);
  let output = bounded(process);
  assert!(output.status.success());
  let result: Value = serde_json::from_slice(&output.stdout).unwrap();
  let calls = fs::read_to_string(bin.path("calls.log"))
    .unwrap()
    .lines()
    .map(|line| line.trim().to_owned())
    .collect();
  (result["plugins"].clone(), calls)
}

#[test]
fn init_installs_missing_ponytail_and_update_refreshes_every_host() {
  let p = Project::new();
  let bin = Project::new();
  fake_host(
    &bin.root,
    "claude",
    r#"[{"id":"ponytail@ponytail","enabled":false}]"#,
    0,
  );
  fake_host(&bin.root, "codex", r#"{"installed":[]}"#, 0);
  let current = json!([
    {"host":"claude","plugin":"ponytail","status":"current"},
    {"host":"codex","plugin":"ponytail","status":"current"}
  ]);
  // An installed (here disabled) plugin is left alone; a missing one is installed.
  let (plugins, calls) = with_hosts(&p, &bin, "init");
  assert_eq!(plugins, current);
  assert_eq!(
    calls,
    [
      "claude plugin list --json",
      "codex plugin list --json",
      "codex plugin marketplace add DietrichGebert/ponytail",
      "codex plugin add ponytail@ponytail",
    ]
  );
  let (plugins, calls) = with_hosts(&p, &bin, "update");
  assert_eq!(plugins, current);
  assert_eq!(
    calls,
    [
      "claude plugin marketplace add DietrichGebert/ponytail",
      "claude plugin marketplace update ponytail",
      "claude plugin install ponytail@ponytail",
      "claude plugin update ponytail@ponytail",
      "codex plugin marketplace add DietrichGebert/ponytail",
      "codex plugin marketplace upgrade ponytail",
      "codex plugin add ponytail@ponytail",
    ]
  );
}

#[test]
fn a_failing_host_stops_at_its_first_step() {
  let p = Project::new();
  let bin = Project::new();
  fake_host(&bin.root, "codex", "", 1);
  let failed = json!([
    {"host":"claude","plugin":"ponytail","status":"missing"},
    {"host":"codex","plugin":"ponytail","status":"failed"}
  ]);
  let (plugins, calls) = with_hosts(&p, &bin, "init");
  assert_eq!(plugins, failed);
  assert_eq!(calls, ["codex plugin list --json"]);
  let (plugins, calls) = with_hosts(&p, &bin, "update");
  assert_eq!(plugins, failed);
  assert_eq!(
    calls,
    ["codex plugin marketplace add DietrichGebert/ponytail"]
  );
}

#[test]
fn version_pins_are_read_like_the_rest_of_the_configuration() {
  let other = Project::new();
  let p = Project::new();
  p.ok(&["init"]);
  p.write("hivex.json", "\u{feff}{\"version\":\"0.0.1\"}");
  // A BOM-prefixed pin is read, not rejected.
  assert_eq!(p.error(&["init"])["error"]["code"], "VERSION_MISMATCH");
  // The warning follows the last `--root`, like every other option.
  let root = other.root.to_str().unwrap();
  subset(
    &p.ok(&["sources", "--root", root]),
    &json!({"versionWarning":{"pinned":"0.0.1"}}),
  );
  assert!(
    other
      .ok(&["sources", &format!("--root={}", p.root.display())])
      .get("versionWarning")
      .is_none()
  );
  // Responses with the warning still fit `--max-bytes`.
  for max in (700..1600).step_by(37) {
    let max = max.to_string();
    let output = p.raw(&["sources", "--max-bytes", &max]);
    if output.status.success() {
      assert!(output.stdout.len() - 1 <= max.parse().unwrap(), "{max}");
      let response: Value = serde_json::from_slice(&output.stdout).unwrap();
      assert!(response.get("versionWarning").is_some(), "{max}");
    }
  }
  p.write("hivex.json", r#"{"version":"latest"}"#);
  assert_eq!(p.error(&["sources"])["error"]["code"], "INVALID_CONFIG");
  p.write("hivex.json", "[]");
  assert_eq!(p.error(&["update"])["error"]["code"], "INVALID_CONFIG");
  assert_eq!(fs::read_to_string(p.path("hivex.json")).unwrap(), "[]");
}
