use serde_json::{Map, Value};

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
