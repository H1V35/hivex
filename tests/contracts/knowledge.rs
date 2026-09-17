use crate::support::{Project, decision, list, subset};
use serde_json::{Value, json};
use std::fs;
use std::time::{Duration, Instant};

#[test]
fn native_update_and_answer_preserve_evidence_and_retained_work() {
    let p = Project::policy();
    let first = p.model_cli(&["update", "--max-calls", "1"]);
    subset(
        &first,
        &json!({"status":"budget-exhausted","work":{"calls":1}}),
    );
    let second = p.model_cli(&["update", "--max-calls", "2"]);
    subset(
        &second,
        &json!({"status":"ready","decisions":2,"relationships":1,"work":{"calls":2,"totalTokens":300}}),
    );
    assert_eq!(first["work"]["id"], second["work"]["id"]);
    let answer = p.model_cli(&["ask", "private cached data"]);
    assert_eq!(answer["status"], "ready");
    assert_eq!(
        answer["evidence"][0]["text"],
        "Revoking access immediately removes cached private data."
    );
    let count = p.calls();
    assert_eq!(p.model_cli(&["ask", "private cached data"]), answer);
    assert_eq!(p.calls(), count);
}

#[test]
fn ordinary_planning_excludes_history_and_does_not_spend_zero_budget() {
    let p = Project::policy();
    p.write("archive/old.md", "# Historical\nOld retention.\n");
    p.json("hivex.json", &json!({"history":["archive/**"]}));
    let result = p.model_cli(&["update", "--max-calls", "0"]);
    subset(
        &result,
        &json!({"status":"budget-exhausted","work":{"calls":0}}),
    );
    assert!(!result["pendingUnits"].to_string().contains("archive"));
    assert_eq!(p.calls(), 0);
    let second = p.model_cli(&["update"]);
    assert_eq!(second["work"]["id"], result["work"]["id"]);
    assert_eq!(second["work"]["calls"], 0);
}

