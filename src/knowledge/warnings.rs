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
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

const MAX_RESOLUTION_FILE_BYTES: u64 = 2 * 1024 * 1024;

mod relationships;

#[derive(Clone, Copy)]
pub(super) struct CandidateContext<'a> {
  pub graph: &'a Graph,
  pub project: &'a Project,
  pub packet: &'a Value,
}

pub(super) fn validate_relationship_changes(
  context: CandidateContext<'_>,
  pending: &Value,
  changes: &Value,
) -> Result<()> {
  relationships::review_changes(
    &json!({"relationshipChanges":changes}),
    &context,
    pending,
    None,
  )?;
  Ok(())
}

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
      let citation: Citation = serde_json::from_value(value.clone())
        .map_err(|_| invalid_resolution("Each resolution citation is invalid."))?;
      if value != json!(citation) {
        return Err(invalid_resolution(
          "Each resolution citation has unsupported fields.",
        ));
      }
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

fn resolvable_target(target: &str, graph: &Graph, project: &Project, pending: &Value) -> bool {
  if graph.decisions.iter().any(|node| {
    node.id == target && super::is_current_source(project, &node.document, Some(&node.version))
  }) {
    return true;
  }
  pending["materializedCheck"] == true
    && (target == "batch"
      || pending["packet"]["documents"]
        .as_array()
        .is_some_and(|documents| {
          documents.iter().any(|document| {
            document["id"] == target
              && super::is_current_source(project, target, document["version"].as_str())
          })
        }))
}

fn resolution_covers_target(resolution: &Resolution, target: &str, pending: &Value) -> bool {
  if target == "batch" {
    return pending["packet"]["units"].as_array().is_some_and(|units| {
      !units.is_empty()
        && units.iter().all(|unit| {
          resolution.evidence.iter().any(|citation| {
            unit["document"] == citation.document
              && unit["lineStart"]
                .as_u64()
                .is_some_and(|start| citation.line_start as u64 <= start)
              && unit["lineEnd"]
                .as_u64()
                .is_some_and(|end| citation.line_end as u64 >= end)
          })
        })
    });
  }
  let document = pending["packet"]["documents"]
    .as_array()
    .is_some_and(|documents| documents.iter().any(|document| document["id"] == target));
  !document
    || resolution
      .evidence
      .iter()
      .any(|citation| citation.document == target)
}

