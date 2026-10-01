use super::{arguments, documents::positive_integer};
use crate::documents::{Project, authored_relations, hash, load_project};
use crate::error::{HivexError, Result};
use serde_json::{Value, json};

struct Options {
  document: String,
  direction: String,
  root: String,
  limit: usize,
  max_bytes: usize,
  cursor: Option<String>,
}

fn invalid(message: &str) -> HivexError {
  HivexError::new("INVALID_ARGUMENT", message)
}

fn options(args: &[String]) -> Result<Options> {
  let parsed = arguments::parse(
    args,
    &["root", "direction", "limit", "max-bytes", "cursor"],
    &[],
  )?;
  if parsed.positionals.len() != 2 || parsed.positionals[1].contains('#') {
    return Err(invalid(
      "Use relations <document> [--direction outgoing|incoming|both] [--limit <count>] [--max-bytes <bytes>] [--cursor <continuation>] [--root <project>]",
    ));
  }
  let direction = parsed
    .values
    .get("direction")
    .cloned()
    .unwrap_or_else(|| "both".into());
  if !["outgoing", "incoming", "both"].contains(&direction.as_str()) {
    return Err(invalid("--direction must be outgoing, incoming or both"));
  }
  let limit = positive_integer(parsed.values.get("limit"), "--limit", Some(20))?;
  let max_bytes = positive_integer(parsed.values.get("max-bytes"), "--max-bytes", Some(16384))?;
  if limit > 2048 || max_bytes > 65536 {
    return Err(invalid(
      "--limit must be at most 2048 and --max-bytes at most 65536",
    ));
  }
  Ok(Options {
    document: parsed.positionals[1].clone(),
    direction,
    root: parsed
      .values
      .get("root")
      .cloned()
      .unwrap_or_else(|| ".".into()),
    limit,
    max_bytes,
    cursor: parsed.values.get("cursor").cloned(),
  })
}

fn cursor_key(project: &Project, options: &Options) -> String {
  hash(
    &json!([
      project.snapshot,
      options.document,
      options.direction,
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
      "Relation continuation belongs to different sources or query options",
    )
  };
  let (key, offset) = cursor
    .strip_prefix("r1.")
    .and_then(|value| value.split_once('.'))
    .ok_or_else(invalid)?;
  if key != cursor_key(project, options)
    || offset.is_empty()
    || !offset.bytes().all(|byte| byte.is_ascii_digit())
  {
    return Err(invalid());
  }
  offset.parse().map_err(|_| invalid())
}

fn response(project: &Project, options: &Options, page: &[Value], bounds: (usize, usize)) -> Value {
  let (start, total) = bounds;
  let next = start + page.len();
  json!({"command":"relations","modelCalls":0,"origin":"current-worktree",
    "snapshot":project.snapshot,"document":options.document,"direction":options.direction,
    "scope":"document; declared direction is preserved; incoming considers ordinary sources and the queried source",
    "status":if project.warnings.is_empty(){"ready"}else{"partial"},
    "coverage":if project.warnings.is_empty(){"selected-sources"}else{"partial"},
    "relations":page,"totalRelations":total,"warnings":project.warnings,
    "continuation":if next<total{Some(format!("r1.{}.{}",cursor_key(project,options),next))}else{None}})
}

fn page(project: &Project, options: &Options, records: &[Value]) -> Result<Value> {
  let start = offset(project, options)?;
  if start > 0 && start >= records.len() {
    return Err(HivexError::new(
      "INVALID_CURSOR",
      "Relation continuation is outside this result",
    ));
  }
  let mut page = Vec::new();
  for record in records.iter().skip(start).take(options.limit) {
    page.push(record.clone());
    if serde_json::to_vec(&response(project, options, &page, (start, records.len())))?.len()
      <= options.max_bytes
    {
      continue;
    }
    page.pop();
    if page.is_empty() {
      return Err(HivexError::new(
        "OUTPUT_LIMIT",
        "The next complete relation exceeds --max-bytes; increase it within 65536",
      ));
    }
    break;
  }
  let value = response(project, options, &page, (start, records.len()));
  if serde_json::to_vec(&value)?.len() > options.max_bytes {
    return Err(HivexError::new(
      "OUTPUT_LIMIT",
      "Relation response metadata exceeds --max-bytes",
    ));
  }
  Ok(value)
}

pub fn command(args: &[String]) -> Result<Value> {
  let options = options(args)?;
  let project = load_project(&options.root)?;
  let records = authored_relations(&project, &options.document, &options.direction)?;
  let values = records
    .into_iter()
    .map(|record| {
      let navigation = match (
        record.from.document == options.document,
        record.to.document == options.document,
      ) {
        (true, true) => "self",
        (true, false) => "outgoing",
        _ => "incoming",
      };
      let mut value = serde_json::to_value(record)?;
      value["navigation"] = json!(navigation);
      Ok(value)
    })
    .collect::<Result<Vec<_>>>()?;
  page(&project, &options, &values)
}
