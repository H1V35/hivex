use super::repair::{REPAIR, preserving_repair};
use crate::support::{Project, decision, list};
use serde_json::{Value, json};

fn operation(kind: &str) -> Vec<&str> {
  match kind {
    "ask" => vec!["ask", "cache"],
    "review" => vec!["review", "cache", "--base", "HEAD"],
    _ => vec!["update"],
  }
}

fn review_project(p: &Project) {
  p.write(
    ".gitignore",
    ".hivex/\nresponses.json*\ncodex\ncalls.log\npids.json\n",
  );
  p.write("cache.rs", "pub const LIMIT: usize = 7;\n");
  p.git_init();
  p.git(&["add", "."]);
  p.git(&["commit", "-qm", "Baseline"]);
  p.write("cache.rs", "pub const LIMIT: usize = 8;\n");
  let mut r = p.read_json("responses.json");
  r["review"] = json!({"findings":[],"uncertainties":[]});
  p.json("responses.json", &r);
}

#[test]
fn explicit_profile_changes_preserve_work_and_run_only_the_pending_phase() {
  for kind in ["update", "ask", "review"] {
    let p = Project::policy();
    if kind == "review" {
      review_project(&p);
    }
    let original_limit = if kind == "update" { "1" } else { "2" };
    let mut request = operation(kind);
    request.extend(["--max-calls", original_limit]);
    let first = p.model_cli(&request);
    let id = first["work"]["id"].as_str().unwrap();
    let before = p.work(id);
    let graph = p.graph();
    let calls = p.calls();
    let mut change = operation(kind);
    change.extend([
      "--resume-with-profile",
      id,
      "--model",
      "fixture-model",
      "--effort",
      "high",
      "--max-calls",
      "0",
    ]);
    let changed = p.model_cli(&change);
    assert_eq!(changed["status"], "budget-exhausted");
    assert_eq!(changed["work"]["id"], id);
    let after = p.work(id);
    for field in [
      "calls",
      "inputBytes",
      "totalTokens",
      "maxCalls",
      "maxInputBytes",
      "attempts",
      "pending",
      "remaining",
      "plannedUnits",
    ] {
      assert_eq!(after[field], before[field], "{kind}:{field}");
    }
    for field in [
      "decisions",
      "relationships",
      "warnings",
      "lastExtraction",
      "documents",
    ] {
      assert_eq!(p.graph()[field], graph[field], "{field}");
    }
    assert_eq!(after["executionProfile"]["model"], "fixture-model");
    assert_eq!(
      after["profileReplacement"]["from"],
      before["executionProfile"]
    );
    assert_eq!(p.calls(), calls);
    p.model_cli(&change);
    assert_eq!(
      p.work(id)["profileReplacement"],
      after["profileReplacement"]
    );
    let mut next = operation(kind);
    next.extend([
      "--resume-with-profile",
      id,
      "--effort",
      "medium",
      "--max-calls",
      "0",
    ]);
    p.model_cli(&next);
    let twice = p.work(id);
    assert_eq!(
      twice["profileReplacement"]["previousReplacements"][0],
      after["profileReplacement"]
    );
    let budget = if kind == "update" { "2" } else { "3" };
    let mut finish = operation(kind);
    finish.extend([
      "--resume-with-profile",
      id,
      "--effort",
      "medium",
      "--max-calls",
      budget,
    ]);
    assert_eq!(p.model_cli(&finish)["status"], "ready");
    let done = p.work(id);
    assert_eq!(
      &list(&done, "attempts")[..list(&before, "attempts").len()],
      list(&before, "attempts")
    );
    assert_eq!(p.calls(), calls + 1);
    assert_eq!(
      done["attempts"][calls]["report"]["executionProfile"]["options"]["effort"],
      "medium"
    );
    assert_eq!(p.model_cli(&finish)["work"]["id"], id);
    assert_eq!(p.calls(), calls + 1);
  }
}

