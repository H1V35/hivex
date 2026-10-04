use super::arguments::{Arguments, invalid};
use super::page::{self, Cursor};
use super::{ORIGIN, coverage, status};
use crate::documents::{Match, Query, hash, load_project, search_sources};
use crate::error::{HivexError, Result};
use serde_json::{Value, json};

const MAX_QUERY_BYTES: usize = 4_096;

/// `search`: lexical source passages ranked by relevance.
pub fn command(args: &[String]) -> Result<Value> {
  let arguments = Arguments::parse(
    args,
    &["cursor", "limit", "max-bytes", "root", "source"],
    &["historical"],
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
  let project = load_project(arguments.root())?;
  let key = hash(
    &json!([
      "context-v1",
      project.snapshot,
      query,
      sources,
      historical,
      limit,
      max_bytes
    ])
    .to_string(),
  );
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
  page::fill(
    result.matches,
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
        "status": status(&project),
        "coverage": coverage(&project),
        "scope": "lexical source passages; a match does not establish semantic applicability or exhaust relevant context",
        "matches": matches,
        "totalMatches": result.total,
        "warnings": project.warnings,
        "continuation": cursor.next(start + matches.len(), result.total),
      })
    },
    oversized,
  )
}

fn usage() -> HivexError {
  invalid(
    "Use search <query up to 4096 bytes> [--source <document>] [--historical] [--limit <count>] [--max-bytes <bytes>] [--cursor <continuation>] [--root <project>]",
  )
}

fn oversized(found: &Match) -> HivexError {
  HivexError::new(
    "OUTPUT_LIMIT",
    "The next complete passage exceeds --max-bytes; read its cited range or increase the limit",
  )
  .with_details(json!({
    "document": found.document,
    "from": found.line_start,
    "to": found.line_end,
    "context": {"from": found.context.line_start, "to": found.context.line_end},
  }))
}
