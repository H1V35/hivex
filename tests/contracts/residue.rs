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
  let mut command = Command::new("python3");
  command
    .current_dir(&p.root)
    .arg(SCRIPT)
    .args(args)
    .env(
      "HIVEX_TRASH",
      format!("sh {}", outside.path("trash.sh").display()),
    )
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
fn a_class_without_its_check_stops_before_anything_moves() {
  let p = Project::new();
  let outside = Project::new();
  declared(&p, &outside);
  let mut declaration = p.read_json("docs/procedures/workspace-hygiene.json");
  declaration["classes"][0]
    .as_object_mut()
    .unwrap()
    .remove("unneeded");
  p.json("docs/procedures/workspace-hygiene.json", &declaration);
  let (code, result) = residue(&p, &outside, &["--trash"]);
  assert_eq!(code, Some(2));
  assert!(result["error"].as_str().unwrap().contains("unneeded"));
  assert!(outside.path("cache/gone/data").is_file());
  assert!(!outside.path("Trash").exists());
}
