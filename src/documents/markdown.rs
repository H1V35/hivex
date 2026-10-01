use granit_parser::{
  Event as YamlEvent, Options as YamlOptions, Parser as YamlParser, ScalarStyle, Tag,
};
use num_traits::ToPrimitive;
use pulldown_cmark::{Event, Options, Parser, Tag as MarkdownTag, TagEnd};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::ops::Range;
use std::path::Path;
use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownDescription {
  pub links: Vec<String>,
  pub status: Option<String>,
  pub title: String,
}

pub fn hash(text: &str) -> String {
  let digest = Sha256::digest(text.as_bytes());
  format!("{digest:x}")
}

pub fn is_markdown_path(path: &str) -> bool {
  let lower = path.to_ascii_lowercase();
  [".md", ".markdown", ".mdown"]
    .iter()
    .any(|extension| lower.ends_with(extension))
}

fn line_end(text: &str, start: usize) -> usize {
  let bytes = text.as_bytes();
  let mut index = start;
  while index < bytes.len() && bytes[index] != b'\r' && bytes[index] != b'\n' {
    index += 1;
  }
  if index == bytes.len() {
    return index;
  }
  if bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
    index + 2
  } else {
    index + 1
  }
}

fn line_without_ending(text: &str) -> &str {
  text
    .strip_suffix("\r\n")
    .or_else(|| text.strip_suffix('\r'))
    .or_else(|| text.strip_suffix('\n'))
    .unwrap_or(text)
}

fn is_frontmatter_fence(line: &str) -> bool {
  line.trim_end_matches([' ', '\t']) == "---"
}

/// Borrow source ranges without allocating one string per line.
pub fn raw_line_ranges(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
  let mut start = 0;
  let mut empty = text.is_empty();
  std::iter::from_fn(move || {
    if empty {
      empty = false;
      return Some(0..0);
    }
    if start >= text.len() {
      return None;
    }
    let end = line_end(text, start);
    let range = start..end;
    start = end;
    Some(range)
  })
}

pub fn line_content(line: &str) -> &str {
  line_without_ending(line)
}

pub(super) struct Frontmatter<'a> {
  pub(super) body: &'a str,
  yaml: &'a str,
}

pub(super) fn frontmatter(text: &str) -> Option<Frontmatter<'_>> {
  // The frontmatter extension recognizes a YAML fence only at the start of
  // the document. A BOM is source content for hashing but does not prevent
  // the first fence from being recognized.
  let opening = text.strip_prefix('\u{feff}').map_or(text, |rest| rest);
  let first_end = line_end(opening, 0);
  if !is_frontmatter_fence(line_without_ending(&opening[..first_end])) {
    return None;
  }
  let yaml_start = first_end;
  let mut cursor = yaml_start;
  while cursor < opening.len() {
    let end = line_end(opening, cursor);
    if is_frontmatter_fence(line_without_ending(&opening[cursor..end])) {
      return Some(Frontmatter {
        body: &opening[end..],
        yaml: &opening[yaml_start..cursor],
      });
    }
    cursor = end;
  }
  None
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum Node {
  Boolean(bool),
  Null,
  Number(u64),
  Object,
  String(String),
}

#[derive(Clone, Copy, PartialEq)]
enum NumberKind {
  Integer,
  Float,
}

fn number_key(value: f64) -> Option<u64> {
  if value.is_nan() {
    return None;
  }
  Some(if value == 0.0 {
    0.0f64.to_bits()
  } else {
    value.to_bits()
  })
}

