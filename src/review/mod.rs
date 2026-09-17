pub(crate) use implementation::{Implementation, capture_implementation, parse_implementation};
mod implementation;
use crate::documents::{Project, load_project};
use crate::error::{HivexError, Result};
use crate::execution::OutputSchema;
use crate::knowledge::{Citation, SuppliedDocument, source_evidence, supplied_citation};

use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_REVIEW_BYTES: u64 = 1_048_576;
const GUIDANCE: &str =
  "Current means the versions still match, not that the implementation is approved.";
pub const REVIEW_INSTRUCTIONS: &str = "Assist the principal reviewer with the task and implementation diff. Discover possible conflicts without requiring suspicions. Explain how documentary rules, direct/indirect dependencies, conditions and exceptions apply. Findings may identify a conflict, a valid exception, or uncertainty; do not turn missing context into approval or reject the entire change. Cite the supplied Markdown ranges and before/after code lines supporting each finding. Distinguish a rule violated by the change from behavior merely seen in context. The reviewer must verify each finding. This is knowledge assistance, not general code review, lint, tests or implementation approval.";

fn error(code: &str, message: impl Into<String>) -> HivexError {
  HivexError::new(code, message)
}

fn invalid_review() -> HivexError {
  error("INVALID_REVIEW", "Review output does not match its schema.")
}

fn code_evidence(citation: &Value, implementation: &Implementation) -> Option<Value> {
  let line_start = citation
    .get("lineStart")?
    .as_u64()
    .and_then(|line| usize::try_from(line).ok())?;
  let line_end = citation
    .get("lineEnd")?
    .as_u64()
    .and_then(|line| usize::try_from(line).ok())?;
  if line_end < line_start {
    return None;
  }
  let path = citation.get("path")?.as_str()?;
  let side = citation.get("side")?.as_str()?;
  let file = implementation.files.iter().find(|file| file.path == path)?;
  let version = match side {
    "before" => file.before.as_ref()?,
    "after" => file.after.as_ref()?,
    _ => return None,
  };
  let expected = line_end.checked_sub(line_start)?.checked_add(1)?;
  let lines = version
    .lines
    .iter()
    .filter(|(number, _)| *number >= line_start && *number <= line_end)
    .collect::<Vec<_>>();
  if lines.len() != expected {
    return None;
  }
  let mut evidence = Map::new();
  evidence.insert("lineEnd".to_owned(), Value::from(line_end));
  evidence.insert("lineStart".to_owned(), Value::from(line_start));
  evidence.insert("path".to_owned(), Value::String(path.to_owned()));
  evidence.insert("side".to_owned(), Value::String(side.to_owned()));
  evidence.insert(
    "text".to_owned(),
    Value::String(
      lines
        .into_iter()
        .map(|(_, text)| text.as_str())
        .collect::<Vec<_>>()
        .join("\n"),
    ),
  );
  evidence.insert("version".to_owned(), Value::String(version.version.clone()));
  Some(Value::Object(evidence))
}

fn document_evidence(
  citation: &Value,
  project: &Project,
  supplied: &[SuppliedDocument],
) -> Option<Value> {
  let citation = serde_json::from_value::<Citation>(citation.clone()).ok()?;
  if !supplied_citation(&citation, supplied) {
    return None;
  }
  source_evidence(&citation, project).and_then(|evidence| serde_json::to_value(evidence).ok())
}

