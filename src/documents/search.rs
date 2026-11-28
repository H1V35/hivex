use super::navigation::{AuthoredRelation, declarations};
use super::relations::{RelationKind, Section, enclosing, sections};
use super::{Document, Project, line_content, raw_line_ranges};
use crate::error::{HiveXError, Result};
use regex::Regex;
use rusqlite::{Connection, Row, Statement, params};
use serde::Serialize;
use std::collections::HashSet;
use std::ops::Range;
use std::sync::OnceLock;
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

const WINDOW_LINES: usize = 32;
const MAX_CONTEXT_BYTES: usize = 8_192;
const MAX_PASSAGES: usize = 32_768;
const EXCERPT_LINES: usize = 3;
const EXCERPT_BYTES: usize = 512;

const SCHEMA: &str = "
  PRAGMA page_size=4096;
  PRAGMA max_page_count=32768;
  CREATE VIRTUAL TABLE passages USING fts5(
    document UNINDEXED, first_line UNINDEXED, last_line UNINDEXED,
    first_byte UNINDEXED, last_byte UNINDEXED,
    context_first UNINDEXED, context_last UNINDEXED,
    context_byte UNINDEXED, context_end UNINDEXED, expand UNINDEXED,
    content, tokenize=unicode61
  )";

const INSERT: &str = "
  INSERT INTO passages(document, first_line, last_line, first_byte, last_byte,
    context_first, context_last, context_byte, context_end, expand, content)
  VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";

// A passage whose structural context fits is reported as that context, so
// distinct windows that expand to the same range are returned once.
const COUNT: &str = "
  SELECT count(*) FROM (
    SELECT document,
      CASE WHEN expand=1 THEN context_first ELSE first_line END AS first,
      CASE WHEN expand=1 THEN context_last ELSE last_line END AS last
    FROM passages WHERE passages MATCH ?
    GROUP BY document, first, last
  )";

const PAGE: &str = "
  WITH hits AS MATERIALIZED (
    SELECT document, first_line, last_line, first_byte, last_byte,
      context_first, context_last, context_byte, context_end, expand,
      bm25(passages) AS score
    FROM passages WHERE passages MATCH ?
  ), ranked AS (
    SELECT *, row_number() OVER (
      PARTITION BY document,
        CASE WHEN expand=1 THEN context_first ELSE first_line END,
        CASE WHEN expand=1 THEN context_last ELSE last_line END
      ORDER BY score, first_line
    ) AS position
    FROM hits
  )
  SELECT document,
    CASE WHEN expand=1 THEN context_first ELSE first_line END,
    CASE WHEN expand=1 THEN context_last ELSE last_line END,
    CASE WHEN expand=1 THEN context_byte ELSE first_byte END,
    CASE WHEN expand=1 THEN context_end ELSE last_byte END,
    context_first, context_last
  FROM ranked WHERE position=1
  ORDER BY score, document, first_line
  LIMIT ? OFFSET ?";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
  pub line_start: usize,
  pub line_end: usize,
  /// Whether the match already is the whole structural context.
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

/// Where a match is, without its text: what a locating search returns.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
  pub document: String,
  pub version: String,
  pub historical: bool,
  pub line_start: usize,
  pub line_end: usize,
  /// The headings enclosing the excerpt, outermost first.
  pub headings: Vec<String>,
  /// The lines of the innermost of those headings' sections, through its subsections.
  pub section: Lines,
  pub excerpt: Excerpt,
  /// The match's structural range; without text, it never claims to be complete.
  pub context: Lines,
  pub relations: Qualifiers,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lines {
  pub line_start: usize,
  pub line_end: usize,
}

/// Up to three whole lines of a match, from its line with the most query terms.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Excerpt {
  pub line_start: usize,
  pub line_end: usize,
  pub text: String,
}

/// The kinds of authored relationship the matched document declares, and those declared
/// towards a range that overlaps the match's section.
#[derive(Serialize)]
pub struct Qualifiers {
  pub outgoing: Vec<RelationKind>,
  pub incoming: Vec<RelationKind>,
}

