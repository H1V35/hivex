mod collation;
mod markdown;
mod metadata;
mod navigation;
mod relations;
mod search;
mod selection;
mod validation;

pub(crate) use markdown::{hash, line_content, raw_line_ranges};
pub(crate) use navigation::{AuthoredRelation, Direction, relations as authored_relations};
pub(crate) use search::{Match, Query, search as search_sources};
pub(crate) use validation::validate_sources;

use crate::compatibility::{normalize_path, project_directory};
use crate::error::{HivexError, Result};
use collation::{compare_paths, compare_serialized};
use markdown::MARKDOWN_EXTENSIONS;
use selection::{Candidate, Config, GENERATED_DIRECTORIES, PROTECTED_DIRECTORIES};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const MAX_DOCUMENTS: usize = 2_048;
const MAX_SOURCE_BYTES: usize = 32 * 1024 * 1024;
const MAX_CORPUS_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Warning {
  pub path: String,
  pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
  /// Root-relative `/`-separated path, also used as the document ID.
  pub id: String,
  pub title: String,
  pub text: String,
  /// SHA-256 of `text`, the document version.
  pub hash: String,
  pub status: Option<String>,
  /// Distinct root-relative Markdown targets of the document's local links.
  pub links: Vec<String>,
  pub historical: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
  pub root: PathBuf,
  /// Identity of the selected documents' versions and the selection rules.
  pub snapshot: String,
  pub documents: Vec<Document>,
  /// Sources that could not be inspected or loaded; their presence makes coverage partial.
  pub warnings: Vec<Warning>,
}

impl Project {
  pub fn document(&self, id: &str) -> Option<&Document> {
    self.documents.iter().find(|document| document.id == id)
  }

  /// The first requested ID that is not a selected document.
  pub fn missing<'a>(&self, ids: &'a [String]) -> Option<&'a String> {
    ids.iter().find(|id| self.document(id).is_none())
  }

  pub fn is_partial(&self) -> bool {
    !self.warnings.is_empty()
  }
}

fn failure(code: &str, message: impl Into<String>, details: serde_json::Value) -> HivexError {
  HivexError::new(code, message).with_details(details)
}

/// Load the selected Markdown of the project at `root` from the working copy.
pub fn load_project(root: &str) -> Result<Project> {
  let root = project_directory(root)?;
  let config = Config::load(&root)?;
  let mut warnings = Vec::new();
  let candidates = selection::discover(&root, &config, &mut warnings);
  let mut documents = Vec::new();
  let mut corpus_bytes = 0;
  for candidate in candidates.iter().take(MAX_DOCUMENTS) {
    match read_document(
      &root,
      candidate,
      MAX_CORPUS_BYTES.saturating_sub(corpus_bytes),
    ) {
      Ok(document) => {
        corpus_bytes += document.text.len();
        documents.push(document);
      }
      Err(error) => warnings.push(Warning {
        path: candidate.path.clone(),
        message: error.message,
      }),
    }
  }
  if candidates.len() > MAX_DOCUMENTS {
    warnings.push(Warning {
      path: ".".to_owned(),
      message: format!("Only the first {MAX_DOCUMENTS} Markdown sources were loaded"),
    });
  }
  documents.sort_by(|left, right| compare_paths(&left.id, &right.id));
  let snapshot = snapshot(&documents, &config);
  Ok(Project {
    root,
    snapshot,
    documents,
    warnings,
  })
}

