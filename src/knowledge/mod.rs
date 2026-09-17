pub(crate) use ingestion::{RepairRange, ingestion_units};
pub(crate) use model::{
  Citation, Graph, SuppliedDocument, active_warnings, digest, empty_graph, graph_value,
  normalize_integral_numbers, parse_check, parse_extraction, parse_graph, source_evidence,
  supplied_citation, validate_citation, warning_summary,
};
pub(crate) use search::{Record, rank_lexically};
pub(crate) use serialization::stringify_knowledge;
pub(crate) use snapshot::{read_knowledge_snapshot, snapshot_report, write_knowledge_snapshot};
pub(crate) use snapshot_file::shared_knowledge;
pub(crate) use update::{document_excerpt, document_packet, update};
pub(crate) use warning_review::{parse_warning_resolutions, warning_baseline};
mod ingestion;
mod model;
mod relocation;
pub(crate) use relocation::{SourceRelocation, relocate_source};
mod search;
mod serialization;
mod snapshot;
mod snapshot_file;
#[cfg(test)]
mod tests;
mod update;
mod warning_review;
mod warnings;
use crate::documents::hash;
use crate::documents::{Document, Project};
use crate::error::{HivexError, Result};
pub(crate) use warnings::warning_report;

use serde_json::{Value, json};
use std::collections::HashSet;

pub fn is_current_source(project: &Project, document: &str, version: Option<&str>) -> bool {
  project
    .documents
    .iter()
    .any(|source| source.id == document && Some(source.hash.as_str()) == version)
}

pub fn historical_graph(project: &Project, mut graph: Graph) -> Graph {
  for entry in &mut graph.decisions {
    if project
      .historical_documents
      .iter()
      .any(|source| source.id == entry.document)
    {
      "historical".clone_into(&mut entry.status);
    }
  }
  graph
}

pub struct AvailableGraph {
  pub graph: Graph,
  pub unavailable: Vec<(String, String, Vec<String>)>,
}

pub fn current_graph(project: &Project) -> Result<AvailableGraph> {
  let graph = model::parse_graph(
    &crate::knowledge::snapshot::stored_graph(&project.root)?,
    false,
  )
  .ok_or_else(|| {
    HivexError::new(
      "INVALID_SNAPSHOT",
      "Stored graph is not a supported knowledge graph",
    )
  })?;
  let mut graph = historical_graph(project, graph);
  let ids: HashSet<_> = graph
    .decisions
    .iter()
    .filter(|entry| is_current_source(project, &entry.document, Some(&entry.version)))
    .map(|entry| entry.id.clone())
    .collect();
  let mut unavailable = Vec::new();
  graph.relationships.retain(|edge| {
    let available = ids.contains(&edge.from)
      && ids.contains(&edge.to)
      && edge
        .evidence
        .iter()
        .all(|entry| is_current_source(project, &entry.document, entry.version.as_deref()));
    if !available {
      let sources = edge
        .evidence
        .iter()
        .map(|citation| (citation.document.clone(), citation.version.clone()))
        .chain(
          graph
            .decisions
            .iter()
            .filter(|entry| entry.id == edge.from || entry.id == edge.to)
            .map(|entry| (entry.document.clone(), Some(entry.version.clone()))),
        );
      unavailable.push((
        edge.from.clone(),
        edge.to.clone(),
        unique(
          sources
            .filter(|(document, version)| !is_current_source(project, document, version.as_deref()))
            .map(|(document, _)| document),
        ),
      ));
    }
    available
  });
  graph.decisions.retain(|entry| ids.contains(&entry.id));
  Ok(AvailableGraph { graph, unavailable })
}

