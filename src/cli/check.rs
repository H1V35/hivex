use super::{arguments, documents::positive_integer};
use crate::documents::{hash, load_project, validate_sources};
use crate::error::{HivexError, Result};
use serde_json::{Value, json};

pub fn command(args: &[String]) -> Result<Value> {
  let parsed = arguments::parse(
    args,
    &["root", "source", "limit", "max-bytes", "cursor"],
    &["historical"],
  )
  .map_err(|mut error| {
    "INVALID_ARGUMENT".clone_into(&mut error.code);
    error
  })?;
  if parsed.positionals.len() != 1 {
    return Err(HivexError::new(
      "INVALID_ARGUMENT",
      "Use check [--source <document>] [--historical] [--limit <count>] [--max-bytes <bytes>] [--cursor <continuation>] [--root <project>]",
    ));
  }
  let limit = positive_integer(parsed.values.get("limit"), "--limit", Some(20))?;
  let max_bytes = positive_integer(parsed.values.get("max-bytes"), "--max-bytes", Some(16384))?;
  if limit > 2048 || max_bytes > 65536 {
    return Err(HivexError::new(
      "INVALID_ARGUMENT",
      "--limit must be at most 2048 and --max-bytes at most 65536",
    ));
  }
  let project = load_project(parsed.values.get("root").map_or(".", String::as_str))?;
  let mut sources = parsed.repeated.get("source").cloned().unwrap_or_default();
  sources.sort();
  sources.dedup();
  let historical = parsed.flags.contains("historical");
  let key = hash(&json!([project.snapshot, sources, historical, limit, max_bytes]).to_string());
  let start = cursor_offset(parsed.values.get("cursor"), &key)?;
  let validation = validate_sources(&project, &sources, historical)?;
  let total = validation.findings.len();
  if start > 0 && start >= total {
    return Err(HivexError::new(
      "INVALID_CURSOR",
      "Validation continuation is outside this result",
    ));
  }
  let status = match (total > 0, project.warnings.is_empty()) {
    (true, _) => "failed",
    (false, true) => "ready",
    _ => "partial",
  };
  let metadata = json!({"command":"check","modelCalls":0,"origin":"current-worktree","snapshot":project.snapshot,
    "status":status,
    "coverage":if project.warnings.is_empty(){"selected-sources"}else{"partial"},"sources":sources,"historical":historical,
    "checkedDocuments":validation.checked,"checkedMetadataDocuments":validation.checked_metadata,
    "checkedReferenceDocuments":validation.checked_references,"totalFindings":total,"warnings":project.warnings,
    "scope":"outer metadata under docs, authored relation syntax and selected local Markdown references; historical bodies require explicit selection; no semantic certification"});
  bounded_page(
    metadata,
    &validation.findings,
    (start, limit),
    (&key, max_bytes),
  )
}

fn cursor_offset(cursor: Option<&String>, key: &str) -> Result<usize> {
  let Some(cursor) = cursor else {
    return Ok(0);
  };
  let invalid = || {
    HivexError::new(
      "INVALID_CURSOR",
      "Validation continuation belongs to different sources or query options",
    )
  };
  let (identity, start) = cursor
    .strip_prefix("v2.")
    .and_then(|value| value.split_once('.'))
    .ok_or_else(invalid)?;
  if identity != key || start.is_empty() || !start.bytes().all(|byte| byte.is_ascii_digit()) {
    return Err(invalid());
  }
  start.parse().map_err(|_| invalid())
}

fn bounded_page(
  mut metadata: Value,
  findings: &[Value],
  page: (usize, usize),
  bounds: (&str, usize),
) -> Result<Value> {
  let (start, limit) = page;
  let (key, max_bytes) = bounds;
  let mut records = Vec::new();
  for record in findings.iter().skip(start).take(limit) {
    records.push(record.clone());
    metadata["findings"] = json!(records);
    metadata["continuation"] = if start + records.len() < findings.len() {
      json!(format!("v2.{key}.{}", start + records.len()))
    } else {
      Value::Null
    };
    if serde_json::to_vec(&metadata)?.len() <= max_bytes {
      continue;
    }
    records.pop();
    if records.is_empty() {
      return Err(HivexError::new(
        "OUTPUT_LIMIT",
        "The next complete validation finding exceeds --max-bytes",
      ));
    }
    break;
  }
  metadata["findings"] = json!(records);
  metadata["continuation"] = if start + records.len() < findings.len() {
    json!(format!("v2.{key}.{}", start + records.len()))
  } else {
    Value::Null
  };
  if serde_json::to_vec(&metadata)?.len() > max_bytes {
    return Err(HivexError::new(
      "OUTPUT_LIMIT",
      "Validation metadata exceeds --max-bytes",
    ));
  }
  Ok(metadata)
}
