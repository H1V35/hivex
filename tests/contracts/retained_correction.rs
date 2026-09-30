use crate::support::{Project, decision, list};
use serde_json::{Value, json};

fn failed_round(foreign: bool, missing_endpoint: bool) -> (Project, Value, Value) {
  let p = Project::new();
  p.model("");
  let mut responses = json!({"capturePackets":true,"byDocument":{},"check":{"findings":[]}});
  for name in ["a.md", "b.md", "c.md", "d.md", "z.md"] {
    p.write(
      name,
      format!("# Rule\n\n{name} establishes the Alpha to Beta dependency.\n"),
    );
    responses["byDocument"][name] =
      json!({"decisions":[decision(name,name,3,&format!("{name} rule."))],"relationships":[]});
  }
  responses["byDocument"]["a.md"]["relationships"] = json!([{
    "id":"r1","from":"a.md","to":"b.md","type":"requires","reason":"Alpha requires Beta under Zeta's scope.",
    "evidence":[{"document":"a.md","lineStart":3,"lineEnd":3},{"document":"b.md","lineStart":3,"lineEnd":3},{"document":"z.md","lineStart":3,"lineEnd":3}]
  }]);
  let mut retained = responses["byDocument"]["a.md"]["relationships"][0].clone();
  retained["from"] = json!("@existing:a.md");
  retained["to"] = json!("@existing:b.md");
  responses["byDocument"]["z.md"]["relationships"] = json!([retained]);
  p.json("responses.json", &responses);
  let initialized = p.model_cli(&["update", "--source", "z.md", "--max-calls", "4"]);
  assert_eq!(initialized["status"], "ready", "{initialized}");
  for name in ["a.md", "b.md", "c.md", "d.md", "z.md"] {
    if !foreign || name == "z.md" {
      p.write(
        name,
        format!("# Rule\n\n{name} establishes the Alpha to Beta dependency. Clarified wording. Gamma also remains an independent rule.\n"),
      );
    }
  }
  responses["byDocument"]["a.md"]["relationships"] = json!([]);
  responses["byDocument"]["z.md"]["relationships"] = json!([]);
  if missing_endpoint {
    responses["byDocument"]["a.md"]["decisions"] = json!([decision(
      "a.md",
      "gamma",
      3,
      "Gamma is an independent rule."
    )]);
  }
  responses["check"]["relationshipChanges"] = json!([]);
  p.json("responses.json", &responses);
  let failed = p.model_cli(&["update", "--source", "z.md", "--max-calls", "4"]);
  assert_eq!(failed["status"], "failed", "{failed}");
  let id = failed["work"]["id"].as_str().unwrap();
  let work = p.work(id);
  let context = &failed["pendingRelationshipReview"];
  // The complete current endpoint definitions are in the exact supplied existing set.
  let packet = &work["pending"]["packet"];
  let endpoint = |doc: &str| {
    list(packet, "existing")
      .iter()
      .find(|node| node["document"] == doc)
      .unwrap()["id"]
      .clone()
  };
  let evidence: Vec<_> = list(context, "suppliedSources")
    .iter()
    .map(|source| {
      json!({
        "document":source["document"],"lineStart":3,"lineEnd":3,"version":source["version"]
      })
    })
    .collect();
  let previous = &context["previousRelationships"][0];
  let mut correction = json!({"workId":id,"checkInputHash":failed["candidateResolutionContext"]["checkInputHash"],
  "reason":"Restore the source-supported dependency omitted in an earlier accepted round of this exact work.",
  "evidence":evidence,"decisions":[],"retainedRelationships":[{"previousId":previous["id"],"replacement":{
    "id":"restore-ab","from":if missing_endpoint { json!("restore-a") } else { endpoint("a.md") },"to":endpoint("b.md"),"type":"requires",
    "reason":"Alpha still requires Beta under the unchanged Zeta scope; the source wording changed without removing this dependency.","evidence":evidence
  }}]});
  if missing_endpoint {
    let restored = decision("a.md", "restore-a", 3, "a.md rule.");
    correction["retainedDecisions"] =
      json!([{"previousId":previous["from"],"replacement":restored}]);
    correction["relationships"] =
      json!([correction["retainedRelationships"][0]["replacement"].clone()]);
    correction["retainedRelationships"] = json!([]);
  }
  (p, failed, correction)
}

