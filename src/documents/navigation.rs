use super::relations::{Anchor, DraftRelation, MAX_RELATIONS, RelationKind, anchors, parse};
use super::{Document, Project, has_uri_scheme, normalize_path, percent_decode, raw_line_ranges};
use crate::error::{HivexError, Result};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
  pub document: String,
  pub anchor: Option<String>,
  pub version: String,
  pub historical: bool,
  pub line_start: usize,
  pub line_end: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthoredRelation {
  pub kind: RelationKind,
  pub literal: &'static str,
  pub from: Reference,
  pub to: Reference,
  pub reason: String,
}

fn invalid(source: &Document, line: usize, message: &str) -> HivexError {
  HivexError::new("INVALID_RELATION", message).with_details(serde_json::json!({
    "document":source.id,"line":line,"version":source.hash
  }))
}

fn target_parts<'a>(
  source: &Document,
  raw: &'a str,
  line: usize,
) -> Result<(&'a str, Option<&'a str>)> {
  if raw.split('#').next().is_some_and(|path| path.contains('?'))
    || has_uri_scheme(raw)
    || Path::new(raw).is_absolute()
  {
    return Err(invalid(
      source,
      line,
      "Relations require a relative Markdown target",
    ));
  }
  let (path, anchor) = raw
    .split_once('#')
    .map_or((raw, None), |(path, anchor)| (path, Some(anchor)));
  if path.is_empty() && anchor.is_none()
    || anchor.is_some_and(|id| id.is_empty() || id.contains('#'))
  {
    return Err(invalid(
      source,
      line,
      "A relation target or anchor is empty or ambiguous",
    ));
  }
  Ok((path, anchor))
}

fn target_document<'a>(
  project: &'a Project,
  source: &Document,
  raw: &str,
  line: usize,
) -> Result<&'a Document> {
  let (path, _) = target_parts(source, raw, line)?;
  let decoded =
    percent_decode(path).ok_or_else(|| invalid(source, line, "Invalid target percent encoding"))?;
  if Path::new(&decoded).is_absolute() || decoded.contains('\\') {
    return Err(invalid(
      source,
      line,
      "Relations require a relative Markdown target",
    ));
  }
  let absolute = if decoded.is_empty() {
    project.root.join(&source.path)
  } else {
    project
      .root
      .join(&source.path)
      .parent()
      .expect("source has root")
      .join(decoded)
  };
  let normalized = normalize_path(&absolute);
  let relative = normalized
    .strip_prefix(&project.root)
    .map_err(|_| invalid(source, line, "Relation target escapes the project"))?;
  let id = relative.to_string_lossy().replace('\\', "/");
  project
    .documents
    .iter()
    .find(|document| document.id == id)
    .ok_or_else(|| {
      invalid(
        source,
        line,
        "Relation target is not a selected Markdown source",
      )
    })
}

struct SourceIndex {
  line_count: usize,
  anchors: Option<Result<Vec<Anchor>>>,
}

pub(super) struct Resolver<'a> {
  project: &'a Project,
  indexes: HashMap<String, SourceIndex>,
  anchor_count: usize,
}

impl<'a> Resolver<'a> {
  pub(super) fn new(project: &'a Project) -> Self {
    Self {
      project,
      indexes: HashMap::new(),
      anchor_count: 0,
    }
  }
  fn reference(&mut self, document: &Document, anchor: Option<&str>) -> Result<Reference> {
    let index = self
      .indexes
      .entry(document.id.clone())
      .or_insert_with(|| SourceIndex {
        line_count: raw_line_ranges(&document.text).count(),
        anchors: None,
      });
    let (identifier, line_start, line_end) = match anchor {
      None => (None, 1, index.line_count),
      Some(id) => {
        let id =
          percent_decode(id).ok_or_else(|| invalid(document, 1, "Invalid anchor encoding"))?;
        let count = &mut self.anchor_count;
        let found = index
          .anchors
          .get_or_insert_with(|| {
            let records = anchors(&document.text)?;
            if *count + records.len() > 65536 {
              return Err(HivexError::new(
                "ANCHOR_LIMIT",
                "A query can retain at most 65536 target anchors; narrow source selection",
              ));
            }
            *count += records.len();
            Ok(records)
          })
          .as_ref()
          .map_err(Clone::clone)?
          .iter()
          .find(|anchor| anchor.id == id)
          .ok_or_else(|| invalid(document, 1, "Relation anchor does not exist"))?;
        (Some(id), found.line_start, found.line_end)
      }
    };
    Ok(Reference {
      document: document.id.clone(),
      anchor: identifier,
      version: document.hash.clone(),
      historical: document.historical,
      line_start,
      line_end,
    })
  }

