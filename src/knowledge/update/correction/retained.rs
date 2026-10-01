use super::{Result, Value, invalid, model};
use crate::knowledge::update::{current_retained_citation, decision_range};
use crate::knowledge::warnings::{CandidateContext, validate_relationship_changes};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DecisionReplacement {
  pub previous_id: String,
  pub replacement: model::ExtractionDecision,
}

fn restored_range(
  record: &DecisionReplacement,
  graph: &model::Graph,
  pending: &Value,
  project: &crate::documents::Project,
) -> Result<model::Citation> {
  let old = graph
    .decisions
    .iter()
    .find(|node| node.id == record.previous_id)
    .ok_or_else(invalid)?;
  let node = &record.replacement;
  let source = pending["packet"]["documents"]
    .as_array()
    .into_iter()
    .flatten()
    .find(|source| source["id"] == node.document)
    .ok_or_else(invalid)?;
  let citation = model::Citation {
    document: node.document.clone(),
    line_start: node.line_start,
    line_end: node.line_end,
    version: source["version"].as_str().map(str::to_owned),
  };
  let protected = pending["packet"]["previousRelationships"]
    .as_array()
    .into_iter()
    .flatten()
    .any(|edge| {
      (edge["from"] == old.id || edge["to"] == old.id)
        && pending["protectedRelationships"]
          .as_array()
          .is_some_and(|ids| ids.contains(&edge["id"]))
    });
  if !protected
    || crate::knowledge::is_current_source(project, &old.document, Some(&old.version))
    || node.document != old.document
    || !crate::knowledge::is_current_source(
      project,
      &citation.document,
      citation.version.as_deref(),
    )
  {
    return Err(invalid());
  }
  Ok(citation)
}

fn same_interpretation(entry: &model::Decision, node: &model::ExtractionDecision) -> bool {
  entry.document == node.document
    && entry.line_start == node.line_start
    && entry.line_end == node.line_end
    && entry.text == node.text
    && entry.conditions == node.conditions
    && entry.exceptions == node.exceptions
    && entry.kind == node.kind
    && entry.status == node.status
}

fn decision_collision(
  record: &DecisionReplacement,
  graph: &model::Graph,
  pending: &Value,
  project: &crate::documents::Project,
) -> bool {
  let node = &record.replacement;
  let overlaps = |document: &str, start: usize, end: usize| {
    document == node.document && start <= node.line_end && end >= node.line_start
  };
  graph.decisions.iter().any(|entry| {
    entry.id == node.id
      || entry.local_id == node.id
      || (crate::knowledge::is_current_source(project, &entry.document, Some(&entry.version))
        && same_interpretation(entry, node))
  }) || pending["extraction"]["decisions"]
    .as_array()
    .into_iter()
    .flatten()
    .any(|entry| {
      entry["id"] == node.id
        || overlaps(
          entry["document"].as_str().unwrap_or_default(),
          entry["lineStart"]
            .as_u64()
            .and_then(|line| usize::try_from(line).ok())
            .unwrap_or_default(),
          entry["lineEnd"]
            .as_u64()
            .and_then(|line| usize::try_from(line).ok())
            .unwrap_or_default(),
        )
    })
}

pub(super) fn decision_ranges(
  evidence: super::BatchEvidence<'_>,
  graph: &model::Graph,
  work: &crate::work::Work,
  correction: &super::Correction,
) -> Result<Vec<model::Citation>> {
  let pending = &work.value()["pending"];
  let mut seen = HashSet::new();
  let mut ranges = Vec::new();
  for record in &correction.retained_decisions {
    let node = &record.replacement;
    let citation = restored_range(record, graph, pending, evidence.project)?;
    let covered = evidence.plan.units.iter().any(|unit| {
      unit.document == citation.document
        && unit.line_start <= citation.line_start
        && unit.line_end >= citation.line_end
        && work.value()["plannedUnits"]
          .as_array()
          .is_some_and(|ids| ids.contains(&json!(unit.id)))
        && !work.remaining().contains(&unit.id)
        && graph.units.get(&unit.id).is_some_and(|coverage| {
          coverage["workKey"] == work.key()
            && coverage["document"] == citation.document
            && coverage["version"].as_str() == citation.version.as_deref()
        })
    });
    if !seen.insert(&record.previous_id)
      || !seen.insert(&node.id)
      || !covered
      || decision_collision(record, graph, pending, evidence.project)
      || correction.remove_decisions.contains(&node.id)
      || !correction
        .relationships
        .iter()
        .any(|edge| edge.from == node.id || edge.to == node.id)
      || !model::valid_citation(&citation, &evidence.project.documents)
    {
      return Err(invalid());
    }
    ranges.push(citation);
  }
  Ok(ranges)
}

