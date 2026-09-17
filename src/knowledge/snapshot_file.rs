use crate::documents::compare_serialized_strings;
use crate::documents::markdown;
use crate::error::{HivexError, Result};
use crate::knowledge::model::{Graph, Warning, empty_graph, graph_value, parse_graph};

use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const MAX_SNAPSHOT_BYTES: usize = 64 * 1024 * 1024;
const PROTECTED_PARTS: [&str; 5] = ["", ".", "..", ".git", ".hivex"];

pub(super) fn invalid_snapshot() -> HivexError {
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

pub(super) fn parse_snapshot_value(value: Value) -> Result<Value> {
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