#[test]
fn multiround_profile_change_preserves_coverage_and_completed_repair_reuse() {
  let p = Project::new();
  p.model("");
  let mut r = json!({"byDocument":{},"check":{"findings":[]}});
  for n in 1..=7 {
    let name = format!("rule{n}.md");
    p.write(
      &name,
      format!(
        "# Rule {n}\n\nRule {n} requires bounded work. {}\n",
        "Detail ".repeat(700)
      ),
    );
    r["byDocument"][&name] = json!({"decisions":[decision(&name,&name,3,&format!("Rule {n} requires bounded work."))],"relationships":[]});
  }
  p.json("responses.json", &r);
  assert_eq!(
    p.model_cli(&["update", "--max-calls", "8", "--max-input-bytes", "1048576"])["status"],
    "ready"
  );
  let repair = [
    "update",
    "--repair",
    "rule1.md",
    "--repair",
    "rule2.md",
    "--repair",
    "rule3.md",
    "--repair",
    "rule4.md",
    "--repair",
    "rule5.md",
    "--repair",
    "rule6.md",
    "--repair",
    "rule7.md",
    "--reason",
    "Preserve the seven bounded-work rules.",
  ];
  let mut start = repair.to_vec();
  start.extend(["--max-calls", "2", "--max-input-bytes", "1048576"]);
  let first = p.model_cli(&start);
  let id = first["work"]["id"].as_str().unwrap();
  let before = p.work(id);
  let graph = p.graph();
  assert_ne!(first["pendingUnits"].as_array().unwrap().len(), 0);
  let mut change = repair.to_vec();
  change.extend([
    "--resume-with-profile",
    id,
    "--model",
    "fixture-model",
    "--effort",
    "high",
    "--max-calls",
    "0",
  ]);
  p.model_cli(&change);
  assert_eq!(p.work(id)["attempts"], before["attempts"]);
  let migrated = p.graph();
  for (unit, coverage) in graph["units"].as_object().unwrap() {
    if coverage["workKey"] == before["key"] {
      assert_eq!(migrated["units"][unit]["workKey"], p.work(id)["key"]);
    }
  }
  let mut finish = repair.to_vec();
  finish.extend([
    "--resume-with-profile",
    id,
    "--model",
    "gpt-6-luna",
    "--effort",
    "medium",
    "--max-calls",
    "8",
  ]);
  assert_eq!(p.model_cli(&finish)["status"], "ready");
  let done = p.work(id);
  assert_eq!(done["calls"], 6);
  for node in list(&graph, "decisions")
    .iter()
    .filter(|node| node["batch"].as_str().unwrap().starts_with(id))
  {
    assert!(list(&p.graph(), "decisions").contains(node));
  }
  let calls = p.calls();
  assert_eq!(p.model_cli(&finish)["work"]["id"], id);
  assert_eq!(p.calls(), calls);
}

fn rejected_repair() -> (Project, Value) {
  let p = Project::policy();
  p.model_cli(&["update"]);
  let mut r = preserving_repair(&p);
  r["byDocument"]["cache.md"]["relationships"] = json!([]);
  r["check"]["relationshipChanges"] = json!([]);
  p.json("responses.json", &r);
  let failed = p.model_cli(&REPAIR);
  assert_eq!(failed["status"], "failed");
  (p, failed)
}

#[test]
fn profile_change_cannot_retry_an_unchanged_adverse_check() {
  let (p, failed) = rejected_repair();
  let id = failed["work"]["id"].as_str().unwrap();
  let before = p.work(id);
  let graph = p.graph();
  let calls = p.calls();
  let mut changed = REPAIR.to_vec();
  changed.extend([
    "--resume-with-profile",
    id,
    "--model",
    "fixture-model",
    "--effort",
    "high",
    "--retry-failed",
    "--max-calls",
    "8",
  ]);
  assert_eq!(p.model_cli(&changed)["status"], "failed");
  assert_eq!(p.work(id)["attempts"], before["attempts"]);
  assert_eq!(p.graph(), graph);
  assert_eq!(p.calls(), calls);
}

