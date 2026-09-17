use crate::error::{HivexError, Result};
use std::collections::{HashMap, HashSet};

pub const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

pub fn trim_js_whitespace(value: &str) -> &str {
    value.trim_matches(|character: char| {
        character == '\u{feff}' || (character.is_whitespace() && character != '\u{85}')
    })
}

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
            let value = match supplied {
                Some(value) => value.to_owned(),
                None => {
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
                    value.clone()
                }
            };
            parsed
                .repeated
                .entry(name.to_owned())
                .or_default()
                .push(value.clone());
            parsed.values.insert(name.to_owned(), value);
        } else if bool_options.contains(&name) {
            if supplied.is_some() {
                return Err(HivexError::new(
                    "READ_FAILED",
                    format!("Option '{option}' does not take an argument"),
                ));
            }
            parsed.flags.insert(name.to_owned());
        } else {
            let option = if option.starts_with("--") || option.chars().count() == 2 {
                option.to_owned()
            } else {
                option.chars().skip(1).take(1).collect()
            };
            return Err(HivexError::new(
                "READ_FAILED",
                format!(
                    "Unknown option '{option}'. To specify a positional argument starting with a '-', place it at the end of the command after '--', as in '-- \"{option}\""
                ),
            ));
        }
    }
    Ok(parsed)
}

pub fn parse_number(value: &str) -> Option<f64> {
    let value = trim_js_whitespace(value);
    if value.is_empty() {
        return Some(0.0);
    }
    let (radix, digits) = if let Some(digits) = value.strip_prefix("0x") {
        (16, digits)
    } else if let Some(digits) = value.strip_prefix("0X") {
        (16, digits)
    } else if let Some(digits) = value.strip_prefix("0o") {
        (8, digits)
    } else if let Some(digits) = value.strip_prefix("0O") {
        (8, digits)
    } else if let Some(digits) = value.strip_prefix("0b") {
        (2, digits)
    } else if let Some(digits) = value.strip_prefix("0B") {
        (2, digits)
    } else {
        return value.parse::<f64>().ok();
    };
    (!digits.is_empty())
        .then(|| u64::from_str_radix(digits, radix).ok())
        .flatten()
        .map(|number| number as f64)
}
