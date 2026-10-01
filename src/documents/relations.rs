use super::{is_markdown_path, line_content, raw_markdown_lines};
use crate::error::{HivexError, Result};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use url::Url;

pub const MAX_RELATIONS: usize = 2_048;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RelationKind {
  DependsOn,
  ExceptionTo,
  Supersedes,
  Implements,
  Extends,
}

impl RelationKind {
  pub const fn literal(self) -> &'static str {
    match self {
      Self::DependsOn => "Depends on",
      Self::ExceptionTo => "Exception to",
      Self::Supersedes => "Supersedes",
      Self::Implements => "Implements",
      Self::Extends => "Extends",
    }
  }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftRelation {
  pub kind: RelationKind,
  pub target: String,
  pub reason: String,
  pub line_start: usize,
  pub line_end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Anchor {
  pub id: String,
  pub line_start: usize,
  pub line_end: usize,
}

#[derive(Clone, Debug)]
struct Heading {
  end: usize,
  level: u8,
  start: usize,
  text: String,
}

#[derive(Clone, Debug)]
struct ExplicitAnchor {
  id: String,
  offset: usize,
}

#[derive(Default)]
struct MarkdownScan {
  body_offset: usize,
  blockquote_depth: usize,
  list_depth: usize,
  heading: Option<Heading>,
  headings: Vec<Heading>,
  explicit_anchors: Vec<ExplicitAnchor>,
  ignored_html_until: Option<&'static str>,
}

impl MarkdownScan {
  fn accept(&mut self, event: Event<'_>, range: Range<usize>) {
    match event {
      Event::Start(Tag::BlockQuote(_)) => self.blockquote_depth += 1,
      Event::End(TagEnd::BlockQuote(_)) => {
        self.blockquote_depth = self.blockquote_depth.saturating_sub(1);
      }
      Event::Start(Tag::List(_)) => self.list_depth += 1,
      Event::End(TagEnd::List(_)) => self.list_depth = self.list_depth.saturating_sub(1),
      Event::Start(Tag::Heading { level, .. })
        if self.blockquote_depth == 0 && self.list_depth == 0 =>
      {
        self.heading = Some(Heading {
          end: self.body_offset + range.end,
          level: heading_level(level),
          start: self.body_offset + range.start,
          text: String::new(),
        });
      }
      Event::End(TagEnd::Heading(_)) => {
        if let Some(heading) = self.heading.take() {
          self.headings.push(heading);
        }
      }
      Event::Text(text) | Event::Code(text) => {
        if let Some(heading) = &mut self.heading {
          heading.text.push_str(&text);
        }
      }
      Event::SoftBreak | Event::HardBreak => {
        if let Some(heading) = &mut self.heading {
          heading.text.push(' ');
        }
      }
      Event::Html(html) | Event::InlineHtml(html) if self.blockquote_depth == 0 => {
        for (id, offset) in explicit_ids(&html, &mut self.ignored_html_until) {
          self.explicit_anchors.push(ExplicitAnchor {
            id,
            offset: self.body_offset + range.start + offset,
          });
        }
      }
      _ => {}
    }
  }
}

fn heading_level(level: HeadingLevel) -> u8 {
  match level {
    HeadingLevel::H1 => 1,
    HeadingLevel::H2 => 2,
    HeadingLevel::H3 => 3,
    HeadingLevel::H4 => 4,
    HeadingLevel::H5 => 5,
    HeadingLevel::H6 => 6,
  }
}

fn is_frontmatter_fence(line: &str) -> bool {
  let content = line_content(line).trim_end_matches([' ', '\t']);
  content == "---"
}

fn source_body(text: &str) -> (usize, &str) {
  let lines = raw_markdown_lines(text);
  let Some(first) = lines.first() else {
    return (0, text);
  };
  let first_content = line_content(first).trim_end_matches([' ', '\t']);
  if first_content
    .strip_prefix('\u{feff}')
    .unwrap_or(first_content)
    != "---"
  {
    return (0, text);
  }
  let mut offset = lines[0].len();
  for line in lines.iter().skip(1) {
    offset += line.len();
    if is_frontmatter_fence(line) {
      return (offset, &text[offset..]);
    }
  }
  (0, text)
}

fn markdown_options() -> Options {
  let mut options = Options::empty();
  options.insert(Options::ENABLE_TABLES);
  options.insert(Options::ENABLE_STRIKETHROUGH);
  options.insert(Options::ENABLE_TASKLISTS);
  options.insert(Options::ENABLE_FOOTNOTES);
  options.insert(Options::ENABLE_GFM);
  options
}

fn explicit_ids(html: &str, ignored_until: &mut Option<&'static str>) -> Vec<(String, usize)> {
  const PREFIX: &str = "<a id=\"";
  const SUFFIX: &str = "\">";
  let lower = html.to_ascii_lowercase();
  let mut cursor = 0;
  let mut result = Vec::new();
  while cursor < html.len() {
    if let Some(ending) = *ignored_until {
      let Some(end) = ignored_html_end(&lower[cursor..], ending) else {
        break;
      };
      cursor += end;
      *ignored_until = None;
      continue;
    }
    let Some(start) = html[cursor..].find('<') else {
      break;
    };
    let start = cursor + start;
    if lower[start..].starts_with("<!--") {
      *ignored_until = Some("-->");
      cursor = start + 4;
      continue;
    }
    let Some(end) = html_tag_end(&html[start..]) else {
      break;
    };
    cursor = start + end + 1;
    let tag = &lower[start..cursor];
    if let Some(ending) = raw_text_ending(tag) {
      *ignored_until = Some(ending);
      continue;
    }
    let Some(id) = html[start..cursor]
      .strip_prefix(PREFIX)
      .and_then(|tag| tag.strip_suffix(SUFFIX))
    else {
      continue;
    };
    if !id.is_empty()
      && !id
        .chars()
        .any(|value| value.is_whitespace() || ['"', '<', '>', '&'].contains(&value))
    {
      result.push((id.to_owned(), start));
    }
  }
  result
}

fn ignored_html_end(html: &str, ending: &str) -> Option<usize> {
  if ending == "\0" {
    return None;
  }
  html.match_indices(ending).find_map(|(index, _)| {
    let boundary = html[index + ending.len()..].chars().next();
    if ending == "-->" {
      return Some(index + ending.len());
    }
    if !boundary.is_some_and(|value| value.is_whitespace() || ['>', '/'].contains(&value)) {
      return None;
    }
    html_tag_end(&html[index..]).map(|end| index + end + 1)
  })
}

fn html_tag_end(tag: &str) -> Option<usize> {
  let mut quote = None;
  for (index, character) in tag.char_indices() {
    match (quote, character) {
      (Some(expected), actual) if expected == actual => quote = None,
      (None, '\'' | '"') => quote = Some(character),
      (None, '>') => return Some(index),
      _ => {}
    }
  }
  None
}

fn raw_text_ending(tag: &str) -> Option<&'static str> {
  [
    ("script", "</script"),
    ("style", "</style"),
    ("textarea", "</textarea"),
    ("title", "</title"),
    ("xmp", "</xmp"),
    ("iframe", "</iframe"),
    ("noembed", "</noembed"),
    ("noframes", "</noframes"),
    ("plaintext", "\0"),
  ]
  .into_iter()
  .find_map(|(name, ending)| {
    tag
      .strip_prefix('<')
      .and_then(|tag| tag.strip_prefix(name))
      .filter(|suffix| suffix.starts_with('>') || suffix.starts_with(char::is_whitespace))
      .map(|_| ending)
  })
}