pub fn materialize_review(
  project: &Project,
  implementation: &Value,
  supplied: &[SuppliedDocument],
  value: &Value,
) -> Result<Value> {
  let implementation = parse_implementation(implementation)?;
  let response = OutputSchema::Review
    .parse(value)
    .ok_or_else(invalid_review)?;
  let findings = response
    .get("findings")
    .and_then(Value::as_array)
    .ok_or_else(invalid_review)?;
  let mut materialized = Vec::with_capacity(findings.len());
  for finding in findings {
    let original_documents = finding
      .get("documents")
      .and_then(Value::as_array)
      .ok_or_else(invalid_review)?;
    let original_code = finding
      .get("code")
      .and_then(Value::as_array)
      .ok_or_else(invalid_review)?;
    let documents = original_documents
      .iter()
      .filter_map(|citation| document_evidence(citation, project, supplied))
      .collect::<Vec<_>>();
    let code = original_code
      .iter()
      .filter_map(|citation| code_evidence(citation, &implementation))
      .collect::<Vec<_>>();
    let references_verified = !documents.is_empty()
      && !code.is_empty()
      && documents.len() == original_documents.len()
      && code.len() == original_code.len();
    let mut output = Map::new();
    output.insert(
      "assessment".to_owned(),
      Value::String(if references_verified {
        finding
          .get("assessment")
          .and_then(Value::as_str)
          .ok_or_else(invalid_review)?
          .to_owned()
      } else {
        "uncertain".to_owned()
      }),
    );
    output.insert("code".to_owned(), Value::Array(code));
    output.insert("documents".to_owned(), Value::Array(documents));
    output.insert(
      "explanation".to_owned(),
      finding
        .get("explanation")
        .cloned()
        .ok_or_else(invalid_review)?,
    );
    output.insert(
      "referencesVerified".to_owned(),
      Value::Bool(references_verified),
    );
    materialized.push(Value::Object(output));
  }
  let invalid_references = materialized
    .iter()
    .any(|finding| finding.get("referencesVerified").and_then(Value::as_bool) == Some(false));
  Ok(serde_json::json!({
      "findings": materialized,
      "invalidReferences": invalid_references,
      "uncertainties": response.get("uncertainties").cloned().ok_or_else(invalid_review)?,
  }))
}

pub fn review_binding(project: &Project, implementation: &Value) -> Value {
  let mut binding = Map::new();
  binding.insert(
    "baseCommit".to_owned(),
    implementation
      .get("baseCommit")
      .cloned()
      .unwrap_or(Value::Null),
  );
  binding.insert(
    "documents".to_owned(),
    Value::String(project.snapshot.clone()),
  );
  binding.insert(
    "implementation".to_owned(),
    implementation
      .get("fingerprint")
      .cloned()
      .unwrap_or(Value::Null),
  );
  Value::Object(binding)
}

