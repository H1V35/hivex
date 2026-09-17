use crate::support::{Project, decision, empty_graph, list, subset};
use serde_json::{Value, json};
use std::fmt::Write;
use std::fs;

fn packets(p: &Project) -> Vec<Value> {
  fs::read_to_string(p.path("responses.json.packets"))
    .unwrap_or_default()
    .lines()
    .map(|line| serde_json::from_str(line).unwrap())
    .collect()
}
fn preserving_repair(p: &Project) -> Value {
  let mut r = p.read_json("responses.json");
  let d = r["extract"]["decisions"][0].clone();
  let mut edge = r["extract"]["relationships"][0].clone();
  edge["from"] = json!("@existing:privacy.md");
  r["byDocument"] = json!({"cache.md":{"decisions":[d],"relationships":[edge]}});
  r["check"] = json!({"findings":[],"relationshipChanges":[{"previousId":"@removed:0","replacements":["@candidate:0"],"reason":"Retain the revocation exception.","evidence":[{"document":"privacy.md","lineStart":3,"lineEnd":3}]}]});
  r
}
const REPAIR: [&str; 5] = [
  "update",
  "--repair-range",
  "cache.md:3-3",
  "--reason",
  "Clarify the ordinary cache lifetime.",
];

#[test]
fn repair_corrects_interpretation_without_changing_sources_or_repeating_completed_work() {
  let p = Project::policy();
  let text = fs::read(p.path("cache.md")).unwrap();
  let mut r = p.read_json("responses.json");
  r["extract"]["decisions"][0]["text"] = json!("Cached data never expires.");
  r["check"]["findings"] =
    json!([{"target":"c1","reason":"The source says seven days, not forever."}]);
  p.json("responses.json", &r);
  assert_eq!(p.model_cli(&["update"])["status"], "partial");
  let mut r = preserving_repair(&p);
  r["byDocument"]["cache.md"]["decisions"][0]["text"] =
    json!("Cached data expires after seven days.");
  p.json("responses.json", &r);
  let repaired = p.model_cli(&REPAIR);
  subset(&repaired, &json!({"status":"ready","work":{"calls":2}}));
  assert_eq!(p.model_cli(&REPAIR)["work"]["id"], repaired["work"]["id"]);
  assert_eq!(p.calls(), 4);
  assert_eq!(fs::read(p.path("cache.md")).unwrap(), text);
  let found = p.ok(&["search", "expires"]);
  assert!(!found["decisions"].to_string().contains("never expires"));
  assert!(
    found["decisions"]
      .to_string()
      .contains("Cached data expires after seven days.")
  );
}

#[test]
fn staged_or_rejected_relationship_repair_cannot_replace_the_current_graph() {
  for rejected in [false, true] {
    let p = Project::policy();
    p.model_cli(&["update"]);
    let before = p.graph();
    let mut r = preserving_repair(&p);
    if rejected {
      r["byDocument"]["cache.md"]["decisions"][0]["text"] = json!("Cached data never expires.");
      r["check"]["findings"] =
        json!([{"target":"c1","reason":"The lifetime contradicts the seven-day source rule."}]);
    } else {
      r["byDocument"]["cache.md"]["relationships"][0]["from"] = json!("not-supplied");
      r["check"]["relationshipChanges"] = json!([]);
    }
    p.json("responses.json", &r);
    p.write("responses.json.packets", "");
    let mut args = REPAIR.to_vec();
    args.extend(["--max-calls", "1"]);
    let first = p.model_cli(&args);
    assert_eq!(first["status"], "budget-exhausted");
    assert_eq!(p.graph(), before);
    args.pop();
    args.push("2");
    let checked = p.model_cli(&args);
    subset(
      &checked,
      &json!({"status":"failed","relationships":1,"work":{"calls":2,"lastAttempt":{"code":"RELATIONSHIP_LOSS"}}}),
    );
    assert_eq!(checked["work"]["id"], first["work"]["id"]);
    assert_eq!(p.graph(), before);
    assert_eq!(p.model_cli(&REPAIR)["work"]["calls"], 2);
    if !rejected {
      let captured = packets(&p);
      let check = captured.iter().find(|v| v["operation"] == "check").unwrap();
      assert_eq!(check["extraction"]["relationships"], json!([]));
      assert_eq!(list(check, "removedRelationships").len(), 1);
      assert_eq!(list(check, "previousDecisions").len(), 2);
      assert_eq!(list(check, "validationWarnings").len(), 1);
      let mut retry = REPAIR.to_vec();
      retry.extend(["--retry-failed", "--max-calls", "0"]);
      let reassessed = p.model_cli(&retry);
      subset(
        &reassessed,
        &json!({"status":"failed","work":{"calls":2,"retainedCheckAssessment":"blocked"}}),
      );
    }
    assert_eq!(p.calls(), 4);
  }
}

