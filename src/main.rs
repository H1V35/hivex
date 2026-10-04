mod cli;
mod compatibility;
mod documents;
mod error;
mod foundation;

use error::HivexError;
use std::ffi::OsString;
use std::io::{self, Write};

/// Print the command's JSON to stdout, or its error to stderr. The exit code
/// is 1 for an error or a `failed` result.
fn main() {
  let result = std::env::args_os()
    .skip(1)
    .map(OsString::into_string)
    .collect::<Result<Vec<_>, _>>()
    .map_err(|_| HivexError::new("INVALID_ARGUMENT", "Arguments must be valid UTF-8"))
    .and_then(|args| cli::run(&args));
  let code = match result {
    Ok(output) => {
      let printed = writeln!(io::stdout().lock(), "{output}").is_ok();
      i32::from(!printed || output["status"] == "failed")
    }
    Err(error) => {
      let _ = writeln!(io::stderr().lock(), "{}", error.diagnostic());
      1
    }
  };
  std::process::exit(code);
}
