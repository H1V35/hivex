use crate::error::{HivexError, Result};
use std::collections::{HashMap, HashSet};

pub(crate) use crate::compatibility::parse_number;
#[derive(Default)]
pub struct Parsed {
  pub positionals: Vec<String>,
  pub values: HashMap<String, String>,
  pub flags: HashSet<String>,
  pub repeated: HashMap<String, Vec<String>>,
}

pub fn parse(args: &[String], string_options: &[&str], bool_options: &[&str]) -> Result<Parsed> {
  let mut parsed = Parsed::default();
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
    let (option, supplied) = argument
      .split_once('=')
      .map_or((argument.as_str(), None), |(key, value)| (key, Some(value)));
    let name = option.strip_prefix("--").unwrap_or(option);
    if string_options.contains(&name) {
      let value = string_value(option, supplied, &mut input)?;
      parsed
        .repeated
        .entry(name.to_owned())
        .or_default()
        .push(value.clone());
      parsed.values.insert(name.to_owned(), value);
      continue;
    }
    if bool_options.contains(&name) {
      if supplied.is_some() {
        return Err(HivexError::new(
          "READ_FAILED",
          format!("Option '{option}' does not take an argument"),
        ));
      }
      parsed.flags.insert(name.to_owned());
      continue;
    }
    return Err(unknown_option(option));
  }
  Ok(parsed)
}

fn string_value(
  option: &str,
  supplied: Option<&str>,
  input: &mut std::slice::Iter<'_, String>,
) -> Result<String> {
  if let Some(value) = supplied {
    return Ok(value.to_owned());
  }
  let value = input.next().ok_or_else(|| {
    HivexError::new(
      "READ_FAILED",
      format!("Option '{option} <value>' argument missing"),
    )
  })?;
  if value.starts_with('-') && value != "-" {
    return Err(HivexError::new(
      "READ_FAILED",
      format!(
        "Option '{option}' argument is ambiguous.\nDid you forget to specify the option argument for '{option}'?\nTo specify an option argument starting with a dash use '{option}=-XYZ'."
      ),
    ));
  }
  Ok(value.clone())
}

fn unknown_option(option: &str) -> HivexError {
  let option = if option.starts_with("--") || option.chars().count() == 2 {
    option.to_owned()
  } else {
    option.chars().skip(1).take(1).collect()
  };
  HivexError::new(
    "READ_FAILED",
    format!(
      "Unknown option '{option}'. To specify a positional argument starting with a '-', place it at the end of the command after '--', as in '-- \"{option}\""
    ),
  )
}
