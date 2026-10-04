use crate::error::{HivexError, Result};
use std::collections::HashMap;

/// Exact JavaScript-safe integers keep options and cursors portable to JSON clients.
pub const MAX_SAFE_INTEGER: usize = 9_007_199_254_740_991;

pub fn invalid(message: impl Into<String>) -> HivexError {
  HivexError::new("INVALID_ARGUMENT", message)
}

/// Parsed command line: positionals (including the command name), valued
/// options in order of appearance and boolean flags.
pub struct Arguments {
  positionals: Vec<String>,
  values: HashMap<String, Vec<String>>,
  flags: Vec<String>,
}

impl Arguments {
  /// Accept `--name value`, `--name=value` and `--flag`; `--` ends options.
  pub fn parse(args: &[String], options: &[&str], flags: &[&str]) -> Result<Self> {
    let mut parsed = Self {
      positionals: Vec::new(),
      values: HashMap::new(),
      flags: Vec::new(),
    };
    let mut input = args.iter();
    while let Some(argument) = input.next() {
      if argument == "--" {
        parsed.positionals.extend(input.cloned());
        break;
      }
      if !argument.starts_with('-') || argument == "-" {
        parsed.positionals.push(argument.clone());
        continue;
      }
      let (option, inline) = argument
        .split_once('=')
        .map_or((argument.as_str(), None), |(option, value)| {
          (option, Some(value))
        });
      let name = option.strip_prefix("--").unwrap_or(option);
      if options.contains(&name) {
        let value = option_value(option, inline, &mut input)?;
        parsed
          .values
          .entry(name.to_owned())
          .or_default()
          .push(value);
        continue;
      }
      if !flags.contains(&name) {
        return Err(invalid(format!("Unknown option '{option}'")));
      }
      if inline.is_some() {
        return Err(invalid(format!("Option '{option}' does not take a value")));
      }
      parsed.flags.push(name.to_owned());
    }
    Ok(parsed)
  }

  pub fn positionals(&self) -> &[String] {
    &self.positionals
  }

  /// The last value given for `name`.
  pub fn value(&self, name: &str) -> Option<&str> {
    self.values.get(name)?.last().map(String::as_str)
  }

  pub fn flag(&self, name: &str) -> bool {
    self.flags.iter().any(|flag| flag == name)
  }

  pub fn root(&self) -> &str {
    self.value("root").unwrap_or(".")
  }

  /// Repeated `--source` values, sorted and deduplicated.
  pub fn sources(&self) -> Vec<String> {
    let mut sources = self.values.get("source").cloned().unwrap_or_default();
    sources.sort();
    sources.dedup();
    sources
  }

  /// A positive integer option no greater than `max`.
  pub fn count(&self, name: &str, default: usize, max: usize) -> Result<usize> {
    Ok(self.optional_count(name, max)?.unwrap_or(default))
  }

  pub fn optional_count(&self, name: &str, max: usize) -> Result<Option<usize>> {
    let Some(value) = self.value(name) else {
      return Ok(None);
    };
    let number = value
      .bytes()
      .all(|byte| byte.is_ascii_digit())
      .then(|| value.parse::<usize>().ok())
      .flatten()
      .filter(|number| (1..=MAX_SAFE_INTEGER).contains(number))
      .ok_or_else(|| invalid(format!("--{name} must be a positive integer")))?;
    if number > max {
      return Err(invalid(format!("--{name} must be at most {max}")));
    }
    Ok(Some(number))
  }
}

fn option_value(
  option: &str,
  inline: Option<&str>,
  input: &mut std::slice::Iter<'_, String>,
) -> Result<String> {
  if let Some(value) = inline {
    return Ok(value.to_owned());
  }
  match input.next() {
    None => Err(invalid(format!("Option '{option}' requires a value"))),
    Some(value) if value.starts_with('-') && value != "-" => Err(invalid(format!(
      "Option '{option}' requires a value; write '{option}=-value' for one starting with '-'"
    ))),
    Some(value) => Ok(value.clone()),
  }
}