pub fn pending_documents(
  project: &Project,
  graph: &Graph,
  relevant: &HashSet<String>,
) -> Vec<String> {
  unique(
    project
      .documents
      .iter()
      .map(|document| document.id.clone())
      .chain(graph.documents.keys().cloned()),
  )
  .into_iter()
  .filter(|id| {
    let source = project.documents.iter().find(|document| &document.id == id);
    let is_relevant = source.is_none_or(|document| !document.historical) || relevant.contains(id);
    source.map(|document| document.hash.as_str()) != graph.documents.get(id).and_then(Value::as_str)
      && is_relevant
  })
  .collect()
}

pub fn knowledge_snapshot(project: &Project, relevant: &HashSet<String>) -> String {
  let history: Vec<_> = project
    .historical_documents
    .iter()
    .filter(|document| relevant.contains(&document.id))
    .map(|document| json!([document.id, document.hash]))
    .collect();
  hash(&json!([project.current_snapshot, history]).to_string())
}

fn neighborhood(graph: &Graph, seeds: &[String], limit: usize) -> (HashSet<String>, Vec<String>) {
  let mut ids: HashSet<_> = seeds.iter().cloned().collect();
  let mut queue = seeds.to_vec();
  let mut pending = Vec::new();
  let mut cursor = 0;
  while cursor < queue.len() {
    let id = queue[cursor].clone();
    cursor += 1;
    for edge in graph
      .relationships
      .iter()
      .filter(|edge| edge.from == id || edge.to == id)
    {
      let next = if edge.from == id {
        &edge.to
      } else {
        &edge.from
      };
      if ids.contains(next) {
        continue;
      }
      if ids.len() >= limit {
        pending.push(next.clone());
        continue;
      }
      ids.insert(next.clone());
      queue.push(next.clone());
    }
  }
  pending.retain(|id| !ids.contains(id));
  (ids, unique(pending))
}

pub fn query_graph(project: &Project, options: &Retrieval) -> Result<Value> {
  let AvailableGraph { graph, unavailable } = current_graph(project)?;
  let visible: Vec<&Document> = project
    .documents
    .iter()
    .filter(|document| !document.historical || options.sources.contains(&document.id))
    .collect();
  let (seeds, document_ids) = retrieval_seeds(&graph, &visible, options)?;
  let (expanded, pending) = if ["neighbors", "ask", "review"].contains(&options.command) {
    neighborhood(&graph, &seeds, options.limit)
  } else {
    (seeds.into_iter().collect(), Vec::new())
  };
  let relationships: Vec<_> = graph
    .relationships
    .iter()
    .filter(|edge| expanded.contains(&edge.from) && expanded.contains(&edge.to))
    .collect();
  let relevant: HashSet<_> = document_ids
    .iter()
    .cloned()
    .chain(
      graph
        .decisions
        .iter()
        .filter(|entry| expanded.contains(&entry.id))
        .map(|entry| entry.document.clone()),
    )
    .chain(relationships.iter().flat_map(|edge| {
      edge
        .evidence
        .iter()
        .map(|citation| citation.document.clone())
    }))
    .collect();
  let decisions: Vec<_> = graph.decisions.iter().filter(|entry| expanded.contains(&entry.id)).map(|entry| {
        let evidence = model::source_evidence(&Citation {document: entry.document.clone(),line_start: entry.line_start,line_end: entry.line_end,version: Some(entry.version.clone())}, project);
        json!({
"conditions":entry.conditions,
"document":entry.document,
"evidence":evidence,
"exceptions":entry.exceptions,
"historical":project.documents.iter().find(|source| source.id == entry.document).is_some_and(|source| source.historical),
"id":entry.id,
"kind":entry.kind,
"quality":entry.quality,
"reason":entry.reason,
"status":entry.status,
"text":entry.text,
"version":entry.version
})
    }).collect();
  let mut warnings: Vec<Value> = project
    .warnings
    .iter()
    .filter(|warning| warning.path == "." || relevant.contains(&warning.path))
    .map(|warning| json!(warning))
    .collect();
  warnings.extend(
    model::active_warnings(&graph.warnings, &project.documents)
      .into_iter()
      .filter(|warning| match warning {
        model::Warning::Legacy(_) => true,
        model::Warning::Structured(record) => {
          record.scope.is_empty()
            || record.scope.iter().any(|scope| {
              relevant.contains(&scope.document)
                && is_current_source(project, &scope.document, Some(&scope.version))
            })
        }
      })
      .map(|warning| json!(warning)),
  );
  warnings.extend(unique(project.documents.iter().filter(|document| relevant.contains(&document.id)).flat_map(|document| document.links.clone())).into_iter().filter(|id| !relevant.contains(id) && project.current_documents.iter().all(|document| document.id != *id)).map(|id| json!(format!("Referenced source has not been consulted: {id}. Read it or select --source to assess applicability."))));
  Ok(json!({
  "command":options.command,
  "decisions":decisions,
  "documents":project.documents.iter().filter(|document| document_ids.contains(&document.id)).map(|document| json!({"id":document.id,"title":document.title,"version":document.hash})).collect::<Vec<_>>(),
  "pendingDocuments":pending_documents(project,&graph,&relevant),
  "relationships":relationships,
  "snapshot":knowledge_snapshot(project,&relevant),
  "unavailableDocuments":unique(unavailable.iter().filter(|(from,to,_)| expanded.contains(from) || expanded.contains(to)).flat_map(|(_,_,documents)| documents.clone())),
  "unexpandedDecisions":unique(pending.into_iter().chain(unavailable.iter().filter_map(|(from,to,_)| match (expanded.contains(from), expanded.contains(to)) { (true, _) => Some(to.clone()), (false, true) => Some(from.clone()), (false, false) => None }))),
  "warnings":warnings
  }))
}

