use crate::support::Project;
use std::fs;

#[test]
fn retired_commands_explain_migration_without_touching_retained_state() {
  let p = Project::new();
  p.write(
    ".hivex/knowledge.sqlite",
    "pending work and its consumed allowance",
  );
  p.write(".hivex/graph.json", "uncertain historical graph");
  for command in [
    "update",
    "ask",
    "neighbors",
    "review",
    "warnings",
    "snapshot",
    "recover",
    "prune",
    "status",
  ] {
    assert_eq!(p.error(&[command])["error"]["code"], "COMMAND_RETIRED");
  }
  assert_eq!(
    fs::read_to_string(p.path(".hivex/knowledge.sqlite")).unwrap(),
    "pending work and its consumed allowance"
  );
  assert_eq!(
    fs::read_to_string(p.path(".hivex/graph.json")).unwrap(),
    "uncertain historical graph"
  );
}
