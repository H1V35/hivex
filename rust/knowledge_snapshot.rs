use crate::documents::{Document, Project, Warning as DocumentWarning, compare_serialized_strings};
use crate::error::{HivexError, Result};
use crate::ingestion::ingestion_units;
use crate::knowledge_model::{
    Graph, Warning, active_warnings, empty_graph, graph_value, parse_graph, warning_summary,
};
use crate::markdown;
use crate::store::{Store, StoreOptions};
use serde_json::{Map, Value, json};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const MAX_SNAPSHOT_BYTES: usize = 64 * 1024 * 1024;
const PROTECTED_PARTS: [&str; 5] = ["", ".", "..", ".git", ".hivex"];

fn invalid_snapshot() -> HivexError {
    HivexError::new(
        "INVALID_SNAPSHOT",
        "Knowledge snapshot is not a supported graph JSON document.",
    )
}

fn snapshot_paths(root: &Path) -> Result<(PathBuf, PathBuf)> {
    let directory = root.join(".hivex");
    match fs::symlink_metadata(&directory) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(invalid_snapshot());
        }
        Ok(metadata) if !metadata.is_dir() => return Err(invalid_snapshot()),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let file = directory.join("graph.json");
    match fs::symlink_metadata(&file) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(invalid_snapshot()),
        Ok(metadata) if !metadata.is_file() => Err(invalid_snapshot()),
        Ok(_) => Ok((directory, file)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok((directory, file)),
        Err(error) => Err(error.into()),
    }
}

fn is_portable_document(id: &str) -> bool {
    markdown::is_markdown_path(id)
        && !id.starts_with('/')
        && !id.contains('\\')
        && !id.contains('\0')
        && id.split('/').all(|part| !PROTECTED_PARTS.contains(&part))
        && !id.split('/').any(|part| part == "node_modules")
}

fn validate_portable_graph(graph: &Graph) -> Result<()> {
    let mut references = Vec::new();
    references.extend(graph.documents.keys().cloned());
    references.extend(graph.units.values().filter_map(|unit| {
        unit.get("document")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
    }));
    references.extend(
        graph
            .decisions
            .iter()
            .map(|decision| decision.document.clone()),
    );
    references.extend(graph.relationships.iter().flat_map(|relationship| {
        relationship
            .evidence
            .iter()
            .map(|citation| citation.document.clone())
    }));
    references.extend(graph.warnings.iter().flat_map(|warning| {
        match warning {
            Warning::Legacy(_) => Vec::new(),
            Warning::Structured(record) => record
                .scope
                .iter()
                .chain(
                    record
                        .resolution
                        .as_ref()
                        .into_iter()
                        .flat_map(|resolution| resolution.evidence.iter()),
                )
                .map(|scope| scope.document.clone())
                .collect(),
        }
    }));
    if references.iter().all(|id| is_portable_document(id)) {
        Ok(())
    } else {
        Err(invalid_snapshot())
    }
}

fn parse_snapshot_value(value: Value) -> Result<Value> {
    let graph = parse_graph(&value, true).ok_or_else(invalid_snapshot)?;
    validate_portable_graph(&graph)?;
    Ok(graph_value(&graph, false))
}

pub fn read_knowledge_snapshot(root: &Path) -> Result<Option<Value>> {
    let (_, file) = snapshot_paths(root)?;
    let metadata = match fs::symlink_metadata(&file) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if metadata.len() > MAX_SNAPSHOT_BYTES as u64 {
        return Err(HivexError::new(
            "INVALID_SNAPSHOT",
            "Knowledge snapshot exceeds 64 MiB.",
        ));
    }
    let bytes = fs::read(&file).map_err(|_| invalid_snapshot())?;
    let value = serde_json::from_slice::<Value>(&bytes).map_err(|_| invalid_snapshot())?;
    parse_snapshot_value(value).map(Some)
}

fn sort_graph_arrays(value: &mut Value) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    for key in ["decisions", "relationships"] {
        let Some(Value::Array(entries)) = object.get_mut(key) else {
            continue;
        };
        entries.sort_by(|left, right| {
            let left = left.get("id").and_then(Value::as_str).unwrap_or_default();
            let right = right.get("id").and_then(Value::as_str).unwrap_or_default();
            compare_serialized_strings(left, right)
        });
    }
}

