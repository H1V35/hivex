use crate::support::{Project, bounded, subset};
use serde_json::{Value, json};
#[cfg(unix)]
use std::fs;
use std::path::Path;

/// A check or trash command line for the platform shell: `/bin/sh` on Unix, `cmd` on Windows.
fn shell(unix: &str, windows: &str) -> String {
  if cfg!(windows) { windows } else { unix }.to_owned()
}

/// Moves `RESIDUE` into the `TRASHED` folder, standing in for the Trash so tests never touch it.
fn stand_in() -> String {
  shell(
    "mkdir -p \"$TRASHED\" && mv \"$RESIDUE\" \"$TRASHED/\"",
    "(if not exist \"%TRASHED%\" mkdir \"%TRASHED%\") & move \"%RESIDUE%\" \"%TRASHED%\\\"",
  )
}

/// Run `residue` with the caller's `PATH`, moving through `trash` with `TRASHED` inside `outside`.
fn residue_with(
  p: &Project,
  outside: &Project,
  args: &[&str],
  trash: &str,
) -> (Option<i32>, Value) {
  let mut arguments = vec!["residue"];
  arguments.extend(args);
  let mut command = p.command(&arguments);
  command
    .env("PATH", std::env::var_os("PATH").unwrap_or_default())
    .env("HIVEX_TRASH", trash)
    .env("TRASHED", outside.path("Trash"));
  let output = bounded(command);
  let stream = if output.stdout.is_empty() {
    &output.stderr
  } else {
    &output.stdout
  };
  (
    output.status.code(),
    serde_json::from_slice(stream).unwrap(),
  )
}

fn residue(p: &Project, outside: &Project, args: &[&str]) -> (Option<i32>, Value) {
  residue_with(p, outside, args, &stand_in())
}

fn declare(p: &Project, classes: &Value) {
  p.json(
    "docs/procedures/workspace-hygiene.json",
    &json!({ "classes": classes }),
  );
}

/// An initialized project whose declaration covers residue in `outside`, with one item per status.
fn declared(p: &Project, outside: &Project) {
  p.ok(&["init"]);
  for (item, marker) in [
    ("cache/gone", None),
    ("cache/live", Some("keep")),
    ("cache/odd", Some("odd")),
    ("evidence/done", None),
  ] {
    outside.write(&format!("{item}/data"), "residue bytes");
    if let Some(marker) = marker {
      outside.write(&format!("{item}/{marker}"), "");
    }
  }
  let root = outside.root.display();
  let check = shell(
    "if [ -e \"$RESIDUE/odd\" ]; then exit 3; fi; test ! -e \"$RESIDUE/keep\"",
    "if exist \"%RESIDUE%\\odd\" (exit 3) else if exist \"%RESIDUE%\\keep\" (exit 1) else (exit 0)",
  );
  declare(
    p,
    &json!([
      {"name": "Cache", "paths": [format!("{root}/cache/*")], "unneeded": check, "removal": "deterministic"},
      {"name": "Evidence", "paths": [format!("{root}/evidence/*")], "unneeded": "exit 0", "removal": "review"},
      {"name": "Project", "paths": ["."], "unneeded": "exit 0", "removal": "deterministic"}
    ]),
  );
}

fn statuses(result: &Value) -> Vec<(String, String)> {
  result["items"]
    .as_array()
    .unwrap()
    .iter()
    .map(|item| {
      let path = Path::new(item["path"].as_str().unwrap());
      let name = path.file_name().unwrap().to_string_lossy().into_owned();
      (name, item["status"].as_str().unwrap().to_owned())
    })
    .collect()
}

fn expect(result: &Value, expected: &[(&str, &str)]) {
  let expected: Vec<_> = expected
    .iter()
    .map(|(name, status)| ((*name).to_owned(), (*status).to_owned()))
    .collect();
  assert_eq!(statuses(result), expected, "{result}");
}

#[test]
fn residue_reports_the_installed_empty_declaration() {
  let p = Project::new();
  let outside = Project::new();
  p.ok(&["init"]);
  let (code, result) = residue(&p, &outside, &["--trash"]);
  assert_eq!(code, Some(0));
  subset(
    &result,
    &json!({
      "command":"residue","modelCalls":0,"mode":"trash","status":"ready",
      "declaration":"docs/procedures/workspace-hygiene.json","classes":[],
      "moved":{"items":0,"bytes":0},"failed":{"items":0,"bytes":0},
      "totalItems":0,"items":[],"omittedItems":0
    }),
  );
  assert_eq!(
    p.error(&["residue", "extra"])["error"]["code"],
    "INVALID_ARGUMENT"
  );
  assert_eq!(
    p.error(&["residue", "--classes", "missing.json"])["error"]["code"],
    "INVALID_CONFIG"
  );
  p.write(
    "hivex.json",
    r#"{"include":["docs/**/*.md"],"version":"0.0.1"}"#,
  );
  subset(
    &p.ok(&["residue"]),
    &json!({"versionWarning":{"pinned":"0.0.1"}}),
  );
}

