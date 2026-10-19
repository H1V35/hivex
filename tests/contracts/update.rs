use crate::support::{Project, bounded, list, subset};
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[test]
fn update_restores_managed_entries_without_touching_project_files() {
  let p = Project::new();
  p.ok(&["init"]);
  p.write("AGENTS.md", "# My rules\n");
  fs::remove_file(p.path(".agents/skills/hivex-git")).unwrap();
  fs::remove_file(p.path("docs/PRD.md")).unwrap();
  p.write(".gitignore", "*.tmp\n");
  let result = p.ok(&["update"]);
  subset(
    &result,
    &json!({
      "command":"update",
      "created":[".agents/skills/hivex-git"],
      "plugins":[
        {"host":"claude","plugin":"ponytail","status":"missing"},
        {"host":"codex","plugin":"ponytail","status":"missing"}
      ],
      "updated":[".gitignore"]
    }),
  );
  assert!(
    fs::symlink_metadata(p.path(".agents/skills/hivex-git"))
      .unwrap()
      .is_symlink()
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
      .starts_with("*.tmp\n/.hivex/\n")
  );
  assert_eq!(
    p.error(&["update", "extra"])["error"]["code"],
    "INVALID_ARGUMENT"
  );
}

/// A stand-in host CLI that records its arguments, prints `listing` for `plugin list` and exits
/// with `code`.
fn fake_host(bin: &Project, name: &str, listing: &str, code: u8) {
  let script = bin.path(name);
  let log = bin.path("calls.log");
  let body = format!(
    "#!/bin/sh\necho \"{name} $*\" >> '{}'\nif [ \"$2\" = list ]; then echo '{listing}'; fi\nexit {code}\n",
    log.display()
  );
  fs::write(&script, body).unwrap();
  fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
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
    .map(str::to_owned)
    .collect();
  (result["plugins"].clone(), calls)
}

#[test]
fn init_installs_missing_ponytail_and_update_refreshes_every_host() {
  let p = Project::new();
  let bin = Project::new();
  fake_host(
    &bin,
    "claude",
    r#"[{"id":"ponytail@ponytail","enabled":false}]"#,
    0,
  );
  fake_host(&bin, "codex", r#"{"installed":[]}"#, 0);
  // An installed (here disabled) plugin is left alone; a missing one is installed.
  let (plugins, calls) = with_hosts(&p, &bin, "init");
  assert_eq!(
    plugins,
    json!([
      {"host":"claude","plugin":"ponytail","status":"preserved"},
      {"host":"codex","plugin":"ponytail","status":"installed"}
    ])
  );
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
  assert_eq!(
    plugins,
    json!([
      {"host":"claude","plugin":"ponytail","status":"current"},
      {"host":"codex","plugin":"ponytail","status":"current"}
    ])
  );
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
  fake_host(&bin, "codex", "", 1);
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
