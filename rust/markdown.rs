use granit_parser::{
    Event as YamlEvent, Options as YamlOptions, Parser as YamlParser, ScalarStyle, Tag,
};
use pulldown_cmark::{Event, Options, Parser, Tag as MarkdownTag, TagEnd};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
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
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
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
    text.strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\r'))
        .or_else(|| text.strip_suffix('\n'))
        .unwrap_or(text)
}

fn is_frontmatter_fence(line: &str) -> bool {
    line.trim_end_matches([' ', '\t']) == "---"
}

/// Split source text without normalizing line endings.
pub fn raw_markdown_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < text.len() {
        let byte = text.as_bytes()[index];
        if byte == b'\r' || byte == b'\n' {
            let end = line_end(text, index);
            lines.push(text[start..end].to_owned());
            start = end;
            index = end;
        } else {
            index += text[index..].chars().next().map_or(1, char::len_utf8);
        }
    }
    if start < text.len() {
        lines.push(text[start..].to_owned());
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

pub fn line_content(line: &str) -> &str {
    line_without_ending(line)
}

struct Frontmatter<'a> {
    body: &'a str,
    yaml: &'a str,
}

fn frontmatter(text: &str) -> Option<Frontmatter<'_>> {
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

#[derive(Clone, Debug, PartialEq)]
enum ScalarValue {
    Boolean(bool),
    Null,
    Number(Option<u64>),
    Object,
    String(String),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum KeyIdentity {
    Boolean(bool),
    Null,
    Number(u64),
    String(String),
}

fn number_key(value: f64) -> Option<u64> {
    if value.is_nan() {
        None
    } else if value == 0.0 {
        Some(0.0f64.to_bits())
    } else {
        Some(value.to_bits())
    }
}

fn parse_yaml_number(value: &str) -> Option<f64> {
    static INT: OnceLock<Regex> = OnceLock::new();
    static HEX: OnceLock<Regex> = OnceLock::new();
    static OCT: OnceLock<Regex> = OnceLock::new();
    static NAN_OR_INF: OnceLock<Regex> = OnceLock::new();
    static EXP: OnceLock<Regex> = OnceLock::new();
    static FLOAT: OnceLock<Regex> = OnceLock::new();
    let is_int = INT
        .get_or_init(|| Regex::new(r"^[-+]?[0-9]+$").expect("valid YAML integer regex"))
        .is_match(value);
    let is_hex = HEX
        .get_or_init(|| Regex::new(r"^0x[0-9a-fA-F]+$").expect("valid YAML hex regex"))
        .is_match(value);
    let is_oct = OCT
        .get_or_init(|| Regex::new(r"^0o[0-7]+$").expect("valid YAML octal regex"))
        .is_match(value);
    let is_nan_or_inf = NAN_OR_INF
        .get_or_init(|| {
            Regex::new(r"^(?:[-+]?\.(?:inf|Inf|INF)|\.nan|\.NaN|\.NAN)$")
                .expect("valid YAML non-finite regex")
        })
        .is_match(value);
    let is_exp = EXP
        .get_or_init(|| {
            Regex::new(r"^[-+]?(?:\.[0-9]+|[0-9]+(?:\.[0-9]*)?)[eE][-+]?[0-9]+$")
                .expect("valid YAML exponent regex")
        })
        .is_match(value);
    let is_float = FLOAT
        .get_or_init(|| {
            Regex::new(r"^[-+]?(?:\.[0-9]+|[0-9]+\.[0-9]*)$").expect("valid YAML float regex")
        })
        .is_match(value);
    if is_nan_or_inf {
        return match value {
            ".nan" | ".NaN" | ".NAN" => Some(f64::NAN),
            ".inf" | ".Inf" | ".INF" | "+.inf" | "+.Inf" | "+.INF" => Some(f64::INFINITY),
            "-.inf" | "-.Inf" | "-.INF" => Some(f64::NEG_INFINITY),
            _ => None,
        };
    }
    if is_hex {
        return Some(radix_number(&value[2..], 4));
    }
    if is_oct {
        return Some(radix_number(&value[2..], 3));
    }
    if is_int || is_exp || is_float {
        return value.parse::<f64>().ok();
    }
    None
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
        return head as f64;
    }
    let mut significand = head >> 1;
    if head & 1 != 0 && (sticky || significand & 1 != 0) {
        significand += 1;
    }
    significand as f64 * 2.0_f64.powi(count - 53)
}

