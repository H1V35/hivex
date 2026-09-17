use super::arguments;
use crate::documents as markdown;
use crate::documents::{Document, MAX_DOCUMENTS, Project, Warning, load_project};
use crate::error::{HivexError, Result};
use serde_json::{Map, Value, json};
const DEFAULT_MAX_BYTES: usize = 16_384;
const MAX_OUTPUT_BYTES: usize = 65_536;
const ORIGIN: &str = "current-worktree";
const SAFE_INTEGER_MAX: u64 = 9_007_199_254_740_991;
struct CommandOptions {
  root: String,
  max_bytes: usize,
  from: Option<usize>,
  to: Option<usize>,
  limit: usize,
  cursor: Option<String>,
}

fn error(code: &str, message: impl Into<String>) -> HivexError {
  HivexError::new(code, message)
}
fn details(code: &str, message: impl Into<String>, value: Value) -> HivexError {
  error(code, message).with_details(value)
}
fn positive_integer(value: Option<&String>, label: &str, fallback: Option<usize>) -> Result<usize> {
  let Some(value) = value else {
    return fallback.ok_or_else(|| error("INVALID_ARGUMENT", format!("{label} is required")));
  };
  if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
    return Err(error(
      "INVALID_ARGUMENT",
      format!("{label} must be a positive integer"),
    ));
  }
  let number = value.parse::<u64>().map_err(|_| {
    error(
      "INVALID_ARGUMENT",
      format!("{label} must be a positive integer"),
    )
  })?;
  if number == 0 || number > SAFE_INTEGER_MAX {
    return Err(error(
      "INVALID_ARGUMENT",
      format!("{label} must be positive"),
    ));
  }
  usize::try_from(number).map_err(|_| {
    error(
      "INVALID_ARGUMENT",
      format!("{label} exceeds platform limit"),
    )
  })
}

fn optional_positive_integer(value: Option<&String>, label: &str) -> Result<Option<usize>> {
  value.map_or(Ok(None), |value| {
    positive_integer(Some(value), label, None).map(Some)
  })
}

fn command_options(args: &[String]) -> Result<(String, Option<String>, CommandOptions)> {
  let parsed = arguments::parse(
    args,
    &["cursor", "from", "limit", "max-bytes", "root", "to"],
    &[],
  )
  .map_err(|mut failure| {
    "INVALID_ARGUMENT".clone_into(&mut failure.code);
    failure
  })?;
  let (command, id) = document_command(&parsed)?;
  let max_bytes = positive_integer(
    parsed.values.get("max-bytes"),
    "--max-bytes",
    Some(DEFAULT_MAX_BYTES),
  )?;
  if max_bytes > MAX_OUTPUT_BYTES {
    return Err(error(
      "INVALID_ARGUMENT",
      format!("--max-bytes must be at most {MAX_OUTPUT_BYTES}"),
    ));
  }
  if command == "sources"
    && (parsed.values.contains_key("from") || parsed.values.contains_key("to"))
  {
    return Err(error(
      "INVALID_ARGUMENT",
      "--from and --to are only valid for read",
    ));
  }
  if command == "read"
    && (parsed.values.contains_key("limit") || parsed.values.contains_key("cursor"))
  {
    return Err(error(
      "INVALID_ARGUMENT",
      "--limit and --cursor are only valid for sources",
    ));
  }
  let from = optional_positive_integer(parsed.values.get("from"), "--from")?;
  let to = optional_positive_integer(parsed.values.get("to"), "--to")?;
  let limit = positive_integer(parsed.values.get("limit"), "--limit", Some(20))?;
  let root = parsed.values.get("root").cloned().unwrap_or_else(|| {
    std::env::current_dir().map_or_else(
      |_| ".".to_owned(),
      |path| path.to_string_lossy().into_owned(),
    )
  });
  Ok((
    command,
    id,
    CommandOptions {
      root,
      max_bytes,
      from,
      to,
      limit,
      cursor: parsed.values.get("cursor").cloned(),
    },
  ))
}