fn yaml_number(value: &str) -> Option<(NumberKind, f64)> {
  static NUMBER: OnceLock<Regex> = OnceLock::new();
  let captures = NUMBER
    .get_or_init(|| {
      Regex::new(concat!(
        r"^(?:(?P<integer>[-+]?[0-9]+|0x[0-9a-fA-F]+|0o[0-7]+)|(?P<float>",
        r"[-+]?\.(?:inf|Inf|INF)|\.nan|\.NaN|\.NAN|",
        r"[-+]?(?:\.[0-9]+|[0-9]+(?:\.[0-9]*)?)[eE][-+]?[0-9]+|",
        r"[-+]?(?:\.[0-9]+|[0-9]+\.[0-9]*)))$"
      ))
      .expect("valid YAML number regex")
    })
    .captures(value)?;
  let kind = if captures.name("integer").is_some() {
    NumberKind::Integer
  } else {
    NumberKind::Float
  };
  let number = match value {
    ".nan" | ".NaN" | ".NAN" => f64::NAN,
    ".inf" | ".Inf" | ".INF" | "+.inf" | "+.Inf" | "+.INF" => f64::INFINITY,
    "-.inf" | "-.Inf" | "-.INF" => f64::NEG_INFINITY,
    hex if hex.starts_with("0x") => radix_number(&hex[2..], 4),
    octal if octal.starts_with("0o") => radix_number(&octal[2..], 3),
    ordinary => ordinary.parse::<f64>().ok()?,
  };
  Some((kind, number))
}

// YAML integers become JavaScript Numbers. Keep the leading significand and
// guard/sticky bits so large radix literals round once, including ties to even.
fn radix_number(digits: &str, digit_bits: u32) -> f64 {
  let mut head = 0_u64;
  let mut count = 0_i32;
  let mut sticky = false;
  for digit in digits.chars() {
    let digit = digit
      .to_digit(1 << digit_bits)
      .expect("validated radix digit");
    for position in (0..digit_bits).rev() {
      let bit = u64::from((digit >> position) & 1);
      if count == 0 && bit == 0 {
        continue;
      }
      count += 1;
      if count <= 54 {
        head = (head << 1) | bit;
      } else {
        sticky |= bit != 0;
      }
    }
  }
  if count <= 53 {
    return head.to_f64().expect("integer converts to float");
  }
  let mut significand = head >> 1;
  if head & 1 != 0 && (sticky || significand & 1 != 0) {
    significand += 1;
  }
  significand.to_f64().expect("53-bit significand") * 2.0_f64.powi(count - 53)
}

fn explicit_tag_name(tag: Option<&Tag>) -> Option<String> {
  let tag = tag?;
  tag.core_suffix().map(str::to_owned).or_else(|| {
    tag
      .suffix_in_namespace("tag:yaml.org,2002:")
      .map(std::borrow::Cow::into_owned)
  })
}

fn scalar_value(value: &str, style: ScalarStyle, tag: Option<&Tag>) -> Option<Node> {
  let tag_name = explicit_tag_name(tag);
  if tag.is_some() && tag_name.is_none() {
    return Some(Node::String(value.to_owned()));
  }
  match tag_name.as_deref() {
    Some("binary") => Some(Node::Object),
    Some("timestamp") => {
      static TIMESTAMP: OnceLock<Regex> = OnceLock::new();
      TIMESTAMP
                .get_or_init(|| Regex::new(r"^[0-9]{4}-[0-9]{1,2}-[0-9]{1,2}(?:(?:t|T|[ \t]+)[0-9]{1,2}:[0-9]{1,2}:[0-9]{1,2}(?:\.[0-9]+)?(?:[ \t]*(?:Z|[-+][012]?[0-9](?::[0-9]{2})?))?)?$").expect("valid YAML timestamp regex"))
                .is_match(value)
                .then_some(Node::Object)
    }
    Some("null") if matches!(value, "" | "~" | "null" | "Null" | "NULL") => Some(Node::Null),
    Some("bool") => match value {
      "true" | "True" | "TRUE" => Some(Node::Boolean(true)),
      "false" | "False" | "FALSE" => Some(Node::Boolean(false)),
      _ => Some(Node::String(value.to_owned())),
    },
    Some(kind @ ("int" | "float")) => yaml_number(value)
      .filter(|(parsed, _)| (*parsed == NumberKind::Integer) == (kind == "int"))
      .map(|(_, value)| number_value(value))
      .or_else(|| Some(Node::String(value.to_owned()))),
    Some("null" | "str" | _) => Some(Node::String(value.to_owned())),
    None if style != ScalarStyle::Plain => Some(Node::String(value.to_owned())),
    None => Some(plain_scalar(value)),
  }
}

