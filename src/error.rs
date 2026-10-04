use serde_json::{Value, json};
use std::fmt;

pub type Result<T> = std::result::Result<T, HivexError>;

const MAX_MESSAGE_BYTES: usize = 256;
const MAX_DETAILS_BYTES: usize = 384;

#[derive(Clone, Debug)]
pub struct HivexError {
  pub code: String,
  pub message: String,
  pub details: Option<Value>,
}

impl HivexError {
  pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
    Self {
      code: code.into(),
      message: message.into(),
      details: None,
    }
  }

  pub fn with_details(mut self, details: Value) -> Self {
    self.details = Some(details);
    self
  }

  /// The stderr report: the message is cut to 256 serialized bytes and
  /// oversized details are omitted, so diagnostics stay bounded.
  pub fn diagnostic(&self) -> Value {
    let message = truncate_serialized(&self.message, MAX_MESSAGE_BYTES);
    let mut error = json!({"code": self.code});
    if let Some(details) = &self.details {
      error["details"] = if details.to_string().len() > MAX_DETAILS_BYTES {
        json!({"omitted": true})
      } else {
        details.clone()
      };
    }
    error["message"] = json!(message);
    error["messageTruncated"] = json!(message.len() < self.message.len());
    json!({"error": error})
  }
}

/// The longest prefix whose JSON string serialization fits `max_bytes`.
fn truncate_serialized(text: &str, max_bytes: usize) -> &str {
  let mut used = 2;
  for (index, character) in text.char_indices() {
    used += serde_json::to_string(&character).map_or(0, |escaped| escaped.len() - 2);
    if used > max_bytes {
      return &text[..index];
    }
  }
  text
}

impl fmt::Display for HivexError {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.message)
  }
}

impl std::error::Error for HivexError {}

impl From<std::io::Error> for HivexError {
  fn from(error: std::io::Error) -> Self {
    Self::new("READ_FAILED", error.to_string())
  }
}

impl From<rusqlite::Error> for HivexError {
  fn from(error: rusqlite::Error) -> Self {
    Self::new("READ_FAILED", error.to_string())
  }
}

impl From<serde_json::Error> for HivexError {
  fn from(error: serde_json::Error) -> Self {
    Self::new("READ_FAILED", error.to_string())
  }
}