#[test]
fn retained_check_reassessment_requires_matching_candidate_and_graph() {
  for state in ["fresh", "different-candidate", "intervening-update"] {
    let p = Project::policy();
    let mut r = p.read_json("responses.json");
    r["check"]["findings"] =
      json!([{"target":"c2","reason":"Eviction acknowledgements are not described."}]);
    p.json("responses.json", &r);
    assert_eq!(p.model_cli(&["update"])["status"], "partial");
    let mut before = p.graph();
    let mut r = preserving_repair(&p);
    r["byDocument"]["cache.md"]["decisions"][0]["text"] =
      json!("Expire ordinary cached data after seven days.");
    p.json("responses.json", &r);
    let mut args = REPAIR.to_vec();
    args.extend(["--max-calls", "1"]);
    let first = p.model_cli(&args);
    let id = first["work"]["id"].as_str().unwrap();
    let pending = p.work(id);
    args.pop();
    args.push("2");
    assert_eq!(p.model_cli(&args)["status"], "partial");
    let mut retained = p.work(id);
    retained["pending"] = pending["pending"].clone();
    retained["remaining"] = pending["remaining"].clone();
    retained["status"] = json!("failed");
    retained["attempts"][1]["error"] = json!("RELATIONSHIP_LOSS");
    if state == "different-candidate" {
      retained["pending"]["extraction"]["decisions"][0]["text"] =
        json!("A changed candidate cannot reuse the old check.");
    }
    if state == "intervening-update" {
      before["lastExtraction"] = json!("intervening-update");
    }
    p.set_work(&retained);
    p.set_graph(&before);
    let mut retry = REPAIR.to_vec();
    retry.extend([
      "--retry-failed",
      "--max-calls",
      "0",
      "--codex",
      "/model-must-not-start",
    ]);
    if state == "fresh" {
      let result = p.cli(&retry);
      subset(
        &result,
        &json!({"status":"partial","work":{"calls":2,"retainedCheckAssessment":"accepted"}}),
      );
      assert_eq!(p.cli(&retry)["work"]["calls"], 2);
      assert_eq!(p.graph()["relationships"][0]["quality"], "uncertain");
      assert_eq!(p.work(id)["attempts"][1]["error"], "RELATIONSHIP_LOSS");
    } else {
      assert_eq!(p.error(&retry)["error"]["code"], "STALE_RETAINED_CHECK");
      assert_eq!(p.graph(), before);
    }
    assert_eq!(p.calls(), 4);
  }
}

