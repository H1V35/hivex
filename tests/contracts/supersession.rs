use super::repair::{REPAIR, preserving_repair, rejected_candidate_resolution};
use crate::support::{Project, subset};
use serde_json::{Value, json};
use std::fs;

fn replacement() -> (Project, Value, Value) {
  let (p, failed) = rejected_candidate_resolution("valid");
  p.json("responses.json", &preserving_repair(&p));
  let done = p.model_cli(&[
    "update",
    "--repair-range",
    "cache.md:3-3",
    "--reason",
    "Replace the rejected cache interpretation.",
  ]);
  assert_eq!(done["status"], "ready", "{done}");
  let request = json!({
    "workId":failed["work"]["id"],
    "checkInputHash":failed["candidateResolutionContext"]["checkInputHash"],
    "replacementWorkId":done["work"]["id"],
    "reason":"The admitted replacement covers the same source-backed cache rule.",
    "evidence":[{"document":"cache.md","lineStart":3,"lineEnd":3,"version":p.graph()["documents"]["cache.md"]}]
  });
  p.json("supersede.json", &request);
  (p, failed, done)
}

fn supersede(p: &Project) -> Value {
  p.cli(&[
    "recover",
    "--supersede",
    p.path("supersede.json").to_str().unwrap(),
  ])
}

fn supersede_error(p: &Project) -> Value {
  p.error(&[
    "recover",
    "--supersede",
    p.path("supersede.json").to_str().unwrap(),
  ])
}

#[test]
fn superseded_failure_preserves_history_unlocks_snapshots_and_pins_pruning() {
  let (p, failed, replacement) = replacement();
  let id = failed["work"]["id"].as_str().unwrap();
  let replacement_id = replacement["work"]["id"].as_str().unwrap();
  let before = p.work(id);
  let graph = p.graph();
  p.ok(&["snapshot", "export"]);
  assert_eq!(
    p.error(&["snapshot", "import"])["error"]["code"],
    "UNFINISHED_WORK"
  );
  subset(
    &supersede(&p),
    &json!({"status":"superseded","modelCalls":0,"workId":id,"replacementWorkId":replacement_id}),
  );
  let after = p.work(id);
  let mut original = after.clone();
  original.as_object_mut().unwrap().remove("supersession");
  assert_eq!(original, before);
  assert_eq!(after["status"], "failed");
  assert_eq!(supersede(&p)["supersession"], after["supersession"]);
  assert_eq!(p.graph(), graph);
  p.ok(&["snapshot", "import"]);
  p.ok(&["prune", "--keep-completed", "0", "--keep-caches", "0"]);
  assert_eq!(p.work(id), after);
  assert_eq!(p.work(replacement_id)["status"], "done");
  assert_eq!(p.ok(&["prune"])["unfinishedWorks"], 0);
  p.write("moved.md", fs::read(p.path("cache.md")).unwrap());
  fs::remove_file(p.path("cache.md")).unwrap();
  p.ok(&["snapshot", "relocate", "cache.md", "moved.md"]);
  assert_eq!(p.work(id), after);
  assert_eq!(p.calls(), 6);
}

#[test]
fn superseded_work_cannot_resume_correct_resolve_or_replace_its_profile() {
  let (p, failed, _) = replacement();
  assert_eq!(supersede(&p)["status"], "superseded");
  let before = p.work(failed["work"]["id"].as_str().unwrap());
  p.json("unused.json", &json!({}));
  let path = p.path("unused.json");
  for extra in [
    vec![],
    vec!["--correct", path.to_str().unwrap()],
    vec!["--resolve", path.to_str().unwrap()],
    vec!["--model", "another-profile"],
  ] {
    let limit = if extra.first() == Some(&"--resolve") {
      "0"
    } else {
      "99"
    };
    let mut args = REPAIR.to_vec();
    args.extend([
      "--retry-failed",
      "--max-calls",
      limit,
      "--codex",
      "/must-not-start",
    ]);
    args.extend(extra);
    let error = p.error(&args);
    assert_eq!(
      error["error"]["code"], "WORK_SUPERSEDED",
      "{args:?}: {error}"
    );
    assert_eq!(p.work(before["id"].as_str().unwrap()), before);
  }
  assert_eq!(p.calls(), 6);
}

