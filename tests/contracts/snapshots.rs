use crate::support::{Project, list, subset, work};
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::symlink;

fn checked() -> Project {
  let p = Project::policy();
  assert_eq!(p.model_cli(&["update"])["status"], "ready");
  p.ok(&["snapshot", "export"]);
  p
}
fn clone_from(p: &Project) -> Project {
  let clone = Project::policy();
  clone.write(
    ".hivex/graph.json",
    fs::read(p.path(".hivex/graph.json")).unwrap(),
  );
  clone
}

#[test]
fn shared_snapshot_supports_fresh_clone_without_reextraction_or_query_artifacts() {
  let p = checked();
  let original = fs::read(p.path(".hivex/graph.json")).unwrap();
  let clone = clone_from(&p);
  let found = clone.ok(&["search", "seven days"]);
  assert_eq!(found["decisions"][0]["quality"], "checked");
  let id = found["decisions"][0]["id"].as_str().unwrap();
  let related = clone.ok(&["neighbors", id]);
  assert_eq!(list(&related, "decisions").len(), 2);
  clone.ok(&["status"]);
  assert!(!clone.path(".hivex/knowledge.sqlite").exists());
  assert_eq!(
    clone.cli(&[
      "ask",
      "seven days",
      "--max-calls",
      "0",
      "--codex",
      "/no-model"
    ])["work"]["calls"],
    0
  );
  subset(
    &clone.ok(&["update", "--max-calls", "0", "--codex", "/no-model"]),
    &json!({"status":"ready","pendingUnits":[],"work":{"calls":0}}),
  );
  assert_eq!(fs::read(clone.path(".hivex/graph.json")).unwrap(), original);
  assert_eq!(clone.calls(), 0);
}

#[test]
fn snapshot_import_blocks_unfinished_work_then_preserves_its_completed_accounting() {
  let source = checked();
  let p = Project::policy();
  let pending = p.model_cli(&["update", "--max-calls", "1"]);
  let before = p.graph();
  let before_work = p.work(pending["work"]["id"].as_str().unwrap());
  p.write(
    ".hivex/graph.json",
    fs::read(source.path(".hivex/graph.json")).unwrap(),
  );
  assert_eq!(
    p.error(&["snapshot", "import"])["error"]["code"],
    "UNFINISHED_WORK"
  );
  assert_eq!(p.graph(), before);
  assert_eq!(p.work(pending["work"]["id"].as_str().unwrap()), before_work);
  let done = p.model_cli(&["update", "--max-calls", "2"]);
  assert_eq!(done["work"]["id"], pending["work"]["id"]);
  assert_eq!(p.ok(&["snapshot", "import"])["status"], "ready");
  assert_eq!(p.model_cli(&["update"])["work"]["calls"], 2);
  assert_eq!(p.calls(), 2);
}

#[test]
fn stale_or_unavailable_snapshot_sources_preserve_unaffected_knowledge() {
  let source = checked();
  for missing in [false, true] {
    let p = clone_from(&source);
    if missing {
      fs::remove_file(p.path("cache.md")).unwrap();
    } else {
      p.write(
        "cache.md",
        "# Cache\n\nCached data expires after thirty days.\n",
      );
    }
    let imported = p.ok(&["snapshot", "import"]);
    assert_eq!(imported["status"], "partial");
    assert!(
      imported["sources"][if missing { "unavailable" } else { "stale" }]
        .as_array()
        .unwrap()
        .contains(&json!("cache.md"))
    );
    assert!(list(&p.ok(&["search", "seven days"]), "decisions").is_empty());
    assert!(!list(&p.ok(&["search", "revocation"]), "decisions").is_empty());
  }
}