fn metadata_value(document: &Document) -> Value {
  let mut value = Map::new();
  value.insert("hash".to_owned(), Value::String(document.hash.clone()));
  value.insert("historical".to_owned(), Value::Bool(document.historical));
  value.insert("id".to_owned(), Value::String(document.id.clone()));
  value.insert(
    "links".to_owned(),
    Value::Array(document.links.iter().cloned().map(Value::String).collect()),
  );
  value.insert("path".to_owned(), Value::String(document.path.clone()));
  value.insert(
    "status".to_owned(),
    document.status.clone().map_or(Value::Null, Value::String),
  );
  value.insert("title".to_owned(), Value::String(document.title.clone()));
  Value::Object(value)
}

fn warnings_value(warnings: &[Warning]) -> Value {
  Value::Array(
    warnings
      .iter()
      .map(|warning| json!({"message": warning.message, "path": warning.path}))
      .collect(),
  )
}

fn bounded_lines(
  lines: &[String],
  start: usize,
  end: usize,
  max_bytes: usize,
) -> Result<(usize, String)> {
  let mut text = String::new();
  let mut prefix = String::new();
  let mut line_end = start - 1;
  for line in start..=end {
    let raw = lines.get(line - 1).map_or("", String::as_str);
    let current = if line == lines.len() {
      raw
    } else {
      markdown::line_content(raw)
    };
    let next = format!("{prefix}{current}");
    if next.len() > max_bytes {
      if line_end < start {
        return Err(details(
          "OUTPUT_LIMIT",
          "The first requested line exceeds --max-bytes",
          json!({
              "line": line,
              "maxBytes": max_bytes,
              "requiredBytes": next.len()
          }),
        ));
      }
      return Ok((line_end, text));
    }
    text = next;
    prefix.push_str(raw);
    line_end = line;
  }
  Ok((line_end, text))
}

fn continuation_value(
  line_end: usize,
  max_bytes: usize,
  requested_end: usize,
  total_lines: usize,
) -> Value {
  if line_end >= total_lines {
    return Value::Null;
  }
  json!({
      "from": line_end + 1,
      "maxBytes": max_bytes,
      "reason": if line_end < requested_end { "max-bytes" } else { "range" },
      "to": total_lines
  })
}

fn read_command(project: &Project, id: &str, options: &CommandOptions) -> Result<Value> {
  let Some(source) = project.documents.iter().find(|document| document.id == id) else {
    return Err(details(
      "SOURCE_NOT_FOUND",
      format!("Markdown source was not selected: {id}"),
      json!({"id": id}),
    ));
  };
  let lines = markdown::raw_markdown_lines(&source.text);
  let start = options.from.unwrap_or(1);
  let requested_end = options.to.unwrap_or(lines.len());
  if start > lines.len() || requested_end > lines.len() || start > requested_end {
    return Err(details(
      "INVALID_RANGE",
      format!("Line range {start}-{requested_end} is outside the source"),
      json!({"id": id, "lineCount": lines.len()}),
    ));
  }
  let (line_end, text) = bounded_lines(&lines, start, requested_end, options.max_bytes)?;
  let continuation = continuation_value(line_end, options.max_bytes, requested_end, lines.len());
  let mut response = Map::new();
  response.insert("command".to_owned(), Value::String("read".to_owned()));
  response.insert("continuation".to_owned(), continuation.clone());
  response.insert("lineEnd".to_owned(), json!(line_end));
  response.insert("lineStart".to_owned(), json!(start));
  response.insert("origin".to_owned(), Value::String(ORIGIN.to_owned()));
  response.insert(
    "snapshot".to_owned(),
    Value::String(project.snapshot.clone()),
  );
  response.insert("source".to_owned(), metadata_value(source));
  response.insert("text".to_owned(), Value::String(text));
  response.insert("truncated".to_owned(), Value::Bool(!continuation.is_null()));
  response.insert("warnings".to_owned(), warnings_value(&project.warnings));
  Ok(Value::Object(response))
}