pub struct Query<'a> {
  pub text: &'a str,
  /// Restrict the search to these documents, including archived ones.
  pub sources: &'a [String],
  pub historical: bool,
}

pub struct SearchResult {
  pub matches: Vec<Match>,
  pub total: usize,
}

/// Distinct lowercase NFKC letter/number terms, in query order.
fn search_terms(text: &str) -> Vec<String> {
  static TERM: OnceLock<Regex> = OnceLock::new();
  let term = TERM.get_or_init(|| Regex::new(r"[\p{L}\p{N}]+").expect("valid term expression"));
  let normalized: String = text.to_lowercase().nfkc().collect();
  let mut seen = HashSet::new();
  term
    .find_iter(&normalized)
    .map(|found| found.as_str())
    .filter(|term| seen.insert(*term))
    .map(str::to_owned)
    .collect()
}

/// Rank nonoverlapping 32-line windows with FTS5 BM25, OR-ing the query terms.
pub fn search(
  project: &Project,
  query: &Query<'_>,
  offset: usize,
  limit: usize,
) -> Result<SearchResult> {
  if let Some(source) = project.missing(query.sources) {
    return Err(
      HiveXError::new("SOURCE_NOT_FOUND", "Search requires selected source IDs")
        .with_details(serde_json::json!({"document":source})),
    );
  }
  let documents: Vec<_> = project
    .documents
    .iter()
    .filter(|document| match query.sources {
      [] => query.historical || !document.historical,
      sources => sources.contains(&document.id),
    })
    .collect();
  let terms = search_terms(query.text);
  if terms.is_empty() {
    return Ok(SearchResult {
      matches: Vec::new(),
      total: 0,
    });
  }
  let expression = terms
    .iter()
    .map(|term| format!("\"{term}\""))
    .collect::<Vec<_>>()
    .join(" OR ");
  let mut database = Connection::open_in_memory()?;
  index(&mut database, &documents)?;
  let total = database.query_row(COUNT, [&expression], |row| row.get(0))?;
  let matches = database
    .prepare(PAGE)?
    .query_map(params![expression, limit, offset], Hit::from_row)?
    .map(|hit| Ok(hit?.into_match(&documents)))
    .collect::<Result<_>>()?;
  Ok(SearchResult { matches, total })
}

struct Hit {
  document: usize,
  lines: (usize, usize),
  bytes: Range<usize>,
  context: (usize, usize),
}

impl Hit {
  fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
    Ok(Self {
      document: row.get(0)?,
      lines: (row.get(1)?, row.get(2)?),
      bytes: row.get(3)?..row.get(4)?,
      context: (row.get(5)?, row.get(6)?),
    })
  }

  fn into_match(self, documents: &[&Document]) -> Match {
    let document = documents[self.document];
    let text = exact(document, self.bytes.clone());
    Match {
      document: document.id.clone(),
      version: document.hash.clone(),
      historical: document.historical,
      line_start: self.lines.0,
      line_end: self.lines.1,
      text: text.to_owned(),
      context: Context {
        line_start: self.context.0,
        line_end: self.context.1,
        complete: self.lines == self.context,
      },
    }
  }
}

/// Source text of whole lines as `read` returns it: only the source's final line keeps its
/// line ending.
fn exact(document: &Document, bytes: Range<usize>) -> &str {
  let text = &document.text[bytes.clone()];
  if bytes.end == document.text.len() {
    text
  } else {
    line_content(text)
  }
}

/// Locate `matches` without copying their text: coordinates, headings, an excerpt and the
/// kinds of authored relationship around each, resolved over the coverage `relations` uses.
pub fn locate(project: &Project, query: &str, matches: Vec<Match>) -> Result<Vec<Location>> {
  if matches.is_empty() {
    return Ok(Vec::new());
  }
  let documents: Vec<&str> = matches
    .iter()
    .map(|found| found.document.as_str())
    .collect();
  let declarations = declarations(project, &documents)?;
  let terms = folded_terms(query);
  matches
    .into_iter()
    .map(|found| {
      let document = project
        .document(&found.document)
        .expect("a match comes from a selected document");
      let excerpt = excerpt(document, &found, &terms);
      let (headings, (line_start, line_end)) = enclosing(&document.text, excerpt.line_start)?;
      let section = Lines {
        line_start,
        line_end,
      };
      Ok(Location {
        relations: qualifiers(&declarations, &found.document, &section),
        headings,
        section,
        document: found.document,
        version: found.version,
        historical: found.historical,
        line_start: found.line_start,
        line_end: found.line_end,
        excerpt,
        context: Lines {
          line_start: found.context.line_start,
          line_end: found.context.line_end,
        },
      })
    })
    .collect()
}

