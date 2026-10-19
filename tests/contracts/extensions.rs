use crate::support::{Project, bounded, list, subset};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// The directory holding `git`, so the CLI finds it without the real Claude or Codex.
fn git_directory() -> PathBuf {
  std::env::split_paths(&std::env::var_os("PATH").unwrap())
    .find(|directory| directory.join("git").is_file() || directory.join("git.exe").is_file())
    .expect("git is installed")
}

fn git(repository: &Project, args: &[&str]) {
  let status = Command::new("git")
    .current_dir(&repository.root)
    .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
    .args(args)
    .output()
    .unwrap();
  assert!(
    status.status.success(),
    "{}",
    String::from_utf8_lossy(&status.stderr)
  );
}

/// Commit the extension's current files and tag them as `version`.
fn release(extension: &Project, version: &str) {
  git(extension, &["add", "-A"]);
  git(extension, &["commit", "-q", "-m", version]);
  git(extension, &["tag", &format!("v{version}")]);
}

fn extension() -> Project {
  let extension = Project::new();
  git(&extension, &["init", "-q"]);
  extension.write("hivex-extension.json", r#"{"replaces":["hivex-git"]}"#);
  extension.write("skills/demo-git/SKILL.md", "# Demo git 1\n");
  extension.write("skills/hivex-design/SKILL.md", "# Replaced design\n");
  extension.write("assets/project/AGENTS.md", "# Demo agents 1\n");
  extension.write(
    "assets/project/docs/notes.md",
    "---\ntitle: Notes\nstatus: draft\ncreated_at: 2026-01-01\ntags: [notes]\n---\n\n# Notes\n",
  );
  release(&extension, "0.1.0");
  extension
}

fn run(p: &Project, args: &[&str]) -> Value {
  let mut command = p.command(args);
  command.env("PATH", git_directory());
  let output = bounded(command);
  let stream = if output.status.success() {
    &output.stdout
  } else {
    &output.stderr
  };
  serde_json::from_slice(stream).unwrap()
}

fn read(p: &Project, file: &str) -> String {
  fs::read_to_string(p.path(file)).unwrap()
}

#[test]
fn init_installs_an_extension_over_the_foundation() {
  let extension = extension();
  let repository = extension.root.to_str().unwrap();
  let p = Project::new();
  let result = run(&p, &["init", "--extension", repository]);
  subset(
    &result,
    &json!({"extensions":[{"repository":repository,"version":"0.1.0"}]}),
  );
  let created = list(&result, "created");
  for path in [
    ".agents/skills/demo-git/SKILL.md",
    ".claude/skills/demo-git",
    "docs/notes.md",
  ] {
    assert!(created.contains(&json!(path)), "{path}");
  }
  // The manifest replaces hivex-git; a same-name skill replaces the bundled one entirely.
  assert!(!p.path(".agents/skills/hivex-git").exists());
  assert!(!p.path(".claude/skills/hivex-git").exists());
  assert_eq!(
    read(&p, ".agents/skills/hivex-design/SKILL.md"),
    "# Replaced design\n"
  );
  assert!(!p.path(".agents/skills/hivex-design/references").exists());
  assert!(p.path(".agents/skills/hivex/SKILL.md").is_file());
  // Extension project files win over the bundled ones and are dated like them.
  assert_eq!(read(&p, "AGENTS.md"), "# Demo agents 1\n");
  assert!(!read(&p, "docs/notes.md").contains("created_at: 2026-01-01"));
  assert!(p.path("docs/PRD.md").is_file());
  assert_eq!(
    p.read_json("hivex.json")["extensions"],
    json!([{"repository": repository, "version": "0.1.0"}])
  );
  assert!(read(&p, ".gitignore").contains("/.claude/skills/demo-git\n"));
  // The cached release is local state and invisible to source selection.
  assert!(
    !list(&run(&p, &["sources"]), "documents")
      .iter()
      .any(|document| document["path"].as_str().unwrap().starts_with(".hivex/"))
  );
}

#[test]
fn update_moves_extensions_forward_and_init_installs_the_pinned_release() {
  let extension = extension();
  let repository = extension.root.to_str().unwrap();
  let p = Project::new();
  run(&p, &["init", "--extension", repository]);
  extension.write("skills/demo-git/SKILL.md", "# Demo git 2\n");
  extension.write("assets/project/AGENTS.md", "# Demo agents 2\n");
  release(&extension, "0.2.0");

  let updated = run(&p, &["update"]);
  subset(
    &updated,
    &json!({"extensions":[{"repository":repository,"version":"0.2.0"}]}),
  );
  for path in [".agents/skills/demo-git/SKILL.md", "hivex.json"] {
    assert!(list(&updated, "updated").contains(&json!(path)), "{path}");
  }
  assert_eq!(
    read(&p, ".agents/skills/demo-git/SKILL.md"),
    "# Demo git 2\n"
  );
  // Project files belong to the project once created.
  assert_eq!(read(&p, "AGENTS.md"), "# Demo agents 1\n");
  subset(&run(&p, &["update"]), &json!({"updated":[]}));

  // Another machine installs the pinned release, not a newer one.
  extension.write("skills/demo-git/SKILL.md", "# Demo git 3\n");
  release(&extension, "0.3.0");
  let other = Project::new();
  other.write("hivex.json", read(&p, "hivex.json"));
  run(&other, &["init"]);
  assert_eq!(
    read(&other, ".agents/skills/demo-git/SKILL.md"),
    "# Demo git 2\n"
  );
}

#[test]
fn extension_failures_are_reported_without_writing() {
  let extension = extension();
  let repository = extension.root.to_str().unwrap();
  let p = Project::new();
  p.write("hivex.json", "{}");
  assert_eq!(
    run(&p, &["init", "--extension", repository])["error"]["code"],
    "INVALID_ARGUMENT"
  );

  let missing = Project::new();
  let gone = missing.path("missing-extension");
  let failed = run(&missing, &["init", "--extension", gone.to_str().unwrap()]);
  assert_eq!(failed["error"]["code"], "EXTENSION_UNAVAILABLE");
  assert!(!missing.path("AGENTS.md").exists());

  extension.write("hivex-extension.json", r#"{"replaces":["hivex-unknown"]}"#);
  release(&extension, "0.2.0");
  let invalid = Project::new();
  assert_eq!(
    run(&invalid, &["init", "--extension", repository])["error"]["code"],
    "INVALID_EXTENSION"
  );
  assert!(!invalid.path("AGENTS.md").exists());
}
