use hivex::{documents, error, initialization, knowledge, maintenance};

use error::Result;
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
        "snapshot" => hivex::snapshot_command::command(args),
        "warnings" => hivex::knowledge_warnings::command(args),
        "review" if args.iter().any(|arg| arg == "--check") => hivex::review::check_review(args),
        _ => knowledge::command(args),
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
