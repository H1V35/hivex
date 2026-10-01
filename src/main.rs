mod cli;
mod compatibility;
mod documents;
mod error;
mod foundation;

use serde_json::Value;
use std::io::{self, Write};

fn main() {
  let args: Vec<String> = std::env::args().skip(1).collect();
  let code = match cli::run(&args) {
    Ok(result) => {
      if writeln!(io::stdout().lock(), "{result}").is_err() {
        1
      } else {
        i32::from(matches!(
          result.get("status").and_then(Value::as_str),
          Some("failed")
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