fn scan_markdown(text: &str) -> MarkdownScan {
  let (body_offset, body) = source_body(text);
  let mut scan = MarkdownScan {
    body_offset,
    ..MarkdownScan::default()
  };
  let mut events = Parser::new_ext(body, markdown_options())
    .into_offset_iter()
    .peekable();
  while let Some((event, mut range)) = events.next() {
    if matches!(event, Event::Html(_) | Event::InlineHtml(_)) {
      while events.peek().is_some_and(|(next, next_range)| {
        matches!(next, Event::Html(_) | Event::InlineHtml(_)) && next_range.start == range.end
      }) {
        range.end = events.next().expect("peeked event").1.end;
      }
      scan.accept(Event::Html(body[range.clone()].into()), range);
      continue;
    }
    scan.accept(event, range);
  }
  scan
}

fn line_starts(lines: &[String]) -> Vec<usize> {
  let mut starts = Vec::with_capacity(lines.len());
  let mut offset = 0;
  for line in lines {
    starts.push(offset);
    offset += line.len();
  }
  starts
}

fn line_number(starts: &[usize], offset: usize) -> usize {
  starts
    .partition_point(|start| *start <= offset)
    .saturating_sub(1)
    + 1
}

fn first_line_after(starts: &[usize], offset: usize) -> usize {
  let line = line_number(starts, offset);
  if starts.get(line - 1) == Some(&offset) {
    line
  } else {
    line + 1
  }
}

fn relation_error(line: usize, cause: &'static str) -> HivexError {
  HivexError::new(
    "INVALID_RELATION",
    "Invalid entry in the Relationships section",
  )
  .with_details(json!({"line": line, "cause": cause}))
}

