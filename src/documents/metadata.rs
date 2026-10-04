//! Validation of the outer YAML metadata header of documentation under `docs/`.
use super::Document;
use super::markdown::{frontmatter, metadata_text, raw_line_ranges};
use granit_parser::{Event, Marker, Options, Parser, Span, StrInput};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

const MAX_HEADER_BYTES: usize = 65_536;
const STATUSES: [&str; 6] = [
  "draft",
  "proposed",
  "accepted",
  "rejected",
  "superseded",
  "historical",
];
const IMPLEMENTATION_STATES: [&str; 4] = ["not-started", "in-progress", "implemented", "removed"];

/// The allowed fields, declared in their required order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Field {
  Title,
  Status,
  Implementation,
  CreatedAt,
  UpdatedAt,
  ArchivedAt,
  Tags,
  Source,
}

impl Field {
  const ALL: [Self; 8] = [
    Self::Title,
    Self::Status,
    Self::Implementation,
    Self::CreatedAt,
    Self::UpdatedAt,
    Self::ArchivedAt,
    Self::Tags,
    Self::Source,
  ];
  const REQUIRED: [Self; 4] = [Self::Title, Self::Status, Self::CreatedAt, Self::Tags];

  const fn name(self) -> &'static str {
    match self {
      Self::Title => "title",
      Self::Status => "status",
      Self::Implementation => "implementation",
      Self::CreatedAt => "created_at",
      Self::UpdatedAt => "updated_at",
      Self::ArchivedAt => "archived_at",
      Self::Tags => "tags",
      Self::Source => "source",
    }
  }
}

enum MetadataValue {
  Text(String),
  Tags(Vec<Rc<Self>>),
  Other,
}

struct Entry {
  field: Field,
  span: Span,
  value: Rc<MetadataValue>,
}

#[derive(Clone, Copy)]
struct Problem {
  /// One-based line and column within the YAML header.
  location: Option<(usize, usize)>,
  field: Option<Field>,
  message: &'static str,
}

impl Problem {
  const fn new(message: &'static str) -> Self {
    Self {
      location: None,
      field: None,
      message,
    }
  }

  fn at(marker: Marker, message: &'static str) -> Self {
    Self {
      location: Some((marker.line(), marker.col() + 1)),
      ..Self::new(message)
    }
  }

  const fn of(self, field: Field) -> Self {
    Self {
      field: Some(field),
      ..self
    }
  }
}

type Yaml<'a> = Parser<'a, StrInput<'a>>;

pub(super) fn is_documentation(source: &Document) -> bool {
  source.id.split('/').any(|part| part == "docs")
}

pub(super) fn findings(source: &Document) -> Vec<Value> {
  let Some(front) = frontmatter(&source.text) else {
    let missing = "Documentation under docs requires a closed outer YAML metadata header";
    return vec![finding(source, 0, Problem::new(missing))];
  };
  let offset = raw_line_ranges(&source.text[..front.yaml_offset]).count();
  if front.yaml.len() > MAX_HEADER_BYTES {
    let oversized = Problem::new("Outer metadata exceeds 65536 bytes");
    return vec![finding(source, offset, oversized)];
  }
  let problems =
    parse_entries(front.yaml).map_or_else(|problem| vec![problem], |entries| check(&entries));
  problems
    .into_iter()
    .map(|problem| finding(source, offset, problem))
    .collect()
}

fn finding(source: &Document, offset: usize, problem: Problem) -> Value {
  let (line, column) = problem.location.unwrap_or((1, 1));
  json!({"document":source.id,"version":source.hash,"code":"INVALID_METADATA",
    "line":offset + line,"column":column,
    "field":problem.field.map(Field::name),"message":problem.message})
}

fn next<'a>(parser: &mut Yaml<'a>) -> Result<(Event<'a>, Span), Problem> {
  parser
    .next()
    .transpose()
    .map_err(|error| Problem::at(*error.marker(), "Outer metadata contains invalid YAML"))?
    .ok_or(Problem::new("Outer metadata contains incomplete YAML"))
}

/// Read the header as one untagged mapping of distinct allowed fields.
fn parse_entries(yaml: &str) -> Result<Vec<Entry>, Problem> {
  let mut options = Options::default();
  options.emit_comments = false;
  let mut parser = Parser::new_from_str_with_options(yaml, options);
  next(&mut parser)?; // stream start
  next(&mut parser)?; // document start
  let (root, span) = next(&mut parser)?;
  if !matches!(root, Event::MappingStart(_, _, None)) {
    return Err(Problem::at(
      span.start,
      "Outer metadata must be one untagged YAML mapping",
    ));
  }
  let mut entries: Vec<Entry> = Vec::new();
  let mut anchors = HashMap::new();
  loop {
    let (key, span) = next(&mut parser)?;
    if matches!(key, Event::MappingEnd) {
      break;
    }
    let key = value(&mut parser, (key, span), &mut anchors)?;
    let field = field(&key).ok_or_else(|| {
      Problem::at(
        span.start,
        "Use only title, status, implementation, created_at, updated_at, archived_at, tags and source",
      )
    })?;
    if entries.iter().any(|entry| entry.field == field) {
      return Err(Problem::at(span.start, "Metadata field is duplicated").of(field));
    }
    let event = next(&mut parser)?;
    let value = value(&mut parser, event, &mut anchors)?;
    entries.push(Entry { field, span, value });
  }
  let (end, span) = next(&mut parser)?;
  let (stream, _) = next(&mut parser)?;
  if !matches!(end, Event::DocumentEnd) || !matches!(stream, Event::StreamEnd) {
    return Err(Problem::at(
      span.start,
      "Outer metadata must contain one YAML document",
    ));
  }
  Ok(entries)
}

