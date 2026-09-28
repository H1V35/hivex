use super::{
  BatchEvidence, BatchExecution, Citation, Graph, HivexError, Result, Value, check_request,
  current_retained_citation, hash, is_current_source, json, materialize, model, model_runtime,
  pending_current, retained_reassessment, supplied_documents,
};
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Correction {
  work_id: String,
  check_input_hash: String,
  reason: String,
  evidence: Vec<Citation>,
  decisions: Vec<model::ExtractionDecision>,
  #[serde(default)]
  relationships: Vec<model::ExtractionRelationship>,
  #[serde(default)]
  remove_decisions: Vec<String>,
  #[serde(default)]
  remove_relationships: Vec<String>,
}

fn invalid() -> HivexError {
  HivexError::new(
    "INVALID_CORRECTION",
    "Use current supplied evidence to correct decisions or relationships in the exact retained failed candidate. No model call was made.",
  )
}

fn parse_correction(value: &Value) -> Result<Correction> {
  let correction: Correction = serde_json::from_value(value.clone())?;
  if value["decisions"] != json!(correction.decisions)
    || value
      .get("relationships")
      .is_some_and(|value| *value != json!(correction.relationships))
  {
    return Err(invalid());
  }
  Ok(correction)
}

fn validate_evidence(
  evidence: BatchEvidence<'_>,
  pending: &Value,
  citations: &[Citation],
) -> Result<()> {
  let supplied = supplied_documents(pending)?;
  for citation in citations {
    if citation.version.is_none()
      || !is_current_source(
        evidence.project,
        &citation.document,
        citation.version.as_deref(),
      )
      || !model::valid_citation(citation, &evidence.project.documents)
      || !model::supplied_citation(citation, &supplied)
    {
      return Err(invalid());
    }
  }
  Ok(())
}

fn validate_request(correction: &Correction) -> Result<()> {
  if correction.reason.trim().is_empty()
    || correction.reason.encode_utf16().count() > 2048
    || !(1..=32).contains(&correction.evidence.len())
    || (correction.decisions.is_empty()
      && correction.relationships.is_empty()
      && correction.remove_decisions.is_empty()
      && correction.remove_relationships.is_empty())
    || correction.decisions.len() > 64
    || correction.relationships.len() > 128
    || correction.remove_decisions.len() > 64
    || correction.remove_relationships.len() > 128
  {
    return Err(invalid());
  }
  Ok(())
}

fn replacement(
  evidence: BatchEvidence<'_>,
  pending: &Value,
  correction: &Correction,
  candidate: &Graph,
) -> Result<Value> {
  let supplied = supplied_documents(pending)?;
  validate_request(correction)?;
  validate_evidence(evidence, pending, &correction.evidence)?;
  let mut extraction = model::parse_extraction(&pending["extraction"]).ok_or_else(invalid)?;
  let ranges: Vec<Citation> = serde_json::from_value(pending["packet"]["units"].clone())?;
  let mut seen = HashSet::new();
  for decision in &correction.decisions {
    let original = extraction
      .decisions
      .iter_mut()
      .find(|entry| entry.id == decision.id && entry.document == decision.document)
      .ok_or_else(invalid)?;
    let citation = Citation {
      document: decision.document.clone(),
      line_start: decision.line_start,
      line_end: decision.line_end,
      version: None,
    };
    if !seen.insert(&decision.id)
      || !ranges.iter().any(|range| {
        range.document == citation.document
          && range.line_start <= citation.line_start
          && range.line_end >= citation.line_end
      })
      || !model::valid_citation(&citation, &evidence.project.documents)
      || !model::supplied_citation(&citation, &supplied)
    {
      return Err(invalid());
    }
    *original = decision.clone();
  }
  for edge in &correction.relationships {
    validate_evidence(evidence, pending, &edge.evidence)?;
  }
  replace_relationships(pending, correction, candidate, &mut extraction)?;
  remove_duplicates(&mut extraction, correction)?;
  if !model::validate_extraction(&extraction) {
    return Err(invalid());
  }
  Ok(json!(extraction))
}

