mod arguments;
mod check;
mod documents;
mod page;
mod relations;
mod search;

use crate::documents::Project;
use crate::error::{HiveXError, Result};
use arguments::{Arguments, invalid};
use serde_json::Value;
use std::io::Read;
use std::path::Path;

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
    "init" => initialize(args),
    "update" => update(args),
    "hook" => hook(args),
    "ask" | "neighbors" | "review" | "warnings" | "snapshot" | "recover" | "prune" | "status" => {
      Err(HiveXError::new(
        "COMMAND_RETIRED",
        "The inferred graph runtime was retired in 0.8. Use sources/search/relations/read and the migration guide; legacy files remain untouched.",
      ))
    }
    _ => Err(invalid(
      "Use sources, read, search, relations, check, init, update or hook; see --help",
    )),
  }
}

fn initialize(args: &[String]) -> Result<Value> {
  let arguments = Arguments::parse(args, &["root", "extension"], &[])?;
  if arguments.positionals().len() != 1 {
    return Err(invalid(
      "Use init [--extension <repository>] [--root <project>]",
    ));
  }
  let report = crate::foundation::initialize(arguments.root(), arguments.value("extension"))?;
  Ok(with_hooks(report, arguments.root()))
}

fn update(args: &[String]) -> Result<Value> {
  let arguments = Arguments::parse(args, &["root"], &[])?;
  if arguments.positionals().len() != 1 {
    return Err(invalid("Use update [--root <project>]"));
  }
  let report = crate::foundation::update(arguments.root())?;
  Ok(with_hooks(report, arguments.root()))
}

/// Adds to an `init` or `update` report whether each host the project uses registers
/// `hivex hook`, recommending the configuration when it does not.
fn with_hooks(mut report: Value, root: &str) -> Value {
  if let Some(object) = report.as_object_mut() {
    object.insert("hooks".into(), crate::hook::registrations(Path::new(root)));
    object.sort_keys();
  }
  report
}

/// Answer one Claude Code or Codex hook event read from stdin. The hook answers every input and
/// lets through any it does not recognise, so it never returns an error.
fn hook(args: &[String]) -> Result<Value> {
  if args.len() != 1 {
    return Err(invalid("Use hook, with the host's hook event on stdin"));
  }
  let mut input = Vec::new();
  // An unreadable stdin leaves the input empty, which the hook lets through with a warning. Input
  // beyond the limit is read and dropped, so the host can finish writing it.
  let mut stdin = std::io::stdin().lock();
  let limit = u64::try_from(crate::hook::MAX_INPUT + 1).unwrap_or(u64::MAX);
  let _ = Read::read_to_end(&mut Read::take(&mut stdin, limit), &mut input);
  let _ = std::io::copy(&mut stdin, &mut std::io::sink());
  Ok(crate::hook::handle(&input))
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
