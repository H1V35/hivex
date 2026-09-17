use crate::arguments::{self, trim_js_whitespace};
use crate::documents::{Document, Project, load_project};
use crate::error::{HivexError, Result};
use crate::ingestion::RepairRange;
use crate::knowledge_model::{self as model, Citation, Graph};
use crate::lexical::{Record, rank_lexically};
use crate::markdown::hash;
use serde_json::{Value, json};
use std::collections::HashSet;

pub struct Options {
    pub command: String,
    pub query: String,
    pub root: String,
    pub sources: Vec<String>,
    pub base: Option<String>,
    pub binary: String,
    pub deadline_ms: u64,
    pub limit: usize,
    pub max_calls: Option<u64>,
    pub max_input_bytes: Option<u64>,
    pub max_context_bytes: usize,
    pub repair: Vec<String>,
    pub repair_ranges: Vec<RepairRange>,
    pub repair_reason: String,
    pub retry_failed: bool,
    pub implementation: Option<Value>,
    pub retrieval_query: Option<String>,
}

fn argument(message: impl Into<String>) -> HivexError {
    HivexError::new("INVALID_ARGUMENT", message)
}

fn bounded(value: Option<&String>, minimum: u64, maximum: u64) -> Result<Option<u64>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let number = arguments::parse_number(value)
        .filter(|number| {
            number.is_finite()
                && number.fract() == 0.0
                && *number >= minimum as f64
                && *number <= maximum as f64
        })
        .ok_or_else(|| {
            argument(format!(
                "Expected an integer between {minimum} and {maximum}"
            ))
        })?;
    Ok(Some(number as u64))
}

fn unique(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

pub fn options_for(input: &[String]) -> Result<Options> {
    let parsed = arguments::parse(
        input,
        &[
            "base",
            "codex",
            "deadline-ms",
            "limit",
            "max-calls",
            "max-context-bytes",
            "max-input-bytes",
            "reason",
            "repair",
            "repair-range",
            "root",
            "source",
        ],
        &["retry-failed"],
    )?;
    let command = parsed.positionals.first().cloned().unwrap_or_default();
    let query = trim_js_whitespace(
        parsed
            .positionals
            .get(1)
            .map(String::as_str)
            .unwrap_or_default(),
    )
    .to_owned();
    let required = ["search", "neighbors", "ask", "review"].contains(&command.as_str());
    if (required && query.is_empty())
        || !["update", "search", "neighbors", "ask", "review", "status"].contains(&command.as_str())
        || parsed.positionals.len() != if required { 2 } else { 1 }
    {
        return Err(argument(
            "Use update, status, or search/ask/neighbors with one query or ID",
        ));
    }
    let mut repair_ranges = Vec::new();
    let expression =
        regex::Regex::new(r"^(.+):([0-9]+)-([0-9]+)$").expect("valid range expression");
    for range in parsed.repeated.get("repair-range").into_iter().flatten() {
        let captures = expression.captures(range);
        let start = captures
            .as_ref()
            .and_then(|found| found[2].parse::<usize>().ok());
        let end = captures
            .as_ref()
            .and_then(|found| found[3].parse::<usize>().ok());
        match (captures, start, end) {
            (Some(found), Some(start), Some(end))
                if start > 0 && end >= start && end <= 9_007_199_254_740_991 =>
            {
                repair_ranges.push(RepairRange {
                    document: found[1].to_owned(),
                    line_start: start,
                    line_end: end,
                })
            }
            _ => {
                return Err(argument(
                    "Use --repair-range <document>:<start>-<end> with positive, ordered line numbers.",
                ));
            }
        }
    }
    let repair = unique(
        parsed
            .repeated
            .get("repair")
            .into_iter()
            .flatten()
            .cloned()
            .chain(repair_ranges.iter().map(|range| range.document.clone())),
    );
    let values = &parsed.values;
    let options = Options {
        command,
        query,
        root: values
            .get("root")
            .cloned()
            .unwrap_or(std::env::current_dir()?.to_string_lossy().into_owned()),
        sources: parsed.repeated.get("source").cloned().unwrap_or_default(),
        base: values.get("base").cloned(),
        binary: values
            .get("codex")
            .cloned()
            .unwrap_or_else(|| "codex".to_owned()),
        deadline_ms: bounded(values.get("deadline-ms"), 100, 1_800_000)?.unwrap_or(1_800_000),
        limit: bounded(values.get("limit"), 1, 64)?.unwrap_or(24) as usize,
        max_calls: bounded(values.get("max-calls"), 0, 4096)?,
        max_input_bytes: bounded(values.get("max-input-bytes"), 1024, 1_073_741_824)?,
        max_context_bytes: bounded(values.get("max-context-bytes"), 1024, 262_144)?
            .unwrap_or(65_536) as usize,
        repair,
        repair_ranges,
        repair_reason: trim_js_whitespace(
            values.get("reason").map(String::as_str).unwrap_or_default(),
        )
        .to_owned(),
        retry_failed: parsed.flags.contains("retry-failed"),
        implementation: None,
        retrieval_query: None,
    };
    if (options.command == "review") != options.base.as_ref().is_some_and(|base| !base.is_empty()) {
        return Err(argument(
            "Use review <task> --base <git-ref>; --base is only for review.",
        ));
    }
    let invalid_reason =
        options.repair_reason.is_empty() || options.repair_reason.encode_utf16().count() > 2048;
    if if options.repair.is_empty() {
        !options.repair_reason.is_empty()
    } else {
        options.command != "update" || invalid_reason
    } {
        return Err(argument(
            "Use update --repair <document> or --repair-range <document>:<start>-<end> with --reason <correction up to 2048 characters>.",
        ));
    }
    Ok(options)
}

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
            entry.status = "historical".to_owned();
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
        &crate::knowledge_snapshot::stored_graph(&project.root)?,
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
                        .filter(|(document, version)| {
                            !is_current_source(project, document, version.as_deref())
                        })
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
        let is_relevant =
            source.is_none_or(|document| !document.historical) || relevant.contains(id);
        source.map(|document| document.hash.as_str())
            != graph.documents.get(id).and_then(Value::as_str)
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
            if !ids.contains(next) {
                if ids.len() >= limit {
                    pending.push(next.clone());
                } else {
                    ids.insert(next.clone());
                    queue.push(next.clone());
                }
            }
        }
    }
    pending.retain(|id| !ids.contains(id));
    (ids, unique(pending))
}