fn number_value(value: f64) -> Node {
  number_key(value).map_or(Node::Object, Node::Number)
}

#[derive(Default)]
struct MappingFrame {
  is_root: bool,
  pending_field: Option<RootField>,
  pending_key: bool,
  seen: HashSet<Node>,
}

enum Frame {
  Mapping(MappingFrame),
  Sequence,
}

#[derive(Default)]
struct MetadataState {
  frames: Vec<Frame>,
  document_count: usize,
  root_seen: bool,
  status: Option<String>,
  title: Option<String>,
}

#[derive(Clone, Copy)]
enum RootField {
  Status,
  Title,
}

fn field_for_key(value: &Node, is_root: bool) -> Option<RootField> {
  if !is_root {
    return None;
  }
  match value {
    Node::String(value) if value == "status" => Some(RootField::Status),
    Node::String(value) if value == "title" => Some(RootField::Title),
    _ => None,
  }
}

fn record_value(state: &mut MetadataState, value: Node) -> std::result::Result<(), ()> {
  let Some(frame) = state.frames.last_mut() else {
    if state.root_seen {
      return Err(());
    }
    state.root_seen = true;
    return Ok(());
  };
  match frame {
    Frame::Sequence => Ok(()),
    Frame::Mapping(mapping) if !mapping.pending_key => {
      let identity = (!matches!(value, Node::Object)).then(|| value.clone());
      if let Some(identity) = identity
        && !mapping.seen.insert(identity)
      {
        return Err(());
      }
      mapping.pending_field = field_for_key(&value, mapping.is_root);
      mapping.pending_key = true;
      Ok(())
    }
    Frame::Mapping(mapping) => {
      if let Some(field) = mapping.pending_field.take()
        && let Node::String(value) = value
      {
        match field {
          RootField::Status => state.status = Some(value),
          RootField::Title => state.title = Some(value),
        }
      }
      mapping.pending_key = false;
      Ok(())
    }
  }
}

fn parse_yaml_metadata(yaml: &str) -> Option<(Option<String>, Option<String>)> {
  let limit = yaml.len().saturating_add(1).max(256);
  let mut options = YamlOptions::default();
  options.emit_comments = false;
  options.flow_nesting_limit = limit;
  options.block_nesting_limit = limit;
  let mut parser = YamlParser::new_from_str_with_options(yaml, options);
  let mut state = MetadataState::default();
  for event in &mut parser {
    let Ok((event, _span)) = event else {
      return None;
    };
    state.accept(event)?;
  }
  if !state.frames.is_empty() || state.document_count > 1 {
    return None;
  }
  Some((
    state.status,
    state
      .title
      .filter(|title| !crate::compatibility::trim_js_whitespace(title).is_empty()),
  ))
}

fn heading_text<'a>(events: impl IntoIterator<Item = Event<'a>>) -> Option<String> {
  let mut depth = 0usize;
  let mut active = false;
  let mut text = String::new();
  for event in events {
    match event {
      Event::Start(MarkdownTag::Heading { .. }) => {
        if depth == 0 && !active {
          active = true;
        }
        depth += 1;
      }
      Event::End(TagEnd::Heading(_)) => {
        depth = depth.saturating_sub(1);
        if active && depth == 0 {
          return Some(text);
        }
      }
      Event::Start(_) => depth += 1,
      Event::End(_) => depth = depth.saturating_sub(1),
      Event::Text(value) | Event::Code(value) if active => text.push_str(&value),
      Event::InlineHtml(value) if active => text.push_str(&value),
      Event::SoftBreak | Event::HardBreak if active => text.push('\n'),
      _ => {}
    }
  }
  None
}

