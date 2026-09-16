mod arguments;
mod documents;
mod error;
mod initialization;
mod maintenance;
mod markdown;

use error::{HivexError, Result};
use serde_json::Value;
use std::io::{self, Write};

fn run(args: &[String]) -> Result<Value> {
    if args.is_empty() || args.iter().any(|arg| arg == "--help") {
        return Ok(serde_json::from_str(include_str!("help.json"))?);
    }
    match args[0].as_str() {
        "sources" | "read" => documents::command(args),
        "init" => initialization::command(args),
        "recover" | "prune" => maintenance::command(args),
        "update" | "search" | "neighbors" | "ask" | "review" | "snapshot" | "warnings"
        | "status" => Err(HivexError::new(
            "MIGRATION_INCOMPLETE",
            "This development binary currently implements sources, read, init, recover and prune. Use the published CLI for knowledge operations until the Rust migration is complete.",
        )),
        _ => {
            arguments::parse(
                args,
                &[
                    "base",
                    "codex",
                    "deadline-ms",
                    "limit",
                    "max-calls",
                    "max-context-bytes",
                    "max-input-bytes",
                    "reason",
                    "repair",
                    "repair-range",
                    "root",
                    "source",
                ],
                &["retry-failed"],
            )?;
            Err(HivexError::new(
                "INVALID_ARGUMENT",
                "Use update, status, or search/ask/neighbors with one query or ID",
            ))
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match run(&args) {
        Ok(result) => {
            if writeln!(io::stdout().lock(), "{result}").is_err() {
                1
            } else {
                i32::from(matches!(
                    result.get("status").and_then(Value::as_str),
                    Some("failed" | "blocked")
                ))
            }
        }
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "{}", error.diagnostic());
            1
        }
    };
    std::process::exit(code);
}
