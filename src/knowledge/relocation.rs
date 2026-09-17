use crate::documents::markdown;
use crate::documents::{Project, compare_serialized_strings};
use crate::error::{HivexError, Result};
use crate::knowledge::model::{Citation, Graph, Warning, WarningScope, graph_value, parse_graph};
use serde_json::{Map, Value};

const PROTECTED_PARTS: [&str; 5] = ["", ".", "..", ".git", ".hivex"];

#[derive(Clone, Debug)]
pub struct SourceRelocation {
    pub destination_version: String,
    pub from: String,
    pub from_versions: Vec<String>,
    pub graph: Value,
    pub reused: bool,
    pub to: String,
}

fn invalid_snapshot() -> HivexError {
    HivexError::new(
        "INVALID_SNAPSHOT",
        "Knowledge snapshot is not a supported graph JSON document.",
    )
}

fn is_portable_path(value: &str) -> bool {
    markdown::is_markdown_path(value)
        && !value.starts_with('/')
        && !value.contains('\\')
        && !value.contains('\0')
        && value
            .split('/')
            .all(|part| !PROTECTED_PARTS.contains(&part) && part != "node_modules")
}

fn warning_scopes(warning: &Warning) -> Vec<&WarningScope> {
    match warning {
        Warning::Legacy(_) => Vec::new(),
        Warning::Structured(record) => record
            .scope
            .iter()
            .chain(
                record
                    .resolution
                    .as_ref()
                    .into_iter()
                    .flat_map(|resolution| resolution.evidence.iter()),
            )
            .collect(),
    }
}

fn relationship_versions(graph: &Graph, document: &str) -> Vec<String> {
    graph
        .relationships
        .iter()
        .flat_map(|relationship| relationship.evidence.iter())
        .filter(|evidence| evidence.document == document)
        .filter_map(|evidence| evidence.version.clone())
        .collect()
}

fn source_versions(graph: &Graph, document: &str) -> Vec<String> {
    let mut versions = Vec::new();
    if let Some(version) = graph.documents.get(document).and_then(Value::as_str) {
        versions.push(version.to_owned());
    }
    versions.extend(graph.units.values().filter_map(|unit| {
        let unit = unit.as_object()?;
        (unit.get("document")?.as_str()? == document)
            .then(|| unit.get("version")?.as_str().map(ToOwned::to_owned))
            .flatten()
    }));
    versions.extend(
        graph
            .decisions
            .iter()
            .filter(|decision| decision.document == document)
            .map(|decision| decision.version.clone()),
    );
    versions.extend(relationship_versions(graph, document));
    versions.extend(graph.warnings.iter().flat_map(|warning| {
        warning_scopes(warning)
            .into_iter()
            .filter(|scope| scope.document == document)
            .map(|scope| scope.version.clone())
            .collect::<Vec<_>>()
    }));
    versions.sort_by(|left, right| compare_serialized_strings(left, right));
    versions.dedup();
    versions
}

fn has_knowledge(graph: &Graph, document: &str) -> bool {
    graph.documents.contains_key(document)
        || graph.units.values().any(|unit| {
            unit.get("document")
                .and_then(Value::as_str)
                .is_some_and(|value| value == document)
        })
        || graph
            .decisions
            .iter()
            .any(|decision| decision.document == document)
        || graph.relationships.iter().any(|relationship| {
            relationship
                .evidence
                .iter()
                .any(|evidence| evidence.document == document)
        })
        || graph.warnings.iter().any(|warning| {
            warning_scopes(warning)
                .iter()
                .any(|scope| scope.document == document)
        })
}

fn map_citation(citation: &mut Citation, from: &str, to: &str) {
    if citation.document == from {
        citation.document = to.to_owned();
    }
}

fn map_scope(scope: &mut WarningScope, from: &str, to: &str) {
    if scope.document == from {
        scope.document = to.to_owned();
    }
}

