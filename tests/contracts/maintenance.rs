use crate::support::{Project, decision, empty_graph, list, subset, work};
use serde_json::json;
use std::fs;

#[test]
fn legacy_pending_work_upgrades_without_resetting_its_budget() {
  for multiround in [false, true] {
    let p = Project::new();
    let text = if multiround {
      (1..=3)
        .map(|n| {
          format!(
            "# Section {n}\n\nRule {n} requires bounded work. {}\n",
            "Detail ".repeat(800).trim_end()
          )
        })
        .collect::<Vec<_>>()
        .join("\n")
    } else {
      "# Policy\nUse bounded work.\nPreserve the budget.\n".to_owned()
    };
    p.write("notes.md", text);
    p.sql_fixture(if multiround {
      "knowledge-multiround-cache-v1.sql"
    } else {
      "knowledge-update-cache-v1.sql"
    });
    let id: String = p
      .db()
      .query_row("SELECT id FROM work", [], |row| row.get(0))
      .unwrap();
    let mut before = p.work(&id);
    if multiround {
      before["executionProfile"] = json!({"integration":"codex","provider":"openai","model":"gpt-5.6-luna","options":{"effort":"max"}});
      p.set_work(&before);
    }
    let result = p.cli(&["update", "--codex", "/model-must-not-start"]);
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
    assert_eq!(after["executionProfile"]["model"], "gpt-6-luna");
    assert_eq!(after["profileReplacement"]["from"]["model"], "gpt-5.6-luna");
    assert_eq!(after["cacheHits"], 0);
    assert_eq!(
      p.cli(&["update", "--codex", "/model-must-not-start"])["work"]["id"],
      id
    );
  }
}

#[test]
fn readonly_maintenance_keeps_fixed_sql_rows_byte_identical() {
  for fixture in [
    "knowledge-cache-v1.sql",
    "knowledge-update-cache-v1.sql",
    "knowledge-multiround-cache-v1.sql",
  ] {
    let p = Project::new();
    p.sql_fixture(fixture);
    let before: String = p
      .db()
      .query_row("SELECT data FROM graph WHERE id=1", [], |row| row.get(0))
      .unwrap();
    let works: Vec<String> = p
      .db()
      .prepare("SELECT data FROM work ORDER BY id")
      .unwrap()
      .query_map([], |row| row.get(0))
      .unwrap()
      .collect::<Result<_, _>>()
      .unwrap();
    let caches: Vec<(String, String)> = p
      .db()
      .prepare("SELECT key,value FROM model_cache ORDER BY key")
      .unwrap()
      .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
      .unwrap()
      .collect::<Result<_, _>>()
      .unwrap();
    p.ok(&["status"]);
    p.ok(&["recover"]);
    p.ok(&["prune"]);
    let graph_after: String = p
      .db()
      .query_row("SELECT data FROM graph WHERE id=1", [], |row| row.get(0))
      .unwrap();
    assert_eq!(graph_after, before, "{fixture}");
    let after: Vec<String> = p
      .db()
      .prepare("SELECT data FROM work ORDER BY id")
      .unwrap()
      .query_map([], |row| row.get(0))
      .unwrap()
      .collect::<Result<_, _>>()
      .unwrap();
    assert_eq!(after, works);
    let cache_after: Vec<(String, String)> = p
      .db()
      .prepare("SELECT key,value FROM model_cache ORDER BY key")
      .unwrap()
      .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
      .unwrap()
      .collect::<Result<_, _>>()
      .unwrap();
    assert_eq!(cache_after, caches);
  }
}

fn dead_pid() -> i32 {
  let pid = 2_000_000_000;
  assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
  assert_eq!(
    std::io::Error::last_os_error().raw_os_error(),
    Some(libc::ESRCH)
  );
  pid
}