fn unique(values: impl IntoIterator<Item = String>) -> Vec<String> {
  let mut seen = HashSet::new();
  values
    .into_iter()
    .filter(|value| seen.insert(value.clone()))
    .collect()
}

/// Retrieval needs no execution profile, retry controls or repair mutation settings.
pub struct Retrieval<'a> {
  pub command: &'a str,
  pub query: &'a str,
  pub sources: &'a [String],
  pub limit: usize,
}

fn retrieval_seeds(
  graph: &Graph,
  visible: &[&Document],
  options: &Retrieval,
) -> Result<(Vec<String>, HashSet<String>)> {
  let visible_ids: HashSet<_> = visible
    .iter()
    .map(|document| document.id.as_str())
    .collect();
  let hits = rank_lexically(
    &graph
      .decisions
      .iter()
      .filter(|entry| visible_ids.contains(entry.document.as_str()))
      .map(|entry| Record {
        id: entry.id.clone(),
        title: entry.document.clone(),
        content: [
          vec![entry.text.clone(), entry.reason.clone()],
          entry.conditions.clone(),
          entry.exceptions.clone(),
        ]
        .concat()
        .join(" "),
      })
      .collect::<Vec<_>>(),
    options.query,
    options.limit,
  )?;
  let document_hits = if options.command == "neighbors" {
    Vec::new()
  } else {
    rank_lexically(
      &visible
        .iter()
        .map(|document| Record {
          id: document.id.clone(),
          title: document.title.clone(),
          content: document.text.clone(),
        })
        .collect::<Vec<_>>(),
      options.query,
      options.limit.min(6),
    )?
  };
  let document_ids: HashSet<_> = document_hits
    .iter()
    .cloned()
    .chain(options.sources.iter().cloned())
    .collect();
  let seeds = if options.command == "neighbors" {
    vec![options.query.to_owned()]
  } else {
    unique(
      hits.into_iter().chain(
        graph
          .decisions
          .iter()
          .filter(|entry| {
            visible_ids.contains(entry.document.as_str()) && document_ids.contains(&entry.document)
          })
          .map(|entry| entry.id.clone()),
      ),
    )
    .into_iter()
    .take(options.limit)
    .collect()
  };
  Ok((seeds, document_ids))
}
