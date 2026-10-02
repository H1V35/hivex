use super::markdown::markdown_references;
use super::metadata;
use super::navigation::Resolver;
use super::{Document, Project, has_uri_scheme, is_markdown_path, percent_decode};
use crate::error::{HivexError, Result};
use serde_json::{Value, json};

const MAX_REFERENCES: usize = 32_768;

pub struct Validation {
  pub findings: Vec<Value>,
  pub checked: usize,
  pub checked_metadata: usize,
  pub checked_references: usize,
}

pub fn validate_sources(
  project: &Project,
  sources: &[String],
  historical: bool,
) -> Result<Validation> {
  for source in sources {
    if project
      .documents
      .iter()
      .all(|document| document.id != *source)
    {
      return Err(
        HivexError::new(
          "SOURCE_NOT_FOUND",
          "Validation requires selected source IDs",
        )
        .with_details(json!({"document":source})),
      );
    }
  }
  let mut result = Validation {
    findings: Vec::new(),
    checked: 0,
    checked_metadata: 0,
    checked_references: 0,
  };
  let mut references = 0;
  let mut resolver = Resolver::new(project);
  for source in project.documents.iter().filter(|document| {
    if !sources.is_empty() {
      return sources.contains(&document.id);
    }
    historical || !document.historical || metadata::is_documentation(document)
  }) {
    result.checked += 1;
    if metadata::is_documentation(source) {
      result.checked_metadata += 1;
      result.findings.extend(metadata::findings(source));
    }
    if source.historical && !historical && sources.is_empty() {
      check_finding_limit(&result.findings)?;
      continue;
    }
    result.checked_references += 1;
    if let Err(error) = resolver.relations(&source.id, "outgoing") {
      result.findings.push(json!({"document":source.id,"version":source.hash,"code":error.code,"message":error.message,"details":error.details}));
    }
    let (findings, count) = reference_findings(&mut resolver, source)?;
    references += count;
    if references > MAX_REFERENCES {
      return Err(HivexError::new(
        "CHECK_LIMIT",
        "At most 32768 local Markdown references can be validated; narrow source selection",
      ));
    }
    result.findings.extend(findings);
    check_finding_limit(&result.findings)?;
  }
  Ok(result)
}

fn check_finding_limit(findings: &[Value]) -> Result<()> {
  if findings.len() > 2048 {
    return Err(HivexError::new(
      "CHECK_LIMIT",
      "Validation exceeds 2048 findings; narrow source selection",
    ));
  }
  Ok(())
}

fn reference_findings(
  resolver: &mut Resolver<'_>,
  source: &Document,
) -> Result<(Vec<Value>, usize)> {
  let mut findings = Vec::new();
  let mut references = 0;
  for (target, line) in markdown_references(&source.text) {
    if target.is_empty() || target.starts_with("//") || has_uri_scheme(&target) {
      continue;
    }
    let path = target.split(['#', '?']).next().unwrap_or_default();
    let decoded = percent_decode(path);
    if !path.is_empty() && !is_markdown_path(decoded.as_deref().unwrap_or(path)) {
      continue;
    }
    references += 1;
    if references > MAX_REFERENCES {
      return Err(
        HivexError::new(
          "CHECK_LIMIT",
          "At most 32768 local Markdown references can be validated; narrow source selection",
        )
        .with_details(json!({"document":source.id,"line":line,"limit":MAX_REFERENCES})),
      );
    }
    if let Err(error) = resolver.source_reference(source, &target, line) {
      findings.push(reference_finding(source, &target, line, &error));
    }
    if findings.len() > 2048 {
      return Err(HivexError::new(
        "CHECK_LIMIT",
        "Validation exceeds 2048 findings; narrow source selection",
      ));
    }
  }
  Ok((findings, references))
}

fn reference_finding(source: &Document, target: &str, line: usize, error: &HivexError) -> Value {
  let code = match error.code.as_str() {
    "INVALID_RELATION" => "INVALID_REFERENCE",
    code => code,
  };
  json!({"document":source.id,"version":source.hash,"line":line,"target":target,
    "code":code,"message":error.message,"details":error.details})
}
