use crate::cli::arguments;
use crate::compatibility::trim_js_whitespace;
use crate::documents::load_project;
use crate::error::{HivexError, Result};
use crate::knowledge::ingestion::RepairRange;
use crate::knowledge::model::{self as model};
use serde_json::{Value, json};
use std::collections::HashSet;

use crate::knowledge::{current_graph, pending_documents, query_graph};
use crate::work::Operation;

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

pub fn operation_for(input: &[String]) -> Result<Operation> {
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
    let options = Operation {
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

pub fn command(args: &[String]) -> Result<Value> {
    let mut options = operation_for(args)?;
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
        return Ok(crate::knowledge::update::update(&mut project, &options, None)?.0);
    }
    if options.command == "review" {
        let implementation = crate::review::implementation::capture_implementation(
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
