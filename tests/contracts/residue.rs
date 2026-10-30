// The residue script and its checks run through `sh`; Windows runs only the report without a test.
#![cfg(unix)]

use crate::support::{Project, bounded, subset};
use serde_json::{Value, json};
use std::path::Path;
use std::process::Command;

const SCRIPT: &str = ".agents/skills/hivex-git/scripts/residue.py";

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
  p.json(
    "docs/procedures/workspace-hygiene.json",
    &json!({"classes": [
      {
        "name": "Cache",
        "paths": [format!("{root}/cache/*")],
        "unneeded": "if [ -e \"$RESIDUE/odd\" ]; then exit 3; fi; test ! -e \"$RESIDUE/keep\"",
        "removal": "deterministic"
      },
      {
        "name": "Evidence",
        "paths": [format!("{root}/evidence/*")],
        "unneeded": "true",
        "removal": "review"
      },
      {"name": "Project", "paths": ["."], "unneeded": "true", "removal": "deterministic"}
    ]}),
  );
}

/// Run the installed script from the project root, moving to a stand-in Trash inside `outside`.
fn residue(p: &Project, outside: &Project, args: &[&str]) -> (Option<i32>, Value) {
  outside.write(
    "trash.sh",
    "mkdir -p \"$TRASHED\" && mv \"$1\" \"$TRASHED/\"\n",
  );
  let trash = format!("sh {}", outside.path("trash.sh").display());
  residue_with(p, outside, args, &trash)
}

fn residue_with(
  p: &Project,
  outside: &Project,
  args: &[&str],
  trash: &str,
) -> (Option<i32>, Value) {
  let mut command = Command::new("python3");
  command
    .current_dir(&p.root)
    .arg(SCRIPT)
    .args(args)
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

fn statuses(result: &Value) -> Vec<(String, String)> {
  result["classes"]
    .as_array()
    .unwrap()
    .iter()
    .flat_map(|class| class["items"].as_array().unwrap())
    .map(|item| {
      let path = Path::new(item["path"].as_str().unwrap());
      let name = path.file_name().unwrap().to_string_lossy().into_owned();
      (name, item["status"].as_str().unwrap().to_owned())
    })
    .collect()
}

#[test]
fn installed_script_reports_an_empty_declaration() {
  let p = Project::new();
  let outside = Project::new();
  p.ok(&["init"]);
  let (code, result) = residue(&p, &outside, &["--trash"]);
  assert_eq!(code, Some(0));
  subset(
    &result,
    &json!({"mode":"trash","classes":[],"moved":{"items":0,"bytes":0},"failed":[]}),
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
    &json!({"mode":"report","moved":{"items":0,"bytes":0}}),
  );
  let project = p.root.file_name().unwrap().to_string_lossy().into_owned();
  let expected = [
    ("gone", "unneeded"),
    ("live", "needed"),
    ("odd", "uncertain"),
    ("done", "unneeded"),
    (project.as_str(), "protected"),
  ];
  assert_eq!(
    statuses(&result),
    expected.map(|(name, status)| (name.to_owned(), status.to_owned()))
  );
  assert!(result["classes"][0]["unneededBytes"].as_u64().unwrap() > 0);
  for item in ["cache/gone", "cache/live", "cache/odd", "evidence/done"] {
    assert!(outside.path(item).join("data").is_file());
  }
  assert!(!outside.path("Trash").exists());
}

#[test]
fn trash_moves_only_unneeded_items_of_deterministic_classes() {
  let p = Project::new();
  let outside = Project::new();
  declared(&p, &outside);
  let (_, report) = residue(&p, &outside, &[]);
  let gone = report["classes"][0]["items"][0]["bytes"].clone();
  let (code, result) = residue(&p, &outside, &["--trash"]);
  assert_eq!(code, Some(0));
  subset(
    &result,
    &json!({"mode":"trash","moved":{"items":1,"bytes":gone},"failed":[]}),
  );
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
    assert_eq!(code, Some(2));
    assert!(result["error"].as_str().unwrap().contains("unneeded"));
    assert!(outside.path("cache/gone/data").is_file());
    assert!(!outside.path("Trash").exists());
  }
}

#[test]
fn an_empty_trash_override_moves_nothing() {
  let p = Project::new();
  let outside = Project::new();
  declared(&p, &outside);
  let (code, result) = residue_with(&p, &outside, &["--trash"], " ");
  assert_eq!(code, Some(2));
  assert!(result["error"].as_str().unwrap().contains("trash"));
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
  p.json(
    "docs/procedures/workspace-hygiene.json",
    &json!({"classes": [
      {
        "name": "Claimed by live work after the first check",
        "paths": [format!("{root}/claimed/*")],
        "unneeded": "if [ -e \"$RESIDUE/seen\" ]; then exit 1; fi; touch \"$RESIDUE/seen\"",
        "removal": "deterministic"
      },
      {"name": "Parent", "paths": [format!("{root}/nest")], "unneeded": "true", "removal": "deterministic"},
      {"name": "Child", "paths": [format!("{root}/nest/kept")], "unneeded": "true", "removal": "review"}
    ]}),
  );
  let (code, result) = residue(&p, &outside, &["--trash"]);
  assert_eq!(code, Some(0));
  subset(&result, &json!({"moved":{"items":0,"bytes":0},"failed":[]}));
  let expected = [
    ("item", "needed"),
    ("nest", "overlaps"),
    ("kept", "unneeded"),
  ];
  assert_eq!(
    statuses(&result),
    expected.map(|(name, status)| (name.to_owned(), status.to_owned()))
  );
  assert!(outside.path("claimed/item/data").is_file());
  assert!(outside.path("nest/kept/data").is_file());
  assert!(!outside.path("Trash").exists());
}

#[test]
fn the_documented_example_check_needs_a_readable_checkout() {
  let p = Project::new();
  let outside = Project::new();
  p.ok(&["init"]);
  let procedure = std::fs::read_to_string(p.path("docs/procedures/workspace-hygiene.md")).unwrap();
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
  p.json(
    "docs/procedures/workspace-hygiene.json",
    &json!({"classes": [
      {"name": "Example", "paths": [format!("{root}/cache/*")], "unneeded": check, "removal": "review"}
    ]}),
  );
  let (code, result) = residue(&p, &outside, &[]);
  assert_eq!(code, Some(0));
  let expected = [
    ("empty", "uncertain"),
    ("gone", "unneeded"),
    ("live", "needed"),
    ("missing", "uncertain"),
  ];
  assert_eq!(
    statuses(&result),
    expected.map(|(name, status)| (name.to_owned(), status.to_owned()))
  );
}