#[test]
fn recovery_preserves_accounting_and_requires_honest_uncertainty_acknowledgement() {
  let p = Project::new();
  let mut w = work("dead-native");
  w["status"] = json!("running");
  w["ownerPid"] = json!(dead_pid());
  w["nativeProcessId"] = json!(dead_pid());
  w["calls"] = json!(1);
  w["inputBytes"] = json!(123);
  w["attempts"] = json!([{"inputBytes":123,"inputHash":"input-hash","result":{"retained":true},"stage":"extract"}]);
  p.set_work(&w);
  subset(
    &p.cli(&["recover"]),
    &json!({"status":"blocked","interruptedWorks":0,"modelCalls":0,"lock":"absent"}),
  );
  subset(
    &p.ok(&["recover", "--acknowledge-uncertain"]),
    &json!({"status":"recovered","interruptedWorks":1,"acknowledgedWorks":1,"modelCalls":0}),
  );
  let recovered = p.work("dead-native");
  subset(
    &recovered,
    &json!({"status":"failed","calls":1,"inputBytes":123,"remaining":[]}),
  );
  assert!(recovered.get("nativeProcessId").is_none());
  subset(
    &recovered["attempts"][0],
    &json!({"result":{"retained":true},"report":{"cleanup":"not-observed","outcome":"interrupted","turnAccepted":"unknown","usage":null},"recoveryAcknowledgement":{"type":"uncertain-invocation"}}),
  );
  assert_eq!(list(&recovered, "attempts").len(), 1);
  assert_eq!(p.ok(&["recover"])["acknowledgedWorks"], 0);
  p.json(
    ".hivex/knowledge.lock",
    &json!({"id":"later-dead-owner","pid":dead_pid()}),
  );
  p.ok(&["recover"]);
  assert!(!p.path(".hivex/knowledge.lock").exists());
}

#[test]
fn recovery_matrix_handles_saved_failure_before_turn_missing_pid_and_live_owner() {
  for state in ["saved", "before-turn", "missing-pid", "live"] {
    let p = Project::new();
    let mut w = work(state);
    w["calls"] = json!(1);
    w["inputBytes"] = json!(17);
    w["ownerPid"] = json!(if state == "live" {
      i32::try_from(std::process::id()).unwrap()
    } else {
      dead_pid()
    });
    w["status"] = json!(if state == "before-turn" || state == "live" {
      "running"
    } else {
      "failed"
    });
    let mut attempt = json!({"stage":"ask","inputBytes":17,"inputHash":"input"});
    let report = json!({"cleanup":"not-observed","code":"MODEL_TIMEOUT","interruption":"unconfirmed","nativeProcessId":dead_pid(),"outcome":"failed","turnAccepted":"unknown","usage":null});
    if state == "saved" {
      attempt["report"] = report.clone();
    }
    if state == "missing-pid" {
      attempt["report"] =
        json!({"interruption":"unconfirmed","turnAccepted":"unknown","usage":null});
    }
    if state == "live" {
      w["nativeProcessId"] = json!(std::process::id());
      p.json(
        ".hivex/knowledge.lock",
        &json!({"id":"live","pid":std::process::id()}),
      );
    }
    w["attempts"] = json!([attempt]);
    p.set_work(&w);
    match state {
      "saved" => {
        assert_eq!(p.cli(&["recover"])["status"], "blocked");
        subset(
          &p.ok(&["recover", "--acknowledge-uncertain"]),
          &json!({"acknowledgedWorks":1,"interruptedWorks":0}),
        );
        assert_eq!(p.work(state)["attempts"][0]["report"], report);
      }
      "before-turn" => {
        subset(
          &p.ok(&["recover"]),
          &json!({"interruptedWorks":1,"acknowledgedWorks":0}),
        );
        subset(
          &p.work(state),
          &json!({"status":"failed","calls":1,"inputBytes":17,"attempts":[{"report":{"code":"MODEL_INTERRUPTED_BEFORE_TURN","usage":null}}]}),
        );
      }
      _ => {
        assert_eq!(
          p.cli(&["recover", "--acknowledge-uncertain"])["status"],
          "blocked"
        );
        assert_eq!(p.work(state), w);
        if state == "live" {
          assert!(p.path(".hivex/knowledge.lock").exists());
        }
      }
    }
  }
  let p = Project::new();
  p.set_graph(&empty_graph());
  p.write(".hivex/knowledge.lock", "{\"id\":\"unknown-owner\"}");
  subset(
    &p.cli(&["recover"]),
    &json!({"status":"blocked","lock":"unreadable","interruptedWorks":0}),
  );
  assert_eq!(
    fs::read_to_string(p.path(".hivex/knowledge.lock")).unwrap(),
    "{\"id\":\"unknown-owner\"}"
  );
}

