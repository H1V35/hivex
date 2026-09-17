use super::arguments;
use crate::compatibility::parse_number;
use crate::error::{HivexError, Result};
use num_traits::ToPrimitive;
use serde_json::Value;
use std::path::PathBuf;

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

pub(super) fn check_review(input: &[String]) -> Result<Value> {
  let parsed = arguments::parse(input, &["check", "root"], &[])?;
  if parsed.positionals.len() != 1
    || parsed.positionals.first().map(String::as_str) != Some("review")
    || parsed.values.get("check").is_none_or(String::is_empty)
  {
    return Err(argument(
      "Use review --check <saved-report.json> [--root <project>].",
    ));
  }
  let root = parsed
    .values
    .get("root")
    .map_or_else(std::env::current_dir, |root| Ok(PathBuf::from(root)))?;
  crate::review::check_review(&root, parsed.values.get("check").expect("checked above"))
}

pub(super) fn warnings(args: &[String]) -> Result<Value> {
  let parsed = arguments::parse(args, &["resolve", "root"], &["all"]).map_err(|mut error| {
    "INVALID_ARGUMENT".clone_into(&mut error.code);
    error
  })?;
  if parsed.positionals.len() != 1 || parsed.positionals[0] != "warnings" {
    return Err(HivexError::new(
      "INVALID_ARGUMENT",
      "Use warnings [--all] [--resolve <resolutions.json>] [--root <project>].",
    ));
  }
  let root = parsed.values.get("root").cloned().unwrap_or_else(|| {
    std::env::current_dir().map_or_else(
      |_| ".".to_owned(),
      |path| path.to_string_lossy().into_owned(),
    )
  });
  crate::knowledge::warning_report(
    &root,
    parsed.values.get("resolve").map(String::as_str),
    parsed.flags.contains("all"),
  )
}

const DEFAULT_KEEP_COMPLETED: usize = 8;
const DEFAULT_KEEP_CACHES: usize = 64;
const USAGE: &str = "Use recover [--acknowledge-uncertain] or prune [--keep-completed <count>] [--keep-caches <count>]";

fn invalid_usage() -> HivexError {
  HivexError::new("INVALID_ARGUMENT", USAGE)
}

fn retention(value: Option<&String>, name: &str, fallback: usize) -> Result<usize> {
  let Some(value) = value else {
    return Ok(fallback);
  };
  // Node's Number() accepts the ordinary decimal and exponent spellings,
  // including an empty string (zero). Keep the same small CLI contract while
  // avoiding lossy values outside the permitted range.
  let number = parse_number(value).ok_or_else(|| invalid_retention(name))?;
  if !number.is_finite() || number.fract() != 0.0 || !(0.0..=4096.0).contains(&number) {
    return Err(invalid_retention(name));
  }
  Ok(number.to_usize().expect("validated positive limit"))
}

fn invalid_retention(name: &str) -> HivexError {
  HivexError::new(
    "INVALID_ARGUMENT",
    format!("{name} must be an integer between 0 and 4096"),
  )
}

pub(super) fn maintenance(args: &[String]) -> Result<Value> {
  let Some(command) = args.first().map(String::as_str) else {
    return Err(invalid_usage());
  };
  if command != "recover" && command != "prune" {
    return Err(invalid_usage());
  }

  let parsed = arguments::parse(
    &args[1..],
    &["root", "keep-completed", "keep-caches"],
    &["acknowledge-uncertain"],
  )?;
  if !parsed.positionals.is_empty() {
    return Err(invalid_usage());
  }
  if command == "recover"
    && (parsed.values.contains_key("keep-completed") || parsed.values.contains_key("keep-caches"))
  {
    return Err(HivexError::new(
      "INVALID_ARGUMENT",
      "recover does not accept retention options",
    ));
  }
  if command == "prune" && parsed.flags.contains("acknowledge-uncertain") {
    return Err(HivexError::new(
      "INVALID_ARGUMENT",
      "prune does not accept --acknowledge-uncertain",
    ));
  }

  let root = parsed
    .values
    .get("root")
    .map_or_else(std::env::current_dir, |value| Ok(PathBuf::from(value)))?;
  let mut storage = crate::work::Store::open(
    &root,
    crate::work::StoreOptions {
      update: command == "prune",
      ..crate::work::StoreOptions::default()
    },
  )?;
  if command == "recover" {
    return crate::work::maintain(
      &mut storage,
      crate::work::Maintenance::Recover {
        acknowledge_uncertain: parsed.flags.contains("acknowledge-uncertain"),
      },
    );
  }
  let keep_completed = retention(
    parsed.values.get("keep-completed"),
    "--keep-completed",
    DEFAULT_KEEP_COMPLETED,
  )?;
  let keep_caches = retention(
    parsed.values.get("keep-caches"),
    "--keep-caches",
    DEFAULT_KEEP_CACHES,
  )?;
  crate::work::maintain(
    &mut storage,
    crate::work::Maintenance::Prune {
      keep_completed,
      keep_caches,
    },
  )
}
