use crate::arguments::{self, trim_js_whitespace};
use crate::documents::{Project, load_project};
use crate::error::{HivexError, Result};
use crate::knowledge_model::{
    Citation, Graph, Warning, WarningResolution, empty_graph, graph_value, is_warning_resolved,
    parse_graph, valid_citation, warning_id, warning_summary, warning_value,
    with_warning_resolution,
};
use crate::knowledge_snapshot::{shared_knowledge, stored_graph};
use crate::store::{Store, StoreOptions};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

const MAX_RESOLUTION_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug)]
struct Resolution {
    evidence: Vec<Citation>,
    id: String,
    reason: String,
}

fn invalid_resolution(message: impl Into<String>) -> HivexError {
    HivexError::new("INVALID_RESOLUTION", message)
}

fn invalid_snapshot() -> HivexError {
    HivexError::new(
        "INVALID_SNAPSHOT",
        "Knowledge snapshot is not a supported graph JSON document.",
    )
}

fn parse_resolution(value: &Value) -> Result<Resolution> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid_resolution("Each resolution must be an object."))?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "evidence" | "id" | "reason"))
    {
        return Err(invalid_resolution(
            "Each resolution must contain only evidence, id and reason.",
        ));
    }
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| invalid_resolution("Each resolution needs a non-empty warning ID."))?;
    let reason = object
        .get("reason")
        .and_then(Value::as_str)
        .map(trim_js_whitespace)
        .filter(|reason| !reason.is_empty() && reason.encode_utf16().count() <= 2048)
        .ok_or_else(|| invalid_resolution("Each resolution needs a bounded reason."))?;
    let evidence = object
        .get("evidence")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_resolution("Each resolution needs evidence."))?;
    if !(1..=32).contains(&evidence.len()) {
        return Err(invalid_resolution(
            "Each resolution needs between one and 32 citations.",
        ));
    }
    let evidence = evidence
        .iter()
        .map(|value| {
            let value = crate::knowledge_model::normalize_integral_numbers(value.clone());
            let citation: Citation = serde_json::from_value(value)
                .map_err(|_| invalid_resolution("Each resolution citation is invalid."))?;
            if citation.version.as_deref().is_none_or(str::is_empty) {
                return Err(invalid_resolution(
                    "Each resolution citation needs a non-empty version.",
                ));
            }
            Ok(citation)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Resolution {
        evidence,
        id: id.to_owned(),
        reason: reason.to_owned(),
    })
}

fn read_resolutions(path: &Path) -> Result<Vec<Resolution>> {
    let metadata = fs::metadata(path)
        .map_err(|error| invalid_resolution(format!("Unable to read resolutions: {error}")))?;
    if metadata.len() > MAX_RESOLUTION_FILE_BYTES {
        return Err(invalid_resolution("Resolution file exceeds 2 MiB."));
    }
    let bytes = fs::read(path)
        .map_err(|error| invalid_resolution(format!("Unable to read resolutions: {error}")))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| invalid_resolution("Resolution file must contain valid JSON."))?;
    let values = value
        .as_array()
        .ok_or_else(|| invalid_resolution("Resolution file must contain an array."))?;
    if !(1..=1024).contains(&values.len()) {
        return Err(invalid_resolution(
            "Resolution file must contain between one and 1024 entries.",
        ));
    }
    values.iter().map(parse_resolution).collect()
}

fn store_graph(store: &Store, root: &Path) -> Result<Value> {
    match store.graph() {
        Ok(graph) => Ok(graph),
        Err(error) if error.code == "SNAPSHOT_REQUIRED" => {
            if store.unfinished()? {
                Ok(graph_value(&empty_graph(), false))
            } else {
                shared_knowledge(root)
            }
        }
        Err(error) => Err(error),
    }
}

