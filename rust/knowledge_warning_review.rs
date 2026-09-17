use crate::arguments::trim_js_whitespace;
use crate::documents::Document;
use crate::knowledge_model::{
    Citation, Graph, SuppliedDocument, Warning, WarningChanges, WarningResolution, WarningScope,
    is_warning_resolved, normalize_integral_numbers, supplied_citation, valid_citation,
    validate_citation, warning_id, with_warning_resolution,
};
use crate::markdown::raw_markdown_lines;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

pub const WARNING_REVIEW_INSTRUCTION: &str = " Review warningCandidates in this same check. Close only a descriptive observation that does not limit a meaningful decision, dependency or exception, or a previous closure still supported by the current documents. Reconsider the entire supplied documents, including later amendments; an unchanged cited paragraph alone is insufficient. Preserve real unanswered choices, contradictions, missing conditions and unavailable evidence. Return warningResolutions with candidate IDs, a specific reason and current citations, or an empty array. Do not repair knowledge through a warning closure or close a warning contradicted by a finding.";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WarningCandidate {
    pub id: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<WarningResolution>,
    pub scope: Vec<WarningScope>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WarningResolutionInput {
    pub evidence: Vec<Citation>,
    pub id: String,
    pub reason: String,
}

pub struct ReviewContext<'a> {
    pub documents: &'a [Document],
    pub supplied: &'a [SuppliedDocument],
    pub uncertainties: &'a [String],
}

pub struct ApplyWarningReviewOptions<'a> {
    pub candidates: &'a [WarningCandidate],
    pub documents: &'a [Document],
    pub resolutions: &'a [WarningResolutionInput],
    pub supplied: &'a [SuppliedDocument],
}

fn normalized_reason(reason: &str) -> Option<String> {
    let reason = trim_js_whitespace(reason);
    (!reason.is_empty() && reason.encode_utf16().count() <= 2048).then(|| reason.to_owned())
}

pub fn parse_warning_resolutions(value: &Value) -> Option<Vec<WarningResolutionInput>> {
    let values = value.as_array()?;
    if values.len() > 32 {
        return None;
    }
    values
        .iter()
        .map(|value| {
            let mut resolution = serde_json::from_value::<WarningResolutionInput>(
                normalize_integral_numbers(value.clone()),
            )
            .ok()?;
            if resolution.id.is_empty()
                || !(1..=32).contains(&resolution.evidence.len())
                || !resolution.evidence.iter().all(validate_citation)
            {
                return None;
            }
            resolution.reason = normalized_reason(&resolution.reason)?;
            for citation in &mut resolution.evidence {
                citation.version = None;
            }
            Some(resolution)
        })
        .collect()
}

fn complete_sources(context: &ReviewContext<'_>) -> HashSet<String> {
    context
        .documents
        .iter()
        .filter(|document| {
            supplied_citation(
                &Citation {
                    document: document.id.clone(),
                    line_end: raw_markdown_lines(&document.text).len(),
                    line_start: 1,
                    version: None,
                },
                context.supplied,
            )
        })
        .map(|document| document.id.clone())
        .collect()
}

fn candidate_value(candidate: &WarningCandidate) -> Value {
    let mut value = Map::new();
    value.insert("id".to_owned(), Value::String(candidate.id.clone()));
    value.insert(
        "message".to_owned(),
        Value::String(candidate.message.clone()),
    );
    if let Some(resolution) = &candidate.resolution {
        let mut node = Map::new();
        node.insert(
            "evidence".to_owned(),
            Value::Array(
                resolution
                    .evidence
                    .iter()
                    .map(|scope| {
                        let mut scope_value = Map::new();
                        scope_value
                            .insert("document".to_owned(), Value::String(scope.document.clone()));
                        scope_value.insert("lineEnd".to_owned(), Value::from(scope.line_end));
                        scope_value.insert("lineStart".to_owned(), Value::from(scope.line_start));
                        scope_value
                            .insert("version".to_owned(), Value::String(scope.version.clone()));
                        Value::Object(scope_value)
                    })
                    .collect(),
            ),
        );
        node.insert(
            "reason".to_owned(),
            Value::String(resolution.reason.clone()),
        );
        value.insert("resolution".to_owned(), Value::Object(node));
    }
    value.insert(
        "scope".to_owned(),
        Value::Array(
            candidate
                .scope
                .iter()
                .map(|scope| {
                    let mut scope_value = Map::new();
                    scope_value
                        .insert("document".to_owned(), Value::String(scope.document.clone()));
                    scope_value.insert("lineEnd".to_owned(), Value::from(scope.line_end));
                    scope_value.insert("lineStart".to_owned(), Value::from(scope.line_start));
                    scope_value.insert("version".to_owned(), Value::String(scope.version.clone()));
                    Value::Object(scope_value)
                })
                .collect(),
        ),
    );
    Value::Object(value)
}