fn is_yaml_int(value: &str) -> bool {
    static INT: OnceLock<Regex> = OnceLock::new();
    static HEX: OnceLock<Regex> = OnceLock::new();
    static OCT: OnceLock<Regex> = OnceLock::new();
    INT.get_or_init(|| Regex::new(r"^[-+]?[0-9]+$").expect("valid YAML integer regex"))
        .is_match(value)
        || HEX
            .get_or_init(|| Regex::new(r"^0x[0-9a-fA-F]+$").expect("valid YAML hex regex"))
            .is_match(value)
        || OCT
            .get_or_init(|| Regex::new(r"^0o[0-7]+$").expect("valid YAML octal regex"))
            .is_match(value)
}

fn is_yaml_float(value: &str) -> bool {
    static NAN_OR_INF: OnceLock<Regex> = OnceLock::new();
    static EXP: OnceLock<Regex> = OnceLock::new();
    static FLOAT: OnceLock<Regex> = OnceLock::new();
    NAN_OR_INF
        .get_or_init(|| {
            Regex::new(r"^(?:[-+]?\.(?:inf|Inf|INF)|\.nan|\.NaN|\.NAN)$")
                .expect("valid YAML non-finite regex")
        })
        .is_match(value)
        || EXP
            .get_or_init(|| {
                Regex::new(r"^[-+]?(?:\.[0-9]+|[0-9]+(?:\.[0-9]*)?)[eE][-+]?[0-9]+$")
                    .expect("valid YAML exponent regex")
            })
            .is_match(value)
        || FLOAT
            .get_or_init(|| {
                Regex::new(r"^[-+]?(?:\.[0-9]+|[0-9]+\.[0-9]*)$").expect("valid YAML float regex")
            })
            .is_match(value)
}

fn explicit_tag_name(tag: Option<&Tag>) -> Option<String> {
    let tag = tag?;
    tag.core_suffix().map(str::to_owned).or_else(|| {
        tag.suffix_in_namespace("tag:yaml.org,2002:")
            .map(|suffix| suffix.into_owned())
    })
}

fn scalar_value(value: &str, style: ScalarStyle, tag: Option<&Tag>) -> Option<ScalarValue> {
    let tag_name = explicit_tag_name(tag);
    if tag.is_some() && tag_name.is_none() {
        return Some(ScalarValue::String(value.to_owned()));
    }
    match tag_name.as_deref() {
        Some("binary") => Some(ScalarValue::Object),
        Some("timestamp") => {
            static TIMESTAMP: OnceLock<Regex> = OnceLock::new();
            TIMESTAMP
                .get_or_init(|| Regex::new(r"^[0-9]{4}-[0-9]{1,2}-[0-9]{1,2}(?:(?:t|T|[ \t]+)[0-9]{1,2}:[0-9]{1,2}:[0-9]{1,2}(?:\.[0-9]+)?(?:[ \t]*(?:Z|[-+][012]?[0-9](?::[0-9]{2})?))?)?$").expect("valid YAML timestamp regex"))
                .is_match(value)
                .then_some(ScalarValue::Object)
        }
        Some("null") if matches!(value, "" | "~" | "null" | "Null" | "NULL") => {
            Some(ScalarValue::Null)
        }
        Some("null") => Some(ScalarValue::String(value.to_owned())),
        Some("bool") => match value {
            "true" => Some(ScalarValue::Boolean(true)),
            "True" | "TRUE" => Some(ScalarValue::Boolean(true)),
            "false" => Some(ScalarValue::Boolean(false)),
            "False" | "FALSE" => Some(ScalarValue::Boolean(false)),
            _ => Some(ScalarValue::String(value.to_owned())),
        },
        Some("int") => is_yaml_int(value)
            .then(|| parse_yaml_number(value).map(number_value))
            .flatten()
            .or_else(|| Some(ScalarValue::String(value.to_owned()))),
        Some("float") => is_yaml_float(value)
            .then(|| parse_yaml_number(value).map(number_value))
            .flatten()
            .or_else(|| Some(ScalarValue::String(value.to_owned()))),
        Some("str") | Some(_) => Some(ScalarValue::String(value.to_owned())),
        None if style != ScalarStyle::Plain => Some(ScalarValue::String(value.to_owned())),
        None => {
            if matches!(value, "" | "~" | "null" | "Null" | "NULL") {
                Some(ScalarValue::Null)
            } else if matches!(value, "true" | "True" | "TRUE") {
                Some(ScalarValue::Boolean(true))
            } else if matches!(value, "false" | "False" | "FALSE") {
                Some(ScalarValue::Boolean(false))
            } else if let Some(number) = parse_yaml_number(value) {
                Some(number_value(number))
            } else {
                Some(ScalarValue::String(value.to_owned()))
            }
        }
    }
}

