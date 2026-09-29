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
pub(super) fn preserving_repair(p: &Project) -> Value {
  let mut r = p.read_json("responses.json");
  let d = r["extract"]["decisions"][0].clone();
  let mut edge = r["extract"]["relationships"][0].clone();
  edge["from"] = json!("@existing:privacy.md");
  r["byDocument"] = json!({"cache.md":{"decisions":[d],"relationships":[edge]}});
  r["check"] = json!({"findings":[],"relationshipChanges":[{"previousId":"@removed:0","replacements":["@candidate:0"],"reason":"Retain the revocation exception.","evidence":[{"document":"privacy.md","lineStart":3,"lineEnd":3}]}]});
  r
}
pub(super) const REPAIR: [&str; 5] = [
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
  let captured = packets(&p);
  let extract = captured
    .iter()
    .rev()
    .find(|packet| packet["operation"] == "extract")
    .unwrap();
  let replacing = list(extract, "replacingDecisions");
  assert_eq!(replacing.len(), 1);
  assert_eq!(replacing[0]["text"], "Cached data never expires.");
  let check = captured.last().unwrap();
  assert!(check.get("replacingDecisions").is_none());
  let old = list(check, "previousDecisions")
    .iter()
    .find(|entry| entry["id"] == replacing[0]["id"])
    .unwrap();
  assert_eq!(old["retainedInCandidate"], false);
  assert!(
    list(&check["extraction"], "decisions")
      .iter()
      .all(|entry| entry["id"] != old["id"])
  );
  assert!(
    extract["documents"]
      .to_string()
      .contains("after seven days")
  );
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
fn explicit_repair_ranges_share_one_byte_bounded_round_and_resume_accounting() {
  let p = Project::new();
  p.model("");
  let mut responses = p.read_json("responses.json");
  let mut text = String::from("# Rules\n\n");
  let mut decisions = Vec::new();
  let mut ranges = Vec::new();
  for index in 0..6 {
    let line = 3 + index * 2;
    let rule = format!("Rule {index} applies independently.");
    text.push_str(&rule);
    text.push_str("\n\n");
    decisions.push(decision("rules.md", &format!("rule-{index}"), line, &rule));
    ranges.push(format!("rules.md:{line}-{line}"));
  }
  p.write("rules.md", text);
  responses["byDocument"] = json!({"rules.md":{"decisions":decisions,"relationships":[]}});
  p.json("responses.json", &responses);
  assert_eq!(p.model_cli(&["update"])["status"], "ready");
  let mut args = vec!["update", "--reason", "Review the six independent rules."];
  for range in &ranges {
    args.extend(["--repair-range", range.as_str()]);
  }
  args.extend(["--max-calls", "1"]);
  let extracted = p.model_cli(&args);
  assert_eq!(extracted["status"], "budget-exhausted");
  assert_eq!(extracted["work"]["calls"], 1);
  let last = args.last_mut().unwrap();
  *last = "2";
  let checked = p.model_cli(&args);
  assert_eq!(checked["status"], "ready");
  assert_eq!(checked["work"]["id"], extracted["work"]["id"]);
  assert_eq!(checked["work"]["calls"], 2);
  assert_eq!(checked["decisions"], 6);
  assert!(list(&checked, "pendingUnits").is_empty());
  let captured = packets(&p);
  assert_eq!(list(&captured[captured.len() - 2], "units").len(), 6);
}

#[test]
fn explicit_repair_ranges_still_respect_the_target_byte_limit() {
  let p = Project::new();
  p.model("");
  let mut responses = p.read_json("responses.json");
  responses["byDocument"] = json!({});
  let mut ranges = Vec::new();
  for index in 0..6 {
    let file = format!("rule-{index}.md");
    let text = format!("Rule {index}. {}", "Detail ".repeat(430));
    p.write(&file, format!("# Rule\n\n{text}\n"));
    responses["byDocument"][&file] = json!({
      "decisions":[decision(&file,&format!("r{index}"),3,&format!("Rule {index}."))],
      "relationships":[]
    });
    ranges.push(format!("{file}:3-3"));
  }
  p.json("responses.json", &responses);
  assert_eq!(
    p.model_cli(&["update", "--max-calls", "4"])["status"],
    "ready"
  );
  let mut args = vec!["update", "--reason", "Review the six long rules."];
  for range in &ranges {
    args.extend(["--repair-range", range.as_str()]);
  }
  args.extend(["--max-calls", "2"]);
  let result = p.model_cli(&args);
  assert_eq!(result["status"], "budget-exhausted");
  assert_eq!(list(&result, "pendingUnits").len(), 1);
  let captured = packets(&p);
  assert_eq!(list(&captured[captured.len() - 2], "units").len(), 5);
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
fn a_finding_on_removed_context_cannot_bypass_the_relationship_guard() {
  let p = Project::policy();
  p.model_cli(&["update"]);
  let before = p.graph();
  let target = list(&before, "decisions")
    .iter()
    .find(|entry| entry["document"] == "cache.md")
    .unwrap();
  let mut r = preserving_repair(&p);
  r["byDocument"]["cache.md"]["decisions"][0]["text"] =
    json!("Ordinary cached data expires after seven days.");
  r["check"]["findings"] = json!([{"target":target["id"],"reason":"A simulated checker incorrectly targets old context."}]);
  p.json("responses.json", &r);
  let result = p.model_cli(&REPAIR);
  subset(
    &result,
    &json!({"status":"failed","work":{"calls":2,"lastAttempt":{"code":"RELATIONSHIP_LOSS"}}}),
  );
  assert_eq!(p.graph(), before);
  let captured = packets(&p);
  let check = captured.last().unwrap();
  assert!(check.get("replacingDecisions").is_none());
  assert!(
    list(check, "previousDecisions")
      .iter()
      .any(|entry| { entry["id"] == target["id"] && entry["retainedInCandidate"] == false })
  );
}

#[test]
fn unchanged_current_endpoints_survive_identical_relationship_repair() {
  for connected in [false, true] {
    let p = Project::new();
    p.write(
      "scope.md",
      "# Scope\n\nAlpha uses Beta.\nBeta provides shared policy.\n",
    );
    p.model("");
    let mut r = p.read_json("responses.json");
    let edge = json!({"id":"alpha-beta","from":"alpha","to":"beta","type":"requires","reason":"Alpha uses Beta through shared policy.","evidence":[{"document":"scope.md","lineStart":3,"lineEnd":3},{"document":"scope.md","lineStart":4,"lineEnd":4}]});
    r["extract"] = json!({"decisions":[decision("scope.md","alpha",3,"Alpha uses Beta."),decision("scope.md","beta",4,"Beta provides shared policy.")],"relationships":[edge],"uncertainties":[]});
    if !connected {
      r["extract"]["relationships"] = json!([]);
    }
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
    let extract = captured
      .iter()
      .rev()
      .find(|packet| packet["operation"] == "extract")
      .unwrap();
    assert!(list(extract, "existing").is_empty());
    let replacing = list(extract, "replacingDecisions");
    assert_eq!(replacing.len(), 2);
    for decision in list(&graph, "decisions") {
      let mut definition = decision.clone();
      for key in ["batch", "quality", "localId"] {
        definition.as_object_mut().unwrap().shift_remove(key);
      }
      assert!(replacing.contains(&definition));
    }
    let check = captured.last().unwrap();
    assert!(check.get("replacingDecisions").is_none());
    for d in list(&graph, "decisions") {
      assert!(
        list(check, "previousDecisions")
          .iter()
          .any(|entry| entry["id"] == d["id"] && entry["retainedInCandidate"] == true)
      );
    }
    assert!(list(check, "validationWarnings").is_empty());
  }
}

pub(super) fn rejected_candidate_resolution(case: &str) -> (Project, Value) {
  let p = Project::policy();
  p.model_cli(&["update"]);
  let graph = p.graph();
  let node = list(&graph, "decisions")
    .iter()
    .find(|node| node["document"] == "privacy.md")
    .unwrap();
  let target = match case {
    "batch" | "batch-missing-edge" => json!("batch"),
    "document" => json!("privacy.md"),
    "unsupplied-document" => json!("unrelated.md"),
    "unknown" => json!("missing-decision"),
    _ => node["id"].clone(),
  };
  let mut r = preserving_repair(&p);
  r["byDocument"]["cache.md"]["decisions"][0]["text"] =
    json!("Expire ordinary cached data after seven days.");
  r["check"]["findings"] = json!([{"target":target,"reason":"The reviewer verifies this precise interpretation against its source."}]);
  if case == "other-finding" {
    r["check"]["findings"]
      .as_array_mut()
      .unwrap()
      .push(json!({"target":"batch","reason":"A separate unresolved batch defect."}));
  }
  if ["missing-edge", "batch-missing-edge"].contains(&case) {
    r["check"]["relationshipChanges"] = json!([]);
  }
  if case == "duplicates" {
    let mut edge = r["byDocument"]["cache.md"]["relationships"][0].clone();
    edge["id"] = json!("duplicate-edge");
    r["byDocument"]["cache.md"]["relationships"]
      .as_array_mut()
      .unwrap()
      .push(edge);
    let mut node = r["byDocument"]["cache.md"]["decisions"][0].clone();
    node["id"] = json!("duplicate-node");
    r["byDocument"]["cache.md"]["decisions"]
      .as_array_mut()
      .unwrap()
      .push(node);
  }
  p.json("responses.json", &r);
  if case == "unsupplied-document" {
    p.write("unrelated.md", "# Unrelated\n\nAnother topic.\n");
  }
  let rejected = p.model_cli(&REPAIR);
  assert_eq!(rejected["status"], "failed");
  let mut resolution = rejected["candidateResolutionContext"].clone();
  resolution["resolutions"] = json!([{
    "id":rejected["pendingCandidateWarnings"][0]["id"],
    "reason":"Independent review confirms immediate revocation in privacy.md:3.",
    "evidence":[{"document":"privacy.md","lineStart":3,"lineEnd":3,"version":node["version"]}]
  }]);
  p.json("resolution.json", &resolution);
  (p, rejected)
}

fn correction_file(p: &Project, rejected: &Value) -> Value {
  let work = p.work(rejected["work"]["id"].as_str().unwrap());
  let mut corrected = work["pending"]["extraction"]["decisions"][0].clone();
  corrected["text"] = json!("Ordinary cached data expires after seven days.");
  let mut correction = rejected["candidateResolutionContext"].clone();
  correction["reason"] = json!("Restore the complete source-backed cache lifetime.");
  correction["evidence"] = p.read_json("resolution.json")["resolutions"][0]["evidence"].clone();
  correction["decisions"] = json!([corrected]);
  p.json("correction.json", &correction);
  correction
}

#[test]
fn candidate_correction_requires_a_fresh_check_and_preserves_history_and_budget() {
  let (p, rejected) = rejected_candidate_resolution("valid");
  correction_file(&p, &rejected);
  let id = rejected["work"]["id"].as_str().unwrap();
  let before = p.work(id);
  let graph = p.graph();
  let path = p.path("correction.json");
  let mut args = REPAIR.to_vec();
  args.extend([
    "--retry-failed",
    "--correct",
    path.to_str().unwrap(),
    "--max-calls",
    "2",
  ]);
  assert_eq!(p.model_cli(&args)["status"], "budget-exhausted");
  assert_eq!(p.model_cli(&args)["status"], "budget-exhausted");
  let staged = p.work(id);
  assert_eq!(list(&staged, "corrections").len(), 1);
  assert_eq!(
    staged["corrections"][0]["previousPending"],
    before["pending"]
  );
  for field in ["attempts", "calls", "inputBytes", "totalTokens"] {
    assert_eq!(before[field], staged[field]);
  }
  assert_eq!(p.graph(), graph);
  let mut responses = p.read_json("responses.json");
  responses["check"]["findings"] = json!([]);
  p.json("responses.json", &responses);
  args.pop();
  args.push("3");
  let result = p.model_cli(&args);
  subset(&result, &json!({"status":"ready","work":{"calls":3}}));
  assert_eq!(result["work"]["id"], rejected["work"]["id"]);
  assert_eq!(p.calls(), 5);
  let after = p.work(id);
  assert_eq!(&list(&after, "attempts")[..2], list(&before, "attempts"));
  assert_eq!(after["attempts"][2]["stage"], "check");
  assert_ne!(
    after["attempts"][2]["inputHash"],
    after["attempts"][1]["inputHash"]
  );
  assert_eq!(p.model_cli(&args)["work"]["calls"], 3);
  assert_eq!(p.calls(), 5);
}

#[test]
fn relationship_correction_reuses_extraction_and_requires_a_budgeted_check() {
  for (replace, complete_check) in [(false, true), (true, true), (false, false)] {
    let p = Project::policy();
    p.model_cli(&["update"]);
    let graph = p.graph();
    let mut responses = preserving_repair(&p);
    responses["byDocument"]["cache.md"]["decisions"][0]["text"] =
      json!("Ordinary cached data expires after seven days.");
    if replace {
      responses["byDocument"]["cache.md"]["relationships"][0]["from"] = json!("unknown");
    } else {
      responses["byDocument"]["cache.md"]["relationships"] = json!([]);
    }
    responses["check"]["relationshipChanges"] = json!([]);
    p.json("responses.json", &responses);
    let rejected = p.model_cli(&REPAIR);
    assert_eq!(rejected["status"], "failed");
    let id = rejected["work"]["id"].as_str().unwrap();
    let before = p.work(id);
    let privacy = list(&graph, "decisions")
      .iter()
      .find(|node| node["document"] == "privacy.md")
      .unwrap();
    let evidence =
      json!([{"document":"privacy.md","lineStart":3,"lineEnd":3,"version":privacy["version"]}]);
    let mut correction = rejected["candidateResolutionContext"].clone();
    correction["reason"] =
      json!("Restore the omitted revocation exception from its current source.");
    correction["evidence"] = evidence.clone();
    correction["decisions"] = json!([]);
    correction["relationships"] = json!([{"id":"r1","from":privacy["id"],"to":"c1","type":"exception-to","reason":"Revocation overrides ordinary retention.","evidence":evidence}]);
    p.json("correction.json", &correction);
    let path = p.path("correction.json");
    let mut args = REPAIR.to_vec();
    args.extend([
      "--retry-failed",
      "--correct",
      path.to_str().unwrap(),
      "--max-calls",
      "2",
    ]);
    assert_eq!(p.model_cli(&args)["status"], "budget-exhausted");
    assert_eq!(p.model_cli(&args)["status"], "budget-exhausted");
    let staged = p.work(id);
    assert_eq!(
      staged["corrections"][0]["previousPending"],
      before["pending"]
    );
    for field in ["attempts", "calls", "inputBytes", "totalTokens"] {
      assert_eq!(staged[field], before[field]);
    }
    assert_eq!(
      staged["pending"]["extraction"]["decisions"],
      before["pending"]["extraction"]["decisions"]
    );
    assert_eq!(p.graph(), graph);
    if complete_check {
      responses["check"]["relationshipChanges"] = json!([{"previousId":"@removed:0","replacements":["@candidate:0"],"reason":"The corrected edge preserves the revocation exception.","evidence":[{"document":"privacy.md","lineStart":3,"lineEnd":3}]}]);
    }
    p.json("responses.json", &responses);
    args.pop();
    args.push("3");
    let result = p.model_cli(&args);
    subset(
      &result,
      &json!({"status":if complete_check { "ready" } else { "failed" },"work":{"calls":3}}),
    );
    if !complete_check {
      assert_eq!(p.graph(), graph);
    }
    assert_eq!(result["work"]["id"], rejected["work"]["id"]);
    let after = p.work(id);
    assert_eq!(&list(&after, "attempts")[..2], list(&before, "attempts"));
    assert_eq!(after["attempts"][2]["stage"], "check");
    assert_ne!(
      after["attempts"][2]["inputHash"],
      before["attempts"][1]["inputHash"]
    );
    assert_eq!(list(&p.graph(), "relationships").len(), 1);
    assert_eq!(p.model_cli(&args)["work"]["calls"], 3);
    assert_eq!(p.calls(), 5);
  }
}

#[test]
fn correction_rechecks_relationships_that_the_original_candidate_preserved() {
  let p = Project::policy();
  let mut responses = p.read_json("responses.json");
  let mut second = responses["extract"]["relationships"][0].clone();
  second["id"] = json!("r2");
  second["type"] = json!("supports");
  second["reason"] = json!("The privacy policy supports the bounded cache policy.");
  responses["extract"]["relationships"]
    .as_array_mut()
    .unwrap()
    .push(second.clone());
  p.json("responses.json", &responses);
  p.model_cli(&["update"]);
  let graph = p.graph();
  let first = list(&graph, "relationships")
    .iter()
    .find(|edge| edge["localId"] == "r1")
    .unwrap();
  let second_id = list(&graph, "relationships")
    .iter()
    .find(|edge| edge["localId"] == "r2")
    .unwrap()["id"]
    .clone();
  responses = preserving_repair(&p);
  second["from"] = json!("@existing:privacy.md");
  responses["byDocument"]["cache.md"]["relationships"] = json!([second]);
  responses["check"]["relationshipChanges"] = json!([]);
  p.json("responses.json", &responses);
  let rejected = p.model_cli(&REPAIR);
  assert_eq!(rejected["status"], "failed");
  let id = rejected["work"]["id"].as_str().unwrap();
  let before = p.work(id);
  assert_eq!(
    before["pending"]["protectedRelationships"],
    json!([first["id"]])
  );
  let mut restored = responses["extract"]["relationships"][0].clone();
  restored["from"] = first["from"].clone();
  restored["evidence"] = first["evidence"].clone();
  let mut rewritten = before["pending"]["extraction"]["relationships"][0].clone();
  rewritten["type"] = json!("requires");
  rewritten["evidence"] = first["evidence"].clone();
  let mut correction = rejected["candidateResolutionContext"].clone();
  correction["reason"] =
    json!("Restore one relationship and revise another; both effects require checking.");
  correction["evidence"] = first["evidence"].clone();
  correction["decisions"] = json!([]);
  correction["relationships"] = json!([restored, rewritten]);
  p.json("correction.json", &correction);
  responses["check"]["relationshipChanges"] = json!([{"previousId":first["id"],"replacements":[first["id"]],"reason":"The first relationship is restored.","evidence":[{"document":"privacy.md","lineStart":3,"lineEnd":3}]}]);
  p.json("responses.json", &responses);
  let path = p.path("correction.json");
  let mut args = REPAIR.to_vec();
  args.extend([
    "--retry-failed",
    "--correct",
    path.to_str().unwrap(),
    "--max-calls",
    "3",
  ]);
  let result = p.model_cli(&args);
  assert_eq!(result["status"], "failed");
  assert_eq!(p.graph(), graph);
  assert_eq!(
    p.work(id)["pending"]["protectedRelationships"],
    json!([second_id])
  );
  assert_eq!(
    p.work(id)["corrections"][0]["previousPending"],
    before["pending"]
  );
  let captured = packets(&p);
  assert_eq!(
    captured.last().unwrap()["removedRelationships"][0]["id"],
    second_id
  );
  assert_eq!(p.calls(), 5);
}

#[test]
fn correction_can_retarget_a_relationship_before_removing_its_duplicate_endpoint() {
  let p = Project::policy();
  p.model_cli(&["update"]);
  let graph = p.graph();
  let mut responses = preserving_repair(&p);
  let mut duplicate = responses["byDocument"]["cache.md"]["decisions"][0].clone();
  duplicate["id"] = json!("duplicate-node");
  responses["byDocument"]["cache.md"]["decisions"]
    .as_array_mut()
    .unwrap()
    .push(duplicate);
  responses["byDocument"]["cache.md"]["relationships"][0]["to"] = json!("duplicate-node");
  responses["check"]["relationshipChanges"] = json!([]);
  p.json("responses.json", &responses);
  let rejected = p.model_cli(&REPAIR);
  assert_eq!(rejected["status"], "failed");
  let id = rejected["work"]["id"].as_str().unwrap();
  let mut edge = p.work(id)["pending"]["extraction"]["relationships"][0].clone();
  edge["to"] = json!("c1");
  edge["evidence"] = graph["relationships"][0]["evidence"].clone();
  let mut correction = rejected["candidateResolutionContext"].clone();
  correction["reason"] =
    json!("Consolidate the duplicate without leaving a dangling relationship.");
  correction["evidence"] = edge["evidence"].clone();
  correction["decisions"] = json!([]);
  correction["removeDecisions"] = json!(["duplicate-node"]);
  correction["relationships"] = json!([edge]);
  p.json("correction.json", &correction);
  let path = p.path("correction.json");
  let mut args = REPAIR.to_vec();
  args.extend([
    "--retry-failed",
    "--correct",
    path.to_str().unwrap(),
    "--max-calls",
    "3",
  ]);
  let checked = p.model_cli(&args);
  subset(&checked, &json!({"status":"ready","work":{"calls":3}}));
  assert_eq!(list(&p.graph(), "decisions").len(), 2);
  assert_eq!(list(&p.graph(), "relationships").len(), 1);
  assert_eq!(p.calls(), 5);
}

#[test]
fn relationship_correction_requires_valid_local_endpoint_citations() {
  for collision in [false, true] {
    let p = Project::policy();
    p.write("policy.md", "# Policy\n\nPreserve the privacy policy.\n");
    let mut responses = p.read_json("responses.json");
    responses["extract"]["decisions"]
      .as_array_mut()
      .unwrap()
      .push(decision(
        "policy.md",
        "c3",
        3,
        "Preserve the privacy policy.",
      ));
    p.json("responses.json", &responses);
    p.model_cli(&["update"]);
    let graph = p.graph();
    let privacy = list(&graph, "decisions")
      .iter()
      .find(|node| node["document"] == "privacy.md")
      .unwrap();
    let policy = list(&graph, "decisions")
      .iter()
      .find(|node| node["document"] == "policy.md")
      .unwrap();
    let local_id = if collision {
      privacy["id"].as_str().unwrap()
    } else {
      "outside"
    };
    responses = preserving_repair(&p);
    responses["byDocument"]["cache.md"]["decisions"]
      .as_array_mut()
      .unwrap()
      .push(decision(
        "cache.md",
        local_id,
        if collision { 3 } else { 99 },
        "A discarded or invalid endpoint.",
      ));
    responses["byDocument"]["cache.md"]["relationships"] = json!([]);
    responses["check"]["relationshipChanges"] = json!([]);
    p.json("responses.json", &responses);
    let mut args = REPAIR.to_vec();
    args.extend(["--source", "policy.md"]);
    let rejected = p.model_cli(&args);
    assert_eq!(rejected["status"], "failed");
    let id = rejected["work"]["id"].as_str().unwrap();
    let before = p.work(id);
    let mut edge = responses["extract"]["relationships"][0].clone();
    edge["from"] = json!(local_id);
    edge["to"] = if collision {
      policy["id"].clone()
    } else {
      json!("c1")
    };
    edge["evidence"] = graph["relationships"][0]["evidence"].clone();
    let mut correction = rejected["candidateResolutionContext"].clone();
    correction["reason"] =
      json!("Only valid materialized endpoints can establish the correction boundary.");
    correction["evidence"] = edge["evidence"].clone();
    correction["decisions"] = json!([]);
    correction["relationships"] = json!([edge]);
    p.json("correction.json", &correction);
    let path = p.path("correction.json");
    let model = p.path("codex");
    args.extend([
      "--retry-failed",
      "--correct",
      path.to_str().unwrap(),
      "--max-calls",
      "3",
      "--codex",
      model.to_str().unwrap(),
    ]);
    let error = p.error(&args);
    assert_eq!(error["error"]["code"], "INVALID_CORRECTION");
    assert_eq!(p.work(id)["attempts"], before["attempts"]);
    assert_eq!(p.work(id)["pending"], before["pending"]);
    assert_eq!(p.graph(), graph);
    assert_eq!(p.calls(), 4);
  }
}

#[test]
fn relationship_correction_rejects_unsafe_or_unchanged_candidates_without_calls() {
  let (p, rejected) = rejected_candidate_resolution("valid");
  let id = rejected["work"]["id"].as_str().unwrap();
  let work = p.work(id);
  let graph = p.graph();
  let mut edge = work["pending"]["extraction"]["relationships"][0].clone();
  for citation in edge["evidence"].as_array_mut().unwrap() {
    citation["version"] = graph["documents"][citation["document"].as_str().unwrap()].clone();
  }
  let mut original = rejected["candidateResolutionContext"].clone();
  original["reason"] = json!("Correct the source-backed revocation exception.");
  original["evidence"] = edge["evidence"].clone();
  original["decisions"] = json!([]);
  original["relationships"] = json!([edge]);
  let path = p.path("correction.json");
  let mut args = REPAIR.to_vec();
  args.extend([
    "--retry-failed",
    "--correct",
    path.to_str().unwrap(),
    "--max-calls",
    "3",
  ]);
  let model = p.path("codex");
  args.extend(["--codex", model.to_str().unwrap()]);
  for case in [
    "no-change",
    "unknown-endpoint",
    "duplicate-id",
    "stale-citation",
    "unversioned-citation",
    "unsupplied-citation",
    "unknown-field",
    "remove-conflict",
    "outside-target",
  ] {
    let mut correction = original.clone();
    let edge = &mut correction["relationships"][0];
    match case {
      "no-change" => {}
      "unknown-endpoint" => edge["from"] = json!("unknown"),
      "duplicate-id" => {
        let duplicate = edge.clone();
        correction["relationships"]
          .as_array_mut()
          .unwrap()
          .push(duplicate);
      }
      "stale-citation" => edge["evidence"][0]["version"] = json!("stale"),
      "unversioned-citation" => {
        edge["evidence"][0]
          .as_object_mut()
          .unwrap()
          .remove("version");
      }
      "unsupplied-citation" => {
        edge["evidence"][0]["lineStart"] = json!(1);
        edge["evidence"][0]["lineEnd"] = json!(1);
      }
      "unknown-field" => edge["extra"] = json!(true),
      "remove-conflict" => correction["removeRelationships"] = json!(["r1"]),
      "outside-target" => edge["to"] = edge["from"].clone(),
      _ => unreachable!(),
    }
    p.json("correction.json", &correction);
    let error = p.error(&args);
    assert_eq!(
      error["error"]["code"], "INVALID_CORRECTION",
      "{case}: {error}"
    );
    assert_eq!(p.graph(), graph);
    assert_eq!(p.work(id)["attempts"], work["attempts"]);
    assert_eq!(p.calls(), 4);
  }
}

#[test]
fn candidate_correction_rejects_stale_or_out_of_scope_input_without_calls() {
  for case in [
    "wrong-work",
    "wrong-check",
    "unknown-id",
    "outside-range",
    "stale-evidence",
    "unknown-field",
    "changed-candidate",
    "changed-graph",
  ] {
    let (p, rejected) = rejected_candidate_resolution("valid");
    let mut correction = correction_file(&p, &rejected);
    let id = rejected["work"]["id"].as_str().unwrap();
    let mut work = p.work(id);
    let mut graph = p.graph();
    match case {
      "wrong-work" => correction["workId"] = json!("other"),
      "wrong-check" => correction["checkInputHash"] = json!("other"),
      "unknown-id" => correction["decisions"][0]["id"] = json!("other"),
      "outside-range" => correction["decisions"][0]["lineStart"] = json!(1),
      "stale-evidence" => correction["evidence"][0]["version"] = json!("stale"),
      "unknown-field" => correction["decisions"][0]["extra"] = json!(true),
      "changed-candidate" => {
        work["pending"]["extraction"]["decisions"][0]["text"] = json!("Changed.");
      }
      "changed-graph" => graph["lastExtraction"] = json!("changed"),
      _ => unreachable!(),
    }
    p.set_work(&work);
    p.set_graph(&graph);
    p.json("correction.json", &correction);
    let path = p.path("correction.json");
    let mut args = REPAIR.to_vec();
    args.extend([
      "--retry-failed",
      "--correct",
      path.to_str().unwrap(),
      "--max-calls",
      "0",
    ]);
    let error = p.error(&args);
    assert!(
      ["INVALID_CORRECTION", "STALE_RETAINED_CHECK"]
        .contains(&error["error"]["code"].as_str().unwrap()),
      "{case}: {error}"
    );
    assert_eq!(p.graph(), graph);
    assert_eq!(p.work(id)["attempts"], work["attempts"]);
    assert_eq!(p.calls(), 4);
  }
}

#[test]
fn candidate_removals_need_a_fresh_check_and_cannot_remove_protected_meaning() {
  for case in ["duplicates", "required-edge"] {
    let (p, rejected) = rejected_candidate_resolution(case);
    let mut correction = correction_file(&p, &rejected);
    if case == "duplicates" {
      correction["removeDecisions"] = json!(["duplicate-node"]);
      correction["removeRelationships"] = json!(["duplicate-edge"]);
    } else {
      correction["removeRelationships"] = json!(["r1"]);
    }
    p.json("correction.json", &correction);
    let graph = p.graph();
    let mut responses = p.read_json("responses.json");
    responses["check"]["findings"] = json!([]);
    if case == "required-edge" {
      responses["check"]["relationshipChanges"] = json!([]);
    }
    p.json("responses.json", &responses);
    let path = p.path("correction.json");
    let mut args = REPAIR.to_vec();
    args.extend([
      "--retry-failed",
      "--correct",
      path.to_str().unwrap(),
      "--max-calls",
      "3",
    ]);
    let checked = p.model_cli(&args);
    assert_eq!(checked["work"]["calls"], 3);
    assert_eq!(p.calls(), 5);
    if case == "duplicates" {
      assert_eq!(checked["status"], "ready");
      assert_eq!(list(&p.graph(), "decisions").len(), 2);
      assert_eq!(list(&p.graph(), "relationships").len(), 1);
    } else {
      assert_eq!(checked["status"], "failed");
      assert_eq!(p.graph(), graph);
    }
  }
}

#[test]
fn checks_include_earlier_round_relationships_between_supplied_endpoints() {
  let p = Project::policy();
  let mut r = p.read_json("responses.json");
  for name in ["z1.md", "z2.md", "z3.md"] {
    p.write(name, "# Rule\n\nAn independent rule.\n");
    r["extract"]["decisions"]
      .as_array_mut()
      .unwrap()
      .push(decision(name, name, 3, "An independent rule."));
  }
  for node in list(&r["extract"], "decisions").clone() {
    let document = node["document"].as_str().unwrap().to_owned();
    r["byDocument"][&document] = json!({"decisions":[node],"relationships":[]});
  }
  r["byDocument"]["privacy.md"]["relationships"] = r["extract"]["relationships"].clone();
  p.json("responses.json", &r);
  let first = p.model_cli(&[
    "update",
    "--source",
    "cache.md",
    "--source",
    "privacy.md",
    "--max-calls",
    "2",
  ]);
  assert_eq!(first["status"], "budget-exhausted");
  let second = p.model_cli(&[
    "update",
    "--source",
    "cache.md",
    "--source",
    "privacy.md",
    "--max-calls",
    "4",
  ]);
  assert_eq!(second["status"], "ready");
  assert_eq!(second["work"]["id"], first["work"]["id"]);
  let captured = packets(&p);
  let packet = captured.last().unwrap();
  let retained = list(packet, "retainedRelationships");
  assert_eq!(retained.len(), 1);
  assert!(
    list(&p.graph(), "relationships")
      .iter()
      .any(|edge| edge["id"] == retained[0]["id"])
  );
}

#[test]
fn candidate_resolution_reuses_the_exact_check_and_preserves_native_quality_and_receipts() {
  let (p, rejected) = rejected_candidate_resolution("valid");
  let id = rejected["work"]["id"].as_str().unwrap();
  let before = p.work(id);
  let path = p.path("resolution.json");
  let mut args = REPAIR.to_vec();
  args.extend([
    "--retry-failed",
    "--max-calls",
    "0",
    "--resolve",
    path.to_str().unwrap(),
    "--codex",
    "/must-not-start",
  ]);
  let result = p.cli(&args);
  subset(
    &result,
    &json!({"status":"ready","warningSummary":{"findings":0,"resolved":1},"work":{"calls":2,"retainedCheckAssessment":"accepted"}}),
  );
  let after = p.work(id);
  for field in ["attempts", "calls", "inputBytes", "totalTokens"] {
    assert_eq!(before[field], after[field]);
  }
  assert_eq!(
    after["candidateResolution"]["checkInputHash"],
    before["attempts"][1]["inputHash"]
  );
  assert_eq!(
    after["candidateResolution"]["warnings"][0]["state"],
    "resolved"
  );
  let graph = p.graph();
  assert_eq!(graph["relationships"][0]["quality"], "uncertain");
  let privacy = list(&graph, "decisions")
    .iter()
    .find(|node| node["document"] == "privacy.md")
    .unwrap();
  assert_eq!(privacy["quality"], "uncertain");
  assert_eq!(p.calls(), 4);
  assert_eq!(p.error(&args)["error"]["code"], "INVALID_RESOLUTION");
}

#[test]
fn batch_and_document_dispositions_reuse_checks_without_waiving_relationship_protection() {
  for case in ["batch", "document", "batch-missing-edge"] {
    let (p, rejected) = rejected_candidate_resolution(case);
    let id = rejected["work"]["id"].as_str().unwrap();
    let before = p.work(id);
    let graph = p.graph();
    let mut resolution = p.read_json("resolution.json");
    resolution["resolutions"][0]["evidence"]
      .as_array_mut()
      .unwrap()
      .push(json!({
        "document":"cache.md","lineStart":3,"lineEnd":3,"version":graph["documents"]["cache.md"]
      }));
    p.json("resolution.json", &resolution);
    let path = p.path("resolution.json");
    let mut args = REPAIR.to_vec();
    args.extend([
      "--retry-failed",
      "--max-calls",
      "0",
      "--resolve",
      path.to_str().unwrap(),
      "--codex",
      "/must-not-start",
    ]);
    let result = p.cli(&args);
    assert_eq!(p.work(id)["attempts"], before["attempts"]);
    assert_eq!(p.calls(), 4);
    if case == "batch-missing-edge" {
      assert_eq!(result["status"], "failed");
      assert_eq!(p.graph(), graph);
    } else {
      assert_eq!(result["status"], "ready", "{case}: {result}");
      assert_eq!(result["work"]["calls"], 2);
      assert_eq!(result["warningSummary"]["resolved"], 1);
      assert_eq!(list(&p.graph(), "relationships").len(), 1);
      assert!(
        list(&p.graph(), "decisions")
          .iter()
          .any(|node| node["quality"] == "uncertain")
      );
    }
  }
}

#[test]
fn scoped_dispositions_reject_unsupplied_documents_and_inadequate_evidence() {
  for case in ["batch", "document", "unsupplied-document"] {
    let (p, rejected) = rejected_candidate_resolution(case);
    let id = rejected["work"]["id"].as_str().unwrap();
    let before = p.work(id);
    let graph = p.graph();
    let mut resolution = p.read_json("resolution.json");
    if case == "document" {
      resolution["resolutions"][0]["evidence"] = json!([{
        "document":"cache.md","lineStart":3,"lineEnd":3,"version":graph["documents"]["cache.md"]
      }]);
    }
    p.json("resolution.json", &resolution);
    let path = p.path("resolution.json");
    let mut args = REPAIR.to_vec();
    args.extend([
      "--retry-failed",
      "--max-calls",
      "0",
      "--resolve",
      path.to_str().unwrap(),
      "--codex",
      "/must-not-start",
    ]);
    assert_eq!(
      p.error(&args)["error"]["code"],
      "INVALID_RESOLUTION",
      "{case}"
    );
    assert_eq!(p.work(id)["attempts"], before["attempts"]);
    assert_eq!(p.graph(), graph);
    assert_eq!(p.calls(), 4);
  }
}

#[test]
fn candidate_resolution_cannot_bypass_identity_evidence_or_relationship_protections() {
  for case in [
    "batch",
    "unknown",
    "missing-edge",
    "stale-evidence",
    "wrong-work",
    "wrong-check",
    "changed-candidate",
    "unfinished-check",
    "other-finding",
    "mixed-ids",
  ] {
    let (p, rejected) = rejected_candidate_resolution(case);
    let graph = p.graph();
    let id = rejected["work"]["id"].as_str().unwrap();
    let mut work = p.work(id);
    let mut resolution = p.read_json("resolution.json");
    match case {
      "stale-evidence" => resolution["resolutions"][0]["evidence"][0]["version"] = json!("stale"),
      "wrong-work" => resolution["workId"] = json!("another-work"),
      "wrong-check" => resolution["checkInputHash"] = json!("another-check"),
      "mixed-ids" => {
        let mut unrelated = resolution["resolutions"][0].clone();
        unrelated["id"] = json!("unrelated-warning");
        resolution["resolutions"]
          .as_array_mut()
          .unwrap()
          .push(unrelated);
      }
      "changed-candidate" => {
        work["pending"]["extraction"]["decisions"][0]["text"] = json!("Changed candidate.");
      }
      "unfinished-check" => work["attempts"][1]["report"]["outcome"] = json!("failed"),
      _ => (),
    }
    p.set_work(&work);
    p.json("resolution.json", &resolution);
    let path = p.path("resolution.json");
    let mut args = REPAIR.to_vec();
    args.extend([
      "--retry-failed",
      "--max-calls",
      "0",
      "--resolve",
      path.to_str().unwrap(),
      "--codex",
      "/must-not-start",
    ]);
    if ["missing-edge", "other-finding"].contains(&case) {
      subset(
        &p.cli(&args),
        &json!({"status":"failed","work":{"calls":2,"retainedCheckAssessment":"blocked"}}),
      );
    } else {
      let expected = if ["changed-candidate", "unfinished-check"].contains(&case) {
        "STALE_RETAINED_CHECK"
      } else {
        "INVALID_RESOLUTION"
      };
      assert_eq!(p.error(&args)["error"]["code"], expected, "{case}");
    }
    assert_eq!(p.graph(), graph, "{case}");
    assert_eq!(p.work(id)["attempts"], work["attempts"], "{case}");
    assert_eq!(p.calls(), 4, "{case}");
  }
}