#[test]
fn profile_change_corrects_from_the_original_receipt_and_checks_only_the_new_candidate() {
  let (p, failed) = rejected_repair();
  let id = failed["work"]["id"].as_str().unwrap();
  let before = p.work(id);
  let graph = p.graph();
  let calls = p.calls();
  let evidence: Vec<_> = ["cache.md", "privacy.md"]
    .iter()
    .map(|doc| json!({"document":doc,"lineStart":3,"lineEnd":3,"version":graph["documents"][*doc]}))
    .collect();
  let privacy = list(&graph, "decisions")
    .iter()
    .find(|node| node["document"] == "privacy.md")
    .unwrap();
  p.json("correction.json", &json!({"workId":id,"checkInputHash":failed["candidateResolutionContext"]["checkInputHash"],
    "reason":"Restore the source-supported revocation exception without depending on the previous model.",
    "evidence":evidence,"decisions":[],"relationships":[{"id":"restored-exception","from":privacy["id"],"to":"c1","type":"exception-to",
      "reason":"Revocation still overrides ordinary retention.","evidence":evidence}]}));
  let path = p.path("correction.json");
  let mut args = REPAIR.to_vec();
  args.extend([
    "--resume-with-profile",
    id,
    "--model",
    "fixture-model",
    "--effort",
    "high",
    "--retry-failed",
    "--correct",
    path.to_str().unwrap(),
    "--max-calls",
    "0",
  ]);
  assert_eq!(p.model_cli(&args)["status"], "budget-exhausted");
  let staged = p.work(id);
  assert_eq!(staged["attempts"], before["attempts"]);
  assert_eq!(staged["maxCalls"], before["maxCalls"]);
  assert_eq!(
    staged["corrections"][0]["previousPending"],
    before["pending"]
  );
  assert_eq!(p.calls(), calls);
  let mut r = preserving_repair(&p);
  r["check"]["relationshipChanges"][0]["evidence"] = json!([
    {"document":"cache.md","lineStart":3,"lineEnd":3},{"document":"privacy.md","lineStart":3,"lineEnd":3}]);
  p.json("responses.json", &r);
  *args.last_mut().unwrap() = "3";
  assert_eq!(p.model_cli(&args)["status"], "ready");
  let done = p.work(id);
  assert_eq!(&list(&done, "attempts")[..2], list(&before, "attempts"));
  assert_eq!(done["attempts"][2]["stage"], "check");
  assert_eq!(
    done["attempts"][2]["report"]["executionProfile"]["model"],
    "fixture-model"
  );
  assert_eq!(p.calls(), calls + 1);
}

#[test]
fn profile_change_can_dispose_a_false_historical_finding_without_relabeling_it() {
  let p = Project::policy();
  p.model_cli(&["update"]);
  let mut r = preserving_repair(&p);
  r["byDocument"]["cache.md"]["decisions"][0]["text"] =
    json!("Ordinary cached data expires after seven days.");
  r["check"]["findings"] =
    json!([{"target":"c1","reason":"The candidate claims cache never expires."}]);
  p.json("responses.json", &r);
  let failed = p.model_cli(&REPAIR);
  assert_eq!(failed["status"], "failed");
  let id = failed["work"]["id"].as_str().unwrap();
  let before = p.work(id);
  let calls = p.calls();
  let graph = p.graph();
  let evidence: Vec<_> = ["cache.md", "privacy.md"]
    .iter()
    .map(|doc| json!({"document":doc,"lineStart":3,"lineEnd":3,"version":graph["documents"][*doc]}))
    .collect();
  let mut review = failed["candidateResolutionContext"].clone();
  review["resolutions"] = json!([{"id":failed["pendingCandidateWarnings"][0]["id"],"reason":"The candidate says seven days, exactly as the current source; it does not claim indefinite retention.","evidence":evidence}]);
  let context = &failed["pendingRelationshipReview"];
  review["relationshipChanges"] = json!([{"previousId":context["previousRelationships"][0]["id"],"replacements":[context["candidateRelationships"][0]["id"]],"reason":"The same revocation exception remains present with current endpoints and evidence.","evidence":evidence}]);
  p.json("resolve.json", &review);
  let path = p.path("resolve.json");
  let mut args = REPAIR.to_vec();
  args.extend([
    "--resume-with-profile",
    id,
    "--model",
    "fixture-model",
    "--effort",
    "high",
    "--retry-failed",
    "--max-calls",
    "0",
    "--resolve",
    path.to_str().unwrap(),
  ]);
  assert_eq!(p.model_cli(&args)["status"], "ready");
  let done = p.work(id);
  assert_eq!(done["status"], "done");
  assert_eq!(done["attempts"], before["attempts"]);
  assert_eq!(
    done["attempts"][1]["report"]["executionProfile"]["model"],
    "gpt-6-luna"
  );
  assert_eq!(done["executionProfile"]["model"], "fixture-model");
  assert_eq!(
    list(&p.graph(), "decisions")
      .iter()
      .find(|node| node["document"] == "cache.md")
      .unwrap()["quality"],
    "uncertain"
  );
  assert_eq!(p.calls(), calls);
}