fn is_lower_hex(value: &str, minimum: usize, maximum: usize) -> bool {
  (minimum..=maximum).contains(&value.len())
    && value
      .bytes()
      .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

#[derive(Clone, Debug)]
struct ReviewBinding {
  base_commit: String,
  documents: String,
  implementation: String,
}

fn parse_binding(value: &Value) -> Result<ReviewBinding> {
  let object = value.as_object().ok_or_else(invalid_review)?;
  let base_commit = object
    .get("baseCommit")
    .and_then(Value::as_str)
    .filter(|value| is_lower_hex(value, 40, 64))
    .ok_or_else(invalid_review)?;
  let documents = object
    .get("documents")
    .and_then(Value::as_str)
    .filter(|value| is_lower_hex(value, 64, 64))
    .ok_or_else(invalid_review)?;
  let implementation = object
    .get("implementation")
    .and_then(Value::as_str)
    .filter(|value| is_lower_hex(value, 64, 64))
    .ok_or_else(invalid_review)?;
  Ok(ReviewBinding {
    base_commit: base_commit.to_owned(),
    documents: documents.to_owned(),
    implementation: implementation.to_owned(),
  })
}

pub fn review_freshness(root: &Path, binding: &Value) -> Result<Value> {
  let binding = parse_binding(binding)?;
  let root_string = root.to_string_lossy();
  let project = load_project(&root_string)?;
  let implementation =
    crate::review::implementation::capture_implementation(root, &binding.base_commit)?;
  let documents_changed = project.snapshot != binding.documents;
  let implementation_changed = implementation.get("fingerprint").and_then(Value::as_str)
    != Some(binding.implementation.as_str());
  Ok(serde_json::json!({
      "documentsChanged": documents_changed,
      "implementationChanged": implementation_changed,
      "status": if documents_changed || implementation_changed { "stale" } else { "current" },
  }))
}

fn report_path(root: &Path, check: &str) -> PathBuf {
  let check = Path::new(check);
  if check.is_absolute() {
    check.to_owned()
  } else {
    root.join(check)
  }
}

pub fn check_review(root: &Path, check: &str) -> Result<Value> {
  let report_file = report_path(root, check);
  let metadata = fs::metadata(&report_file)?;
  if metadata.len() > MAX_REVIEW_BYTES {
    return Err(error("INVALID_REVIEW", "Saved review exceeds 1 MiB."));
  }
  let report: Value =
    serde_json::from_slice(&fs::read(report_file)?).map_err(|_| invalid_review())?;
  if report.get("command").and_then(Value::as_str) != Some("review") {
    return Err(invalid_review());
  }
  let binding = report.get("binding").ok_or_else(invalid_review)?;
  let freshness = review_freshness(root, binding)?;
  let mut output = Map::new();
  output.insert(
    "command".to_owned(),
    Value::String("review-check".to_owned()),
  );
  if let Some(object) = freshness.as_object() {
    output.extend(object.clone());
  }
  output.insert("guidance".to_owned(), Value::String(GUIDANCE.to_owned()));
  Ok(Value::Object(output))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::documents::Document;
  use serde_json::json;

  fn project() -> Project {
    let text = "# Privacy\n\nRevoking access removes cached data.\n";
    let document = Document {
      id: "privacy.md".to_owned(),
      path: "privacy.md".to_owned(),
      title: "Privacy".to_owned(),
      text: text.to_owned(),
      hash: crate::documents::hash(text),
      status: None,
      links: Vec::new(),
      historical: false,
    };
    Project {
      root: PathBuf::from("."),
      snapshot: "snapshot".to_owned(),
      current_snapshot: "snapshot".to_owned(),
      documents: vec![document.clone()],
      current_documents: vec![document],
      historical_documents: Vec::new(),
      warnings: Vec::new(),
    }
  }

  fn implementation() -> Value {
    json!({
        "baseCommit": "a".repeat(40),
        "diff": "diff",
        "files": [{
            "path": "cache.ts",
            "before": null,
            "after": {
                "version": "b".repeat(64),
                "lines": [[1, "export const purge = false;"]]
            }
        }],
        "warnings": [],
        "fingerprint": "c".repeat(64)
    })
  }

  #[test]
  fn materializes_verified_and_unverified_references() {
    let project = project();
    let supplied = vec![SuppliedDocument {
      id: "privacy.md".to_owned(),
      lines: vec![vec![
        Value::from(3),
        Value::String("Revoking access removes cached data.".to_owned()),
      ]],
    }];
    let value = json!({
        "findings": [
            {
                "assessment": "conflict",
                "code": [{"lineEnd": 1, "lineStart": 1, "path": "cache.ts", "side": "after"}],
                "documents": [{"document": "privacy.md", "lineEnd": 3, "lineStart": 3}],
                "explanation": "The changed code skips the required purge."
            },
            {
                "assessment": "exception",
                "code": [{"lineEnd": 9, "lineStart": 9, "path": "cache.ts", "side": "after"}],
                "documents": [{"document": "privacy.md", "lineEnd": 3, "lineStart": 3}],
                "explanation": "The unsupported location stays uncertain."
            }
        ],
        "uncertainties": []
    });
    let result =
      materialize_review(&project, &implementation(), &supplied, &value).expect("review");
    assert_eq!(result["invalidReferences"], true);
    assert_eq!(result["findings"][0]["assessment"], "conflict");
    assert_eq!(result["findings"][0]["referencesVerified"], true);
    assert_eq!(result["findings"][1]["assessment"], "uncertain");
    assert_eq!(
      review_binding(&project, &implementation())["documents"],
      "snapshot"
    );
  }
}