/// Letter/number terms folded as the unicode61 tokenizer folds diacritics, near enough to find
/// the line of a match that holds most of the query.
fn folded_terms(text: &str) -> HashSet<String> {
  let folded: String = text
    .to_lowercase()
    .nfkd()
    .filter(|character| !is_combining_mark(*character))
    .collect();
  search_terms(&folded).into_iter().collect()
}

/// From the first nonblank line with the most distinct query terms, up to three whole lines of
/// the match within 512 bytes; the first line is always whole, and trailing blank lines are left.
fn excerpt(document: &Document, found: &Match, terms: &HashSet<String>) -> Excerpt {
  let lines: Vec<_> = raw_line_ranges(&document.text)
    .skip(found.line_start - 1)
    .take(found.line_end + 1 - found.line_start)
    .collect();
  let line = |index: usize| &document.text[lines[index].clone()];
  let scores: Vec<_> = (0..lines.len())
    .map(|index| {
      let found = folded_terms(line(index)).intersection(terms).count();
      (found, !line(index).trim().is_empty())
    })
    .collect();
  let best = scores.iter().max().map_or(0, |top| {
    scores
      .iter()
      .position(|score| score == top)
      .expect("the maximum is a score")
  });
  let limit = lines.len().min(best + EXCERPT_LINES);
  let mut last = best;
  while last + 1 < limit && lines[last + 1].end - lines[best].start <= EXCERPT_BYTES {
    last += 1;
  }
  while last > best && line(last).trim().is_empty() {
    last -= 1;
  }
  Excerpt {
    line_start: found.line_start + best,
    line_end: found.line_start + last,
    text: exact(document, lines[best].start..lines[last].end).to_owned(),
  }
}

/// The relationship kinds around a section: those its document declares, and those declared
/// towards a range overlapping it by an ordinary source or by the document itself.
fn qualifiers(declarations: &[AuthoredRelation], document: &str, section: &Lines) -> Qualifiers {
  let kinds = |selected: &dyn Fn(&AuthoredRelation) -> bool| {
    RelationKind::ALL
      .into_iter()
      .filter(|kind| {
        declarations
          .iter()
          .any(|relation| relation.kind == *kind && selected(relation))
      })
      .collect()
  };
  Qualifiers {
    outgoing: kinds(&|relation| relation.from.document == document),
    incoming: kinds(&|relation| {
      relation.to.document == document
        && (!relation.from.historical || relation.from.document == document)
        && relation.to.line_start <= section.line_end
        && section.line_start <= relation.to.line_end
    }),
  }
}

fn index(database: &mut Connection, documents: &[&Document]) -> Result<()> {
  database.execute_batch(SCHEMA)?;
  let transaction = database.transaction()?;
  let mut insert = transaction.prepare(INSERT)?;
  let mut count = 0;
  for (index, document) in documents.iter().enumerate() {
    index_document(&mut insert, (index, document), &mut count)?;
  }
  drop(insert);
  transaction.commit()?;
  Ok(())
}

fn index_document(
  insert: &mut Statement<'_>,
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
        HiveXError::new(
          "SEARCH_LIMIT",
          "At most 32768 source passages can be indexed; narrow selected sources",
        )
        .with_details(
          serde_json::json!({"document":document.id,"line":first+1,"limit":MAX_PASSAGES}),
        ),
      );
    }
    let (context_first, context_last, context_bytes) = context(&sections, &(range.start..end));
    insert.execute(params![
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

/// The lines and bytes of the sections a passage touches, through their subsections.
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
