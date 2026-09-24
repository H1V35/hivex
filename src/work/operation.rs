use crate::documents::RepairRange;
use serde_json::Value;

pub struct Operation {
  pub command: String,
  pub query: String,
  pub root: String,
  pub sources: Vec<String>,
  pub base: Option<String>,
  pub execution: crate::execution::Execution,
  pub limit: usize,
  pub max_calls: Option<u64>,
  pub max_input_bytes: Option<u64>,
  pub max_context_bytes: usize,
  pub repair: Vec<String>,
  pub repair_ranges: Vec<RepairRange>,
  pub repair_reason: String,
  pub retry_failed: bool,
  pub resolve: Option<String>,
  pub correct: Option<String>,
  pub implementation: Option<Value>,
  pub retrieval_query: Option<String>,
}

impl Operation {
  pub fn retrieval(&self) -> crate::knowledge::Retrieval<'_> {
    crate::knowledge::Retrieval {
      command: &self.command,
      query: self.retrieval_query.as_deref().unwrap_or(&self.query),
      sources: &self.sources,
      limit: self.limit,
    }
  }
}
