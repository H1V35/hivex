use crate::compatibility::trim_js_whitespace;
use crate::documents::{Project, load_project};
use crate::error::{HivexError, Result};
use crate::knowledge::model::{
  Citation, Graph, KnowledgeCheck, Warning, WarningResolution, empty_graph, graph_value,
  is_warning_resolved, parse_graph, valid_citation, warning_id, warning_summary, warning_value,
  with_warning_resolution,
};
use crate::knowledge::snapshot::{shared_knowledge, stored_graph};
use crate::work::{Store, StoreOptions, Work};
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
      let value = crate::knowledge::model::normalize_integral_numbers(value.clone());
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

fn read_resolution_file(path: &Path) -> Result<Value> {
  let metadata = fs::metadata(path)
    .map_err(|error| invalid_resolution(format!("Unable to read resolutions: {error}")))?;
  if metadata.len() > MAX_RESOLUTION_FILE_BYTES {
    return Err(invalid_resolution("Resolution file exceeds 2 MiB."));
  }
  let bytes = fs::read(path)
    .map_err(|error| invalid_resolution(format!("Unable to read resolutions: {error}")))?;
  serde_json::from_slice(&bytes)
    .map_err(|_| invalid_resolution("Resolution file must contain valid JSON."))
}

fn parse_resolutions(value: &Value) -> Result<Vec<Resolution>> {
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
                .map(|citation| crate::knowledge::model::WarningScope {
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

fn candidate_findings(graph: &Graph, check: &KnowledgeCheck) -> Vec<Warning> {
  graph
    .warnings
    .iter()
    .filter(|warning| match warning {
      Warning::Structured(warning) => {
        warning.kind.as_deref() == Some("finding")
          && check.findings.iter().any(|finding| {
            warning.target.as_deref() == Some(&finding.target) && warning.message == finding.reason
          })
      }
      Warning::Legacy(_) => false,
    })
    .cloned()
    .collect()
}

pub(super) fn candidate_report(
  graph: &Graph,
  project: &Project,
  check: &KnowledgeCheck,
) -> Vec<Value> {
  candidate_findings(graph, check)
    .iter()
    .filter_map(|warning| warning_output(warning, project))
    .collect()
}

pub(super) fn resolve_candidate(
  graph: &Graph,
  project: &Project,
  work: &Work,
  path: &str,
) -> Result<(Graph, Value)> {
  let file = read_resolution_file(Path::new(path))?;
  let attempt = work
    .attempts()
    .and_then(|attempts| attempts.last())
    .ok_or_else(|| invalid_resolution("The candidate needs a retained check."))?;
  if file["workId"] != work.id()
    || file["checkInputHash"] != attempt["inputHash"]
    || file.as_object().is_none_or(|object| object.len() != 3)
  {
    return Err(invalid_resolution(
      "Candidate resolutions must identify this work and exact check input hash.",
    ));
  }
  let value = &attempt["result"];
  let check = crate::knowledge::model::parse_check(value)
    .ok_or_else(|| invalid_resolution("The candidate needs a retained check."))?;
  let findings = crate::knowledge::model::check_warnings(graph, &check, "", &[]);
  let eligible: Vec<_> = findings
    .iter()
    .filter(|warning| match warning {
      Warning::Structured(warning) => graph.decisions.iter().any(|node| {
        warning.target.as_deref() == Some(&node.id)
          && project
            .documents
            .iter()
            .any(|source| source.id == node.document && source.hash == node.version)
      }),
      Warning::Legacy(_) => false,
    })
    .map(warning_id)
    .collect();
  let resolutions = parse_resolutions(&file["resolutions"])?;
  if resolutions
    .iter()
    .any(|resolution| !eligible.contains(&resolution.id))
  {
    return Err(invalid_resolution(
      "Resolve only retained-check findings on current canonical candidate decisions; structural and unknown findings remain blocking.",
    ));
  }
  let resolved = resolve_warnings(graph, project, &resolutions)?;
  let closed = candidate_findings(&resolved, &check);
  let mut reviewed = value.clone();
  reviewed["findings"] = serde_json::to_value(
    check
      .findings
      .iter()
      .filter(|finding| {
        !closed.iter().any(|warning| match warning {
          Warning::Structured(record) => {
            record.target.as_deref() == Some(&finding.target)
              && record.message == finding.reason
              && resolutions
                .iter()
                .any(|resolution| resolution.id == warning_id(warning))
              && is_warning_resolved(warning, &project.documents)
          }
          Warning::Legacy(_) => false,
        })
      })
      .collect::<Vec<_>>(),
  )?;
  Ok((resolved, reviewed))
}

pub fn warning_report(root: &str, resolve: Option<&str>, show_all: bool) -> Result<Value> {
  let project = load_project(root)?;
  let resolutions = resolve
    .map(|path| parse_resolutions(&read_resolution_file(Path::new(path))?))
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