fn read_document(root: &Path, candidate: &Candidate, budget: usize) -> Result<Document> {
  let unreadable = |error: std::io::Error| {
    failure(
      "SOURCE_READ_FAILED",
      "Unable to read Markdown source",
      json!({"path": candidate.path, "reason": error.to_string()}),
    )
  };
  let size = fs::symlink_metadata(&candidate.absolute_path)
    .map_err(unreadable)?
    .len();
  if size > budget as u64 {
    return Err(HivexError::new(
      "CORPUS_LIMIT",
      "Selected Markdown exceeds the 64 MiB memory budget; narrow include paths",
    ));
  }
  let bytes = fs::read(&candidate.absolute_path).map_err(unreadable)?;
  let text = decode_utf8(bytes, &candidate.path, MAX_SOURCE_BYTES)?;
  let description = markdown::describe(&candidate.path, &text);
  let mut links: Vec<String> = Vec::new();
  for link in description.links {
    if let Some(link) = local_markdown_link(root, &candidate.path, &link)
      && !links.contains(&link)
    {
      links.push(link);
    }
  }
  Ok(Document {
    id: candidate.path.clone(),
    title: description.title,
    hash: hash(&text),
    text,
    status: description.status,
    links,
    historical: candidate.historical,
  })
}

fn decode_utf8(bytes: Vec<u8>, path: &str, limit: usize) -> Result<String> {
  if bytes.len() > limit {
    return Err(failure(
      "DOCUMENT_TOO_LARGE",
      format!("Markdown source exceeds {limit} bytes"),
      json!({"actualBytes": bytes.len(), "maxBytes": limit, "path": path}),
    ));
  }
  String::from_utf8(bytes).map_err(|_| {
    failure(
      "INVALID_UTF8",
      "Markdown source is not valid UTF-8",
      json!({"path": path}),
    )
  })
}

/// Decode `%XX` escapes; `None` for a malformed escape or non-UTF-8 result.
fn percent_decode(value: &str) -> Option<String> {
  let mut decoded = Vec::with_capacity(value.len());
  let mut bytes = value.bytes();
  while let Some(byte) = bytes.next() {
    if byte != b'%' {
      decoded.push(byte);
      continue;
    }
    let high = char::from(bytes.next()?).to_digit(16)?;
    let low = char::from(bytes.next()?).to_digit(16)?;
    decoded.push(u8::try_from(high << 4 | low).ok()?);
  }
  String::from_utf8(decoded).ok()
}

/// RFC 3986 scheme prefix such as `https:` or `mailto:`.
fn has_uri_scheme(value: &str) -> bool {
  let Some((scheme, _)) = value.split_once(':') else {
    return false;
  };
  scheme.starts_with(|character: char| character.is_ascii_alphabetic())
    && scheme
      .chars()
      .all(|character| character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.'))
}

/// Resolve a link written in `source` to a selected-root Markdown path, if it is one.
fn local_markdown_link(root: &Path, source: &str, link: &str) -> Option<String> {
  if link.starts_with('#') || has_uri_scheme(link) {
    return None;
  }
  let target = &link[..link.find(['?', '#']).unwrap_or(link.len())];
  if target.is_empty() {
    return None;
  }
  let decoded = percent_decode(target)?;
  let parent = root.join(source);
  let parent = parent.parent().unwrap_or(root);
  let relative = selection::relative_path(root, &normalize_path(&parent.join(decoded)));
  let markdown = markdown::is_markdown_path(&relative);
  (!relative.is_empty() && markdown).then_some(relative)
}

fn sorted(values: impl Iterator<Item = impl Into<String>>) -> Vec<String> {
  let mut values: Vec<String> = values.map(Into::into).collect();
  values.sort_by(|left, right| compare_serialized(left, right));
  values
}

fn snapshot(documents: &[Document], config: &Config) -> String {
  let identities = sorted(
    documents
      .iter()
      .map(|document| format!("{}\0{}", document.id, document.hash)),
  );
  let ignored = PROTECTED_DIRECTORIES.iter().chain(&GENERATED_DIRECTORIES);
  // `history` preserves v1 identities after the field was renamed to `archive`.
  let selection = json!({
    "include": sorted(config.include.sources()),
    "exclude": sorted(config.exclude.sources()),
    "history": sorted(config.archive.sources()),
    "ignoredDirectories": sorted(ignored.copied()),
    "markdownExtensions": MARKDOWN_EXTENSIONS,
  });
  hash(&format!(
    "{}\nselection\0{selection}",
    identities.join("\n")
  ))
}
