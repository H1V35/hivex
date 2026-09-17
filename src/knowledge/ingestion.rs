use crate::documents::{Document, Warning};
use crate::documents::{hash, line_content, raw_markdown_lines};
use crate::error::{HivexError, Result};
use crate::knowledge::model::{Citation, Decision, valid_citation};
use serde::{Deserialize, Serialize};

const MAX_BYTES: usize = 8_192;
const MAX_REPAIR_BYTES: usize = 2 * MAX_BYTES;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestionUnit {
  pub document: String,
  pub hash: String,
  pub id: String,
  pub line_end: usize,
  pub line_start: usize,
  pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IngestionResult {
  pub units: Vec<IngestionUnit>,
  pub warnings: Vec<Warning>,
}

pub(crate) use crate::documents::RepairRange;

#[derive(Clone, Debug)]
struct Fence {
  marker: char,
  length: usize,
  closing: bool,
}

#[derive(Clone, Debug)]
struct SourceLine {
  blank: bool,
  bytes: usize,
  fence: Option<Fence>,
  heading: bool,
  number: usize,
  text: String,
}

fn error(code: &str, message: impl Into<String>) -> HivexError {
  HivexError::new(code, message)
}

fn is_js_whitespace(character: char) -> bool {
  character == '\u{feff}' || (character.is_whitespace() && character != '\u{85}')
}

fn js_trim(value: &str) -> &str {
  crate::compatibility::trim_js_whitespace(value)
}

fn fence_of(content: &str) -> Option<Fence> {
  let indentation = content.bytes().take_while(|byte| *byte == b' ').count();
  if indentation > 3 {
    return None;
  }
  let source = &content[indentation..];
  let marker = source.chars().next()?;
  if marker != '`' && marker != '~' {
    return None;
  }
  let length = source
    .chars()
    .take_while(|character| *character == marker)
    .count();
  if length < 3 {
    return None;
  }
  let marker_bytes = marker.len_utf8() * length;
  Some(Fence {
    closing: js_trim(&source[marker_bytes..]).is_empty(),
    length,
    marker,
  })
}

fn is_heading(content: &str) -> bool {
  // Markdown headings allow zero to three spaces followed by one to six
  // hashes and then either whitespace or the end of the line. Keep the
  // scan byte-safe while avoiding a regular expression in this hot path.
  let mut offset = 0;
  let mut whitespace = 0;
  for character in content.chars() {
    if !(is_js_whitespace(character)) {
      break;
    }
    whitespace += 1;
    if whitespace > 3 {
      return false;
    }
    offset += character.len_utf8();
  }
  let trimmed = &content[offset..];
  let hash_count = trimmed
    .chars()
    .take_while(|character| *character == '#')
    .count();
  if !(1..=6).contains(&hash_count) {
    return false;
  }
  trimmed.chars().nth(hash_count).is_none_or(is_js_whitespace)
}

fn source_lines(text: &str) -> Vec<SourceLine> {
  raw_markdown_lines(text)
    .into_iter()
    .filter(|line| !line.is_empty())
    .enumerate()
    .map(|(index, line)| {
      let content = line_content(&line);
      SourceLine {
        blank: js_trim(content).is_empty(),
        bytes: line.len(),
        fence: fence_of(content),
        heading: is_heading(content),
        number: index + 1,
        text: line,
      }
    })
    .collect()
}

fn blocks_for(document: &Document, warnings: &mut Vec<Warning>) -> Vec<Vec<SourceLine>> {
  let mut blocks = Vec::new();
  let mut block = Vec::new();
  let mut active_fence: Option<Fence> = None;
  let flush = |blocks: &mut Vec<Vec<SourceLine>>, block: &mut Vec<SourceLine>| {
    if !block.is_empty() {
      blocks.push(std::mem::take(block));
    }
  };

  for line in source_lines(&document.text) {
    let is_in_fence = active_fence.is_some();
    let is_same_fence = active_fence.as_ref().is_some_and(|active| {
      line.fence.as_ref().is_some_and(|candidate| {
        candidate.marker == active.marker && candidate.length >= active.length
      })
    });
    let is_closing_fence = line
      .fence
      .as_ref()
      .is_some_and(|candidate| candidate.closing)
      && is_same_fence;
    match (is_closing_fence, active_fence.is_none()) {
      (true, _) => active_fence = None,
      (false, true) => active_fence.clone_from(&line.fence),
      (false, false) => {}
    }

    if line.bytes > MAX_BYTES {
      flush(&mut blocks, &mut block);
      warnings.push(Warning {
        message: format!(
          "Line {} is {} UTF-8 bytes, exceeding the {}-byte limit; omitted as unread.",
          line.number, line.bytes, MAX_BYTES
        ),
        path: document.path.clone(),
      });
      continue;
    }

    if !is_in_fence && line.heading {
      flush(&mut blocks, &mut block);
    }
    block.push(line);
    if active_fence.is_none() && (block.last().is_some_and(|line| line.blank) || is_closing_fence) {
      flush(&mut blocks, &mut block);
    }
  }
  flush(&mut blocks, &mut block);
  blocks
}

fn split_block(block: &[SourceLine]) -> Vec<Vec<SourceLine>> {
  let mut pieces = Vec::new();
  let mut piece = Vec::new();
  let mut bytes = 0;
  for line in block {
    if !piece.is_empty() && bytes + line.bytes > MAX_BYTES {
      pieces.push(std::mem::take(&mut piece));
      bytes = 0;
    }
    piece.push(line.clone());
    bytes += line.bytes;
  }
  if !piece.is_empty() {
    pieces.push(piece);
  }
  pieces
}

fn packed_blocks(blocks: &[Vec<SourceLine>]) -> Vec<Vec<SourceLine>> {
  let split_blocks = blocks.iter().flat_map(|block| split_block(block));
  let mut packed = Vec::new();
  let mut current = Vec::new();
  let mut bytes = 0;
  for block in split_blocks {
    let Some(first) = block.first() else {
      continue;
    };
    let block_bytes: usize = block.iter().map(|line| line.bytes).sum();
    let should_flush = current.last().is_none_or(|last: &SourceLine| {
      last.number.saturating_add(1) != first.number || bytes + block_bytes > MAX_BYTES
    });
    if should_flush && !current.is_empty() {
      packed.push(std::mem::take(&mut current));
      bytes = 0;
    }
    current.extend(block);
    bytes += block_bytes;
  }
  if !current.is_empty() {
    packed.push(current);
  }
  packed
}

fn make_unit(document: &Document, lines: &[SourceLine]) -> IngestionUnit {
  let first = lines
    .first()
    .expect("the block splitter must not create an empty unit");
  let last = lines
    .last()
    .expect("the block splitter must not create an empty unit");
  let text = lines
    .iter()
    .map(|line| line.text.as_str())
    .collect::<String>();
  IngestionUnit {
    document: document.id.clone(),
    hash: hash(&text),
    id: format!("{}:{}-{}", document.path, first.number, last.number),
    line_end: last.number,
    line_start: first.number,
    text,
  }
}

pub fn ingestion_units(documents: &[Document]) -> IngestionResult {
  let mut units = Vec::new();
  let mut warnings = Vec::new();
  for document in documents {
    let blocks = blocks_for(document, &mut warnings);
    for block in packed_blocks(&blocks) {
      units.push(make_unit(document, &block));
    }
  }
  IngestionResult { units, warnings }
}

pub fn unit_from_range(document: &Document, range: &RepairRange) -> Result<IngestionUnit> {
  let lines = source_lines(&document.text);
  let Some(start) = range.line_start.checked_sub(1) else {
    return Err(error(
      "INVALID_REPAIR_RANGE",
      "The repair range must contain source text.",
    ));
  };
  let Some(count) = range
    .line_end
    .checked_sub(range.line_start)
    .and_then(|count| count.checked_add(1))
  else {
    return Err(error(
      "INVALID_REPAIR_RANGE",
      "The repair range must contain source text.",
    ));
  };
  let selected = lines
    .iter()
    .skip(start)
    .take(count)
    .cloned()
    .collect::<Vec<_>>();
  if selected.is_empty() {
    return Err(error(
      "INVALID_REPAIR_RANGE",
      "The repair range must contain source text.",
    ));
  }
  let unit = make_unit(document, &selected);
  if unit.line_start != range.line_start || unit.line_end != range.line_end {
    return Err(error(
      "INVALID_REPAIR_RANGE",
      "The repair range must contain complete source lines.",
    ));
  }
  Ok(unit)
}

fn current_decision(decision: &Decision, document: &Document) -> bool {
  if decision.document != document.id || decision.version != document.hash {
    return false;
  }
  valid_citation(
    &Citation {
      document: decision.document.clone(),
      line_end: decision.line_end,
      line_start: decision.line_start,
      version: None,
    },
    std::slice::from_ref(document),
  )
}

fn expanded_ranges(
  document: &Document,
  decisions: &[Decision],
  requested: &[RepairRange],
) -> Vec<RepairRange> {
  let current = decisions
    .iter()
    .filter(|decision| current_decision(decision, document))
    .collect::<Vec<_>>();
  let mut ranges = requested.to_vec();
  loop {
    let mut changed = false;
    for range in &mut ranges {
      let overlaps = current.iter().filter(|decision| {
        decision.line_start <= range.line_end && decision.line_end >= range.line_start
      });
      let mut start = range.line_start;
      let mut end = range.line_end;
      for decision in overlaps {
        start = start.min(decision.line_start);
        end = end.max(decision.line_end);
      }
      changed |= start != range.line_start || end != range.line_end;
      range.line_start = start;
      range.line_end = end;
    }
    if !changed {
      break;
    }
  }

  ranges.sort_by_key(|range| range.line_start);
  let mut merged: Vec<RepairRange> = Vec::new();
  for range in ranges {
    if let Some(previous) = merged.last_mut()
      && previous.line_end >= range.line_start
    {
      previous.line_end = previous.line_end.max(range.line_end);
    } else {
      merged.push(range);
    }
  }
  merged
}

pub fn repair_units(
  documents: &[Document],
  decisions: &[Decision],
  ranges: &[RepairRange],
) -> Result<Vec<IngestionUnit>> {
  let mut units = Vec::new();
  for document in documents {
    let selected = ranges
      .iter()
      .filter(|range| range.document == document.id)
      .cloned()
      .collect::<Vec<_>>();
    for range in expanded_ranges(document, decisions, &selected) {
      units.push(unit_from_range(document, &range)?);
    }
  }
  Ok(units)
}

pub fn validate_repair_unit_size(units: &[IngestionUnit]) -> Result<()> {
  if units.iter().any(|unit| unit.text.len() > MAX_REPAIR_BYTES) {
    return Err(error(
      "REPAIR_RANGE_TOO_LARGE",
      "A complete decision range exceeds the 16 KiB round limit. Inspect its source scope before repairing it.",
    ));
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn document(path: &str, text: &str) -> Document {
    Document {
      id: path.to_owned(),
      path: path.to_owned(),
      title: path.to_owned(),
      text: text.to_owned(),
      hash: hash(text),
      status: None,
      links: Vec::new(),
      historical: false,
    }
  }

  #[test]
  fn preserves_small_text_and_raw_line_endings() {
    let source = document("notes.md", "First line\rSecond line\r\nThird line\n");
    let result = ingestion_units(&[source]);
    assert!(result.warnings.is_empty());
    assert_eq!(result.units.len(), 1);
    assert_eq!(
      result.units[0].text,
      "First line\rSecond line\r\nThird line\n"
    );
    assert_eq!(result.units[0].id, "notes.md:1-3");
    assert_eq!(result.units[0].hash, hash(&result.units[0].text));
  }

  #[test]
  fn splits_at_raw_utf8_bytes_and_omits_only_oversized_lines() {
    let line = "😀".repeat(2_000);
    let result = ingestion_units(&[document(
      "unicode.md",
      &format!("before\n{line}\n{line}\nafter\n"),
    )]);
    assert_eq!(result.warnings.len(), 0);
    assert!(result.units.len() >= 2);
    assert!(result.units.iter().all(|unit| unit.text.len() <= MAX_BYTES));

    let oversized = ingestion_units(&[document(
      "oversized.md",
      &format!("before\n{}\nafter\n", "x".repeat(MAX_BYTES + 1)),
    )]);
    assert_eq!(
      oversized
        .units
        .iter()
        .map(|unit| unit.text.as_str())
        .collect::<String>(),
      "before\nafter\n"
    );
    assert_eq!(oversized.units[0].line_start, 1);
    assert_eq!(oversized.units[1].line_start, 3);
    assert_eq!(oversized.warnings.len(), 1);
    assert!(oversized.warnings[0].message.contains("Line 2"));
  }

  #[test]
  fn keeps_fences_together_and_respects_headings() {
    let fence = "```ts\n# inside\n~~~\nstill inside\n```\n";
    let text = format!("# First\n\nparagraph\n\n{fence}\n# Second\n");
    let result = ingestion_units(&[document("fences.md", &text)]);
    assert!(result.warnings.is_empty());
    assert_eq!(
      result
        .units
        .iter()
        .map(|unit| unit.text.as_str())
        .collect::<String>(),
      text
    );
    assert!(result.units.iter().any(|unit| unit.text.contains(fence)));
  }

  #[test]
  fn expands_overlapping_current_decisions_and_merges_requested_ranges() {
    let text = "# Rules\n\nfirst\nsecond\nthird\nfourth\n";
    let source = document("rules.md", text);
    let decisions = vec![
      Decision {
        document: source.id.clone(),
        line_start: 3,
        line_end: 5,
        version: source.hash.clone(),
        ..Decision::default()
      },
      Decision {
        document: source.id.clone(),
        line_start: 5,
        line_end: 6,
        version: source.hash.clone(),
        ..Decision::default()
      },
    ];
    let units = repair_units(
      &[source],
      &decisions,
      &[
        RepairRange {
          document: "rules.md".to_owned(),
          line_start: 4,
          line_end: 4,
        },
        RepairRange {
          document: "rules.md".to_owned(),
          line_start: 6,
          line_end: 6,
        },
      ],
    )
    .expect("valid repair ranges");
    assert_eq!(units.len(), 1);
    assert_eq!((units[0].line_start, units[0].line_end), (3, 6));
    assert_eq!(units[0].text, "first\nsecond\nthird\nfourth\n");
  }

  #[test]
  fn rejects_incomplete_ranges_and_sixteen_kib_repairs() {
    let source = document("notes.md", "one\ntwo\n");
    let invalid = unit_from_range(
      &source,
      &RepairRange {
        document: "notes.md".to_owned(),
        line_start: 2,
        line_end: 3,
      },
    )
    .expect_err("range extends beyond source");
    assert_eq!(invalid.code, "INVALID_REPAIR_RANGE");

    let source = document(
      "large.md",
      &format!("{}\n{}\n", "a".repeat(8_200), "b".repeat(8_200)),
    );
    let unit = unit_from_range(
      &source,
      &RepairRange {
        document: "large.md".to_owned(),
        line_start: 1,
        line_end: 2,
      },
    )
    .expect("complete source range");
    let too_large = validate_repair_unit_size(&[unit]).expect_err("over sixteen KiB");
    assert_eq!(too_large.code, "REPAIR_RANGE_TOO_LARGE");
  }
}