#[test]
fn explicit_profile_continuation_accepts_legacy_v1_without_a_retired_runtime() {
  let p = Project::new();
  p.write(
    "notes.md",
    "# Policy\nUse bounded work.\nPreserve the budget.\n",
  );
  p.sql_fixture("knowledge-update-cache-v1.sql");
  let id: String = p
    .db()
    .query_row("SELECT id FROM work", [], |row| row.get(0))
    .unwrap();
  let before = p.work(&id);
  let result = p.cli(&[
    "update",
    "--resume-with-profile",
    &id,
    "--model",
    "fixture-model",
    "--effort",
    "high",
    "--max-calls",
    "0",
    "--codex",
    "/no-retired-runtime",
  ]);
  assert_eq!(result["status"], "budget-exhausted");
  assert_eq!(result["work"]["id"], id);
  let after = p.work(&id);
  for field in [
    "calls",
    "inputBytes",
    "totalTokens",
    "maxCalls",
    "maxInputBytes",
    "attempts",
  ] {
    assert_eq!(after[field], before[field], "{field}");
  }
  assert_eq!(after["profileReplacement"]["from"]["model"], "gpt-5.6-luna");
  assert_eq!(after["executionProfile"]["model"], "fixture-model");
  assert_eq!(p.calls(), 0);
}

#[test]
fn profile_transition_errors_preserve_work_graph_and_calls() {
  for case in [
    "unknown",
    "scope",
    "kind",
    "running",
    "uncertain",
    "uncertain-limit",
    "unbound",
    "history",
    "done",
  ] {
    let p = Project::policy();
    let limit = if case == "done" { "2" } else { "1" };
    let initial = p.model_cli(&["update", "--max-calls", limit]);
    let id = initial["work"]["id"].as_str().unwrap();
    let mut saved = p.work(id);
    match case {
      "running" => saved["status"] = json!("running"),
      "uncertain" | "uncertain-limit" => {
        saved["status"] = json!(if case == "uncertain" {
          "failed"
        } else {
          "context-limit"
        });
        saved["attempts"][0]["report"] = json!({"outcome":"failed","turnAccepted":"unknown","cleanup":"not-observed","usage":null});
      }
      "unbound" => {
        saved.as_object_mut().unwrap().remove("executionProfile");
      }
      "history" => saved["profileReplacement"] = json!({"previousReplacements":"invalid-history"}),
      _ => (),
    }
    p.set_work(&saved);
    let graph = p.graph();
    let calls = p.calls();
    let mut args = match case {
      "kind" => vec!["ask", "cache"],
      _ => vec!["update"],
    };
    args.extend([
      "--resume-with-profile",
      if case == "unknown" {
        "not-this-work"
      } else {
        id
      },
      "--model",
      "fixture-model",
      "--effort",
      "high",
    ]);
    if case == "scope" {
      args.extend(["--source", "cache.md"]);
    }
    let code = match case {
      "running" => "WORK_RUNNING",
      "uncertain" | "uncertain-limit" => "WORK_UNCERTAIN",
      _ => "INVALID_PROFILE_RESUMPTION",
    };
    assert_eq!(p.error(&args)["error"]["code"], code, "{case}");
    assert_eq!(p.work(id), saved, "{case}");
    assert_eq!(p.graph(), graph, "{case}");
    assert_eq!(p.calls(), calls, "{case}");
  }
}
