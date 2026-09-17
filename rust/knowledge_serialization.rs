use serde_json::{Map, Value};
use std::collections::HashSet;

const CITATION: &[&str] = &[
    "document",
    "lineStart",
    "lineEnd",
    "version",
    "historical",
    "text",
];
const CODE: &[&str] = &["path", "side", "lineStart", "lineEnd", "version", "text"];
const CONTEXT: &[&str] = &[
    "command",
    "snapshot",
    "documents",
    "unavailableDocuments",
    "unexpandedDecisions",
    "decisions",
    "relationships",
    "pendingDocuments",
    "warnings",
];
const DECISION: &[&str] = &[
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
const DOCUMENT: &[&str] = &[
    "id",
    "title",
    "status",
    "historical",
    "version",
    "lineCount",
    "lines",
];
const EXTRACTION: &[&str] = &[
    "operation",
    "targets",
    "repairReason",
    "units",
    "documents",
    "existing",
    "previousRelationships",
    "scope",
    "extraction",
];
const FILE: &[&str] = &["path", "before", "after"];
const FINDING: &[&str] = &["target", "reason"];
const IMPLEMENTATION: &[&str] = &["baseCommit", "diff", "files", "warnings", "fingerprint"];
const PACKET: &[&str] = &[
    "operation",
    "implementation",
    "task",
    "context",
    "documents",
    "omittedUnits",
    "warnings",
];
const QUERIED_DECISION: &[&str] = &[
    "historical",
    "id",
    "document",
    "version",
    "text",
    "kind",
    "status",
    "quality",
    "conditions",
    "exceptions",
    "reason",
    "evidence",
];
const RELATIONSHIP: &[&str] = &[
    "id", "from", "to", "type", "reason", "evidence", "batch", "localId", "quality",
];
const REVIEW_FINDING: &[&str] = &["assessment", "explanation", "documents", "code"];
const UNIT: &[&str] = &["id", "document", "hash", "lineStart", "lineEnd", "text"];
const VERSION: &[&str] = &["version", "lines"];
const WARNING: &[&str] = &["path", "message"];

fn has_fields(value: &Map<String, Value>, fields: &[&str]) -> bool {
    fields.iter().all(|field| value.contains_key(*field))
}

fn has_order_prefix(value: &Map<String, Value>, fields: &[&str]) -> bool {
    fields
        .iter()
        .enumerate()
        .all(|(index, field)| value.keys().nth(index).is_some_and(|key| key == *field))
}

fn order_record(value: Map<String, Value>, order: &[&str]) -> Map<String, Value> {
    let mut result = Map::new();
    let mut remaining = value;
    for field in order {
        if let Some(node) = remaining.shift_remove(*field) {
            result.insert((*field).to_owned(), node);
        }
    }
    result.extend(remaining);
    result
}

fn source_order(value: &Map<String, Value>) -> Option<&'static [&'static str]> {
    if has_fields(value, &["kind", "id"]) {
        if value.contains_key("historical") {
            return Some(QUERIED_DECISION);
        }
        return if has_order_prefix(value, &DECISION[..10]) {
            None
        } else {
            Some(DECISION)
        };
    }
    if has_fields(value, &["from", "to", "type"]) {
        return if has_order_prefix(value, &RELATIONSHIP[..6]) {
            None
        } else {
            Some(RELATIONSHIP)
        };
    }
    if has_fields(value, &["id", "title"]) {
        return Some(DOCUMENT);
    }
    if has_fields(value, &["document", "lineStart", "lineEnd"]) {
        return Some(if value.contains_key("id") {
            UNIT
        } else {
            CITATION
        });
    }
    if has_fields(value, &["path", "side"]) {
        return Some(CODE);
    }
    if has_fields(value, &["version", "lines"]) {
        return Some(VERSION);
    }
    if has_fields(value, &["path", "message"]) {
        return Some(WARNING);
    }
    Some(&[])
}

fn packet_order(key: &str, value: &Map<String, Value>) -> Option<&'static [&'static str]> {
    if key.is_empty() && value.get("operation").and_then(Value::as_str).is_some() {
        let operation = value.get("operation").and_then(Value::as_str);
        return Some(
            if operation.is_some_and(|operation| operation == "extract" || operation == "check") {
                EXTRACTION
            } else {
                PACKET
            },
        );
    }
    if key == "context" && has_fields(value, &["command", "snapshot"]) {
        return Some(CONTEXT);
    }
    if has_fields(value, &["baseCommit", "files"]) {
        return Some(IMPLEMENTATION);
    }
    if has_fields(value, &["path", "before", "after"]) {
        return Some(FILE);
    }
    let order = source_order(value);
    if order.is_none() || order.is_some_and(|order| !order.is_empty()) {
        return order;
    }
    if has_fields(value, &["target", "reason"]) {
        return Some(FINDING);
    }
    None
}

