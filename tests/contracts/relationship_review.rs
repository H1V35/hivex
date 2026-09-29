use super::repair::{REPAIR, preserving_repair};
use crate::support::{Project, decision, list};
use serde_json::{Value, json};
use std::fs;

fn rejected(case: &str) -> (Project, Value, Value) {
  let p = Project::policy();
  let mut r = p.read_json("responses.json");
  let mut second = r["extract"]["relationships"][0].clone();
  second["id"] = json!("r2");
  second["from"] = json!("c1");
  second["to"] = json!("c2");
  second["type"] = json!("supports");
  r["extract"]["relationships"]
    .as_array_mut()
    .unwrap()
    .push(second.clone());
  p.json("responses.json", &r);
  p.model_cli(&["update"]);
  let graph = p.graph();
  let mut r = preserving_repair(&p);
  second["to"] = json!("@existing:privacy.md");
  r["byDocument"]["cache.md"]["relationships"]
    .as_array_mut()
    .unwrap()
    .push(second);
  r["byDocument"]["cache.md"]["decisions"][0]["text"] =
    json!("Ordinary cache expires after seven days.");
  if case == "absent" {
    r["byDocument"]["cache.md"]["relationships"] = json!([]);
  }
  r["check"]["relationshipChanges"] = json!([]);
  if case != "missing-mappings" {
    r["check"]["findings"] = json!([{"target":graph["relationships"][0]["id"],"reason":"The previous exception has no replacement."}]);
  }
  if case == "other-finding" {
    let privacy = list(&graph, "decisions")
      .iter()
      .find(|node| node["document"] == "privacy.md")
      .unwrap();
    r["check"]["findings"].as_array_mut().unwrap().push(
      json!({"target":privacy["id"],"reason":"A genuine independent endpoint defect remains."}),
    );
  }
  p.json("responses.json", &r);
  let failed = p.model_cli(&REPAIR);
  assert_eq!(failed["status"], "failed");
  let packet: Value = serde_json::from_str(
    fs::read_to_string(p.path("responses.json.packets"))
      .unwrap()
      .lines()
      .last()
      .unwrap(),
  )
  .unwrap();
  (p, failed, packet)
}

fn review(failed: &Value, packet: &Value) -> Value {
  let evidence: Vec<_> = list(packet, "documents").iter().map(|document| {
    json!({"document":document["id"],"lineStart":3,"lineEnd":3,"version":document["version"]})
  }).collect();
  let changes: Vec<_> = list(packet, "removedRelationships").iter().enumerate().map(|(index, previous)| {
    let replacement = list(&packet["extraction"], "relationships").get(index)
      .map_or(&previous["id"], |edge| &edge["id"]);
    json!({"previousId":previous["id"],"replacements":[replacement],
      "reason":"The current endpoint definitions and source passages preserve the previous relationship meaning, direction and conditions.","evidence":evidence})
  }).collect();
  let resolutions: Vec<_> = list(failed, "pendingCandidateWarnings").iter().map(|warning| {
    json!({"id":warning["id"],"reason":"The claimed missing relation is present in the exact materialized candidate with the same scope and current source evidence.","evidence":evidence})
  }).collect();
  let mut file = failed["candidateResolutionContext"].clone();
  file["resolutions"] = json!(resolutions);
  file["relationshipChanges"] = json!(changes);
  file
}

