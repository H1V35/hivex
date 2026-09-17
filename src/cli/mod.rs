pub(crate) mod arguments;
mod documents;
mod options;
use crate::error::Result;
use crate::{foundation, work};
#[cfg(test)]
pub(crate) use options::operation_for;
use serde_json::Value;
pub fn run(args: &[String]) -> Result<Value> {
    if args.is_empty() || args.iter().any(|arg| arg == "--help") {
        return Ok(serde_json::from_str(include_str!("help.json"))?);
    }
    match args[0].as_str() {
        "sources" | "read" => documents::command(args),
        "init" => foundation::command(args),
        "recover" | "prune" => work::maintenance::command(args),
        "snapshot" => crate::knowledge::snapshot_command::command(args),
        "warnings" => crate::knowledge::warnings::command(args),
        "review" if args.iter().any(|arg| arg == "--check") => crate::review::check_review(args),
        _ => options::command(args),
    }
}
