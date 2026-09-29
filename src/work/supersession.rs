use super::{Phase, State, Store, StoreOptions, Work, store};
use crate::documents::{Project, load_project};
use crate::error::{HivexError, Result};
use crate::knowledge::{
  Citation, Graph, is_current_source, parse_graph, parse_range, valid_citation,
};
use chrono::{SecondsFormat, Utc};
use rusqlite::TransactionBehavior;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
  work_id: String,
  check_input_hash: String,
  replacement_work_id: String,
  reason: String,
  evidence: Vec<Citation>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
  request: Request,
  replacement_key: String,
  covered_units: Vec<Citation>,
  closed_at: String,
}

fn invalid() -> HivexError {
  HivexError::new(
    "INVALID_SUPERSESSION",
    "Supersession requires an exact safely ended failed update, current evidence, and current coverage attributable to its completed replacement. No model call was made.",
  )
}

fn citation_shape(citation: &Citation) -> bool {
  crate::knowledge::validate_citation(citation)
    && citation.line_start <= citation.line_end
    && citation
      .version
      .as_deref()
      .is_some_and(|version| !version.is_empty())
}

impl Request {
  fn valid(&self) -> bool {
    !self.work_id.is_empty()
      && !self.check_input_hash.is_empty()
      && !self.replacement_work_id.is_empty()
      && self.work_id != self.replacement_work_id
      && !self.reason.trim().is_empty()
      && self.reason.encode_utf16().count() <= 2048
      && (1..=32).contains(&self.evidence.len())
      && self.evidence.iter().all(citation_shape)
  }
}

pub(super) fn validate_record(value: &Value) -> bool {
  let Some(record) = value.get("supersession") else {
    return true;
  };
  let Ok(record) = serde_json::from_value::<Record>(record.clone()) else {
    return false;
  };
  record.request.valid()
    && value["id"] == record.request.work_id
    && value["kind"] == "update"
    && value["status"] == "failed"
    && safely_ended_value(value)
    && value["attempts"]
      .as_array()
      .and_then(|attempts| attempts.last())
      .is_some_and(|attempt| {
        attempt["stage"] == "check" && attempt["inputHash"] == record.request.check_input_hash
      })
    && !record.replacement_key.is_empty()
    && !record.covered_units.is_empty()
    && record.covered_units.iter().all(citation_shape)
    && target_ranges(value).is_ok_and(|targets| targets == record.covered_units)
    && chrono::DateTime::parse_from_rfc3339(&record.closed_at).is_ok()
}

fn read_request(path: &Path) -> Result<Request> {
  let mut bytes = Vec::new();
  File::open(path)?
    .take(2 * 1024 * 1024 + 1)
    .read_to_end(&mut bytes)?;
  if bytes.len() > 2 * 1024 * 1024 {
    return Err(invalid());
  }
  let request: Request = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
  if !request.valid() {
    return Err(invalid());
  }
  Ok(request)
}

fn safely_ended_value(value: &Value) -> bool {
  value["nativeProcessId"].is_null()
    && value["attempts"].as_array().is_some_and(|attempts| {
      attempts.iter().all(|attempt| {
        let report = &attempt["report"];
        report["outcome"] == "completed"
          && report["cleanup"] == "confirmed"
          && report["turnAccepted"] == "confirmed"
          && report["interruption"] != "unconfirmed"
      })
    })
}

fn eligible(work: &Work, replacement: &Work, request: &Request) -> bool {
  work.id() == request.work_id
    && replacement.id() == request.replacement_work_id
    && work.kind() == Phase::Update
    && work.status() == State::Failed
    && work.value()["pending"]["staged"] == true
    && work
      .attempts()
      .and_then(|attempts| attempts.last())
      .is_some_and(|attempt| {
        attempt["stage"] == "check" && attempt["inputHash"] == request.check_input_hash
      })
    && safely_ended_value(work.value())
    && replacement.kind() == Phase::Update
    && replacement.status() == State::Done
    && replacement.remaining().is_empty()
    && replacement.value()["pending"].is_null()
    && safely_ended_value(replacement.value())
}

fn target_ranges(value: &Value) -> Result<Vec<Citation>> {
  let mut units: Vec<String> =
    serde_json::from_value(value["remaining"].clone()).map_err(|_| invalid())?;
  let pending = &value["pending"];
  let staged: Vec<String> =
    serde_json::from_value(pending["units"].clone()).map_err(|_| invalid())?;
  units.extend(staged);
  units.sort();
  units.dedup();
  if units.is_empty() {
    return Err(invalid());
  }
  let documents = pending["packet"]["documents"]
    .as_array()
    .ok_or_else(invalid)?;
  units
    .iter()
    .map(|unit| {
      let range = parse_range(unit).ok_or_else(invalid)?;
      let source = documents
        .iter()
        .find(|source| source["id"] == range.document)
        .ok_or_else(invalid)?;
      Ok(Citation {
        document: range.document,
        line_start: range.line_start,
        line_end: range.line_end,
        version: Some(source["version"].as_str().ok_or_else(invalid)?.to_owned()),
      })
    })
    .collect()
}

