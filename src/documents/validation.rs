use super::markdown::{is_markdown_path, markdown_references};
use super::metadata;
use super::navigation::{Direction, Resolver};
use super::{Document, Project, has_uri_scheme, percent_decode};
use crate::error::{HivexError, Result};
use serde_json::{Value, json};

const MAX_REFERENCES: usize = 32_768;
const MAX_FINDINGS: usize = 2_048;

pub struct Validation {
  pub findings: Vec<Value>,
  pub checked: usize,
  pub checked_metadata: usize,
  pub checked_references: usize,
}

/// Validate outer metadata under `docs/` and the references of ordinary
/// sources. Historical bodies are checked only when selected explicitly.
pub fn validate_sources(
  project: &Project,
  sources: &[String],
  historical: bool,
) -> Result<Validation> {
  if let Some(source) = project.missing(sources) {
    return Err(
      HivexError::new(
        "SOURCE_NOT_FOUND",
        "Validation requires selected source IDs",
      )
      .with_details(json!({"document":source})),
    );
  }
  let explicit = !sources.is_empty();
  let selected = project.documents.iter().filter(|document| {
    if explicit {
      return sources.contains(&document.id);
    }
    historical || !document.historical || metadata::is_documentation(document)
  });
  let mut result = Validation {
    findings: Vec::new(),
    checked: 0,
    checked_metadata: 0,
    checked_references: 0,
  };
  let mut references = 0;
  let mut resolver = Resolver::new(project);
  for source in selected {
    result.checked += 1;
    if metadata::is_documentation(source) {
      result.checked_metadata += 1;
      result.findings.extend(metadata::findings(source));
    }
    // A default check validates archived wrapper metadata but not frozen bodies.
    if !source.historical || historical || explicit {
      result.checked_references += 1;
      if let Err(error) = resolver.relations(&source.id, Direction::Outgoing) {
        result
          .findings
          .push(json!({"document":source.id,"version":source.hash,
          "code":error.code,"message":error.message,"details":error.details}));
      }
      references += check_references(&mut resolver, source, &mut result.findings)?;
      if references > MAX_REFERENCES {
        return Err(reference_limit());
      }
    }
    if result.findings.len() > MAX_FINDINGS {
      return Err(finding_limit());
    }
  }
  Ok(result)
}

fn reference_limit() -> HivexError {
  HivexError::new(
    "CHECK_LIMIT",
    "At most 32768 local Markdown references can be validated; narrow source selection",
  )
}

fn finding_limit() -> HivexError {
  HivexError::new(
    "CHECK_LIMIT",
    "Validation exceeds 2048 findings; narrow source selection",
  )
}

/// Whether a Markdown link destination refers to a local Markdown source or anchor.
fn is_local_markdown(target: &str) -> bool {
  if target.is_empty() || target.starts_with("//") || has_uri_scheme(target) {
    return false;
  }
  let path = target.split(['#', '?']).next().unwrap_or_default();
  path.is_empty() || is_markdown_path(&percent_decode(path).unwrap_or_else(|| path.to_owned()))
}

/// Append a finding for each unresolved local reference; return how many were checked.
fn check_references(
  resolver: &mut Resolver<'_>,
  source: &Document,
  findings: &mut Vec<Value>,
) -> Result<usize> {
  let mut references = 0;
  for (target, line) in markdown_references(&source.text) {
    if !is_local_markdown(&target) {
      continue;
    }
    references += 1;
    if references > MAX_REFERENCES {
      return Err(
        reference_limit()
          .with_details(json!({"document":source.id,"line":line,"limit":MAX_REFERENCES})),
      );
    }
    let Err(error) = resolver.source_reference(source, &target, line) else {
      continue;
    };
    let code = match error.code.as_str() {
      "INVALID_RELATION" => "INVALID_REFERENCE",
      code => code,
    };
    findings.push(
      json!({"document":source.id,"version":source.hash,"line":line,"target":target,
      "code":code,"message":error.message,"details":error.details}),
    );
    if findings.len() > MAX_FINDINGS {
      return Err(finding_limit());
    }
  }
  Ok(references)
}
