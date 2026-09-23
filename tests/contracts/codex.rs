use crate::support::{Project, list, subset};
use serde_json::{Value, json};
use std::fs;
use std::time::{Duration, Instant};

#[test]
fn compatible_versions_and_admission_boundaries() {
  for scenario in [
    "future-version",
    "instruction-source-metadata",
    "terminal-before-response",
    "catalog-21-pages",
    "empty-managed-origin",
    "normalized-endpoint",
    "configured-mcp",
    "secret-environment",
  ] {
    let p = Project::policy();
    p.model(scenario);
    let mut command = p.command(&[
      "update",
      "--max-calls",
      "2",
      "--codex",
      p.path("codex").to_str().unwrap(),
    ]);
    command
      .env("GH_TOKEN", "synthetic-not-a-secret")
      .env("DATABASE_URL", "synthetic-not-a-secret")
      .env("HTTP_PROXY", "http://synthetic.invalid")
      .env("HTTPS_PROXY", "http://synthetic.invalid")
      .env("ALL_PROXY", "http://synthetic.invalid");
    let output = crate::support::bounded(command);
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    subset(&value, &json!({"status":"ready","work":{"calls":2}}));
    assert_eq!(p.calls(), 2, "{scenario}");
  }
  for scenario in [
    "changed-effort",
    "unreadable-version",
    "catalog-22-pages",
    "ignored-mcp",
    "redirected-provider",
  ] {
    let p = Project::policy();
    p.model(scenario);
    let result = p.model_cli(&["update"]);
    subset(
      &result,
      &json!({"status":"failed","work":{"calls":1,"lastAttempt":{"code":"MODEL_ADMISSION_FAILED"}}}),
    );
    assert_eq!(p.calls(), 0, "{scenario}");
    let again = p.model_cli(&["update", "--max-calls", "8"]);
    assert_eq!(again["work"]["calls"], 1);
    assert_eq!(again["work"]["id"], result["work"]["id"]);
  }
  let p = Project::policy();
  let first = p.model_cli(&["update", "--max-calls", "1"]);
  p.model("future-version");
  let second = p.model_cli(&["update", "--max-calls", "2"]);
  assert_eq!(second["status"], "ready");
  assert_eq!(first["work"]["id"], second["work"]["id"]);
  assert_eq!(first["pendingCheck"], json!(["cache.md", "privacy.md"]));
  assert_eq!(second["pendingCheck"], json!([]));
  let work = p.work(second["work"]["id"].as_str().unwrap());
  assert_eq!(
    list(&work, "attempts")
      .iter()
      .map(|a| a["stage"].as_str().unwrap())
      .collect::<Vec<_>>(),
    ["extract", "check"]
  );
  assert!(
    list(&p.graph(), "decisions")
      .iter()
      .all(|d| d["quality"] == "checked")
  );
  assert_ne!(
    work["attempts"][0]["report"]["admission"],
    work["attempts"][1]["report"]["admission"]
  );
  assert_eq!(p.calls(), 2);
}