#[test]
fn range_repair_expands_full_citation_and_preserves_unrelated_neighbors() {
  let p = Project::new();
  p.write("rules.md","# Rules\n\nTarget decision starts here.\nTarget decision continues with its condition.\nTarget decision ends with its scope.\nNeighbor rule stays in the same source block.\n");
  p.write(
    "authority.md",
    "# Authority\n\nThe authority remains independent.\n",
  );
  p.model("");
  let mut r = p.read_json("responses.json");
  let mut target = decision(
    "rules.md",
    "target",
    3,
    "Target decision covers its complete three-line passage.",
  );
  target["lineEnd"] = json!(5);
  let neighbor = decision(
    "rules.md",
    "neighbor",
    6,
    "Neighbor rule stays in the same source block.",
  );
  let authority = decision(
    "authority.md",
    "authority",
    3,
    "The authority remains independent.",
  );
  r["byDocument"] = json!({"authority.md":{"decisions":[authority],"relationships":[]},"rules.md":{"decisions":[target,neighbor],"relationships":[{"id":"neighbor-authority","from":"neighbor","to":"authority","type":"requires","reason":"Neighbor uses the independent authority.","evidence":[{"document":"rules.md","lineStart":6,"lineEnd":6},{"document":"authority.md","lineStart":3,"lineEnd":3}]}]}});
  p.json("responses.json", &r);
  assert_eq!(p.model_cli(&["update"])["status"], "ready");
  let before = p.graph();
  target["text"] = json!("Target decision is corrected from its source.");
  r["byDocument"]["rules.md"] = json!({"decisions":[target],"relationships":[]});
  p.json("responses.json", &r);
  p.write("responses.json.packets", "");
  let result = p.model_cli(&[
    "update",
    "--repair-range",
    "rules.md:4-4",
    "--reason",
    "Check the complete citation.",
  ]);
  assert_eq!(result["status"], "ready");
  assert_eq!(packets(&p)[0]["units"][0]["id"], "rules.md:3-5");
  let after = p.graph();
  for d in list(&before, "decisions")
    .iter()
    .filter(|d| d["localId"] != "target")
  {
    assert!(list(&after, "decisions").contains(d));
  }
  assert_eq!(after["relationships"], before["relationships"]);
  p.write(
    "oversized.md",
    format!(
      "# Oversized\n\n{}\n{}\n{}\n",
      "a".repeat(6000),
      "b".repeat(6000),
      "c".repeat(6000)
    ),
  );
  let source = p.ok(&["read", "oversized.md"])["source"].clone();
  let mut graph = empty_graph();
  let mut d = decision("oversized.md", "oversized", 3, "Whole passage.");
  d["lineEnd"] = json!(5);
  d["version"] = source["hash"].clone();
  d["localId"] = json!("oversized");
  d["quality"] = json!("checked");
  d["batch"] = json!("synthetic");
  graph["decisions"] = json!([d]);
  graph["documents"]["oversized.md"] = source["hash"].clone();
  p.set_graph(&graph);
  assert_eq!(
    p.error(&[
      "update",
      "--repair-range",
      "oversized.md:4-4",
      "--reason",
      "Check full citation.",
      "--codex",
      "/no-model"
    ])["error"]["code"],
    "REPAIR_RANGE_TOO_LARGE"
  );
  assert_eq!(p.graph(), graph);
}

#[test]
fn legacy_full_unit_repair_resumes_from_precise_range_without_reextracting() {
  let p = Project::new();
  p.write(
    "legacy.md",
    "# Legacy\n\nRule 1 requires bounded retention.\nRule 2 remains outside the repair.\n",
  );
  p.model("");
  let mut r = p.read_json("responses.json");
  r["byDocument"] = json!({"legacy.md":{"decisions":[decision("legacy.md","one",3,"Rule 1 requires bounded retention."),decision("legacy.md","two",4,"Rule 2 remains outside the repair.")],"relationships":[]}});
  p.json("responses.json", &r);
  p.model_cli(&["update"]);
  let range = [
    "update",
    "--repair-range",
    "legacy.md:3-3",
    "--reason",
    "Check Rule 1 against its source.",
  ];
  let mut args = range.to_vec();
  args.extend(["--max-calls", "0"]);
  let zero = p.model_cli(&args);
  let key = p.work(zero["work"]["id"].as_str().unwrap())["key"].clone();
  let full = p.model_cli(&[
    "update",
    "--repair",
    "legacy.md",
    "--reason",
    "Check Rule 1 against its source.",
    "--max-calls",
    "1",
  ]);
  let id = full["work"]["id"].as_str().unwrap();
  let mut retained = p.work(id);
  let original_unit = retained["plannedUnits"][0].clone();
  retained["key"] = key;
  p.set_work(&retained);
  p.write("responses.json.packets", "");
  args.pop();
  args.push("2");
  let done = p.model_cli(&args);
  assert_eq!(done["status"], "ready");
  assert_eq!(done["work"]["id"], id);
  assert_eq!(done["work"]["calls"], 2);
  let captured = packets(&p);
  assert_eq!(captured.len(), 1);
  assert_eq!(captured[0]["operation"], "check");
  assert_eq!(captured[0]["units"][0]["id"], original_unit);
  assert!(captured[0].get("ranges").is_none());
}