fn kind_prefix(value: &str) -> Option<(RelationKind, &str)> {
  [
    RelationKind::DependsOn,
    RelationKind::ExceptionTo,
    RelationKind::Supersedes,
    RelationKind::Implements,
    RelationKind::Extends,
  ]
  .into_iter()
  .find_map(|kind| {
    value
      .strip_prefix(kind.literal())
      .and_then(|rest| rest.strip_prefix(' '))
      .map(|rest| (kind, rest))
  })
}

fn parsed_link_target(line: &str, line_number: usize) -> Result<String> {
  let mut list_count = 0;
  let mut item_count = 0;
  let mut targets = Vec::new();
  let mut invalid = false;
  for event in Parser::new_ext(line, markdown_options()) {
    match event {
      Event::Start(Tag::List(None)) => list_count += 1,
      Event::Start(Tag::Item) => item_count += 1,
      Event::Start(Tag::Link {
        dest_url, title, ..
      }) => {
        if !title.is_empty() {
          invalid = true;
        }
        targets.push(dest_url.into_string());
      }
      Event::Text(_) | Event::End(TagEnd::List(_) | TagEnd::Item | TagEnd::Link) => {}
      _ => invalid = true,
    }
  }
  if invalid || list_count != 1 || item_count != 1 || targets.len() != 1 {
    return Err(relation_error(
      line_number,
      "entry must be one unordered list item with exactly one plain inline link",
    ));
  }
  Ok(targets.remove(0))
}

fn valid_target(target: &str) -> bool {
  let (path, anchor) = target
    .split_once('#')
    .map_or((target, None), |(path, anchor)| (path, Some(anchor)));
  (!path.is_empty() || anchor.is_some())
    && anchor.is_none_or(|id| !id.is_empty() && !id.contains('#'))
    && !target.contains('?')
    && !path.starts_with('/')
    && !path.starts_with('\\')
    && !path.contains('\\')
    && !target.chars().any(char::is_whitespace)
    && (path.is_empty() || is_markdown_path(path))
    && Url::parse(target).is_err()
}

fn parse_relation_line(line: &str, line_number: usize) -> Result<DraftRelation> {
  let entry = line
    .strip_prefix("- ")
    .ok_or_else(|| relation_error(line_number, "expected one unindented unordered bullet line"))?;
  let (kind, declaration) = kind_prefix(entry)
    .ok_or_else(|| relation_error(line_number, "unknown or malformed relationship literal"))?;
  let Some(declaration) = declaration.strip_prefix('[') else {
    return Err(relation_error(
      line_number,
      "expected a plain Markdown link after the literal",
    ));
  };
  let Some((label, remainder)) = declaration.split_once("](") else {
    return Err(relation_error(line_number, "malformed Markdown link"));
  };
  if label.is_empty() || label.contains('[') || label.contains(']') {
    return Err(relation_error(
      line_number,
      "link label must be non-empty plain text",
    ));
  }
  let Some((raw_target, suffix)) = remainder.split_once(')') else {
    return Err(relation_error(
      line_number,
      "malformed Markdown link destination",
    ));
  };
  let Some(reason) = suffix.strip_prefix(": ") else {
    return Err(relation_error(
      line_number,
      "expected ': ' followed by a reason",
    ));
  };
  if reason.trim().is_empty() {
    return Err(relation_error(
      line_number,
      "relationship reason must be non-empty",
    ));
  }
  let target = parsed_link_target(line, line_number)?;
  if target != raw_target {
    return Err(relation_error(
      line_number,
      "entry must contain only its one parsed link",
    ));
  }
  if !valid_target(&target) {
    return Err(relation_error(
      line_number,
      "target must be a relative Markdown source with an optional non-empty fragment",
    ));
  }
  Ok(DraftRelation {
    kind,
    target,
    reason: reason.to_owned(),
    line_start: line_number,
    line_end: line_number,
  })
}

