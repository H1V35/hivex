use super::relations::{Section, sections};
use super::{Document, Project, line_content, raw_line_ranges};
use crate::error::{HivexError, Result};
use rusqlite::{Connection, params};
use serde::Serialize;
use std::collections::HashSet;
use std::ops::Range;
use unicode_normalization::UnicodeNormalization;

const WINDOW_LINES: usize = 32;
const MAX_CONTEXT_BYTES: usize = 8_192;
const MAX_PASSAGES: usize = 32_768;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
  pub line_start: usize,
  pub line_end: usize,
  pub complete: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Match {
  pub document: String,
  pub version: String,
  pub historical: bool,
  pub line_start: usize,
  pub line_end: usize,
  pub text: String,
  pub context: Context,
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

fn context(sections: &[Section], bytes: &Range<usize>) -> (usize, usize, Range<usize>) {
  let first = sections.partition_point(|section| section.bytes.end <= bytes.start);
  let last = sections.partition_point(|section| section.bytes.start < bytes.end);
  let end = sections[first..last]
    .iter()
    .max_by_key(|section| section.line_end)
    .expect("a passage intersects a source section");
  (
    sections[first].line_start,
    end.line_end,
    sections[first].bytes.start..end.context_end,
  )
}

fn index_document(
  statement: &mut rusqlite::Statement<'_>,
  source: (usize, &Document),
  count: &mut usize,
) -> Result<()> {
  let (index, document) = source;
  let sections = sections(&document.text)?;
  let mut lines = raw_line_ranges(&document.text).enumerate();
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
    *count += 1;
    if *count > MAX_PASSAGES {
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
    let (context_first, context_last, context_bytes) = context(&sections, &(range.start..end));
    statement.execute(params![
      index,
      first + 1,
      last + 1,
      range.start,
      end,
      context_first,
      context_last,
      context_bytes.start,
      context_bytes.end,
      context_bytes.len() <= MAX_CONTEXT_BYTES,
      content
    ])?;
  }
  Ok(())
}

fn index(database: &mut Connection, documents: &[&Document]) -> Result<()> {
  database.execute_batch("PRAGMA page_size=4096; PRAGMA max_page_count=32768; CREATE VIRTUAL TABLE passages USING fts5(document UNINDEXED,first_line UNINDEXED,last_line UNINDEXED,first_byte UNINDEXED,last_byte UNINDEXED,context_first UNINDEXED,context_last UNINDEXED,context_byte UNINDEXED,context_end UNINDEXED,expand UNINDEXED,content,tokenize=unicode61)")?;
  let transaction = database.transaction()?;
  let mut insert = transaction.prepare("INSERT INTO passages(document,first_line,last_line,first_byte,last_byte,context_first,context_last,context_byte,context_end,expand,content) VALUES(?,?,?,?,?,?,?,?,?,?,?)")?;
  let mut count = 0;
  for (index, document) in documents.iter().enumerate() {
    index_document(&mut insert, (index, document), &mut count)?;
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
  let total: usize = database.query_row("SELECT count(*) FROM (SELECT document,CASE WHEN expand=1 THEN context_first ELSE first_line END AS first,CASE WHEN expand=1 THEN context_last ELSE last_line END AS last FROM passages WHERE passages MATCH ? GROUP BY document,first,last)", [&expression], |row| row.get(0))?;
  let mut statement = database.prepare("WITH hits AS MATERIALIZED (SELECT *,bm25(passages) AS score FROM passages WHERE passages MATCH ?), ranked AS (SELECT *,row_number() OVER(PARTITION BY document,CASE WHEN expand=1 THEN context_first ELSE first_line END,CASE WHEN expand=1 THEN context_last ELSE last_line END ORDER BY score,first_line) AS position FROM hits) SELECT document,CASE WHEN expand=1 THEN context_first ELSE first_line END,CASE WHEN expand=1 THEN context_last ELSE last_line END,CASE WHEN expand=1 THEN context_byte ELSE first_byte END,CASE WHEN expand=1 THEN context_end ELSE last_byte END,context_first,context_last FROM ranked WHERE position=1 ORDER BY score,document,first_line LIMIT ? OFFSET ?")?;
  let ranges = statement
    .query_map(params![expression, limit, offset], |row| {
      Ok((
        row.get::<_, usize>(0)?,
        row.get::<_, usize>(1)?,
        row.get::<_, usize>(2)?,
        row.get::<_, usize>(3)?,
        row.get::<_, usize>(4)?,
        row.get::<_, usize>(5)?,
        row.get::<_, usize>(6)?,
      ))
    })?
    .collect::<rusqlite::Result<Vec<_>>>()?;
  let matches = ranges
    .into_iter()
    .map(
      |(index, first, last, start, end, context_first, context_last)| {
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
          context: Context {
            line_start: context_first,
            line_end: context_last,
            complete: first == context_first && last == context_last,
          },
        }
      },
    )
    .collect();
  Ok(SearchResult { matches, total })
}
