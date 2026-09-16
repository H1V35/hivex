use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use serde_json::Value;
use sha2::{Digest, Sha256};
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

fn metadata(yaml: &str) -> (Option<String>, Option<String>) {
    let options = serde_saphyr::options! {
        duplicate_keys: serde_saphyr::DuplicateKeyPolicy::Error,
        merge_keys: serde_saphyr::MergeKeyPolicy::AsOrdinary,
        alias_limits: serde_saphyr::alias_limits! {
            max_total_replayed_events: 0,
        },
        strict_booleans: true,
    };
    let Ok(value) = serde_saphyr::from_str_with_options::<serde_json::Value>(yaml, options) else {
        return (None, None);
    };
    let Some(value) = value.as_object() else {
        return (None, None);
    };
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let title = value
        .get("title")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .filter(|value| !value.trim().is_empty());
    (status, title)
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