#[test]
fn failures_do_not_retry_implicitly_or_hide_usage() {
  for scenario in [
    "invalid-json",
    "timeout",
    "timeout-unmeasured",
    "unconfirmed-interrupt",
    "start-unconfirmed",
    "oversized-frame",
    "usage-regression",
    "duplicate-terminal",
  ] {
    let p = Project::policy();
    p.model(scenario);
    let deadline = if ["timeout", "timeout-unmeasured", "start-unconfirmed"].contains(&scenario) {
      "100"
    } else {
      "5000"
    };
    let result = p.model_cli(&["update", "--deadline-ms", deadline]);
    assert_ne!(result["status"], "ready", "{scenario}: {result}");
    assert_eq!(result["work"]["calls"], 1, "{scenario}");
    let count = p.calls();
    let repeated = p.model_cli(&["update", "--deadline-ms", "100", "--max-calls", "9"]);
    assert_eq!(repeated["work"]["calls"], 1, "{scenario}");
    assert_eq!(p.calls(), count);
    let w = p.work(result["work"]["id"].as_str().unwrap());
    assert_eq!(list(&w, "attempts").len(), 1);
    let report = &w["attempts"][0]["report"];
    match scenario {
      "invalid-json" => {
        assert_eq!(w["attempts"][0]["error"], "INVALID_KNOWLEDGE_OUTPUT");
        assert_eq!(report["outcome"], "completed");
        assert_eq!(report["usage"]["totalTokens"], 150);
      }
      "timeout" | "timeout-unmeasured" => {
        assert_eq!(report["code"], "MODEL_TIMEOUT");
        assert_eq!(report["turnAccepted"], "confirmed");
        assert_eq!(report["interruption"], "confirmed");
      }
      "start-unconfirmed" => {
        assert_eq!(report["code"], "MODEL_START_UNCONFIRMED");
        assert_eq!(report["turnAccepted"], "unknown");
        assert!(report["usage"].is_null());
      }
      "oversized-frame" => {
        assert_eq!(report["code"], "MODEL_START_UNCONFIRMED");
        assert!(
          report["diagnostic"]
            .to_string()
            .contains("frame exceeded 4 MiB"),
          "{report}"
        );
      }
      _ => {
        assert_eq!(report["code"], "MODEL_PROTOCOL_FAILED");
        assert_eq!(report["turnAccepted"], "confirmed");
        assert_eq!(report["interruption"], "unconfirmed");
      }
    }
    if [
      "timeout-unmeasured",
      "unconfirmed-interrupt",
      "usage-regression",
    ]
    .contains(&scenario)
    {
      assert!(report["usage"].is_null());
    }
    if scenario == "duplicate-terminal" {
      assert_eq!(report["usage"]["totalTokens"], 150);
    }
    assert_eq!(
      w["attempts"][0]["report"]["cleanup"], "confirmed",
      "{scenario}"
    );
    if scenario == "timeout" {
      assert_eq!(w["attempts"][0]["report"]["usage"]["totalTokens"], 125);
    }
    if ["timeout-unmeasured", "start-unconfirmed"].contains(&scenario) {
      assert_eq!(result["work"]["unmeasuredAttempts"], 1);
    }
  }
  for scenario in ["invalid-json", "changed-effort", "unreadable-version"] {
    let p = Project::policy();
    p.model(scenario);
    let first = p.model_cli(&["update"]);
    p.model("");
    let recovered = p.model_cli(&["update", "--retry-failed", "--max-calls", "3"]);
    assert_eq!(recovered["status"], "ready", "{scenario}: {recovered}");
    assert_eq!(recovered["work"]["id"], first["work"]["id"]);
    assert_eq!(recovered["work"]["calls"], 3);
    assert_eq!(
      list(&p.work(first["work"]["id"].as_str().unwrap()), "attempts").len(),
      3
    );
  }
}

#[test]
fn native_descendants_and_cancellation_are_cleaned_up() {
  let p = Project::policy();
  p.model("descendant");
  assert_eq!(p.model_cli(&["update"])["status"], "ready");
  let pids = p.read_json("pids.json");
  for pid in pids.as_array().unwrap() {
    let pid = i32::try_from(pid.as_i64().unwrap()).unwrap();
    let start = Instant::now();
    while unsafe { libc::kill(pid, 0) } == 0 && start.elapsed() < Duration::from_secs(3) {
      std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
      unsafe { libc::kill(pid, 0) },
      -1,
      "descendant still alive {pid}"
    );
  }
  let p = Project::policy();
  p.model("cancel");
  let result = p.model_cli(&["update", "--deadline-ms", "1000"]);
  assert_ne!(result["status"], "ready");
  assert_eq!(result["work"]["calls"], 1);
  let w = p.work(result["work"]["id"].as_str().unwrap());
  assert_eq!(w["attempts"][0]["report"]["cleanup"], "confirmed");
  assert_eq!(w["attempts"][0]["report"]["code"], "MODEL_CANCELLED");
}