fn covers(target: &Citation, ranges: &[Citation]) -> bool {
  let mut spans: Vec<_> = ranges
    .iter()
    .filter(|range| {
      range.document == target.document
        && range.version == target.version
        && range.line_end >= target.line_start
        && range.line_start <= target.line_end
    })
    .collect();
  spans.sort_by_key(|range| range.line_start);
  let mut next = target.line_start;
  for span in spans {
    if span.line_start > next {
      return false;
    }
    if span.line_end >= target.line_end {
      return true;
    }
    next = next.max(span.line_end.saturating_add(1));
  }
  false
}

fn attributed_ranges(graph: &Graph, replacement: &Work) -> Result<Vec<Citation>> {
  let planned: Vec<String> =
    serde_json::from_value(replacement.value()["plannedUnits"].clone()).map_err(|_| invalid())?;
  Ok(
    graph
      .units
      .iter()
      .filter_map(|(id, unit)| {
        if unit["workKey"] != replacement.key() || !planned.contains(id) {
          return None;
        }
        let range = parse_range(id)?;
        if unit["document"] != range.document {
          return None;
        }
        Some(Citation {
          document: range.document,
          line_start: range.line_start,
          line_end: range.line_end,
          version: Some(unit["version"].as_str()?.to_owned()),
        })
      })
      .collect(),
  )
}

fn current_citation(project: &Project, citation: &Citation) -> bool {
  citation_shape(citation)
    && valid_citation(citation, &project.documents)
    && is_current_source(project, &citation.document, citation.version.as_deref())
}

fn prove_coverage(
  project: &Project,
  graph: &Graph,
  work: &Work,
  replacement: &Work,
) -> Result<Vec<Citation>> {
  let targets = target_ranges(work.value())?;
  let attributed: Vec<_> = attributed_ranges(graph, replacement)?
    .into_iter()
    .filter(|range| current_citation(project, range))
    .collect();
  if targets.iter().any(|target| {
    !current_citation(project, target)
      || graph
        .documents
        .get(&target.document)
        .and_then(Value::as_str)
        != target.version.as_deref()
      || !covers(target, &attributed)
  }) {
    return Err(invalid());
  }
  Ok(targets)
}

fn response(record: &Record) -> Value {
  json!({"command":"recover", "status":"superseded", "modelCalls":0,
    "workId":record.request.work_id,"replacementWorkId":record.request.replacement_work_id,
    "supersession":record})
}

pub fn supersede(root: &Path, path: &str) -> Result<Value> {
  let request = read_request(Path::new(path))?;
  let project = load_project(root.to_str().ok_or_else(invalid)?)?;
  let mut storage = Store::open(
    &project.root,
    StoreOptions {
      update: true,
      ..StoreOptions::default()
    },
  )?;
  let transaction = storage
    .database_mut()
    .transaction_with_behavior(TransactionBehavior::Immediate)?;
  let result = apply_supersession(&project, &transaction, request)?;
  transaction.commit()?;
  Ok(result)
}

fn apply_supersession(
  project: &Project,
  transaction: &rusqlite::Connection,
  request: Request,
) -> Result<Value> {
  let mut work = store::read_work_by_id(transaction, &request.work_id)?;
  if let Some(record) = work.value().get("supersession") {
    let record: Record = serde_json::from_value(record.clone()).map_err(|_| invalid())?;
    if record.request != request {
      return Err(invalid());
    }
    return Ok(response(&record));
  }
  let replacement = store::read_work_by_id(transaction, &request.replacement_work_id)?;
  let matching: i64 = transaction.query_row(
    "SELECT count(*) FROM work WHERE kind='update' AND key=?1",
    [replacement.key()],
    |row| row.get(0),
  )?;
  if matching != 1
    || !eligible(&work, &replacement, &request)
    || request
      .evidence
      .iter()
      .any(|citation| !current_citation(project, citation))
  {
    return Err(invalid());
  }
  let data: String =
    transaction.query_row("SELECT data FROM graph WHERE id=1", [], |row| row.get(0))?;
  let graph = parse_graph(&serde_json::from_str(&data)?, false).ok_or_else(invalid)?;
  let covered_units = prove_coverage(project, &graph, &work, &replacement)?;
  if covered_units.iter().any(|target| {
    !request
      .evidence
      .iter()
      .any(|citation| citation.document == target.document && citation.version == target.version)
  }) {
    return Err(invalid());
  }
  let record = Record {
    request,
    replacement_key: replacement.key().to_owned(),
    covered_units,
    closed_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
  };
  work.value["supersession"] = serde_json::to_value(&record)?;
  store::save_work(transaction, &mut work)?;
  Ok(response(&record))
}