fn replace_relationships(
  pending: &Value,
  correction: &Correction,
  candidate: &Graph,
  extraction: &mut model::Extraction,
) -> Result<()> {
  let local: HashSet<_> = extraction
    .decisions
    .iter()
    .map(|node| node.id.as_str())
    .collect();
  let supplied: HashSet<_> = pending["existing"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(Value::as_str)
    .chain(
      pending["packet"]["replacingDecisions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|node| node["id"].as_str()),
    )
    .collect();
  let endpoint = |id: &str| {
    local.contains(id)
      || (supplied.contains(id)
        && candidate.decisions.iter().any(|node| {
          node.id == id && current_retained_citation(&super::decision_range(node), pending)
        }))
  };
  let mut seen = HashSet::new();
  for edge in &correction.relationships {
    if !seen.insert(&edge.id)
      || correction.remove_relationships.contains(&edge.id)
      || correction.remove_decisions.contains(&edge.from)
      || correction.remove_decisions.contains(&edge.to)
      || !endpoint(&edge.from)
      || !endpoint(&edge.to)
      || !(local.contains(edge.from.as_str()) || local.contains(edge.to.as_str()))
    {
      return Err(invalid());
    }
    if let Some(original) = extraction
      .relationships
      .iter_mut()
      .find(|entry| entry.id == edge.id)
    {
      *original = edge.clone();
    } else {
      extraction.relationships.push(edge.clone());
    }
  }
  Ok(())
}

fn remove_duplicates(extraction: &mut model::Extraction, correction: &Correction) -> Result<()> {
  let decisions: HashSet<_> = correction.remove_decisions.iter().collect();
  let relationships: HashSet<_> = correction.remove_relationships.iter().collect();
  if decisions.len() != correction.remove_decisions.len()
    || relationships.len() != correction.remove_relationships.len()
    || decisions
      .iter()
      .any(|id| !extraction.decisions.iter().any(|node| &node.id == *id))
    || relationships
      .iter()
      .any(|id| !extraction.relationships.iter().any(|edge| &edge.id == *id))
    || correction
      .decisions
      .iter()
      .any(|node| decisions.contains(&node.id))
  {
    return Err(invalid());
  }
  extraction
    .relationships
    .retain(|edge| !relationships.contains(&edge.id));
  if extraction
    .relationships
    .iter()
    .any(|edge| decisions.contains(&edge.from) || decisions.contains(&edge.to))
  {
    return Err(invalid());
  }
  extraction
    .decisions
    .retain(|node| !decisions.contains(&node.id));
  Ok(())
}

fn validate_changed_candidate(
  correction: &Correction,
  before: &Graph,
  after: super::SourceGraph<'_>,
  pending: &Value,
) -> Result<()> {
  let batch = pending["batch"].as_str().unwrap_or_default();
  let valid_edge = |edge: &model::Relationship| {
    let endpoints: Vec<_> = after
      .graph
      .decisions
      .iter()
      .filter(|node| node.id == edge.from || node.id == edge.to)
      .collect();
    endpoints.iter().any(|node| node.batch == batch)
      && endpoints.iter().all(|node| {
        let citation = super::decision_range(node);
        model::valid_citation(&citation, &after.project.documents)
          && current_retained_citation(&citation, pending)
      })
  };
  if (before.decisions == after.graph.decisions
    && before.relationships == after.graph.relationships)
    || correction.relationships.iter().any(|edge| {
      !after
        .graph
        .relationships
        .iter()
        .any(|entry| entry.local_id == edge.id && entry.batch == batch && valid_edge(entry))
    })
  {
    return Err(invalid());
  }
  Ok(())
}

pub(super) fn apply(
  evidence: BatchEvidence<'_>,
  graph: &Graph,
  execution: BatchExecution<'_>,
) -> Result<()> {
  let BatchExecution {
    runtime,
    store,
    work,
  } = execution;
  let Some(path) = &runtime.correct else {
    return Ok(());
  };
  let value: Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
  let correction = parse_correction(&value)?;
  let fingerprint = hash(&value.to_string());
  if correction.work_id != work.id() {
    return Err(invalid());
  }
  if work.value()["corrections"]
    .as_array()
    .is_some_and(|records| records.iter().any(|record| record["hash"] == fingerprint))
  {
    if work.status() != crate::work::State::Done
      && (work.value()["pending"]["packet"]["candidateCorrection"] != fingerprint
        || !pending_current(evidence.project, &work.value()["pending"]))
    {
      return Err(invalid());
    }
    return Ok(());
  }
  if !retained_reassessment(runtime, work)?
    || work.value()["pending"]["staged"] != true
    || !pending_current(evidence.project, &work.value()["pending"])
    || work
      .attempts()
      .and_then(|attempts| attempts.last())
      .is_none_or(|attempt| attempt["inputHash"] != correction.check_input_hash)
  {
    return Err(invalid());
  }
  let mut pending = work.value()["pending"].clone();
  let candidate = materialize(evidence, graph, &pending, true)?;
  if model_runtime::retained_check_result(
    work,
    &check_request(graph, &candidate, &pending),
    &runtime.execution,
  )?
  .is_none()
  {
    return Err(invalid());
  }
  let extraction = replacement(evidence, &pending, &correction, &candidate)?;
  let record = json!({"hash":fingerprint,"correction":value,"previousPending":pending});
  pending["extraction"] = extraction;
  let corrected = materialize(evidence, graph, &pending, true)?;
  validate_changed_candidate(
    &correction,
    &candidate,
    super::SourceGraph {
      project: evidence.project,
      graph: &corrected,
    },
    &pending,
  )?;
  pending["protectedRelationships"] = json!(super::protected_relationships(
    super::SourceGraph {
      project: evidence.project,
      graph
    },
    &corrected,
    &pending,
  ));
  pending["retainedRelationshipContext"] = json!(2);
  pending["warnings"] = json!([]);
  // A new input identity also prevents reuse of any older native check.
  pending["packet"]["candidateCorrection"] = json!(fingerprint);
  work.correct_pending(pending, record);
  store.save(work)
}