#[test]
fn consultation_shares_maintenance_budget_and_legacy_phase_across_resumption() {
    for legacy in [false, true] {
        let p = Project::policy();
        let first = p.model_cli(&["ask", "private cached data", "--max-calls", "0"]);
        subset(
            &first,
            &json!({"status":"budget-exhausted","work":{"calls":0,"phase":"update"}}),
        );
        if legacy {
            p.db()
                .execute_batch("UPDATE work SET data=json_remove(data,'$.phase','$.cacheHits')")
                .unwrap();
        }
        let held = p.model_cli(&["ask", "private cached data"]);
        assert_eq!(held["work"]["calls"], 0);
        assert_eq!(held["work"]["id"], first["work"]["id"]);
        let checked = p.model_cli(&["ask", "private cached data", "--max-calls", "2"]);
        assert_eq!(checked["work"]["calls"], 2);
        assert_eq!(checked["status"], "budget-exhausted");
        let answer = p.model_cli(&["ask", "private cached data", "--max-calls", "3"]);
        subset(
            &answer,
            &json!({"status":"ready","work":{"calls":3,"phase":"ask","totalTokens":450}}),
        );
        assert_eq!(answer["work"]["id"], first["work"]["id"]);
        assert_eq!(p.calls(), 3);
    }
}

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
    let work = p.work(second["work"]["id"].as_str().unwrap());
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
        let deadline = if ["timeout", "timeout-unmeasured", "start-unconfirmed"].contains(&scenario)
        {
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
        let pid = pid.as_i64().unwrap() as i32;
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
fn search_uses_source_terms_and_evidence_has_only_coordinates_text_and_version() {
    let p = Project::policy();
    let mut responses = p.read_json("responses.json");
    responses["extract"]["decisions"][0]["text"] = json!("Retention is bounded.");
    p.json("responses.json", &responses);
    assert_eq!(p.model_cli(&["update"])["status"], "ready");
    let result = p.ok(&["search", "seven days"]);
    assert!(
        list(&result, "decisions")
            .iter()
            .any(|d| d["text"] == "Retention is bounded.")
    );
    let evidence = &result["decisions"][0]["evidence"];
    let mut keys = evidence
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    keys.sort();
    assert_eq!(
        keys,
        [
            "document",
            "historical",
            "lineEnd",
            "lineStart",
            "text",
            "version"
        ]
    );
    let selected = p.model_cli(&["ask", "unrelated vocabulary", "--source", "privacy.md"]);
    assert!(!selected["answer"].is_null());
    fs::remove_file(p.path("cache.md")).unwrap();
    let unavailable = p.ok(&["search", "revocation"]);
    assert!(
        unavailable["unavailableDocuments"]
            .to_string()
            .contains("cache.md")
    );
    let answer = p.model_cli(&["ask", "revocation"]);
    subset(
        &answer,
        &json!({"status":"partial","pendingDocuments":[],"work":{"calls":1}}),
    );
}

#[test]
fn historical_selection_is_explicit_and_preserves_provenance() {
    let p = Project::new();
    p.write(
        "current.md",
        "# Current\n\nUse bounded work. See [history](archive/old.md).\n",
    );
    p.write(
        "archive/old.md",
        "# Historical\n\nLegacy work may be unbounded.\n",
    );
    p.json("hivex.json", &json!({"history":["archive/**"]}));
    p.model("");
    let mut r = p.read_json("responses.json");
    r["byDocument"] = json!({"current.md":{"decisions":[decision("current.md","current",3,"Use bounded work.")],"relationships":[]},"archive/old.md":{"decisions":[decision("archive/old.md","old",3,"Legacy work may be unbounded.")],"relationships":[]}});
    r["ask"] = json!({"answer":"Historical work could be unbounded.","evidence":[{"document":"archive/old.md","lineStart":3,"lineEnd":3}],"uncertainties":[]});
    p.json("responses.json", &r);
    assert_eq!(p.model_cli(&["update"])["status"], "ready");
    assert!(
        !p.graph()["documents"]
            .as_object()
            .unwrap()
            .contains_key("archive/old.md")
    );
    let current = p.ok(&["search", "bounded"]);
    assert!(
        current["warnings"]
            .to_string()
            .contains("not been consulted")
    );
    let answer = p.model_cli(&[
        "ask",
        "Legacy",
        "--source",
        "archive/old.md",
        "--max-calls",
        "3",
    ]);
    assert_eq!(answer["evidence"][0]["historical"], true);
    assert_eq!(
        answer["evidence"][0]["text"],
        "Legacy work may be unbounded."
    );
    let old = p.ok(&["search", "Legacy", "--source", "archive/old.md"]);
    assert!(
        list(&old, "decisions")
            .iter()
            .any(|d| d["historical"] == true)
    );
}

#[test]
fn changed_sources_reuse_cached_extraction_when_original_bytes_return() {
    let p = Project::policy();
    fs::remove_file(p.path("privacy.md")).unwrap();
    let mut r = p.read_json("responses.json");
    r["extract"]["decisions"] = json!([r["extract"]["decisions"][0].clone()]);
    r["extract"]["relationships"] = json!([]);
    p.json("responses.json", &r);
    assert_eq!(p.model_cli(&["update"])["status"], "ready");
    p.write(
        "cache.md",
        "# Cache\n\nCached data expires after seven days.\n\nA new explanation.\n",
    );
    assert_eq!(p.model_cli(&["update"])["work"]["calls"], 2);
    p.write(
        "cache.md",
        "# Cache\n\nCached data expires after seven days.\n",
    );
    subset(
        &p.model_cli(&["update", "--max-calls", "0"]),
        &json!({"status":"ready","work":{"cacheHits":2,"calls":0}}),
    );
    assert_eq!(p.calls(), 4);
}

#[test]
fn large_sources_resume_incrementally_and_prioritize_matching_later_fragments() {
    for budget in [1, 2, 16] {
        let p = Project::new();
        p.write(
            "rules.md",
            (0..30)
                .map(|n| {
                    format!(
                        "## Rule {n}\n\nRule {n} requires bounded work. {}\n",
                        "Detail ".repeat(60)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );
        p.model("");
        let mut r = p.read_json("responses.json");
        r["fromVisibleRules"] = json!(true);
        p.json("responses.json", &r);
        let first = p.model_cli(&["update", "--max-calls", &budget.to_string()]);
        let before = p.graph();
        let second = p.model_cli(&["update", "--max-calls", "30"]);
        assert_eq!(second["status"], "ready", "{second}");
        assert_eq!(list(&p.graph(), "decisions").len(), 30);
        if first["status"] != "ready" {
            assert_eq!(first["work"]["id"], second["work"]["id"]);
        }
        for d in list(&before, "decisions") {
            assert!(
                p.graph()["decisions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|after| after["id"] == d["id"] && after["text"] == d["text"])
            );
        }
    }
    let p = Project::new();
    p.write(
        "rules.md",
        (0..100)
            .map(|n| {
                format!(
                    "## Rule {n}\n\nRule {n} requires cache expiry. {}{}\n",
                    "Background detail. ".repeat(24),
                    if n == 99 { " Quasar." } else { "" }
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
    );
    p.model("");
    let mut r = p.read_json("responses.json");
    r["fromVisibleRules"] = json!(true);
    p.json("responses.json", &r);
    let result = p.model_cli(&["ask", "Quasar", "--max-calls", "2"]);
    assert_eq!(result["status"], "budget-exhausted");
    assert!(
        p.ok(&["search", "Rule 99"])["decisions"]
            .to_string()
            .contains("Rule 99 requires cache expiry.")
    );
    let huge = Project::new();
    huge.write(
        "large.md",
        format!("# Large\n\n{}", "A useful bounded line.\n".repeat(110000)),
    );
    let plan = huge.cli(&["update", "--max-calls", "0"]);
    assert!(!list(&plan, "pendingUnits").is_empty());
}

#[test]
fn invalid_evidence_and_local_findings_remain_scoped() {
    let p = Project::policy();
    p.write(
        "decoration.md",
        "# Decoration\n\nAmber controls glyph colour.\n",
    );
    let mut r = p.read_json("responses.json");
    r["extract"]["decisions"]
        .as_array_mut()
        .unwrap()
        .push(decision(
            "decoration.md",
            "c3",
            3,
            "Amber controls glyph colour.",
        ));
    r["check"]["findings"] =
        json!([{"target":"c3","reason":"The decorative exception is unclear."}]);
    p.json("responses.json", &r);
    assert_eq!(p.model_cli(&["update"])["status"], "partial");
    let answer = p.model_cli(&["ask", "cache"]);
    assert_eq!(answer["status"], "ready");
    assert!(
        !answer["warnings"]
            .to_string()
            .contains("decorative exception")
    );
    assert!(
        p.ok(&["search", "Amber"])["warnings"]
            .to_string()
            .contains("decorative exception")
    );
    let p = Project::policy();
    let mut r = p.read_json("responses.json");
    r["extract"]["decisions"][0]["lineEnd"] = json!(99);
    p.json("responses.json", &r);
    assert_eq!(p.model_cli(&["update"])["status"], "partial");
    let search = p.ok(&["search", "seven days"]);
    assert!(
        list(&search, "decisions")
            .iter()
            .any(|d| d["quality"] == "uncertain")
    );
    assert!(!p.model_cli(&["ask", "cache"])["answer"].is_null());
    let p = Project::policy();
    let mut r = p.read_json("responses.json");
    r["extract"]["uncertainties"] =
        json!(["This policy does not establish current deployment state."]);
    p.json("responses.json", &r);
    let updated = p.model_cli(&["update"]);
    subset(
        &updated,
        &json!({"coverage":"current","status":"ready","warningSummary":{"limitations":1,"findings":0,"validation":0,"unknown":0}}),
    );
    assert!(
        !updated["warnings"]
            .to_string()
            .contains("Cached data expires after seven days.")
    );
    p.write("oversized.md", format!("{}\n", "x".repeat(9000)));
    subset(
        &p.model_cli(&["update", "--max-calls", "0"]),
        &json!({"coverage":"pending","status":"partial","warningSummary":{"limitations":1,"sources":1},"work":{"calls":0}}),
    );
}

#[test]
fn legacy_pre_spawn_failure_preserves_pending_check_and_attempt_history() {
    let p = Project::policy();
    let first = p.model_cli(&["update", "--max-calls", "1"]);
    let id = first["work"]["id"].as_str().unwrap();
    let mut w = p.work(id);
    w["maxCalls"] = json!(2);
    w["calls"] = json!(2);
    w["inputBytes"] = json!(w["inputBytes"].as_u64().unwrap() + 1);
    let report = json!({"cleanup":"not-observed","code":"MODEL_ADMISSION_FAILED","diagnostic":{"kind":"native-admission","message":"Knowledge execution requires verified codex-cli 0.153.2"},"outcome":"failed","usage":null});
    w["attempts"].as_array_mut().unwrap().push(json!({"inputBytes":1,"inputHash":"legacy-admission-failure","report":report,"stage":"check"}));
    w["status"] = json!("failed");
    p.set_work(&w);
    p.model("future-version");
    let result = p.model_cli(&["update", "--max-calls", "3", "--retry-failed"]);
    subset(
        &result,
        &json!({"status":"ready","pendingCheck":[],"pendingUnits":[],"work":{"calls":3,"id":id}}),
    );
    let done = p.work(id);
    assert_eq!(done["attempts"][1]["report"], report);
    assert_eq!(done["attempts"][2]["stage"], "check");
    assert_eq!(
        done["attempts"][2]["report"]["admission"]["nativeVersion"],
        "codex-cli 9.99.0"
    );
    assert_eq!(p.calls(), 2);
}

#[test]
fn uncertain_invocations_require_acknowledgement_before_explicit_retry() {
    for scenario in ["unconfirmed-interrupt", "start-unconfirmed"] {
        let p = Project::policy();
        p.model(scenario);
        let failed = p.model_cli(&["update", "--deadline-ms", "100"]);
        let id = failed["work"]["id"].as_str().unwrap();
        let before = p.work(id);
        p.model("");
        let blocked = p.error(&[
            "update",
            "--retry-failed",
            "--max-calls",
            "3",
            "--codex",
            p.path("codex").to_str().unwrap(),
        ]);
        assert_eq!(blocked["error"]["code"], "WORK_UNCERTAIN");
        assert_eq!(p.calls(), 1);
        let recovery = p.ok(&["recover", "--acknowledge-uncertain"]);
        assert_eq!(recovery["acknowledgedWorks"], 1);
        let result = p.model_cli(&["update", "--retry-failed", "--max-calls", "3"]);
        assert_eq!(result["status"], "ready");
        assert_eq!(result["work"]["calls"], 3);
        assert_eq!(
            p.work(id)["attempts"][0]["report"],
            before["attempts"][0]["report"]
        );
        assert_eq!(result["work"]["unmeasuredAttempts"], 1);
    }
}

#[test]
fn cross_batch_relations_traverse_indirect_dependencies_with_explicit_limits() {
    let p = Project::new();
    p.write(
        "01-cache.md",
        "# Cache\n\nCached data expires after seven days.\n",
    );
    for name in ["02-note.md", "03-note.md", "04-note.md"] {
        p.write(name, "# Notes\n");
    }
    p.write("05-access.md","# Permission\n\nWithdrawing authorisation destroys retained personal records immediately.\n");
    p.write(
        "06-media.md",
        "# Previews\n\nDerivative previews inherit withdrawal handling.\n",
    );
    p.model("");
    let mut r = p.read_json("responses.json");
    r["byDocument"] = json!({"01-cache.md":{"decisions":[decision("01-cache.md","c1",3,"Cached data expires after seven days.")],"relationships":[]},"05-access.md":{"decisions":[decision("05-access.md","c2",3,"Access revocation immediately purges private cache.")],"relationships":[{"id":"r1","from":"c2","to":"@existing:01-cache.md","type":"exception-to","reason":"Revocation overrides retention.","evidence":[{"document":"01-cache.md","lineStart":3,"lineEnd":3},{"document":"05-access.md","lineStart":3,"lineEnd":3}]}]},"06-media.md":{"decisions":[decision("06-media.md","c3",3,"Thumbnail caches honor access revocation.")],"relationships":[{"id":"r2","from":"c3","to":"c2","type":"requires","reason":"Thumbnail cleanup depends on revocation.","evidence":[{"document":"05-access.md","lineStart":3,"lineEnd":3},{"document":"06-media.md","lineStart":3,"lineEnd":3}]}]}});
    p.json("responses.json", &r);
    assert_eq!(
        p.model_cli(&["update", "--max-calls", "4"])["status"],
        "ready"
    );
    let found = p.ok(&["search", "seven days"]);
    let id = found["decisions"][0]["id"].as_str().unwrap();
    assert!(
        p.ok(&["neighbors", id])["decisions"]
            .to_string()
            .contains("Thumbnail caches honor access revocation.")
    );
    let bounded = p.ok(&["neighbors", id, "--limit", "2"]);
    assert_eq!(list(&bounded, "decisions").len(), 2);
    assert_eq!(list(&bounded, "unexpandedDecisions").len(), 1);
}

#[test]
fn large_document_retains_early_knowledge_and_cites_late_lines_with_omissions() {
    let p = Project::new();
    p.write(
        "cache.md",
        (0..140)
            .map(|n| {
                format!(
                    "## Rule {n}\n\nRule {n} requires cache expiry. {}\n",
                    "Detailed rationale. ".repeat(16)
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
    );
    p.model("");
    let mut r = p.read_json("responses.json");
    r["fromVisibleRules"] = json!(true);
    p.json("responses.json", &r);
    let first = p.model_cli(&["update", "--max-calls", "2"]);
    assert_eq!(first["status"], "budget-exhausted");
    assert_eq!(first["pendingDocuments"], json!(["cache.md"]));
    assert!(!list(&first, "pendingUnits").is_empty());
    assert!(
        p.ok(&["search", "Rule 0"])["decisions"]
            .to_string()
            .contains("Rule 0 requires cache expiry.")
    );
    let resumed = p.model_cli(&[
        "update",
        "--max-calls",
        "32",
        "--max-input-bytes",
        "1048576",
    ]);
    assert_eq!(resumed["status"], "ready");
    assert_eq!(resumed["pendingUnits"], json!([]));
    assert_eq!(p.ok(&["status"])["availableDecisions"], 140);
    let found = p.ok(&["search", "Rule 139"]);
    let last = list(&found, "decisions")
        .iter()
        .find(|d| d["text"] == "Rule 139 requires cache expiry.")
        .unwrap();
    assert_eq!(last["evidence"]["lineStart"], 559);
    r["ask"] = json!({"answer":"Rule 139 requires cache expiry.","evidence":[{"document":"cache.md","lineStart":559,"lineEnd":559}],"uncertainties":[]});
    p.json("responses.json", &r);
    let held = p.model_cli(&["ask", "Rule 139", "--max-calls", "0"]);
    assert_eq!(held["status"], "budget-exhausted");
    assert!(held["omittedUnits"].as_u64().unwrap() > 0);
    let answer = p.model_cli(&["ask", "Rule 139", "--max-calls", "1"]);
    assert_eq!(answer["answer"], "Rule 139 requires cache expiry.");
    assert_eq!(answer["evidence"][0]["lineStart"], 559);
    assert!(answer["omittedUnits"].as_u64().unwrap() > 0);
    assert_eq!(
        p.calls() as u64,
        resumed["work"]["calls"].as_u64().unwrap() + 1
    );
}

#[test]
fn another_snapshot_rechecks_retained_decisions_from_the_same_unit() {
    let p = Project::policy();
    fs::remove_file(p.path("privacy.md")).unwrap();
    let mut r = p.read_json("responses.json");
    r["extract"]["decisions"] = json!([r["extract"]["decisions"][0]]);
    r["extract"]["relationships"] = json!([]);
    p.json("responses.json", &r);
    assert_eq!(
        p.model_cli(&["update", "--max-calls", "1"])["status"],
        "budget-exhausted"
    );
    p.write("note.md", "# Note\n\nAdditional project background.\n");
    assert_eq!(p.model_cli(&["update"])["status"], "ready");
    fs::remove_file(p.path("note.md")).unwrap();
    assert_eq!(
        p.model_cli(&["update", "--max-calls", "2"])["status"],
        "ready"
    );
    assert_eq!(p.ok(&["status"])["uncheckedDecisions"], json!([]));
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
        if scenario == "interactions" {
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
}
