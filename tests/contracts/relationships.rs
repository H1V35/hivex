use crate::support::{Project, decision, list, subset};
use serde_json::{Value, json};
use std::fs;
fn packets(p: &Project) -> Vec<Value> {
    fs::read_to_string(p.path("responses.json.packets"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn independent_supporting_evidence_is_supplied_and_invalidated_on_change() {
    let p = Project::policy();
    p.write(
        "scope.md",
        "# Scope\n\nRevocation overrides cache retention for private records.\n",
    );
    let mut r = p.read_json("responses.json");
    r["extract"]["relationships"][0]["evidence"] =
        json!([{"document":"scope.md","lineStart":3,"lineEnd":3}]);
    r["ask"]["evidence"] = json!([{"document":"scope.md","lineStart":3,"lineEnd":3}]);
    p.json("responses.json", &r);
    assert_eq!(p.model_cli(&["update"])["relationships"], 1);
    let found = p.ok(&["search", "seven days"]);
    let id = found["decisions"][0]["id"].as_str().unwrap();
    let answer = p.model_cli(&["ask", "seven days"]);
    assert!(!answer["answer"].is_null());
    subset(
        &answer["evidence"][0],
        &json!({"document":"scope.md","lineStart":3,"lineEnd":3,"text":"Revocation overrides cache retention for private records."}),
    );
    let captured = packets(&p);
    assert!(
        list(captured.last().unwrap(), "documents")
            .iter()
            .any(|d| d["id"] == "scope.md")
    );
    p.write(
        "scope.md",
        "# Scope\n\nThis relationship is awaiting a decision.\n",
    );
    let neighbors = p.ok(&["neighbors", id]);
    assert!(list(&neighbors, "relationships").is_empty());
    assert_eq!(list(&neighbors, "unexpandedDecisions").len(), 1);
}

#[test]
fn endpoint_maintenance_pauses_before_spending_when_required_context_cannot_fit() {
    let p = Project::policy();
    let line = "The blue pulse condition governs this exception. ".repeat(3);
    p.write(
        "scope.md",
        format!("# Scope\n\n{}\n", vec![line; 70].join("\n")),
    );
    let mut r = p.read_json("responses.json");
    r["extract"]["relationships"][0]["evidence"] =
        json!([{"document":"scope.md","lineStart":3,"lineEnd":72}]);
    p.json("responses.json", &r);
    assert_eq!(p.model_cli(&["update"])["status"], "ready");
    p.write(
        "cache.md",
        "# Cache\n\nCached data expires after two days.\n",
    );
    let mut edge = r["extract"]["relationships"][0].clone();
    edge["from"] = json!("@existing:privacy.md");
    edge["requiresEvidenceDocument"] = json!("scope.md");
    r["byDocument"] = json!({"cache.md":{"decisions":[decision("cache.md","c1",3,"Cached data expires after two days.")],"relationships":[edge]}});
    p.json("responses.json", &r);
    let before = p.calls();
    let limited = p.model_cli(&[
        "ask",
        "cache",
        "--max-calls",
        "3",
        "--max-context-bytes",
        "4096",
    ]);
    subset(
        &limited,
        &json!({"status":"context-limit","work":{"calls":0}}),
    );
    assert!(list(&limited["work"]["contextLimit"], "documents").contains(&json!("scope.md")));
    assert_eq!(p.calls(), before);
    let resumed = p.model_cli(&[
        "ask",
        "cache",
        "--max-calls",
        "3",
        "--max-context-bytes",
        "65536",
    ]);
    subset(&resumed, &json!({"status":"ready","work":{"calls":3}}));
    assert_eq!(resumed["work"]["id"], limited["work"]["id"]);
    assert_eq!(list(&p.ok(&["search", "cache"]), "relationships").len(), 1);
    let p = Project::policy();
    let first = p.model_cli(&[
        "ask",
        "cache",
        "--max-calls",
        "2",
        "--max-context-bytes",
        "2048",
    ]);
    assert_eq!(first["work"]["calls"], 2);
    let second = p.model_cli(&[
        "ask",
        "cache",
        "--max-calls",
        "3",
        "--max-context-bytes",
        "65536",
    ]);
    assert_eq!(second["status"], "ready");
    assert_eq!(second["work"]["id"], first["work"]["id"]);
    assert_eq!(second["work"]["calls"], 3);
}

#[test]
fn changed_supporting_source_precedes_unrelated_pending_sources() {
    let p = Project::policy();
    p.write(
        "z-scope.md",
        "# Scope\n\nThe blue pulse activates this exception.\n",
    );
    let mut r = p.read_json("responses.json");
    r["extract"]["relationships"][0]["evidence"] =
        json!([{"document":"z-scope.md","lineStart":3,"lineEnd":3}]);
    p.json("responses.json", &r);
    p.model_cli(&["update"]);
    for name in ["b.md", "c.md", "d.md", "e.md"] {
        p.write(name, "# Decoration\n\nAmber controls glyphs.\n");
    }
    p.write(
        "z-scope.md",
        "# Scope\n\nThe blue pulse no longer activates this exception.\n",
    );
    r["byDocument"] = json!({"z-scope.md":{"decisions":[],"relationships":[]}});
    p.json("responses.json", &r);
    let result = p.model_cli(&["ask", "cache"]);
    assert_eq!(result["work"]["calls"], 3);
    assert!(!list(&result, "pendingDocuments").contains(&json!("z-scope.md")));
    assert!(!list(&result, "pendingDocuments").is_empty());
    assert!(!list(&result, "unavailableDocuments").contains(&json!("z-scope.md")));
}

#[test]
fn supporting_source_repair_can_remove_unsupported_relation_with_unchanged_endpoints() {
    let p = Project::policy();
    p.write(
        "privacy.md",
        "# Access\n\nPrivate reports require explicit authorisation.\n",
    );
    p.write(
        "scope.md",
        "# Scope\n\nThe examples do not approve an exception to retention.\n",
    );
    let mut r = p.read_json("responses.json");
    r["extract"]["decisions"][1]["text"] = json!("Private reports require explicit authorisation.");
    r["extract"]["relationships"][0]["evidence"] =
        json!([{"document":"scope.md","lineStart":3,"lineEnd":3}]);
    r["check"]["findings"] =
        json!([{"target":"r1","reason":"The claimed exception is unsupported."}]);
    p.json("responses.json", &r);
    assert_eq!(p.model_cli(&["update"])["relationships"], 1);
    let before = p.graph();
    r["byDocument"] = json!({"scope.md":{"decisions":[],"relationships":[]}});
    r["check"] = json!({"findings":[],"relationshipChanges":[{"previousId":"@removed:0","replacements":[],"reason":"The scope excludes the claimed exception.","evidence":[{"document":"scope.md","lineStart":3,"lineEnd":3}]}]});
    p.json("responses.json", &r);
    subset(
        &p.model_cli(&[
            "update",
            "--repair",
            "scope.md",
            "--reason",
            "Remove the unsupported exception.",
        ]),
        &json!({"status":"ready","decisions":2,"relationships":0}),
    );
    assert_eq!(p.graph()["decisions"], before["decisions"]);
}

#[test]
fn questioned_relationships_limit_answers_and_document_findings_stay_local() {
    let p = Project::policy();
    let mut r = p.read_json("responses.json");
    r["check"]["findings"] =
        json!([{"target":"r1","reason":"The exception relationship is uncertain."}]);
    p.json("responses.json", &r);
    assert_eq!(p.model_cli(&["update"])["status"], "partial");
    assert_eq!(p.model_cli(&["ask", "cache"])["status"], "partial");
    let p = Project::policy();
    p.write(
        "decoration.md",
        "# Decoration\n\nAmber controls glyph colour.\n",
    );
    let mut r = p.read_json("responses.json");
    r["check"]["findings"] =
        json!([{"target":"decoration.md","reason":"The glyph decision was omitted."}]);
    p.json("responses.json", &r);
    assert_eq!(p.model_cli(&["update"])["status"], "partial");
    let result = p.model_cli(&["ask", "cache"]);
    assert_eq!(result["status"], "ready");
    assert!(!result["warnings"].to_string().contains("glyph decision"));
    assert!(
        p.ok(&["search", "Amber"])["warnings"]
            .to_string()
            .contains("glyph decision")
    );
}

#[test]
fn full_required_authority_exceeds_optional_cap_but_respects_hard_bound() {
    let p = Project::new();
    p.write("target.md", "# Target\n\nTarget before revision.\n");
    p.write(
        "authority.md",
        format!(
            "# Authority\n\n{}",
            (0..8)
                .map(|n| format!("Context evidence {n}: {}\n", "synthetic detail ".repeat(80)))
                .collect::<String>()
        ),
    );
    p.model("");
    let mut r = p.read_json("responses.json");
    let mut authority = decision(
        "authority.md",
        "authority",
        3,
        "Authority rule covers the supplied context.",
    );
    authority["lineEnd"] = json!(10);
    let mut edge = json!({"id":"target-authority","from":"target","to":"authority","type":"requires","reason":"Target uses the authority.","evidence":[{"document":"target.md","lineStart":3,"lineEnd":3},{"document":"authority.md","lineStart":3,"lineEnd":10}]});
    r["byDocument"] = json!({"authority.md":{"decisions":[authority],"relationships":[]},"target.md":{"decisions":[decision("target.md","target",3,"Target before revision.")],"relationships":[edge]}});
    p.json("responses.json", &r);
    assert_eq!(p.model_cli(&["update"])["status"], "ready");
    p.write("target.md", "# Target\n\nTarget after revision.\n");
    edge["to"] = json!("@existing:authority.md");
    r["byDocument"]["target.md"] = json!({"decisions":[decision("target.md","target",3,"Target after revision.")],"relationships":[edge]});
    p.json("responses.json", &r);
    assert_eq!(
        p.model_cli(&["update", "--max-context-bytes", "32768"])["status"],
        "ready"
    );
    let captured = packets(&p);
    let extraction = captured
        .iter()
        .rev()
        .find(|v| v["operation"] == "extract")
        .unwrap();
    let context = list(extraction, "documents")
        .iter()
        .find(|d| d["id"] == "authority.md")
        .unwrap();
    assert!(context["lines"].to_string().len() > 8192);
    assert!(
        list(extraction, "existing")
            .iter()
            .any(|d| d["document"] == "authority.md")
    );
    let calls = p.calls();
    p.write("target.md", "# Target\n\nTarget limited revision.\n");
    subset(
        &p.model_cli(&["update", "--max-context-bytes", "1024"]),
        &json!({"status":"context-limit","work":{"calls":0,"contextLimit":{"maxBytes":1024}}}),
    );
    assert_eq!(p.calls(), calls);
}

#[test]
fn earlier_rounds_restore_after_an_intervening_document_version() {
    for budget in [1, 2, 16] {
        let p = Project::new();
        let original = (0..90)
            .map(|n| {
                format!(
                    "## Rule {n}\n\nRule {n} requires cache expiry. {}\n",
                    "Reason. ".repeat(40)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        p.write("cache.md", &original);
        p.model("");
        let mut r = p.read_json("responses.json");
        r["fromVisibleRules"] = json!(true);
        p.json("responses.json", &r);
        let first = p.model_cli(&[
            "update",
            "--max-calls",
            &budget.to_string(),
            "--max-input-bytes",
            "1048576",
        ]);
        assert_eq!(
            first["status"],
            if budget < 16 {
                "budget-exhausted"
            } else {
                "ready"
            }
        );
        p.write("cache.md", original.replacen("Rule 0", "Rule zero", 1));
        let intermediate = p.model_cli(&[
            "update",
            "--max-calls",
            if budget == 1 { "16" } else { "2" },
            "--max-input-bytes",
            "1048576",
        ]);
        assert_eq!(
            intermediate["status"],
            if budget == 1 {
                "ready"
            } else {
                "budget-exhausted"
            }
        );
        p.write("cache.md", &original);
        let resumed = p.model_cli(&[
            "update",
            "--max-calls",
            "16",
            "--max-input-bytes",
            "1048576",
        ]);
        assert_eq!(resumed["status"], "ready");
        if budget < 16 {
            assert_eq!(resumed["work"]["id"], first["work"]["id"]);
        } else {
            assert_eq!(resumed["work"]["calls"], 0);
        }
        subset(
            &p.ok(&["status"]),
            &json!({"availableDecisions":90,"pendingDocuments":[]}),
        );
    }
}

#[test]
fn unrelated_history_does_not_change_work_and_unconsulted_references_stay_partial() {
    let p = Project::policy();
    p.write(
        "archive/replaced.md",
        "# Replaced\n\nThe old cache rule allowed seven days.\n",
    );
    p.json(
        "hivex.json",
        &json!({"history":["archive/**/*.md"],"include":["*.md"]}),
    );
    let first = p.cli(&[
        "ask",
        "old cache rule",
        "--max-calls",
        "0",
        "--codex",
        "/no-model",
    ]);
    subset(&first, &json!({"status":"budget-exhausted","answer":null}));
    assert!(
        !list(&first, "documents")
            .iter()
            .any(|d| d["id"] == "archive/replaced.md")
    );
    assert!(!list(&first, "pendingDocuments").contains(&json!("archive/replaced.md")));
    p.write(
        "archive/replaced.md",
        "# Replaced\n\nUnrelated historical note.\n",
    );
    let resumed = p.cli(&["ask", "old cache rule", "--codex", "/no-model"]);
    assert_eq!(resumed["work"]["id"], first["work"]["id"]);
    assert_eq!(resumed["work"]["maxCalls"], 0);
    let p = Project::policy();
    p.write(
        "archive/exception.md",
        "# Exception\n\nThe old exemption.\n",
    );
    p.write("cache.md","# Cache\n\nCached data expires after seven days.\n\nSee the [exception](archive/exception.md).\n");
    p.json("hivex.json", &json!({"history":["archive/**/*.md"]}));
    p.model_cli(&["update"]);
    let answer = p.model_cli(&["ask", "cache retention"]);
    assert_eq!(answer["status"], "partial");
    assert!(
        answer["warnings"]
            .to_string()
            .contains("archive/exception.md")
    );
    assert_eq!(answer["work"]["calls"], 1);
}

#[test]
fn consulted_historical_dependency_survives_snapshot_and_supplies_changed_evidence() {
    let p = Project::policy();
    p.write(
        "archive/replaced.md",
        "# Replaced\n\nThe old cache rule allowed seven days. See [current](../cache.md).\n",
    );
    p.json(
        "hivex.json",
        &json!({"history":["archive/**/*.md"],"include":["*.md"]}),
    );
    let mut r = p.read_json("responses.json");
    r["byDocument"] = json!({"cache.md":{"decisions":[r["extract"]["decisions"][0]],"relationships":[]},"privacy.md":{"decisions":[r["extract"]["decisions"][1]],"relationships":[]},"archive/replaced.md":{"decisions":[decision("archive/replaced.md","history-rule",3,"The old cache rule allowed seven days.")],"relationships":[{"id":"history-to-cache","from":"history-rule","to":"@existing:cache.md","type":"supersedes","reason":"Current rule replaced historical retention.","evidence":[{"document":"archive/replaced.md","lineStart":3,"lineEnd":3},{"document":"cache.md","lineStart":3,"lineEnd":3}]}]}});
    r["ask"] = json!({"answer":"Current rule superseded historical retention.","evidence":[{"document":"archive/replaced.md","lineStart":3,"lineEnd":3}],"uncertainties":[]});
    p.json("responses.json", &r);
    p.model_cli(&["update"]);
    assert_eq!(
        p.model_cli(&["ask", "old cache rule", "--source", "archive/replaced.md"])["status"],
        "ready"
    );
    let answer = p.model_cli(&["ask", "cache retention"]);
    assert_eq!(answer["evidence"][0]["historical"], true);
    assert!(!list(&answer, "pendingDocuments").contains(&json!("archive/replaced.md")));
    p.write("archive/unread.md", "# Unrelated history\n\nOld rule.\n");
    assert_eq!(p.ok(&["snapshot", "export"])["status"], "ready");
    let clone = Project::policy();
    for file in [
        "hivex.json",
        "archive/replaced.md",
        "archive/unread.md",
        ".hivex/graph.json",
    ] {
        clone.write(file, fs::read(p.path(file)).unwrap());
    }
    let found = clone.ok(&["search", "cache retention"]);
    let id = found["decisions"][0]["id"].as_str().unwrap();
    assert!(
        list(&clone.ok(&["neighbors", id]), "decisions")
            .iter()
            .any(|d| d["document"] == "archive/replaced.md" && d["status"] == "historical")
    );
    assert!(!list(&found, "pendingDocuments").contains(&json!("archive/unread.md")));
    assert!(!clone.path(".hivex/knowledge.sqlite").exists());
    clone.ok(&["snapshot", "import"]);
    let limited = clone.cli(&[
        "ask",
        "retention",
        "--source",
        "cache.md",
        "--limit",
        "1",
        "--max-calls",
        "0",
        "--codex",
        "/no-model",
    ]);
    let expanded = clone.cli(&[
        "ask",
        "retention",
        "--source",
        "cache.md",
        "--limit",
        "2",
        "--codex",
        "/no-model",
    ]);
    assert_eq!(expanded["work"]["id"], limited["work"]["id"]);
    assert_eq!(expanded["work"]["calls"], 0);
    p.write(
        "cache.md",
        "# Cache\n\nCached data expires after seven days.\n\nNew current context.\n",
    );
    let pending = p.model_cli(&["update", "--max-calls", "1"]);
    p.write(
        "archive/replaced.md",
        "# Replaced\n\nUpdated historical explanation.\n",
    );
    let resumed = p.model_cli(&["update", "--max-calls", "2"]);
    assert_eq!(resumed["work"]["id"], pending["work"]["id"]);
    assert_eq!(resumed["work"]["calls"], 2);
    let captured = packets(&p);
    assert_eq!(captured.last().unwrap()["operation"], "extract");
    assert!(
        captured.last().unwrap()["documents"]
            .to_string()
            .contains("Updated historical explanation.")
    );
}

#[test]
fn all_affected_endpoints_survive_the_optional_context_cap() {
    let p = Project::new();
    p.write("target.md", "# Target\n\nTarget rules before revision.\n");
    p.write(
        "endpoints.md",
        "# Endpoints\n\nEndpoint zero. Endpoint one.\n",
    );
    p.model("");
    let mut r = p.read_json("responses.json");
    let targets: Vec<Value> = (0..19)
        .map(|n| {
            decision(
                "target.md",
                &format!("target-{n}"),
                3,
                &format!("Target rule {n} is revised with its endpoint."),
            )
        })
        .collect();
    let endpoints: Vec<Value> = (0..2)
        .map(|n| {
            decision(
                "endpoints.md",
                &format!("endpoint-{n}"),
                3,
                &format!("Endpoint {n} remains available."),
            )
        })
        .collect();
    let edges:Vec<Value>=(0..19).map(|n|json!({"id":format!("old-edge-{n}"),"from":format!("target-{n}"),"to":if n==18{"endpoint-1"}else{"endpoint-0"},"type":"requires","reason":"Target uses an endpoint.","evidence":[{"document":"target.md","lineStart":3,"lineEnd":3},{"document":"endpoints.md","lineStart":3,"lineEnd":3}]})).collect();
    r["byDocument"] = json!({"endpoints.md":{"decisions":endpoints,"relationships":[]},"target.md":{"decisions":targets,"relationships":edges}});
    p.json("responses.json", &r);
    assert_eq!(p.model_cli(&["update"])["status"], "ready");
    let graph = p.graph();
    let endpoint = list(&graph, "decisions")
        .iter()
        .find(|d| d["localId"] == "endpoint-1")
        .unwrap()["id"]
        .clone();
    p.write("target.md", "# Target\n\nTarget rules after revision.\n");
    r["byDocument"]["target.md"]["decisions"][0]["text"] = json!("Restored target sentinel omega.");
    let mut edge = edges[0].clone();
    edge["to"] = endpoint.clone();
    edge["id"] = json!("restored-edge");
    r["byDocument"]["target.md"]["relationships"] = json!([edge]);
    p.json("responses.json", &r);
    assert_eq!(p.model_cli(&["update"])["status"], "ready");
    let captured = packets(&p);
    let packet = captured
        .iter()
        .rev()
        .find(|v| v["operation"] == "extract")
        .unwrap();
    assert_eq!(packet["targets"], json!(["target.md"]));
    assert!(list(packet, "existing").iter().any(|d| d["id"] == endpoint));
    let snapshot = p.ok(&["snapshot", "export"]);
    subset(
        &snapshot,
        &json!({"decisions":21,"relationships":1,"status":"ready"}),
    );
    let graph = p.graph();
    let target = list(&graph, "decisions")
        .iter()
        .find(|d| d["localId"] == "target-0")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let neighbors = p.ok(&["neighbors", target]);
    assert!(
        list(&neighbors, "decisions")
            .iter()
            .any(|d| d["id"] == endpoint)
    );
    assert_eq!(neighbors["relationships"][0]["localId"], "restored-edge");
}

#[test]
fn untouched_later_units_do_not_become_required_context_for_a_pending_unit() {
    let p = Project::new();
    p.write(
        "notes.md",
        (0..16)
            .map(|n| {
                format!(
                    "# Section {n}\n\nRule {n} requires bounded work. {}\n",
                    "Detail ".repeat(800)
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
    );
    p.model("");
    let mut r = p.read_json("responses.json");
    r["byDocument"] = json!({"notes.md":{"decisions":[],"relationships":[]}});
    p.json("responses.json", &r);
    let plan = p.model_cli(&["update", "--max-calls", "0"]);
    let source = p.ok(&["sources"])["documents"][0].clone();
    let mut graph = crate::support::empty_graph();
    for n in 1..16 {
        let line = 4 * n + 3;
        let mut d = decision(
            "notes.md",
            &format!("node{n}"),
            line,
            &format!("Rule {n} requires bounded work."),
        );
        d["localId"] = json!(format!("c{n}"));
        d["version"] = source["hash"].clone();
        d["quality"] = json!("checked");
        d["batch"] = json!("synthetic-seed");
        graph["decisions"].as_array_mut().unwrap().push(d);
        if n > 1 {
            graph["relationships"].as_array_mut().unwrap().push(json!({"id":format!("edge{n}"),"localId":format!("r{n}"),"from":format!("node{}",n-1),"to":format!("node{n}"),"type":"supports","reason":"Related later rules.","quality":"checked","batch":"synthetic-seed","evidence":[{"document":"notes.md","lineStart":line,"lineEnd":line,"version":source["hash"]}]}));
        }
    }
    for unit in list(&plan, "pendingUnits").iter().skip(1) {
        graph["units"][unit.as_str().unwrap()] =
            json!({"document":"notes.md","version":source["hash"]});
    }
    p.set_graph(&graph);
    subset(
        &p.model_cli(&["update", "--max-calls", "1"]),
        &json!({"decisions":15,"relationships":14,"pendingCheck":["notes.md"],"status":"budget-exhausted","work":{"calls":1,"contextLimit":null}}),
    );
}

#[test]
fn changed_decision_refreshes_incoming_dependency_beyond_lexical_and_recent_matches() {
    let p = Project::new();
    p.write(
        "01-cache.md",
        "# Cache\n\nCached data expires after seven days.\n",
    );
    p.write(
        "02-privacy.md",
        "# Access\n\nRevoking access immediately removes cached private data.\n",
    );
    p.model("");
    let mut r = p.read_json("responses.json");
    let edge = json!({"id":"exception","from":"c2","to":"c1","type":"exception-to","reason":"Revocation overrides retention.","evidence":[{"document":"01-cache.md","lineStart":3,"lineEnd":3},{"document":"02-privacy.md","lineStart":3,"lineEnd":3}]});
    r["byDocument"] = json!({"01-cache.md":{"decisions":[decision("01-cache.md","c1",3,"Cached data expires after seven days.")],"relationships":[]},"02-privacy.md":{"decisions":[decision("02-privacy.md","c2",3,"Access revocation immediately purges private cache.")],"relationships":[edge]}});
    for n in 3..=8 {
        let file = format!("0{n}-note.md");
        let text = format!("Amber {n} controls decorative glyphs.");
        p.write(&file, format!("# Decoration {n}\n\n{text}\n"));
        r["byDocument"][&file] =
            json!({"decisions":[decision(&file,&format!("c{n}"),3,&text)],"relationships":[]});
    }
    r["ask"]["evidence"] = json!([{"document":"02-privacy.md","lineStart":3,"lineEnd":3}]);
    p.json("responses.json", &r);
    assert_eq!(
        p.model_cli(&["update", "--max-calls", "4"])["status"],
        "ready"
    );
    let found = p.ok(&["search", "revocation"]);
    let id = found["decisions"][0]["id"].clone();
    p.write(
        "01-cache.md",
        "# Timer\n\nFreshness renews every second sunrise.\n",
    );
    r["byDocument"]["01-cache.md"]["decisions"][0]["text"] =
        json!("Freshness renews every second sunrise.");
    let mut edge = edge;
    edge["from"] = json!("@existing:02-privacy.md");
    r["byDocument"]["01-cache.md"]["relationships"] = json!([edge]);
    r["ask"]["answer"] = json!("Freshness renews every second sunrise; revocation takes priority.");
    p.json("responses.json", &r);
    subset(
        &p.model_cli(&["ask", "Freshness"]),
        &json!({"status":"ready","work":{"calls":3}}),
    );
    let neighbors = p.ok(&["neighbors", id.as_str().unwrap()]);
    assert_eq!(list(&neighbors, "relationships").len(), 1);
    assert!(list(&neighbors, "decisions").iter().any(|d| d["id"] == id));
    assert!(
        neighbors["decisions"]
            .to_string()
            .contains("Freshness renews every second sunrise.")
    );
}