fn number_value(value: f64) -> ScalarValue {
    ScalarValue::Number(number_key(value))
}

fn key_identity(value: &ScalarValue) -> Option<KeyIdentity> {
    match value {
        ScalarValue::Boolean(value) => Some(KeyIdentity::Boolean(*value)),
        ScalarValue::Null => Some(KeyIdentity::Null),
        ScalarValue::Number(Some(value)) => Some(KeyIdentity::Number(*value)),
        ScalarValue::Number(None) | ScalarValue::Object => None,
        ScalarValue::String(value) => Some(KeyIdentity::String(value.clone())),
    }
}

enum NodeResult {
    Object,
    Scalar(ScalarValue),
}

#[derive(Default)]
struct MappingFrame {
    is_root: bool,
    pending_field: Option<RootField>,
    pending_key: bool,
    seen: HashSet<KeyIdentity>,
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

fn field_for_key(value: &NodeResult, is_root: bool) -> Option<RootField> {
    if !is_root {
        return None;
    }
    match value {
        NodeResult::Scalar(ScalarValue::String(value)) if value == "status" => {
            Some(RootField::Status)
        }
        NodeResult::Scalar(ScalarValue::String(value)) if value == "title" => {
            Some(RootField::Title)
        }
        _ => None,
    }
}

fn record_value(state: &mut MetadataState, value: NodeResult) -> std::result::Result<(), ()> {
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
            let identity = match &value {
                NodeResult::Scalar(value) => key_identity(value),
                NodeResult::Object => None,
            };
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
                && let NodeResult::Scalar(ScalarValue::String(value)) = value
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
        let result = match event {
            YamlEvent::StreamStart
            | YamlEvent::StreamEnd
            | YamlEvent::Comment(_, _)
            | YamlEvent::DocumentEnd => {
                if matches!(event, YamlEvent::DocumentEnd)
                    && (!state.frames.is_empty()
                        || state.frames.iter().any(|frame| {
                            matches!(
                                frame,
                                Frame::Mapping(MappingFrame {
                                    pending_key: true,
                                    ..
                                })
                            )
                        }))
                {
                    return None;
                }
                continue;
            }
            YamlEvent::DocumentStart(_, _) => {
                state.document_count += 1;
                if state.document_count > 1 {
                    return None;
                }
                continue;
            }
            YamlEvent::Alias(_) => return None,
            YamlEvent::Scalar(value, style, _, tag) => {
                NodeResult::Scalar(scalar_value(&value, style, tag.as_deref())?)
            }
            YamlEvent::SequenceStart(_, _, _) => {
                state.frames.push(Frame::Sequence);
                continue;
            }
            YamlEvent::MappingStart(_, _, _) => {
                state.frames.push(Frame::Mapping(MappingFrame {
                    is_root: state.frames.is_empty(),
                    ..MappingFrame::default()
                }));
                continue;
            }
            YamlEvent::SequenceEnd => {
                if !matches!(state.frames.pop(), Some(Frame::Sequence)) {
                    return None;
                }
                NodeResult::Object
            }
            YamlEvent::MappingEnd => {
                let Some(Frame::Mapping(mapping)) = state.frames.pop() else {
                    return None;
                };
                if mapping.pending_key {
                    return None;
                }
                NodeResult::Object
            }
            _ => return None,
        };
        record_value(&mut state, result).ok()?;
    }
    if !state.frames.is_empty() || state.document_count > 1 {
        return None;
    }
    Some((
        state.status,
        state
            .title
            .filter(|title| !crate::arguments::trim_js_whitespace(title).is_empty()),
    ))
}

fn heading_text(events: impl IntoIterator<Item = Event<'static>>) -> Option<String> {
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

fn parse_body(body: &str) -> (Option<String>, Vec<String>) {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_GFM);
    let events: Vec<Event<'static>> = Parser::new_ext(body, options)
        .map(Event::into_static)
        .collect();
    let title = heading_text(events.clone());
    let links = events
        .into_iter()
        .filter_map(|event| match event {
            Event::Start(MarkdownTag::Link { dest_url, .. }) if !dest_url.is_empty() => {
                Some(dest_url.into_string())
            }
            _ => None,
        })
        .collect();
    (title, links)
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