fn transitioned_candidate(stale: bool) -> (Project, Value) {
  let p = Project::new();
  p.model("");
  let sources = [
    (
      "a.md",
      "Alpha requires Beta. Alternative requires Beta separately.",
    ),
    ("b.md", "Beta is a prerequisite."),
    ("c.md", "An independent Gamma rule."),
    ("d.md", "An independent Delta rule."),
    ("z.md", "Zeta requires Alpha."),
  ];
  let mut responses = json!({"capturePackets":true,"byDocument":{},"check":{"findings":[]}});
  for (name, text) in sources {
    p.write(name, format!("# Rule\n\n{text}\n"));
    responses["byDocument"][name] =
      json!({"decisions":[decision(name,name,3,text)],"relationships":[]});
  }
  responses["byDocument"]["a.md"]["decisions"][0]["text"] = json!("Alpha requires Beta.");
  responses["byDocument"]["a.md"]["decisions"]
    .as_array_mut()
    .unwrap()
    .push(decision(
      "a.md",
      "alternative",
      3,
      "Alternative requires Beta separately.",
    ));
  responses["byDocument"]["a.md"]["relationships"] = json!([{
    "id":"r1","from":"a.md","to":"b.md","type":"requires","reason":"Alpha requires the Beta prerequisite.",
    "evidence":[{"document":"a.md","lineStart":3,"lineEnd":3},{"document":"b.md","lineStart":3,"lineEnd":3}]
  }]);
  responses["byDocument"]["z.md"]["relationships"] = json!([{
    "id":"r2","from":"z.md","to":"@existing:a.md","type":"requires","reason":"Zeta requires Alpha.",
    "evidence":[{"document":"a.md","lineStart":3,"lineEnd":3},{"document":"z.md","lineStart":3,"lineEnd":3}]
  }]);
  p.json("responses.json", &responses);
  assert_eq!(
    p.model_cli(&["update", "--max-calls", "8"])["status"],
    "ready"
  );
  for (name, text) in sources {
    if stale || name != "a.md" {
      p.write(name, format!("# Rule\n\n{text} Clarified wording.\n"));
    }
  }
  if !stale {
    let graph = p.graph();
    let alternative = list(&graph, "decisions")
      .iter()
      .find(|node| node["text"] == "Alternative requires Beta separately.")
      .unwrap();
    let mut edge = responses["byDocument"]["a.md"]["relationships"][0].clone();
    edge["from"] = alternative["id"].clone();
    responses["byDocument"]["b.md"]["relationships"] = json!([edge]);
  }
  responses["check"]["relationshipChanges"] = json!([]);
  p.json("responses.json", &responses);
  let failed = p.model_cli(&["update", "--max-calls", "8"]);
  assert_eq!(failed["status"], "failed");
  (p, failed)
}

#[test]
fn relationship_review_rebinds_obsolete_transitional_endpoints_but_keeps_current_ids() {
  for stale in [true, false] {
    let (p, failed) = transitioned_candidate(stale);
    let context = &failed["pendingRelationshipReview"];
    let previous = &context["previousRelationships"][0];
    let old = list(context, "previousDecisions")
      .iter()
      .find(|node| node["id"] == previous["from"])
      .unwrap();
    let beta = list(context, "candidateDecisions")
      .iter()
      .find(|node| node["document"] == "b.md")
      .unwrap();
    let edge = list(context, "candidateRelationships")
      .iter()
      .find(|edge| edge["to"] == beta["id"])
      .unwrap();
    let source = list(context, "suppliedSources")
      .iter()
      .find(|source| source["document"] == "a.md")
      .unwrap();
    assert_eq!(old["retainedInCandidate"], true);
    assert_eq!(old["version"] != source["version"], stale);
    assert_ne!(old["id"], edge["from"]);
    let evidence: Vec<_> = list(context, "suppliedSources").iter().map(|source| {
      json!({"document":source["document"],"lineStart":3,"lineEnd":3,"version":source["version"]})
    }).collect();
    let mut file = failed["candidateResolutionContext"].clone();
    file["resolutions"] = json!([]);
    file["relationshipChanges"] = json!([{"previousId":previous["id"],"replacements":[edge["id"]],
      "reason":"The current source authorities and endpoint definitions preserve the same dependency. Obsolete endpoints survive only for a deferred relationship.","evidence":evidence}]);
    p.json("review.json", &file);
    let path = p.path("review.json");
    let args = [
      "update",
      "--retry-failed",
      "--max-calls",
      "0",
      "--resolve",
      path.to_str().unwrap(),
      "--codex",
      "/must-not-start",
    ];
    let id = failed["work"]["id"].as_str().unwrap();
    let before = p.work(id);
    let graph = p.graph();
    if stale {
      let mut progress_args = args.to_vec();
      progress_args.extend(["--progress", "always"]);
      let output = p.raw(&progress_args);
      assert!(output.status.success());
      let result: Value = serde_json::from_slice(&output.stdout).unwrap();
      assert_eq!(result["status"], "pending");
      let progress = String::from_utf8(output.stderr).unwrap();
      assert!(progress.contains("work resumed"));
      assert!(progress.contains("reassessing retained result"));
      assert!(progress.contains("admission finished"));
      assert_ne!(p.graph(), graph);
    } else {
      assert_eq!(p.error(&args)["error"]["code"], "INVALID_RESOLUTION");
      assert_eq!(p.graph(), graph);
    }
    unchanged(&before, &p.work(id));
    assert_eq!(p.calls(), 6);
  }
}