#[test]
fn report_mode_changes_nothing() {
  let p = Project::new();
  let outside = Project::new();
  declared(&p, &outside);
  let (code, result) = residue(&p, &outside, &[]);
  assert_eq!(code, Some(0));
  subset(
    &result,
    &json!({"mode":"report","moved":{"items":0,"bytes":0},"totalItems":5}),
  );
  let project = p.root.file_name().unwrap().to_string_lossy().into_owned();
  expect(
    &result,
    &[
      ("gone", "unneeded"),
      ("live", "needed"),
      ("odd", "uncertain"),
      ("done", "unneeded"),
      (&project, "protected"),
    ],
  );
  assert!(result["classes"][0]["unneededBytes"].as_u64().unwrap() > 0);
  for item in ["cache/gone", "cache/live", "cache/odd", "evidence/done"] {
    assert!(outside.path(item).join("data").is_file());
  }
  assert!(!outside.path("Trash").exists());
  let (_, limited) = residue(&p, &outside, &["--limit", "2"]);
  subset(&limited, &json!({"totalItems":5,"omittedItems":3}));
}

#[test]
fn trash_moves_only_unneeded_items_of_deterministic_classes() {
  let p = Project::new();
  let outside = Project::new();
  declared(&p, &outside);
  let (_, report) = residue(&p, &outside, &[]);
  let gone = report["items"][0]["bytes"].clone();
  let (code, result) = residue(&p, &outside, &["--trash"]);
  assert_eq!(code, Some(0), "{result}");
  subset(
    &result,
    &json!({"mode":"trash","status":"ready","moved":{"items":1,"bytes":gone},"failed":{"items":0}}),
  );
  assert_eq!(result["items"][0]["status"], "trashed");
  assert!(outside.path("Trash/gone/data").is_file());
  assert!(!outside.path("cache/gone").exists());
  for item in ["cache/live", "cache/odd", "evidence/done"] {
    assert!(outside.path(item).join("data").is_file());
  }
  assert!(p.path("AGENTS.md").is_file());
}

#[test]
fn a_class_without_a_check_stops_before_anything_moves() {
  for check in [None, Some(" ")] {
    let p = Project::new();
    let outside = Project::new();
    declared(&p, &outside);
    let mut declaration = p.read_json("docs/procedures/workspace-hygiene.json");
    let class = declaration["classes"][0].as_object_mut().unwrap();
    match check {
      Some(check) => class.insert("unneeded".into(), json!(check)),
      None => class.remove("unneeded"),
    };
    p.json("docs/procedures/workspace-hygiene.json", &declaration);
    let (code, result) = residue(&p, &outside, &["--trash"]);
    assert_eq!(code, Some(1));
    assert_eq!(result["error"]["code"], "INVALID_CONFIG");
    assert!(
      result["error"]["message"]
        .as_str()
        .unwrap()
        .contains("unneeded")
    );
    assert!(outside.path("cache/gone/data").is_file());
    assert!(!outside.path("Trash").exists());
  }
}

#[test]
fn without_a_trash_command_nothing_moves() {
  let p = Project::new();
  let outside = Project::new();
  declared(&p, &outside);
  let (code, result) = residue_with(&p, &outside, &["--trash"], " ");
  assert_eq!(code, Some(1));
  assert_eq!(result["error"]["code"], "TRASH_UNAVAILABLE");
  // No HIVEX_TRASH and no `trash` or `gio` on the test's empty PATH.
  let mut command = p.command(&["residue", "--trash"]);
  command.env_remove("HIVEX_TRASH");
  let output = bounded(command);
  assert_eq!(output.status.code(), Some(1));
  let error: Value = serde_json::from_slice(&output.stderr).unwrap();
  assert_eq!(error["error"]["code"], "TRASH_UNAVAILABLE");
  assert!(outside.path("cache/gone/data").is_file());
}