fn normalize_packet_value(key: &str, value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(|value| normalize_packet_value("", value))
                .collect(),
        ),
        Value::Object(value) => {
            let normalized = value
                .into_iter()
                .map(|(key, value)| (key.clone(), normalize_packet_value(&key, value)))
                .collect::<Map<_, _>>();
            match packet_order(key, &normalized) {
                Some(order) => Value::Object(order_record(normalized, order)),
                None => Value::Object(normalized),
            }
        }
        value => value,
    }
}

pub fn normalize_knowledge(packet: &Value) -> Value {
    normalize_packet_value("", packet.clone())
}

pub fn stringify_knowledge(packet: &Value) -> String {
    serde_json::to_string(&normalize_knowledge(packet)).expect("serde_json values are serializable")
}

pub fn with_live_provenance(value: &Value, kind: &str) -> Value {
    let Value::Object(mut value) = value.clone() else {
        return value.clone();
    };
    let fields: Vec<&str> = match kind {
        "decision" => DECISION[..10]
            .iter()
            .copied()
            .chain(["localId", "version", "batch", "quality"])
            .collect(),
        "relationship" => RELATIONSHIP[..6]
            .iter()
            .copied()
            .chain(["localId", "batch", "quality"])
            .collect(),
        _ => return Value::Object(value),
    };
    for field in fields {
        if let Some(node) = value.shift_remove(field) {
            value.insert(field.to_owned(), node);
        }
    }
    Value::Object(value)
}

fn schema_order(properties: &Map<String, Value>) -> Option<&'static [&'static str]> {
    if has_fields(properties, &["kind", "id"]) {
        Some(DECISION)
    } else if has_fields(properties, &["from", "to", "type"]) {
        Some(RELATIONSHIP)
    } else if has_fields(properties, &["document", "lineStart", "lineEnd"]) {
        Some(CITATION)
    } else if has_fields(properties, &["path", "side"]) {
        Some(CODE)
    } else if has_fields(properties, &["assessment", "explanation"]) {
        Some(REVIEW_FINDING)
    } else if has_fields(properties, &["target", "reason"]) {
        Some(FINDING)
    } else {
        None
    }
}

fn normalize_schema_node(value: Value) -> Value {
    match value {
        Value::Array(values) => {
            Value::Array(values.into_iter().map(normalize_schema_node).collect())
        }
        Value::Object(value) => {
            // Rebuild in iteration order, as Object.assign/Object.fromEntries do in
            // the TypeScript serializer. In particular, touching `properties` must
            // not move that key to the end of its parent object.
            let mut normalized = value
                .into_iter()
                .map(|(key, node)| (key, normalize_schema_node(node)))
                .collect::<Map<_, _>>();
            let property_order = normalized
                .get("properties")
                .and_then(Value::as_object)
                .and_then(schema_order)
                .map(|order| order.to_vec());
            if let Some(order) = property_order {
                if let Some(Value::Object(properties)) = normalized.get_mut("properties") {
                    let properties = std::mem::take(properties);
                    *normalized
                        .get_mut("properties")
                        .expect("properties remains present") =
                        Value::Object(order_record(properties, &order));
                }
                if let Some(Value::Array(required)) = normalized.get_mut("required") {
                    let original = required.clone();
                    let mut names = HashSet::new();
                    let mut reordered = Vec::new();
                    for name in &order {
                        if let Some(required_name) = original
                            .iter()
                            .find(|required_name| required_name.as_str() == Some(*name))
                            && names.insert(required_name.clone())
                        {
                            reordered.push(required_name.clone());
                        }
                    }
                    for required_name in original {
                        if names.insert(required_name.clone()) {
                            reordered.push(required_name);
                        }
                    }
                    *required = reordered;
                }
            }
            Value::Object(normalized)
        }
        value => value,
    }
}

pub fn schema_for_knowledge(schema: &Value) -> Option<Value> {
    schema
        .is_object()
        .then(|| normalize_schema_node(schema.clone()))
}
