use super::Document;
use super::markdown::{frontmatter, metadata_text, raw_line_ranges};
use granit_parser::{Event, Options, Parser, Span, StrInput};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

const FIELDS: [&str; 8] = [
  "title",
  "status",
  "implementation",
  "created_at",
  "updated_at",
  "archived_at",
  "tags",
  "source",
];
const REQUIRED: [usize; 4] = [0, 1, 3, 6];
const MAX_HEADER_BYTES: usize = 65_536;

enum MetadataValue {
  Text(String),
  Tags(Vec<Rc<Self>>),
  Other,
}

struct Field {
  index: usize,
  span: Span,
  value: Rc<MetadataValue>,
}

#[derive(Clone, Copy)]
struct Problem {
  location: Option<(usize, usize)>,
  field: Option<&'static str>,
  message: &'static str,
}

type Yaml<'a> = Parser<'a, StrInput<'a>>;

pub(super) fn is_documentation(source: &Document) -> bool {
  source.id.split('/').any(|part| part == "docs")
}

pub(super) fn findings(source: &Document) -> Vec<Value> {
  let Some(front) = frontmatter(&source.text) else {
    return vec![finding(source, 0, missing_header())];
  };
  let offset = raw_line_ranges(&source.text[..front.yaml_offset]).count();
  if front.yaml.len() > MAX_HEADER_BYTES {
    return vec![finding(
      source,
      offset,
      Problem {
        location: None,
        field: None,
        message: "Outer metadata exceeds 65536 bytes",
      },
    )];
  }
  match parse_fields(front.yaml) {
    Ok(fields) => validate_fields(&fields)
      .into_iter()
      .map(|problem| finding(source, offset, problem))
      .collect(),
    Err(problem) => vec![finding(source, offset, problem)],
  }
}

fn missing_header() -> Problem {
  Problem {
    location: None,
    field: None,
    message: "Documentation under docs requires a closed outer YAML metadata header",
  }
}

fn finding(source: &Document, offset: usize, problem: Problem) -> Value {
  json!({"document":source.id,"version":source.hash,"code":"INVALID_METADATA",
    "line":offset + problem.location.map_or(1, |(line, _)| line),
    "column":problem.location.map_or(1, |(_, column)| column),
    "field":problem.field,"message":problem.message})
}

fn next<'a>(parser: &mut Yaml<'a>) -> Result<(Event<'a>, Span), Problem> {
  parser
    .next()
    .transpose()
    .map_err(|error| Problem {
      location: Some((error.marker().line(), error.marker().col() + 1)),
      field: None,
      message: "Outer metadata contains invalid YAML",
    })?
    .ok_or(Problem {
      location: None,
      field: None,
      message: "Outer metadata contains incomplete YAML",
    })
}

fn parse_fields(yaml: &str) -> Result<Vec<Field>, Problem> {
  let mut options = Options::default();
  options.emit_comments = false;
  let mut parser = Parser::new_from_str_with_options(yaml, options);
  next(&mut parser)?;
  next(&mut parser)?;
  let (root, span) = next(&mut parser)?;
  if !matches!(root, Event::MappingStart(_, _, None)) {
    return Err(Problem {
      location: Some((span.start.line(), span.start.col() + 1)),
      field: None,
      message: "Outer metadata must be one untagged YAML mapping",
    });
  }
  let mut fields = Vec::new();
  let mut anchors = HashMap::new();
  loop {
    let (key, span) = next(&mut parser)?;
    if matches!(key, Event::MappingEnd) {
      break;
    }
    let key = value(&mut parser, (key, span), &mut anchors)?;
    let index = field_index(&key, span)?;
    if fields.iter().any(|field: &Field| field.index == index) {
      return Err(Problem {
        location: Some((span.start.line(), span.start.col() + 1)),
        field: Some(FIELDS[index]),
        message: "Metadata field is duplicated",
      });
    }
    let event = next(&mut parser)?;
    fields.push(Field {
      index,
      span,
      value: value(&mut parser, event, &mut anchors)?,
    });
  }
  let (end, span) = next(&mut parser)?;
  let (stream, _) = next(&mut parser)?;
  if !matches!(end, Event::DocumentEnd) || !matches!(stream, Event::StreamEnd) {
    return Err(Problem {
      location: Some((span.start.line(), span.start.col() + 1)),
      field: None,
      message: "Outer metadata must contain one YAML document",
    });
  }
  Ok(fields)
}

