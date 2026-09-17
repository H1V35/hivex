use crate::documents::{Document, Project, Warning as DocumentWarning, compare_serialized_strings};
use crate::error::Result;
use crate::knowledge::ingestion::ingestion_units;
use crate::knowledge::model::{
  Graph, Warning, active_warnings, empty_graph, graph_value, parse_graph, warning_summary,
};
use crate::work::{Store, StoreOptions};
use serde_json::{Map, Value, json};
use std::path::Path;

use super::snapshot_file::{invalid_snapshot, parse_snapshot_value};
pub(crate) use super::snapshot_file::{
  read_knowledge_snapshot, shared_knowledge, write_knowledge_snapshot,
};
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
    Ok(value) => parse_snapshot_value(&value),
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
    let source_status =
      if let Some(source) = project.documents.iter().find(|source| source.id == id) {
        match version.as_deref() {
          Some(version) if version == source.hash => 0,
          _ => 1,
        }
      } else {
        2
      };
    let target = match source_status {
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