  pub(super) fn source_reference(
    &mut self,
    source: &Document,
    target: &str,
    line: usize,
  ) -> Result<Reference> {
    let (prefix, fragment) = target
      .split_once('#')
      .map_or((target, None), |(path, fragment)| (path, Some(fragment)));
    let path = prefix.split('?').next().unwrap_or_default();
    let document = if path.is_empty() {
      source
    } else {
      target_document(self.project, source, path, line)?
    };
    self.reference(document, fragment)
  }

  fn resolve(&mut self, source: &Document, relation: DraftRelation) -> Result<AuthoredRelation> {
    let target = target_document(self.project, source, &relation.target, relation.line_start)?;
    let (_, anchor) = target_parts(source, &relation.target, relation.line_start)?;
    let mut from = self.reference(source, None)?;
    from.line_start = relation.line_start;
    from.line_end = relation.line_end;
    let to = self.reference(target, anchor).map_err(|mut error| {
      error.details = Some(serde_json::json!({
        "document":source.id,"line":relation.line_start,
        "target":relation.target,"targetDetails":error.details
      }));
      error
    })?;
    Ok(AuthoredRelation {
      kind: relation.kind,
      literal: relation.kind.literal(),
      from,
      to,
      reason: relation.reason,
    })
  }

  pub(super) fn relations(&mut self, id: &str, direction: &str) -> Result<Vec<AuthoredRelation>> {
    let project = self.project;
    let selected = project
      .documents
      .iter()
      .find(|source| source.id == id)
      .ok_or_else(|| {
        HivexError::new(
          "SOURCE_NOT_FOUND",
          "Relations require a selected document ID",
        )
      })?;
    let sources = project.documents.iter().filter(|source| {
      if direction == "outgoing" {
        return source.id == id;
      }
      !source.historical || source.id == selected.id
    });
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    let mut count = 0;
    for source in sources {
      let declarations = parse(&source.text).map_err(|mut error| {
        error.details.get_or_insert(serde_json::json!({}))["document"] =
          serde_json::json!(source.id);
        error.details.as_mut().expect("details set")["version"] = serde_json::json!(source.hash);
        error
      })?;
      for relation in declarations {
        count += 1;
        if count > MAX_RELATIONS {
          return Err(HivexError::new("RELATION_LIMIT", "At most 2048 authored declarations are supported per query")
          .with_details(serde_json::json!({"document":source.id,"line":relation.line_start,"limit":MAX_RELATIONS})));
        }
        let resolved = self.resolve(source, relation)?;
        let identity = serde_json::to_string(&(
          &resolved.literal,
          &resolved.from.document,
          &resolved.to.document,
          &resolved.to.anchor,
          &resolved.reason,
        ))?;
        if !seen.insert(identity) {
          return Err(invalid(
            source,
            resolved.from.line_start,
            "Duplicate authored relationship declaration",
          ));
        }
        let outgoing = resolved.from.document == id;
        let incoming = resolved.to.document == id;
        if outgoing && direction != "incoming" || incoming && direction != "outgoing" {
          result.push(resolved);
        }
      }
    }
    result.sort_by(|left, right| {
      (
        &left.from.document,
        left.from.line_start,
        &left.to.document,
        &left.to.anchor,
      )
        .cmp(&(
          &right.from.document,
          right.from.line_start,
          &right.to.document,
          &right.to.anchor,
        ))
    });
    Ok(result)
  }
}

pub fn relations(project: &Project, id: &str, direction: &str) -> Result<Vec<AuthoredRelation>> {
  Resolver::new(project).relations(id, direction)
}
