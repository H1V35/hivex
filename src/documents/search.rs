use super::{Document, Project, line_content, raw_line_ranges};
use crate::error::{HivexError, Result};
use rusqlite::{Connection, params};
use serde::Serialize;
use std::collections::HashSet;
use unicode_normalization::UnicodeNormalization;

const WINDOW_LINES: usize = 32;
const MAX_PASSAGES: usize = 32_768;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Match {
  pub document: String,
  pub version: String,
  pub historical: bool,
  pub line_start: usize,
  pub line_end: usize,
  pub text: String,
}

pub struct SearchResult {
  pub matches: Vec<Match>,
  pub total: usize,
}

pub fn search_terms(text: &str) -> Vec<String> {
  let normalized: String = text.to_lowercase().nfkc().collect();
  let expression = regex::Regex::new(r"[\p{L}\p{N}]+").expect("valid Unicode expression");
  let mut seen = HashSet::new();
  expression
    .find_iter(&normalized)
    .map(|item| item.as_str().to_owned())
    .filter(|term| seen.insert(term.clone()))
    .collect()
}

fn index(database: &mut Connection, documents: &[&Document]) -> Result<()> {
  database.execute_batch("PRAGMA page_size=4096; PRAGMA max_page_count=32768; CREATE VIRTUAL TABLE passages USING fts5(document UNINDEXED, first_line UNINDEXED, last_line UNINDEXED, first_byte UNINDEXED, last_byte UNINDEXED, content, tokenize=unicode61)")?;
  let transaction = database.transaction()?;
  let mut insert = transaction.prepare(
    "INSERT INTO passages(document,first_line,last_line,first_byte,last_byte,content) VALUES(?,?,?,?,?,?)",
  )?;
  let mut count = 0;
  for (document_index, document) in documents.iter().enumerate() {
    let mut lines = raw_line_ranges(&document.text).enumerate();
    // ponytail: fixed source windows; use section windows only if retrieval measurements justify them.
    while let Some((first, range)) = lines.next() {
      let (last, end) = lines
        .by_ref()
        .take(WINDOW_LINES - 1)
        .last()
        .map_or((first, range.end), |(line, range)| (line, range.end));
      let content = &document.text[range.start..end];
      if content.trim().is_empty() {
        continue;
      }
      count += 1;
      if count > MAX_PASSAGES {
        return Err(
          HivexError::new(
            "SEARCH_LIMIT",
            "At most 32768 source passages can be indexed; narrow selected sources",
          )
          .with_details(
            serde_json::json!({"document":document.id,"line":first+1,"limit":MAX_PASSAGES}),
          ),
        );
      }
      insert.execute(params![
        document_index,
        first + 1,
        last + 1,
        range.start,
        end,
        content
      ])?;
    }
  }
  drop(insert);
  transaction.commit()?;
  Ok(())
}

pub fn search(
  project: &Project,
  query: &str,
  selection: (&[String], bool),
  page: (usize, usize),
) -> Result<SearchResult> {
  let (sources, historical) = selection;
  let (offset, limit) = page;
  for source in sources {
    if project
      .documents
      .iter()
      .all(|document| document.id != *source)
    {
      return Err(
        HivexError::new("SOURCE_NOT_FOUND", "Search requires selected source IDs")
          .with_details(serde_json::json!({"document":source})),
      );
    }
  }
  let documents: Vec<_> = project
    .documents
    .iter()
    .filter(|document| {
      if !sources.is_empty() {
        return sources.contains(&document.id);
      }
      historical || !document.historical
    })
    .collect();
  let expression = search_terms(query)
    .iter()
    .map(|term| format!("\"{term}\""))
    .collect::<Vec<_>>()
    .join(" OR ");
  if expression.is_empty() {
    return Ok(SearchResult {
      matches: Vec::new(),
      total: 0,
    });
  }
  let mut database = Connection::open_in_memory()?;
  index(&mut database, &documents)?;
  let total: usize = database.query_row(
    "SELECT count(*) FROM passages WHERE passages MATCH ?",
    [&expression],
    |row| row.get(0),
  )?;
  let mut statement = database.prepare("SELECT document,first_line,last_line,first_byte,last_byte FROM passages WHERE passages MATCH ? ORDER BY bm25(passages), document, first_line LIMIT ? OFFSET ?")?;
  let ranges = statement
    .query_map(params![expression, limit, offset], |row| {
      Ok((
        row.get::<_, usize>(0)?,
        row.get::<_, usize>(1)?,
        row.get::<_, usize>(2)?,
        row.get::<_, usize>(3)?,
        row.get::<_, usize>(4)?,
      ))
    })?
    .collect::<rusqlite::Result<Vec<_>>>()?;
  let matches = ranges
    .into_iter()
    .map(|(index, first, last, start, end)| {
      let document = documents[index];
      Match {
        document: document.id.clone(),
        version: document.hash.clone(),
        historical: document.historical,
        line_start: first,
        line_end: last,
        text: if end == document.text.len() {
          document.text[start..end].to_owned()
        } else {
          line_content(&document.text[start..end]).to_owned()
        },
      }
    })
    .collect();
  Ok(SearchResult { matches, total })
}
