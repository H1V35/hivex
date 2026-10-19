mod arguments;
mod check;
mod documents;
mod page;
mod relations;
mod search;

use crate::documents::Project;
use crate::error::{HivexError, Result};
use arguments::{Arguments, invalid};
use serde_json::Value;

const ORIGIN: &str = "current-worktree";

/// Run one command; its JSON result goes to stdout, an error to stderr. Paginated commands carry
/// a version warning when the project pins another release; `init` and `update` enforce the pin.
pub fn run(args: &[String]) -> Result<Value> {
  let mut output = command(args)?;
  if let Some(warning) = page::take_version_warning()
    && let Some(object) = output.as_object_mut()
  {
    object.insert("versionWarning".into(), warning);
  }
  Ok(output)
}

fn command(args: &[String]) -> Result<Value> {
  let Some(command) = args
    .first()
    .filter(|_| !args.iter().any(|arg| arg == "--help"))
  else {
    return Ok(serde_json::from_str(include_str!("help.json"))?);
  };
  match command.as_str() {
    "sources" => documents::sources(args),
    "read" => documents::read(args),
    "search" => search::command(args),
    "relations" => relations::command(args),
    "check" => check::command(args),
    "init" => project(args, "init", crate::foundation::initialize),
    "update" => project(args, "update", crate::foundation::update),
    "ask" | "neighbors" | "review" | "warnings" | "snapshot" | "recover" | "prune" | "status" => {
      Err(HivexError::new(
        "COMMAND_RETIRED",
        "The inferred graph runtime was retired in 0.8. Use sources/search/relations/read and the migration guide; legacy files remain untouched.",
      ))
    }
    _ => Err(invalid(
      "Use sources, read, search, relations, check, init or update; see --help",
    )),
  }
}

fn project(args: &[String], command: &str, run: fn(&str) -> Result<Value>) -> Result<Value> {
  let arguments = Arguments::parse(args, &["root"], &[])?;
  if arguments.positionals().len() != 1 {
    return Err(invalid(format!("Use {command} [--root <project>]")));
  }
  run(arguments.root())
}

fn status(project: &Project) -> &'static str {
  if project.is_partial() {
    "partial"
  } else {
    "ready"
  }
}

fn coverage(project: &Project) -> &'static str {
  if project.is_partial() {
    "partial"
  } else {
    "selected-sources"
  }
}
