mod arguments;
mod commands;
mod documents;
mod options;
mod snapshot;
use crate::error::Result;
#[cfg(test)]
pub(crate) use options::operation_for;
use serde_json::Value;
pub fn run(args: &[String]) -> Result<Value> {
  if args.is_empty() || args.iter().any(|arg| arg == "--help") {
    return Ok(serde_json::from_str(include_str!("help.json"))?);
  }
  match args[0].as_str() {
    "sources" | "read" => documents::command(args),
    "init" => commands::initialize(args),
    "recover" | "prune" => commands::maintenance(args),
    "snapshot" => snapshot::command(args),
    "warnings" => commands::warnings(args),
    "review" if args.iter().any(|arg| arg == "--check") => commands::check_review(args),
    _ => options::command(args),
  }
}
