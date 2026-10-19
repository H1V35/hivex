use crate::support::{Project, bounded, list, subset};
use serde_json::{Value, json};
use std::fs;
use std::process::Command;

/// The real `PATH` without the directories of Claude and Codex, so the CLI finds `git` and
/// everything Git for Windows needs, but not the hosts of whoever runs the tests.
fn path_without_hosts() -> std::ffi::OsString {
  let path = std::env::var_os("PATH").unwrap();
  let directories = std::env::split_paths(&path).filter(|directory| {
    !["claude", "codex"].iter().any(|host| {
      ["", "exe", "cmd", "bat"]
        .iter()
        .any(|extension| directory.join(host).with_extension(extension).is_file())
    })
  });
  std::env::join_paths(directories).unwrap()
}

fn git(repository: &Project, args: &[&str]) {
  let status = Command::new("git")
    .current_dir(&repository.root)
    // Fixtures are committed byte for byte, also where Git converts line endings by default.
    .args(["-c", "core.autocrlf=false", "-c", "user.name=Test"])
    .args(["-c", "user.email=test@example.com"])
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
  extension.write(
    "hivex-extension.json",
    r#"{"replaces":["hivex-git"],"ignore":["/work/"]}"#,
  );
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
  command.env("PATH", path_without_hosts());
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
  assert!(read(&p, ".gitignore").contains("/.claude/skills/demo-git\n/work/\n"));
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

#[test]
fn extension_ignore_templates_and_tags_are_honoured() {
  let extension = extension();
  extension.write("assets/project/.gitignore", "custom\n");
  release(&extension, "0.2.0");
  // A branch with the release's name must not shadow its tag.
  extension.write("skills/demo-git/SKILL.md", "# From a branch\n");
  git(&extension, &["commit", "-q", "-am", "branch"]);
  git(&extension, &["branch", "v0.2.0"]);
  let repository = extension.root.to_str().unwrap();
  let p = Project::new();
  run(&p, &["init", "--extension", repository]);
  assert_eq!(
    read(&p, ".agents/skills/demo-git/SKILL.md"),
    "# Demo git 1\n"
  );
  let ignore = read(&p, ".gitignore");
  // Extension rules come first and Hivex's own rules last.
  assert!(
    ignore.starts_with("custom\n/.claude/skills/demo-git\n/work/\n"),
    "{ignore}"
  );
  assert!(
    ignore.ends_with("/.claude/skills/hivex*\n/.hivex/\n/.reviews/\n"),
    "{ignore}"
  );
  assert!(ignore.contains("/work/\n"));
}

#[test]
fn update_removes_what_a_release_retires_and_old_caches() {
  let extension = extension();
  extension.write("skills/demo-old/SKILL.md", "# Old\n");
  extension.write("skills/demo-git/extra.md", "# Extra\n");
  release(&extension, "0.2.0");
  let repository = extension.root.to_str().unwrap();
  let p = Project::new();
  run(&p, &["init", "--extension", repository]);
  assert!(p.path(".agents/skills/demo-old/SKILL.md").is_file());
  fs::remove_dir_all(extension.path("skills/demo-old")).unwrap();
  fs::remove_file(extension.path("skills/demo-git/extra.md")).unwrap();
  release(&extension, "0.3.0");
  let updated = run(&p, &["update"]);
  let removed = list(&updated, "removed");
  for path in [
    ".agents/skills/demo-old/SKILL.md",
    ".agents/skills/demo-git/extra.md",
    ".claude/skills/demo-old",
  ] {
    assert!(removed.contains(&json!(path)), "{path}");
  }
  assert!(!p.path(".agents/skills/demo-old").exists());
  assert!(!p.path(".claude/skills/demo-old").exists());
  assert!(p.path(".agents/skills/demo-git/SKILL.md").is_file());
  let cached: Vec<_> = fs::read_dir(p.path(".hivex/extensions"))
    .unwrap()
    .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
    .collect();
  assert_eq!(cached.len(), 1, "{cached:?}");
  assert!(cached[0].ends_with("@0.3.0"));
}

#[test]
fn invalid_releases_are_never_cached_and_relative_paths_follow_the_project() {
  let p = Project::new();
  let nested = Project {
    root: p.path("vendor/extension"),
  };
  fs::create_dir_all(&nested.root).unwrap();
  git(&nested, &["init", "-q"]);
  nested.write("hivex-extension.json", "[]");
  release(&nested, "0.1.0");
  p.write(
    "hivex.json",
    r#"{"extensions":[{"repository":"./vendor/extension","version":"0.1.0"}]}"#,
  );
  for _ in 0..2 {
    assert_eq!(run(&p, &["init"])["error"]["code"], "INVALID_EXTENSION");
  }
  assert_eq!(
    fs::read_dir(p.path(".hivex/extensions")).unwrap().count(),
    0
  );
  nested.write("hivex-extension.json", "{}");
  nested.write("skills/nested/SKILL.md", "# Nested\n");
  git(&nested, &["add", "-A"]);
  git(&nested, &["commit", "-q", "-m", "fix"]);
  git(&nested, &["tag", "-f", "v0.1.0"]);
  run(&p, &["init"]);
  assert_eq!(read(&p, ".agents/skills/nested/SKILL.md"), "# Nested\n");
}

#[cfg(unix)]
#[test]
fn extensions_cannot_reach_outside_the_project_or_their_checkout() {
  use std::os::unix::fs::{PermissionsExt, symlink};
  let outside = Project::new();
  outside.write("AGENTS.md", "# Outside\n");

  // A linked cache is refused before anything is fetched or written.
  let guarded = extension();
  let repository = guarded.root.to_str().unwrap();
  let p = Project::new();
  fs::create_dir(p.path(".hivex")).unwrap();
  symlink(&outside.root, p.path(".hivex/extensions")).unwrap();
  assert_eq!(
    run(&p, &["init", "--extension", repository])["error"]["code"],
    "INVALID_DESTINATION"
  );
  assert_eq!(fs::read_dir(&outside.root).unwrap().count(), 1);

  // A linked source directory is not followed.
  let linked = extension();
  fs::remove_dir_all(linked.path("assets")).unwrap();
  symlink(&outside.root, linked.path("assets")).unwrap();
  release(&linked, "0.2.0");
  let p = Project::new();
  let linked_repository = linked.root.to_str().unwrap();
  assert_eq!(
    run(&p, &["init", "--extension", linked_repository])["error"]["code"],
    "INVALID_EXTENSION"
  );

  // Names that could leave the tree are rejected.
  let escaping = extension();
  escaping.write("skills/demo-git/..\\..\\outside.md", "# Escape\n");
  release(&escaping, "0.2.0");
  let p = Project::new();
  assert_eq!(
    run(
      &p,
      &["init", "--extension", escaping.root.to_str().unwrap()]
    )["error"]["code"],
    "INVALID_EXTENSION"
  );

  // Executable scripts stay executable.
  let scripted = extension();
  scripted.write("skills/demo-git/scripts/run.sh", "#!/bin/sh\n");
  let script = scripted.path("skills/demo-git/scripts/run.sh");
  fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
  release(&scripted, "0.2.0");
  let p = Project::new();
  run(
    &p,
    &["init", "--extension", scripted.root.to_str().unwrap()],
  );
  let mode = fs::metadata(p.path(".agents/skills/demo-git/scripts/run.sh"))
    .unwrap()
    .permissions()
    .mode();
  assert_ne!(mode & 0o111, 0);
}

fn cache_entries(p: &Project) -> Vec<String> {
  let mut names: Vec<_> = fs::read_dir(p.path(".hivex/extensions"))
    .unwrap()
    .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
    .collect();
  names.sort();
  names
}

#[test]
fn inventories_track_ownership_and_reject_paths_outside_the_managed_trees() {
  let extension = extension();
  extension.write("skills/demo-old/SKILL.md", "# Old\n");
  release(&extension, "0.2.0");
  let repository = extension.root.to_str().unwrap();
  let p = Project::new();
  run(&p, &["init", "--extension", repository]);
  // `init` at a release without the skill keeps recording it, so `update` can retire it.
  fs::remove_dir_all(extension.path("skills/demo-old")).unwrap();
  release(&extension, "0.3.0");
  let config = read(&p, "hivex.json").replace("\"0.2.0\"", "\"0.3.0\"");
  p.write("hivex.json", config);
  run(&p, &["init"]);
  assert!(p.path(".agents/skills/demo-old/SKILL.md").is_file());
  assert!(read(&p, ".agents/.hivex-managed.json").contains("demo-old"));
  run(&p, &["update"]);
  assert!(!p.path(".agents/skills/demo-old").exists());

  // A tampered inventory cannot reach project files.
  p.write(
    ".agents/.hivex-managed.json",
    r#"[".agents/skills/hivex/../../../AGENTS.md"]"#,
  );
  assert_eq!(run(&p, &["update"])["error"]["code"], "INVALID_CONFIG");
  assert!(p.path("AGENTS.md").is_file());
}

#[test]
fn projects_without_inventory_keep_their_own_hivex_named_skills() {
  let p = Project::new();
  run(&p, &["init"]);
  fs::remove_file(p.path(".agents/.hivex-managed.json")).unwrap();
  p.write(".agents/skills/hivex-custom/SKILL.md", "# Mine\n");
  run(&p, &["update"]);
  assert_eq!(read(&p, ".agents/skills/hivex-custom/SKILL.md"), "# Mine\n");
}

#[test]
fn invalid_releases_leave_the_pinned_cache_and_negations_are_refused() {
  let extension = extension();
  let repository = extension.root.to_str().unwrap();
  let p = Project::new();
  run(&p, &["init", "--extension", repository]);
  let before = cache_entries(&p);
  extension.write("hivex-extension.json", r#"{"replaces":["hivex-unknown"]}"#);
  release(&extension, "0.2.0");
  assert_eq!(run(&p, &["update"])["error"]["code"], "INVALID_EXTENSION");
  assert_eq!(cache_entries(&p), before);

  extension.write("hivex-extension.json", r#"{"ignore":["!/.hivex/"]}"#);
  release(&extension, "0.3.0");
  assert_eq!(run(&p, &["update"])["error"]["code"], "INVALID_EXTENSION");
  assert_eq!(cache_entries(&p), before);

  // A rule hiding a second line cannot smuggle a negation in.
  extension.write(
    "hivex-extension.json",
    r#"{"ignore":["/work/\n!/.hivex/"]}"#,
  );
  release(&extension, "0.4.0");
  assert_eq!(run(&p, &["update"])["error"]["code"], "INVALID_EXTENSION");
  assert_eq!(cache_entries(&p), before);
}

#[test]
fn crlf_extension_documents_are_dated() {
  let extension = extension();
  extension.write(
    "assets/project/docs/windows.md",
    "---\r\ntitle: Windows\r\nstatus: draft\r\ncreated_at: 2026-01-01\r\ntags: [notes]\r\n---\r\n\r\n# Windows\r\n",
  );
  release(&extension, "0.2.0");
  let p = Project::new();
  run(
    &p,
    &["init", "--extension", extension.root.to_str().unwrap()],
  );
  let text = read(&p, "docs/windows.md");
  assert!(!text.contains("2026-01-01"), "{text}");
  assert!(text.contains("\r\ncreated_at: "), "{text}");
}

#[cfg(unix)]
#[test]
fn cache_entries_and_checkout_content_cannot_redirect_writes() {
  use std::os::unix::fs::{PermissionsExt, symlink};
  let outside = Project::new();
  outside.write("AGENTS.md", "# Outside\n");

  // A file the repository calls `source` stays inside the checkout.
  let extension = extension();
  symlink(outside.path("AGENTS.md"), extension.path("source")).unwrap();
  release(&extension, "0.2.0");
  let p = Project::new();
  run(
    &p,
    &["init", "--extension", extension.root.to_str().unwrap()],
  );
  assert_eq!(read(&outside, "AGENTS.md"), "# Outside\n");

  // A linked staging entry is refused instead of being written through.
  let entry = cache_entries(&p).pop().unwrap();
  fs::remove_dir_all(p.path(&format!(".hivex/extensions/{entry}"))).unwrap();
  symlink(
    &outside.root,
    p.path(&format!(".hivex/extensions/{entry}.partial")),
  )
  .unwrap();
  assert_eq!(run(&p, &["update"])["error"]["code"], "INVALID_DESTINATION");
  assert_eq!(fs::read_dir(&outside.root).unwrap().count(), 1);

  // An unreadable source directory is reported, not treated as absent.
  let q = Project::new();
  run(
    &q,
    &["init", "--extension", extension.root.to_str().unwrap()],
  );
  let entry = cache_entries(&q).pop().unwrap();
  let assets = q.path(&format!(".hivex/extensions/{entry}/checkout/assets"));
  fs::set_permissions(&assets, fs::Permissions::from_mode(0o000)).unwrap();
  let failed = run(&q, &["init"]);
  fs::set_permissions(&assets, fs::Permissions::from_mode(0o755)).unwrap();
  assert_eq!(failed["error"]["code"], "INVALID_EXTENSION");
}