/// Parse one optional level-two Relationships section.
///
/// Every nonblank line in the section must be a formal relation entry; errors
/// identify its one-based source line and cause instead of dropping it.
pub fn parse(text: &str) -> Result<Vec<DraftRelation>> {
  let scan = scan_markdown(text);
  let lines = raw_markdown_lines(text);
  let starts = line_starts(&lines);
  let blocks: Vec<_> = scan
    .headings
    .iter()
    .enumerate()
    .filter(|(_, heading)| heading.level == 2 && heading.text.trim() == "Relationships")
    .collect();
  if blocks.len() > 1 {
    let line = line_number(&starts, blocks[1].1.start);
    return Err(relation_error(
      line,
      "document has more than one H2 Relationships section",
    ));
  }
  let Some((index, heading)) = blocks.first().copied() else {
    return Ok(Vec::new());
  };
  let heading_line = line_number(&starts, heading.start);
  if line_content(&lines[heading_line - 1]) != "## Relationships" {
    return Err(relation_error(
      heading_line,
      "expected exact '## Relationships' heading",
    ));
  }
  let start_line = first_line_after(&starts, heading.end);
  let end_line = scan
    .headings
    .iter()
    .skip(index + 1)
    .find(|next| next.level <= 2)
    .map_or(lines.len(), |next| {
      line_number(&starts, next.start).saturating_sub(1)
    });
  let mut relations = Vec::new();
  for number in start_line..=end_line {
    let line = line_content(&lines[number - 1]);
    if !line.trim().is_empty() {
      if relations.len() == MAX_RELATIONS {
        return Err(
          HivexError::new(
            "RELATION_LIMIT",
            "At most 2048 authored declarations are supported per query",
          )
          .with_details(json!({"line":number,"limit":MAX_RELATIONS})),
        );
      }
      relations.push(parse_relation_line(line, number)?);
    }
  }
  Ok(relations)
}

fn github_slug(text: &str) -> String {
  const REMOVED: &str = "!\"#$%&'()*+,./:;<=>?@[\\]^`{|}~";
  text
    .to_lowercase()
    .chars()
    .filter_map(|character| match character {
      value if REMOVED.contains(value) => None,
      value if value.is_whitespace() => Some('-'),
      value => Some(value),
    })
    .collect()
}

fn unique_heading_slug(
  base: &str,
  counts: &mut HashMap<String, usize>,
  used: &mut HashSet<String>,
) -> String {
  let count = counts.entry(base.to_owned()).or_default();
  loop {
    let candidate = if *count == 0 {
      base.to_owned()
    } else {
      format!("{base}-{}", *count)
    };
    *count += 1;
    if used.insert(candidate.clone()) {
      return candidate;
    }
  }
}

fn section_end(headings: &[Heading], index: usize, starts: &[usize], line_count: usize) -> usize {
  let heading = &headings[index];
  headings
    .iter()
    .skip(index + 1)
    .find(|next| next.level <= heading.level)
    .map_or(line_count, |next| {
      line_number(starts, next.start).saturating_sub(1)
    })
}

fn explicit_end(scan: &MarkdownScan, offset: usize, starts: &[usize], line_count: usize) -> usize {
  let heading_index = scan
    .headings
    .iter()
    .position(|heading| heading.start <= offset && offset <= heading.end)
    .or_else(|| {
      scan
        .headings
        .iter()
        .position(|heading| heading.start >= offset)
    });
  heading_index.map_or_else(
    || {
      scan
        .headings
        .iter()
        .enumerate()
        .rev()
        .find(|(_, heading)| heading.start < offset)
        .map_or(line_count, |(index, _)| {
          section_end(&scan.headings, index, starts, line_count)
        })
    },
    |index| section_end(&scan.headings, index, starts, line_count),
  )
}

fn ambiguous_anchor(id: &str, first_line: usize, line: usize) -> HivexError {
  HivexError::new("AMBIGUOUS_ANCHOR", "Document contains a duplicate anchor").with_details(json!({
    "anchor": id,
    "cause": "the same anchor ID names more than one target section",
    "firstLine": first_line,
    "line": line,
  }))
}

/// Return root heading slugs and explicit empty `<a id="…"></a>` anchors.
///
/// Heading ranges include the heading through the next same-or-higher heading.
/// An explicit ID starts at its marker and ends with its nearest following
/// heading's section, or the preceding section when no heading follows.
pub fn anchors(text: &str) -> Result<Vec<Anchor>> {
  let scan = scan_markdown(text);
  let lines = raw_markdown_lines(text);
  let starts = line_starts(&lines);
  let mut counts = HashMap::new();
  let mut used = HashSet::new();
  let mut anchors = Vec::new();
  for (index, heading) in scan.headings.iter().enumerate() {
    let base = github_slug(&heading.text);
    if !base.is_empty() {
      anchors.push(Anchor {
        id: unique_heading_slug(&base, &mut counts, &mut used),
        line_start: line_number(&starts, heading.start),
        line_end: section_end(&scan.headings, index, &starts, lines.len()),
      });
    }
  }
  for marker in &scan.explicit_anchors {
    let line = line_number(&starts, marker.offset);
    anchors.push(Anchor {
      id: marker.id.clone(),
      line_start: line,
      line_end: explicit_end(&scan, marker.offset, &starts, lines.len()),
    });
  }
  anchors.sort_by_key(|anchor| (anchor.line_start, anchor.line_end));
  let mut seen = HashMap::new();
  for anchor in &anchors {
    if let Some(first_line) = seen.insert(anchor.id.clone(), anchor.line_start) {
      return Err(ambiguous_anchor(&anchor.id, first_line, anchor.line_start));
    }
  }
  Ok(anchors)
}