#[test]
fn explicit_update_context_is_complete_without_reingesting_it() {
  let p = Project::new();
  p.model("");
  let mut r = p.read_json("responses.json");
  r["byDocument"] = json!({});
  for index in 0..8 {
    let file = format!("0{index}-rule.md");
    let text = format!("Independent rule {index} applies locally.");
    p.write(&file, format!("# Rule\n\n{text}\n"));
    r["byDocument"][&file] =
      json!({"decisions":[decision(&file,&format!("d{index}"),3,&text)],"relationships":[]});
  }
  p.json("responses.json", &r);
  assert_eq!(
    p.model_cli(&["update", "--max-calls", "8"])["status"],
    "ready"
  );
  p.write("00-rule.md", "# Rule\n\nRevision beta applies locally.\n");
  r["byDocument"]["00-rule.md"]["decisions"][0]["text"] = json!("Revision beta applies locally.");
  r["byDocument"]["00-rule.md"]["relationships"] = json!([{"id":"authority","from":"d0","to":"@existing:01-rule.md","type":"requires","reason":"Target depends on authority.","evidence":[{"document":"00-rule.md","lineStart":3,"lineEnd":3},{"document":"01-rule.md","lineStart":3,"lineEnd":3}]}]);
  p.json("responses.json", &r);
  p.write("responses.json.packets", "");
  let first = p.model_cli(&["update", "--source", "01-rule.md", "--max-calls", "1"]);
  let done = p.model_cli(&["update", "--source", "01-rule.md", "--max-calls", "2"]);
  assert_eq!(done["status"], "ready");
  assert_eq!(done["work"]["id"], first["work"]["id"]);
  let captured = packets(&p);
  assert_eq!(captured[0]["targets"], json!(["00-rule.md"]));
  assert!(
    list(&captured[0], "documents")
      .iter()
      .any(|d| d["id"] == "01-rule.md")
  );
  assert!(
    list(&captured[0], "existing")
      .iter()
      .any(|d| d["document"] == "01-rule.md")
  );
  assert_eq!(list(&p.graph(), "decisions").len(), 8);
  assert_eq!(list(&p.graph(), "relationships").len(), 1);
  let other = p.model_cli(&["update", "--source", "02-rule.md", "--max-calls", "0"]);
  assert_eq!(other["status"], "ready");
  assert_ne!(other["work"]["id"], done["work"]["id"]);
  let p = Project::new();
  p.write(
    "notes.md",
    (0..8)
      .map(|n| format!("# Section {n}\n\nRule {n}. {}\n", "Detail ".repeat(800)))
      .collect::<Vec<_>>()
      .join("\n"),
  );
  p.model("");
  let mut r = p.read_json("responses.json");
  r["byDocument"] = json!({"notes.md":{"decisions":[],"relationships":[]}});
  p.json("responses.json", &r);
  assert_eq!(
    p.model_cli(&["update", "--source", "notes.md", "--max-calls", "1"])["status"],
    "budget-exhausted"
  );
  let packet = &packets(&p)[0];
  let doc = &packet["documents"][0];
  assert_eq!(
    list(doc, "lines").len() as u64,
    doc["lineCount"].as_u64().unwrap()
  );
  assert!(packet["units"][0]["lineEnd"].as_u64().unwrap() < doc["lineCount"].as_u64().unwrap());
}

#[test]
fn portable_failed_relationship_fixture_resumes_check_without_reextracting() {
  let p = Project::new();
  p.write(
    "scope.md",
    "# Scope\n\nAlpha uses Beta.\nBeta provides shared policy.\n",
  );
  p.sql_fixture("retained-endpoint-check.sql");
  let id: String = p
    .db()
    .query_row("SELECT id FROM work", [], |row| row.get(0))
    .unwrap();
  let before = p.work(&id);
  let graph = p.graph();
  p.model("");
  let mut r = p.read_json("responses.json");
  r["check"] = json!({"findings":[],"relationshipChanges":[{"previousId":"@removed:0","replacements":["@candidate:0"],"reason":"Corrected relationship is supported by both definitions.","evidence":[{"document":"scope.md","lineStart":3,"lineEnd":4}]}]});
  p.json("responses.json", &r);
  let args = [
    "update",
    "--repair-range",
    "scope.md:3-4",
    "--reason",
    "Correct the Alpha to Beta dependency.",
    "--retry-failed",
    "--max-calls",
    "2",
  ];
  let exhausted = p.model_cli(&args);
  subset(
    &exhausted,
    &json!({"status":"budget-exhausted","pendingCheck":["scope.md"],"pendingUnits":["scope.md:3-4"],"work":{"calls":2,"id":id}}),
  );
  assert_eq!(p.calls(), 0);
  assert_eq!(p.work(&id)["attempts"], before["attempts"]);
  let mut args = args.to_vec();
  args.pop();
  args.push("3");
  let repaired = p.model_cli(&args);
  subset(
    &repaired,
    &json!({"status":"ready","decisions":2,"relationships":1,"work":{"calls":3,"id":id}}),
  );
  assert_eq!(p.calls(), 1);
  let after = p.work(&id);
  assert_eq!(
    &list(&after, "attempts")[..2],
    &list(&before, "attempts")[..]
  );
  assert_eq!(after["attempts"][2]["stage"], "check");
  let captured = packets(&p);
  assert_eq!(captured.len(), 1);
  assert_eq!(captured[0]["operation"], "check");
  for d in list(&graph, "decisions") {
    assert!(
      list(&captured[0], "previousDecisions")
        .iter()
        .any(|entry| entry["id"] == d["id"])
    );
  }
  assert!(
    !p.graph()["warnings"]
      .to_string()
      .contains("RELATIONSHIP_LOSS")
  );
}