#[test]
fn trash_skips_items_claimed_since_their_check_or_holding_other_residue() {
  let p = Project::new();
  let outside = Project::new();
  p.ok(&["init"]);
  for item in ["claimed/item/data", "nest/kept/data"] {
    outside.write(item, "residue bytes");
  }
  let root = outside.root.display();
  let claimed = shell(
    "if [ -e \"$RESIDUE/seen\" ]; then exit 1; fi; touch \"$RESIDUE/seen\"",
    "if exist \"%RESIDUE%\\seen\" (exit 1) else (type nul > \"%RESIDUE%\\seen\" & exit 0)",
  );
  declare(
    &p,
    &json!([
      {"name": "Claimed by live work after the first check", "paths": [format!("{root}/claimed/*")], "unneeded": claimed, "removal": "deterministic"},
      {"name": "Parent", "paths": [format!("{root}/nest")], "unneeded": "exit 0", "removal": "deterministic"},
      {"name": "Child", "paths": [format!("{root}/nest/kept")], "unneeded": "exit 0", "removal": "review"}
    ]),
  );
  let (code, result) = residue(&p, &outside, &["--trash"]);
  assert_eq!(code, Some(0), "{result}");
  subset(&result, &json!({"moved":{"items":0,"bytes":0}}));
  expect(
    &result,
    &[
      ("item", "needed"),
      ("nest", "overlaps"),
      ("kept", "unneeded"),
    ],
  );
  assert!(outside.path("claimed/item/data").is_file());
  assert!(outside.path("nest/kept/data").is_file());
  assert!(!outside.path("Trash").exists());
}

#[test]
fn an_alias_of_a_review_item_is_not_moved() {
  let p = Project::new();
  let outside = Project::new();
  p.ok(&["init"]);
  outside.write("alias/data", "residue bytes");
  let root = outside.root.display();
  // On a case-insensitive volume, as on macOS and Windows by default, both name the same folder.
  declare(
    &p,
    &json!([
      {"name": "Upper", "paths": [format!("{root}/ALIAS")], "unneeded": "exit 0", "removal": "deterministic"},
      {"name": "Lower", "paths": [format!("{root}/alias")], "unneeded": "exit 0", "removal": "review"}
    ]),
  );
  let (code, result) = residue(&p, &outside, &["--trash"]);
  assert_eq!(code, Some(0), "{result}");
  subset(&result, &json!({"moved":{"items":0,"bytes":0}}));
  assert!(outside.path("alias/data").is_file());
}

#[test]
fn patterns_expand_wildcards_without_hidden_names_or_recursion() {
  let p = Project::new();
  let outside = Project::new();
  p.ok(&["init"]);
  for item in ["builds/one/data", "builds/two/data", "builds/.hidden/data"] {
    outside.write(item, "");
  }
  let root = outside.root.display();
  declare(
    &p,
    &json!([
      {"name": "Builds", "paths": [format!("{root}/builds/*"), format!("{root}/builds/one")], "unneeded": "exit 1", "removal": "review"}
    ]),
  );
  let (_, result) = residue(&p, &outside, &[]);
  expect(&result, &[("one", "needed"), ("two", "needed")]);
  declare(
    &p,
    &json!([{"name": "Deep", "paths": [format!("{root}/**")], "unneeded": "exit 0", "removal": "review"}]),
  );
  let (code, result) = residue(&p, &outside, &[]);
  assert_eq!(code, Some(1));
  assert_eq!(result["error"]["code"], "INVALID_CONFIG");
}

// The documented example is a POSIX shell command.
#[cfg(unix)]
#[test]
fn the_documented_example_check_needs_a_readable_checkout() {
  let p = Project::new();
  let outside = Project::new();
  p.ok(&["init"]);
  let procedure = fs::read_to_string(p.path("docs/procedures/workspace-hygiene.md")).unwrap();
  let example = procedure
    .split("```json\n")
    .nth(1)
    .unwrap()
    .split("```")
    .next()
    .unwrap();
  let check = serde_json::from_str::<Value>(example).unwrap()["classes"][0]["unneeded"].clone();
  outside.write("live-checkout/README", "");
  outside.write("cache/missing/data", "");
  outside.write("cache/empty/checkout-path", "");
  outside.write(
    "cache/live/checkout-path",
    outside.path("live-checkout").to_str().unwrap(),
  );
  outside.write(
    "cache/gone/checkout-path",
    outside.path("removed").to_str().unwrap(),
  );
  let root = outside.root.display();
  declare(
    &p,
    &json!([{"name": "Example", "paths": [format!("{root}/cache/*")], "unneeded": check, "removal": "review"}]),
  );
  let (code, result) = residue(&p, &outside, &[]);
  assert_eq!(code, Some(0));
  expect(
    &result,
    &[
      ("empty", "uncertain"),
      ("gone", "unneeded"),
      ("live", "needed"),
      ("missing", "uncertain"),
    ],
  );
}