pub(super) fn supply_ranges(
  project: &crate::documents::Project,
  pending: &mut Value,
  ranges: &[model::Citation],
) -> Result<()> {
  for range in ranges {
    let supplied = super::super::document_packet(project, std::slice::from_ref(&range.document))
      .into_iter()
      .next()
      .ok_or_else(invalid)?;
    let excerpt = super::super::document_excerpt(supplied, std::slice::from_ref(range));
    let source = pending["packet"]["documents"]
      .as_array_mut()
      .ok_or_else(invalid)?
      .iter_mut()
      .find(|source| source["id"] == range.document)
      .ok_or_else(invalid)?;
    if source["version"] != excerpt["version"] || source["lineCount"] != excerpt["lineCount"] {
      return Err(invalid());
    }
    let lines = source["lines"].as_array_mut().ok_or_else(invalid)?;
    for line in excerpt["lines"].as_array().ok_or_else(invalid)? {
      match lines.iter().find(|row| row[0] == line[0]) {
        Some(row) if row != line => return Err(invalid()),
        Some(_) => (),
        None => lines.push(line.clone()),
      }
    }
    lines.sort_by_key(|row| row[0].as_u64().unwrap_or_default());
  }
  Ok(())
}

pub(super) fn comparison_ranges(
  pending: &Value,
  graph: &model::Graph,
  records: &[Replacement],
  project: &crate::documents::Project,
) -> Result<Vec<model::Citation>> {
  let mut ranges = Vec::new();
  for previous in pending["packet"]["previousRelationships"]
    .as_array()
    .into_iter()
    .flatten()
  {
    if !pending["protectedRelationships"]
      .as_array()
      .is_some_and(|ids| ids.contains(&previous["id"]))
    {
      continue;
    }
    let edge: model::Relationship =
      serde_json::from_value(previous.clone()).map_err(|_| invalid())?;
    ranges.extend(edge.evidence);
    for id in [&edge.from, &edge.to] {
      let node = graph
        .decisions
        .iter()
        .find(|node| node.id == *id)
        .ok_or_else(invalid)?;
      ranges.push(decision_range(node));
    }
  }
  for record in records {
    if !pending["protectedRelationships"]
      .as_array()
      .is_some_and(|ids| ids.contains(&json!(record.previous_id)))
    {
      return Err(invalid());
    }
    for id in [&record.replacement.from, &record.replacement.to] {
      if !pending["existing"]
        .as_array()
        .is_some_and(|ids| ids.contains(&json!(id)))
      {
        return Err(invalid());
      }
      let node = graph
        .decisions
        .iter()
        .find(|node| node.id == *id)
        .ok_or_else(invalid)?;
      ranges.push(decision_range(node));
    }
  }
  ranges.extend(current_edge_ranges(graph, pending, project));
  Ok(ranges)
}

fn current_edge_ranges(
  graph: &model::Graph,
  pending: &Value,
  project: &crate::documents::Project,
) -> Vec<model::Citation> {
  let mut ranges = Vec::new();
  let work = pending["batch"]
    .as_str()
    .and_then(|batch| batch.split_once(':'))
    .map(|(id, _)| id);
  for edge in &graph.relationships {
    let supplied = [&edge.from, &edge.to].iter().all(|id| {
      pending["existing"]
        .as_array()
        .is_some_and(|ids| ids.contains(&json!(id)))
        && graph.decisions.iter().any(|node| {
          node.id == **id
            && crate::knowledge::is_current_source(project, &node.document, Some(&node.version))
        })
    });
    if edge.batch != pending["batch"]
      && edge.batch.split_once(':').map(|(id, _)| id) == work
      && supplied
      && edge.evidence.iter().all(|citation| {
        crate::knowledge::is_current_source(
          project,
          &citation.document,
          citation.version.as_deref(),
        ) && model::valid_citation(citation, &project.documents)
          && pending["packet"]["documents"]
            .as_array()
            .is_some_and(|documents| {
              documents.iter().any(|document| {
                document["id"] == citation.document
                  && document["version"].as_str() == citation.version.as_deref()
              })
            })
      })
    {
      ranges.extend(edge.evidence.clone());
    }
  }
  ranges
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Replacement {
  pub previous_id: String,
  pub replacement: model::ExtractionRelationship,
}

fn exact_previous<'a>(
  record: &Replacement,
  graph: &'a model::Graph,
) -> Option<&'a model::Relationship> {
  graph.relationships.iter().find(|edge| {
    edge.id == record.previous_id
      && edge.local_id == record.replacement.id
      && edge.from == record.replacement.from
      && edge.to == record.replacement.to
      && edge.kind == record.replacement.kind
      && edge.reason == record.replacement.reason
      && edge.evidence == record.replacement.evidence
  })
}

