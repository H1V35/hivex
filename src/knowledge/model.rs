use crate::compatibility::{MAX_SAFE_INTEGER, trim_js_whitespace};
use crate::documents::{Document, Project};
use crate::documents::{raw_markdown_lines, source_range};
use num_traits::ToPrimitive;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

pub type JsonMap = Map<String, Value>;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Citation {
  pub document: String,
  pub line_end: usize,
  pub line_start: usize,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub version: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WarningScope {
  pub document: String,
  pub line_end: usize,
  pub line_start: usize,
  pub version: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
  pub conditions: Vec<String>,
  pub document: String,
  pub exceptions: Vec<String>,
  pub id: String,
  pub kind: String,
  pub line_end: usize,
  pub line_start: usize,
  pub reason: String,
  pub status: String,
  pub text: String,
  pub batch: String,
  pub local_id: String,
  pub quality: String,
  pub version: String,
  /// Persisted schema order for decoded records; new records use live order.
  /// This is metadata for packet serialization and is not part of the graph schema.
  #[serde(skip)]
  pub field_order: Vec<String>,
}

impl Serialize for Decision {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    decision_value(self, false).serialize(serializer)
  }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Relationship {
  pub evidence: Vec<Citation>,
  pub from: String,
  pub id: String,
  pub reason: String,
  pub to: String,
  #[serde(rename = "type")]
  pub kind: String,
  pub batch: String,
  pub local_id: String,
  pub quality: String,
  /// Persisted schema order for decoded records; new records use live order.
  #[serde(skip)]
  pub field_order: Vec<String>,
}

impl Serialize for Relationship {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    relationship_value(self, false).serialize(serializer)
  }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionDecision {
  pub conditions: Vec<String>,
  pub document: String,
  pub exceptions: Vec<String>,
  pub id: String,
  pub kind: String,
  pub line_end: usize,
  pub line_start: usize,
  pub reason: String,
  pub status: String,
  pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionRelationship {
  pub evidence: Vec<Citation>,
  pub from: String,
  pub id: String,
  pub reason: String,
  pub to: String,
  #[serde(rename = "type")]
  pub kind: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extraction {
  pub decisions: Vec<ExtractionDecision>,
  pub relationships: Vec<ExtractionRelationship>,
  pub uncertainties: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckFinding {
  pub reason: String,
  pub target: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeCheck {
  pub findings: Vec<CheckFinding>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WarningResolution {
  pub evidence: Vec<WarningScope>,
  pub reason: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WarningRecord {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub kind: Option<String>,
  pub message: String,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub previous_resolutions: Vec<WarningResolution>,
  /// Distinguishes an explicitly supplied empty array from an omitted field.
  #[serde(skip)]
  pub previous_resolutions_present: bool,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub resolution: Option<WarningResolution>,
  pub scope: Vec<WarningScope>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub target: Option<String>,
}

impl Serialize for WarningRecord {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    warning_value(&Warning::Structured(self.clone())).serialize(serializer)
  }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Warning {
  Legacy(String),
  Structured(WarningRecord),
}

impl Default for Warning {
  fn default() -> Self {
    Self::Legacy(String::new())
  }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Graph {
  pub decisions: Vec<Decision>,
  pub documents: JsonMap,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub last_extraction: Option<String>,
  pub relationships: Vec<Relationship>,
  #[serde(default)]
  pub units: JsonMap,
  pub version: u8,
  pub warnings: Vec<Warning>,
}

impl Serialize for Graph {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    graph_value(self, false).serialize(serializer)
  }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuppliedDocument {
  pub id: String,
  pub lines: Vec<Vec<Value>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceEvidence {
  pub document: String,
  pub historical: bool,
  pub line_end: usize,
  pub line_start: usize,
  pub text: String,
  pub version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WarningSummary {
  pub findings: usize,
  pub limitations: usize,
  pub resolved: usize,
  pub unknown: usize,
  pub validation: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckImpact {
  pub decision_ids: HashSet<String>,
  pub is_uncertain_batch: bool,
  pub relationship_ids: HashSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WarningChange {
  pub id: String,
  pub message: String,
  pub state: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WarningChanges {
  pub new: Vec<WarningChange>,
  pub reopened: Vec<WarningChange>,
  pub resolved: Vec<WarningChange>,
}

#[derive(Clone, Copy, Debug)]
pub struct ExtractionOptions<'a> {
  pub batch: &'a str,
  pub context_documents: Option<&'a [Document]>,
  pub context_ranges: Option<&'a [Citation]>,
  pub documents: &'a [Document],
  pub existing_ids: Option<&'a [String]>,
  pub extraction: &'a Extraction,
  pub graph: &'a Graph,
  pub target_ranges: Option<&'a [Citation]>,
}

fn utf16_len(value: &str) -> usize {
  value.encode_utf16().count()
}

fn valid_explanation(value: &str) -> bool {
  !value.is_empty() && utf16_len(value) <= 2048
}

fn valid_kind(value: &str) -> bool {
  matches!(value, "decision" | "constraint" | "definition" | "lesson")
}

fn valid_status(value: &str) -> bool {
  matches!(value, "current" | "proposed" | "historical" | "uncertain")
}

fn valid_quality(value: &str) -> bool {
  matches!(value, "unchecked" | "checked" | "uncertain")
}

fn valid_relationship_type(value: &str) -> bool {
  matches!(
    value,
    "requires" | "exception-to" | "supersedes" | "supports" | "contradicts"
  )
}

fn valid_warning_kind(value: Option<&str>) -> bool {
  value.is_none_or(|value| matches!(value, "limitation" | "finding" | "validation"))
}

fn valid_extraction_decision(entry: &ExtractionDecision) -> bool {
  !entry.document.is_empty()
    && !entry.id.is_empty()
    && valid_kind(&entry.kind)
    && valid_line_number(entry.line_start)
    && valid_line_number(entry.line_end)
    && valid_explanation(&entry.reason)
    && valid_status(&entry.status)
    && valid_explanation(&entry.text)
    && entry.conditions.len() <= 16
    && entry
      .conditions
      .iter()
      .all(|value| valid_explanation(value))
    && entry.exceptions.len() <= 16
    && entry
      .exceptions
      .iter()
      .all(|value| valid_explanation(value))
}

fn valid_extraction_relationship(entry: &ExtractionRelationship) -> bool {
  !entry.from.is_empty()
    && !entry.id.is_empty()
    && !entry.reason.is_empty()
    && utf16_len(&entry.reason) <= 2048
    && !entry.to.is_empty()
    && valid_relationship_type(&entry.kind)
    && (1..=8).contains(&entry.evidence.len())
    && entry.evidence.iter().all(validate_citation)
}

fn valid_warning_scope(scope: &WarningScope) -> bool {
  validate_citation(&Citation {
    document: scope.document.clone(),
    line_end: scope.line_end,
    line_start: scope.line_start,
    version: Some(scope.version.clone()),
  })
}

fn valid_warning_resolution(resolution: &WarningResolution) -> bool {
  (1..=32).contains(&resolution.evidence.len())
    && resolution.evidence.iter().all(valid_warning_scope)
    && valid_explanation(&resolution.reason)
}

fn valid_warning(warning: &Warning) -> bool {
  match warning {
    Warning::Legacy(_) => true,
    Warning::Structured(record) => {
      valid_warning_kind(record.kind.as_deref())
        && record
          .previous_resolutions
          .iter()
          .all(valid_warning_resolution)
        && record
          .resolution
          .as_ref()
          .is_none_or(valid_warning_resolution)
        && record.scope.iter().all(valid_warning_scope)
    }
  }
}

pub fn validate_extraction(extraction: &Extraction) -> bool {
  extraction.decisions.len() <= 64
    && extraction.decisions.iter().all(valid_extraction_decision)
    && extraction.relationships.len() <= 128
    && extraction
      .relationships
      .iter()
      .all(valid_extraction_relationship)
    && extraction.uncertainties.len() <= 32
    && extraction
      .uncertainties
      .iter()
      .all(|value| valid_explanation(value))
}

pub fn validate_check(check: &KnowledgeCheck) -> bool {
  check.findings.len() <= 64
    && check
      .findings
      .iter()
      .all(|finding| !finding.target.is_empty() && valid_explanation(&finding.reason))
}

pub fn parse_check(value: &Value) -> Option<KnowledgeCheck> {
  let check =
    serde_json::from_value::<KnowledgeCheck>(normalize_integral_numbers(value.clone())).ok()?;
  validate_check(&check).then_some(check)
}

pub fn validate_graph(graph: &Graph) -> bool {
  if graph.version != 1 {
    return false;
  }
  let decision_ids: HashSet<&str> = graph
    .decisions
    .iter()
    .map(|entry| entry.id.as_str())
    .collect();
  if decision_ids.len() != graph.decisions.len()
    || !graph.decisions.iter().all(|entry| {
      !entry.document.is_empty()
        && !entry.id.is_empty()
        && valid_kind(&entry.kind)
        && valid_line_number(entry.line_start)
        && valid_line_number(entry.line_end)
        && valid_explanation(&entry.reason)
        && valid_status(&entry.status)
        && valid_explanation(&entry.text)
        && valid_quality(&entry.quality)
        && (entry.quality != "checked" || entry.line_start <= entry.line_end)
        && entry.conditions.len() <= 16
        && entry
          .conditions
          .iter()
          .all(|value| valid_explanation(value))
        && entry.exceptions.len() <= 16
        && entry
          .exceptions
          .iter()
          .all(|value| valid_explanation(value))
    })
  {
    return false;
  }
  let relationship_ids: HashSet<&str> = graph
    .relationships
    .iter()
    .map(|entry| entry.id.as_str())
    .collect();
  if relationship_ids.len() != graph.relationships.len()
    || !graph.relationships.iter().all(|entry| {
      !entry.from.is_empty()
        && !entry.id.is_empty()
        && !entry.reason.is_empty()
        && utf16_len(&entry.reason) <= 2048
        && !entry.to.is_empty()
        && valid_relationship_type(&entry.kind)
        && valid_quality(&entry.quality)
        && decision_ids.contains(entry.from.as_str())
        && decision_ids.contains(entry.to.as_str())
        && (1..=8).contains(&entry.evidence.len())
        && entry.evidence.iter().all(validate_citation)
        && entry
          .evidence
          .iter()
          .all(|citation| citation.line_start <= citation.line_end)
    })
  {
    return false;
  }
  graph.documents.values().all(Value::is_string)
    && graph.units.values().all(|unit| {
      let Value::Object(unit) = unit else {
        return false;
      };
      unit.get("document").and_then(Value::as_str).is_some()
        && unit.get("version").and_then(Value::as_str).is_some()
        && unit.get("workKey").is_none_or(Value::is_string)
    })
    && graph.warnings.iter().all(valid_warning)
}

pub fn parse_extraction(value: &Value) -> Option<Extraction> {
  let mut extraction =
    serde_json::from_value::<Extraction>(normalize_integral_numbers(value.clone())).ok()?;
  // `version` is not part of the extraction citation schema. Zod strips it
  // before the identity is computed, so do the same at this boundary.
  for relationship in &mut extraction.relationships {
    for citation in &mut relationship.evidence {
      citation.version = None;
    }
  }
  validate_extraction(&extraction).then_some(extraction)
}

pub(crate) fn normalize_integral_numbers(value: Value) -> Value {
  match value {
    Value::Array(values) => {
      Value::Array(values.into_iter().map(normalize_integral_numbers).collect())
    }
    Value::Object(values) => Value::Object(
      values
        .into_iter()
        .map(|(key, value)| (key, normalize_integral_numbers(value)))
        .collect(),
    ),
    Value::Number(number) if number.is_f64() => number
      .as_f64()
      .filter(|number| number.is_finite() && number.fract() == 0.0 && *number >= 0.0)
      .and_then(|number| number.to_u64())
      .map_or(Value::Number(number), Value::from),
    value => value,
  }
}

fn has_explicit_graph_null(value: &Value) -> bool {
  let Some(root) = value.as_object() else {
    return false;
  };
  if root.get("lastExtraction").is_some_and(Value::is_null) {
    return true;
  }
  if root
    .get("relationships")
    .and_then(Value::as_array)
    .is_some_and(|relationships| {
      relationships.iter().any(|relationship| {
        relationship
          .as_object()
          .and_then(|relationship| relationship.get("evidence"))
          .and_then(Value::as_array)
          .is_some_and(|evidence| {
            evidence.iter().any(|citation| {
              citation
                .as_object()
                .and_then(|citation| citation.get("version"))
                .is_some_and(Value::is_null)
            })
          })
      })
    })
  {
    return true;
  }
  if root
    .get("units")
    .and_then(Value::as_object)
    .is_some_and(|units| {
      units.values().any(|unit| {
        unit
          .as_object()
          .and_then(|unit| unit.get("workKey"))
          .is_some_and(Value::is_null)
      })
    })
  {
    return true;
  }
  if root
    .get("warnings")
    .and_then(Value::as_array)
    .is_some_and(|warnings| {
      warnings.iter().any(|warning| {
        let Some(warning) = warning.as_object() else {
          return false;
        };
        ["kind", "resolution", "target"]
          .iter()
          .any(|field| warning.get(*field).is_some_and(Value::is_null))
      })
    })
  {
    return true;
  }
  false
}

fn annotate_graph_serialization(mut graph: Graph, value: &Value) -> Graph {
  let Some(root) = value.as_object() else {
    return graph;
  };
  for (entry, _) in graph
    .decisions
    .iter_mut()
    .zip(
      root
        .get("decisions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten(),
    )
    .filter(|(_, record)| record.is_object())
  {
    entry.field_order = DECISION_FIELDS
      .iter()
      .map(|field| (*field).to_owned())
      .collect();
  }
  for (entry, _) in graph
    .relationships
    .iter_mut()
    .zip(
      root
        .get("relationships")
        .and_then(Value::as_array)
        .into_iter()
        .flatten(),
    )
    .filter(|(_, record)| record.is_object())
  {
    entry.field_order = RELATIONSHIP_FIELDS
      .iter()
      .map(|field| (*field).to_owned())
      .collect();
  }
  for (entry, record) in graph.warnings.iter_mut().zip(
    root
      .get("warnings")
      .and_then(Value::as_array)
      .into_iter()
      .flatten(),
  ) {
    if let (Warning::Structured(entry), Some(record)) = (entry, record.as_object()) {
      entry.previous_resolutions_present = record.contains_key("previousResolutions");
    }
  }
  graph
}

pub fn parse_graph(value: &Value, strict: bool) -> Option<Graph> {
  if has_explicit_graph_null(value) {
    return None;
  }
  if strict {
    const FIELDS: &[&str] = &[
      "decisions",
      "documents",
      "lastExtraction",
      "relationships",
      "units",
      "version",
      "warnings",
    ];
    let object = value.as_object()?;
    if object.keys().any(|key| !FIELDS.contains(&key.as_str())) {
      return None;
    }
  }
  let graph = serde_json::from_value::<Graph>(normalize_integral_numbers(value.clone())).ok()?;
  let graph = annotate_graph_serialization(graph, value);
  validate_graph(&graph).then_some(graph)
}

pub fn digest(value: &[u8]) -> String {
  format!("{:x}", Sha256::digest(value))
}

fn json_bytes(value: &Value) -> Vec<u8> {
  serde_json::to_vec(value).expect("JSON values are serializable")
}

fn string_array(values: &[String]) -> Value {
  Value::Array(values.iter().cloned().map(Value::String).collect())
}

fn citation_value(citation: &Citation) -> Value {
  let mut value = Map::new();
  value.insert(
    "document".to_owned(),
    Value::String(citation.document.clone()),
  );
  value.insert("lineEnd".to_owned(), Value::from(citation.line_end));
  value.insert("lineStart".to_owned(), Value::from(citation.line_start));
  if let Some(version) = &citation.version {
    value.insert("version".to_owned(), Value::String(version.clone()));
  }
  Value::Object(value)
}

fn citation_identity_value(citation: &Citation) -> Value {
  let mut value = Map::new();
  value.insert(
    "document".to_owned(),
    Value::String(citation.document.clone()),
  );
  value.insert("lineStart".to_owned(), Value::from(citation.line_start));
  value.insert("lineEnd".to_owned(), Value::from(citation.line_end));
  if let Some(version) = &citation.version {
    value.insert("version".to_owned(), Value::String(version.clone()));
  }
  Value::Object(value)
}

fn warning_scope_value(scope: &WarningScope) -> Value {
  let mut value = Map::new();
  value.insert("document".to_owned(), Value::String(scope.document.clone()));
  value.insert("lineEnd".to_owned(), Value::from(scope.line_end));
  value.insert("lineStart".to_owned(), Value::from(scope.line_start));
  value.insert("version".to_owned(), Value::String(scope.version.clone()));
  Value::Object(value)
}

pub fn warning_value(warning: &Warning) -> Value {
  match warning {
    Warning::Legacy(message) => Value::String(message.clone()),
    Warning::Structured(record) => {
      let mut value = Map::new();
      if let Some(kind) = &record.kind {
        value.insert("kind".to_owned(), Value::String(kind.clone()));
      }
      value.insert("message".to_owned(), Value::String(record.message.clone()));
      if record.previous_resolutions_present || !record.previous_resolutions.is_empty() {
        value.insert(
          "previousResolutions".to_owned(),
          Value::Array(
            record
              .previous_resolutions
              .iter()
              .map(resolution_value)
              .collect(),
          ),
        );
      }
      if let Some(resolution) = &record.resolution {
        value.insert("resolution".to_owned(), resolution_value(resolution));
      }
      value.insert(
        "scope".to_owned(),
        Value::Array(record.scope.iter().map(warning_scope_value).collect()),
      );
      if let Some(target) = &record.target {
        value.insert("target".to_owned(), Value::String(target.clone()));
      }
      Value::Object(value)
    }
  }
}

fn resolution_value(resolution: &WarningResolution) -> Value {
  let mut value = Map::new();
  value.insert(
    "evidence".to_owned(),
    Value::Array(
      resolution
        .evidence
        .iter()
        .map(warning_scope_value)
        .collect(),
    ),
  );
  value.insert(
    "reason".to_owned(),
    Value::String(resolution.reason.clone()),
  );
  Value::Object(value)
}

const DECISION_FIELDS: &[&str] = &[
  "id",
  "document",
  "text",
  "kind",
  "status",
  "conditions",
  "exceptions",
  "reason",
  "lineStart",
  "lineEnd",
  "version",
  "batch",
  "localId",
  "quality",
];
const RELATIONSHIP_FIELDS: &[&str] = &[
  "id", "from", "to", "type", "reason", "evidence", "batch", "localId", "quality",
];
const LIVE_DECISION_FIELDS: &[&str] = &[
  "id",
  "document",
  "text",
  "kind",
  "status",
  "conditions",
  "exceptions",
  "reason",
  "lineStart",
  "lineEnd",
  "localId",
  "version",
  "batch",
  "quality",
];
const LIVE_RELATIONSHIP_FIELDS: &[&str] = &[
  "id", "from", "to", "type", "reason", "evidence", "localId", "batch", "quality",
];

fn order_record(mut value: Map<String, Value>, fields: &[String]) -> Map<String, Value> {
  let mut result = Map::new();
  for field in fields {
    if let Some(node) = value.shift_remove(field) {
      result.insert(field.clone(), node);
    }
  }
  result.extend(value);
  result
}

fn order_record_static(mut value: Map<String, Value>, fields: &[&str]) -> Map<String, Value> {
  let mut result = Map::new();
  for field in fields {
    if let Some(node) = value.shift_remove(*field) {
      result.insert((*field).to_owned(), node);
    }
  }
  result.extend(value);
  result
}

fn decision_value(decision: &Decision, live: bool) -> Value {
  let mut value = Map::new();
  value.insert("id".to_owned(), Value::String(decision.id.clone()));
  value.insert(
    "document".to_owned(),
    Value::String(decision.document.clone()),
  );
  value.insert("text".to_owned(), Value::String(decision.text.clone()));
  value.insert("kind".to_owned(), Value::String(decision.kind.clone()));
  value.insert("status".to_owned(), Value::String(decision.status.clone()));
  value.insert("conditions".to_owned(), string_array(&decision.conditions));
  value.insert("exceptions".to_owned(), string_array(&decision.exceptions));
  value.insert("reason".to_owned(), Value::String(decision.reason.clone()));
  value.insert("lineStart".to_owned(), Value::from(decision.line_start));
  value.insert("lineEnd".to_owned(), Value::from(decision.line_end));
  value.insert(
    "version".to_owned(),
    Value::String(decision.version.clone()),
  );
  value.insert("batch".to_owned(), Value::String(decision.batch.clone()));
  value.insert(
    "localId".to_owned(),
    Value::String(decision.local_id.clone()),
  );
  value.insert(
    "quality".to_owned(),
    Value::String(decision.quality.clone()),
  );
  let value = if decision.field_order.is_empty() {
    order_record_static(
      value,
      if live {
        LIVE_DECISION_FIELDS
      } else {
        DECISION_FIELDS
      },
    )
  } else {
    order_record(value, &decision.field_order)
  };
  Value::Object(value)
}

fn relationship_value(relationship: &Relationship, live: bool) -> Value {
  let mut value = Map::new();
  value.insert("id".to_owned(), Value::String(relationship.id.clone()));
  value.insert("from".to_owned(), Value::String(relationship.from.clone()));
  value.insert("to".to_owned(), Value::String(relationship.to.clone()));
  value.insert("type".to_owned(), Value::String(relationship.kind.clone()));
  value.insert(
    "reason".to_owned(),
    Value::String(relationship.reason.clone()),
  );
  value.insert(
    "evidence".to_owned(),
    Value::Array(relationship.evidence.iter().map(citation_value).collect()),
  );
  value.insert(
    "batch".to_owned(),
    Value::String(relationship.batch.clone()),
  );
  value.insert(
    "localId".to_owned(),
    Value::String(relationship.local_id.clone()),
  );
  value.insert(
    "quality".to_owned(),
    Value::String(relationship.quality.clone()),
  );
  let value = if relationship.field_order.is_empty() {
    order_record_static(
      value,
      if live {
        LIVE_RELATIONSHIP_FIELDS
      } else {
        RELATIONSHIP_FIELDS
      },
    )
  } else {
    order_record(value, &relationship.field_order)
  };
  Value::Object(value)
}

/// Converts the typed graph to protocol JSON. `live` is only the fallback for
/// records made without a parsed/staged field order; records carrying one keep
/// their own store or live disposition.
pub fn graph_value(graph: &Graph, live: bool) -> Value {
  let mut value = Map::new();
  value.insert(
    "decisions".to_owned(),
    Value::Array(
      graph
        .decisions
        .iter()
        .map(|decision| decision_value(decision, live))
        .collect(),
    ),
  );
  value.insert(
    "documents".to_owned(),
    Value::Object(graph.documents.clone()),
  );
  if let Some(last_extraction) = &graph.last_extraction {
    value.insert(
      "lastExtraction".to_owned(),
      Value::String(last_extraction.clone()),
    );
  }
  value.insert(
    "relationships".to_owned(),
    Value::Array(
      graph
        .relationships
        .iter()
        .map(|relationship| relationship_value(relationship, live))
        .collect(),
    ),
  );
  value.insert("units".to_owned(), Value::Object(graph.units.clone()));
  value.insert("version".to_owned(), Value::from(graph.version));
  value.insert(
    "warnings".to_owned(),
    Value::Array(graph.warnings.iter().map(warning_value).collect()),
  );
  Value::Object(value)
}

pub fn empty_graph() -> Graph {
  Graph {
    version: 1,
    ..Graph::default()
  }
}

fn valid_line_number(line: usize) -> bool {
  line > 0 && line as u64 <= MAX_SAFE_INTEGER
}

pub fn validate_citation(citation: &Citation) -> bool {
  !citation.document.is_empty()
    && valid_line_number(citation.line_start)
    && valid_line_number(citation.line_end)
}

pub fn valid_citation(citation: &Citation, documents: &[Document]) -> bool {
  let Some(document) = documents
    .iter()
    .find(|document| document.id == citation.document)
  else {
    return false;
  };
  validate_citation(citation)
    && citation.line_end <= raw_markdown_lines(&document.text).len()
    && !trim_js_whitespace(&source_range(
      &document.text,
      citation.line_start,
      citation.line_end,
    ))
    .is_empty()
}

pub fn supplied_citation(citation: &Citation, documents: &[SuppliedDocument]) -> bool {
  if !validate_citation(citation) || citation.line_end < citation.line_start {
    return false;
  }
  let lines: HashSet<usize> = documents
    .iter()
    .filter(|document| document.id == citation.document)
    .flat_map(|document| {
      document.lines.iter().filter_map(|line| {
        line.first().and_then(|value| match value {
          Value::Number(number) => number.as_u64().and_then(|line| usize::try_from(line).ok()),
          Value::String(number) => number.parse::<usize>().ok(),
          _ => None,
        })
      })
    })
    .collect();
  (citation.line_start..=citation.line_end).all(|line| lines.contains(&line))
}

pub fn warning_scope(documents: &[Document], ranges: Option<&[Citation]>) -> Vec<WarningScope> {
  let ranges: Vec<Citation> = ranges.map_or_else(
    || {
      documents
        .iter()
        .map(|document| Citation {
          document: document.id.clone(),
          line_end: raw_markdown_lines(&document.text).len(),
          line_start: 1,
          version: None,
        })
        .collect()
    },
    <[Citation]>::to_vec,
  );
  ranges
    .into_iter()
    .filter_map(|range| {
      documents
        .iter()
        .find(|document| document.id == range.document)
        .map(|document| WarningScope {
          document: range.document,
          line_end: range.line_end,
          line_start: range.line_start,
          version: document.hash.clone(),
        })
    })
    .collect()
}

pub fn source_evidence(entry: &Citation, project: &Project) -> Option<SourceEvidence> {
  let document = project
    .documents
    .iter()
    .find(|document| document.id == entry.document)?;
  if !valid_citation(entry, &project.documents) {
    return None;
  }
  Some(SourceEvidence {
    document: entry.document.clone(),
    historical: document.historical,
    line_end: entry.line_end,
    line_start: entry.line_start,
    text: source_range(&document.text, entry.line_start, entry.line_end),
    version: document.hash.clone(),
  })
}

pub fn warning_id(warning: &Warning) -> String {
  let mut value = Map::new();
  match warning {
    Warning::Legacy(message) => {
      value.insert("message".to_owned(), Value::String(message.clone()));
      value.insert("scope".to_owned(), Value::Array(Vec::new()));
    }
    Warning::Structured(record) => {
      if let Some(kind) = &record.kind {
        value.insert("kind".to_owned(), Value::String(kind.clone()));
      }
      value.insert("message".to_owned(), Value::String(record.message.clone()));
      value.insert(
        "scope".to_owned(),
        Value::Array(record.scope.iter().map(warning_scope_value).collect()),
      );
      if let Some(target) = &record.target {
        value.insert("target".to_owned(), Value::String(target.clone()));
      }
    }
  }
  digest(&json_bytes(&Value::Object(value)))
}

pub fn is_warning_resolved(warning: &Warning, documents: &[Document]) -> bool {
  let Warning::Structured(record) = warning else {
    return false;
  };
  let Some(resolution) = &record.resolution else {
    return false;
  };
  resolution.evidence.iter().all(|citation| {
    documents
      .iter()
      .find(|document| document.id == citation.document)
      .is_some_and(|document| document.hash == citation.version)
      && valid_citation(
        &Citation {
          document: citation.document.clone(),
          line_end: citation.line_end,
          line_start: citation.line_start,
          version: Some(citation.version.clone()),
        },
        documents,
      )
  })
}

pub fn with_warning_resolution(warning: &Warning, resolution: WarningResolution) -> Warning {
  let mut record = match warning {
    Warning::Legacy(message) => WarningRecord {
      message: message.clone(),
      ..WarningRecord::default()
    },
    Warning::Structured(record) => record.clone(),
  };
  if let Some(previous) = record.resolution.take() {
    record.previous_resolutions.push(previous);
  }
  record.resolution = Some(resolution);
  Warning::Structured(record)
}

pub fn active_warnings<'a>(warnings: &'a [Warning], documents: &[Document]) -> Vec<&'a Warning> {
  warnings
    .iter()
    .filter(|warning| !is_warning_resolved(warning, documents))
    .collect()
}

pub fn warning_summary(warnings: &[Warning], documents: &[Document]) -> WarningSummary {
  let mut summary = WarningSummary {
    findings: 0,
    limitations: 0,
    resolved: 0,
    unknown: 0,
    validation: 0,
  };
  for warning in warnings {
    if is_warning_resolved(warning, documents) {
      summary.resolved += 1;
      continue;
    }
    let kind = match warning {
      Warning::Legacy(_) => None,
      Warning::Structured(record) => record.kind.as_deref(),
    };
    match kind {
      Some("finding") => summary.findings += 1,
      Some("limitation") => summary.limitations += 1,
      Some("validation") => summary.validation += 1,
      _ => summary.unknown += 1,
    }
  }
  summary
}

fn in_ranges(citation: &Citation, ranges: Option<&[Citation]>) -> bool {
  let Some(ranges) = ranges else {
    return true;
  };
  (citation.line_start..=citation.line_end).all(|line| {
    ranges.iter().any(|range| {
      range.document == citation.document && range.line_start <= line && range.line_end >= line
    })
  })
}

fn overlaps(left_start: usize, left_end: usize, right_start: usize, right_end: usize) -> bool {
  left_start <= right_end && right_start <= left_end
}

fn append_warnings(existing: &[Warning], incoming: Vec<Warning>) -> Vec<Warning> {
  let mut warnings = existing.to_vec();
  let mut known: HashSet<String> = warnings.iter().map(warning_id).collect();
  for warning in incoming {
    if known.insert(warning_id(&warning)) {
      warnings.push(warning);
    }
  }
  warnings
}

fn retained_warnings(graph: &Graph, scope: &[WarningScope]) -> Vec<Warning> {
  graph
    .warnings
    .iter()
    .filter(|warning| {
      let Warning::Structured(record) = warning else {
        return true;
      };
      if record.resolution.is_some() {
        return true;
      }
      record.scope.iter().all(|old| {
        !scope.iter().any(|current| {
          current.document == old.document
            && (current.version != old.version
              || overlaps(
                current.line_start,
                current.line_end,
                old.line_start,
                old.line_end,
              ))
        })
      })
    })
    .cloned()
    .collect()
}

fn source_version(documents: &[Document], id: &str) -> Option<String> {
  documents
    .iter()
    .find(|document| document.id == id)
    .map(|document| document.hash.clone())
}

fn changed_document(documents: &[Document], id: &str, version: &str) -> bool {
  documents
    .iter()
    .find(|document| document.id == id)
    .is_some_and(|document| document.hash != version)
}

fn decision_identity(entry: &ExtractionDecision, version: &str) -> String {
  let mut value = Map::new();
  value.insert("version".to_owned(), Value::String(version.to_owned()));
  value.insert("id".to_owned(), Value::String(entry.id.clone()));
  value.insert("document".to_owned(), Value::String(entry.document.clone()));
  value.insert("text".to_owned(), Value::String(entry.text.clone()));
  value.insert("kind".to_owned(), Value::String(entry.kind.clone()));
  value.insert("status".to_owned(), Value::String(entry.status.clone()));
  value.insert("conditions".to_owned(), string_array(&entry.conditions));
  value.insert("exceptions".to_owned(), string_array(&entry.exceptions));
  value.insert("reason".to_owned(), Value::String(entry.reason.clone()));
  value.insert("lineStart".to_owned(), Value::from(entry.line_start));
  value.insert("lineEnd".to_owned(), Value::from(entry.line_end));
  digest(&json_bytes(&Value::Object(value)))
}

fn relationship_identity(
  entry: &ExtractionRelationship,
  from: &str,
  to: &str,
  evidence: &[Citation],
) -> String {
  let mut value = Map::new();
  value.insert("id".to_owned(), Value::String(entry.id.clone()));
  value.insert("from".to_owned(), Value::String(from.to_owned()));
  value.insert("to".to_owned(), Value::String(to.to_owned()));
  value.insert("type".to_owned(), Value::String(entry.kind.clone()));
  value.insert("reason".to_owned(), Value::String(entry.reason.clone()));
  value.insert(
    "evidence".to_owned(),
    Value::Array(evidence.iter().map(citation_identity_value).collect()),
  );
  digest(&json_bytes(&Value::Object(value)))
}

fn decision_source(entry: &Decision, documents: &[Document]) -> Option<Document> {
  documents
    .iter()
    .find(|document| document.id == entry.document)
    .cloned()
}

fn relationship_changed(relationship: &Relationship, options: &ExtractionOptions<'_>) -> bool {
  relationship.evidence.iter().any(|citation| {
    let range_changed = options.target_ranges.is_some_and(|ranges| {
      ranges.iter().any(|range| {
        range.document == citation.document
          && overlaps(
            range.line_start,
            range.line_end,
            citation.line_start,
            citation.line_end,
          )
      })
    });
    let document_changed = options
      .context_documents
      .unwrap_or(options.documents)
      .iter()
      .find(|document| document.id == citation.document)
      .is_some_and(|document| citation.version.as_deref() != Some(document.hash.as_str()));
    range_changed || document_changed
  })
}

pub fn apply_extraction(options: ExtractionOptions<'_>) -> Graph {
  let mut decisions = retained_decisions(options);
  let mut ids: HashMap<String, String> = decisions
    .iter()
    .filter(|entry| {
      options
        .existing_ids
        .is_some_and(|ids| ids.contains(&entry.id))
    })
    .map(|entry| (entry.id.clone(), entry.id.clone()))
    .collect();
  let mut extraction_warnings: Vec<Warning> = options
    .extraction
    .uncertainties
    .iter()
    .cloned()
    .map(|message| {
      Warning::Structured(WarningRecord {
        kind: Some("limitation".to_owned()),
        message,
        ..WarningRecord::default()
      })
    })
    .collect();
  integrate_decisions(options, &mut decisions, &mut ids, &mut extraction_warnings);
  let relationships = integrate_relationships(options, &decisions, &ids, &mut extraction_warnings);
  let scope = warning_scope(options.documents, options.target_ranges);
  let incoming_warnings = extraction_warnings
    .into_iter()
    .map(|warning| match warning {
      Warning::Legacy(message) => Warning::Structured(WarningRecord {
        message,
        scope: scope.clone(),
        ..WarningRecord::default()
      }),
      Warning::Structured(mut record) => {
        record.scope.clone_from(&scope);
        Warning::Structured(record)
      }
    })
    .collect();
  let mut documents = options.graph.documents.clone();
  documents.retain(|id, version| {
    !options
      .documents
      .iter()
      .any(|document| document.id == *id && document.hash != version.as_str().unwrap_or_default())
  });
  let mut units = options.graph.units.clone();
  units.retain(|_, unit| {
    let Some(unit_document) = unit.get("document").and_then(Value::as_str) else {
      return true;
    };
    let Some(unit_version) = unit.get("version").and_then(Value::as_str) else {
      return true;
    };
    !changed_document(options.documents, unit_document, unit_version)
  });
  Graph {
    decisions,
    documents,
    last_extraction: Some(options.batch.to_owned()),
    relationships,
    units,
    version: 1,
    warnings: append_warnings(&retained_warnings(options.graph, &scope), incoming_warnings),
  }
}

fn finding_scope(
  graph: &Graph,
  target: &str,
  batch: &str,
  fallback: &[WarningScope],
) -> Vec<WarningScope> {
  let decisions: Vec<WarningScope> = graph
    .decisions
    .iter()
    .filter(|entry| {
      entry.id == target
        || (entry.batch == batch && (entry.local_id == target || entry.document == target))
    })
    .map(|entry| WarningScope {
      document: entry.document.clone(),
      line_end: entry.line_end,
      line_start: entry.line_start,
      version: entry.version.clone(),
    })
    .collect();
  let relationships: Vec<WarningScope> = graph
    .relationships
    .iter()
    .filter(|entry| entry.id == target || (entry.batch == batch && entry.local_id == target))
    .flat_map(|entry| {
      entry.evidence.iter().filter_map(|citation| {
        citation.version.clone().map(|version| WarningScope {
          document: citation.document.clone(),
          line_end: citation.line_end,
          line_start: citation.line_start,
          version,
        })
      })
    })
    .collect();
  if !decisions.is_empty() || !relationships.is_empty() {
    decisions.into_iter().chain(relationships).collect()
  } else {
    let selected: Vec<_> = fallback
      .iter()
      .filter(|scope| scope.document == target)
      .cloned()
      .collect();
    if selected.is_empty() {
      fallback.to_vec()
    } else {
      selected
    }
  }
}

pub fn check_impact(
  graph: &Graph,
  check: &KnowledgeCheck,
  batch: &str,
  scope: &[WarningScope],
) -> CheckImpact {
  let targets: HashSet<&str> = check
    .findings
    .iter()
    .map(|finding| finding.target.as_str())
    .collect();
  let referenced_ids: HashSet<&str> = graph
    .relationships
    .iter()
    .filter(|entry| entry.batch == batch)
    .flat_map(|entry| [entry.from.as_str(), entry.to.as_str()])
    .collect();
  let mut known: HashSet<&str> = HashSet::from(["batch"]);
  known.extend(scope.iter().map(|entry| entry.document.as_str()));
  known.extend(graph.decisions.iter().map(|entry| entry.id.as_str()));
  known.extend(graph.relationships.iter().map(|entry| entry.id.as_str()));
  known.extend(
    graph
      .decisions
      .iter()
      .filter(|entry| entry.batch == batch)
      .flat_map(|entry| [entry.local_id.as_str(), entry.document.as_str()]),
  );
  known.extend(
    graph
      .relationships
      .iter()
      .filter(|entry| entry.batch == batch)
      .map(|entry| entry.local_id.as_str()),
  );
  let is_uncertain_batch =
    targets.contains("batch") || targets.iter().any(|target| !known.contains(target));
  let decision_ids: HashSet<String> = graph
    .decisions
    .iter()
    .filter(|entry| {
      targets.contains(entry.id.as_str())
        || (targets.contains(entry.document.as_str())
          && (entry.batch == batch || referenced_ids.contains(entry.id.as_str())))
        || (entry.batch == batch && targets.contains(entry.local_id.as_str()))
    })
    .map(|entry| entry.id.clone())
    .collect();
  let relationship_ids: HashSet<String> = graph
    .relationships
    .iter()
    .filter(|entry| {
      targets.contains(entry.id.as_str())
        || (entry.batch == batch && targets.contains(entry.local_id.as_str()))
        || decision_ids.contains(&entry.from)
        || decision_ids.contains(&entry.to)
    })
    .map(|entry| entry.id.clone())
    .collect();
  CheckImpact {
    decision_ids,
    is_uncertain_batch,
    relationship_ids,
  }
}

pub(super) fn check_warnings(
  graph: &Graph,
  check: &KnowledgeCheck,
  batch: &str,
  scope: &[WarningScope],
) -> Vec<Warning> {
  check
    .findings
    .iter()
    .map(|finding| {
      Warning::Structured(WarningRecord {
        kind: Some("finding".to_owned()),
        message: finding.reason.clone(),
        scope: finding_scope(graph, &finding.target, batch, scope),
        target: Some(finding.target.clone()),
        ..WarningRecord::default()
      })
    })
    .collect()
}

pub fn apply_check(
  graph: &Graph,
  check: &KnowledgeCheck,
  batch: &str,
  scope: &[WarningScope],
) -> Graph {
  let targets: HashSet<&str> = check
    .findings
    .iter()
    .map(|finding| finding.target.as_str())
    .collect();
  let impact = check_impact(graph, check, batch, scope);
  let decisions: Vec<Decision> = graph
    .decisions
    .iter()
    .map(|entry| {
      if entry.batch != batch && !impact.decision_ids.contains(&entry.id) {
        return entry.clone();
      }
      let uncertain = impact.decision_ids.contains(&entry.id)
        || entry.quality == "uncertain"
        || impact.is_uncertain_batch;
      Decision {
        quality: if uncertain { "uncertain" } else { "checked" }.to_owned(),
        ..entry.clone()
      }
    })
    .collect();
  let relationships: Vec<Relationship> = graph
    .relationships
    .iter()
    .map(|entry| {
      if entry.batch != batch && !targets.contains(entry.id.as_str()) {
        return entry.clone();
      }
      let uncertain = impact.relationship_ids.contains(&entry.id)
        || impact.is_uncertain_batch
        || decisions.iter().any(|decision| {
          (decision.id == entry.from || decision.id == entry.to) && decision.quality == "uncertain"
        });
      Relationship {
        quality: if uncertain { "uncertain" } else { "checked" }.to_owned(),
        ..entry.clone()
      }
    })
    .collect();
  let warnings = check_warnings(graph, check, batch, scope);
  Graph {
    decisions,
    relationships,
    warnings: append_warnings(&graph.warnings, warnings),
    ..graph.clone()
  }
}

fn retained_decisions(options: ExtractionOptions<'_>) -> Vec<Decision> {
  let referenced_ids: HashSet<String> = options
    .extraction
    .relationships
    .iter()
    .flat_map(|entry| [entry.from.clone(), entry.to.clone()])
    .collect();
  let decisions: Vec<Decision> = options
    .graph
    .decisions
    .iter()
    .filter(|entry| {
      let source = decision_source(entry, options.documents);
      let is_target_range = options.target_ranges.is_some_and(|ranges| {
        ranges.iter().any(|range| {
          range.document == entry.document
            && overlaps(
              range.line_start,
              range.line_end,
              entry.line_start,
              entry.line_end,
            )
        })
      });
      let is_referenced = options
        .existing_ids
        .is_some_and(|ids| ids.contains(&entry.id))
        && referenced_ids.contains(&entry.id)
        && in_ranges(
          &Citation {
            document: entry.document.clone(),
            line_end: entry.line_end,
            line_start: entry.line_start,
            version: Some(entry.version.clone()),
          },
          options.context_ranges,
        );
      let retained = !is_target_range || is_referenced;
      source.is_none_or(|source| {
        source.hash == entry.version
          && valid_citation(
            &Citation {
              document: entry.document.clone(),
              line_end: entry.line_end,
              line_start: entry.line_start,
              version: Some(entry.version.clone()),
            },
            std::slice::from_ref(&source),
          )
          && retained
      })
    })
    .cloned()
    .collect();
  decisions
}

fn integrate_decisions(
  options: ExtractionOptions<'_>,
  decisions: &mut Vec<Decision>,
  ids: &mut HashMap<String, String>,
  extraction_warnings: &mut Vec<Warning>,
) {
  for entry in &options.extraction.decisions {
    let Some(source) = options
      .documents
      .iter()
      .find(|document| document.id == entry.document)
    else {
      extraction_warnings.push(Warning::Structured(WarningRecord {
        kind: Some("validation".to_owned()),
        message: format!("Decision {} has an unknown source.", entry.id),
        ..WarningRecord::default()
      }));
      continue;
    };
    if ids.contains_key(&entry.id) {
      extraction_warnings.push(Warning::Structured(WarningRecord {
        kind: Some("validation".to_owned()),
        message: format!("Decision {} has a duplicate ID.", entry.id),
        ..WarningRecord::default()
      }));
      continue;
    }
    let citation = Citation {
      document: entry.document.clone(),
      line_end: entry.line_end,
      line_start: entry.line_start,
      version: Some(source.hash.clone()),
    };
    let located =
      valid_citation(&citation, options.documents) && in_ranges(&citation, options.target_ranges);
    if !located {
      extraction_warnings.push(Warning::Structured(WarningRecord {
        kind: Some("validation".to_owned()),
        message: format!(
          "Decision {} has an unverified line range; its document remains available.",
          entry.id
        ),
        ..WarningRecord::default()
      }));
    }
    let id = decision_identity(entry, &source.hash);
    ids.insert(entry.id.clone(), id.clone());
    if let Some(previous) = decisions.iter().position(|decision| decision.id == id) {
      decisions.remove(previous);
    }
    decisions.push(Decision {
      conditions: entry.conditions.clone(),
      document: entry.document.clone(),
      exceptions: entry.exceptions.clone(),
      id,
      kind: entry.kind.clone(),
      line_end: entry.line_end,
      line_start: entry.line_start,
      reason: entry.reason.clone(),
      status: entry.status.clone(),
      text: entry.text.clone(),
      batch: options.batch.to_owned(),
      local_id: entry.id.clone(),
      quality: if located { "unchecked" } else { "uncertain" }.to_owned(),
      version: source.hash.clone(),
      field_order: LIVE_DECISION_FIELDS
        .iter()
        .map(|field| (*field).to_owned())
        .collect(),
    });
  }
}

fn integrate_relationships(
  options: ExtractionOptions<'_>,
  decisions: &[Decision],
  ids: &HashMap<String, String>,
  extraction_warnings: &mut Vec<Warning>,
) -> Vec<Relationship> {
  let available: HashSet<String> = decisions.iter().map(|entry| entry.id.clone()).collect();
  let mut relationships: Vec<Relationship> = options
    .graph
    .relationships
    .iter()
    .filter(|entry| {
      available.contains(&entry.from)
        && available.contains(&entry.to)
        && !relationship_changed(entry, &options)
    })
    .cloned()
    .collect();
  let mut seen_relationship_ids = HashSet::new();
  for entry in &options.extraction.relationships {
    let from = ids.get(&entry.from).cloned();
    let to = ids.get(&entry.to).cloned();
    let context = options.context_documents.unwrap_or(options.documents);
    let invalid_evidence = entry.evidence.iter().any(|citation| {
      !valid_citation(citation, context) || !in_ranges(citation, options.context_ranges)
    });
    let missing_endpoint = from.is_none() || to.is_none();
    let duplicate = seen_relationship_ids.contains(&entry.id);
    if missing_endpoint || duplicate || invalid_evidence {
      let reason = match (missing_endpoint, duplicate) {
        (true, _) => "an unknown endpoint",
        (false, true) => "a duplicate ID",
        (false, false) => "invalid evidence",
      };
      let message = format!("Relationship {} has {reason}.", entry.id);
      extraction_warnings.push(Warning::Structured(WarningRecord {
        kind: Some("validation".to_owned()),
        message,
        ..WarningRecord::default()
      }));
      continue;
    }
    seen_relationship_ids.insert(entry.id.clone());
    let evidence: Vec<Citation> = entry
      .evidence
      .iter()
      .map(|citation| Citation {
        version: source_version(context, &citation.document),
        ..citation.clone()
      })
      .collect();
    let id = relationship_identity(
      entry,
      from.as_deref().unwrap_or_default(),
      to.as_deref().unwrap_or_default(),
      &evidence,
    );
    if let Some(previous) = relationships
      .iter()
      .position(|relationship| relationship.id == id)
    {
      relationships.remove(previous);
    }
    relationships.push(Relationship {
      evidence,
      from: from.unwrap_or_default(),
      id,
      reason: entry.reason.clone(),
      to: to.unwrap_or_default(),
      kind: entry.kind.clone(),
      batch: options.batch.to_owned(),
      local_id: entry.id.clone(),
      quality: "unchecked".to_owned(),
      field_order: LIVE_RELATIONSHIP_FIELDS
        .iter()
        .map(|field| (*field).to_owned())
        .collect(),
    });
  }
  relationships
}
