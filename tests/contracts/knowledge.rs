use crate::support::{Project, decision, list, subset};
use serde_json::{Value, json};
use std::fs;

#[test]
fn native_update_and_answer_preserve_evidence_and_retained_work() {
    let p = Project::policy();
    p.write("archive/old.md", "# Replaced policy\nOld retention.\n");
    p.json("hivex.json", &json!({"history":["archive/**"]}));
    let sources = p.ok(&["sources"]);
    let first = p.model_cli(&["update", "--max-calls", "1"]);
    subset(
        &first,
        &json!({"status":"budget-exhausted","work":{"calls":1}}),
    );
    p.json("hivex.json", &json!({"archive":["archive/**"]}));
    assert_eq!(p.ok(&["sources"]), sources);
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
    p.json("hivex.json", &json!({"history":["archive/**"]}));
    assert_eq!(p.model_cli(&["ask", "private cached data"]), answer);
    assert_eq!(p.calls(), count);
}

#[test]
fn ordinary_planning_excludes_history_and_does_not_spend_zero_budget() {
    let p = Project::policy();
    p.write("archive/old.md", "# Historical\nOld retention.\n");
    p.json("hivex.json", &json!({"archive":["archive/**"]}));
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
    let p = Project::policy();
    p.write(
        "archive/replaced.md",
        "# Replaced\n\nThe old cache rule allowed seven days.\n",
    );
    p.json(
        "hivex.json",
        &json!({"archive":["archive/**/*.md"],"include":["*.md"]}),
    );
    let mut r = p.read_json("responses.json");
    r["byDocument"] = json!({"archive/replaced.md":{"decisions":[decision("archive/replaced.md","history-rule",3,"The old cache rule allowed seven days.")],"relationships":[]}});
    r["ask"] = json!({"answer":"The old cache rule allowed seven days.","evidence":[{"document":"archive/replaced.md","lineStart":3,"lineEnd":3}],"uncertainties":[]});
    p.json("responses.json", &r);
    let first = p.model_cli(&[
        "ask",
        "old cache rule",
        "--source",
        "archive/replaced.md",
        "--max-calls",
        "2",
    ]);
    subset(
        &first,
        &json!({"status":"budget-exhausted","work":{"calls":2,"maxCalls":2}}),
    );
    let answer = p.model_cli(&[
        "ask",
        "old cache rule",
        "--source",
        "archive/replaced.md",
        "--max-calls",
        "3",
    ]);
    assert_eq!(answer["answer"], "The old cache rule allowed seven days.");
    assert_eq!(answer["work"]["id"], first["work"]["id"]);
    assert_eq!(answer["work"]["calls"], 3);
    subset(
        &answer["evidence"][0],
        &json!({"document":"archive/replaced.md","historical":true,"text":"The old cache rule allowed seven days."}),
    );
    let found = p.ok(&[
        "search",
        "old cache rule",
        "--source",
        "archive/replaced.md",
    ]);
    assert!(
        list(&found, "decisions")
            .iter()
            .any(|d| d["historical"] == true && d["status"] == "historical")
    );
    subset(
        &p.model_cli(&[
            "update",
            "--repair",
            "archive/replaced.md",
            "--reason",
            "Check the historical lifetime.",
        ]),
        &json!({"status":"ready","work":{"calls":2}}),
    );
    p.write(
        "cache.md",
        "# Cache\n\nCurrent cache expires after eight days.\n",
    );
    assert_eq!(p.model_cli(&["update"])["status"], "ready");
    let text = fs::read_to_string(p.path("responses.json.packets")).unwrap();
    let captured: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let ordinary = captured
        .iter()
        .rev()
        .find(|v| v["operation"] == "extract")
        .unwrap();
    assert!(!list(ordinary, "targets").contains(&json!("archive/replaced.md")));
    assert!(
        !list(ordinary, "documents")
            .iter()
            .any(|d| d["id"] == "archive/replaced.md")
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
fn prioritizes_matching_fragment_and_plans_large_documents() {
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
fn omitted_consultation_call_limit_preserves_nonzero_exhausted_budget() {
    let p = Project::policy();
    let first = p.model_cli(&["ask", "cache", "--max-calls", "1"]);
    subset(&first["work"], &json!({"calls":1,"maxCalls":1}));
    let repeated = p.model_cli(&["ask", "cache"]);
    subset(
        &repeated,
        &json!({"status":"budget-exhausted","work":{"calls":1,"maxCalls":1,"id":first["work"]["id"]}}),
    );
    assert_eq!(p.calls(), 1);
}