fn args<'a>(path: &'a std::path::Path, budget: &'a str) -> Vec<&'a str> {
  vec![
    "update",
    "--source",
    "z.md",
    "--retry-failed",
    "--correct",
    path.to_str().unwrap(),
    "--max-calls",
    budget,
  ]
}

fn evidence_only_repair() -> (Project, Value, Value, Value) {
  let p = Project::new();
  p.model("");
  let mut responses =
    json!({"capturePackets":true,"byDocument":{},"check":{"findings":[],"relationshipChanges":[]}});
  for name in ["a.md", "b.md", "z.md"] {
    p.write(
      name,
      format!("# Rule\n\n{name} establishes the Alpha to Beta dependency.\n"),
    );
    responses["byDocument"][name] =
      json!({"decisions":[decision(name,name,3,&format!("{name} rule."))],"relationships":[]});
  }
  responses["byDocument"]["a.md"]["relationships"] = json!([{
    "id":"r1","from":"a.md","to":"b.md","type":"requires","reason":"Alpha requires Beta under Zeta's scope.",
    "evidence":[{"document":"a.md","lineStart":3,"lineEnd":3},{"document":"b.md","lineStart":3,"lineEnd":3},{"document":"z.md","lineStart":3,"lineEnd":3}]
  }]);
  p.json("responses.json", &responses);
  assert_eq!(
    p.model_cli(&["update", "--source", "z.md", "--max-calls", "4"])["status"],
    "ready"
  );
  let graph = p.graph();
  let previous = &graph["relationships"][0];
  responses["byDocument"]["a.md"]["relationships"] = json!([]);
  p.json("responses.json", &responses);
  let failed = p.model_cli(&[
    "update",
    "--repair-range",
    "z.md:3-3",
    "--reason",
    "Restore omitted dependency.",
    "--max-calls",
    "2",
  ]);
  assert_eq!(failed["status"], "failed", "{failed}");
  let id = failed["work"]["id"].as_str().unwrap();
  let mut replacement = previous.clone();
  replacement["id"] = replacement["localId"].clone();
  for field in ["batch", "localId", "quality"] {
    replacement.as_object_mut().unwrap().remove(field);
  }
  let correction = json!({"workId":id,"checkInputHash":failed["candidateResolutionContext"]["checkInputHash"],
    "reason":"Retain the exact current protected dependency removed only by overlap with its unchanged evidence.",
    "evidence":previous["evidence"],"decisions":[],"retainedRelationships":[{"previousId":previous["id"],"replacement":replacement}]});
  (p, failed, graph, correction)
}

fn exact_repair_args<'a>(path: &'a std::path::Path, budget: &'a str) -> Vec<&'a str> {
  vec![
    "update",
    "--repair-range",
    "z.md:3-3",
    "--reason",
    "Restore omitted dependency.",
    "--retry-failed",
    "--max-calls",
    budget,
    "--correct",
    path.to_str().unwrap(),
  ]
}