#[test]
fn prune_only_removes_old_completed_work_and_caches() {
  let p = Project::new();
  p.set_graph(&empty_graph());
  let mut unfinished = work("unfinished");
  unfinished["status"] = json!("failed");
  unfinished["calls"] = json!(4);
  unfinished["inputBytes"] = json!(321);
  unfinished["totalTokens"] = json!(19);
  unfinished["remaining"] = json!(["unit-1"]);
  unfinished["attempts"] =
    json!([{"inputBytes":7,"inputHash":"retained","result":{"retained":true},"stage":"extract"}]);
  p.set_work(&unfinished);
  for id in ["done-1", "done-2", "done-3"] {
    let mut w = work(id);
    w["status"] = json!("done");
    p.set_work(&w);
    p.db()
      .execute(
        "INSERT INTO model_cache VALUES(?,?)",
        [id, "{\"useful\":true}"],
      )
      .unwrap();
  }
  subset(
    &p.ok(&["prune", "--keep-completed", "1", "--keep-caches", "1"]),
    &json!({"deletedCaches":2,"deletedCompletedWorks":2,"modelCalls":0,"retainedCaches":1,"retainedCompletedWorks":1,"unfinishedWorks":1}),
  );
  assert_eq!(p.work("unfinished"), unfinished);
  assert_eq!(p.work("done-3")["status"], "done");
  let cache: (String, String) = p
    .db()
    .query_row("SELECT key,value FROM model_cache", [], |row| {
      Ok((row.get(0)?, row.get(1)?))
    })
    .unwrap();
  assert_eq!(cache, ("done-3".into(), "{\"useful\":true}".into()));
  assert_eq!(p.graph(), empty_graph());
  assert_eq!(
    p.error(&["prune", "--keep-caches", "\u{85}"])["error"]["code"],
    "INVALID_ARGUMENT"
  );
  assert_eq!(
    p.ok(&["prune", "--keep-caches", "\u{feff}0"])["deletedCaches"],
    1
  );
}

#[test]
fn prune_validates_every_retained_extraction_before_deleting_anything() {
  let p = Project::new();
  let mut w = work("invalid");
  w["status"] = json!("done");
  p.db()
    .execute("INSERT INTO model_cache VALUES('retained','{}')", [])
    .unwrap();
  let valid = decision("notes.md", "d1", 1, "Valid decision.");
  let mut cases = vec![json!({})];
  for (field, value) in [
    ("reason", json!("")),
    ("text", json!("😀".repeat(1025))),
    ("lineEnd", json!(9_007_199_254_740_992_u64)),
    ("conditions", json!(vec!["condition"; 17])),
  ] {
    let mut d = valid.clone();
    d[field] = value;
    cases.push(json!({"decisions":[d],"relationships":[],"uncertainties":[]}));
  }
  cases.push(json!({"decisions":[],"relationships":[{"id":"r1","from":"d1","to":"d1","type":"supports","reason":"","evidence":[{"document":"notes.md","lineStart":1,"lineEnd":1}]}],"uncertainties":[]}));
  cases.push(json!({"decisions":[],"relationships":[],"uncertainties":[""]}));
  for extraction in cases {
    w["pending"] = json!({"batch":"batch","documents":[],"extraction":extraction});
    p.set_work(&w);
    let result = p.raw(&["prune", "--keep-completed", "0", "--keep-caches", "0"]);
    assert!(!result.status.success());
    assert_eq!(p.work("invalid"), w);
    assert_eq!(
      p.db()
        .query_row("SELECT count(*) FROM model_cache", [], |row| row
          .get::<_, i64>(0))
        .unwrap(),
      1
    );
    assert!(!p.path(".hivex/knowledge.lock").exists());
  }
}

#[test]
fn persisted_legacy_work_fingerprints_remain_reusable() {
  for (kind, key) in [
    (
      "update",
      "7196fc877822ee2f3634c2c9014fd0b8c507e5dd796736a64011ff3515d9a7f3",
    ),
    (
      "ask",
      "0d9874f4a3dfb82655c55f7c5200409d74f6cfd6c680fb091722b2d9afa51202",
    ),
  ] {
    let p = Project::new();
    p.write(
      "docs/cache.md",
      "# Private cache\n\nRemove cached private data when access is revoked.\n",
    );
    let id = format!("{kind}-legacy-work");
    let mut w = work(&id);
    w["key"] = json!(key);
    w["kind"] = json!(kind);
    w["phase"] = json!(kind);
    w["maxCalls"] = json!(0);
    w["remaining"] = json!(["docs/cache.md:1-3"]);
    w["plannedUnits"] = w["remaining"].clone();
    w["snapshot"] = json!("a82b745c845cc5f7cc34d71a796ffb2894f3478a3e522836456fe47eff88eea5");
    p.set_work(&w);
    let args = if kind == "ask" {
      vec![
        "ask",
        "private cache",
        "--source",
        "docs/cache.md",
        "--max-calls",
        "0",
      ]
    } else {
      vec!["update", "--max-calls", "0"]
    };
    subset(
      &p.cli(&args),
      &json!({"status":"budget-exhausted","work":{"calls":0,"id":id,"maxCalls":0}}),
    );
  }
}