#[test]
fn malformed_snapshots_never_replace_local_knowledge() {
  let p = checked();
  let baseline = p.graph();
  let snapshot = p.read_json(".hivex/graph.json");
  let mut bad_edge = snapshot.clone();
  bad_edge["relationships"][0]["from"] = json!("unknown-endpoint");
  let mut bad_evidence = snapshot.clone();
  bad_evidence["decisions"][0]["lineStart"] = json!(0);
  let mut empty = snapshot.clone();
  empty["relationships"][0]["evidence"] = json!([]);
  let mut reversed = snapshot.clone();
  reversed["relationships"][0]["evidence"][0]["lineEnd"] = json!(1);
  let mut out_of_range = snapshot.clone();
  out_of_range["decisions"][0]["lineEnd"] = json!(1);
  let original = fs::read(p.path(".hivex/graph.json")).unwrap();
  for value in [bad_edge, bad_evidence, empty, reversed, out_of_range] {
    p.json(".hivex/graph.json", &value);
    assert!(!p.raw(&["snapshot", "import"]).status.success());
    assert_eq!(p.graph(), baseline);
    p.ok(&["snapshot", "export"]);
    assert_eq!(fs::read(p.path(".hivex/graph.json")).unwrap(), original);
  }
  p.write(".hivex/graph.json", "{broken");
  assert!(!p.raw(&["snapshot", "import"]).status.success());
  assert_eq!(p.graph(), baseline);
  let fresh = Project::policy();
  fresh.write(".hivex/graph.json", "{broken");
  assert!(!fresh.raw(&["search", "cache"]).status.success());
  assert!(!fresh.path(".hivex/knowledge.sqlite").exists());
}

#[test]
fn snapshot_symlinks_and_absent_local_graph_do_not_bypass_protection() {
  let p = checked();
  let outside = Project::new();
  outside.write(
    "snapshot.json",
    fs::read(p.path(".hivex/graph.json")).unwrap(),
  );
  let before = fs::read(outside.path("snapshot.json")).unwrap();
  fs::remove_file(p.path(".hivex/graph.json")).unwrap();
  symlink(outside.path("snapshot.json"), p.path(".hivex/graph.json")).unwrap();
  for op in ["import", "export"] {
    assert!(!p.raw(&["snapshot", op]).status.success());
    assert_eq!(fs::read(outside.path("snapshot.json")).unwrap(), before);
  }
  let unseeded = Project::policy();
  fs::create_dir_all(unseeded.path(".hivex")).unwrap();
  symlink(
    outside.path("snapshot.json"),
    unseeded.path(".hivex/graph.json"),
  )
  .unwrap();
  assert_eq!(
    unseeded.error(&["search", "cache"])["error"]["code"],
    "INVALID_SNAPSHOT"
  );
  assert_eq!(fs::read(outside.path("snapshot.json")).unwrap(), before);
  let fresh = clone_from(&outside_snapshot(&before));
  let mut w = work("unfinished-first");
  w["kind"] = json!("update");
  w["remaining"] = json!(["cache.md:1-3"]);
  fresh.set_work(&w);
  assert_eq!(
    fresh.error(&["snapshot", "import"])["error"]["code"],
    "UNFINISHED_WORK"
  );
  assert_eq!(fresh.work("unfinished-first"), w);
  assert_eq!(
    fresh
      .db()
      .query_row("SELECT count(*) FROM graph", [], |row| row.get::<_, i64>(0))
      .unwrap(),
    0
  );
}
fn outside_snapshot(bytes: &[u8]) -> Project {
  let p = Project::policy();
  p.write(".hivex/graph.json", bytes);
  p
}

#[test]
fn relocation_preserves_ids_and_distinguishes_reusable_coverage() {
  for kind in [
    "identical",
    "changed",
    "mixed-version",
    "missing-version",
    "existing-target",
  ] {
    assert_relocation(kind);
  }
}

#[test]
fn relocation_rejects_protected_destinations_and_unfinished_work_without_changes() {
  let p = checked();
  fs::remove_file(p.path("cache.md")).unwrap();
  let before = p.graph();
  for destination in ["../relocated.md", ".hivex/relocated.md"] {
    assert_eq!(
      p.error(&["snapshot", "relocate", "cache.md", destination])["error"]["code"],
      "INVALID_ARGUMENT"
    );
    assert_eq!(p.graph(), before);
  }
  let p = Project::policy();
  p.model_cli(&["update", "--max-calls", "1"]);
  let before = p.graph();
  fs::remove_file(p.path("cache.md")).unwrap();
  p.write(
    "docs/cache.md",
    "# Cache\n\nCached data expires after seven days.\n",
  );
  assert_eq!(
    p.error(&["snapshot", "relocate", "cache.md", "docs/cache.md"])["error"]["code"],
    "UNFINISHED_WORK"
  );
  assert_eq!(p.graph(), before);
}

