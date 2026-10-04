//! Byte-bounded result pages and the continuations that resume them.
use super::arguments::Arguments;
use crate::error::{HivexError, Result};
use serde_json::Value;

const DEFAULT_MAX_BYTES: usize = 16_384;
const MAX_BYTES: usize = 65_536;

/// `--max-bytes`, the bound of a complete serialized response.
pub fn max_bytes(arguments: &Arguments) -> Result<usize> {
  arguments.count("max-bytes", DEFAULT_MAX_BYTES, MAX_BYTES)
}

/// A continuation `{version}.{key}.{offset}`. The key binds it to the
/// snapshot and options of the result it pages.
pub struct Cursor<'a> {
  pub version: &'static str,
  pub key: &'a str,
}

impl Cursor<'_> {
  pub fn encode(&self, offset: usize) -> String {
    format!("{}.{}.{offset}", self.version, self.key)
  }

  /// The offset `supplied` resumes from; 0 when absent.
  pub fn offset(&self, supplied: Option<&str>) -> Result<usize> {
    let Some(supplied) = supplied else {
      return Ok(0);
    };
    supplied
      .strip_prefix(self.version)
      .and_then(|rest| {
        rest
          .strip_prefix('.')?
          .strip_prefix(self.key)?
          .strip_prefix('.')
      })
      .filter(|offset| !offset.is_empty() && offset.bytes().all(|byte| byte.is_ascii_digit()))
      .and_then(|offset| offset.parse().ok())
      .ok_or_else(|| {
        HivexError::new(
          "INVALID_CURSOR",
          "Continuation belongs to a different snapshot or query options",
        )
      })
  }

  /// The continuation after a page ending at `next`, if records remain.
  pub fn next(&self, next: usize, total: usize) -> Option<String> {
    (next < total).then(|| self.encode(next))
  }
}

/// Reject a continuation that starts past the end of its result.
pub fn check_offset(offset: usize, total: usize) -> Result<()> {
  if offset > 0 && offset >= total {
    return Err(HivexError::new(
      "INVALID_CURSOR",
      "Continuation is outside this result",
    ));
  }
  Ok(())
}

/// Render the longest prefix of `records` whose complete response fits
/// `max_bytes`. Records are never cut: when the first does not fit,
/// `oversized` explains it.
pub fn fill<T>(
  records: impl IntoIterator<Item = T>,
  max_bytes: usize,
  render: impl Fn(&[T]) -> Value,
  oversized: impl FnOnce(&T) -> HivexError,
) -> Result<Value> {
  let fits = |response: &Value| response.to_string().len() <= max_bytes;
  let mut page = Vec::new();
  for record in records {
    page.push(record);
    if fits(&render(&page)) {
      continue;
    }
    let record = page.pop().expect("a record was just added");
    if page.is_empty() {
      return Err(oversized(&record));
    }
    break;
  }
  let response = render(&page);
  if !fits(&response) {
    return Err(HivexError::new(
      "OUTPUT_LIMIT",
      "Response metadata exceeds --max-bytes",
    ));
  }
  Ok(response)
}