#[test]
fn range_repair_keeps_untouched_units_and_rejects_changed_authority() {
  let p = Project::new();
  let mut text = String::new();
  for n in 0..24 {
    write!(
      text,
      "Rule {} requires bounded retention. {}\n\n",
      n + 1,
      "Background context. ".repeat(40)
    )
    .unwrap();
  }
  p.write("large.md", &text);
  p.model("");
  let mut r = p.read_json("responses.json");
  r["fromVisibleRules"] = json!(true);
  p.json("responses.json", &r);
  assert_eq!(
    p.model_cli(&[
      "update",
      "--max-calls",
      "12",
      "--max-input-bytes",
      "1048576"
    ])["status"],
    "ready"
  );
  let original = p.graph();
  p.write("responses.json.packets", "");
  let args = [
    "update",
    "--repair-range",
    "large.md:1-1",
    "--reason",
    "Check first rule.",
    "--max-calls",
    "1",
  ];
  let first = p.model_cli(&args);
  let mut args = args.to_vec();
  args.pop();
  args.push("2");
  let done = p.model_cli(&args);
  assert_eq!(done["status"], "ready");
  assert_eq!(done["work"]["id"], first["work"]["id"]);
  let captured = packets(&p);
  assert_eq!(captured.len(), 2);
  assert_eq!(captured[0]["units"][0]["id"], "large.md:1-1");
  let after = p.graph();
  for d in list(&original, "decisions")
    .iter()
    .filter(|d| d["lineStart"] != 1)
  {
    assert!(list(&after, "decisions").contains(d));
  }
  assert_eq!(p.model_cli(&args)["work"]["calls"], 2);
  p.write("large.md", format!("{text}Changed authority.\n"));
  assert_eq!(p.error(&args)["error"]["code"], "SOURCE_NOT_CURRENT");
}

#[test]
fn unchanged_current_endpoints_survive_identical_relationship_repair() {
  let p = Project::new();
  p.write(
    "scope.md",
    "# Scope\n\nAlpha uses Beta.\nBeta provides shared policy.\n",
  );
  p.model("");
  let mut r = p.read_json("responses.json");
  let edge = json!({"id":"alpha-beta","from":"alpha","to":"beta","type":"requires","reason":"Alpha uses Beta through shared policy.","evidence":[{"document":"scope.md","lineStart":3,"lineEnd":3},{"document":"scope.md","lineStart":4,"lineEnd":4}]});
  r["extract"] = json!({"decisions":[decision("scope.md","alpha",3,"Alpha uses Beta."),decision("scope.md","beta",4,"Beta provides shared policy.")],"relationships":[edge],"uncertainties":[]});
  p.json("responses.json", &r);
  p.model_cli(&["update"]);
  let graph = p.graph();
  let mut edge = edge;
  edge["from"] = graph["decisions"][0]["id"].clone();
  edge["to"] = graph["decisions"][1]["id"].clone();
  r["extract"] = json!({"decisions":[],"relationships":[edge],"uncertainties":[]});
  p.json("responses.json", &r);
  let repaired = p.model_cli(&[
    "update",
    "--repair-range",
    "scope.md:3-4",
    "--reason",
    "Retain dependency.",
  ]);
  subset(
    &repaired,
    &json!({"status":"ready","decisions":2,"relationships":1,"warningSummary":{"validation":0}}),
  );
  let captured = packets(&p);
  let check = captured.last().unwrap();
  for d in list(&graph, "decisions") {
    assert!(
      list(check, "previousDecisions")
        .iter()
        .any(|entry| entry["id"] == d["id"])
    );
  }
  assert!(list(check, "validationWarnings").is_empty());
}
