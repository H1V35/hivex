#[cfg(test)]
mod tests {
    use crate::documents::{Document, Project};
    use crate::knowledge_model::{
        CheckFinding, Citation, Extraction, ExtractionDecision, ExtractionOptions,
        ExtractionRelationship, Graph, KnowledgeCheck, SuppliedDocument, Warning, WarningRecord,
        WarningScope, apply_check, apply_extraction, check_impact, empty_graph, graph_value,
        is_warning_resolved, parse_check, parse_extraction, parse_graph, source_evidence,
        supplied_citation, validate_check, validate_extraction, validate_graph, warning_id,
        warning_summary, with_warning_resolution,
    };
    use crate::knowledge_serialization::stringify_knowledge;
    use crate::knowledge_warning_review::{
        ApplyWarningReviewOptions, ReviewContext, apply_warning_review, parse_warning_resolutions,
        warning_baseline, warning_changes, warning_review_candidates,
    };
    use serde_json::Value;
    use std::path::PathBuf;

    fn document(id: &str, text: &str) -> Document {
        Document {
            id: id.to_owned(),
            path: id.to_owned(),
            title: id.to_owned(),
            text: text.to_owned(),
            hash: format!("hash-{id}"),
            status: None,
            links: Vec::new(),
            historical: false,
        }
    }

    fn source() -> Document {
        let mut source = document(
            "docs/cache.md",
            "# Private cache\n\nRemove cached private data when access is revoked.\n",
        );
        source.hash = "c6faa807ee055873a73b0e8b823511be81bfaa4b69bc3e141cafc636cfc31c2a".to_owned();
        source.title = "Private cache".to_owned();
        source
    }

    fn extraction() -> Extraction {
        Extraction {
            decisions: vec![
                ExtractionDecision {
                    conditions: Vec::new(),
                    document: "docs/cache.md".to_owned(),
                    exceptions: Vec::new(),
                    id: "c1".to_owned(),
                    kind: "constraint".to_owned(),
                    line_end: 3,
                    line_start: 3,
                    reason: "Revoked access must not retain data.".to_owned(),
                    status: "current".to_owned(),
                    text: "Remove cached private data when access is revoked.".to_owned(),
                },
                ExtractionDecision {
                    conditions: Vec::new(),
                    document: "docs/cache.md".to_owned(),
                    exceptions: Vec::new(),
                    id: "c2".to_owned(),
                    kind: "decision".to_owned(),
                    line_end: 3,
                    line_start: 3,
                    reason: "Keep cache behavior aligned with access.".to_owned(),
                    status: "current".to_owned(),
                    text: "Remove cached private data when access is revoked.".to_owned(),
                },
            ],
            relationships: vec![ExtractionRelationship {
                evidence: vec![Citation {
                    document: "docs/cache.md".to_owned(),
                    line_end: 3,
                    line_start: 3,
                    version: None,
                }],
                from: "c1".to_owned(),
                id: "r1".to_owned(),
                reason: "Both refer to revoking cached data.".to_owned(),
                to: "c2".to_owned(),
                relationship_type: "supports".to_owned(),
            }],
            uncertainties: Vec::new(),
        }
    }

