use super::{
  BatchEvidence, BatchExecution, Citation, Graph, HivexError, Result, Value, check_request,
  current_retained_citation, hash, is_current_source, json, materialize, model, model_runtime,
  pending_current, retained_reassessment, supplied_documents,
};
use serde::Deserialize;
use std::collections::HashSet;

mod retained;

pub(super) fn preserve_relationships(
  graph: &Graph,
  candidate: &mut Graph,
  pending: &Value,
) -> Result<()> {
  retained::preserve(graph, candidate, pending)
}

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
  #[serde(default)]
  retained_relationships: Vec<retained::Replacement>,
  #[serde(default)]
  retained_decisions: Vec<retained::DecisionReplacement>,
}

fn invalid() -> HivexError {
  HivexError::new(
    "INVALID_CORRECTION",
    "Use current supplied evidence to correct decisions or relationships in the exact retained failed candidate. No model call was made.",
  )
}

pub(super) fn restored_ranges(pending: &Value) -> Vec<Citation> {
  serde_json::from_value(pending["retainedDecisionRanges"].clone()).unwrap_or_default()
}

fn decision_in_scope(
  original: &model::ExtractionDecision,
  citation: &Citation,
  pending: &Value,
) -> bool {
  let restored = restored_ranges(pending);
  if let Some(range) = restored.iter().find(|range| {
    range.document == original.document
      && range.line_start <= original.line_start
      && range.line_end >= original.line_end
  }) {
    return model::in_ranges(citation, Some(std::slice::from_ref(range)));
  }
  let ranges =
    serde_json::from_value::<Vec<Citation>>(pending["packet"]["units"].clone()).unwrap_or_default();
  model::in_ranges(citation, Some(&ranges))
}

fn parse_correction(value: &Value) -> Result<Correction> {
  let correction: Correction = serde_json::from_value(value.clone())?;
  if value["decisions"] != json!(correction.decisions)
    || value
      .get("relationships")
      .is_some_and(|value| *value != json!(correction.relationships))
    || value
      .get("retainedRelationships")
      .is_some_and(|value| *value != json!(correction.retained_relationships))
    || value
      .get("retainedDecisions")
      .is_some_and(|value| *value != json!(correction.retained_decisions))
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
      && correction.retained_relationships.is_empty()
      && correction.retained_decisions.is_empty()
      && correction.remove_decisions.is_empty()
      && correction.remove_relationships.is_empty())
    || correction.decisions.len() > 64
    || correction.relationships.len() > 128
    || correction.remove_decisions.len() > 64
    || correction.remove_relationships.len() > 128
    || correction.retained_relationships.len() > 128
    || correction.retained_decisions.len() > 64
  {
    return Err(invalid());
  }
  Ok(())
}

fn replacement(
  evidence: BatchEvidence<'_>,
  pending: &Value,
  correction: &Correction,
  graphs: (&Graph, &Graph),
) -> Result<(Value, Vec<String>)> {
  let (candidate, accepted) = graphs;
  let supplied = supplied_documents(pending)?;
  validate_request(correction)?;
  validate_evidence(evidence, pending, &correction.evidence)?;
  let mut extraction = model::parse_extraction(&pending["extraction"]).ok_or_else(invalid)?;
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
      || !model::valid_citation(&citation, &evidence.project.documents)
      || !decision_in_scope(original, &citation, pending)
      || !model::supplied_citation(&citation, &supplied)
    {
      return Err(invalid());
    }
    *original = decision.clone();
  }
  for edge in &correction.relationships {
    validate_evidence(evidence, pending, &edge.evidence)?;
  }
  extraction.decisions.extend(
    correction
      .retained_decisions
      .iter()
      .map(|record| record.replacement.clone()),
  );
  replace_relationships(pending, correction, candidate, &mut extraction)?;
  remove_duplicates(&mut extraction, correction)?;
  let preserved = retained::append(
    (
      super::super::warnings::CandidateContext {
        graph: candidate,
        project: evidence.project,
        packet: &pending["packet"],
      },
      accepted,
    ),
    pending,
    correction,
    &mut extraction,
  )?;
  if !model::validate_extraction(&extraction) {
    return Err(invalid());
  }
  Ok((json!(extraction), preserved))
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
  let ranges: Vec<Citation> = serde_json::from_value(pending["packet"]["units"].clone())?;
  let present: std::collections::HashMap<_, _> = after
    .graph
    .decisions
    .iter()
    .map(|node| (&node.id, node))
    .collect();
  if !correction.retained_decisions.is_empty()
    && before.decisions.iter().any(|node| {
      node.batch != batch
        && is_current_source(after.project, &node.document, Some(&node.version))
        && !ranges
          .iter()
          .any(|range| super::overlaps(range, &super::decision_range(node)))
        && present
          .get(&node.id)
          .is_none_or(|current| **current != *node)
    })
  {
    return Err(invalid());
  }
  let valid_edge = |edge: &model::Relationship| {
    let endpoints: Vec<_> = after
      .graph
      .decisions
      .iter()
      .filter(|node| node.id == edge.from || node.id == edge.to)
      .collect();
    (endpoints.iter().any(|node| node.batch == batch)
      || pending["preservedRelationshipIds"]
        .as_array()
        .is_some_and(|ids| ids.contains(&json!(edge.id)))
      || correction
        .retained_relationships
        .iter()
        .any(|record| record.replacement.id == edge.local_id))
      && endpoints.iter().all(|node| {
        let citation = super::decision_range(node);
        model::valid_citation(&citation, &after.project.documents)
          && current_retained_citation(&citation, pending)
      })
  };
  if (before.decisions == after.graph.decisions
    && before.relationships == after.graph.relationships)
    || correction
      .relationships
      .iter()
      .chain(
        correction
          .retained_relationships
          .iter()
          .map(|record| &record.replacement),
      )
      .any(|edge| {
        !after.graph.relationships.iter().any(|entry| {
          entry.local_id == edge.id
            && (entry.batch == batch
              || pending["preservedRelationshipIds"]
                .as_array()
                .is_some_and(|ids| ids.contains(&json!(entry.id))))
            && valid_edge(entry)
        })
      })
  {
    return Err(invalid());
  }
  Ok(())
}