fn warning_project() -> Project {
  let p = checked();
  let mut graph = p.graph();
  let version = graph["documents"]["privacy.md"].clone();
  graph["warnings"] = json!([{"kind":"limitation","message":"First warning needs closure.","scope":[{"document":"privacy.md","lineStart":3,"lineEnd":3,"version":version}]},{"kind":"limitation","message":"Second warning stays active.","scope":[{"document":"privacy.md","lineStart":3,"lineEnd":3,"version":version}]}]);
  p.set_graph(&graph);
  p
}
fn closure(p: &Project, index: usize) -> Value {
  let warnings = p.ok(&["warnings", "--all"]);
  json!({"id":warnings["warnings"][index]["id"],"reason":"Original source evidence closes this warning.","evidence":[{"document":"privacy.md","lineStart":3,"lineEnd":3,"version":p.graph()["documents"]["privacy.md"]}]})
}

#[test]
fn warning_resolution_is_atomic_scoped_and_reactivates_without_losing_history() {
  let p = warning_project();
  let graph = p.graph();
  let first = closure(&p, 0);
  let second = closure(&p, 1);
  let work_id: String = p
    .db()
    .query_row("SELECT id FROM work LIMIT 1", [], |row| row.get(0))
    .unwrap();
  let work_before = p.work(&work_id);
  for kind in ["missing-id", "duplicate", "bad-line", "stale-version"] {
    let mut bad = second.clone();
    match kind {
      "missing-id" => bad["id"] = json!("missing-warning"),
      "duplicate" => bad = first.clone(),
      "bad-line" => bad["evidence"][0]["lineStart"] = json!(99),
      _ => bad["evidence"][0]["version"] = json!("stale-version"),
    }
    p.json("resolutions.json", &json!([first, bad]));
    assert!(
      !p.raw(&[
        "warnings",
        "--resolve",
        p.path("resolutions.json").to_str().unwrap()
      ])
      .status
      .success()
    );
    assert_eq!(p.graph(), graph);
  }
  p.json("resolutions.json", &json!([first]));
  subset(
    &p.ok(&[
      "warnings",
      "--resolve",
      p.path("resolutions.json").to_str().unwrap(),
    ]),
    &json!({"resolved":1,"warningSummary":{"limitations":1,"resolved":1}}),
  );
  assert_eq!(p.graph()["decisions"], graph["decisions"]);
  assert_eq!(p.graph()["relationships"], graph["relationships"]);
  assert_eq!(p.work(work_before["id"].as_str().unwrap()), work_before);
  for args in [
    vec!["warnings"],
    vec!["status"],
    vec!["search", "revocation"],
    vec!["snapshot", "export"],
  ] {
    let result = p.ok(&args);
    assert!(
      !result["warnings"]
        .to_string()
        .contains("First warning needs closure.")
    );
    assert!(
      result["warnings"]
        .to_string()
        .contains("Second warning stays active.")
    );
  }
  let clone = clone_from(&p);
  subset(
    &clone.ok(&["warnings"]),
    &json!({"warningSummary":{"limitations":1,"resolved":1}}),
  );
  assert!(!clone.path(".hivex/knowledge.sqlite").exists());
  p.write(
    "privacy.md",
    "# Access\n\nRevoking access immediately removes cached private data.\nChanged source.\n",
  );
  let warnings = p.ok(&["warnings", "--all"]);
  let reopened = list(&warnings, "warnings")
    .iter()
    .find(|w| w["id"] == first["id"])
    .unwrap();
  assert_eq!(reopened["state"], "active");
  assert_eq!(reopened["resolution"]["reason"], first["reason"]);
}