fn parse_cursor(cursor: &str) -> Option<(&str, usize)> {
  let value = cursor.strip_prefix("s1.")?;
  let (snapshot, start) = value.rsplit_once('.')?;
  if snapshot.len() != 64
    || !snapshot
      .bytes()
      .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
  {
    return None;
  }
  if start.is_empty() || !start.bytes().all(|byte| byte.is_ascii_digit()) {
    return None;
  }
  let parsed = start.parse::<u64>().ok()?;
  if parsed > SAFE_INTEGER_MAX {
    return None;
  }
  Some((snapshot, usize::try_from(parsed).ok()?))
}

fn sources_response(project: &Project, documents: &[Value], start: usize) -> Value {
  let continuation = if start + documents.len() < project.documents.len() {
    Value::String(format!(
      "s1.{}.{}",
      project.snapshot,
      start + documents.len()
    ))
  } else {
    Value::Null
  };
  let mut response = Map::new();
  response.insert("command".to_owned(), Value::String("sources".to_owned()));
  response.insert("continuation".to_owned(), continuation);
  response.insert("documents".to_owned(), Value::Array(documents.to_owned()));
  response.insert("origin".to_owned(), Value::String(ORIGIN.to_owned()));
  response.insert(
    "snapshot".to_owned(),
    Value::String(project.snapshot.clone()),
  );
  response.insert("totalDocuments".to_owned(), json!(project.documents.len()));
  response.insert("warnings".to_owned(), warnings_value(&project.warnings));
  Value::Object(response)
}

fn list_sources(project: &Project, options: &CommandOptions) -> Result<Value> {
  let start = match options.cursor.as_deref() {
    None => 0,
    Some(cursor) => {
      let Some((snapshot, start)) = parse_cursor(cursor) else {
        return Err(error(
          "INVALID_CURSOR",
          "Source continuation belongs to a different or invalid snapshot",
        ));
      };
      if snapshot != project.snapshot {
        return Err(error(
          "INVALID_CURSOR",
          "Source continuation belongs to a different or invalid snapshot",
        ));
      }
      if start > 0 && start >= project.documents.len() {
        return Err(error(
          "INVALID_CURSOR",
          "Source continuation is outside this snapshot",
        ));
      }
      start
    }
  };
  let end = start
    .saturating_add(options.limit.min(MAX_DOCUMENTS))
    .min(project.documents.len());
  let mut documents = Vec::new();
  for document in &project.documents[start..end] {
    documents.push(metadata_value(document));
    if serde_json::to_vec(&sources_response(project, &documents, start))?.len() > options.max_bytes
    {
      documents.pop();
      if documents.is_empty() {
        return Err(error(
          "OUTPUT_LIMIT",
          "The next source metadata does not fit; increase --max-bytes or narrow the selected sources",
        ));
      }
      break;
    }
  }
  let response = sources_response(project, &documents, start);
  if serde_json::to_vec(&response)?.len() > options.max_bytes {
    return Err(error(
      "OUTPUT_LIMIT",
      "Source-list metadata exceeds --max-bytes",
    ));
  }
  Ok(response)
}

pub fn command(args: &[String]) -> Result<Value> {
  let (command, id, options) = command_options(args)?;
  let project = load_project(&options.root)?;
  if command == "sources" {
    list_sources(&project, &options)
  } else {
    read_command(&project, id.as_deref().unwrap_or_default(), &options)
  }
}

fn document_command(parsed: &arguments::Parsed) -> Result<(String, Option<String>)> {
  let command = parsed.positionals.first().cloned();
  let id = parsed.positionals.get(1).cloned();
  let extra = parsed.positionals.get(2);
  let Some(command) = command else {
    return Err(error(
      "INVALID_ARGUMENT",
      "Usage: hivex sources | read <id> [options]",
    ));
  };
  if command != "sources" && command != "read" {
    return Err(error(
      "INVALID_ARGUMENT",
      "Usage: hivex sources | read <id> [options]",
    ));
  }
  if command == "sources" && (id.is_some() || extra.is_some()) {
    return Err(error(
      "INVALID_ARGUMENT",
      "sources does not accept a source id",
    ));
  }
  if command == "read" && id.is_none() {
    return Err(error("INVALID_ARGUMENT", "read requires a source id"));
  }
  if command == "read" && extra.is_some() {
    return Err(error("INVALID_ARGUMENT", "read accepts one source id"));
  }
  Ok((command, id))
}
