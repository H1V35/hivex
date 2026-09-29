use super::{CandidateContext, Resolution, invalid_resolution, parse_resolution};
use crate::error::Result;
use crate::knowledge::model::{Citation, ExtractionRelationship, Relationship, SuppliedDocument};
use crate::knowledge::{is_current_source, supplied_citation, valid_citation};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashSet;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Change {
  previous_id: String,
  replacements: Vec<String>,
  reason: String,
  evidence: Vec<Citation>,
}

fn invalid() -> crate::error::HivexError {
  invalid_resolution(
    "Relationship reviews need unique protected IDs, present replacements, current supplied endpoint/evidence ranges and a semantic reason. Removal is not supported by this review.",
  )
}

fn covers(evidence: &[Citation], range: &Citation) -> bool {
  evidence.iter().any(|citation| {
    citation.document == range.document
      && citation.line_start <= range.line_start
      && citation.line_end >= range.line_end
      && range
        .version
        .as_ref()
        .is_none_or(|version| citation.version.as_ref() == Some(version))
  })
}

fn current_evidence(citations: &[Citation], context: &CandidateContext<'_>) -> bool {
  let Ok(supplied) =
    serde_json::from_value::<Vec<SuppliedDocument>>(context.packet["documents"].clone())
  else {
    return false;
  };
  !citations.is_empty()
    && citations.iter().all(|citation| {
      is_current_source(
        context.project,
        &citation.document,
        citation.version.as_deref(),
      ) && valid_citation(citation, &context.project.documents)
        && supplied_citation(citation, &supplied)
    })
}

fn current_ranges(edge: &Relationship, context: &CandidateContext<'_>) -> Option<Vec<Citation>> {
  let mut ranges = edge.evidence.clone();
  for id in [&edge.from, &edge.to] {
    let node = context.graph.decisions.iter().find(|node| node.id == *id)?;
    ranges.push(Citation {
      document: node.document.clone(),
      line_start: node.line_start,
      line_end: node.line_end,
      version: Some(node.version.clone()),
    });
  }
  current_evidence(&ranges, context).then_some(ranges)
}

fn supplied_edge<'a>(id: &str, context: &'a CandidateContext<'_>) -> Option<&'a Relationship> {
  let present = ["extraction", "retainedRelationships"].iter().any(|field| {
    let value = if *field == "extraction" {
      &context.packet["extraction"]["relationships"]
    } else {
      &context.packet[*field]
    };
    value
      .as_array()
      .is_some_and(|edges| edges.iter().any(|edge| edge["id"] == id))
  });
  present
    .then(|| {
      context
        .graph
        .relationships
        .iter()
        .find(|edge| edge.id == id)
    })
    .flatten()
}

fn previous_ranges(id: &str, packet: &Value) -> Option<Vec<Citation>> {
  let previous = packet["removedRelationships"]
    .as_array()?
    .iter()
    .find(|edge| edge["id"] == id)?;
  let edge: ExtractionRelationship = serde_json::from_value(previous.clone()).ok()?;
  let mut ranges = edge.evidence;
  for id in [&edge.from, &edge.to] {
    let node = packet["previousDecisions"]
      .as_array()?
      .iter()
      .find(|node| node["id"] == *id)?;
    ranges.push(serde_json::from_value(node.clone()).ok()?);
  }
  // Prior versions remain comparison evidence; the reviewer cites these ranges at current versions.
  for range in &mut ranges {
    range.version = None;
  }
  Some(ranges)
}