fn resolve_warnings(graph: &Graph, project: &Project, resolutions: &[Resolution]) -> Result<Graph> {
    let known: HashMap<String, &Warning> = graph
        .warnings
        .iter()
        .map(|warning| (warning_id(warning), warning))
        .collect();
    let mut resolved = HashMap::new();
    for resolution in resolutions {
        let warning = known.get(&resolution.id).ok_or_else(|| {
            invalid_resolution(
                "Each resolution needs a unique known warning ID and valid current evidence.",
            )
        })?;
        if resolved.contains_key(&resolution.id) {
            return Err(invalid_resolution(
                "Each resolution needs a unique known warning ID and valid current evidence.",
            ));
        }
        let current = resolution.evidence.iter().all(|citation| {
            let source = project
                .documents
                .iter()
                .find(|document| document.id == citation.document);
            source.is_some_and(|source| {
                citation.version.as_deref() == Some(source.hash.as_str())
                    && valid_citation(citation, &project.documents)
            })
        });
        if !current {
            return Err(invalid_resolution(
                "Each resolution needs a unique known warning ID and valid current evidence.",
            ));
        }
        if is_warning_resolved(warning, &project.documents) {
            return Err(HivexError::new(
                "WARNING_ALREADY_RESOLVED",
                "The warning already has a current resolution; its history is preserved.",
            ));
        }
        resolved.insert(resolution.id.clone(), resolution);
    }
    let warnings = graph
        .warnings
        .iter()
        .map(|warning| {
            resolved.get(&warning_id(warning)).map_or_else(
                || Ok(warning.clone()),
                |resolution| {
                    Ok(with_warning_resolution(
                        warning,
                        WarningResolution {
                            evidence: resolution
                                .evidence
                                .iter()
                                .map(|citation| crate::knowledge_model::WarningScope {
                                    document: citation.document.clone(),
                                    line_end: citation.line_end,
                                    line_start: citation.line_start,
                                    version: citation.version.clone().unwrap_or_default(),
                                })
                                .collect(),
                            reason: resolution.reason.clone(),
                        },
                    ))
                },
            )
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Graph {
        warnings,
        ..graph.clone()
    })
}

fn warning_output(warning: &Warning, project: &Project) -> Option<Value> {
    let resolved = is_warning_resolved(warning, &project.documents);
    let mut value = match warning {
        Warning::Legacy(message) => {
            let mut value = Map::new();
            value.insert("message".to_owned(), Value::String(message.clone()));
            value.insert("scope".to_owned(), Value::Array(Vec::new()));
            Value::Object(value)
        }
        Warning::Structured(_) => warning_value(warning),
    };
    let object = value.as_object_mut()?;
    object.insert("id".to_owned(), Value::String(warning_id(warning)));
    object.insert(
        "state".to_owned(),
        Value::String(if resolved { "resolved" } else { "active" }.to_owned()),
    );
    Some(value)
}

pub fn command(args: &[String]) -> Result<Value> {
    let parsed = arguments::parse(args, &["resolve", "root"], &["all"]).map_err(|mut error| {
        error.code = "INVALID_ARGUMENT".to_owned();
        error
    })?;
    if parsed.positionals.len() != 1 || parsed.positionals[0] != "warnings" {
        return Err(HivexError::new(
            "INVALID_ARGUMENT",
            "Use warnings [--all] [--resolve <resolutions.json>] [--root <project>].",
        ));
    }
    let root = parsed.values.get("root").cloned().unwrap_or_else(|| {
        std::env::current_dir().map_or_else(
            |_| ".".to_owned(),
            |path| path.to_string_lossy().into_owned(),
        )
    });
    let project = load_project(&root)?;
    let resolutions = parsed
        .values
        .get("resolve")
        .map(|path| read_resolutions(Path::new(path)))
        .transpose()?;
    let (graph, original) = if let Some(resolutions) = resolutions {
        let store = Store::open(
            &project.root,
            StoreOptions {
                readonly: false,
                update: true,
            },
        )?;
        let original_value = store_graph(&store, &project.root)?;
        let original = parse_graph(&original_value, true).ok_or_else(invalid_snapshot)?;
        let graph = resolve_warnings(&original, &project, &resolutions)?;
        store.save_graph(&graph_value(&graph, false))?;
        (graph, original)
    } else {
        let original_value = stored_graph(&project.root)?;
        let original = parse_graph(&original_value, true).ok_or_else(invalid_snapshot)?;
        (original.clone(), original)
    };
    let summary = warning_summary(&graph.warnings, &project.documents);
    let previous = warning_summary(&original.warnings, &project.documents);
    let show_all = parsed.flags.contains("all");
    let warnings = graph
        .warnings
        .iter()
        .filter_map(|warning| {
            let resolved = is_warning_resolved(warning, &project.documents);
            if resolved && !show_all {
                return None;
            }
            warning_output(warning, &project)
        })
        .collect::<Vec<_>>();
    let mut response = Map::new();
    response.insert("command".to_owned(), Value::String("warnings".to_owned()));
    response.insert("modelCalls".to_owned(), Value::from(0));
    response.insert(
        "resolved".to_owned(),
        Value::from(summary.resolved.saturating_sub(previous.resolved)),
    );
    response.insert(
        "warningSummary".to_owned(),
        serde_json::to_value(summary)
            .map_err(|error| HivexError::new("READ_FAILED", error.to_string()))?,
    );
    response.insert("warnings".to_owned(), Value::Array(warnings));
    Ok(Value::Object(response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge_model::{WarningRecord, WarningScope, empty_graph, warning_id};
    use crate::knowledge_snapshot::write_knowledge_snapshot;
    use serde_json::json;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("hivex-warnings-{}", Uuid::new_v4()));
        fs::create_dir(&root).expect("temporary root");
        fs::write(root.join("notes.md"), "# Notes\n\nNeed context.\n").expect("source");
        root
    }

    #[test]
    fn lists_and_resolves_current_warning_evidence_without_models() {
        let root = root();
        let hash = crate::markdown::hash("# Notes\n\nNeed context.\n");
        let warning = Warning::Structured(WarningRecord {
            kind: Some("limitation".to_owned()),
            message: "Need context.".to_owned(),
            scope: vec![WarningScope {
                document: "notes.md".to_owned(),
                line_end: 3,
                line_start: 3,
                version: hash.clone(),
            }],
            ..WarningRecord::default()
        });
        let graph = Graph {
            warnings: vec![warning.clone()],
            ..empty_graph()
        };
        write_knowledge_snapshot(&root, &graph_value(&graph, false)).expect("snapshot");
        let listed = command(&[
            "warnings".to_owned(),
            "--root".to_owned(),
            root.to_string_lossy().into_owned(),
        ])
        .expect("list warnings");
        assert_eq!(listed["warningSummary"]["limitations"], 1);
        assert_eq!(listed["warnings"][0]["id"], warning_id(&warning));

        let resolutions = root.join("resolutions.json");
        fs::write(
            &resolutions,
            serde_json::to_vec(&json!([{
                "evidence": [{
                    "document": "notes.md",
                    "lineEnd": 3,
                    "lineStart": 3,
                    "version": hash
                }],
                "id": warning_id(&warning),
                "reason": "The current source is sufficient."
            }]))
            .expect("resolution JSON"),
        )
        .expect("resolution file");
        let resolved = command(&[
            "warnings".to_owned(),
            "--resolve".to_owned(),
            resolutions.to_string_lossy().into_owned(),
            "--root".to_owned(),
            root.to_string_lossy().into_owned(),
            "--all".to_owned(),
        ])
        .expect("resolve warning");
        assert_eq!(resolved["resolved"], 1);
        assert_eq!(resolved["warningSummary"]["resolved"], 1);
        assert_eq!(resolved["warnings"][0]["state"], "resolved");
        let _ = fs::remove_dir_all(root);
    }
}
