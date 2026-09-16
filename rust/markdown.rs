use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt;
use std::path::Path;

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

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum YamlKey {
    Boolean(bool),
    Null,
    Number(u64),
    Other,
    String(String),
}

fn number_key(value: f64) -> u64 {
    if value == 0.0 {
        0.0f64.to_bits()
    } else if value.is_nan() {
        f64::NAN.to_bits()
    } else {
        value.to_bits()
    }
}

struct YamlKeyVisitor;

impl<'de> Visitor<'de> for YamlKeyVisitor {
    type Value = YamlKey;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a YAML mapping key")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(YamlKey::Boolean(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(YamlKey::Number(number_key(value as f64)))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(YamlKey::Number(number_key(value as f64)))
    }

    fn visit_i128<E>(self, value: i128) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(YamlKey::Number(number_key(value as f64)))
    }

    fn visit_u128<E>(self, value: u128) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(YamlKey::Number(number_key(value as f64)))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(YamlKey::Number(number_key(value)))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(YamlKey::Null)
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(YamlKey::Null)
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(YamlKey::String(value.to_owned()))
    }

    fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(YamlKey::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(YamlKey::String(value))
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence.next_element::<SkipValue>()?.is_some() {}
        Ok(YamlKey::Other)
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut seen = HashSet::new();
        while let Some(key) = map.next_key::<YamlKey>()? {
            if !matches!(key, YamlKey::Other) && !seen.insert(key) {
                return Err(de::Error::custom("duplicate mapping key"));
            }
            map.next_value::<SkipValue>()?;
        }
        Ok(YamlKey::Other)
    }
}

impl<'de> Deserialize<'de> for YamlKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(YamlKeyVisitor)
    }
}

struct SkipValue;

struct SkipValueVisitor;

impl<'de> Visitor<'de> for SkipValueVisitor {
    type Value = SkipValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("any YAML value")
    }

    fn visit_bool<E>(self, _: bool) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(SkipValue)
    }

    fn visit_i64<E>(self, _: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(SkipValue)
    }

    fn visit_u64<E>(self, _: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(SkipValue)
    }

    fn visit_i128<E>(self, _: i128) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(SkipValue)
    }

    fn visit_u128<E>(self, _: u128) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(SkipValue)
    }

    fn visit_f32<E>(self, _: f32) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(SkipValue)
    }

    fn visit_f64<E>(self, _: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(SkipValue)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(SkipValue)
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(SkipValue)
    }

    fn visit_str<E>(self, _: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(SkipValue)
    }

    fn visit_borrowed_str<E>(self, _: &'de str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(SkipValue)
    }

    fn visit_string<E>(self, _: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(SkipValue)
    }

    fn visit_bytes<E>(self, _: &[u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(SkipValue)
    }

    fn visit_byte_buf<E>(self, _: Vec<u8>) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(SkipValue)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence.next_element::<SkipValue>()?.is_some() {}
        Ok(SkipValue)
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut seen = HashSet::new();
        while let Some(key) = map.next_key::<YamlKey>()? {
            if !matches!(key, YamlKey::Other) && !seen.insert(key) {
                return Err(de::Error::custom("duplicate mapping key"));
            }
            map.next_value::<SkipValue>()?;
        }
        Ok(SkipValue)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        SkipValue::deserialize(deserializer)
    }

    fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        SkipValue::deserialize(deserializer)
    }
}

impl<'de> Deserialize<'de> for SkipValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(SkipValueVisitor)
    }
}

struct Metadata {
    status: Option<String>,
    title: Option<String>,
}

struct MetadataVisitor;

struct OptionalString(Option<String>);

struct OptionalStringVisitor;

impl<'de> Visitor<'de> for OptionalStringVisitor {
    type Value = OptionalString;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a YAML scalar")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(OptionalString(Some(value.to_owned())))
    }

    fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(OptionalString(Some(value.to_owned())))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(OptionalString(Some(value)))
    }

    fn visit_bool<E>(self, _: bool) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(OptionalString(None))
    }

    fn visit_i64<E>(self, _: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(OptionalString(None))
    }

    fn visit_u64<E>(self, _: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(OptionalString(None))
    }

    fn visit_i128<E>(self, _: i128) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(OptionalString(None))
    }

    fn visit_u128<E>(self, _: u128) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(OptionalString(None))
    }

    fn visit_f64<E>(self, _: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(OptionalString(None))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(OptionalString(None))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(OptionalString(None))
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence.next_element::<SkipValue>()?.is_some() {}
        Ok(OptionalString(None))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        while map.next_key::<YamlKey>()?.is_some() {
            map.next_value::<SkipValue>()?;
        }
        Ok(OptionalString(None))
    }
}

impl<'de> Deserialize<'de> for OptionalString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(OptionalStringVisitor)
    }
}

impl<'de> Visitor<'de> for MetadataVisitor {
    type Value = Metadata;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a YAML mapping")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut seen = HashSet::new();
        let mut status = None;
        let mut title = None;
        while let Some(key) = map.next_key::<YamlKey>()? {
            if !matches!(key, YamlKey::Other) && !seen.insert(key.clone()) {
                return Err(de::Error::custom("duplicate mapping key"));
            }
            match key {
                YamlKey::String(key) if key == "status" => {
                    status = map.next_value::<OptionalString>()?.0;
                }
                YamlKey::String(key) if key == "title" => {
                    title = map.next_value::<OptionalString>()?.0;
                }
                _ => {
                    map.next_value::<SkipValue>()?;
                }
            }
        }
        Ok(Metadata { status, title })
    }
}

impl<'de> Deserialize<'de> for Metadata {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(MetadataVisitor)
    }
}

fn metadata(yaml: &str) -> (Option<String>, Option<String>) {
    let max_depth = yaml.len().saturating_add(1).max(256);
    let options = serde_saphyr::options! {
        budget: serde_saphyr::budget! {
            max_depth: max_depth,
            flow_nesting_limit: max_depth,
        },
        // Typed visitors retain JavaScript's numeric key equality without
        // conflating numbers and explicitly tagged strings.
        duplicate_keys: serde_saphyr::DuplicateKeyPolicy::Error,
        merge_keys: serde_saphyr::MergeKeyPolicy::AsOrdinary,
        alias_limits: serde_saphyr::alias_limits! {
            max_total_replayed_events: 0,
        },
        strict_booleans: true,
    };
    let value =
        serde_saphyr::with_deserializer_from_str_with_options(yaml, options, |deserializer| {
            let mut deserializer = serde_stacker::Deserializer::new(deserializer);
            // YAML's debug deserializer frames exceed the adapter's 64 KiB default.
            deserializer.red_zone = 1024 * 1024;
            deserializer.stack_size = 8 * 1024 * 1024;
            Metadata::deserialize(deserializer)
        });
    let Ok(value) = value else {
        return (None, None);
    };
    (
        value.status,
        value.title.filter(|value| !value.trim().is_empty()),
    )
}

fn heading_text(events: impl IntoIterator<Item = Event<'static>>) -> Option<String> {
    let mut depth = 0usize;
    let mut active = false;
    let mut text = String::new();
    for event in events {
        match event {
            Event::Start(Tag::Heading { .. }) => {
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
            Event::Start(Tag::Link { dest_url, .. }) if !dest_url.is_empty() => {
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
    let (status, front_title) = yaml.map_or((None, None), metadata);
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