fn resolve(p: &Project, file: &Value) -> Value {
  p.json("review.json", file);
  let path = p.path("review.json");
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
  p.cli(&args)
}

fn resolve_error(p: &Project, file: &Value) -> Value {
  p.json("review.json", file);
  let path = p.path("review.json");
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
  p.error(&args)
}

fn unchanged(before: &Value, after: &Value) {
  for field in [
    "attempts",
    "calls",
    "inputBytes",
    "totalTokens",
    "maxCalls",
    "maxInputBytes",
    "corrections",
  ] {
    assert_eq!(after[field], before[field], "{field}");
  }
}

#[test]
fn relationship_review_recovers_false_findings_and_missing_mappings_without_calls() {
  for findings in [true, false] {
    let (p, failed, packet) = rejected(if findings {
      "false-finding"
    } else {
      "missing-mappings"
    });
    let id = failed["work"]["id"].as_str().unwrap();
    let before = p.work(id);
    let file = review(&failed, &packet);
    assert_eq!(
      failed["pendingRelationshipReview"]["previousRelationships"],
      packet["removedRelationships"]
    );
    assert_eq!(
      failed["pendingRelationshipReview"]["candidateRelationships"],
      packet["extraction"]["relationships"]
    );
    assert_eq!(
      failed["pendingRelationshipReview"]["nativeCheck"],
      before["attempts"][1]["result"]
    );
    let recovered = resolve(&p, &file);
    assert_eq!(recovered["status"], "ready", "{recovered}");
    assert_eq!(recovered["work"]["retainedCheckAssessment"], "accepted");
    let after = p.work(id);
    unchanged(&before, &after);
    assert_eq!(after["candidateResolution"]["review"], file);
    assert_eq!(
      after["candidateResolution"]["previousPending"],
      before["pending"]
    );
    assert_eq!(list(&p.graph(), "relationships").len(), 2);
    if findings {
      assert!(
        list(&p.graph(), "relationships")
          .iter()
          .all(|edge| edge["quality"] == "uncertain")
      );
      assert!(
        list(&p.graph(), "decisions")
          .iter()
          .any(|node| node["quality"] == "uncertain")
      );
      assert_eq!(recovered["warningSummary"]["resolved"], 1);
    }
    assert_eq!(resolve(&p, &file)["work"]["id"], id);
    assert_eq!(p.work(id), after);
    assert_eq!(p.calls(), 4);
    let mut different = file;
    different["relationshipChanges"][0]["reason"] = json!("A different review.");
    assert_eq!(
      resolve_error(&p, &different)["error"]["code"],
      "INVALID_RESOLUTION"
    );
    assert_eq!(p.work(id), after);
  }
}