#[test]
fn exact_current_relationship_survives_evidence_only_repair_with_original_metadata() {
  let (p, failed, graph, correction) = evidence_only_repair();
  let previous = &graph["relationships"][0];
  let id = failed["work"]["id"].as_str().unwrap();
  let before = p.work(id);
  assert_eq!(before["attempts"][1]["error"], "RELATIONSHIP_LOSS");
  let calls = p.calls();
  let path = p.path("correction.json");
  for case in [
    "reason",
    "type",
    "endpoint",
    "stale-evidence",
    "evidence",
    "previous",
    "id",
    "duplicate",
    "metadata",
  ] {
    let mut changed = correction.clone();
    let record = &mut changed["retainedRelationships"][0];
    match case {
      "reason" => record["replacement"]["reason"] = json!("Different meaning."),
      "type" => record["replacement"]["type"] = json!("supports"),
      "endpoint" => record["replacement"]["from"] = json!("unknown"),
      "stale-evidence" => record["replacement"]["evidence"][0]["version"] = json!("stale"),
      "evidence" => {
        record["replacement"]["evidence"]
          .as_array_mut()
          .unwrap()
          .pop();
      }
      "previous" => record["previousId"] = json!("unprotected"),
      "id" => record["replacement"]["id"] = json!("new-id"),
      "duplicate" => {
        let copy = record.clone();
        changed["retainedRelationships"]
          .as_array_mut()
          .unwrap()
          .push(copy);
      }
      "metadata" => record["replacement"]["quality"] = json!("checked"),
      _ => unreachable!(),
    }
    p.json("correction.json", &changed);
    assert_eq!(
      p.error(&exact_repair_args(&path, "0"))["error"]["code"],
      "INVALID_CORRECTION",
      "{case}"
    );
    let after = p.work(id);
    for field in ["attempts", "calls", "inputBytes", "pending", "corrections"] {
      assert_eq!(after[field], before[field], "{case}: {field}");
    }
    assert_eq!(p.graph(), graph);
    assert_eq!(p.calls(), calls);
  }
  p.json("correction.json", &correction);
  let request = |budget| p.model_cli(&exact_repair_args(&path, budget));
  let staged_result = request("0");
  assert_eq!(
    staged_result["status"], "budget-exhausted",
    "{staged_result}"
  );
  let staged = p.work(id);
  assert_eq!(staged["attempts"], before["attempts"]);
  assert_eq!(
    staged["corrections"][0]["previousPending"],
    before["pending"]
  );
  assert_eq!(p.graph(), graph);
  assert_eq!(p.calls(), calls);
  assert_eq!(request("3")["status"], "ready");
  assert_eq!(p.graph()["relationships"][0], *previous);
  assert_eq!(
    &list(&p.work(id), "attempts")[..2],
    list(&before, "attempts")
  );
  assert_eq!(p.calls(), calls + 1);
  let packet: Value = serde_json::from_str(
    std::fs::read_to_string(p.path("responses.json.packets"))
      .unwrap()
      .lines()
      .last()
      .unwrap(),
  )
  .unwrap();
  assert_eq!(packet["retainedRelationships"][0]["id"], previous["id"]);
}

#[test]
fn retained_relationship_correction_preserves_prior_rounds_and_requires_a_budgeted_fresh_check() {
  for missing_endpoint in [false, true] {
    let (p, failed, correction) = failed_round(false, missing_endpoint);
    let id = failed["work"]["id"].as_str().unwrap();
    let before = p.work(id);
    let graph = p.graph();
    let calls = p.calls();
    p.json("correction.json", &correction);
    let path = p.path("correction.json");
    let limited = p.model_cli(&args(&path, "4"));
    assert_eq!(limited["status"], "budget-exhausted", "{limited}");
    let staged = p.work(id);
    assert_eq!(staged["attempts"], before["attempts"]);
    assert_eq!(staged["calls"], before["calls"]);
    assert_eq!(
      staged["corrections"][0]["previousPending"],
      before["pending"]
    );
    assert_eq!(p.graph(), graph);
    assert_eq!(p.calls(), calls);
    assert_eq!(p.model_cli(&args(&path, "4"))["status"], "budget-exhausted");
    assert_eq!(list(&p.work(id), "corrections").len(), 1);
    let mut responses = p.read_json("responses.json");
    responses["check"] = json!({"findings":[],"relationshipChanges":[{"previousId":"@removed:0","replacements":["@candidate:0"],
    "reason":"The current canonical endpoints preserve the original dependency.","evidence":[{"document":"a.md","lineStart":3,"lineEnd":3},{"document":"b.md","lineStart":3,"lineEnd":3},{"document":"z.md","lineStart":3,"lineEnd":3}]}]});
    p.json("responses.json", &responses);
    let admitted = p.model_cli(&args(&path, "5"));
    assert_eq!(admitted["status"], "ready", "{admitted}");
    let after = p.work(id);
    assert_eq!(
      &list(&after, "attempts")[..list(&before, "attempts").len()],
      list(&before, "attempts")
    );
    assert_eq!(after["calls"], 5);
    assert_eq!(after["attempts"][4]["stage"], "check");
    assert_eq!(p.calls(), calls + 1);
    assert_eq!(list(&p.graph(), "relationships").len(), 1);
    for node in list(&graph, "decisions").iter().filter(|node| {
      node["batch"]
        .as_str()
        .is_some_and(|batch| batch.starts_with(id))
    }) {
      assert!(
        list(&p.graph(), "decisions").contains(node),
        "accepted current interpretation changed"
      );
    }
    assert_eq!(p.model_cli(&args(&path, "5"))["work"]["id"], id);
    assert_eq!(p.calls(), calls + 1);
  }
}

