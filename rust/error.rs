use serde_json::{Value, json};
use std::fmt;

pub type Result<T> = std::result::Result<T, HivexError>;

#[derive(Debug)]
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

    pub fn diagnostic(&self) -> Value {
        let mut message = String::new();
        for character in self.message.chars() {
            let previous = message.len();
            message.push(character);
            if serde_json::to_vec(&message)
                .expect("string serializes")
                .len()
                > 256
            {
                message.truncate(previous);
                break;
            }
        }
        let mut error = json!({"code": self.code});
        if let Some(details) = &self.details {
            error["details"] = if details.to_string().len() > 384 {
                json!({"omitted": true})
            } else {
                details.clone()
            };
        }
        error["message"] = json!(message);
        error["messageTruncated"] = json!(message != self.message);
        json!({"error": error})
    }
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
