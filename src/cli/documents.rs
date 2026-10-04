use super::ORIGIN;
use super::arguments::{Arguments, MAX_SAFE_INTEGER, invalid};
use super::page::{self, Cursor};
use crate::documents::{
  Document, MAX_DOCUMENTS, Project, line_content, load_project, raw_line_ranges,
};
use crate::error::{HivexError, Result};
use serde_json::{Value, json};

fn metadata(document: &Document) -> Value {
  json!({
    "hash": document.hash,
    "historical": document.historical,
    "id": document.id,
    "links": document.links,
    "path": document.id,
    "status": document.status,
    "title": document.title,
  })
}

/// Warnings as `sources` and `read` have always serialized them.
fn warnings(project: &Project) -> Vec<Value> {
  let warnings = project.warnings.iter();
  warnings
    .map(|warning| json!({"message": warning.message, "path": warning.path}))
    .collect()
}

/// `sources`: page through the selected documents' metadata.
pub fn sources(args: &[String]) -> Result<Value> {
  let arguments = Arguments::parse(args, &["cursor", "limit", "max-bytes", "root"], &[])?;
  if arguments.positionals().len() != 1 {
    return Err(invalid("sources does not accept a source id"));
  }
  let limit = arguments
    .count("limit", 20, MAX_SAFE_INTEGER)?
    .min(MAX_DOCUMENTS);
  let max_bytes = page::max_bytes(&arguments)?;
  let project = load_project(arguments.root())?;
  let cursor = Cursor {
    version: "s1",
    key: &project.snapshot,
  };
  let start = cursor.offset(arguments.value("cursor"))?;
  let total = project.documents.len();
  page::check_offset(start, total)?;
  let records = project.documents.iter().skip(start).take(limit);
  let warnings = warnings(&project);
  page::fill(
    records.map(metadata),
    max_bytes,
    |documents| {
      json!({
        "command": "sources",
        "continuation": cursor.next(start + documents.len(), total),
        "documents": documents,
        "origin": ORIGIN,
        "snapshot": project.snapshot,
        "totalDocuments": total,
        "warnings": warnings,
      })
    },
    |_| {
      HivexError::new(
        "OUTPUT_LIMIT",
        "The next source metadata does not fit; increase --max-bytes or narrow the selected sources",
      )
    },
  )
}

/// `read`: exact source text for a line range, cut on line boundaries to `--max-bytes`.
pub fn read(args: &[String]) -> Result<Value> {
  let arguments = Arguments::parse(args, &["from", "max-bytes", "root", "to"], &[])?;
  let [_, id] = arguments.positionals() else {
    return Err(invalid("read requires one source id"));
  };
  let from = arguments.optional_count("from", MAX_SAFE_INTEGER)?;
  let to = arguments.optional_count("to", MAX_SAFE_INTEGER)?;
  let max_bytes = page::max_bytes(&arguments)?;
  let project = load_project(arguments.root())?;
  let source = project.document(id).ok_or_else(|| {
    HivexError::new(
      "SOURCE_NOT_FOUND",
      format!("Markdown source was not selected: {id}"),
    )
    .with_details(json!({"id": id}))
  })?;
  let line_count = raw_line_ranges(&source.text).count();
  let start = from.unwrap_or(1);
  let end = to.unwrap_or(line_count);
  if start > end || end > line_count {
    return Err(
      HivexError::new(
        "INVALID_RANGE",
        format!("Line range {start}-{end} is outside the source"),
      )
      .with_details(json!({"id": id, "lineCount": line_count})),
    );
  }
  let (line_end, text) = excerpt(&source.text, (start, end), line_count, max_bytes)?;
  let continuation = (line_end < line_count).then(|| {
    json!({
      "from": line_end + 1,
      "maxBytes": max_bytes,
      "reason": if line_end < end { "max-bytes" } else { "range" },
      "to": line_count,
    })
  });
  let truncated = continuation.is_some();
  Ok(json!({
    "command": "read",
    "continuation": continuation,
    "lineEnd": line_end,
    "lineStart": start,
    "origin": ORIGIN,
    "snapshot": project.snapshot,
    "source": metadata(source),
    "text": text,
    "truncated": truncated,
    "warnings": warnings(&project),
  }))
}

/// The longest prefix of lines `start..=end` fitting `max_bytes`, as its last
/// line number and exact source slice. Separators between lines are kept; the
/// last returned line keeps its ending only when it ends the source.
fn excerpt(
  text: &str,
  (start, end): (usize, usize),
  line_count: usize,
  max_bytes: usize,
) -> Result<(usize, &str)> {
  let first_byte = raw_line_ranges(text)
    .nth(start - 1)
    .map_or(0, |range| range.start);
  let mut included = None;
  for (line, range) in (start..=end).zip(raw_line_ranges(text).skip(start - 1)) {
    let last_byte = if line == line_count {
      range.end
    } else {
      range.start + line_content(&text[range]).len()
    };
    let bytes = last_byte - first_byte;
    if bytes > max_bytes {
      let Some(included) = included else {
        return Err(
          HivexError::new(
            "OUTPUT_LIMIT",
            "The first requested line exceeds --max-bytes",
          )
          .with_details(json!({"line": line, "maxBytes": max_bytes, "requiredBytes": bytes})),
        );
      };
      return Ok(included);
    }
    included = Some((line, &text[first_byte..last_byte]));
  }
  Ok(included.expect("a valid range has at least one line"))
}
