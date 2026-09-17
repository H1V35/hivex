use crate::knowledge::ingestion::RepairRange;
use serde_json::Value;

pub struct Operation {
    pub command: String,
    pub query: String,
    pub root: String,
    pub sources: Vec<String>,
    pub base: Option<String>,
    pub binary: String,
    pub deadline_ms: u64,
    pub limit: usize,
    pub max_calls: Option<u64>,
    pub max_input_bytes: Option<u64>,
    pub max_context_bytes: usize,
    pub repair: Vec<String>,
    pub repair_ranges: Vec<RepairRange>,
    pub repair_reason: String,
    pub retry_failed: bool,
    pub implementation: Option<Value>,
    pub retrieval_query: Option<String>,
}