#[test]
fn relationship_review_requires_evidence_covering_the_false_finding_itself() {
  let (p, failed, packet) = rejected("false-finding");
  let id = failed["work"]["id"].as_str().unwrap();
  let before = p.work(id);
  let graph = p.graph();
  let mut file = review(&failed, &packet);
  file["resolutions"][0]["evidence"]
    .as_array_mut()
    .unwrap()
    .pop();
  assert_eq!(
    resolve_error(&p, &file)["error"]["code"],
    "INVALID_RESOLUTION"
  );
  unchanged(&before, &p.work(id));
  assert_eq!(p.graph(), graph);
  assert_eq!(p.calls(), 4);
}

#[test]
fn relationship_review_replay_rejects_graph_divergence_without_reapplying_the_candidate() {
  let (p, failed, packet) = rejected("false-finding");
  let id = failed["work"]["id"].as_str().unwrap();
  let file = review(&failed, &packet);
  assert_eq!(resolve(&p, &file)["status"], "ready");
  let after = p.work(id);
  let mut graph = p.graph();
  graph["relationships"][0]["reason"] = json!("Changed after admission.");
  p.set_graph(&graph);
  assert_eq!(
    resolve_error(&p, &file)["error"]["code"],
    "STALE_RETAINED_CHECK"
  );
  assert_eq!(p.work(id), after);
  assert_eq!(p.graph(), graph);
  assert_eq!(p.calls(), 4);
}

#[test]
fn relationship_review_replay_after_another_repair_cannot_create_a_new_work() {
  let (p, failed, packet) = rejected("false-finding");
  let id = failed["work"]["id"].as_str().unwrap();
  let file = review(&failed, &packet);
  assert_eq!(resolve(&p, &file)["status"], "ready");
  let mut responses = p.read_json("responses.json");
  responses["check"]["findings"] = json!([]);
  p.json("responses.json", &responses);
  let mut other = REPAIR.to_vec();
  other[4] = "Another source-backed clarification.";
  assert_eq!(p.model_cli(&other)["status"], "ready");
  let before = p.work(id);
  let graph = p.graph();
  let count: usize = p
    .db()
    .query_row("SELECT count(*) FROM work", [], |row| row.get(0))
    .unwrap();
  assert_eq!(
    resolve_error(&p, &file)["error"]["code"],
    "STALE_RETAINED_CHECK"
  );
  let after: usize = p
    .db()
    .query_row("SELECT count(*) FROM work", [], |row| row.get(0))
    .unwrap();
  assert_eq!(after, count);
  assert_eq!(p.work(id), before);
  assert_eq!(p.graph(), graph);
  assert_eq!(p.calls(), 6);
}

#[test]
fn relationship_review_keeps_partial_coverage_and_other_findings_blocked() {
  for case in ["partial", "unclosed", "other-finding"] {
    let (p, failed, packet) = rejected(if case == "partial" {
      "missing-mappings"
    } else {
      case
    });
    let id = failed["work"]["id"].as_str().unwrap();
    let before = p.work(id);
    let graph = p.graph();
    let mut file = review(&failed, &packet);
    match case {
      "partial" => {
        file["relationshipChanges"].as_array_mut().unwrap().pop();
      }
      "unclosed" => file["resolutions"] = json!([]),
      "other-finding" => {
        file["resolutions"].as_array_mut().unwrap().pop();
      }
      _ => unreachable!(),
    }
    assert_eq!(resolve(&p, &file)["status"], "failed");
    unchanged(&before, &p.work(id));
    assert_eq!(p.graph(), graph);
    assert_eq!(p.calls(), 4);
  }
}

