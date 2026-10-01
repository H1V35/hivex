mod arguments;
mod commands;
mod documents;
mod relations;
mod search;
use crate::error::{HivexError, Result};
use serde_json::Value;

pub fn run(args: &[String]) -> Result<Value> {
  if args.is_empty() || args.iter().any(|arg| arg == "--help") {
    return Ok(serde_json::from_str(include_str!("help.json"))?);
  }
  match args[0].as_str() {
    "sources" | "read" => documents::command(args),
    "search" => search::command(args),
    "relations" => relations::command(args),
    "init" => commands::initialize(args),
    "update" | "ask" | "neighbors" | "review" | "warnings" | "snapshot" | "recover" | "prune"
    | "status" => Err(HivexError::new(
      "COMMAND_RETIRED",
      "The inferred graph runtime was retired in 0.8. Use sources/search/relations/read and the migration guide; legacy files remain untouched.",
    )),
    _ => {
      arguments::parse(args, &["root"], &[])?;
      Err(HivexError::new(
        "INVALID_ARGUMENT",
        "Use sources, search, relations, read or init; see --help",
      ))
    }
  }
}