pub fn query_graph(project: &Project, options: &Options) -> Result<Value> {
    let AvailableGraph { graph, unavailable } = current_graph(project)?;
    let visible: Vec<&Document> = project
        .documents
        .iter()
        .filter(|document| !document.historical || options.sources.contains(&document.id))
        .collect();
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
        options.retrieval_query.as_deref().unwrap_or(&options.query),
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
            options.retrieval_query.as_deref().unwrap_or(&options.query),
            options.limit.min(6),
        )?
    };
    let document_ids: HashSet<_> = document_hits
        .iter()
        .cloned()
        .chain(options.sources.iter().cloned())
        .collect();
    let seeds = if options.command == "neighbors" {
        vec![options.query.clone()]
    } else {
        unique(
            hits.into_iter().chain(
                graph
                    .decisions
                    .iter()
                    .filter(|entry| {
                        visible_ids.contains(entry.document.as_str())
                            && document_ids.contains(&entry.document)
                    })
                    .map(|entry| entry.id.clone()),
            ),
        )
        .into_iter()
        .take(options.limit)
        .collect()
    };
    let (expanded, pending) = if ["neighbors", "ask", "review"].contains(&options.command.as_str())
    {
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
            edge.evidence
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
    "unexpandedDecisions":unique(pending.into_iter().chain(unavailable.iter().filter_map(|(from,to,_)| if expanded.contains(from) {Some(to.clone())} else if expanded.contains(to) {Some(from.clone())} else {None}))),
    "warnings":warnings
    }))
}

pub fn command(args: &[String]) -> Result<Value> {
    let mut options = options_for(args)?;
    let mut project = load_project(&options.root)?;
    if options
        .sources
        .iter()
        .chain(&options.repair)
        .any(|id| project.documents.iter().all(|document| document.id != *id))
    {
        return Err(HivexError::new(
            "SOURCE_NOT_FOUND",
            "An explicit source is not in the selected project documents",
        ));
    }
    if options.command == "update" {
        return Ok(crate::knowledge_update::update(&mut project, &options, None)?.0);
    }
    if options.command == "review" {
        let implementation = crate::implementation::capture_implementation(
            &project.root,
            options.base.as_deref().unwrap_or_default(),
        )?;
        let files = implementation["files"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let paths = files
            .iter()
            .filter_map(|file| file["path"].as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let new_lines = files
            .iter()
            .filter(|file| file["before"].is_null())
            .flat_map(|file| {
                file["after"]["lines"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|line| line[1].as_str())
            })
            .collect::<Vec<_>>()
            .join(" ");
        options.retrieval_query = Some(format!(
            "{} {} {} {}",
            options.query,
            paths,
            implementation["diff"].as_str().unwrap_or_default(),
            new_lines
        ));
        options.implementation = Some(implementation);
        return crate::consultation::ask(&mut project, &options);
    }
    if options.command == "ask" {
        return crate::consultation::ask(&mut project, &options);
    }
    if options.command == "status" {
        let graph = current_graph(&project)?.graph;
        let mut summary = json!(model::warning_summary(&graph.warnings, &project.documents));
        summary["sources"] = json!(project.warnings.len());
        let mut warnings: Vec<Value> = project
            .warnings
            .iter()
            .map(|warning| json!(warning))
            .collect();
        warnings.extend(
            model::active_warnings(&graph.warnings, &project.documents)
                .into_iter()
                .map(|warning| json!(warning)),
        );
        return Ok(json!({
        "availableDecisions":graph.decisions.len(),
        "availableRelationships":graph.relationships.len(),
        "command":"status",
        "pendingDocuments":pending_documents(&project,&graph,&project.current_documents.iter().map(|document| document.id.clone()).collect()),
        "selectedDocuments":project.documents.len(),
        "snapshot":project.snapshot,
        "uncheckedDecisions":graph.decisions.iter().filter(|entry| entry.quality != "checked").map(|entry| &entry.id).collect::<Vec<_>>(),
        "warningSummary":summary,
        "warnings":warnings
        }));
    }
    query_graph(&project, &options)
}
