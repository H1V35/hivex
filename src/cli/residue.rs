use super::arguments::{Arguments, invalid};
use super::page;
use crate::compatibility::project_directory;
use crate::error::{HiveXError, Result};
use crate::residue::{DECLARATION, sweep};
use serde_json::{Value, json};

const USAGE: &str = "Use residue [--classes <file>] [--trash] [--limit <count>] [--max-bytes <bytes>] [--root <project>]";

/// `residue`: report the declared residue and, with `--trash`, move what is unneeded to the Trash.
pub fn command(args: &[String]) -> Result<Value> {
  let arguments = Arguments::parse(args, &["classes", "limit", "max-bytes", "root"], &["trash"])?;
  if arguments.positionals().len() != 1 {
    return Err(invalid(USAGE));
  }
  let limit = arguments.count("limit", 100, 2_048)?;
  let max_bytes = page::max_bytes(&arguments)?;
  let root = project_directory(arguments.root())?;
  let declaration = arguments.value("classes").unwrap_or(DECLARATION);
  let trash = arguments.flag("trash");
  let result = sweep(&root, declaration, trash)?;
  let total = result.items.len();
  let status = if result.failed.items == 0 {
    "ready"
  } else {
    "failed"
  };
  page::fill(
    result.items.iter().take(limit),
    max_bytes,
    |items| {
      json!({
        "command": "residue",
        "modelCalls": 0,
        "mode": if trash { "trash" } else { "report" },
        "status": status,
        "declaration": declaration,
        "classes": result.classes,
        "moved": result.moved,
        "failed": result.failed,
        "totalItems": total,
        "items": items,
        "omittedItems": total - items.len(),
      })
    },
    |_| HiveXError::new("OUTPUT_LIMIT", "The next residue item exceeds --max-bytes"),
  )
}