pub(super) fn resolve_candidate(
  context: CandidateContext<'_>,
  work: &Work,
  path: &str,
) -> Result<(Graph, Value, Value)> {
  let CandidateContext { graph, project, .. } = context;
  let file = read_resolution_file(Path::new(path))?;
  let attempt = work
    .attempts()
    .and_then(|attempts| attempts.last())
    .ok_or_else(|| invalid_resolution("The candidate needs a retained check."))?;
  if file["workId"] != work.id()
    || file["checkInputHash"] != attempt["inputHash"]
    || file.as_object().is_none_or(|object| {
      object.keys().any(|key| {
        !matches!(
          key.as_str(),
          "workId" | "checkInputHash" | "resolutions" | "relationshipChanges"
        )
      })
    })
  {
    return Err(invalid_resolution(
      "Candidate resolutions must identify this work and exact check input hash.",
    ));
  }
  let value = &attempt["result"];
  let check = crate::knowledge::model::parse_check(value)
    .ok_or_else(|| invalid_resolution("The candidate needs a retained check."))?;
  let pending = &work.value()["pending"];
  let changes = relationships::review_changes(&file, &context, pending, Some(value))?;
  let ranges: Vec<Citation> = serde_json::from_value(pending["packet"]["units"].clone())
    .map_err(|_| invalid_resolution("The retained check needs its original source ranges."))?;
  let scope = crate::knowledge::model::warning_scope(&project.documents, Some(&ranges));
  let findings = crate::knowledge::model::check_warnings(
    graph,
    &check,
    pending["batch"].as_str().unwrap_or_default(),
    &scope,
  );
  let eligible: HashMap<_, _> = findings
    .iter()
    .filter_map(|warning| match warning {
      Warning::Structured(record) => record
        .target
        .as_deref()
        .filter(|target| {
          resolvable_target(target, graph, project, pending)
            || relationships::resolvable(target, &context, &changes)
        })
        .map(|target| (warning_id(warning), target)),
      Warning::Legacy(_) => None,
    })
    .collect();
  let resolutions = if file["resolutions"] == json!([]) && !changes.is_empty() {
    Vec::new()
  } else {
    parse_resolutions(&file["resolutions"])?
  };
  if resolutions.iter().any(|resolution| {
    eligible.get(&resolution.id).is_none_or(|target| {
      !resolution_covers_target(resolution, target, pending)
        || !relationships::covers_target(resolution, target, &context, &changes)
    })
  }) {
    return Err(invalid_resolution(
      "Resolve only retained semantic findings on current candidate knowledge, supplied documents, the batch or explicitly mapped protected relationships. Cite the target ranges; structural and unknown findings remain blocking.",
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
  relationships::apply_changes(&mut reviewed, &changes);
  let record = json!({
    "checkInputHash":attempt["inputHash"],
    "review":file,
    "previousPending":pending,
    "warnings":candidate_report(&resolved, project, &check)
  });
  Ok((resolved, reviewed, record))
}

pub(super) fn review_candidate(
  context: CandidateContext<'_>,
  work: &Work,
  path: Option<&str>,
  value: &Value,
) -> Result<(Graph, Value, Option<Value>)> {
  let Some(path) = path else {
    return Ok((context.graph.clone(), value.clone(), None));
  };
  let (graph, reviewed, record) = resolve_candidate(context, work, path)?;
  Ok((graph, reviewed, Some(record)))
}

pub(super) fn replay_resolution(
  work: &Work,
  path: Option<&str>,
  project: &Project,
  graph: &Graph,
) -> Result<bool> {
  let Some(path) = path else {
    return Ok(false);
  };
  let record = &work.value()["candidateResolution"];
  if record.is_null() {
    return Ok(false);
  }
  if work
    .attempts()
    .and_then(|attempts| attempts.last())
    .is_some_and(|attempt| attempt["inputHash"] != record["checkInputHash"])
  {
    return Ok(false);
  }
  let file = read_resolution_file(Path::new(path))?;
  if file != record["review"]
    || file["workId"] != work.id()
    || file["checkInputHash"] != record["checkInputHash"]
    || work
      .attempts()
      .and_then(|attempts| attempts.last())
      .is_none_or(|attempt| attempt["inputHash"] != file["checkInputHash"])
  {
    return Err(invalid_resolution(
      "A completed candidate review can only replay its exact disposition.",
    ));
  }
  if !super::update::pending_current(project, &record["previousPending"]) {
    return Err(HivexError::new(
      "STALE_RETAINED_CHECK",
      "The reviewed source versions changed. No model call was made.",
    ));
  }
  if record["admittedGraphHash"] != graph_hash(graph)? {
    return Err(HivexError::new(
      "STALE_RETAINED_CHECK",
      "The admitted graph changed after this review. No model call was made.",
    ));
  }
  Ok(true)
}

pub(super) fn graph_hash(graph: &Graph) -> Result<String> {
  let normalized = parse_graph(&graph_value(graph, false), false).ok_or_else(invalid_snapshot)?;
  Ok(crate::documents::hash(
    &graph_value(&normalized, false).to_string(),
  ))
}

pub(super) fn select_resolution_work(
  path: Option<&str>,
  key: &str,
  store: &Store,
) -> Result<Option<Work>> {
  let Some(path) = path else {
    return Ok(None);
  };
  let works = store.works()?;
  if let Some(work) = works.iter().find(|work| work.key() == key) {
    work.ensure_open()?;
  }
  let file = read_resolution_file(Path::new(path))?;
  let work = works
    .iter()
    .find(|work| file["workId"] == work.id())
    .ok_or_else(|| invalid_resolution("The review must identify an existing work."))?;
  if work.key() != key {
    return Err(HivexError::new(
      "STALE_RETAINED_CHECK",
      "The review belongs to different source versions, update arguments or execution profile. No model call was made.",
    ));
  }
  work.ensure_open()?;
  if work.status() == crate::work::State::Running {
    return Err(HivexError::new(
      "WORK_RUNNING",
      "Inspect the unfinished invocation before reviewing its candidate.",
    ));
  }
  Ok(Some(work.clone()))
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