pub(super) fn markdown_options() -> Options {
  let mut options = Options::empty();
  options.insert(Options::ENABLE_TABLES);
  options.insert(Options::ENABLE_STRIKETHROUGH);
  options.insert(Options::ENABLE_TASKLISTS);
  options.insert(Options::ENABLE_FOOTNOTES);
  options.insert(Options::ENABLE_GFM);
  options
}

fn parse_body(body: &str) -> (Option<String>, Vec<String>) {
  let title = heading_text(Parser::new_ext(body, markdown_options()));
  let links = Parser::new_ext(body, markdown_options())
    .filter_map(|event| match event {
      Event::Start(MarkdownTag::Link { dest_url, .. }) if !dest_url.is_empty() => {
        Some(dest_url.into_string())
      }
      _ => None,
    })
    .collect();
  (title, links)
}

pub fn markdown_references(text: &str) -> impl Iterator<Item = (String, usize)> + '_ {
  let body = frontmatter(text).map_or(text, |front| front.body);
  let offset = text.len() - body.len();
  let mut lines = raw_line_ranges(text).enumerate().peekable();
  Parser::new_ext(body, markdown_options())
    .into_offset_iter()
    .filter_map(move |(event, range)| {
      let Event::Start(MarkdownTag::Link { dest_url, .. }) = event else {
        return None;
      };
      let start = offset + range.start;
      while lines
        .peek()
        .is_some_and(|(_, line)| line.end <= start && line.start != line.end)
      {
        lines.next();
      }
      let line = lines.peek().map_or(1, |(line, _)| line + 1);
      Some((dest_url.into_string(), line))
    })
}

pub fn describe_markdown(path: &str, content: &str) -> MarkdownDescription {
  let (yaml, body) =
    frontmatter(content).map_or((None, content), |front| (Some(front.yaml), front.body));
  let (status, front_title) = yaml.and_then(parse_yaml_metadata).unwrap_or((None, None));
  let (heading, links) = parse_body(body.strip_prefix('\u{feff}').unwrap_or(body));
  let title = front_title.or(heading).unwrap_or_else(|| {
    Path::new(path)
      .file_name()
      .and_then(|name| name.to_str())
      .unwrap_or(path)
      .to_owned()
  });
  MarkdownDescription {
    links,
    status,
    title,
  }
}

fn plain_scalar(value: &str) -> Node {
  match value {
    "" | "~" | "null" | "Null" | "NULL" => Node::Null,
    "true" | "True" | "TRUE" => Node::Boolean(true),
    "false" | "False" | "FALSE" => Node::Boolean(false),
    _ => yaml_number(value).map_or_else(
      || Node::String(value.to_owned()),
      |(_, value)| number_value(value),
    ),
  }
}