fn map_warning(warning: &Warning, from: &str, to: &str) -> Warning {
    match warning {
        Warning::Legacy(message) => Warning::Legacy(message.clone()),
        Warning::Structured(record) => {
            let mut record = record.clone();
            for scope in &mut record.scope {
                map_scope(scope, from, to);
            }
            if let Some(resolution) = &mut record.resolution {
                for citation in &mut resolution.evidence {
                    map_scope(citation, from, to);
                }
            }
            Warning::Structured(record)
        }
    }
}

fn map_units(graph: &Graph, from: &str, to: &str, coverage_relocated: bool) -> Map<String, Value> {
    let mut units = Map::new();
    for (id, unit) in &graph.units {
        let document = unit.get("document").and_then(Value::as_str);
        if !coverage_relocated
            && document.is_some_and(|document| document == from || document == to)
        {
            continue;
        }
        if document != Some(from) {
            units.insert(id.clone(), unit.clone());
            continue;
        }
        let relocated_id = id
            .strip_prefix(&format!("{from}:"))
            .map_or_else(|| id.clone(), |suffix| format!("{to}:{suffix}"));
        let mut unit = unit.clone();
        if let Some(document) = unit.get_mut("document") {
            *document = Value::String(to.to_owned());
        }
        units.insert(relocated_id, unit);
    }
    units
}

fn map_documents(
    graph: &Graph,
    from: &str,
    to: &str,
    coverage_relocated: bool,
) -> Map<String, Value> {
    let mut documents = Map::new();
    for (id, version) in &graph.documents {
        if id != from && id != to {
            documents.insert(id.clone(), version.clone());
        }
    }
    if coverage_relocated && let Some(version) = graph.documents.get(from) {
        documents.insert(to.to_owned(), version.clone());
    }
    documents
}

fn map_graph(graph: Graph, from: &str, to: &str, coverage_relocated: bool) -> Graph {
    let mut relocated = graph;
    for decision in &mut relocated.decisions {
        if decision.document == from {
            decision.document = to.to_owned();
        }
    }
    relocated.documents = map_documents(&relocated, from, to, coverage_relocated);
    relocated.units = map_units(&relocated, from, to, coverage_relocated);
    for relationship in &mut relocated.relationships {
        for evidence in &mut relationship.evidence {
            map_citation(evidence, from, to);
        }
    }
    relocated.warnings = relocated
        .warnings
        .iter()
        .map(|warning| map_warning(warning, from, to))
        .collect();
    relocated
}

pub fn relocate_source(
    graph: &Value,
    project: &Project,
    from: &str,
    to: &str,
) -> Result<SourceRelocation> {
    if from == to || !is_portable_path(from) || !is_portable_path(to) {
        return Err(HivexError::new(
            "INVALID_ARGUMENT",
            "Source relocation paths must be distinct project-local Markdown files",
        ));
    }
    let graph = parse_graph(graph, true).ok_or_else(invalid_snapshot)?;
    if !has_knowledge(&graph, from) {
        return Err(HivexError::new(
            "SOURCE_NOT_FOUND",
            format!("Source is not present in knowledge: {from}"),
        ));
    }
    if project.documents.iter().any(|document| document.id == from) {
        return Err(HivexError::new(
            "INVALID_ARGUMENT",
            format!("Source must no longer be selected: {from}"),
        ));
    }
    let destination = project
        .current_documents
        .iter()
        .find(|document| document.id == to)
        .ok_or_else(|| {
            HivexError::new(
                "SOURCE_NOT_FOUND",
                format!("Destination is not a selected current Markdown source: {to}"),
            )
        })?;
    let versions = source_versions(&graph, from);
    let has_destination_knowledge = has_knowledge(&graph, to);
    let has_unversioned_evidence = graph.relationships.iter().any(|relationship| {
        relationship
            .evidence
            .iter()
            .any(|evidence| evidence.document == from && evidence.version.is_none())
    });
    let reused = !has_destination_knowledge
        && !has_unversioned_evidence
        && !versions.is_empty()
        && versions.iter().all(|version| version == &destination.hash);
    let relocated = map_graph(graph, from, to, reused);
    Ok(SourceRelocation {
        destination_version: destination.hash.clone(),
        from: from.to_owned(),
        from_versions: versions,
        graph: graph_value(&relocated, false),
        reused,
        to: to.to_owned(),
    })
}
