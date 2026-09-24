use super::{
  BatchEvidence, BatchExecution, Citation, Graph, HivexError, Result, Value, check_request, hash,
  is_current_source, json, materialize, model, model_runtime, pending_current,
  retained_reassessment, supplied_documents,
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
  remove_decisions: Vec<String>,
  #[serde(default)]
  remove_relationships: Vec<String>,
}

fn invalid() -> HivexError {
  HivexError::new(
    "INVALID_CORRECTION",
    "Use current supplied evidence to replace existing decisions in the exact retained failed candidate. No model call was made.",
  )
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

fn replacement(
  evidence: BatchEvidence<'_>,
  pending: &Value,
  correction: &Correction,
) -> Result<Value> {
  let supplied = supplied_documents(pending)?;
  if correction.reason.trim().is_empty()
    || correction.reason.encode_utf16().count() > 2048
    || !(1..=32).contains(&correction.evidence.len())
    || (correction.decisions.is_empty()
      && correction.remove_decisions.is_empty()
      && correction.remove_relationships.is_empty())
    || correction.decisions.len() > 64
    || correction.remove_decisions.len() > 64
    || correction.remove_relationships.len() > 128
  {
    return Err(invalid());
  }
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
  remove_duplicates(&mut extraction, correction)?;
  if !model::validate_extraction(&extraction) {
    return Err(invalid());
  }
  Ok(json!(extraction))
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
  let correction: Correction = serde_json::from_value(value.clone())?;
  if value["decisions"] != json!(correction.decisions) {
    return Err(invalid());
  }
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
  let extraction = replacement(evidence, &pending, &correction)?;
  let record = json!({"hash":fingerprint,"correction":value,"previousPending":pending});
  pending["extraction"] = extraction;
  pending["retainedRelationshipContext"] = json!(2);
  pending["warnings"] = json!([]);
  // A new input identity also prevents reuse of any older native check.
  pending["packet"]["candidateCorrection"] = json!(fingerprint);
  work.correct_pending(pending, record);
  store.save(work)
}