#[cfg(test)]
mod tests {
  use super::{Anchor, RelationKind, anchors, parse};

  #[test]
  fn parses_only_the_root_relationships_block_with_original_lines() {
    let source = "---\r\ntitle: demo\r\n---\r\n> ## Relationships\r\n> - Requires [ignored](ignored.md#x): quote\r\n\r\n## Relationships\r\n\r\n- Depends on [Foundation](foundation.md#scope): required first\r\n- Exception to [Foundation](foundation.md#scope): narrow case\r\n\r\n## Notes\r\nfree prose\r\n";
    let parsed = parse(source).expect("valid relations");
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].kind, RelationKind::DependsOn);
    assert_eq!(parsed[0].target, "foundation.md#scope");
    assert_eq!(parsed[0].reason, "required first");
    assert_eq!((parsed[0].line_start, parsed[0].line_end), (9, 9));
    assert_eq!(parsed[1].kind, RelationKind::ExceptionTo);
    assert_eq!((parsed[1].line_start, parsed[1].line_end), (10, 10));
  }

  #[test]
  fn reports_unknown_malformed_and_multiple_relationship_blocks() {
    let error = parse("## Relationships\n- Requires [X](x.md#y): reason\n")
      .expect_err("unknown relation literal must not disappear");
    assert_eq!(error.code, "INVALID_RELATION");
    assert_eq!(error.details.unwrap()["line"], 2);

    let error = parse("## Relationships\n- Depends on **[X](x.md#y)**: reason\n")
      .expect_err("formatted entries are outside the grammar");
    assert_eq!(error.code, "INVALID_RELATION");

    let error = parse("## Relationships\n- Depends on [X](x.md#y): reason\n## Relationships\n")
      .expect_err("only one formal block is allowed");
    assert_eq!(error.details.unwrap()["line"], 3);
  }

  #[test]
  fn ignores_relationship_text_in_code_and_quoted_blocks() {
    let source =
      "```md\n## Relationships\nnot an entry\n```\n> ## Relationships\n> prose\n## Other\nprose\n";
    assert_eq!(
      parse(source)
        .expect("outside text is ordinary Markdown")
        .len(),
      0
    );
  }

  #[test]
  fn rejects_multiple_links_and_non_markdown_targets() {
    for entry in [
      "- Depends on [X](x.md#y): see [also](z.md#q)",
      "- Depends on [X](https://example.test/x.md#y): reason",
      "- Depends on [X](x.md#): reason",
    ] {
      let source = format!("## Relationships\n{entry}\n");
      assert_eq!(
        parse(&source).expect_err("invalid target").code,
        "INVALID_RELATION"
      );
    }
  }

  #[test]
  fn gives_duplicate_headings_github_style_suffixes_and_rejects_duplicate_ids() {
    let values = anchors("# A title\n## A title\n## A title\n").expect("unique heading IDs");
    assert_eq!(
      values
        .iter()
        .map(|value| value.id.as_str())
        .collect::<Vec<_>>(),
      vec!["a-title", "a-title-1", "a-title-2"]
    );
    assert!(
      values
        .iter()
        .all(|value: &Anchor| value.line_end >= value.line_start)
    );

    let error =
      anchors("## Same\n<a id=\"same\"></a>\n").expect_err("heading and explicit ID collide");
    assert_eq!(error.code, "AMBIGUOUS_ANCHOR");
  }

  #[test]
  fn ignores_frontmatter_fences_code_and_quoted_anchors() {
    let source = "---\ntitle: demo\n---\n> ## Quoted\n> <a id=\"quoted\"></a>\n```md\n# Code\n<a id=\"code\"></a>\n```\n## Real\n<a id=\"manual\"></a>\n";
    let values = anchors(source).expect("unambiguous anchors");
    assert_eq!(
      values
        .iter()
        .map(|value| value.id.as_str())
        .collect::<Vec<_>>(),
      vec!["real", "manual"]
    );
    assert_eq!((values[0].line_start, values[0].line_end), (10, 11));
    assert_eq!((values[1].line_start, values[1].line_end), (11, 11));
  }
}