#[test]
fn update_warning_closures_survive_resumption_but_not_validation_failures() {
  for validation in [false, true] {
    let p = Project::policy();
    let mut r = p.read_json("responses.json");
    r["extract"]["uncertainties"] = json!([
      "The document does not demonstrate a live deployment.",
      "Refund responsibility is undecided."
    ]);
    if validation {
      for d in r["extract"]["decisions"].as_array_mut().unwrap() {
        d["lineEnd"] = json!(999);
      }
    }
    p.json("responses.json", &r);
    let paused = p.model_cli(&["update", "--max-calls", "1"]);
    let warnings = p.ok(&["warnings", "--all"]);
    let id = warnings["warnings"][0]["id"].clone();
    r["check"]["warningResolutions"] = json!([{"id":id,"reason":"A retention rule does not assert deployment.","evidence":[{"document":"cache.md","lineStart":3,"lineEnd":3}]}]);
    p.json("responses.json", &r);
    let result = p.model_cli(&["update", "--max-calls", "2"]);
    assert_eq!(result["work"]["id"], paused["work"]["id"]);
    assert_eq!(result["work"]["calls"], 2);
    if validation {
      assert_eq!(result["warningChanges"]["resolved"], json!([]));
      assert_eq!(result["warningSummary"]["resolved"], 0);
      assert!(result["warningSummary"]["validation"].as_u64().unwrap() > 0);
    } else {
      assert_eq!(result["warningSummary"]["resolved"], 1);
      assert_eq!(list(&result["warningChanges"], "new").len(), 2);
      assert_eq!(result["warningChanges"]["resolved"][0]["id"], id);
      assert_eq!(
        p.model_cli(&["update", "--max-calls", "2"])["work"]["calls"],
        2
      );
    }
  }
}

#[test]
fn updated_sources_revalidate_closures_only_without_conflicting_findings() {
  for finding in [false, true] {
    let p = Project::policy();
    fs::remove_file(p.path("privacy.md")).unwrap();
    let mut r = p.read_json("responses.json");
    r["extract"]["decisions"] = json!([r["extract"]["decisions"][0].clone()]);
    r["extract"]["relationships"] = json!([]);
    r["extract"]["uncertainties"] = json!(["No production deployment evidence is provided."]);
    p.json("responses.json", &r);
    p.model_cli(&["update"]);
    let id = p.ok(&["warnings", "--all"])["warnings"][0]["id"].clone();
    let original = json!({"reason":"The document specifies retention, not delivery evidence.","evidence":[{"document":"cache.md","lineStart":3,"lineEnd":3,"version":p.graph()["documents"]["cache.md"]}]});
    let mut entry = original.clone();
    entry["id"] = id.clone();
    p.json("resolutions.json", &json!([entry]));
    p.ok(&[
      "warnings",
      "--resolve",
      p.path("resolutions.json").to_str().unwrap(),
    ]);
    p.write(
      "cache.md",
      format!(
        "# Cache\n\nCached data expires after seven days.\n\n{}\n",
        if finding {
          "Amendment: regulated records must be retained for thirty days."
        } else {
          "Examples use synthetic records."
        }
      ),
    );
    r["extract"]["uncertainties"] = json!([]);
    r["check"]["findings"] = if finding {
      json!([{"target":"batch","reason":"The later amendment changes retention."}])
    } else {
      json!([])
    };
    r["check"]["warningResolutions"] = json!([{"id":id,"reason":"The updated document still does not require deployment proof.","evidence":[{"document":"cache.md","lineStart":3,"lineEnd":5}]}]);
    p.json("responses.json", &r);
    let result = p.model_cli(&["update"]);
    let graph = p.graph();
    let current = list(&graph, "warnings")
      .iter()
      .find(|w| w["message"] == "No production deployment evidence is provided.")
      .unwrap();
    if finding {
      assert_eq!(current["resolution"], original);
      assert_eq!(result["warningChanges"]["reopened"][0]["id"], id);
      assert_eq!(result["warningSummary"]["resolved"], 0);
    } else {
      assert_eq!(current["previousResolutions"], json!([original]));
      assert_eq!(result["warningChanges"]["reopened"], json!([]));
      assert_eq!(result["warningSummary"]["resolved"], 1);
    }
  }
}

