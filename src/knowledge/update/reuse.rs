use super::{Citation, IngestionUnit, SourceGraph, decision_range, model, overlaps};

// Reuse supplies a proposal, never current coverage or semantic approval.
pub(super) fn extraction(
  state: SourceGraph<'_>,
  units: &[IngestionUnit],
) -> Option<model::Extraction> {
  let SourceGraph { project, graph } = state;
  let covered = |citation: &Citation| {
    units.iter().any(|unit| {
      unit.document == citation.document
        && unit.line_start <= citation.line_start
        && unit.line_end >= citation.line_end
        && citation.version.as_deref()
          == graph
            .units
            .get(&unit.id)
            .and_then(|record| record["version"].as_str())
    })
  };
  let touches = |citation: &Citation| {
    units
      .iter()
      .any(|unit| overlaps(&super::unit_range(unit), citation))
  };
  if units.is_empty()
    || units.iter().any(|unit| {
      graph.units.get(&unit.id).is_none_or(|record| {
        record["document"] != unit.document
          || record["unitHash"] != unit.hash
          || record["version"].as_str().is_none()
      })
    })
    || model::active_warnings(&graph.warnings, &project.documents)
      .iter()
      .any(|warning| match warning {
        model::Warning::Legacy(_) => true,
        model::Warning::Structured(record) => {
          record.scope.is_empty()
            || record.scope.iter().any(|scope| {
              touches(&Citation {
                document: scope.document.clone(),
                line_start: scope.line_start,
                line_end: scope.line_end,
                version: None,
              })
            })
        }
      })
  {
    return None;
  }
  let nodes: Vec<_> = graph
    .decisions
    .iter()
    .filter(|node| touches(&decision_range(node)))
    .collect();
  if nodes
    .iter()
    .any(|node| node.quality != "checked" || !covered(&decision_range(node)))
  {
    return None;
  }
  let edges: Vec<_> = graph
    .relationships
    .iter()
    .filter(|edge| {
      nodes
        .iter()
        .any(|node| node.id == edge.from || node.id == edge.to)
        || edge.evidence.iter().any(touches)
    })
    .collect();
  if edges.iter().any(|edge| {
    edge.quality != "checked"
      || edge.evidence.is_empty()
      || !edge.evidence.iter().all(covered)
      || [&edge.from, &edge.to]
        .iter()
        .any(|id| !nodes.iter().any(|node| node.id == **id))
  }) {
    return None;
  }
  let decisions = serde_json::from_value(super::json!(nodes)).ok()?;
  let mut relationships: Vec<model::ExtractionRelationship> =
    serde_json::from_value(super::json!(edges)).ok()?;
  for edge in &mut relationships {
    for citation in &mut edge.evidence {
      citation.version = None;
    }
  }
  let extraction = model::Extraction {
    decisions,
    relationships,
    uncertainties: Vec::new(),
  };
  model::validate_extraction(&extraction).then_some(extraction)
}

pub(super) fn prepare(
  state: SourceGraph<'_>,
  units: &[IngestionUnit],
  request: &mut crate::execution::Request,
  max_context_bytes: usize,
) -> Option<model::Extraction> {
  let extraction = extraction(state, units)?;
  let documents = super::unique(units.iter().map(|unit| unit.document.clone()));
  let mut packet = request.packet.clone();
  let mut sources = packet["documents"].as_array()?.clone();
  sources.retain(|source| !documents.iter().any(|id| source["id"] == *id));
  sources.extend(super::document_packet(state.project, &documents));
  packet["documents"] = super::json!(sources);
  if super::stringify_knowledge(&packet).len() > max_context_bytes {
    return None;
  }
  request.packet = packet;
  Some(extraction)
}