#[test]
fn malformed_rpc_and_streams_stop_with_precise_accounting_and_cleanup() {
  for scenario in [
    "rpc-non-object",
    "rpc-invalid-id",
    "rpc-invalid-method",
    "rpc-two-outcomes",
    "rpc-remote-error",
    "rpc-partial-eof",
  ] {
    let p = Project::policy();
    p.model(scenario);
    let result = p.model_cli(&["update"]);
    subset(
      &result,
      &json!({"status":"failed","work":{"calls":1,"lastAttempt":{"code":"MODEL_ADMISSION_FAILED"}}}),
    );
    assert_eq!(p.calls(), 0);
    let w = p.work(result["work"]["id"].as_str().unwrap());
    assert!(w["attempts"][0]["report"]["usage"].is_null());
    assert_eq!(w["attempts"][0]["report"]["cleanup"], "confirmed");
  }
  for scenario in ["wrong-identity", "inconsistent-usage", "oversized-stream"] {
    let p = Project::policy();
    p.model(scenario);
    let result = p.model_cli(&["update", "--deadline-ms", "10000"]);
    assert_eq!(result["status"], "failed");
    assert_eq!(result["work"]["calls"], 1);
    let w = p.work(result["work"]["id"].as_str().unwrap());
    let report = &w["attempts"][0]["report"];
    assert_eq!(report["cleanup"], "confirmed");
    assert_eq!(
      report["code"],
      if scenario == "oversized-stream" {
        "MODEL_START_UNCONFIRMED"
      } else {
        "MODEL_PROTOCOL_FAILED"
      }
    );
    if scenario == "oversized-stream" {
      assert!(
        report["diagnostic"].to_string().contains("32 MiB"),
        "{report}"
      );
    }
    if scenario == "inconsistent-usage" {
      assert!(report["usage"].is_null());
    }
  }
}

#[test]
fn interaction_requests_are_declined_and_unrelated_rpc_responses_are_ignored() {
  for scenario in ["interactions", "rpc-out-of-order"] {
    let p = Project::policy();
    p.model(scenario);
    assert_eq!(p.model_cli(&["update"])["status"], "ready");
    assert_eq!(p.calls(), 2);
    if scenario != "interactions" {
      continue;
    }
    let records = fs::read_to_string(p.path("responses.json.interactions")).unwrap();
    let values: Vec<Value> = records
      .lines()
      .map(|line| serde_json::from_str(line).unwrap())
      .collect();
    assert_eq!(values.len(), 6);
    for value in values {
      if value["id"].as_str().unwrap().starts_with("approval-") {
        assert_eq!(value["result"]["decision"], "decline");
      } else {
        assert_eq!(value["error"]["code"], -32601);
      }
    }
  }
}

#[test]
fn codex_model_profiles_are_selectable_and_do_not_reuse_incompatible_work() {
  let p = Project::policy();
  let updated = p.model_cli(&["update"]);
  assert_eq!(updated["status"], "ready");
  assert_eq!(updated["model"]["name"], "gpt-6-luna");
  assert_eq!(updated["model"]["effort"], "max");
  let stored = p.work(updated["work"]["id"].as_str().unwrap());
  assert_eq!(
    stored["attempts"][0]["report"]["admission"]["model"],
    "gpt-6-luna"
  );
  assert_eq!(
    stored["attempts"][0]["report"]["admission"]["effort"],
    "max"
  );
  let graph = p.graph();
  let calls = p.calls();
  let profile_update = p.model_cli(&[
    "update",
    "--model",
    "fixture-model",
    "--effort",
    "high",
    "--max-calls",
    "0",
  ]);
  assert_eq!(profile_update["status"], "ready");
  assert_eq!(p.calls(), calls);
  assert_eq!(p.graph(), graph);
  assert_eq!(profile_update["model"]["name"], "fixture-model");
  let original = p.model_cli(&["ask", "cache"]);
  let before = p.calls();
  let other = p.model_cli(&[
    "ask",
    "cache",
    "--model",
    "fixture-model",
    "--effort",
    "high",
  ]);
  assert_eq!(other["status"], "ready");
  assert_ne!(other["work"]["id"], original["work"]["id"]);
  assert_eq!(p.calls(), before + 1);
  let stored = p.work(other["work"]["id"].as_str().unwrap());
  assert_eq!(stored["executionProfile"]["model"], "fixture-model");
  assert_eq!(
    stored["attempts"][0]["report"]["admission"]["model"],
    "fixture-model"
  );
  assert_eq!(
    stored["attempts"][0]["report"]["admission"]["effort"],
    "high"
  );
  assert_eq!(
    p.model_cli(&[
      "ask",
      "cache",
      "--model",
      "fixture-model",
      "--effort",
      "high"
    ]),
    other
  );
  assert_eq!(p.calls(), before + 1);
  let same_runtime = p.model_cli(&["ask", "cache", "--effort", "medium"]);
  assert_eq!(same_runtime["status"], "ready");
  assert_ne!(same_runtime["work"]["id"], original["work"]["id"]);
  let unsupported = p.model_cli(&[
    "ask",
    "unsupported",
    "--source",
    "cache.md",
    "--model",
    "missing-model",
  ]);
  assert_eq!(unsupported["status"], "failed");
  assert_eq!(
    unsupported["work"]["lastAttempt"]["code"],
    "MODEL_ADMISSION_FAILED"
  );
  assert_eq!(
    p.error(&["status", "--integration", "uninstalled"])["error"]["code"],
    "UNSUPPORTED_INTEGRATION"
  );
  assert_eq!(
    p.error(&["status", "--model-provider", "uninstalled"])["error"]["code"],
    "UNSUPPORTED_PROFILE"
  );
}