fn assert_relocation(kind: &str) {
  let p = checked();
  let old = p.graph();
  let original_id = old["decisions"][0]["id"].clone();
  if kind == "mixed-version" {
    let mut snapshot = p.read_json(".hivex/graph.json");
    snapshot["warnings"].as_array_mut().unwrap().push(json!({"message":"A retained observation used an earlier version.","scope":[{"document":"cache.md","lineStart":1,"lineEnd":3,"version":"0".repeat(64)}]}));
    p.json(".hivex/graph.json", &snapshot);
    p.ok(&["snapshot", "import"]);
  }
  if kind == "missing-version" {
    let mut snapshot = p.read_json(".hivex/graph.json");
    snapshot["relationships"][0]["evidence"][0]
      .as_object_mut()
      .unwrap()
      .remove("version");
    p.json(".hivex/graph.json", &snapshot);
    p.ok(&["snapshot", "import"]);
  }
  fs::remove_file(p.path("cache.md")).unwrap();
  let destination = if kind == "existing-target" {
    "privacy.md"
  } else {
    "docs/cache.md"
  };
  if kind != "existing-target" {
    p.write(
      destination,
      if kind == "changed" {
        "# Cache\n\nCached data expires after thirty days.\n"
      } else {
        "# Cache\n\nCached data expires after seven days.\n"
      },
    );
  }
  let relocated = p.ok(&["snapshot", "relocate", "cache.md", destination]);
  assert_eq!(relocated["modelCalls"], 0);
  assert_eq!(relocated["reused"], kind == "identical");
  let graph = p.graph();
  assert!(
    list(&graph, "decisions")
      .iter()
      .any(|d| d["id"] == original_id && d["document"] == destination)
  );
  if kind == "identical" {
    assert!(list(&relocated, "pendingUnits").is_empty());
    assert_eq!(
      p.ok(&["update", "--max-calls", "0", "--codex", "/no-model"])["status"],
      "ready"
    );
  } else {
    assert!(!list(&relocated, "pendingUnits").is_empty());
  }
  assert_eq!(p.calls(), 2);
  if kind == "existing-target" {
    for d in list(&old, "decisions") {
      assert!(
        list(&graph, "decisions")
          .iter()
          .any(|entry| entry["id"] == d["id"])
      );
    }
    assert!(graph["documents"].get("cache.md").is_none());
    assert!(graph["documents"].get("privacy.md").is_none());
  }
  if kind == "missing-version" {
    p.ok(&["snapshot", "export"]);
    assert!(
      p.read_json(".hivex/graph.json")["relationships"][0]["evidence"][0]
        .get("version")
        .is_none()
    );
  }
  if kind == "changed" {
    assert_reingestion_uses_relocated_evidence(&p, &old);
  }
}

fn assert_reingestion_uses_relocated_evidence(p: &Project, old: &Value) {
  let mut r = p.read_json("responses.json");
  let mut d = r["extract"]["decisions"][0].clone();
  d["document"] = json!("docs/cache.md");
  d["text"] = json!("Cached data expires after thirty days.");
  let mut edge = r["extract"]["relationships"][0].clone();
  edge["from"] = list(old, "decisions")
    .iter()
    .find(|d| d["document"] == "privacy.md")
    .unwrap()["id"]
    .clone();
  edge["evidence"][0]["document"] = json!("docs/cache.md");
  r["extract"]["decisions"] = json!([d]);
  r["extract"]["relationships"] = json!([edge]);
  p.json("responses.json", &r);
  p.write("responses.json.packets", "");
  let updated = p.model_cli(&["update"]);
  assert_eq!(updated["warnings"], json!([]));
  let text = fs::read_to_string(p.path("responses.json.packets")).unwrap();
  let packet: Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
  assert!(!packet.to_string().contains("\"document\":\"cache.md\""));
  assert_eq!(
    packet["previousRelationships"][0]["evidence"][0]["document"],
    "docs/cache.md"
  );
  assert_eq!(
    packet["previousRelationships"][0]["evidence"][1]["document"],
    "privacy.md"
  );
}