fn field_index(key: &MetadataValue, span: Span) -> Result<usize, Problem> {
  if let MetadataValue::Text(key) = key
    && let Some(index) = FIELDS.iter().position(|field| key == field)
  {
    return Ok(index);
  }
  Err(Problem {
    location: Some((span.start.line(), span.start.col() + 1)),
    field: None,
    message: "Use only title, status, implementation, created_at, updated_at, archived_at, tags and source",
  })
}

fn value<'a>(
  parser: &mut Yaml<'a>,
  event: (Event<'a>, Span),
  anchors: &mut HashMap<usize, Rc<MetadataValue>>,
) -> Result<Rc<MetadataValue>, Problem> {
  let (event, span) = event;
  let (anchor, parsed) = match event {
    Event::Scalar(text, style, anchor, tag) => (
      anchor,
      Rc::new(
        metadata_text(&text, style, tag.as_deref())
          .map_or(MetadataValue::Other, MetadataValue::Text),
      ),
    ),
    Event::SequenceStart(_, anchor, None) => (anchor, tags(parser, anchors)?),
    Event::Alias(anchor) => (
      0,
      anchors.get(&anchor).cloned().ok_or(Problem {
        location: Some((span.start.line(), span.start.col() + 1)),
        field: None,
        message: "Metadata aliases must reference an earlier scalar or flat sequence",
      })?,
    ),
    _ => {
      return Err(Problem {
        location: Some((span.start.line(), span.start.col() + 1)),
        field: None,
        message: "Metadata values must be text or a flat sequence of text tags",
      });
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
    let event = next(parser)?;
    if matches!(event.0, Event::SequenceEnd) {
      return Ok(Rc::new(MetadataValue::Tags(items)));
    }
    if !matches!(event.0, Event::Scalar(..) | Event::Alias(_)) {
      return Err(Problem {
        location: Some((event.1.start.line(), event.1.start.col() + 1)),
        field: Some("tags"),
        message: "Tags must be a flat sequence of text",
      });
    }
    items.push(value(parser, event, anchors)?);
  }
}

fn validate_fields(fields: &[Field]) -> Vec<Problem> {
  let mut problems = Vec::new();
  let mut previous = None;
  for field in fields {
    if previous.is_some_and(|index| index > field.index) {
      problems.push(Problem {
        location: Some((field.span.start.line(), field.span.start.col() + 1)),
        field: Some(FIELDS[field.index]),
        message: "Metadata fields must follow the standard field order",
      });
    }
    previous = Some(field.index);
    if let Some(message) = field_problem(field) {
      problems.push(Problem {
        location: Some((field.span.start.line(), field.span.start.col() + 1)),
        field: Some(FIELDS[field.index]),
        message,
      });
    }
  }
  for index in REQUIRED {
    if fields.iter().all(|field| field.index != index) {
      problems.push(Problem {
        location: None,
        field: Some(FIELDS[index]),
        message: "Required metadata field is missing",
      });
    }
  }
  problems
}

fn field_problem(field: &Field) -> Option<&'static str> {
  if field.index == 6 {
    return (!valid_tags(&field.value))
      .then_some("Tags must be nonempty, distinct lowercase kebab-case text keywords");
  }
  let MetadataValue::Text(text) = field.value.as_ref() else {
    return Some("Metadata field must be text");
  };
  if text.trim().is_empty() {
    return Some("Metadata field must be nonempty text");
  }
  match field.index {
    1 if ![
      "draft",
      "proposed",
      "accepted",
      "rejected",
      "superseded",
      "historical",
    ]
    .contains(&text.as_str()) =>
    {
      Some("Unknown documentary status")
    }
    2 if !["not-started", "in-progress", "implemented", "removed"].contains(&text.as_str()) => {
      Some("Unknown implementation state")
    }
    3..=5 if !valid_date(text) => Some("Date must be a real calendar date in YYYY-MM-DD format"),
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

fn valid_date(text: &str) -> bool {
  let bytes = text.as_bytes();
  if bytes.len() != 10
    || bytes[4] != b'-'
    || bytes[7] != b'-'
    || !bytes
      .iter()
      .enumerate()
      .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
  {
    return false;
  }
  let year = text[..4].parse::<u16>().unwrap_or(0);
  let month = text[5..7].parse::<usize>().unwrap_or(0);
  let day = text[8..].parse::<u8>().unwrap_or(0);
  let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
  let days = [
    0,
    31,
    28 + u8::from(leap),
    31,
    30,
    31,
    30,
    31,
    31,
    30,
    31,
    30,
    31,
  ];
  year > 0 && day > 0 && days.get(month).is_some_and(|maximum| day <= *maximum)
}