#[test]
fn pending_native_work_rejects_unrelated_profile_changes() {
  let p = Project::policy();
  let pending = p.model_cli(&["update", "--max-calls", "1"]);
  let id = pending["work"]["id"].as_str().unwrap();
  let saved = p.work(id);
  let error = p.error(&[
    "update",
    "--max-calls",
    "5",
    "--model",
    "fixture-model",
    "--effort",
    "high",
  ]);
  assert_eq!(error["error"]["code"], "EXECUTION_PROFILE_CHANGED");
  assert_eq!(p.work(id), saved);
  assert_eq!(p.calls(), 1);
  assert_eq!(
    p.model_cli(&["update", "--max-calls", "2"])["status"],
    "ready"
  );
}

#[test]
fn luna_upgrade_keeps_existing_knowledge_but_separates_cached_answers() {
  for pending in [false, true] {
    let p = Project::new();
    p.write(
      "notes.md",
      "# Policy\nUse bounded work.\nPreserve the budget.\n",
    );
    p.sql_fixture("knowledge-cache-v1.sql");
    let old_id = "84171802-e68b-43d8-b327-9ff47d302375";
    let mut old = p.work(old_id);
    if pending {
      old["status"] = json!("pending");
      old.as_object_mut().unwrap().remove("result");
      p.set_work(&old);
    }
    let graph = p.graph();
    p.model("");
    let mut responses = p.read_json("responses.json");
    responses["ask"] = json!({"answer":"Use bounded work.","evidence":[{"document":"notes.md","lineStart":2,"lineEnd":2}],"uncertainties":[]});
    p.json("responses.json", &responses);
    let args = ["ask", "bounded", "--source", "notes.md", "--max-calls", "2"];
    let new = p.model_cli(&args);
    assert_eq!(new["status"], "ready");
    assert_eq!(new["work"]["id"] == old_id, pending);
    assert_eq!(new["work"]["calls"], if pending { 2 } else { 1 });
    assert_eq!(p.calls(), 1);
    assert_eq!(p.graph(), graph);
    let caches: Vec<String> = p
      .db()
      .prepare("SELECT key FROM model_cache")
      .unwrap()
      .query_map([], |row| row.get(0))
      .unwrap()
      .collect::<Result<_, _>>()
      .unwrap();
    assert_eq!(caches.len(), 1);
    assert!(caches[0].starts_with("v2:"));
    if pending {
      assert_eq!(p.work(old_id)["attempts"][0], old["attempts"][0]);
    } else {
      assert_eq!(p.work(old_id), old);
    }
    assert_eq!(p.model_cli(&args), new);
    assert_eq!(p.calls(), 1);
    let cache_after: String = p
      .db()
      .query_row("SELECT key FROM model_cache", [], |row| row.get(0))
      .unwrap();
    assert_eq!(cache_after, caches[0]);
  }
}