pub(super) fn check_accepted(records: &[Replacement], graph: &model::Graph) -> Result<()> {
  if records.iter().any(|record| {
    exact_previous(record, graph).is_none()
      && graph
        .relationships
        .iter()
        .any(|edge| edge.id == record.replacement.id || edge.local_id == record.replacement.id)
  }) {
    return Err(invalid());
  }
  Ok(())
}

fn valid_endpoints(
  record: &Replacement,
  context: CandidateContext<'_>,
  pending: &Value,
  exact: bool,
) -> bool {
  let supplied: HashSet<_> = pending["existing"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(Value::as_str)
    .collect();
  let work = pending["batch"]
    .as_str()
    .and_then(|batch| batch.split_once(':'))
    .map(|(id, _)| id);
  let endpoints: Option<Vec<_>> = [&record.replacement.from, &record.replacement.to]
    .iter()
    .map(|id| {
      context
        .graph
        .decisions
        .iter()
        .find(|node| node.id == **id && supplied.contains(id.as_str()))
    })
    .collect();
  endpoints.is_some_and(|nodes| {
    nodes.iter().all(|node| {
      node.batch != pending["batch"] && current_retained_citation(&decision_range(node), pending)
    }) && (exact
      || nodes.iter().any(|node| {
        node.batch != pending["batch"]
          && node
            .batch
            .split_once(':')
            .is_some_and(|(id, _)| Some(id) == work)
      }))
  })
}

pub(super) fn append(
  context: (CandidateContext<'_>, &model::Graph),
  pending: &Value,
  correction: &super::Correction,
  extraction: &mut model::Extraction,
) -> Result<Vec<String>> {
  let (context, accepted) = context;
  let protected: HashSet<_> = pending["protectedRelationships"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(Value::as_str)
    .collect();
  let previous: HashSet<_> = pending["packet"]["previousRelationships"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|edge| edge["id"].as_str())
    .collect();
  let mut seen = HashSet::new();
  let mut preserved = Vec::new();
  for record in &correction.retained_relationships {
    let edge = &record.replacement;
    let exact = exact_previous(record, accepted);
    if !seen.insert(&record.previous_id)
      || !protected.contains(record.previous_id.as_str())
      || !previous.contains(record.previous_id.as_str())
      || !valid_endpoints(record, context, pending, exact.is_some())
      || correction.remove_relationships.contains(&edge.id)
      || correction.remove_decisions.contains(&edge.from)
      || correction.remove_decisions.contains(&edge.to)
      || (exact.is_none()
        && extraction
          .relationships
          .iter()
          .any(|entry| entry.id == edge.id))
      || context.graph.relationships.iter().any(|entry| {
        entry.id == edge.id
          || (exact.is_none() && entry.local_id == edge.id)
          || (entry.from == edge.from && entry.to == edge.to && entry.kind == edge.kind)
      })
    {
      return Err(invalid());
    }
    if let Some(previous) = exact {
      preserved.push(previous.id.clone());
    } else {
      extraction.relationships.push(edge.clone());
    }
  }
  Ok(preserved)
}

pub(super) fn preserve(
  accepted: &model::Graph,
  candidate: &mut model::Graph,
  pending: &Value,
) -> Result<()> {
  for id in super::super::strings(&pending["preservedRelationshipIds"]) {
    let edge = accepted
      .relationships
      .iter()
      .find(|edge| edge.id == id)
      .ok_or_else(invalid)?;
    // Eligibility was checked while the edge was lost. Restoring it removes it
    // from the pending loss set, but its exact previous record remains bound.
    if !pending["packet"]["previousRelationships"]
      .as_array()
      .is_some_and(|edges| edges.iter().any(|previous| *previous == json!(edge)))
      || !super::super::current_retained_edge(edge, candidate, pending)
      || [&edge.from, &edge.to].iter().any(|id| {
        let previous = accepted.decisions.iter().find(|node| node.id == **id);
        previous.is_none()
          || !candidate
            .decisions
            .iter()
            .any(|node| Some(node) == previous)
      })
      || candidate.relationships.iter().any(|entry| {
        entry.id == edge.id
          || (entry.from == edge.from && entry.to == edge.to && entry.kind == edge.kind)
      })
    {
      return Err(invalid());
    }
    candidate.relationships.push(edge.clone());
  }
  Ok(())
}

pub(super) fn validate(
  context: CandidateContext<'_>,
  pending: &Value,
  records: &[Replacement],
) -> Result<()> {
  if records.is_empty() {
    return Ok(());
  }
  let mut changes = Vec::new();
  for record in records {
    if exact_previous(record, context.graph).is_some()
      && pending["preservedRelationshipIds"]
        .as_array()
        .is_some_and(|ids| ids.contains(&json!(record.previous_id)))
    {
      continue;
    }
    let edge = context
      .graph
      .relationships
      .iter()
      .find(|edge| edge.local_id == record.replacement.id && edge.batch == pending["batch"])
      .ok_or_else(invalid)?;
    changes.push(
      json!({"previousId":record.previous_id,"replacements":[edge.id],
      "reason":record.replacement.reason,"evidence":record.replacement.evidence}),
    );
  }
  if changes.is_empty() {
    Ok(())
  } else {
    validate_relationship_changes(context, pending, &json!(changes)).map_err(|_| invalid())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::documents::{Document, Project};

  #[test]
  fn corrected_context_adds_only_required_current_passages_and_rejects_changed_supplied_text() {
    let document = Document {
      id: "guide.md".into(),
      path: "guide.md".into(),
      title: "Guide".into(),
      text: "# Guide\n\nComparison is context, not authority.\n\nUnrelated passage.\n".into(),
      hash: "current".into(),
      status: None,
      historical: false,
      links: Vec::new(),
    };
    let project = Project {
      root: std::path::PathBuf::new(),
      snapshot: String::new(),
      current_snapshot: String::new(),
      documents: vec![document.clone()],
      current_documents: vec![document],
      historical_documents: Vec::new(),
      warnings: Vec::new(),
    };
    let original = json!({"packet":{"documents":[{"id":"guide.md","version":"current","lineCount":5,"lines":[[1,"# Guide"]]}]}});
    let range = model::Citation {
      document: "guide.md".into(),
      line_start: 3,
      line_end: 3,
      version: Some("current".into()),
    };
    let mut pending = original.clone();
    supply_ranges(&project, &mut pending, std::slice::from_ref(&range)).unwrap();
    assert_eq!(
      pending["packet"]["documents"][0]["lines"],
      json!([[1, "# Guide"], [3, "Comparison is context, not authority."]])
    );
    supply_ranges(&project, &mut pending, std::slice::from_ref(&range)).unwrap();
    assert_eq!(
      pending["packet"]["documents"][0]["lines"]
        .as_array()
        .unwrap()
        .len(),
      2
    );
    assert_eq!(
      original["packet"]["documents"][0]["lines"],
      json!([[1, "# Guide"]])
    );
    for case in ["version", "line-count", "text"] {
      let mut changed = pending.clone();
      match case {
        "version" => changed["packet"]["documents"][0]["version"] = json!("stale"),
        "line-count" => changed["packet"]["documents"][0]["lineCount"] = json!(99),
        "text" => changed["packet"]["documents"][0]["lines"][1][1] = json!("Other interpretation."),
        _ => unreachable!(),
      }
      assert!(
        supply_ranges(&project, &mut changed, std::slice::from_ref(&range)).is_err(),
        "{case}"
      );
    }
  }

  #[test]
  fn current_edge_context_does_not_add_an_unselected_third_authority() {
    let document = |id: &str| Document {
      id: id.into(),
      path: id.into(),
      title: id.into(),
      text: "# Rule\n\nCurrent rule.\n".into(),
      hash: "current".into(),
      status: None,
      historical: false,
      links: Vec::new(),
    };
    let project = Project {
      root: std::path::PathBuf::new(),
      snapshot: String::new(),
      current_snapshot: String::new(),
      documents: vec![document("a.md"), document("b.md"), document("c.md")],
      current_documents: Vec::new(),
      historical_documents: Vec::new(),
      warnings: Vec::new(),
    };
    let node = |id: &str| model::Decision {
      id: id.into(),
      document: id.into(),
      version: "current".into(),
      line_start: 3,
      line_end: 3,
      ..Default::default()
    };
    let evidence = model::Citation {
      document: "c.md".into(),
      line_start: 3,
      line_end: 3,
      version: Some("current".into()),
    };
    let graph = model::Graph {
      decisions: vec![node("a.md"), node("b.md")],
      relationships: vec![model::Relationship {
        from: "a.md".into(),
        to: "b.md".into(),
        batch: "work:earlier".into(),
        evidence: vec![evidence.clone()],
        ..Default::default()
      }],
      ..Default::default()
    };
    let mut pending = json!({"batch":"work:pending","existing":["a.md","b.md"],"packet":{"documents":[
      {"id":"a.md","version":"current"},{"id":"b.md","version":"current"}
    ]}});
    assert!(current_edge_ranges(&graph, &pending, &project).is_empty());
    pending["packet"]["documents"]
      .as_array_mut()
      .unwrap()
      .push(json!({"id":"c.md","version":"current"}));
    assert_eq!(
      current_edge_ranges(&graph, &pending, &project),
      vec![evidence]
    );
  }
}
