use super::{arguments, documents::positive_integer};
use crate::documents::{Project, hash, load_project, search_sources};
use crate::error::{HivexError, Result};
use serde_json::{Value, json};

struct Options {
  query: String,
  root: String,
  sources: Vec<String>,
  historical: bool,
  limit: usize,
  max_bytes: usize,
  cursor: Option<String>,
}

fn options(args: &[String]) -> Result<Options> {
  let parsed = arguments::parse(
    args,
    &["root", "source", "limit", "max-bytes", "cursor"],
    &["historical"],
  )
  .map_err(|mut error| {
    "INVALID_ARGUMENT".clone_into(&mut error.code);
    error
  })?;
  if parsed.positionals.len() != 2
    || parsed.positionals[1].trim().is_empty()
    || parsed.positionals[1].len() > 4096
  {
    return Err(HivexError::new(
      "INVALID_ARGUMENT",
      "Use search <query up to 4096 bytes> [--source <document>] [--historical] [--limit <count>] [--max-bytes <bytes>] [--cursor <continuation>] [--root <project>]",
    ));
  }
  let limit = positive_integer(parsed.values.get("limit"), "--limit", Some(6))?;
  let max_bytes = positive_integer(parsed.values.get("max-bytes"), "--max-bytes", Some(16384))?;
  if limit > 64 || max_bytes > 65536 {
    return Err(HivexError::new(
      "INVALID_ARGUMENT",
      "--limit must be at most 64 and --max-bytes at most 65536",
    ));
  }
  let mut sources = parsed.repeated.get("source").cloned().unwrap_or_default();
  sources.sort();
  sources.dedup();
  Ok(Options {
    query: parsed.positionals[1].clone(),
    root: parsed
      .values
      .get("root")
      .cloned()
      .unwrap_or_else(|| ".".into()),
    sources,
    historical: parsed.flags.contains("historical"),
    limit,
    max_bytes,
    cursor: parsed.values.get("cursor").cloned(),
  })
}

fn key(project: &Project, options: &Options) -> String {
  hash(
    &json!([
      project.snapshot,
      options.query,
      options.sources,
      options.historical,
      options.limit,
      options.max_bytes
    ])
    .to_string(),
  )
}

fn offset(project: &Project, options: &Options) -> Result<usize> {
  let Some(cursor) = options.cursor.as_deref() else {
    return Ok(0);
  };
  let invalid = || {
    HivexError::new(
      "INVALID_CURSOR",
      "Search continuation belongs to different sources or query options",
    )
  };
  let (identity, start) = cursor
    .strip_prefix("q1.")
    .and_then(|value| value.split_once('.'))
    .ok_or_else(invalid)?;
  if identity != key(project, options)
    || start.is_empty()
    || !start.bytes().all(|byte| byte.is_ascii_digit())
  {
    return Err(invalid());
  }
  start.parse().map_err(|_| invalid())
}

fn response(project: &Project, options: &Options, page: &[Value], bounds: (usize, usize)) -> Value {
  let (start, total) = bounds;
  let next = start + page.len();
  json!({"command":"search","modelCalls":0,"origin":"current-worktree","snapshot":project.snapshot,
    "query":options.query,"sources":options.sources,"historical":options.historical,
    "status":if project.warnings.is_empty(){"ready"}else{"partial"},
    "coverage":if project.warnings.is_empty(){"selected-sources"}else{"partial"},
    "scope":"lexical source passages; a match does not establish semantic applicability or exhaust relevant context",
    "matches":page,"totalMatches":total,"warnings":project.warnings,
    "continuation":if next<total{Some(format!("q1.{}.{}",key(project,options),next))}else{None}})
}

pub fn command(args: &[String]) -> Result<Value> {
  let options = options(args)?;
  let project = load_project(&options.root)?;
  let start = offset(&project, &options)?;
  let result = search_sources(
    &project,
    &options.query,
    (&options.sources, options.historical),
    (start, options.limit),
  )?;
  if start > 0 && start >= result.total {
    return Err(HivexError::new(
      "INVALID_CURSOR",
      "Search continuation is outside this result",
    ));
  }
  let mut page = Vec::new();
  for found in result.matches {
    page.push(serde_json::to_value(&found)?);
    if serde_json::to_vec(&response(&project, &options, &page, (start, result.total)))?.len()
      <= options.max_bytes
    {
      continue;
    }
    page.pop();
    if page.is_empty() {
      return Err(HivexError::new("OUTPUT_LIMIT", "The next complete passage exceeds --max-bytes; read its cited range or increase the limit")
        .with_details(json!({"document":found.document,"from":found.line_start,"to":found.line_end})));
    }
    break;
  }
  let response = response(&project, &options, &page, (start, result.total));
  if serde_json::to_vec(&response)?.len() > options.max_bytes {
    return Err(HivexError::new(
      "OUTPUT_LIMIT",
      "Search metadata exceeds --max-bytes",
    ));
  }
  Ok(response)
}