    #[test]
    fn extraction_identities_are_bound_to_source_versions_and_evidence() {
        let source = source();
        let graph = apply_extraction(ExtractionOptions {
            batch: "fixture",
            context_documents: None,
            context_ranges: None,
            documents: std::slice::from_ref(&source),
            existing_ids: None,
            extraction: &extraction(),
            graph: &empty_graph(),
            target_ranges: None,
        });
        assert_eq!(
            graph
                .decisions
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "fda25af550b379877147825c3e375f5aa6ad5398fc4664dc8d062bc115a18afc",
                "dc94835ead5426394740d8d63fe98c5ce70e6522fe44e6adaf8e0a94fa2c3c31",
            ]
        );
        assert_eq!(
            graph
                .relationships
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["8df360ba09823f9f5ef282ab21b4634d3d15f7e9b1212b0f986eeb6a80fe2c74"]
        );
        assert!(
            graph
                .decisions
                .iter()
                .all(|entry| entry.quality == "unchecked")
        );
        assert!(
            graph.relationships[0].evidence[0].version.as_deref()
                == Some("c6faa807ee055873a73b0e8b823511be81bfaa4b69bc3e141cafc636cfc31c2a")
        );
    }

    #[test]
    fn check_impact_and_application_keep_unrelated_provenance() {
        let source = source();
        let graph = apply_extraction(ExtractionOptions {
            batch: "fixture",
            context_documents: None,
            context_ranges: None,
            documents: std::slice::from_ref(&source),
            existing_ids: None,
            extraction: &extraction(),
            graph: &empty_graph(),
            target_ranges: None,
        });
        let check = KnowledgeCheck {
            findings: vec![CheckFinding {
                reason: "Review c1.".to_owned(),
                target: graph.decisions[0].id.clone(),
            }],
        };
        let impact = check_impact(&graph, &check, "fixture", &[]);
        assert_eq!(impact.decision_ids.len(), 1);
        assert_eq!(impact.relationship_ids.len(), 1);
        let checked = apply_check(&graph, &check, "fixture", &[]);
        assert_eq!(checked.decisions[0].quality, "uncertain");
        assert_eq!(checked.relationships[0].quality, "uncertain");
        assert_eq!(checked.decisions[0].batch, "fixture");
    }

    #[test]
    fn live_serialization_keeps_provenance_after_source_fields() {
        let value = serde_json::json!({
            "id": "d1",
            "document": "notes.md",
            "text": "Keep evidence.",
            "kind": "decision",
            "status": "current",
            "conditions": [],
            "exceptions": [],
            "reason": "Source reason.",
            "lineStart": 1,
            "lineEnd": 1,
            "localId": "local",
            "version": "v1",
            "batch": "b1",
            "quality": "unchecked",
        });
        let live = value;
        assert_eq!(
            stringify_knowledge(&live),
            r#"{"id":"d1","document":"notes.md","text":"Keep evidence.","kind":"decision","status":"current","conditions":[],"exceptions":[],"reason":"Source reason.","lineStart":1,"lineEnd":1,"localId":"local","version":"v1","batch":"b1","quality":"unchecked"}"#
        );
    }

    #[test]
    fn live_serialization_keeps_finding_fields_after_provenance() {
        let values = [
            (
                "decision",
                serde_json::json!({
                    "id": "d1",
                    "document": "docs/cache.md",
                    "text": "Keep cache private",
                    "kind": "constraint",
                    "status": "current",
                    "conditions": [],
                    "exceptions": [],
                    "reason": "Access",
                    "lineStart": 1,
                    "lineEnd": 1,
                    "localId": "c1",
                    "version": "v1",
                    "batch": "b1",
                    "quality": "accepted",
                    "target": "d2"
                }),
                r#"{"id":"d1","document":"docs/cache.md","text":"Keep cache private","kind":"constraint","status":"current","conditions":[],"exceptions":[],"reason":"Access","lineStart":1,"lineEnd":1,"localId":"c1","version":"v1","batch":"b1","quality":"accepted","target":"d2"}"#,
            ),
            (
                "relationship",
                serde_json::json!({
                    "id": "r1",
                    "from": "d1",
                    "to": "d2",
                    "type": "supports",
                    "reason": "Access",
                    "evidence": [],
                    "localId": "r1",
                    "batch": "b1",
                    "quality": "accepted",
                    "target": "d2"
                }),
                r#"{"id":"r1","from":"d1","to":"d2","type":"supports","reason":"Access","evidence":[],"localId":"r1","batch":"b1","quality":"accepted","target":"d2"}"#,
            ),
        ];
        for (_, value, expected) in values {
            assert_eq!(stringify_knowledge(&value), expected);
        }
    }

    #[test]
    fn warning_closures_keep_identity_and_report_baseline_changes() {
        let source = source();
        let scope = WarningScope {
            document: source.id.clone(),
            line_end: 3,
            line_start: 3,
            version: source.hash.clone(),
        };
        let warning = Warning::Structured(WarningRecord {
            kind: Some("limitation".to_owned()),
            message: "Need context.".to_owned(),
            scope: vec![scope.clone()],
            ..WarningRecord::default()
        });
        let graph = Graph {
            warnings: vec![warning.clone()],
            ..empty_graph()
        };
        let candidates = warning_review_candidates(
            &graph,
            &ReviewContext {
                documents: std::slice::from_ref(&source),
                supplied: &[crate::knowledge_model::SuppliedDocument {
                    id: source.id.clone(),
                    lines: vec![
                        vec![Value::from(1), Value::String("# Private cache".to_owned())],
                        vec![Value::from(2), Value::String(String::new())],
                        vec![
                            Value::from(3),
                            Value::String(
                                "Remove cached private data when access is revoked.".to_owned(),
                            ),
                        ],
                    ],
                }],
                uncertainties: &["Need context.".to_owned()],
            },
            None,
        );
        assert_eq!(candidates.len(), 1);
        let closed = apply_warning_review(
            &graph,
            ApplyWarningReviewOptions {
                candidates: &candidates,
                documents: std::slice::from_ref(&source),
                resolutions: &[crate::knowledge_warning_review::WarningResolutionInput {
                    evidence: vec![Citation {
                        document: source.id.clone(),
                        line_end: 3,
                        line_start: 3,
                        version: None,
                    }],
                    id: candidates[0].id.clone(),
                    reason: "Current evidence is sufficient.".to_owned(),
                }],
                supplied: &[crate::knowledge_model::SuppliedDocument {
                    id: source.id.clone(),
                    lines: vec![
                        vec![Value::from(1), Value::String("# Private cache".to_owned())],
                        vec![Value::from(2), Value::String(String::new())],
                        vec![
                            Value::from(3),
                            Value::String(
                                "Remove cached private data when access is revoked.".to_owned(),
                            ),
                        ],
                    ],
                }],
            },
        );
        let baseline = warning_baseline(&closed);
        let changes = warning_changes(&closed, std::slice::from_ref(&source), &baseline);
        assert!(changes.new.is_empty());
        assert!(changes.reopened.is_empty());
        assert!(changes.resolved.is_empty());
        assert_eq!(warning_id(&warning), warning_id(&closed.warnings[0]));
    }

    #[test]
    fn supplied_lines_accept_numeric_strings_and_source_evidence_keeps_line_text() {
        let source = source();
        let supplied = SuppliedDocument {
            id: source.id.clone(),
            lines: vec![
                vec![
                    Value::String("1".to_owned()),
                    Value::String("heading".to_owned()),
                ],
                vec![Value::String("2".to_owned()), Value::String(String::new())],
                vec![
                    Value::String("3".to_owned()),
                    Value::String("rule".to_owned()),
                ],
            ],
        };
        let citation = Citation {
            document: source.id.clone(),
            line_end: 3,
            line_start: 1,
            version: None,
        };
        assert!(supplied_citation(&citation, &[supplied]));
        let project = Project {
            root: PathBuf::new(),
            snapshot: String::new(),
            current_snapshot: String::new(),
            documents: vec![source.clone()],
            current_documents: vec![source.clone()],
            historical_documents: Vec::new(),
            warnings: Vec::new(),
        };
        let evidence = source_evidence(&citation, &project).expect("source is present");
        assert_eq!(
            evidence.text,
            "# Private cache\n\nRemove cached private data when access is revoked."
        );
        assert!(!is_warning_resolved(
            &Warning::Legacy("legacy".to_owned()),
            &project.documents
        ));
    }

    #[test]
    fn relationship_with_missing_version_is_invalidated_by_context() {
        let source = source();
        let mut graph = apply_extraction(ExtractionOptions {
            batch: "fixture",
            context_documents: None,
            context_ranges: None,
            documents: std::slice::from_ref(&source),
            existing_ids: None,
            extraction: &extraction(),
            graph: &empty_graph(),
            target_ranges: None,
        });
        graph.relationships[0].evidence[0].version = None;
        let next = apply_extraction(ExtractionOptions {
            batch: "later",
            context_documents: Some(std::slice::from_ref(&source)),
            context_ranges: None,
            documents: std::slice::from_ref(&source),
            existing_ids: None,
            extraction: &Extraction::default(),
            graph: &graph,
            target_ranges: None,
        });
        assert!(next.relationships.is_empty());
        assert_eq!(next.decisions.len(), graph.decisions.len());
    }

    #[test]
    fn invalid_relationship_id_does_not_poison_a_following_valid_entry() {
        let source = source();
        let mut extraction = extraction();
        extraction.relationships = vec![
            ExtractionRelationship {
                evidence: extraction.relationships[0].evidence.clone(),
                from: "c1".to_owned(),
                id: "duplicate".to_owned(),
                reason: "The endpoint is not known yet.".to_owned(),
                to: "missing".to_owned(),
                relationship_type: "supports".to_owned(),
            },
            ExtractionRelationship {
                evidence: extraction.relationships[0].evidence.clone(),
                from: "c1".to_owned(),
                id: "duplicate".to_owned(),
                reason: "The endpoint is known here.".to_owned(),
                to: "c2".to_owned(),
                relationship_type: "supports".to_owned(),
            },
        ];
        let graph = apply_extraction(ExtractionOptions {
            batch: "fixture",
            context_documents: None,
            context_ranges: None,
            documents: std::slice::from_ref(&source),
            existing_ids: None,
            extraction: &extraction,
            graph: &empty_graph(),
            target_ranges: None,
        });
        assert_eq!(graph.relationships.len(), 1);
        assert_eq!(graph.warnings.len(), 1);
        assert!(matches!(
            &graph.warnings[0],
            Warning::Structured(record) if record.message == "Relationship duplicate has an unknown endpoint."
        ));
    }

    #[test]
    fn validators_cover_schema_limits_without_normalizing_live_order() {
        let source = source();
        let extraction = extraction();
        assert!(validate_extraction(&extraction));
        assert!(validate_check(&KnowledgeCheck {
            findings: vec![CheckFinding {
                reason: "Review the source.".to_owned(),
                target: "c1".to_owned(),
            }],
        }));
        let graph = apply_extraction(ExtractionOptions {
            batch: "fixture",
            context_documents: None,
            context_ranges: None,
            documents: std::slice::from_ref(&source),
            existing_ids: None,
            extraction: &extraction,
            graph: &empty_graph(),
            target_ranges: None,
        });
        assert!(validate_graph(&graph));
        let mut invalid = graph.clone();
        invalid.decisions[0].quality = "accepted".to_owned();
        assert!(!validate_graph(&invalid));
        let mut invalid_extraction = extraction;
        invalid_extraction.decisions[0].conditions = vec!["x".to_owned(); 17];
        assert!(!validate_extraction(&invalid_extraction));
        assert_eq!(
            warning_summary(&graph.warnings, std::slice::from_ref(&source)).unknown,
            0
        );
    }

    #[test]
    fn warning_resolution_preserves_identity_and_previous_closures() {
        let source = source();
        let scope = WarningScope {
            document: source.id.clone(),
            line_end: 3,
            line_start: 3,
            version: source.hash.clone(),
        };
        let warning = Warning::Structured(WarningRecord {
            kind: Some("limitation".to_owned()),
            message: "Need a current review.".to_owned(),
            scope: vec![scope.clone()],
            ..WarningRecord::default()
        });
        // Fixed identifier from the v0.3.11 TypeScript warningId contract.
        assert_eq!(
            warning_id(&warning),
            "5a41ab5c5e1d50b8b61ab55737ef36c53bc2523f6f53b1db790fe18a5355fe52"
        );
        let first = with_warning_resolution(
            &warning,
            crate::knowledge_model::WarningResolution {
                evidence: vec![scope.clone()],
                reason: "First review.".to_owned(),
            },
        );
        let second = with_warning_resolution(
            &first,
            crate::knowledge_model::WarningResolution {
                evidence: vec![scope],
                reason: "Second review.".to_owned(),
            },
        );
        let Warning::Structured(record) = &second else {
            panic!("expected structured warning");
        };
        assert_eq!(record.previous_resolutions.len(), 1);
        assert!(is_warning_resolved(&second, std::slice::from_ref(&source)));
        assert_eq!(warning_id(&warning), warning_id(&second));
    }

    #[test]
    fn parsers_validate_model_input_and_keep_store_map_order() {
        let source = source();
        let extraction = extraction();
        let encoded_extraction = serde_json::to_value(&extraction).expect("extraction encodes");
        assert_eq!(
            parse_extraction(&encoded_extraction),
            Some(extraction.clone())
        );

        let graph = apply_extraction(ExtractionOptions {
            batch: "fixture",
            context_documents: None,
            context_ranges: None,
            documents: std::slice::from_ref(&source),
            existing_ids: None,
            extraction: &extraction,
            graph: &empty_graph(),
            target_ranges: None,
        });
        let encoded_graph = graph_value(&graph, false);
        let parsed = parse_graph(&encoded_graph, true).expect("valid graph parses");
        assert_eq!(parsed.decisions.len(), graph.decisions.len());
        assert_eq!(
            graph_value(&parsed, false)["decisions"][0]
                .as_object()
                .expect("stored decision")
                .keys()
                .nth(10),
            Some(&"version".to_owned())
        );

        let mut integral_float = encoded_graph.clone();
        integral_float
            .as_object_mut()
            .expect("graph object")
            .get_mut("version")
            .expect("version")
            .clone_from(&serde_json::json!(1.0));
        for decision in integral_float
            .get_mut("decisions")
            .and_then(Value::as_array_mut)
            .expect("decisions")
        {
            decision
                .as_object_mut()
                .expect("decision")
                .get_mut("lineStart")
                .expect("lineStart")
                .clone_from(&serde_json::json!(3.0));
            decision
                .as_object_mut()
                .expect("decision")
                .get_mut("lineEnd")
                .expect("lineEnd")
                .clone_from(&serde_json::json!(3.0));
        }
        let parsed_float = parse_graph(&integral_float, true).expect("integral floats parse");
        assert_eq!(parsed_float.version, 1);
        assert!(
            parsed_float
                .decisions
                .iter()
                .all(|decision| decision.line_start == 3 && decision.line_end == 3)
        );

        let mut null_optional = encoded_graph.clone();
        null_optional
            .as_object_mut()
            .expect("graph object")
            .get_mut("lastExtraction")
            .expect("lastExtraction")
            .clone_from(&Value::Null);
        assert!(parse_graph(&null_optional, false).is_none());
        let mut null_version = encoded_graph.clone();
        null_version["relationships"][0]["evidence"][0]["version"] = Value::Null;
        assert!(parse_graph(&null_version, false).is_none());

        let mut invalid = encoded_graph;
        invalid
            .as_object_mut()
            .expect("graph object")
            .get_mut("version")
            .expect("version")
            .clone_from(&Value::from(2));
        assert!(parse_graph(&invalid, true).is_none());
        assert!(parse_graph(&invalid, false).is_none());

        let mut unknown = graph_value(&graph, false);
        unknown
            .as_object_mut()
            .expect("graph object")
            .insert("futureField".to_owned(), Value::Bool(true));
        assert!(parse_graph(&unknown, true).is_none());
        assert!(parse_graph(&unknown, false).is_some());
    }

    #[test]
    fn graph_serialization_preserves_each_record_order_and_optional_presence() {
        let source = source();
        let mut graph = apply_extraction(ExtractionOptions {
            batch: "fixture",
            context_documents: None,
            context_ranges: None,
            documents: std::slice::from_ref(&source),
            existing_ids: None,
            extraction: &extraction(),
            graph: &empty_graph(),
            target_ranges: None,
        });
        graph.decisions[1].field_order = [
            "id",
            "document",
            "text",
            "kind",
            "status",
            "conditions",
            "exceptions",
            "reason",
            "lineStart",
            "lineEnd",
            "version",
            "batch",
            "localId",
            "quality",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let value = graph_value(&graph, true);
        let decisions = value
            .get("decisions")
            .and_then(Value::as_array)
            .expect("decisions");
        assert_eq!(
            decisions[0]
                .as_object()
                .expect("live decision")
                .keys()
                .nth(10),
            Some(&"localId".to_owned())
        );
        assert_eq!(
            decisions[1]
                .as_object()
                .expect("store decision")
                .keys()
                .nth(10),
            Some(&"version".to_owned())
        );
        let normalized = parse_graph(&value, true).expect("mixed graph parses");
        let normalized_value = graph_value(&normalized, true);
        let normalized_decisions = normalized_value["decisions"]
            .as_array()
            .expect("normalized decisions");
        assert!(normalized_decisions.iter().all(|decision| {
            decision
                .as_object()
                .and_then(|decision| decision.keys().nth(10))
                .is_some_and(|key| key == "version")
        }));

        let warning = Warning::Structured(WarningRecord {
            message: "An explicitly empty history is retained.".to_owned(),
            previous_resolutions_present: true,
            scope: Vec::new(),
            ..WarningRecord::default()
        });
        graph.warnings.push(warning);
        let encoded = graph_value(&graph, false);
        let warning = encoded
            .get("warnings")
            .and_then(Value::as_array)
            .and_then(|warnings| warnings.last())
            .and_then(Value::as_object)
            .expect("warning object");
        assert_eq!(
            warning.get("previousResolutions"),
            Some(&Value::Array(Vec::new()))
        );
        let parsed = parse_graph(&encoded, true).expect("graph parses");
        assert!(matches!(
            parsed.warnings.last(),
            Some(Warning::Structured(record)) if record.previous_resolutions_present
        ));
    }

    #[test]
    fn parsers_apply_zod_like_limits_and_trim_review_reasons() {
        let value = serde_json::json!([
            {
                "evidence": [{
                    "document": "docs/cache.md",
                    "lineStart": 3.0,
                    "lineEnd": 3.0,
                    "version": "ignored-by-review-schema"
                }],
                "id": "warning-id",
                "reason": "  Current evidence is sufficient.  "
            }
        ]);
        let resolutions = parse_warning_resolutions(&value).expect("valid resolutions");
        assert_eq!(resolutions[0].reason, "Current evidence is sufficient.");
        assert_eq!(resolutions[0].evidence[0].version, None);
        assert!(
            parse_warning_resolutions(&serde_json::json!([{
                "evidence": [],
                "id": "warning-id",
                "reason": "closure"
            }]))
            .is_none()
        );

        let check = parse_check(&serde_json::json!({
            "findings": [{"target": "d1", "reason": "Needs review."}],
            "futureField": true
        }))
        .expect("check parses and unknown fields are stripped");
        assert_eq!(check.findings[0].target, "d1");
    }

    #[test]
    fn decision_identity_uses_utf8_json_with_utf16_length_validation() {
        let mut source = document("docs/😀.md", "Keep é😀.\n");
        source.hash = "v-é😀".to_owned();
        let extraction = Extraction {
            decisions: vec![ExtractionDecision {
                conditions: vec!["é😀".to_owned()],
                document: source.id.clone(),
                exceptions: Vec::new(),
                id: "local-é😀".to_owned(),
                kind: "decision".to_owned(),
                line_end: 1,
                line_start: 1,
                reason: "R".to_owned(),
                status: "current".to_owned(),
                text: "Keep é😀.".to_owned(),
            }],
            relationships: Vec::new(),
            uncertainties: Vec::new(),
        };
        let graph = apply_extraction(ExtractionOptions {
            batch: "unicode",
            context_documents: None,
            context_ranges: None,
            documents: std::slice::from_ref(&source),
            existing_ids: None,
            extraction: &extraction,
            graph: &empty_graph(),
            target_ranges: None,
        });
        assert_eq!(
            graph.decisions[0].id,
            "732df427a25af5819ab01ba47288232b4b01282366a5b6afcb8a6c65143a5803"
        );
        assert!(validate_extraction(&extraction));
    }
    #[test]
    fn rejects_unsafe_ranges_and_matches_javascript_citation_whitespace() {
        let maximum = crate::arguments::MAX_SAFE_INTEGER as usize;
        let mut candidate = extraction();
        candidate.decisions[0].line_start = maximum;
        candidate.decisions[0].line_end = maximum;
        assert!(validate_extraction(&candidate));
        candidate.decisions[0].line_end = maximum + 1;
        assert!(!validate_extraction(&candidate));
        let unsafe_citation = Citation {
            document: "notes.md".to_owned(),
            line_start: maximum + 1,
            line_end: maximum + 1,
            version: None,
        };
        assert!(!crate::knowledge_model::validate_citation(&unsafe_citation));
        let answer = serde_json::json!({"answer":"Bounded evidence", "evidence":[{"document":"notes.md","lineStart":maximum+1,"lineEnd":maximum+1}], "uncertainties":[]});
        assert!(
            crate::model_runtime::OutputSchema::Answer
                .parse(&answer)
                .is_none()
        );
        let resolution = serde_json::json!([{"id":"warning", "reason":"Bounded evidence", "evidence":[{"document":"notes.md","lineStart":maximum+1,"lineEnd":maximum+1}]}]);
        assert!(parse_warning_resolutions(&resolution).is_none());

        let reversed = Citation {
            document: "notes.md".to_owned(),
            line_start: 2,
            line_end: 1,
            version: None,
        };
        assert!(!supplied_citation(&reversed, &[]));
        let citation = Citation {
            document: "notes.md".to_owned(),
            line_start: 1,
            line_end: 1,
            version: None,
        };
        assert!(crate::knowledge_model::valid_citation(
            &citation,
            &[document("notes.md", "\u{85}")]
        ));
        assert!(!crate::knowledge_model::valid_citation(
            &citation,
            &[document("notes.md", "\u{feff}")]
        ));
        let mut graph = apply_extraction(ExtractionOptions {
            batch: "safe-integer",
            context_documents: None,
            context_ranges: None,
            documents: &[source()],
            existing_ids: None,
            extraction: &extraction(),
            graph: &empty_graph(),
            target_ranges: None,
        });
        graph.decisions[0].line_end = maximum + 1;
        assert!(!validate_graph(&graph));
    }
    #[test]
    fn check_targets_preserve_the_scope_and_provenance_of_retained_knowledge() {
        let source = source();
        let mut graph = apply_extraction(ExtractionOptions {
            batch: "current",
            context_documents: None,
            context_ranges: None,
            documents: std::slice::from_ref(&source),
            existing_ids: None,
            extraction: &extraction(),
            graph: &empty_graph(),
            target_ranges: None,
        });
        for decision in &mut graph.decisions {
            decision.quality = "checked".to_owned();
        }
        let mut neighbor = graph.decisions[0].clone();
        neighbor.id = "retained-neighbor".to_owned();
        neighbor.document = "neighbor.md".to_owned();
        neighbor.batch = "previous".to_owned();
        neighbor.local_id = "neighbor-local".to_owned();
        graph.decisions.push(neighbor);
        graph.relationships[0].to = "retained-neighbor".to_owned();
        graph.relationships[0].quality = "checked".to_owned();
        let mut previous_edge = graph.relationships[0].clone();
        previous_edge.id = "previous-edge".to_owned();
        previous_edge.batch = "previous".to_owned();
        graph.relationships.push(previous_edge);
        let check = |target: &str| KnowledgeCheck {
            findings: vec![CheckFinding {
                reason: "Inspect this scope.".to_owned(),
                target: target.to_owned(),
            }],
        };
        for target in [&graph.decisions[0].id, &graph.decisions[0].local_id] {
            let impact = check_impact(&graph, &check(target), "current", &[]);
            assert_eq!(
                impact.decision_ids,
                [graph.decisions[0].id.clone()].into_iter().collect()
            );
            assert_eq!(impact.relationship_ids.len(), 2);
            assert!(!impact.is_uncertain_batch);
        }
        for target in ["unknown-target", "batch"] {
            let impact = check_impact(&graph, &check(target), "current", &[]);
            assert!(impact.is_uncertain_batch);
            assert!(impact.decision_ids.is_empty());
            let applied = apply_check(&graph, &check(target), "current", &[]);
            assert!(
                applied.decisions[..2]
                    .iter()
                    .all(|entry| entry.quality == "uncertain")
            );
            assert_eq!(applied.relationships[0].quality, "uncertain");
            assert_eq!(applied.relationships[1].quality, "checked");
        }
        let impact = check_impact(&graph, &check(&source.id), "current", &[]);
        assert_eq!(impact.decision_ids.len(), 2);
        assert_eq!(impact.relationship_ids.len(), 2);
        let scope = [WarningScope {
            document: "neighbor.md".to_owned(),
            line_start: 1,
            line_end: 1,
            version: "v1".to_owned(),
        }];
        let applied = apply_check(&graph, &check("neighbor.md"), "current", &scope);
        assert_eq!(applied.decisions[2].batch, "previous");
        assert_eq!(applied.decisions[2].quality, "uncertain");
        assert_eq!(applied.relationships[1].quality, "checked");
        graph.decisions[2].quality = "uncertain".to_owned();
        let check = KnowledgeCheck {
            findings: Vec::new(),
        };
        let impact = check_impact(&graph, &check, "current", &[]);
        assert!(!impact.is_uncertain_batch);
        assert!(impact.decision_ids.is_empty());
        assert!(impact.relationship_ids.is_empty());
        assert_eq!(
            apply_check(&graph, &check, "current", &[]).relationships[0].quality,
            "uncertain"
        );
    }
}
