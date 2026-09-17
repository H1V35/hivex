use crate::documents::markdown::{hash, line_content, raw_markdown_lines};
use crate::documents::{Document, Project, compare_serialized_strings};
use crate::error::{HivexError, Result};
use crate::execution::runtime::{self as model_runtime, OutputSchema, Request};
use crate::knowledge::ingestion::{self as ingestion, IngestionResult, IngestionUnit, RepairRange};
use crate::knowledge::model::{self as model, Citation, Graph, SuppliedDocument};
use crate::knowledge::search::{Record, rank_lexically};
use crate::knowledge::serialization::stringify_knowledge;
use crate::knowledge::warning_review::{
    self as warnings, ApplyWarningReviewOptions, ReviewContext,
};
use crate::knowledge::{historical_graph, is_current_source, knowledge_snapshot};
use crate::work::Operation as Options;
use crate::work::store::{BeginWork, Store, StoreOptions, Work};
use serde_json::{Value, json};
use std::collections::HashSet;

fn unique(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}
fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(str::to_owned))
        .collect()
}
fn overlaps(left: &Citation, right: &Citation) -> bool {
    left.document == right.document
        && left.line_start <= right.line_end
        && left.line_end >= right.line_start
}
fn unit_range(unit: &IngestionUnit) -> Citation {
    Citation {
        document: unit.document.clone(),
        line_start: unit.line_start,
        line_end: unit.line_end,
        version: None,
    }
}
fn decision_range(entry: &model::Decision) -> Citation {
    Citation {
        document: entry.document.clone(),
        line_start: entry.line_start,
        line_end: entry.line_end,
        version: Some(entry.version.clone()),
    }
}
fn affected(project: &Project, ranges: &[Citation], entry: &Citation) -> bool {
    ranges.iter().any(|range| range.document == entry.document)
        && (!is_current_source(project, &entry.document, entry.version.as_deref())
            || ranges.iter().any(|range| overlaps(range, entry)))
}
pub fn document_packet(project: &Project, ids: &[String]) -> Vec<Value> {
    project.documents.iter().filter(|document| ids.contains(&document.id)).map(|document| {
        let lines=raw_markdown_lines(&document.text);
        json!({
"historical":document.historical,
"id":document.id,
"lineCount":lines.len(),
"lines":lines.iter().enumerate().map(|(index,line)| json!([index+1,line_content(line)])).collect::<Vec<_>>(),
"status":document.status,
"title":document.title,
"version":document.hash
})
    }).collect()
}
pub fn document_excerpt(mut document: Value, ranges: &[Citation]) -> Value {
    let id = document["id"].as_str().unwrap_or_default().to_owned();
    if let Some(lines) = document["lines"].as_array_mut() {
        lines.retain(|line| {
            line[0].as_u64().is_some_and(|line| {
                ranges.iter().any(|range| {
                    range.document == id
                        && line >= range.line_start as u64
                        && line <= range.line_end as u64
                })
            })
        });
    }
    document
}
struct BatchContext {
    documents: Vec<Value>,
    existing: Vec<Value>,
    missing: Vec<String>,
    previous: Vec<model::Relationship>,
}
fn batch_context(
    project: &Project,
    graph: &Graph,
    units: &[IngestionUnit],
    retained_sources: &[String],
    context_sources: &[String],
) -> Result<BatchContext> {
    let mut ranges: Vec<_> = units.iter().map(unit_range).collect();
    let candidates: Vec<_> = graph
        .decisions
        .iter()
        .filter(|entry| {
            is_current_source(project, &entry.document, Some(&entry.version))
                && ranges
                    .iter()
                    .all(|range| !overlaps(range, &decision_range(entry)))
        })
        .collect();
    let hits = rank_lexically(
        &candidates
            .iter()
            .map(|entry| Record {
                id: entry.id.clone(),
                title: entry.document.clone(),
                content: format!("{} {}", entry.text, entry.reason),
            })
            .collect::<Vec<_>>(),
        &units
            .iter()
            .map(|unit| unit.text.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        12,
    )?;
    let targets: HashSet<_> = units.iter().map(|unit| unit.document.clone()).collect();
    let historical: HashSet<_> = project
        .historical_documents
        .iter()
        .map(|document| document.id.clone())
        .collect();
    let linked: HashSet<_> = project
        .documents
        .iter()
        .filter(|document| targets.contains(&document.id))
        .flat_map(|document| document.links.clone())
        .collect();
    let target_nodes: HashSet<_> = graph
        .decisions
        .iter()
        .filter(|entry| affected(project, &ranges, &decision_range(entry)))
        .map(|entry| entry.id.clone())
        .collect();
    let previous: Vec<_> = graph
        .relationships
        .iter()
        .filter(|edge| {
            target_nodes.contains(&edge.from)
                || target_nodes.contains(&edge.to)
                || edge
                    .evidence
                    .iter()
                    .any(|citation| affected(project, &ranges, citation))
        })
        .cloned()
        .collect();
    let references = previous
        .iter()
        .flat_map(|edge| edge.evidence.clone())
        .chain(retained_sources.iter().map(|document| Citation {
            document: document.clone(),
            line_start: 1,
            line_end: 1,
            version: None,
        }));
    let mut missing = Vec::new();
    for citation in references {
        if let Some(document) = project
            .documents
            .iter()
            .find(|source| source.id == citation.document)
        {
            if targets.contains(&citation.document)
                && citation.version.as_deref() != Some(&document.hash)
            {
                continue;
            }
            ranges.push(if citation.version.as_deref() == Some(&document.hash) {
                citation
            } else {
                Citation {
                    document: document.id.clone(),
                    line_start: 1,
                    line_end: raw_markdown_lines(&document.text).len(),
                    version: None,
                }
            });
        } else {
            missing.push(citation.document);
        }
    }
    ranges.extend(
        project
            .documents
            .iter()
            .filter(|document| context_sources.contains(&document.id))
            .map(|document| Citation {
                document: document.id.clone(),
                line_start: 1,
                line_end: raw_markdown_lines(&document.text).len(),
                version: None,
            }),
    );
    let required = unique(
        previous
            .iter()
            .flat_map(|edge| [edge.from.clone(), edge.to.clone()])
            .filter(|id| candidates.iter().any(|entry| entry.id == *id))
            .chain(
                candidates
                    .iter()
                    .filter(|entry| context_sources.contains(&entry.document))
                    .map(|entry| entry.id.clone()),
            ),
    );
    let allowed: HashSet<_> = candidates
        .iter()
        .filter(|entry| !historical.contains(&entry.document) || targets.contains(&entry.document))
        .map(|entry| entry.id.clone())
        .collect();
    let recent: Vec<_> = candidates
        .iter()
        .filter(|entry| allowed.contains(&entry.id))
        .collect();
    let priorities = unique(
        required
            .iter()
            .cloned()
            .chain(
                candidates
                    .iter()
                    .filter(|entry| {
                        linked.contains(&entry.document) && !historical.contains(&entry.document)
                    })
                    .map(|entry| entry.id.clone()),
            )
            .chain(hits.into_iter().filter(|id| allowed.contains(id)))
            .chain(
                recent
                    .iter()
                    .skip(recent.len().saturating_sub(6))
                    .map(|entry| entry.id.clone()),
            ),
    );
    let mut existing = Vec::new();
    let mut bytes = 0;
    for id in priorities {
        let Some(entry) = candidates.iter().find(|entry| entry.id == id) else {
            continue;
        };
        let evidence = model::source_evidence(&decision_range(entry), project);
        if required.contains(&id) && evidence.is_none() {
            missing.push(entry.document.clone());
        }
        if let Some(evidence) = evidence
            && (required.contains(&id)
                || (existing.len() < 18 && bytes + evidence.text.len() <= 8192))
        {
            bytes += evidence.text.len();
            let mut value = json!(entry);
            value.as_object_mut().unwrap().shift_remove("batch");
            existing.push(value);
            ranges.push(decision_range(entry));
        }
    }
    let documents = document_packet(
        project,
        &unique(ranges.iter().map(|range| range.document.clone())),
    )
    .into_iter()
    .map(|document| document_excerpt(document, &ranges))
    .collect();
    Ok(BatchContext {
        documents,
        existing,
        missing: unique(missing),
        previous,
    })
}

fn parse_range(id: &str) -> Option<RepairRange> {
    let (document, lines) = id.rsplit_once(':')?;
    let (start, end) = lines.split_once('-')?;
    let range = RepairRange {
        document: document.to_owned(),
        line_start: start.parse().ok()?,
        line_end: end.parse().ok()?,
    };
    (!document.is_empty()
        && range.line_start > 0
        && range.line_end >= range.line_start
        && range.line_end <= 9_007_199_254_740_991)
        .then_some(range)
}
fn repair_coverage(graph: &Graph, unit: &IngestionUnit, key: &str) -> bool {
    let mut spans: Vec<_> = graph
        .units
        .iter()
        .filter(|(id, covered)| {
            id.starts_with(&format!("{}:", unit.document))
                && covered["document"] == unit.document
                && covered["workKey"] == key
                && covered["version"] == graph.documents[&unit.document]
        })
        .filter_map(|(id, _)| parse_range(id))
        .collect();
    spans.sort_by_key(|span| span.line_start);
    let mut next = unit.line_start;
    for span in spans {
        if span.line_end < next {
            continue;
        }
        if span.line_start > next {
            return false;
        }
        next = span.line_end + 1;
        if next > unit.line_end {
            return true;
        }
    }
    false
}
fn replace_range_units(
    base: &[IngestionUnit],
    selected: &[IngestionUnit],
    sources: &HashSet<String>,
) -> Vec<IngestionUnit> {
    unique(base.iter().map(|unit| unit.document.clone()))
        .into_iter()
        .flat_map(|document| {
            if sources.contains(&document) {
                selected
            } else {
                base
            }
            .iter()
            .filter(move |unit| unit.document == document)
            .cloned()
        })
        .collect()
}
fn prepare_update(
    project: &mut Project,
    runtime: &Options,
    store: &mut Store,
    graph: &Graph,
    shared: Option<Work>,
) -> Result<(IngestionResult, Work)> {
    for range in &runtime.repair_ranges {
        let source = project
            .documents
            .iter()
            .find(|document| document.id == range.document);
        let Some(source) =
            source.filter(|source| range.line_end <= raw_markdown_lines(&source.text).len())
        else {
            return Err(HivexError::new(
                "INVALID_ARGUMENT",
                format!("Repair range is outside {}.", range.document),
            ));
        };
        if graph.documents.get(&source.id).and_then(Value::as_str) != Some(&source.hash) {
            return Err(HivexError::new(
                "SOURCE_NOT_CURRENT",
                format!(
                    "Finish updating {} before repairing selected ranges; its unchanged knowledge must be current.",
                    source.id
                ),
            ));
        }
    }
    let selected_history: Vec<_> = project
        .historical_documents
        .iter()
        .filter(|document| {
            runtime.repair.contains(&document.id)
                || shared.as_ref().is_some_and(|work| {
                    strings(&work.value()["plannedUnits"])
                        .iter()
                        .any(|id| id.starts_with(&format!("{}:", document.id)))
                })
        })
        .cloned()
        .collect();
    let documents: Vec<Document> = project
        .current_documents
        .iter()
        .cloned()
        .chain(selected_history.iter().cloned())
        .collect();
    let mut plan = ingestion::ingestion_units(&documents);
    let original = plan.units.clone();
    let range_sources: HashSet<_> = runtime
        .repair_ranges
        .iter()
        .map(|range| range.document.clone())
        .collect();
    if !range_sources.is_empty() {
        plan.units = replace_range_units(
            &original,
            &ingestion::repair_units(&documents, &graph.decisions, &runtime.repair_ranges)?,
            &range_sources,
        );
    }
    let context_sources = if runtime.command == "update" {
        runtime.sources.clone()
    } else {
        Vec::new()
    };
    let snapshot = knowledge_snapshot(
        project,
        &selected_history
            .iter()
            .map(|document| document.id.clone())
            .chain(context_sources.iter().cloned())
            .collect(),
    );
    project.warnings.extend(plan.warnings.clone());
    let mut identity = json!({"snapshot":snapshot,"model":runtime.execution.cache_identity(),"repair":runtime.repair,"reason":runtime.repair_reason,"format":3});
    if !runtime.repair_ranges.is_empty() {
        identity["repairRanges"] = json!(runtime.repair_ranges);
    }
    if !context_sources.is_empty() {
        let mut sources = unique(context_sources);
        sources.sort_by(|a, b| compare_serialized_strings(a, b));
        identity["contextSources"] = json!(sources);
    }
    let key = hash(&identity.to_string());
    let scoped = shared
        .as_ref()
        .map(|work| strings(&work.value()["plannedUnits"]));
    let mut remaining: Vec<_> = plan
        .units
        .iter()
        .filter(|unit| {
            if scoped.as_ref().is_some_and(|ids| !ids.contains(&unit.id)) {
                return false;
            }
            if !runtime.repair.is_empty() {
                let ranges: Vec<_> = runtime
                    .repair_ranges
                    .iter()
                    .filter(|range| range.document == unit.document)
                    .collect();
                let selected = ranges.is_empty()
                    || ranges.iter().any(|range| {
                        range.line_start <= unit.line_end && range.line_end >= unit.line_start
                    });
                runtime.repair.contains(&unit.document)
                    && selected
                    && !(if range_sources.contains(&unit.document) {
                        repair_coverage(graph, unit, &key)
                    } else {
                        graph
                            .units
                            .get(&unit.id)
                            .is_some_and(|covered| covered["workKey"] == key)
                    })
            } else {
                graph
                    .units
                    .get(&unit.id)
                    .and_then(|covered| covered["version"].as_str())
                    != project
                        .documents
                        .iter()
                        .find(|source| source.id == unit.document)
                        .map(|source| source.hash.as_str())
            }
        })
        .map(|unit| unit.id.clone())
        .collect();
    let mut work = match shared {
        Some(work) => work,
        None => store.begin_with_profile(
            BeginWork {
                key,
                kind: "update".to_owned(),
                max_calls: runtime.max_calls,
                max_input_bytes: runtime.max_input_bytes,
                remaining: remaining.clone(),
                result_key: None,
                snapshot,
                warning_baseline: Some(json!(warnings::warning_baseline(graph))),
            },
            runtime.execution.binding(&identity),
        )?,
    };
    if !range_sources.is_empty() && work.status() != crate::work::State::Done {
        if work.calls() == 0 && work.value()["pending"].is_null() {
            work.set_plan(&remaining)?;
        } else {
            let retained = strings(&work.value()["plannedUnits"])
                .iter()
                .map(|id| {
                    let range = parse_range(id).ok_or_else(|| {
                        HivexError::new("INVALID_ARGUMENT", "Invalid retained repair range")
                    })?;
                    let document = documents
                        .iter()
                        .find(|document| document.id == range.document)
                        .ok_or_else(|| {
                            HivexError::new(
                                "STALE_RETAINED_CHECK",
                                "A retained repair source is unavailable.",
                            )
                        })?;
                    ingestion::unit_from_range(document, &range)
                })
                .collect::<Result<Vec<_>>>()?;
            plan.units = replace_range_units(&original, &retained, &range_sources);
            remaining = work.remaining();
        }
        ingestion::validate_repair_unit_size(
            &plan
                .units
                .iter()
                .filter(|unit| remaining.contains(&unit.id))
                .cloned()
                .collect::<Vec<_>>(),
        )?;
    }
    let pending = &work.value()["pending"];
    let changed = if pending["staged"] == true {
        json!(graph.last_extraction) != pending["baseExtraction"]
    } else {
        graph.last_extraction.as_deref() != pending["batch"].as_str()
    };
    if remaining.iter().any(|id| !work.remaining().contains(id)) || changed {
        if runtime.retry_failed
            && work.status() == crate::work::State::Failed
            && work
                .attempts()
                .and_then(|attempts| attempts.last())
                .is_some_and(|attempt| attempt["error"] == "RELATIONSHIP_LOSS")
        {
            return Err(HivexError::new(
                "STALE_RETAINED_CHECK",
                "The graph changed after the retained check. No model call was made.",
            ));
        }
        work.set_pending(Value::Null);
    }
    work.set_remaining(remaining);
    store.save(&mut work)?;
    if work.retry_failed(runtime.retry_failed)? {
        store.save(&mut work)?;
    }
    Ok((plan, work))
}

fn pending_current(project: &Project, pending: &Value) -> bool {
    if pending.is_null() {
        return false;
    }
    pending["packet"]["documents"]
        .as_array()
        .is_some_and(|documents| {
            documents.iter().all(|source| {
                source["id"]
                    .as_str()
                    .zip(source["version"].as_str())
                    .is_some_and(|(id, version)| is_current_source(project, id, Some(version)))
            })
        })
}
fn supplied_documents(pending: &Value) -> Result<Vec<SuppliedDocument>> {
    serde_json::from_value(pending["packet"]["documents"].clone()).map_err(Into::into)
}
fn materialize(
    project: &Project,
    graph: &Graph,
    plan: &IngestionResult,
    pending: &Value,
    retain_supplied: bool,
) -> Result<Graph> {
    let supplied = supplied_documents(pending)?;
    let ids = strings(&pending["units"]);
    let targets: Vec<_> = plan
        .units
        .iter()
        .filter(|unit| ids.contains(&unit.id))
        .map(unit_range)
        .collect();
    let previous_ids: HashSet<_> = pending["packet"]["previousRelationships"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|edge| [edge["from"].as_str(), edge["to"].as_str()])
        .flatten()
        .collect();
    let current = graph
        .decisions
        .iter()
        .filter(|entry| {
            previous_ids.contains(entry.id.as_str())
                && is_current_source(project, &entry.document, Some(&entry.version))
                && model::valid_citation(&decision_range(entry), &project.documents)
                && model::supplied_citation(&decision_range(entry), &supplied)
        })
        .map(|entry| entry.id.clone());
    let original = strings(&pending["existing"]);
    let existing = if retain_supplied {
        unique(original.into_iter().chain(current))
    } else {
        original
            .into_iter()
            .filter(|id| {
                graph
                    .decisions
                    .iter()
                    .find(|entry| entry.id == *id)
                    .is_none_or(|entry| {
                        targets
                            .iter()
                            .all(|target| !overlaps(target, &decision_range(entry)))
                    })
            })
            .collect()
    };
    let context_ids = strings(&pending["context"]);
    let document_ids = strings(&pending["documents"]);
    let context: Vec<_> = project
        .documents
        .iter()
        .filter(|document| context_ids.contains(&document.id))
        .cloned()
        .collect();
    let documents: Vec<_> = project
        .documents
        .iter()
        .filter(|document| document_ids.contains(&document.id))
        .cloned()
        .collect();
    let context_ranges: Vec<_> = supplied
        .iter()
        .flat_map(|document| {
            document
                .lines
                .iter()
                .filter_map(|line| line.first().and_then(Value::as_u64))
                .map(|line| Citation {
                    document: document.id.clone(),
                    line_start: line as usize,
                    line_end: line as usize,
                    version: None,
                })
        })
        .collect();
    let extraction = model::parse_extraction(&pending["extraction"])
        .ok_or_else(|| HivexError::new("READ_FAILED", "Invalid pending extraction"))?;
    Ok(model::apply_extraction(model::ExtractionOptions {
        batch: pending["batch"].as_str().unwrap_or_default(),
        context_documents: Some(&context),
        context_ranges: Some(&context_ranges),
        documents: &documents,
        existing_ids: Some(&existing),
        extraction: &extraction,
        graph,
        target_ranges: Some(&targets),
    }))
}
fn without_execution(mut value: Value) -> Value {
    if let Some(record) = value.as_object_mut() {
        for field in ["batch", "quality", "localId"] {
            record.shift_remove(field);
        }
    }
    value
}
fn checked_packet(
    graph: &Graph,
    candidate: &Graph,
    pending: &Value,
    include_referenced: bool,
) -> Value {
    let mut packet = pending["packet"].clone();
    if pending["materializedCheck"] != true {
        packet["extraction"] = pending["extraction"].clone();
        return packet;
    }
    let protected = strings(&pending["protectedRelationships"]);
    let existing = strings(&pending["existing"]);
    let batch = pending["batch"].as_str().unwrap_or_default();
    let removed: Vec<_> = graph
        .relationships
        .iter()
        .filter(|edge| protected.contains(&edge.id))
        .collect();
    let mut endpoints: HashSet<_> = removed
        .iter()
        .flat_map(|edge| [edge.from.clone(), edge.to.clone()])
        .collect();
    if include_referenced {
        endpoints.extend(
            candidate
                .relationships
                .iter()
                .filter(|edge| edge.batch == batch)
                .flat_map(|edge| [edge.from.clone(), edge.to.clone()])
                .filter(|id| !existing.contains(id)),
        );
    }
    let ranges: Vec<Citation> =
        serde_json::from_value(pending["packet"]["units"].clone()).unwrap_or_default();
    let validation: Vec<_> = candidate
        .warnings
        .iter()
        .filter(|warning| match warning {
            model::Warning::Structured(warning) => {
                warning.kind.as_deref() == Some("validation")
                    && warning.scope.iter().any(|scope| {
                        ranges.iter().any(|range| {
                            range.document == scope.document
                                && range.line_start <= scope.line_end
                                && range.line_end >= scope.line_start
                        })
                    })
            }
            _ => false,
        })
        .collect();
    packet["extraction"] = json!({
    "decisions":candidate.decisions.iter().filter(|entry|entry.batch==batch).map(|entry|without_execution(json!(entry))).collect::<Vec<_>>(),
    "relationships":candidate.relationships.iter().filter(|entry|entry.batch==batch).map(|entry|without_execution(json!(entry))).collect::<Vec<_>>(),
    "uncertainties":pending["extraction"]["uncertainties"]
    });
    packet["previousDecisions"] = json!(
        graph
            .decisions
            .iter()
            .filter(|entry| endpoints.contains(&entry.id))
            .map(|entry| without_execution(json!(entry)))
            .collect::<Vec<_>>()
    );
    packet["removedRelationships"] = json!(
        removed
            .iter()
            .map(|edge| without_execution(json!(edge)))
            .collect::<Vec<_>>()
    );
    packet["validationWarnings"] = json!(validation);
    packet
}
fn finish_round(
    project: &Project,
    graph: &mut Graph,
    plan: &IngestionResult,
    work: &mut Work,
    ids: &[String],
) {
    work.set_remaining(
        work.remaining()
            .into_iter()
            .filter(|id| !ids.contains(id))
            .collect::<Vec<_>>(),
    );
    for unit in plan.units.iter().filter(|unit| ids.contains(&unit.id)) {
        let Some(source) = project
            .documents
            .iter()
            .find(|document| document.id == unit.document)
        else {
            continue;
        };
        for (id, coverage) in &mut graph.units {
            if coverage["document"] == unit.document
                && coverage["workKey"] != work.key()
                && parse_range(id).is_some_and(|range| {
                    range.line_start <= unit.line_end && range.line_end >= unit.line_start
                })
                && let Some(record) = coverage.as_object_mut()
            {
                record.shift_remove("workKey");
            }
        }
        graph.units.insert(
            unit.id.clone(),
            json!({"document":source.id,"version":source.hash,"workKey":work.key()}),
        );
    }
    for source in &project.documents {
        let units: Vec<_> = plan
            .units
            .iter()
            .filter(|unit| unit.document == source.id)
            .collect();
        if !units.is_empty()
            && units.iter().all(|unit| {
                graph
                    .units
                    .get(&unit.id)
                    .is_some_and(|coverage| coverage["version"] == source.hash)
            })
            && plan
                .warnings
                .iter()
                .all(|warning| warning.path != source.path)
        {
            graph
                .documents
                .insert(source.id.clone(), json!(source.hash));
        }
    }
    work.finish_round();
}

fn extract_batch(
    project: &Project,
    graph: &Graph,
    plan: &IngestionResult,
    runtime: &Options,
    store: &mut Store,
    work: &mut Work,
) -> Result<Option<Graph>> {
    let mut units = Vec::new();
    let mut bytes = 0;
    for unit in plan
        .units
        .iter()
        .filter(|unit| work.remaining().contains(&unit.id))
    {
        if units.len() == 4 || bytes + unit.text.len() > 16_384 {
            break;
        }
        bytes += unit.text.len();
        units.push(unit.clone());
    }
    let documents = unique(units.iter().map(|unit| unit.document.clone()));
    let context = batch_context(
        project,
        graph,
        &units,
        &strings(&work.value()["pending"]["context"]),
        if runtime.command == "update" {
            &runtime.sources
        } else {
            &[]
        },
    )?;
    let packet = json!({
    "documents":context.documents,
    "existing":context.existing,
    "operation":"extract",
    "previousRelationships":context.previous,
    "repairReason":runtime.repair_reason,
    "scope":"Only the target line ranges are being ingested. Selected neighbors are context, not exhaustive coverage. Preserve uncertainty when conditions may lie outside these excerpts.",
    "targets":documents,
    "units":units.iter().map(|unit|{let mut value=json!(unit);value.as_object_mut().unwrap().shift_remove("text");value}).collect::<Vec<_>>()
    });
    let required = stringify_knowledge(&packet).len();
    if !context.missing.is_empty() || required > runtime.max_context_bytes {
        work.limit_context(
            unique(
                context.missing.into_iter().chain(
                    context
                        .documents
                        .iter()
                        .filter_map(|document| document["id"].as_str().map(str::to_owned)),
                ),
            ),
            runtime.max_context_bytes,
            required,
        );
        store.save(work)?;
        return Ok(None);
    }
    work.clear_context_limit();
    let mut instruction="For a repair, check repairReason against Markdown; it is not new authority. Extract meaningful decisions, constraints, definitions and lessons, not every sentence or incidental numeric value. Use c1,c2,... decision IDs and r1,r2,... relationship IDs. Discover supported semantic relationships even without authored links. Extract decisions only within the target unit line ranges. Other ranges are context; do not duplicate their decisions. Existing decision IDs may be relationship endpoints. Cite each decision in its own document and relationships in the documents supporting their scope.".to_owned();
    if work.value().get("warningBaseline").is_some() {
        instruction.push_str(" Report uncertainties only when an unanswered choice, contradiction or missing condition affects the interpretation or application of a decision, dependency or exception. Explain that consequence. Missing deployment proof, incidental detail or background alone is not a warning. Preserve genuine uncertainty and do not infer answers from absence. Keep descriptive facts descriptive; do not turn a current setup into a permanent obligation. For A requires B, A is the dependent and B the prerequisite. Each decision citation must support its conditions and exceptions too.");
    }
    let request = Request {
        instruction,
        packet: packet.clone(),
        schema: OutputSchema::Extraction,
        stage: "extract".to_owned(),
    };
    let Some(mut extraction) = model_runtime::run_model(work, store, &runtime.execution, &request)?
    else {
        return Ok(None);
    };
    if let Some(decisions) = extraction["decisions"].as_array_mut() {
        for entry in decisions {
            if project
                .historical_documents
                .iter()
                .any(|document| entry["document"] == document.id)
            {
                entry["status"] = json!("historical");
            }
        }
    }
    let mut check_packet = packet.clone();
    check_packet["operation"] = json!("check");
    let mut pending = json!({
    "baseExtraction":graph.last_extraction,
    "batch":format!("{}:{}",work.id(),hash(&stringify_knowledge(&packet))),
    "context":context.documents.iter().map(|document|document["id"].clone()).collect::<Vec<_>>(),
    "documents":documents,
    "existing":context.existing.iter().map(|entry|entry["id"].clone()).collect::<Vec<_>>(),
    "extraction":extraction,
    "materializedCheck":work.value()["materializedChecks"]==true,
    "packet":check_packet,
    "units":units.iter().map(|unit|&unit.id).collect::<Vec<_>>()
    });
    let candidate = materialize(project, graph, plan, &pending, true)?;
    let protected: Vec<_> = if work.value()["materializedChecks"] == true {
        graph
            .relationships
            .iter()
            .filter(|edge| {
                !candidate
                    .relationships
                    .iter()
                    .any(|candidate| candidate.id == edge.id)
                    && [&edge.from, &edge.to].iter().all(|id| {
                        graph.decisions.iter().any(|node| {
                            node.id == **id
                                && is_current_source(project, &node.document, Some(&node.version))
                        })
                    })
                    && edge.evidence.iter().all(|citation| {
                        is_current_source(project, &citation.document, citation.version.as_deref())
                    })
            })
            .map(|edge| edge.id.clone())
            .collect()
    } else {
        Vec::new()
    };
    pending["staged"] = json!(!protected.is_empty());
    pending["protectedRelationships"] = json!(protected);
    if work.value().get("warningBaseline").is_some() {
        let supplied = supplied_documents(&pending)?;
        let uncertainties = strings(&extraction["uncertainties"]);
        let mut sized = checked_packet(graph, &candidate, &pending, true);
        sized["warningCandidates"] = json!([]);
        let available = runtime
            .max_context_bytes
            .saturating_sub(stringify_knowledge(&sized).len());
        pending["packet"]["warningCandidates"] = json!(warnings::warning_review_candidates(
            &candidate,
            &ReviewContext {
                documents: &project.documents,
                supplied: &supplied,
                uncertainties: &uncertainties
            },
            Some(available)
        ));
    }
    let staged = pending["staged"] == true;
    work.set_pending(pending);
    if staged {
        store.save(work)?;
        Ok(Some(graph.clone()))
    } else {
        store.commit(work, &model::graph_value(&candidate, false))?;
        Ok(Some(candidate))
    }
}

fn check_request(graph: &Graph, candidate: &Graph, pending: &Value) -> Request {
    let guarded = !strings(&pending["protectedRelationships"]).is_empty();
    let warning_review = pending["packet"].get("warningCandidates").is_some();
    let mut instruction="Check this batch once against the Markdown. Identify important omitted decisions, distorted scope, or invented relationships. Target a decision ID, relationship ID, document ID, or batch. Report concrete issues only; do not enumerate every node, re-extract the documents or invent certainty.".to_owned();
    if pending["materializedCheck"] == true {
        instruction.push_str(" The extraction is the materialized candidate, after local validation. Check meaningful decisions, dependencies and exceptions; reading the cited Markdown supplies incidental details. Missing live deployment evidence or unexpanded background alone is not a defect. Use the supplied canonical IDs.");
    }
    if guarded {
        instruction.push_str(" For each removedRelationships entry, justify its replacement or removal in relationshipChanges using current Markdown evidence. List canonical replacement relationship IDs, or an empty list only for a supported removal. If the loss is unjustified, report a finding and omit its resolution. Do not approve missing dependencies merely because the candidate omitted them.");
    }
    if warning_review {
        instruction.push_str(warnings::WARNING_REVIEW_INSTRUCTION);
    }
    Request {
        instruction,
        packet: checked_packet(graph, candidate, pending, true),
        schema: OutputSchema::Check {
            relationships: guarded,
            warnings: warning_review,
        },
        stage: "check".to_owned(),
    }
}
fn unresolved_changes(
    project: &Project,
    candidate: &Graph,
    plan: &IngestionResult,
    pending: &Value,
    value: &Value,
) -> Result<Vec<String>> {
    let protected = strings(&pending["protectedRelationships"]);
    if protected.is_empty() {
        return Ok(protected);
    }
    let supplied = supplied_documents(pending)?;
    let selected = strings(&pending["units"]);
    let ranges: Vec<_> = plan
        .units
        .iter()
        .filter(|unit| selected.contains(&unit.id))
        .map(unit_range)
        .collect();
    let check = model::parse_check(value)
        .ok_or_else(|| HivexError::new("READ_FAILED", "Invalid knowledge check"))?;
    let batch = pending["batch"].as_str().unwrap_or_default();
    let impact = model::check_impact(
        candidate,
        &check,
        batch,
        &model::warning_scope(&project.documents, Some(&ranges)),
    );
    let supported: HashSet<_> = candidate
        .decisions
        .iter()
        .filter(|node| {
            !(node.batch == batch && node.quality == "uncertain")
                && model::valid_citation(&decision_range(node), &project.documents)
                && is_current_source(project, &node.document, Some(&node.version))
        })
        .map(|node| node.id.as_str())
        .collect();
    let available: HashSet<_> = candidate
        .relationships
        .iter()
        .filter(|edge| {
            supported.contains(edge.from.as_str())
                && supported.contains(edge.to.as_str())
                && !impact.relationship_ids.contains(&edge.id)
                && edge.evidence.iter().all(|citation| {
                    model::valid_citation(citation, &project.documents)
                        && is_current_source(
                            project,
                            &citation.document,
                            citation.version.as_deref(),
                        )
                })
        })
        .map(|edge| edge.id.as_str())
        .collect();
    let justified: HashSet<_> = value["relationshipChanges"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|change| {
            let citations: Option<Vec<Citation>> =
                serde_json::from_value(change["evidence"].clone()).ok();
            !impact.is_uncertain_batch
                && check.findings.iter().all(|finding| {
                    finding.target != "batch" && change["previousId"] != finding.target
                })
                && strings(&change["replacements"])
                    .iter()
                    .all(|id| available.contains(id.as_str()))
                && citations.is_some_and(|citations| {
                    citations.iter().all(|citation| {
                        model::valid_citation(citation, &project.documents)
                            && model::supplied_citation(citation, &supplied)
                    })
                })
        })
        .filter_map(|change| change["previousId"].as_str())
        .collect();
    Ok(protected
        .into_iter()
        .filter(|id| !justified.contains(id.as_str()))
        .collect())
}
fn check_batch(
    project: &Project,
    graph: &Graph,
    plan: &IngestionResult,
    runtime: &Options,
    store: &mut Store,
    work: &mut Work,
    retained_only: bool,
) -> Result<Option<Graph>> {
    let pending = work.value()["pending"].clone();
    let candidate = if pending["staged"] == true {
        materialize(project, graph, plan, &pending, true)?
    } else {
        graph.clone()
    };
    let request = check_request(graph, &candidate, &pending);
    let prior_calls = work.calls();
    let value = if retained_only {
        match model_runtime::retained_check_result(work, &request, &runtime.execution) {
            Ok(value) => Some(value),
            Err(error) if error.code == "STALE_RETAINED_CHECK" => {
                let legacy = materialize(project, graph, plan, &pending, false)?;
                let legacy_request = Request {
                    instruction: request.instruction.clone(),
                    packet: checked_packet(graph, &legacy, &pending, false),
                    schema: request.schema,
                    stage: request.stage.clone(),
                };
                model_runtime::retained_check_result(work, &legacy_request, &runtime.execution)?;
                model_runtime::run_model(work, store, &runtime.execution, &request)?
            }
            Err(error) => return Err(error),
        }
    } else {
        model_runtime::run_model(work, store, &runtime.execution, &request)?
    };
    let Some(value) = value else {
        return Ok(None);
    };
    let ids = strings(&pending["units"]);
    let ranges: Vec<_> = plan
        .units
        .iter()
        .filter(|unit| ids.contains(&unit.id))
        .map(unit_range)
        .collect();
    let check = model::parse_check(&value)
        .ok_or_else(|| HivexError::new("READ_FAILED", "Invalid knowledge check"))?;
    let mut checked = model::apply_check(
        &candidate,
        &check,
        pending["batch"].as_str().unwrap_or_default(),
        &model::warning_scope(&project.documents, Some(&ranges)),
    );
    let unresolved = unresolved_changes(project, &candidate, plan, &pending, &value)?;
    let local = retained_only && work.calls() == prior_calls;
    if local {
        work.assess_retained_check(unresolved.is_empty());
    }
    if !unresolved.is_empty() {
        work.reject_check(!local,format!("Previous graph retained. Unresolved relationship changes: {}. Inspect this result before proposing a different repair; no automatic retry.",unresolved.join(", ")));
        store.save(work)?;
        return Ok(Some(graph.clone()));
    }

    let documents = strings(&pending["documents"]);
    let validation = candidate.warnings.iter().any(|warning| match warning {
        model::Warning::Structured(warning) => {
            warning.kind.as_deref() == Some("validation")
                && warning
                    .scope
                    .iter()
                    .any(|scope| documents.contains(&scope.document))
        }
        _ => false,
    });
    if !validation
        && pending["packet"].get("warningCandidates").is_some()
        && check.findings.is_empty()
    {
        let supplied = supplied_documents(&pending)?;
        let uncertainties = strings(&pending["extraction"]["uncertainties"]);
        let offered: HashSet<_> = pending["packet"]["warningCandidates"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|candidate| candidate["id"].as_str())
            .collect();
        let candidates: Vec<_> = warnings::warning_review_candidates(
            &candidate,
            &ReviewContext {
                documents: &project.documents,
                supplied: &supplied,
                uncertainties: &uncertainties,
            },
            None,
        )
        .into_iter()
        .filter(|candidate| offered.contains(candidate.id.as_str()))
        .collect();
        let resolutions = warnings::parse_warning_resolutions(
            value.get("warningResolutions").unwrap_or(&json!([])),
        )
        .ok_or_else(|| HivexError::new("READ_FAILED", "Invalid warning resolutions"))?;
        checked = warnings::apply_warning_review(
            &checked,
            ApplyWarningReviewOptions {
                candidates: &candidates,
                documents: &project.documents,
                resolutions: &resolutions,
                supplied: &supplied,
            },
        );
    }
    work.accept_check();
    finish_round(project, &mut checked, plan, work, &ids);
    store.commit(work, &model::graph_value(&checked, false))?;
    Ok(Some(checked))
}
fn update_response(
    project: &Project,
    graph: &Graph,
    units: &[IngestionUnit],
    work: &Work,
    runtime: &Options,
) -> Value {
    let summary = model::warning_summary(&graph.warnings, &project.documents);
    let status = if work.status() == crate::work::State::Done {
        if summary.findings + summary.validation + summary.unknown + project.warnings.len() > 0 {
            "partial"
        } else {
            "ready"
        }
    } else {
        work.status().as_str()
    };
    let coverage = if !project.warnings.is_empty()
        || units.iter().any(|unit| {
            graph.documents.get(&unit.document).and_then(Value::as_str)
                != project
                    .documents
                    .iter()
                    .find(|document| document.id == unit.document)
                    .map(|document| document.hash.as_str())
        }) {
        "pending"
    } else {
        "current"
    };
    let mut warning_summary = json!(summary);
    warning_summary["sources"] = json!(project.warnings.len());
    let mut all_warnings: Vec<_> = project
        .warnings
        .iter()
        .map(|warning| json!(warning))
        .collect();
    all_warnings.extend(
        model::active_warnings(&graph.warnings, &project.documents)
            .iter()
            .map(|warning| json!(warning)),
    );
    let mut response = json!({
    "command":"update",
    "coverage":coverage,
    "decisions":graph.decisions.len(),
    "model":runtime.execution.model_summary(),
    "pendingCheck":strings(&work.value()["pending"]["documents"]),
    "pendingDocuments":unique(units.iter().filter(|unit|work.remaining().contains(&unit.id)).map(|unit|unit.document.clone())),
    "pendingUnits":work.remaining(),
    "relationshipCoverage":"Bounded authored, lexical and recent neighbors; not an exhaustive comparison of all decisions.",
    "relationships":graph.relationships.len(),
    "snapshot":project.snapshot,
    "status":status,
    "warningSummary":warning_summary,
    "warnings":all_warnings,
    "work":model_runtime::work_summary(work)
    });
    if let Some(baseline) = work.value()["warningBaseline"].as_object() {
        response["warningChanges"] = json!(warnings::warning_changes(
            graph,
            &project.documents,
            baseline
        ));
    }
    response
}

pub fn update_with_store(
    project: &mut Project,
    runtime: &Options,
    store: &mut Store,
    shared: Option<Work>,
) -> Result<(Value, Work)> {
    let parsed = model::parse_graph(&store.graph()?, false)
        .ok_or_else(|| HivexError::new("READ_FAILED", "Invalid stored graph"))?;
    let mut graph = historical_graph(project, parsed);
    let current: HashSet<_> = project
        .documents
        .iter()
        .map(|document| document.id.as_str())
        .collect();
    graph
        .documents
        .retain(|id, _| current.contains(id.as_str()));
    graph.units.retain(|_, unit| {
        unit["document"]
            .as_str()
            .is_some_and(|id| current.contains(id))
    });
    let (plan, mut work) = prepare_update(project, runtime, store, &graph, shared)?;
    let reassess = runtime.retry_failed
        && work.status() == crate::work::State::Failed
        && work
            .attempts()
            .and_then(|attempts| attempts.last())
            .is_some_and(|attempt| attempt["error"] == "RELATIONSHIP_LOSS");
    if reassess {
        if !pending_current(project, &work.value()["pending"]) {
            return Err(HivexError::new(
                "STALE_RETAINED_CHECK",
                "The evidence changed after the retained check. No model call was made.",
            ));
        }
        graph =
            check_batch(project, &graph, &plan, runtime, store, &mut work, true)?.unwrap_or(graph);
        return Ok((
            update_response(project, &graph, &plan.units, &work, runtime),
            work,
        ));
    }
    if [crate::work::State::Done, crate::work::State::Failed].contains(&work.status()) {
        return Ok((
            update_response(project, &graph, &plan.units, &work, runtime),
            work,
        ));
    }
    while !work.remaining().is_empty() || !work.value()["pending"].is_null() {
        if !pending_current(project, &work.value()["pending"]) {
            let Some(extracted) = extract_batch(project, &graph, &plan, runtime, store, &mut work)?
            else {
                break;
            };
            graph = extracted;
        }
        let Some(checked) = check_batch(project, &graph, &plan, runtime, store, &mut work, false)?
        else {
            break;
        };
        graph = checked;
        if work.status() == crate::work::State::Failed {
            break;
        }
    }
    if work.remaining().is_empty() && work.value()["pending"].is_null() {
        finish_round(project, &mut graph, &plan, &mut work, &[]);
        store.commit(&mut work, &model::graph_value(&graph, false))?;
    }
    Ok((
        update_response(project, &graph, &plan.units, &work, runtime),
        work,
    ))
}

pub fn update(
    project: &mut Project,
    runtime: &Options,
    shared: Option<Work>,
) -> Result<(Value, Work)> {
    let mut store = Store::open(
        &project.root,
        StoreOptions {
            readonly: false,
            update: true,
        },
    )?;
    update_with_store(project, runtime, &mut store, shared)
}