#[test]
fn relationship_review_rejects_absent_replacements_invalid_ids_and_evidence() {
  for case in [
    "absent",
    "empty",
    "wrong-previous",
    "wrong-replacement",
    "wrong-present-replacement",
    "duplicate-previous",
    "duplicate-replacement",
    "missing-evidence",
    "incomplete-evidence",
    "stale-evidence",
    "unversioned-evidence",
    "unknown-field",
    "unknown-citation-field",
    "blank-reason",
    "wrong-check",
    "wrong-work",
  ] {
    let (p, failed, packet) = rejected(case);
    let id = failed["work"]["id"].as_str().unwrap();
    let before = p.work(id);
    let graph = p.graph();
    let mut file = review(&failed, &packet);
    match case {
      "absent" | "wrong-replacement" => {
        file["relationshipChanges"][0]["replacements"] = json!(["unknown-edge"]);
      }
      "wrong-present-replacement" => {
        file["relationshipChanges"][0]["replacements"] =
          json!([packet["extraction"]["relationships"][1]["id"]]);
      }
      "empty" => file["relationshipChanges"][0]["replacements"] = json!([]),
      "wrong-previous" => {
        file["relationshipChanges"][0]["previousId"] =
          packet["extraction"]["relationships"][0]["id"].clone();
      }
      "duplicate-previous" => {
        file["relationshipChanges"][1] = file["relationshipChanges"][0].clone();
      }
      "duplicate-replacement" => file["relationshipChanges"][0]["replacements"]
        .as_array_mut()
        .unwrap()
        .push(packet["extraction"]["relationships"][0]["id"].clone()),
      "missing-evidence" => file["relationshipChanges"][0]["evidence"] = json!([]),
      "incomplete-evidence" => {
        file["relationshipChanges"][0]["evidence"]
          .as_array_mut()
          .unwrap()
          .pop();
      }
      "stale-evidence" => file["relationshipChanges"][0]["evidence"][0]["version"] = json!("stale"),
      "unversioned-evidence" => {
        file["relationshipChanges"][0]["evidence"][0]
          .as_object_mut()
          .unwrap()
          .shift_remove("version");
      }
      "unknown-field" => file["relationshipChanges"][0]["approve"] = json!(true),
      "unknown-citation-field" => {
        file["relationshipChanges"][0]["evidence"][0]["approve"] = json!(true);
      }
      "blank-reason" => file["relationshipChanges"][0]["reason"] = json!(" "),
      "wrong-check" => file["checkInputHash"] = json!("stale"),
      "wrong-work" => file["workId"] = json!("another-work"),
      _ => unreachable!(),
    }
    assert_eq!(
      resolve_error(&p, &file)["error"]["code"],
      "INVALID_RESOLUTION",
      "{case}"
    );
    unchanged(&before, &p.work(id));
    assert_eq!(p.graph(), graph);
    assert_eq!(p.calls(), 4);
  }
}

#[test]
fn relationship_review_rejects_changed_sources_candidate_graph_and_receipts() {
  for case in ["source", "candidate", "graph", "unfinished", "cleanup"] {
    let (p, failed, packet) = rejected("false-finding");
    let id = failed["work"]["id"].as_str().unwrap();
    let mut before = p.work(id);
    let file = review(&failed, &packet);
    match case {
      "source" => p.write("privacy.md", "# Access\n\nRevocation behavior changed.\n"),
      "candidate" => {
        before["pending"]["extraction"]["decisions"][0]["text"] = json!("Different candidate.");
      }
      "graph" => {
        let mut graph = p.graph();
        graph["lastExtraction"] = json!("diverged");
        p.set_graph(&graph);
      }
      "unfinished" => before["attempts"][1]["report"]["outcome"] = json!("failed"),
      "cleanup" => before["attempts"][1]["report"]["cleanup"] = json!("uncertain"),
      _ => unreachable!(),
    }
    p.set_work(&before);
    let graph = p.graph();
    let error = resolve_error(&p, &file);
    assert!(
      ["STALE_RETAINED_CHECK", "INVALID_RESOLUTION"]
        .contains(&error["error"]["code"].as_str().unwrap()),
      "{case}: {error}"
    );
    unchanged(&before, &p.work(id));
    assert_eq!(p.graph(), graph);
    assert_eq!(p.calls(), 4);
  }
}