pub fn warning_review_candidates(
    graph: &Graph,
    context: &ReviewContext<'_>,
    max_bytes: Option<usize>,
) -> Vec<WarningCandidate> {
    let complete = complete_sources(context);
    let mut candidates: Vec<_> = graph
        .warnings
        .iter()
        .filter_map(|warning| {
            if matches!(warning, Warning::Legacy(_))
                || is_warning_resolved(warning, context.documents)
            {
                return None;
            }
            let Warning::Structured(record) = warning else {
                return None;
            };
            let limitation = record.kind.as_deref() == Some("limitation")
                && context
                    .uncertainties
                    .iter()
                    .any(|message| message == &record.message);
            let closure = record.resolution.is_some()
                && record.target.is_none()
                && !matches!(record.kind.as_deref(), Some("finding" | "validation"));
            let mut evidence = record.scope.clone();
            if let Some(resolution) = &record.resolution {
                evidence.extend(resolution.evidence.clone());
            }
            if !(limitation || closure)
                || evidence.is_empty()
                || evidence
                    .iter()
                    .any(|entry| !complete.contains(&entry.document))
            {
                return None;
            }
            Some(WarningCandidate {
                id: warning_id(warning),
                message: record.message.clone(),
                resolution: record.resolution.clone(),
                scope: record.scope.clone(),
            })
        })
        .collect();
    candidates.sort_by_key(|candidate| usize::from(candidate.resolution.is_some()));
    candidates.truncate(32);
    if let Some(max_bytes) = max_bytes {
        while !candidates.is_empty() {
            let value = Value::Array(candidates.iter().map(candidate_value).collect());
            if serde_json::to_vec(&value).is_ok_and(|bytes| bytes.len() <= max_bytes) {
                break;
            }
            candidates.pop();
        }
    }
    candidates
}

pub fn apply_warning_review(graph: &Graph, options: ApplyWarningReviewOptions<'_>) -> Graph {
    let candidates: HashMap<_, _> = options
        .candidates
        .iter()
        .map(|candidate| (candidate.id.as_str(), candidate))
        .collect();
    let context = ReviewContext {
        documents: options.documents,
        supplied: options.supplied,
        uncertainties: &[],
    };
    let complete = complete_sources(&context);
    let mut accepted = HashMap::new();
    for resolution in options.resolutions {
        if !candidates.contains_key(resolution.id.as_str()) {
            continue;
        }
        let valid = normalized_reason(&resolution.reason).is_some()
            && !resolution.id.is_empty()
            && (1..=32).contains(&resolution.evidence.len())
            && resolution.evidence.iter().all(|citation| {
                complete.contains(&citation.document)
                    && supplied_citation(citation, options.supplied)
                    && valid_citation(citation, options.documents)
            });
        if valid {
            accepted.entry(resolution.id.clone()).or_insert(resolution);
        }
    }
    let warnings = graph
        .warnings
        .iter()
        .map(|warning| {
            let id = warning_id(warning);
            let Some(resolution) = accepted.get(&id) else {
                return warning.clone();
            };
            let candidate = candidates.get(id.as_str());
            let mut document_ids = HashSet::new();
            if let Some(candidate) = candidate {
                document_ids.extend(candidate.scope.iter().map(|scope| scope.document.clone()));
                if let Some(old) = &candidate.resolution {
                    document_ids.extend(old.evidence.iter().map(|scope| scope.document.clone()));
                }
            }
            document_ids.extend(
                resolution
                    .evidence
                    .iter()
                    .map(|citation| citation.document.clone()),
            );
            let evidence: Vec<_> = options
                .documents
                .iter()
                .filter(|document| document_ids.contains(&document.id))
                .map(|document| WarningScope {
                    document: document.id.clone(),
                    line_end: raw_markdown_lines(&document.text).len(),
                    line_start: 1,
                    version: document.hash.clone(),
                })
                .collect();
            if evidence.len() > 32 {
                return warning.clone();
            }
            with_warning_resolution(
                warning,
                WarningResolution {
                    evidence,
                    reason: trim_js_whitespace(&resolution.reason).to_owned(),
                },
            )
        })
        .collect();
    Graph {
        warnings,
        ..graph.clone()
    }
}

pub fn warning_baseline(graph: &Graph) -> Map<String, Value> {
    let mut baseline = Map::new();
    for warning in &graph.warnings {
        baseline.insert(
            warning_id(warning),
            Value::String(
                if matches!(warning, Warning::Structured(record) if record.resolution.is_some()) {
                    "resolved"
                } else {
                    "active"
                }
                .to_owned(),
            ),
        );
    }
    baseline
}

pub fn warning_changes(
    graph: &Graph,
    documents: &[Document],
    baseline: &Map<String, Value>,
) -> WarningChanges {
    let entries: Vec<_> = graph
        .warnings
        .iter()
        .map(|warning| {
            let state = if is_warning_resolved(warning, documents) {
                "resolved"
            } else {
                "active"
            };
            (warning_id(warning), warning, state)
        })
        .collect();
    let mut changes = WarningChanges::default();
    for (id, warning, state) in entries {
        let previous = baseline.get(&id).and_then(Value::as_str);
        let message = match warning {
            Warning::Legacy(message) => message.clone(),
            Warning::Structured(record) => record.message.clone(),
        };
        let change = || crate::knowledge_model::WarningChange {
            id: id.clone(),
            message: message.clone(),
            state: state.to_owned(),
        };
        if previous.is_none() {
            changes.new.push(change());
        }
        if previous == Some("resolved") && state == "active" {
            changes.reopened.push(change());
        }
        if previous != Some("resolved") && state == "resolved" {
            changes.resolved.push(change());
        }
    }
    changes
}
