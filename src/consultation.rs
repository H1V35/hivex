use crate::documents::markdown::hash;
use crate::documents::{Project, compare_serialized_strings};
use crate::error::{HivexError, Result};
use crate::execution::runtime::{self as model_runtime, OutputSchema, Request};
use crate::knowledge;
use crate::knowledge::ingestion::ingestion_units;
use crate::knowledge::model::{self as model, Citation, SuppliedDocument};
use crate::knowledge::search::{Record, rank_lexically};
use crate::knowledge::serialization::stringify_knowledge;
use crate::knowledge::update::{self as update, document_excerpt, document_packet};
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
fn array(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}
fn context_documents(context: &Value) -> Vec<String> {
    unique(
        strings(&context["unavailableDocuments"])
            .into_iter()
            .chain(
                array(&context["decisions"])
                    .iter()
                    .filter_map(|entry| entry["document"].as_str().map(str::to_owned)),
            )
            .chain(array(&context["relationships"]).iter().flat_map(|edge| {
                array(&edge["evidence"])
                    .iter()
                    .filter_map(|citation| citation["document"].as_str().map(str::to_owned))
            }))
            .chain(
                array(&context["documents"])
                    .iter()
                    .filter_map(|source| source["id"].as_str().map(str::to_owned)),
            ),
    )
}
fn answer_packet(
    project: &Project,
    runtime: &Options,
    context: Value,
    documents: &[String],
) -> Result<Value> {
    let selected_sources = project
        .documents
        .iter()
        .filter(|document| documents.contains(&document.id))
        .cloned()
        .collect::<Vec<_>>();
    let plan = ingestion_units(&selected_sources);
    let query = runtime.retrieval_query.as_deref().unwrap_or(&runtime.query);
    let hits = rank_lexically(
        &plan
            .units
            .iter()
            .map(|unit| Record {
                id: unit.id.clone(),
                title: unit.document.clone(),
                content: unit.text.clone(),
            })
            .collect::<Vec<_>>(),
        query,
        plan.units.len(),
    )?;
    let ids = unique(
        hits.into_iter()
            .chain(plan.units.iter().map(|unit| unit.id.clone())),
    );
    let originals = document_packet(project, documents);
    let mut selected = Vec::new();
    // Preserve the reference object's insertion order before the context byte check.
    let mut packet = json!({"context":context,"documents":[]});
    if let Some(implementation) = &runtime.implementation {
        packet["implementation"] = implementation.clone();
    }
    packet["omittedUnits"] = json!(plan.units.len());
    packet["operation"] = json!(runtime.command);
    packet["task"] = json!(runtime.query);
    packet["warnings"] = json!(plan.warnings);
    for id in ids {
        let Some(unit) = plan.units.iter().find(|unit| unit.id == id) else {
            continue;
        };
        let range = Citation {
            document: unit.document.clone(),
            line_start: unit.line_start,
            line_end: unit.line_end,
            version: None,
        };
        let mut proposed = selected.clone();
        proposed.push(range);
        let excerpts: Vec<_> = originals
            .iter()
            .cloned()
            .map(|document| document_excerpt(document, &proposed))
            .filter(|document| !array(&document["lines"]).is_empty())
            .collect();
        let mut candidate = packet.clone();
        candidate["documents"] = json!(excerpts);
        if candidate.to_string().len() <= runtime.max_context_bytes {
            selected = proposed;
            packet["documents"] = candidate["documents"].clone();
        }
    }
    packet["omittedUnits"] = json!(plan.units.len() - selected.len());
    Ok(packet)
}
fn begin_consultation(
    project: &Project,
    runtime: &Options,
    store: &mut Store,
    documents: &[String],
    packet: &Value,
) -> Result<Work> {
    let graph = model::parse_graph(&store.graph()?, false)
        .ok_or_else(|| HivexError::new("READ_FAILED", "Invalid stored graph"))?;
    let relevant: HashSet<_> = documents.iter().chain(&runtime.sources).cloned().collect();
    let units: Vec<_> = ingestion_units(&project.documents)
        .units
        .into_iter()
        .filter(|unit| {
            project.documents.iter().any(|source| {
                source.id == unit.document && (!source.historical || relevant.contains(&source.id))
            })
        })
        .collect();
    let changed: Vec<_> = units
        .iter()
        .filter(|unit| {
            graph
                .units
                .get(&unit.id)
                .and_then(|coverage| coverage["version"].as_str())
                != project
                    .documents
                    .iter()
                    .find(|source| source.id == unit.document)
                    .map(|source| source.hash.as_str())
        })
        .collect();
    let unavailable = strings(&packet["context"]["unavailableDocuments"]);
    let hits = rank_lexically(
        &changed
            .iter()
            .map(|unit| Record {
                id: unit.id.clone(),
                title: unit.document.clone(),
                content: unit.text.clone(),
            })
            .collect::<Vec<_>>(),
        runtime.retrieval_query.as_deref().unwrap_or(&runtime.query),
        64,
    )?;
    let order = unique(
        changed
            .iter()
            .filter(|unit| unavailable.contains(&unit.document))
            .map(|unit| unit.id.clone())
            .chain(hits)
            .chain(
                changed
                    .iter()
                    .filter(|unit| relevant.contains(&unit.document))
                    .map(|unit| unit.id.clone()),
            )
            .chain(changed.iter().map(|unit| unit.id.clone())),
    );
    let mut bytes = 0;
    let mut remaining = Vec::new();
    for id in order {
        let Some(unit) = changed.iter().find(|unit| unit.id == id) else {
            continue;
        };
        if remaining.len() == 4 || bytes + unit.text.len() > 16_384 {
            break;
        }
        bytes += unit.text.len();
        remaining.push(unit.id.clone());
    }
    let mut sources = unique(runtime.sources.clone());
    sources.sort_by(|a, b| compare_serialized_strings(a, b));
    let mut identity = json!({"task":runtime.query});
    if let Some(implementation) = &runtime.implementation {
        identity["implementation"] = implementation["fingerprint"].clone();
    }
    identity["sources"] = json!(sources);
    identity["snapshot"] = json!(knowledge::knowledge_snapshot(
        project,
        &runtime.sources.iter().cloned().collect()
    ));
    identity["model"] = runtime.execution.cache_identity();
    identity["automatic"] = json!(1);
    store.begin_with_profile(
        BeginWork {
            key: hash(&identity.to_string()),
            kind: if runtime.command == "review" {
                "review"
            } else {
                "ask"
            }
            .to_owned(),
            max_calls: runtime.max_calls,
            max_input_bytes: runtime.max_input_bytes,
            remaining,
            result_key: Some(hash(&stringify_knowledge(packet))),
            snapshot: packet["context"]["snapshot"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            warning_baseline: Some(json!(crate::knowledge::warning_review::warning_baseline(
                &graph
            ))),
        },
        runtime.execution.binding(&identity),
    )
}
pub fn supplied_documents(packet: &Value) -> Result<Vec<SuppliedDocument>> {
    let mut documents: Vec<SuppliedDocument> = serde_json::from_value(packet["documents"].clone())?;
    let separator = regex::Regex::new(r"\r\n|\r|\n").expect("valid line separator");
    for entry in array(&packet["context"]["decisions"]) {
        let evidence = &entry["evidence"];
        if evidence.is_null() {
            continue;
        }
        let text = evidence["text"].as_str().unwrap_or_default();
        let first = evidence["lineStart"].as_u64().unwrap_or(1);
        let lines: Vec<_> = separator
            .split(text)
            .enumerate()
            .map(|(index, line)| vec![json!(first + index as u64), json!(line)])
            .collect();
        documents.push(SuppliedDocument {
            id: evidence["document"].as_str().unwrap_or_default().to_owned(),
            lines,
        });
    }
    Ok(documents)
}
fn finish_answer(
    project: &Project,
    store: &mut Store,
    work: &mut Work,
    packet: &Value,
    value: &Value,
) -> Result<Value> {
    let answer = OutputSchema::Answer
        .parse(value)
        .ok_or_else(|| HivexError::new("READ_FAILED", "Invalid answer"))?;
    if work.status() != crate::work::State::Done {
        work.complete(answer.clone(), hash(&stringify_knowledge(packet)))?;
        store.save(work)?;
    }
    let context = &packet["context"];
    let documents = context_documents(context);
    let supplied = supplied_documents(packet)?;
    let citations: Vec<Citation> = serde_json::from_value(answer["evidence"].clone())?;
    let evidence: Vec<_> = citations
        .iter()
        .filter(|citation| model::supplied_citation(citation, &supplied))
        .filter_map(|citation| model::source_evidence(citation, project))
        .collect();
    let invalid = evidence.len() != citations.len();
    let unreviewed = array(&context["decisions"])
        .iter()
        .chain(array(&context["relationships"]))
        .any(|entry| entry["quality"] != "checked")
        || strings(&context["pendingDocuments"])
            .iter()
            .any(|id| documents.contains(id));
    let omitted = packet["omittedUnits"].as_u64().unwrap_or(0) > 0
        || !array(&packet["warnings"]).is_empty()
        || !array(&context["unexpandedDecisions"]).is_empty();
    let uncertain =
        !array(&context["warnings"]).is_empty() || !array(&answer["uncertainties"]).is_empty();
    let mut warnings: Vec<_> = array(&context["warnings"])
        .iter()
        .chain(array(&packet["warnings"]))
        .cloned()
        .collect();
    if invalid {
        warnings.push(json!(
            "Some model references could not be verified; they are omitted."
        ));
    }
    let graph = model::parse_graph(&store.graph()?, false)
        .ok_or_else(|| HivexError::new("READ_FAILED", "Invalid stored graph"))?;
    Ok(json!({
    "answer":answer["answer"],
    "command":"ask",
    "evidence":evidence,
    "omittedUnits":packet["omittedUnits"],
    "pendingDocuments":knowledge::pending_documents(project,&graph,&documents.into_iter().collect()),
    "snapshot":context["snapshot"],
    "status":if invalid||unreviewed||omitted||uncertain{"partial"}else{"ready"},
    "unavailableDocuments":context["unavailableDocuments"],
    "uncertainties":answer["uncertainties"],
    "unexpandedDecisions":context["unexpandedDecisions"],
    "warnings":warnings,
    "work":model_runtime::work_summary(work)
    }))
}

pub fn ask(project: &mut Project, runtime: &Options) -> Result<Value> {
    let mut context = knowledge::query_graph(project, &runtime.retrieval())?;
    let mut documents = context_documents(&context);
    if documents.is_empty() {
        context["answer"] = Value::Null;
        context["command"] = json!(runtime.command);
        context["guidance"] = json!(
            "Use project terminology, inspect sources, or select a document with --source; do not assume no decision exists."
        );
        context["status"] = json!("no-context");
        return Ok(context);
    }
    let mut packet = answer_packet(project, runtime, context, &documents)?;
    let mut store = Store::open(&project.root, StoreOptions::default())?;
    let mut work = begin_consultation(project, runtime, &mut store, &documents, &packet)?;
    if work.retry_failed(runtime.retry_failed)? {
        store.save(&mut work)?;
    }
    if work.status() != crate::work::State::Done && work.phase() == crate::work::Phase::Update {
        work = update::update(project, runtime, Some(work))?.1;
    }
    context = knowledge::query_graph(project, &runtime.retrieval())?;
    documents = context_documents(&context);
    packet = answer_packet(project, runtime, context.clone(), &documents)?;
    if work.status() == crate::work::State::Failed || work.phase() == crate::work::Phase::Update {
        context["answer"] = Value::Null;
        context["omittedUnits"] = packet["omittedUnits"].clone();
        context["status"] = json!(work.status());
        context["work"] = model_runtime::work_summary(&work);
        return Ok(context);
    }
    if array(&packet["documents"]).is_empty()
        || stringify_knowledge(&packet).len() > runtime.max_context_bytes
    {
        context["answer"] = Value::Null;
        context["command"] = json!(runtime.command);
        context["omittedUnits"] = packet["omittedUnits"].clone();
        context["status"] = json!("context-limit");
        context["warnings"] = json!(
            array(&context["warnings"])
                .iter()
                .chain(array(&packet["warnings"]))
                .cloned()
                .collect::<Vec<_>>()
        );
        context["work"] = model_runtime::work_summary(&work);
        return Ok(context);
    }
    let mut request = Request {
 instruction:"Help the responsible agent with this task. Explain applicable decisions, dependencies and exceptions using the Markdown. Derived graph quality does not itself establish authority or applicability. Do not ask the owner to repeat decisions settled by the supplied evidence. State missing context and uncertainty, including omitted document units and relevant unexpanded dependencies. Do not approve an entire implementation. Cite only the supplied document ranges.".to_owned(),
 packet: packet.clone(),
 schema: OutputSchema::Answer,
 stage: "ask".to_owned(),
};
    if runtime.implementation.is_some() {
        request.instruction = crate::review::REVIEW_INSTRUCTIONS.to_owned();
        request.schema = OutputSchema::Review;
        request.stage = "review".to_owned();
    }
    let value = if work.status() == crate::work::State::Done {
        Some(work.value()["result"].clone())
    } else {
        model_runtime::run_model(&mut work, &mut store, &runtime.execution, &request)?
    };
    let Some(value) = value else {
        context["answer"] = Value::Null;
        context["omittedUnits"] = packet["omittedUnits"].clone();
        context["status"] = json!(work.status());
        context["work"] = model_runtime::work_summary(&work);
        return Ok(context);
    };
    if let Some(implementation) = &runtime.implementation {
        return finish_review(
            project,
            implementation,
            &mut store,
            &mut work,
            &packet,
            &value,
        );
    }
    finish_answer(project, &mut store, &mut work, &packet, &value)
}

fn finish_review(
    project: &Project,
    implementation: &Value,
    store: &mut Store,
    work: &mut Work,
    packet: &Value,
    value: &Value,
) -> Result<Value> {
    let review = crate::review::materialize_review(
        project,
        implementation,
        &supplied_documents(packet)?,
        value,
    )?;
    if work.status() != crate::work::State::Done {
        work.complete(value.clone(), hash(&stringify_knowledge(packet)))?;
        store.save(work)?;
    }
    let binding = crate::review::review_binding(project, implementation);
    let freshness = crate::review::review_freshness(&project.root, &binding)?;
    let context = &packet["context"];
    let warnings: Vec<_> = array(&context["warnings"])
        .iter()
        .chain(array(&packet["warnings"]))
        .chain(array(&implementation["warnings"]))
        .cloned()
        .collect();
    let missing = packet["omittedUnits"].as_u64().unwrap_or(0) > 0
        || !warnings.is_empty()
        || !array(&context["unexpandedDecisions"]).is_empty()
        || !array(&context["unavailableDocuments"]).is_empty();
    let documents = context_documents(context);
    let unreviewed = array(&context["decisions"])
        .iter()
        .chain(array(&context["relationships"]))
        .any(|entry| entry["quality"] != "checked")
        || strings(&context["pendingDocuments"])
            .iter()
            .any(|id| documents.contains(id));
    let incomplete = review["invalidReferences"] == true
        || !array(&review["uncertainties"]).is_empty()
        || missing
        || unreviewed;
    let status = if freshness["status"] == "stale" {
        "stale"
    } else if incomplete {
        "partial"
    } else {
        "ready"
    };
    let graph = model::parse_graph(&store.graph()?, false)
        .ok_or_else(|| HivexError::new("READ_FAILED", "Invalid stored graph"))?;
    Ok(json!({
    "binding":binding,
    "command":"review",
    "findings":review["findings"],
    "freshness":freshness,
    "guidance":"The principal reviewer must verify findings and resolve evidenced conflicts. This report does not approve the implementation.",
    "omittedUnits":packet["omittedUnits"],
    "pendingDocuments":knowledge::pending_documents(project,&graph,&documents.into_iter().collect()),
    "status":status,
    "unavailableDocuments":context["unavailableDocuments"],
    "uncertainties":review["uncertainties"],
    "unexpandedDecisions":context["unexpandedDecisions"],
    "warnings":warnings,
    "work":model_runtime::work_summary(work)
    }))
}