fn field(key: &MetadataValue) -> Option<Field> {
  let MetadataValue::Text(key) = key else {
    return None;
  };
  Field::ALL.into_iter().find(|field| field.name() == key)
}

/// A scalar, a flat sequence of scalars or an alias to an earlier one.
fn value<'a>(
  parser: &mut Yaml<'a>,
  (event, span): (Event<'a>, Span),
  anchors: &mut HashMap<usize, Rc<MetadataValue>>,
) -> Result<Rc<MetadataValue>, Problem> {
  let (anchor, parsed) = match event {
    Event::Scalar(text, style, anchor, tag) => {
      let text = metadata_text(&text, style, tag.as_deref());
      (
        anchor,
        Rc::new(text.map_or(MetadataValue::Other, MetadataValue::Text)),
      )
    }
    Event::SequenceStart(_, anchor, None) => (anchor, tags(parser, anchors)?),
    Event::Alias(anchor) => {
      let aliased = anchors.get(&anchor).cloned().ok_or_else(|| {
        Problem::at(
          span.start,
          "Metadata aliases must reference an earlier scalar or flat sequence",
        )
      })?;
      return Ok(aliased);
    }
    _ => {
      return Err(Problem::at(
        span.start,
        "Metadata values must be text or a flat sequence of text tags",
      ));
    }
  };
  if anchor != 0 {
    anchors.insert(anchor, Rc::clone(&parsed));
  }
  Ok(parsed)
}

fn tags(
  parser: &mut Yaml<'_>,
  anchors: &mut HashMap<usize, Rc<MetadataValue>>,
) -> Result<Rc<MetadataValue>, Problem> {
  let mut items = Vec::new();
  loop {
    let (event, span) = next(parser)?;
    if matches!(event, Event::SequenceEnd) {
      return Ok(Rc::new(MetadataValue::Tags(items)));
    }
    if !matches!(event, Event::Scalar(..) | Event::Alias(_)) {
      let problem = Problem::at(span.start, "Tags must be a flat sequence of text");
      return Err(problem.of(Field::Tags));
    }
    items.push(value(parser, (event, span), anchors)?);
  }
}

fn check(entries: &[Entry]) -> Vec<Problem> {
  let mut problems = Vec::new();
  let mut previous = None;
  for entry in entries {
    let at = |message| Problem::at(entry.span.start, message).of(entry.field);
    if previous > Some(entry.field) {
      problems.push(at("Metadata fields must follow the standard field order"));
    }
    previous = Some(entry.field);
    if let Some(message) = value_problem(entry) {
      problems.push(at(message));
    }
  }
  for field in Field::REQUIRED {
    if entries.iter().all(|entry| entry.field != field) {
      problems.push(Problem::new("Required metadata field is missing").of(field));
    }
  }
  problems
}

fn value_problem(entry: &Entry) -> Option<&'static str> {
  if entry.field == Field::Tags {
    return (!valid_tags(&entry.value))
      .then_some("Tags must be nonempty, distinct lowercase kebab-case text keywords");
  }
  let MetadataValue::Text(text) = entry.value.as_ref() else {
    return Some("Metadata field must be text");
  };
  if text.trim().is_empty() {
    return Some("Metadata field must be nonempty text");
  }
  match entry.field {
    Field::Status if !STATUSES.contains(&text.as_str()) => Some("Unknown documentary status"),
    Field::Implementation if !IMPLEMENTATION_STATES.contains(&text.as_str()) => {
      Some("Unknown implementation state")
    }
    Field::CreatedAt | Field::UpdatedAt | Field::ArchivedAt if !valid_date(text) => {
      Some("Date must be a real calendar date in YYYY-MM-DD format")
    }
    _ => None,
  }
}

fn valid_tags(value: &MetadataValue) -> bool {
  let MetadataValue::Tags(tags) = value else {
    return false;
  };
  let mut seen = HashSet::new();
  !tags.is_empty()
    && tags.iter().all(|item| {
      let MetadataValue::Text(tag) = item.as_ref() else {
        return false;
      };
      *tag == tag.to_lowercase()
        && tag
          .split('-')
          .all(|part| !part.is_empty() && part.chars().all(char::is_alphanumeric))
        && seen.insert(tag)
    })
}

/// A real proleptic Gregorian `YYYY-MM-DD` date after year 0.
fn valid_date(text: &str) -> bool {
  let number = |range: std::ops::Range<usize>| {
    let digits = text.get(range)?;
    digits
      .bytes()
      .all(|byte| byte.is_ascii_digit())
      .then(|| digits.parse::<u32>().ok())?
  };
  if text.len() != 10 || text.get(4..5) != Some("-") || text.get(7..8) != Some("-") {
    return false;
  }
  let (Some(year), Some(month), Some(day)) = (number(0..4), number(5..7), number(8..10)) else {
    return false;
  };
  let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
  let days = match month {
    1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
    4 | 6 | 9 | 11 => 30,
    2 => 28 + u32::from(leap),
    _ => 0,
  };
  year > 0 && (1..=days).contains(&day)
}
