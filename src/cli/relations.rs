use super::arguments::{Arguments, invalid};
use super::page::{self, Cursor};
use super::{ORIGIN, coverage, status};
use crate::documents::{AuthoredRelation, Direction, authored_relations, hash, load_project};
use crate::error::{HivexError, Result};
use serde::Serialize;
use serde_json::{Value, json};

/// A relation with its direction relative to the queried document.
#[derive(Serialize)]
struct Navigated<'a> {
  #[serde(flatten)]
  relation: &'a AuthoredRelation,
  navigation: &'static str,
}

/// `relations`: authored relationships declared by or targeting one document.
pub fn command(args: &[String]) -> Result<Value> {
  let arguments = Arguments::parse(
    args,
    &["cursor", "direction", "limit", "max-bytes", "root"],
    &[],
  )?;
  let [_, document] = arguments.positionals() else {
    return Err(usage());
  };
  if document.contains('#') {
    return Err(usage());
  }
  let direction = Direction::parse(arguments.value("direction").unwrap_or("both"))
    .ok_or_else(|| invalid("--direction must be outgoing, incoming or both"))?;
  let limit = arguments.count("limit", 20, 2_048)?;
  let max_bytes = page::max_bytes(&arguments)?;
  let project = load_project(arguments.root())?;
  let relations = authored_relations(&project, document, direction)?;
  let key = hash(&json!([project.snapshot, document, direction, limit, max_bytes]).to_string());
  let cursor = Cursor {
    version: "r1",
    key: &key,
  };
  let start = cursor.offset(arguments.value("cursor"))?;
  let total = relations.len();
  page::check_offset(start, total)?;
  let records = relations.iter().skip(start).take(limit).map(|relation| {
    let navigation = match (
      relation.from.document == *document,
      relation.to.document == *document,
    ) {
      (true, true) => "self",
      (true, false) => "outgoing",
      _ => "incoming",
    };
    Navigated {
      relation,
      navigation,
    }
  });
  page::fill(
    records,
    max_bytes,
    |relations| {
      json!({
        "command": "relations",
        "modelCalls": 0,
        "origin": ORIGIN,
        "snapshot": project.snapshot,
        "document": document,
        "direction": direction,
        "scope": "document; declared direction is preserved; incoming considers ordinary sources and the queried source",
        "status": status(&project),
        "coverage": coverage(&project),
        "relations": relations,
        "totalRelations": total,
        "warnings": project.warnings,
        "continuation": cursor.next(start + relations.len(), total),
      })
    },
    |_| {
      HivexError::new(
        "OUTPUT_LIMIT",
        "The next complete relation exceeds --max-bytes; increase it within 65536",
      )
    },
  )
}

fn usage() -> HivexError {
  invalid(
    "Use relations <document> [--direction outgoing|incoming|both] [--limit <count>] [--max-bytes <bytes>] [--cursor <continuation>] [--root <project>]",
  )
}
