use super::{
  Citation, Document, Graph, IngestionResult, IngestionUnit, Project, SourceGraph, Value,
  decision_range, is_current_source, json, model, strings, unique,
};
use std::collections::HashSet;

pub(super) fn source_ready(graph: &Graph, source: &Document) -> bool {
  graph
    .decisions
    .iter()
    .filter(|node| node.document == source.id)
    .all(|node| node.version == source.hash)
    && graph
      .relationships
      .iter()
      .flat_map(|edge| &edge.evidence)
      .filter(|citation| citation.document == source.id)
      .all(|citation| citation.version.as_deref() == Some(&source.hash))
}

pub(super) fn scope(
  state: SourceGraph<'_>,
  plan: &IngestionResult,
  units: &[IngestionUnit],
) -> Value {
  let selected: HashSet<_> = units.iter().map(|unit| &unit.id).collect();
  let SourceGraph { project, graph } = state;
  json!({
    "documents":unique(plan.units.iter().map(|unit| unit.document.clone())),
    "remainingDocuments":unique(plan.units.iter()
      .filter(|unit| !selected.contains(&unit.id) && !is_current_source(project,
        &unit.document, graph.units.get(&unit.id).and_then(|coverage| coverage["version"].as_str())))
      .map(|unit| unit.document.clone()))
  })
}

pub(super) fn protects_source(project: &Project, citation: &Citation, pending: &Value) -> bool {
  if pending["sourceTransition"].is_object() {
    return project
      .documents
      .iter()
      .any(|source| source.id == citation.document);
  }
  is_current_source(project, &citation.document, citation.version.as_deref())
}

fn edge_sources(edge: &model::Relationship, graph: &Graph) -> Vec<Citation> {
  edge
    .evidence
    .iter()
    .cloned()
    .chain(
      graph
        .decisions
        .iter()
        .filter(|node| node.id == edge.from || node.id == edge.to)
        .map(decision_range),
    )
    .collect()
}

pub(super) fn preserve(project: &Project, graph: &Graph, candidate: &mut Graph, pending: &Value) {
  let transition = &pending["sourceTransition"];
  if !transition.is_object() {
    return;
  }
  let remaining = strings(&transition["remainingDocuments"]);
  let documents = strings(&transition["documents"]);
  let deferred: Vec<_> = graph
    .relationships
    .iter()
    .filter(|edge| {
      let citations = edge_sources(edge, graph);
      citations
        .iter()
        .any(|citation| remaining.contains(&citation.document))
        && citations.iter().any(|citation| {
          !is_current_source(project, &citation.document, citation.version.as_deref())
        })
    })
    .collect();
  let mut keep: HashSet<_> = deferred
    .iter()
    .flat_map(|edge| [&edge.from, &edge.to])
    .collect();
  keep.extend(
    graph
      .decisions
      .iter()
      .filter(|node| remaining.contains(&node.document))
      .map(|node| &node.id),
  );
  candidate.decisions.retain(|node| {
    !documents.contains(&node.document)
      || is_current_source(project, &node.document, Some(&node.version))
      || keep.contains(&node.id)
  });
  for node in graph
    .decisions
    .iter()
    .filter(|node| keep.contains(&node.id))
  {
    if !candidate.decisions.iter().any(|entry| entry.id == node.id) {
      candidate.decisions.push(node.clone());
    }
  }
  let ids: HashSet<_> = candidate.decisions.iter().map(|node| &node.id).collect();
  candidate.relationships.retain(|edge| {
    ids.contains(&edge.from)
      && ids.contains(&edge.to)
      && edge.evidence.iter().all(|citation| {
        !documents.contains(&citation.document)
          || remaining.contains(&citation.document)
          || is_current_source(project, &citation.document, citation.version.as_deref())
      })
  });
  for edge in deferred {
    if !candidate
      .relationships
      .iter()
      .any(|entry| entry.id == edge.id)
    {
      candidate.relationships.push(edge.clone());
    }
  }
}
