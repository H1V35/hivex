use super::arguments::{Arguments, invalid};
use super::page::{self, Cursor};
use super::{ORIGIN, coverage};
use crate::documents::{hash, load_project, validate_sources};
use crate::error::{HivexError, Result};
use serde_json::{Value, json};

/// `check`: structural validation of metadata, relationships and local references.
pub fn command(args: &[String]) -> Result<Value> {
  let arguments = Arguments::parse(
    args,
    &["cursor", "limit", "max-bytes", "root", "source"],
    &["historical"],
  )?;
  if arguments.positionals().len() != 1 {
    return Err(invalid(
      "Use check [--source <document>] [--historical] [--limit <count>] [--max-bytes <bytes>] [--cursor <continuation>] [--root <project>]",
    ));
  }
  let limit = arguments.count("limit", 20, 2_048)?;
  let max_bytes = page::max_bytes(&arguments)?;
  let project = load_project(arguments.root())?;
  let sources = arguments.sources();
  let historical = arguments.flag("historical");
  let key = hash(&json!([project.snapshot, sources, historical, limit, max_bytes]).to_string());
  let cursor = Cursor {
    version: "v2",
    key: &key,
  };
  let start = cursor.offset(arguments.value("cursor"))?;
  let validation = validate_sources(&project, &sources, historical)?;
  let total = validation.findings.len();
  page::check_offset(start, total)?;
  let status = match (total, project.is_partial()) {
    (0, false) => "ready",
    (0, true) => "partial",
    _ => "failed",
  };
  page::fill(
    validation.findings.iter().skip(start).take(limit),
    max_bytes,
    |findings| {
      json!({
        "command": "check",
        "modelCalls": 0,
        "origin": ORIGIN,
        "snapshot": project.snapshot,
        "status": status,
        "coverage": coverage(&project),
        "sources": sources,
        "historical": historical,
        "checkedDocuments": validation.checked,
        "checkedMetadataDocuments": validation.checked_metadata,
        "checkedReferenceDocuments": validation.checked_references,
        "totalFindings": total,
        "warnings": project.warnings,
        "scope": "outer metadata under docs, authored relation syntax and selected local Markdown references; historical bodies require explicit selection; no semantic certification",
        "findings": findings,
        "continuation": cursor.next(start + findings.len(), total),
      })
    },
    |_| {
      HivexError::new(
        "OUTPUT_LIMIT",
        "The next complete validation finding exceeds --max-bytes",
      )
    },
  )
}
