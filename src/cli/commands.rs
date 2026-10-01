use super::arguments;
use crate::error::{HivexError, Result};
use serde_json::Value;

fn argument(message: &str) -> HivexError {
  HivexError::new("INVALID_ARGUMENT", message)
}

pub(super) fn initialize(args: &[String]) -> Result<Value> {
  let parsed = arguments::parse(args, &["root"], &[])
    .map_err(|parse_error| HivexError::new("INVALID_ARGUMENT", parse_error.to_string()))?;
  if parsed.positionals.len() != 1 || parsed.positionals[0] != "init" {
    return Err(argument("Use init [--root <project>]"));
  }

  crate::foundation::initialize(parsed.values.get("root").map(String::as_str))
}