#[test]
fn retained_relationship_correction_rejects_unrelated_stale_colliding_or_partial_references() {
  for case in [
    "foreign",
    "previous",
    "duplicate",
    "endpoint",
    "alias",
    "collision",
    "type",
    "stale",
    "coverage",
  ] {
    let (p, failed, mut correction) = failed_round(case == "foreign", false);
    let id = failed["work"]["id"].as_str().unwrap();
    let before = p.work(id);
    let graph = p.graph();
    let calls = p.calls();
    let record = &mut correction["retainedRelationships"][0];
    match case {
      "previous" => record["previousId"] = json!("not-protected"),
      "duplicate" => {
        let copy = record.clone();
        correction["retainedRelationships"]
          .as_array_mut()
          .unwrap()
          .push(copy);
      }
      "endpoint" => record["replacement"]["from"] = json!("unknown"),
      "alias" => record["replacement"]["from"] = json!("a.md"),
      "collision" => record["replacement"]["id"] = json!("r1"),
      "type" => record["replacement"]["type"] = json!("supports"),
      "stale" => record["replacement"]["evidence"][0]["version"] = json!("stale"),
      "coverage" => {
        record["replacement"]["evidence"]
          .as_array_mut()
          .unwrap()
          .pop();
      }
      _ => (),
    }
    p.json("correction.json", &correction);
    let path = p.path("correction.json");
    let mut request = args(&path, "4");
    request.extend(["--codex", "/must-not-start"]);
    assert_eq!(
      p.error(&request)["error"]["code"],
      "INVALID_CORRECTION",
      "{case}"
    );
    assert_eq!(p.graph(), graph);
    assert_eq!(p.work(id)["attempts"], before["attempts"]);
    assert_eq!(p.calls(), calls);
  }
}

#[test]
fn omitted_endpoint_restoration_cannot_expand_scope_or_overwrite_current_knowledge() {
  for case in [
    "previous",
    "range",
    "document",
    "collision",
    "current",
    "pending",
    "unused",
    "duplicate",
    "unknown-field",
  ] {
    let (p, failed, mut correction) = failed_round(false, true);
    let id = failed["work"]["id"].as_str().unwrap();
    let before = p.work(id);
    let graph = p.graph();
    let calls = p.calls();
    let record = &mut correction["retainedDecisions"][0];
    match case {
      "previous" => record["previousId"] = json!("not-protected"),
      "range" => record["replacement"]["lineStart"] = json!(1),
      "document" => record["replacement"]["document"] = json!("c.md"),
      "collision" => record["replacement"]["id"] = json!("b.md"),
      "current" => {
        let previous = &failed["pendingRelationshipReview"]["previousRelationships"][0];
        record["previousId"] = previous["to"].clone();
        record["replacement"]["document"] = json!("b.md");
        record["replacement"]["text"] = json!("b.md rule.");
      }
      "pending" => {
        record["previousId"] = json!("missing-z");
        record["replacement"]["document"] = json!("z.md");
      }
      "unused" => correction["relationships"] = json!([]),
      "duplicate" => {
        let copy = record.clone();
        correction["retainedDecisions"]
          .as_array_mut()
          .unwrap()
          .push(copy);
      }
      "unknown-field" => record["replacement"]["quality"] = json!("checked"),
      _ => unreachable!(),
    }
    p.json("correction.json", &correction);
    let path = p.path("correction.json");
    let mut request = args(&path, "4");
    request.extend(["--codex", "/must-not-start"]);
    assert_eq!(
      p.error(&request)["error"]["code"],
      "INVALID_CORRECTION",
      "{case}"
    );
    assert_eq!(p.graph(), graph);
    assert_eq!(p.work(id)["attempts"], before["attempts"]);
    assert_eq!(p.calls(), calls);
  }
}
