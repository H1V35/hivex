use crate::support::Project;
use serde_json::json;
use std::fs;

#[test]
fn search_returns_reproducible_source_passages_without_legacy_state() {
  let p = Project::new();
  let mut source = String::from("# Autonomy\r\n");
  source.push_str(&"Ordinary line.\r\n".repeat(31));
  source.push_str("La documentación conserva autonomía y contexto histórico.\r\nException: only explicit GO permits implementation.\r\n");
  p.write("authority.md", &source);
  p.write(".hivex/knowledge.sqlite", "invalid retained SQLite");
  p.write(".hivex/graph.json", "invalid retained graph");
  let first = p.raw(&["search", "autonomía"]);
  let second = p.raw(&["search", "autonomía"]);
  assert!(first.status.success());
  assert_eq!(first.stdout, second.stdout);
  let result: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
  assert_eq!(result["modelCalls"], 0);
  let hit = &result["matches"][0];
  assert_eq!(hit["document"], "authority.md");
  assert_eq!(hit["lineStart"], 33);
  assert!(hit["text"].as_str().unwrap().contains("Exception:"));
  let from = hit["lineStart"].to_string();
  let to = hit["lineEnd"].to_string();
  let read = p.ok(&["read", "authority.md", "--from", &from, "--to", &to]);
  assert_eq!(read["text"], hit["text"]);
  assert_eq!(read["source"]["hash"], hit["version"]);
  assert_eq!(
    fs::read_to_string(p.path(".hivex/knowledge.sqlite")).unwrap(),
    "invalid retained SQLite"
  );
}

#[test]
fn history_scope_and_source_omissions_are_explicit() {
  let p = Project::new();
  p.write("current.md", "# Current\nOrdinary current policy.\n");
  p.write(
    "archive/old.md",
    "# Retired\nThe amber rule was replaced.\n",
  );
  p.json("hivex.json", &json!({"archive":["archive/*.md"]}));
  assert_eq!(p.ok(&["search", "amber"])["totalMatches"], 0);
  let historical = p.ok(&["search", "amber", "--historical"]);
  assert_eq!(historical["matches"][0]["historical"], true);
  assert_eq!(
    p.ok(&["search", "amber", "--source", "archive/old.md"])["totalMatches"],
    1
  );
  assert_eq!(
    p.error(&["search", "amber", "--source", "missing.md"])["error"]["code"],
    "SOURCE_NOT_FOUND"
  );
  p.write("invalid.md", [0xff]);
  let partial = p.ok(&["search", "policy"]);
  assert_eq!(partial["status"], "partial");
  assert_eq!(partial["coverage"], "partial");
  assert_eq!(partial["warnings"][0]["path"], "invalid.md");
}

#[test]
fn source_search_pages_bind_options_versions_and_json_byte_budget() {
  let p = Project::new();
  p.write("a.md", "# A\nAmber policy.\n");
  p.write("b.md", "# B\nAmber policy.\n");
  let first = p.ok(&["search", "amber", "--limit", "1"]);
  let cursor = first["continuation"].as_str().unwrap();
  let second = p.ok(&["search", "amber", "--limit", "1", "--cursor", cursor]);
  assert_ne!(
    first["matches"][0]["document"],
    second["matches"][0]["document"]
  );
  assert!(second["continuation"].is_null());
  assert_eq!(
    p.error(&["search", "amber", "--limit", "2", "--cursor", cursor])["error"]["code"],
    "INVALID_CURSOR"
  );
  assert_eq!(
    p.error(&["search", "changed", "--limit", "1", "--cursor", cursor])["error"]["code"],
    "INVALID_CURSOR"
  );
  let full = p.raw(&["search", "amber"]);
  let bytes = (full.stdout.len() - 1).to_string();
  assert_eq!(
    p.raw(&["search", "amber", "--max-bytes", &bytes]).stdout,
    full.stdout
  );
  assert_eq!(
    p.error(&["search", "amber", "--max-bytes", "1"])["error"]["code"],
    "OUTPUT_LIMIT"
  );
  p.write("a.md", "# A\nChanged source.\n");
  assert_eq!(
    p.error(&["search", "amber", "--limit", "1", "--cursor", cursor])["error"]["code"],
    "INVALID_CURSOR"
  );
}

#[test]
fn many_short_or_blank_lines_use_bounded_search_windows() {
  let p = Project::new();
  p.write("blank.md", "\n".repeat(16 * 1024 * 1024));
  assert_eq!(p.ok(&["search", "missing"])["totalMatches"], 0);
  p.write("blank.md", "amber\n".repeat(32 * 32769));
  assert_eq!(
    p.error(&["search", "amber"])["error"]["code"],
    "SEARCH_LIMIT"
  );
}

#[test]
fn a_document_title_does_not_make_every_window_a_match() {
  let p = Project::new();
  p.write(
    "title.md",
    format!(
      "---\ntitle: Retention\n---\n{}",
      "Other unrelated prose.\n".repeat(130)
    ),
  );
  let result = p.ok(&["search", "retention"]);
  assert_eq!(result["totalMatches"], 1);
  assert!(
    result["matches"][0]["text"]
      .as_str()
      .unwrap()
      .contains("Retention")
  );
}