fn verified_candidate(
  evidence: BatchEvidence<'_>,
  graph: &Graph,
  execution: &BatchExecution<'_>,
  correction: &Correction,
) -> Result<(Value, Graph)> {
  let work = &execution.work;
  let prepared = work.value()["corrections"]
    .as_array()
    .and_then(|records| records.last())
    .is_some_and(|record| {
      record["hash"] == work.value()["pending"]["packet"]["candidateCorrection"]
        && record["correction"]["checkInputHash"] == correction.check_input_hash
    });
  if !(retained_reassessment(execution.runtime, work)? || prepared)
    || work.value()["pending"]["staged"] != true
    || !pending_current(evidence.project, &work.value()["pending"])
    || work
      .attempts()
      .and_then(|attempts| attempts.last())
      .is_none_or(|attempt| attempt["inputHash"] != correction.check_input_hash)
  {
    return Err(invalid());
  }
  let pending = work.value()["pending"].clone();
  let candidate = materialize(evidence, graph, &pending, true)?;
  verify_original_check(evidence, graph, execution, &pending)?;
  Ok((pending, candidate))
}

fn verify_original_check(
  evidence: BatchEvidence<'_>,
  graph: &Graph,
  execution: &BatchExecution<'_>,
  pending: &Value,
) -> Result<()> {
  let verify = |pending: &Value| {
    let candidate = materialize(evidence, graph, pending, true)?;
    model_runtime::verified_retained_check(
      execution.work,
      &check_request(graph, &candidate, pending),
      &execution.runtime.execution,
    )
    .map(|_| ())
  };
  if verify(pending).is_ok() {
    return Ok(());
  }
  let mut previous = pending;
  for record in execution.work.value()["corrections"]
    .as_array()
    .into_iter()
    .flatten()
    .rev()
  {
    if previous["packet"]["candidateCorrection"] != record["hash"] {
      continue;
    }
    previous = &record["previousPending"];
    if !pending_current(evidence.project, previous) || previous["units"] != pending["units"] {
      return Err(invalid());
    }
    if verify(previous).is_ok() {
      return Ok(());
    }
  }
  Err(invalid())
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
  let (mut pending, candidate) = verified_candidate(
    evidence,
    graph,
    &BatchExecution {
      runtime,
      store,
      work,
    },
    &correction,
  )?;
  let record = json!({"hash":fingerprint,"correction":value,"previousPending":pending});
  let retained_ranges = retained::decision_ranges(evidence, graph, work, &correction)?;
  retained::supply_ranges(evidence.project, &mut pending, &retained_ranges)?;
  let comparison_ranges = retained::comparison_ranges(
    &pending,
    graph,
    &correction.retained_relationships,
    evidence.project,
  )?;
  retained::supply_ranges(evidence.project, &mut pending, &comparison_ranges)?;
  retained::check_accepted(&correction.retained_relationships, graph)?;
  let (extraction, preserved) = replacement(evidence, &pending, &correction, (&candidate, graph))?;
  pending["extraction"] = extraction;
  let mut ids = super::strings(&pending["preservedRelationshipIds"]);
  ids.extend(preserved);
  pending["preservedRelationshipIds"] = json!(ids);
  if !retained_ranges.is_empty() {
    let mut ranges = restored_ranges(&pending);
    ranges.extend(retained_ranges);
    pending["retainedDecisionRanges"] = json!(ranges);
  }
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
  retained::validate(
    super::super::warnings::CandidateContext {
      graph: &corrected,
      project: evidence.project,
      packet: &check_request(graph, &corrected, &pending).packet,
    },
    &pending,
    &correction.retained_relationships,
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
  pending["boundCheckTargets"] = json!(true);
  pending["restoredDecisionContext"] = json!(true);
  work.correct_pending(pending, record);
  store.save(work)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn later_corrections_keep_restored_ranges_separate_from_ordinary_pending_targets() {
    let pending = json!({"packet":{"units":[
      {"document":"guide.md","lineStart":200,"lineEnd":215},
      {"document":"guide.md","lineStart":216,"lineEnd":240}
    ]},"retainedDecisionRanges":[{"document":"guide.md","lineStart":118,"lineEnd":118,"version":"current"}]});
    let original = |line| model::ExtractionDecision {
      document: "guide.md".into(),
      line_start: line,
      line_end: line,
      ..Default::default()
    };
    let cite = |start, end| Citation {
      document: "guide.md".into(),
      line_start: start,
      line_end: end,
      version: None,
    };
    assert!(decision_in_scope(&original(118), &cite(118, 118), &pending));
    assert!(!decision_in_scope(
      &original(118),
      &cite(240, 240),
      &pending
    ));
    assert!(decision_in_scope(&original(205), &cite(240, 240), &pending));
    assert!(decision_in_scope(&original(205), &cite(210, 220), &pending));
    assert!(!decision_in_scope(
      &original(205),
      &cite(118, 118),
      &pending
    ));
  }
}
