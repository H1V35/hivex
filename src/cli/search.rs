use super::arguments::{Arguments, invalid};
use super::page::{self, Cursor};
use super::{ORIGIN, coverage, status};
use crate::documents::{Query, hash, load_project, locate, search_sources};
use crate::error::{HiveXError, Result};
use serde_json::{Value, json};

const MAX_QUERY_BYTES: usize = 4_096;

/// `search`: lexical source passages ranked by relevance.
pub fn command(args: &[String]) -> Result<Value> {
  let arguments = Arguments::parse(
    args,
    &["cursor", "limit", "max-bytes", "root", "source"],
    &["historical", "locate"],
  )?;
  let [_, query] = arguments.positionals() else {
    return Err(usage());
  };
  if query.trim().is_empty() || query.len() > MAX_QUERY_BYTES {
    return Err(usage());
  }
  let limit = arguments.count("limit", 6, 64)?;
  let max_bytes = page::max_bytes(&arguments)?;
  let sources = arguments.sources();
  let historical = arguments.flag("historical");
  let locating = arguments.flag("locate");
  let project = load_project(arguments.root())?;
  let mut options = json!([
    "context-v1",
    project.snapshot,
    query,
    sources,
    historical,
    limit,
    max_bytes
  ]);
  // Only locating cursors carry the option, so ordinary cursors keep their earlier key.
  if locating {
    options
      .as_array_mut()
      .expect("an array")
      .push(json!("locate"));
  }
  let key = hash(&options.to_string());
  let cursor = Cursor {
    version: "q2",
    key: &key,
  };
  let start = cursor.offset(arguments.value("cursor"))?;
  let scope = Query {
    text: query,
    sources: &sources,
    historical,
  };
  let result = search_sources(&project, &scope, start, limit)?;
  page::check_offset(start, result.total)?;
  let records: Vec<Value> = if locating {
    let locations = locate(&project, query, result.matches)?;
    locations.iter().map(|location| json!(location)).collect()
  } else {
    result.matches.iter().map(|found| json!(found)).collect()
  };
  let scope = if locating {
    "lexical source locations without their text; a location orients, so read its section before applying it"
  } else {
    "lexical source passages; a match does not establish semantic applicability or exhaust relevant context"
  };
  page::fill(
    records,
    max_bytes,
    |matches| {
      json!({
        "command": "search",
        "modelCalls": 0,
        "origin": ORIGIN,
        "snapshot": project.snapshot,
        "query": query,
        "sources": sources,
        "historical": historical,
        "locate": locating,
        "status": status(&project),
        "coverage": coverage(&project),
        "scope": scope,
        "matches": matches,
        "totalMatches": result.total,
        "warnings": project.warnings,
        "continuation": cursor.next(start + matches.len(), result.total),
      })
    },
    oversized,
  )
}

fn usage() -> HiveXError {
  invalid(
    "Use search <query up to 4096 bytes> [--source <document>] [--historical] [--locate] [--limit <count>] [--max-bytes <bytes>] [--cursor <continuation>] [--root <project>]",
  )
}

fn oversized(found: &Value) -> HiveXError {
  HiveXError::new(
    "OUTPUT_LIMIT",
    "The next complete passage exceeds --max-bytes; read its cited range or increase the limit",
  )
  .with_details(json!({
    "document": found["document"],
    "from": found["lineStart"],
    "to": found["lineEnd"],
    "context": {"from": found["context"]["lineStart"], "to": found["context"]["lineEnd"]},
  }))
}