impl MetadataState {
  fn accept(&mut self, event: YamlEvent<'_>) -> Option<()> {
    let result = match event {
      YamlEvent::StreamStart
      | YamlEvent::StreamEnd
      | YamlEvent::Comment(_, _)
      | YamlEvent::DocumentEnd => {
        if matches!(event, YamlEvent::DocumentEnd) && !self.frames.is_empty() {
          return None;
        }
        return Some(());
      }
      YamlEvent::DocumentStart(_, _) => {
        self.document_count += 1;
        if self.document_count > 1 {
          return None;
        }
        return Some(());
      }
      YamlEvent::Scalar(value, style, _, tag) => scalar_value(&value, style, tag.as_deref())?,
      YamlEvent::SequenceStart(_, _, _) => {
        self.frames.push(Frame::Sequence);
        return Some(());
      }
      YamlEvent::MappingStart(_, _, _) => {
        self.frames.push(Frame::Mapping(MappingFrame {
          is_root: self.frames.is_empty(),
          ..MappingFrame::default()
        }));
        return Some(());
      }
      YamlEvent::SequenceEnd => {
        if !matches!(self.frames.pop(), Some(Frame::Sequence)) {
          return None;
        }
        Node::Object
      }
      YamlEvent::MappingEnd => {
        let Some(Frame::Mapping(mapping)) = self.frames.pop() else {
          return None;
        };
        if mapping.pending_key {
          return None;
        }
        Node::Object
      }
      _ => return None,
    };
    record_value(self, result).ok()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::{Value, json};
  use std::fmt::Write;
  #[test]
  fn yaml_compatibility_and_depth_preserve_selected_metadata() {
    let cases = [
      (
        "title: &title Wrong\nstatus: *title",
        "Fallback",
        Value::Null,
      ),
      (
        "title: &title Anchored title\nstatus: accepted",
        "Anchored title",
        json!("accepted"),
      ),
      (
        "title: |-\n  *Literal title\nstatus: accepted # *not-an-alias",
        "*Literal title",
        json!("accepted"),
      ),
      ("title: One\ntitle: Two", "Fallback", Value::Null),
      ("1: a\n1.0: b\ntitle: Main", "Fallback", Value::Null),
      ("1: a\n!!str 1: b\ntitle: Main", "Main", Value::Null),
      (
        "title: !!binary SGVsbG8=\nstatus: accepted",
        "Fallback",
        json!("accepted"),
      ),
      ("title: \"  \"\nstatus: \"\"", "Fallback", json!("")),
      ("title: \"\u{feff}\"", "Fallback", Value::Null),
      ("title: \"\"", "Fallback", Value::Null),
      ("title: Main\nstatus: .inf", "Main", Value::Null),
      (
        "title: Main\nstatus: !!timestamp invalid",
        "Fallback",
        Value::Null,
      ),
      (
        "title: 0x10000000000000000\nstatus: accepted",
        "Fallback",
        json!("accepted"),
      ),
      (
        "0x2000000000000101: one\n2305843009213694464: two\ntitle: Main",
        "Fallback",
        Value::Null,
      ),
      ("? {a: 1}\n? {a: 1}\ntitle: Main", "Main", Value::Null),
      ("title: Main\nstatus: 1e999", "Main", Value::Null),
      (
        "title: Main\nstatus: !!timestamp 2024-01-01",
        "Main",
        Value::Null,
      ),
      (
        "title: 1_000\nstatus: tRuE\nbinary: 0b101\nhex: +0x10\nnan: +.nan\ninfinity: .iNF\nnullish: nUlL",
        "1_000",
        json!("tRuE"),
      ),
      ("title: !!float 2\nstatus: !!bool tRuE", "2", json!("tRuE")),
      ("status: [reviewed, adopted]", "Fallback", Value::Null),
    ];
    for (metadata, title, status) in cases {
      let parsed = describe_markdown("fixture.md", &format!("---\n{metadata}\n---\n# Fallback\n"));
      assert_eq!(parsed.title, title);
      assert_eq!(json!(parsed.status), status);
    }
    for depth in [24, 100] {
      let mut text = String::from("---\ntitle: Deep metadata\nstatus: accepted\nnested:\n");
      for level in 1..=depth {
        writeln!(text, "{}nested:", "  ".repeat(level)).unwrap();
      }
      write!(
        text,
        "{}unknown: value\n---\n# Fallback\n",
        "  ".repeat(depth + 1)
      )
      .unwrap();
      let parsed = describe_markdown("deep.md", &text);
      assert_eq!(parsed.title, "Deep metadata");
      assert_eq!(parsed.status.as_deref(), Some("accepted"));
    }
    let flow = format!(
      "---\ntitle: Flow metadata\nstatus: accepted\nnested: {}value{}\n---\n# Fallback\n",
      "[".repeat(1000),
      "]".repeat(1000)
    );
    assert_eq!(describe_markdown("flow.md", &flow).title, "Flow metadata");
  }
}