fn ordered(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(ordered).collect()),
        Value::Object(value) => {
            let mut entries: Vec<_> = value.into_iter().collect();
            entries.sort_by(|(left, _), (right, _)| compare_serialized_strings(left, right));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, ordered(value)))
                    .collect(),
            )
        }
        value => value,
    }
}

fn write_atomic(file: &Path, bytes: &[u8]) -> Result<()> {
    let directory = file.parent().ok_or_else(invalid_snapshot)?;
    fs::create_dir_all(directory)?;
    match fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(invalid_snapshot());
        }
        Ok(_) => {}
        Err(error) => return Err(error.into()),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(directory)?.permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(directory, permissions)?;
    }
    let temporary = directory.join(format!("graph-{}.tmp", Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut handle = options.open(&temporary)?;
        handle.write_all(bytes)?;
        handle.flush()?;
        fs::rename(&temporary, file)?;
        Ok::<(), std::io::Error>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(Into::into)
}

pub fn write_knowledge_snapshot(root: &Path, value: &Value) -> Result<PathBuf> {
    let (_, file) = snapshot_paths(root)?;
    let graph = parse_snapshot_value(value.clone())?;
    let mut graph = graph;
    sort_graph_arrays(&mut graph);
    let text = format!("{}\n", serde_json::to_string_pretty(&ordered(graph))?);
    if text.len() > MAX_SNAPSHOT_BYTES {
        return Err(HivexError::new(
            "INVALID_SNAPSHOT",
            "Knowledge snapshot exceeds 64 MiB.",
        ));
    }
    write_atomic(&file, text.as_bytes())?;
    Ok(file)
}

pub fn shared_knowledge(root: &Path) -> Result<Value> {
    read_knowledge_snapshot(root)
        .map(|snapshot| snapshot.unwrap_or_else(|| graph_value(&empty_graph(), false)))
}

pub fn stored_graph(root: &Path) -> Result<Value> {
    let database = root.join(".hivex/knowledge.sqlite");
    if !database.exists() {
        return shared_knowledge(root);
    }
    let store = Store::open(
        root,
        StoreOptions {
            readonly: true,
            update: false,
        },
    )?;
    match store.graph() {
        Ok(value) => parse_snapshot_value(value),
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

pub fn snapshot_sources(
    project: &Project,
    graph: &Value,
) -> Result<(Vec<String>, Vec<DocumentWarning>)> {
    let parsed = parse_graph(graph, true).ok_or_else(invalid_snapshot)?;
    let plan = ingestion_units(&project.current_documents);
    let (units, warnings) = (plan.units, plan.warnings);
    let pending = units
        .into_iter()
        .map(|unit| unit.id)
        .filter(|id| {
            let Some(unit) = parsed.units.get(id) else {
                return true;
            };
            let Some(document) = unit.get("document").and_then(Value::as_str) else {
                return true;
            };
            let Some(version) = unit.get("version").and_then(Value::as_str) else {
                return true;
            };
            project
                .documents
                .iter()
                .find(|source| source.id == document)
                .is_none_or(|source| source.hash != version)
        })
        .collect();
    Ok((pending, warnings))
}

pub fn source_versions(project: &Project, graph: &Graph) -> Value {
    let mut references: Vec<(String, Option<String>)> = graph
        .documents
        .iter()
        .map(|(document, version)| (document.clone(), version.as_str().map(ToOwned::to_owned)))
        .collect();
    references.extend(graph.units.values().filter_map(|unit| {
        Some((
            unit.get("document")?.as_str()?.to_owned(),
            Some(unit.get("version")?.as_str()?.to_owned()),
        ))
    }));
    references.extend(
        graph
            .decisions
            .iter()
            .map(|entry| (entry.document.clone(), Some(entry.version.clone()))),
    );
    references.extend(graph.relationships.iter().flat_map(|relationship| {
        relationship
            .evidence
            .iter()
            .map(|citation| (citation.document.clone(), citation.version.clone()))
    }));
    references.extend(graph.warnings.iter().flat_map(|warning| {
        match warning {
            Warning::Legacy(_) => Vec::new(),
            Warning::Structured(record) => record
                .scope
                .iter()
                .chain(
                    record
                        .resolution
                        .as_ref()
                        .into_iter()
                        .flat_map(|resolution| resolution.evidence.iter()),
                )
                .map(|scope| (scope.document.clone(), Some(scope.version.clone())))
                .collect(),
        }
    }));
    let mut current = Vec::new();
    let mut stale = Vec::new();
    let mut unavailable = Vec::new();
    for (id, version) in references {
        let state = if let Some(source) = project.documents.iter().find(|source| source.id == id) {
            match version.as_deref() {
                Some(version) if version == source.hash => 0,
                _ => 1,
            }
        } else {
            2
        };
        let target = match state {
            0 => &mut current,
            1 => &mut stale,
            _ => &mut unavailable,
        };
        if !target.contains(&id) {
            target.push(id);
        }
    }
    current.retain(|id| !stale.contains(id));
    current.sort_by(|left, right| compare_serialized_strings(left, right));
    stale.sort_by(|left, right| compare_serialized_strings(left, right));
    unavailable.sort_by(|left, right| compare_serialized_strings(left, right));
    json!({"current": current, "stale": stale, "unavailable": unavailable})
}

pub fn warning_summary_value(warnings: &[Warning], documents: &[Document]) -> Value {
    let summary = warning_summary(warnings, documents);
    json!({
        "findings": summary.findings,
        "limitations": summary.limitations,
        "resolved": summary.resolved,
        "unknown": summary.unknown,
        "validation": summary.validation,
    })
}

pub fn active_warning_values(warnings: &[Warning], documents: &[Document]) -> Vec<Value> {
    let graph = Graph {
        warnings: warnings.to_vec(),
        ..empty_graph()
    };
    let encoded = graph_value(&graph, false);
    let encoded_warnings = encoded
        .get("warnings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    active_warnings(warnings, documents)
        .iter()
        .filter_map(|warning| {
            let index = warnings
                .iter()
                .position(|candidate| std::ptr::eq(candidate, *warning))?;
            encoded_warnings.get(index).cloned()
        })
        .collect()
}

pub fn snapshot_report(project: &Project, graph: &Value, operation: &str) -> Result<Value> {
    let parsed = parse_graph(graph, true).ok_or_else(invalid_snapshot)?;
    let (pending, plan_warnings) = snapshot_sources(project, graph)?;
    let sources = source_versions(project, &parsed);
    let mut warnings = active_warning_values(&parsed.warnings, &project.documents);
    warnings.extend(
        project
            .warnings
            .iter()
            .map(|warning| json!({"message": warning.message, "path": warning.path})),
    );
    warnings.extend(
        plan_warnings
            .iter()
            .map(|warning| json!({"message": warning.message, "path": warning.path})),
    );
    let partial = !pending.is_empty()
        || !warnings.is_empty()
        || sources["stale"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
        || sources["unavailable"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
        || parsed
            .decisions
            .iter()
            .any(|entry| entry.quality != "checked")
        || parsed
            .relationships
            .iter()
            .any(|entry| entry.quality != "checked");
    let mut response = Map::new();
    response.insert("command".to_owned(), Value::String("snapshot".to_owned()));
    response.insert("decisions".to_owned(), Value::from(parsed.decisions.len()));
    response.insert("modelCalls".to_owned(), Value::from(0));
    response.insert("operation".to_owned(), Value::String(operation.to_owned()));
    response.insert(
        "path".to_owned(),
        Value::String(".hivex/graph.json".to_owned()),
    );
    response.insert(
        "pendingUnits".to_owned(),
        Value::Array(pending.into_iter().map(Value::String).collect()),
    );
    response.insert(
        "relationships".to_owned(),
        Value::from(parsed.relationships.len()),
    );
    response.insert("sources".to_owned(), sources);
    response.insert(
        "status".to_owned(),
        Value::String(if partial { "partial" } else { "ready" }.to_owned()),
    );
    response.insert(
        "warningSummary".to_owned(),
        warning_summary_value(&parsed.warnings, &project.documents),
    );
    response.insert("warnings".to_owned(), Value::Array(warnings));
    Ok(Value::Object(response))
}