fn validate_change(change: &Change, context: &CandidateContext<'_>) -> Result<()> {
  let previous = previous_ranges(&change.previous_id, context.packet).ok_or_else(invalid)?;
  let previous_edge = context.packet["removedRelationships"]
    .as_array()
    .and_then(|edges| edges.iter().find(|edge| edge["id"] == change.previous_id))
    .ok_or_else(invalid)?;
  if !current_evidence(&change.evidence, context)
    || !previous.iter().all(|range| covers(&change.evidence, range))
    || !(1..=128).contains(&change.replacements.len())
    || change.replacements.iter().collect::<HashSet<_>>().len() != change.replacements.len()
  {
    return Err(invalid());
  }
  for id in &change.replacements {
    let edge = supplied_edge(id, context).ok_or_else(invalid)?;
    let ranges = current_ranges(edge, context).ok_or_else(invalid)?;
    if !preserves_endpoints(previous_edge, edge, context)
      || !ranges.iter().all(|range| covers(&change.evidence, range))
    {
      return Err(invalid());
    }
  }
  Ok(())
}

fn preserves_endpoints(
  previous: &Value,
  edge: &Relationship,
  context: &CandidateContext<'_>,
) -> bool {
  if previous["type"] != edge.kind {
    return false;
  }
  [("from", &edge.from), ("to", &edge.to)]
    .iter()
    .all(|(side, id)| {
      let old = &previous[*side];
      if context.graph.decisions.iter().any(|node| *old == node.id) {
        return old == *id;
      }
      let source = context.packet["previousDecisions"]
        .as_array()
        .and_then(|nodes| nodes.iter().find(|node| node["id"] == *old));
      source.is_some_and(|source| {
        context
          .graph
          .decisions
          .iter()
          .any(|node| node.id == **id && source["document"] == node.document)
      })
    })
}

pub(super) fn review_changes(
  file: &Value,
  context: &CandidateContext<'_>,
  pending: &Value,
) -> Result<Vec<Value>> {
  let Some(values) = file.get("relationshipChanges") else {
    return Ok(Vec::new());
  };
  let values = values.as_array().ok_or_else(invalid)?;
  if !(1..=128).contains(&values.len())
    || pending["materializedCheck"] != true
    || pending["staged"] != true
  {
    return Err(invalid());
  }
  let mut seen = HashSet::new();
  for value in values {
    let change: Change = serde_json::from_value(value.clone()).map_err(|_| invalid())?;
    parse_resolution(
      &json!({"id":change.previous_id,"reason":change.reason,"evidence":value["evidence"]}),
    )?;
    if !seen.insert(change.previous_id.clone())
      || !pending["protectedRelationships"]
        .as_array()
        .is_some_and(|ids| ids.contains(&json!(change.previous_id)))
    {
      return Err(invalid());
    }
    validate_change(&change, context)?;
  }
  Ok(values.clone())
}

pub(super) fn resolvable(target: &str, context: &CandidateContext<'_>, changes: &[Value]) -> bool {
  changes.iter().any(|change| change["previousId"] == target)
    || supplied_edge(target, context).is_some_and(|edge| current_ranges(edge, context).is_some())
}

pub(super) fn covers_target(
  resolution: &Resolution,
  target: &str,
  context: &CandidateContext<'_>,
  changes: &[Value],
) -> bool {
  if let Some(change) = changes.iter().find(|change| change["previousId"] == target) {
    let Ok(evidence) = serde_json::from_value::<Vec<Citation>>(change["evidence"].clone()) else {
      return false;
    };
    return current_evidence(&resolution.evidence, context)
      && evidence
        .iter()
        .all(|range| covers(&resolution.evidence, range));
  }
  let Some(edge) = supplied_edge(target, context) else {
    return true;
  };
  current_ranges(edge, context).is_some_and(|ranges| {
    current_evidence(&resolution.evidence, context)
      && ranges
        .iter()
        .all(|range| covers(&resolution.evidence, range))
  })
}

pub(super) fn apply_changes(reviewed: &mut Value, changes: &[Value]) {
  if changes.is_empty() {
    return;
  }
  let mut merged = reviewed["relationshipChanges"]
    .as_array()
    .cloned()
    .unwrap_or_default();
  merged.retain(|original| {
    !changes
      .iter()
      .any(|change| change["previousId"] == original["previousId"])
  });
  merged.extend_from_slice(changes);
  reviewed["relationshipChanges"] = json!(merged);
}
