use crate::support::{Project, bounded, list, skills_section, subset};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

#[test]
fn installed_skills_are_self_contained() {
  let p = Project::new();
  p.ok(&["init"]);
  // Copied skills cannot rely on paths that exist only in the HiveX repository.
  p.json("hivex.json", &json!({"include":[".agents/skills/**/*.md"]}));
  subset(
    &p.ok(&["check"]),
    &json!({"status":"ready","totalFindings":0}),
  );
}

#[test]
fn update_restores_managed_entries_and_only_the_skills_section_of_project_files() {
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
      "updated":[".agents/skills/hivex-design/SKILL.md", ".gitignore", "AGENTS.md"]
    }),
  );
  assert_eq!(
    fs::read(p.path(".agents/skills/hivex-design/SKILL.md")).unwrap(),
    include_bytes!("../../skills/hivex-design/SKILL.md")
  );
  assert_eq!(
    fs::read_to_string(p.path("AGENTS.md")).unwrap(),
    format!("# My rules\n\n{}\n", skills_section("\n"))
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

const SKILLS: [&str; 6] = [
  "hivex",
  "hivex-design",
  "hivex-document",
  "hivex-git",
  "hivex-implement",
  "hivex-review",
];

#[test]
fn new_clones_and_worktrees_carry_the_skills_for_claude_code() {
  let p = Project::new();
  p.git(&["init", "-q"]);
  p.ok(&["init"]);
  p.git(&["add", "-A"]);
  p.git(&["commit", "-q", "-m", "init"]);
  let clone = Project::new();
  let worktree = Project::new();
  p.git(&["clone", "-q", ".", clone.root.to_str().unwrap()]);
  p.git(&[
    "worktree",
    "add",
    "-q",
    "--detach",
    worktree.root.to_str().unwrap(),
  ]);
  for checkout in [&clone, &worktree] {
    // Claude Code loads skills only from `.claude/skills`; Git brings them without a step.
    for skill in SKILLS {
      for directory in [".agents/skills", ".claude/skills"] {
        assert_eq!(
          fs::read(checkout.path(&format!("{directory}/{skill}/SKILL.md"))).unwrap(),
          fs::read(p.path(&format!(".agents/skills/{skill}/SKILL.md"))).unwrap(),
          "{directory}/{skill}"
        );
      }
    }
    assert!(
      checkout
        .path(".claude/skills/hivex/templates/README.md")
        .is_file()
    );
    subset(&checkout.ok(&["init"]), &json!({"created":[],"updated":[]}));
    subset(
      &checkout.ok(&["update"]),
      &json!({"created":[],"removed":[],"updated":[]}),
    );
  }
}

/// A link in place of `link` to `target`, both relative to the project, as earlier releases
/// created Claude's: a symlink on Unix and a directory junction on Windows.
fn former_link(p: &Project, link: &str, target: &str) {
  #[cfg(unix)]
  std::os::unix::fs::symlink(Path::new("../..").join(target), p.path(link)).unwrap();
  // `mklink` needs backslashes, which rebuilding the path from its components gives.
  #[cfg(windows)]
  let native = |path: &str| p.path(path).components().collect::<std::path::PathBuf>();
  #[cfg(windows)]
  assert!(
    std::process::Command::new("cmd")
      .args(["/C", "mklink", "/J"])
      .arg(native(link))
      .arg(native(target))
      .output()
      .unwrap()
      .status
      .success()
  );
}

#[test]
fn update_replaces_claude_s_former_links_and_ignore_rules_with_copies() {
  let p = Project::new();
  p.ok(&["init"]);
  // The layout earlier releases left: ignored links to `.agents/skills`, recorded as links.
  for skill in SKILLS {
    fs::remove_dir_all(p.path(&format!(".claude/skills/{skill}"))).unwrap();
  }
  for skill in &SKILLS[..5] {
    former_link(
      &p,
      &format!(".claude/skills/{skill}"),
      &format!(".agents/skills/{skill}"),
    );
  }
  // A link elsewhere is the project's own.
  p.write("owner-skill/SKILL.md", "# Owner skill\n");
  former_link(&p, ".claude/skills/hivex-review", "owner-skill");
  let inventory: Vec<String> =
    serde_json::from_value(p.read_json(".agents/.hivex-managed.json")).unwrap();
  let mut former: Vec<String> = inventory
    .iter()
    .map(|path| match path.strip_prefix(".claude/skills/") {
      Some(rest) => format!(".claude/skills/{}", rest.split('/').next().unwrap()),
      None => path.clone(),
    })
    .collect();
  former.dedup();
  p.json(".agents/.hivex-managed.json", &json!(former));
  p.write(
    ".gitignore",
    "*.tmp\n/.claude/skills/hivex*\n/.hivex/\n/.reviews/\n",
  );
  // `init` keeps the former links and their rule, so no link becomes visible to Git.
  p.ok(&["init"]);
  assert!(fs::read_link(p.path(".claude/skills/hivex")).is_ok());
  assert!(
    fs::read_to_string(p.path(".gitignore"))
      .unwrap()
      .contains("/.claude/skills/hivex*\n")
  );
  // A former link whose copy is gone, as after npm's `node_modules` went, is still HiveX's.
  fs::remove_dir_all(p.path(".agents/skills/hivex-git")).unwrap();

  let updated = p.ok(&["update"]);
  for path in [".claude/skills/hivex", ".gitignore"] {
    assert!(list(&updated, "updated").contains(&json!(path)), "{path}");
  }
  assert!(list(&updated, "created").contains(&json!(".claude/skills/hivex/SKILL.md")));
  assert!(list(&updated, "preserved").contains(&json!(".claude/skills/hivex-review")));
  subset(&updated, &json!({"removed":[]}));
  for skill in &SKILLS[..5] {
    let directory = p.path(&format!(".claude/skills/{skill}"));
    assert!(fs::read_link(&directory).is_err(), "{skill}");
    assert_eq!(
      fs::read(directory.join("SKILL.md")).unwrap(),
      fs::read(p.path(&format!(".agents/skills/{skill}/SKILL.md"))).unwrap()
    );
  }
  assert_eq!(
    fs::read_to_string(p.path(".claude/skills/hivex-review/SKILL.md")).unwrap(),
    "# Owner skill\n"
  );
  assert_eq!(
    fs::read_to_string(p.path(".gitignore")).unwrap(),
    "*.tmp\n/.hivex/\n/.reviews/\n"
  );
  let recorded = p.read_json(".agents/.hivex-managed.json");
  assert!(
    recorded
      .as_array()
      .unwrap()
      .contains(&json!(".claude/skills/hivex/SKILL.md"))
  );
  assert!(
    !recorded
      .as_array()
      .unwrap()
      .contains(&json!(".claude/skills/hivex"))
  );
  subset(
    &p.ok(&["update"]),
    &json!({"created":[],"removed":[],"updated":[]}),
  );

  // A project link put in place of a managed copy is the project's from then on.
  fs::remove_dir_all(p.path(".claude/skills/hivex")).unwrap();
  former_link(&p, ".claude/skills/hivex", "owner-skill");
  let relinquished = p.ok(&["update"]);
  assert!(list(&relinquished, "preserved").contains(&json!(".claude/skills/hivex")));
  subset(&relinquished, &json!({"created":[],"removed":[]}));
  assert!(
    !p.read_json(".agents/.hivex-managed.json")
      .as_array()
      .unwrap()
      .contains(&json!(".claude/skills/hivex/SKILL.md"))
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

#[test]
fn a_version_warning_does_not_shrink_read_text() {
  let p = Project::new();
  p.ok(&["init"]);
  p.write("notes.md", "a line of text\n".repeat(200));
  let read = |p: &Project| p.ok(&["read", "notes.md", "--max-bytes", "300"]);
  let pinned = read(&p);
  p.write("hivex.json", r#"{"version":"0.0.1"}"#);
  let warned = read(&p);
  assert!(warned.get("versionWarning").is_some());
  assert_eq!(warned["text"], pinned["text"]);
  assert_eq!(warned["continuation"], pinned["continuation"]);
}

#[test]
fn the_skills_section_of_agents_md_is_kept_current_and_the_rest_left_alone() {
  let p = Project::new();
  p.ok(&["init"]);
  // A new project gets the section in its place in the bundled entrypoint.
  let created = fs::read_to_string(p.path("AGENTS.md")).unwrap();
  assert_eq!(created.matches(&skills_section("\n")).count(), 1);
  assert!(list(&p.ok(&["update"]), "preserved").contains(&json!("AGENTS.md")));

  // An edit inside the markers is replaced; the text around them stays.
  let section = skills_section("\n");
  let edited = section.replace("with no exception", "with exceptions");
  p.write("AGENTS.md", format!("# Rules\n\n{edited}\n\nMore rules.\n"));
  subset(&p.ok(&["update"]), &json!({"updated":["AGENTS.md"]}));
  assert_eq!(
    fs::read_to_string(p.path("AGENTS.md")).unwrap(),
    format!("# Rules\n\n{section}\n\nMore rules.\n")
  );
  subset(&p.ok(&["init"]), &json!({"updated":[]}));

  // Added to a file without it, the section goes before a final Relationships section.
  let relationships = "## Relationships\n\n- Depends on [Map](docs/README.md): orientation.\n";
  p.write("AGENTS.md", format!("# Rules\n\nText.\n\n{relationships}"));
  p.ok(&["update"]);
  assert_eq!(
    fs::read_to_string(p.path("AGENTS.md")).unwrap(),
    format!("# Rules\n\nText.\n\n{section}\n\n{relationships}")
  );
  subset(
    &p.ok(&["check", "--source", "AGENTS.md"]),
    &json!({"totalFindings":0}),
  );

  // A heading inside code or an HTML comment is not the final section; at the file's edges it
  // still is.
  for literal in [
    "# Rules\n\n```md\n## Relationships\n```\n",
    "# Rules\n\n```md\n```example\n## Relationships\n```\n",
    "# Rules\n\n<!--\n## Relationships\n-->\n",
    "# Rules\n\n> ## Relationships\n> Example\n",
    "# Rules\n\n- ## Relationships\n",
  ] {
    p.write("AGENTS.md", literal);
    p.ok(&["update"]);
    assert_eq!(
      fs::read_to_string(p.path("AGENTS.md")).unwrap(),
      format!("{literal}\n{section}\n")
    );
  }
  for (before, after) in [
    (
      "## Relationships\n",
      format!("{section}\n\n## Relationships\n"),
    ),
    (
      "# Rules\n\n## Relationships",
      format!("# Rules\n\n{section}\n\n## Relationships"),
    ),
  ] {
    p.write("AGENTS.md", before);
    p.ok(&["update"]);
    assert_eq!(fs::read_to_string(p.path("AGENTS.md")).unwrap(), after);
  }
  // Appending keeps the project's own trailing blank lines.
  p.write("AGENTS.md", "# Owner\n\n\n\n");
  p.ok(&["update"]);
  assert_eq!(
    fs::read_to_string(p.path("AGENTS.md")).unwrap(),
    format!("# Owner\n\n\n\n{section}\n")
  );

  // The section as Prettier formats Markdown is already current: formatting and update agree.
  let formatted = format!("# Rules\n\n{section}\n");
  assert!(section.starts_with("<!-- hivex:skills -->\n\n## HiveX skills\n"));
  assert!(section.ends_with("\n\n<!-- /hivex:skills -->"));
  p.write("AGENTS.md", &formatted);
  assert!(list(&p.ok(&["update"]), "preserved").contains(&json!("AGENTS.md")));
  assert_eq!(fs::read_to_string(p.path("AGENTS.md")).unwrap(), formatted);

  // A lone or extra marker is refused before anything is written.
  for broken in [
    format!("{section}\n<!-- hivex:skills -->\n"),
    format!("{section}\n<!-- /hivex:skills -->\n"),
  ] {
    p.write("AGENTS.md", &broken);
    assert_eq!(p.error(&["update"])["error"]["code"], "INVALID_AGENTS");
    assert_eq!(fs::read_to_string(p.path("AGENTS.md")).unwrap(), broken);
  }
  p.write("AGENTS.md", "# Rules\n<!-- /hivex:skills -->\n");
  fs::remove_file(p.path(".agents/skills/hivex-git/SKILL.md")).unwrap();
  for command in ["update", "init"] {
    assert_eq!(p.error(&[command])["error"]["code"], "INVALID_AGENTS");
  }
  assert!(!p.path(".agents/skills/hivex-git/SKILL.md").exists());
  assert_eq!(
    fs::read_to_string(p.path("AGENTS.md")).unwrap(),
    "# Rules\n<!-- /hivex:skills -->\n"
  );
}