fn invalid_state(p: &Project, failed: &mut Value, replacement: &mut Value, case: &str) {
  match case {
    "running" => failed["status"] = json!("running"),
    "uncertain" => failed["attempts"][1]["report"]["turnAccepted"] = json!("unknown"),
    "cleanup" => failed["attempts"][1]["report"]["cleanup"] = json!("not-observed"),
    "older-uncertain" => failed["attempts"][0]["report"]["turnAccepted"] = json!("unknown"),
    "replacement-pending" => replacement["status"] = json!("budget-exhausted"),
    "replacement-uncertain" => {
      replacement["attempts"][1]["report"]["cleanup"] = json!("not-observed");
    }
    "wrong-kind" => failed["kind"] = json!("ask"),
    "uncovered" => failed["remaining"] = json!(["cache.md:1-4"]),
    "stale-source" => p.write("cache.md", "# Changed\n\nA different rule.\n"),
    "ambiguous-key" => {
      let mut duplicate = replacement.clone();
      duplicate["id"] = json!("another-admission");
      p.set_work(&duplicate);
    }
    "lost-provenance" => {
      let mut graph = p.graph();
      graph["units"]["cache.md:3-3"]
        .as_object_mut()
        .unwrap()
        .remove("workKey");
      p.set_graph(&graph);
    }
    _ => unreachable!(),
  }
}

#[test]
fn supersession_rejects_unsafe_execution_or_unproven_replacement_without_mutation() {
  for case in [
    "running",
    "uncertain",
    "cleanup",
    "older-uncertain",
    "replacement-pending",
    "replacement-uncertain",
    "wrong-kind",
    "uncovered",
    "stale-source",
    "lost-provenance",
    "ambiguous-key",
  ] {
    let (p, failed, replacement) = replacement();
    let mut old = p.work(failed["work"]["id"].as_str().unwrap());
    let mut new = p.work(replacement["work"]["id"].as_str().unwrap());
    invalid_state(&p, &mut old, &mut new, case);
    p.set_work(&old);
    p.set_work(&new);
    let graph = p.graph();
    let rejected = supersede_error(&p);
    assert!(rejected.get("error").is_some(), "{case}: {rejected}");
    assert_eq!(p.work(old["id"].as_str().unwrap()), old, "{case}");
    assert_eq!(p.work(new["id"].as_str().unwrap()), new, "{case}");
    assert_eq!(p.graph(), graph, "{case}");
    assert_eq!(p.calls(), 6, "{case}");
  }
}

#[test]
fn supersession_requires_exact_identity_current_evidence_and_exclusive_update_lease() {
  for case in [
    "wrong-check",
    "same-work",
    "stale-evidence",
    "missing-evidence",
    "wrong-evidence",
    "unknown-field",
    "lock",
  ] {
    let (p, failed, _) = replacement();
    let before = p.work(failed["work"]["id"].as_str().unwrap());
    let mut request = p.read_json("supersede.json");
    match case {
      "wrong-check" => request["checkInputHash"] = json!("another-check"),
      "same-work" => request["replacementWorkId"] = request["workId"].clone(),
      "stale-evidence" => request["evidence"][0]["version"] = json!("old"),
      "missing-evidence" => request["evidence"] = json!([]),
      "wrong-evidence" => {
        request["evidence"][0] = json!({"document":"privacy.md","lineStart":3,"lineEnd":3,"version":p.graph()["documents"]["privacy.md"]});
      }
      "unknown-field" => request["force"] = json!(true),
      "lock" => p.json(
        ".hivex/knowledge.lock",
        &json!({"id":"busy","pid":std::process::id()}),
      ),
      _ => unreachable!(),
    }
    p.json("supersede.json", &request);
    assert!(supersede_error(&p).get("error").is_some(), "{case}");
    assert_eq!(p.work(before["id"].as_str().unwrap()), before);
    assert_eq!(p.calls(), 6);
  }
}
