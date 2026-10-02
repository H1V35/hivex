use crate::support::Project;
use serde_json::{Value, json};
use std::fmt::Write;
use std::fs;

fn report(project: &Project, args: &[&str]) -> (i32, Value) {
  let result = project.raw(args);
  (
    result.status.code().unwrap(),
    serde_json::from_slice(&result.stdout).unwrap(),
  )
}

#[test]
fn structural_validation_finds_broken_references_and_respects_history() {
  let p = Project::new();
  p.write(
    "a.md",
    "---\r\ntitle: Authority\r\n---\r\n# A\r\n[Broken](b.md#missing)\r\n",
  );
  p.write("b.md", "# B\n## Policy\nActual rule.\n");
  p.write(
    "archive/old.md",
    "## Relationships\nPrior unsupported prose.\n",
  );
  p.json("hivex.json", &json!({"archive":["archive/*.md"]}));
  let (code, result) = report(&p, &["check"]);
  assert_eq!(code, 1);
  assert_eq!(result["status"], "failed");
  assert_eq!(result["findings"][0]["document"], "a.md");
  assert_eq!(result["findings"][0]["line"], 5);
  p.write("a.md", "# A\n[Policy](b.md#policy)\n");
  assert_eq!(p.ok(&["check"])["status"], "ready");
  assert_eq!(report(&p, &["check", "--historical"]).1["status"], "failed");
  assert_eq!(p.ok(&["check", "--source", "b.md"])["checkedDocuments"], 1);
  assert_eq!(
    p.error(&["check", "--source", "unknown.md"])["error"]["code"],
    "SOURCE_NOT_FOUND"
  );
}

#[test]
fn validation_rejects_formal_grammar_escapes_and_inaccessible_targets() {
  let p = Project::new();
  p.write("b.md", "# B\n");
  for entry in [
    "## Relationships\n- Requires [B](b.md): reason\n",
    "[Outside](../outside.md)",
    "[Missing](unknown.md)",
    "[Encoded missing](unknown%2Emd)",
    "[Absolute](%2Fb.md)",
  ] {
    p.write("a.md", entry);
    let (code, result) = report(&p, &["check", "--source", "a.md"]);
    assert_eq!(code, 1);
    assert_ne!(result["findings"].as_array().unwrap().len(), 0);
  }
  p.write(
    "a.md",
    "# A\n[External](https://example.test/guide.md)\n[CDN](//cdn.example/guide.md)\n```md\n[Not a link](missing.md)\n```\n",
  );
  assert_eq!(p.ok(&["check"])["status"], "ready");
}

#[test]
fn validation_continuations_and_omissions_never_report_a_false_success() {
  let p = Project::new();
  p.write("a.md", "[One](one.md)\n[Two](two.md)\n");
  let (code, first) = report(&p, &["check", "--limit", "1"]);
  assert_eq!(code, 1);
  let cursor = first["continuation"].as_str().unwrap();
  let (_, second) = report(&p, &["check", "--limit", "1", "--cursor", cursor]);
  assert_eq!(second["totalFindings"], 2);
  assert!(second["continuation"].is_null());
  assert_ne!(
    first["findings"][0]["target"],
    second["findings"][0]["target"]
  );
  assert_eq!(
    p.error(&["check", "--limit", "2", "--cursor", cursor])["error"]["code"],
    "INVALID_CURSOR"
  );
  assert_eq!(
    p.error(&["check", "--max-bytes", "1"])["error"]["code"],
    "OUTPUT_LIMIT"
  );
  p.write("a.md", "# Valid current source\n");
  p.write("invalid.md", [0xff]);
  assert_eq!(
    p.error(&["check", "--limit", "1", "--cursor", cursor])["error"]["code"],
    "INVALID_CURSOR"
  );
  assert_eq!(p.ok(&["check"])["status"], "partial");
  assert_eq!(p.ok(&["check"])["coverage"], "partial");
  p.write(".hivex/knowledge.sqlite", "unchanged legacy bytes");
  p.ok(&["check"]);
  assert_eq!(
    fs::read_to_string(p.path(".hivex/knowledge.sqlite")).unwrap(),
    "unchanged legacy bytes"
  );
}

#[test]
fn validation_handles_large_blank_sources_and_rejects_excessive_anchor_records() {
  let p = Project::new();
  p.write("blank.md", "\n".repeat(16 * 1024 * 1024));
  assert_eq!(p.ok(&["check"])["status"], "ready");
  p.write("blank.md", "## Heading\n".repeat(32769));
  let (code, result) = report(&p, &["check"]);
  assert_eq!(code, 1);
  assert_eq!(result["findings"][0]["code"], "ANCHOR_LIMIT");
}

#[test]
fn repeated_anchor_references_share_bounded_query_local_coordinates() {
  let p = Project::new();
  p.write(
    "target.md",
    format!("# Policy\n{}", "\n".repeat(16 * 1024 * 1024)),
  );
  p.write(
    "source.md",
    "[Policy](target.md?view=raw#policy)\n".repeat(512),
  );
  assert_eq!(p.ok(&["check", "--source", "source.md"])["status"], "ready");
}

#[test]
fn anchor_reuse_has_an_explicit_query_wide_bound() {
  let p = Project::new();
  let mut links = String::new();
  for index in 0..4 {
    let file = format!("target{index}.md");
    p.write(&file, "## Heading\n".repeat(20000));
    writeln!(links, "[Target]({file}#heading)").unwrap();
  }
  p.write("source.md", links);
  let (code, result) = report(&p, &["check", "--source", "source.md"]);
  assert_eq!(code, 1);
  assert_eq!(result["findings"][0]["code"], "ANCHOR_LIMIT");
}

#[test]
fn query_components_and_question_marks_in_fragments_have_distinct_meanings() {
  let p = Project::new();
  p.write("target.md", "# Target\n<a id=\"foo?bar\"></a>\n");
  p.write("source.md","# Source\n[Exact](target.md?view=raw#foo?bar)\n[Plain](target.md#foo?bar)\n\n## Relationships\n- Extends [Target](target.md#foo?bar): the fragment is exact; no query component is authored.\n");
  assert_eq!(p.ok(&["check"])["status"], "ready");
  assert_eq!(
    p.ok(&["relations", "source.md", "--direction", "outgoing"])["relations"][0]["to"]["anchor"],
    "foo?bar"
  );
}

#[test]
fn metadata_scope_preserves_native_headers_and_frozen_historical_bodies() {
  let p = Project::new();
  p.write("AGENTS.md", "# Agent entrypoint\n");
  p.write(
    "skills/example/SKILL.md",
    "---\nname: example\ndescription: Native skill\n---\n",
  );
  p.write("module/docs/current.md", "\u{feff}---\r\n\"title\": &name Policy\r\nstatus: accepted # current\r\nimplementation: not-started\r\ncreated_at: 2024-02-29\r\ntags:\r\n  - &topic policy\r\n  - source-reading\r\nsource: *name\r\n---\r\n# Policy\r\n");
  let historical = "---\ntitle: Original\nstatus: historical\ncreated_at: 2020-01-01\narchived_at: 2026-10-02\ntags: [history]\n---\n# Capture\n\n---\nold_field: preserved\n---\n[Old reference](missing.md)\n## Relationships\nUnsupported original prose.\n";
  p.write("docs/archive/capture.md", historical);
  p.json("hivex.json", &json!({"archive":["docs/archive/*.md"]}));
  let result = p.ok(&["check"]);
  assert_eq!(result["checkedMetadataDocuments"], 2);
  assert_eq!(result["checkedReferenceDocuments"], 3);
  assert_eq!(result["checkedDocuments"], 4);
  assert_eq!(report(&p, &["check", "--historical"]).1["status"], "failed");
  assert_eq!(
    fs::read_to_string(p.path("docs/archive/capture.md")).unwrap(),
    historical
  );
  p.write(
    "docs/archive/capture.md",
    historical.replace("status: historical", "status: Historical"),
  );
  let (_, result) = report(&p, &["check"]);
  assert_eq!(result["findings"][0]["field"], "status");
  p.write(
    "AGENTS.md",
    "## Relationships\n- Requires [Policy](module/docs/current.md): invalid literal.\n",
  );
  assert_eq!(
    report(&p, &["check", "--source", "AGENTS.md"]).1["status"],
    "failed"
  );
}

#[test]
fn metadata_reports_schema_errors_without_disabling_legacy_reading() {
  let p = Project::new();
  let valid = "title: Policy\nstatus: accepted\nimplementation: implemented\ncreated_at: 2024-02-29\nupdated_at: 2026-10-02\ntags: [policy, source-reading]\nsource: https://example.test/original\n";
  for (old, new, field) in [
    ("title: Policy", "title: 42", "title"),
    ("status: accepted", "status: Accepted", "status"),
    (
      "implementation: implemented",
      "implementation: complete",
      "implementation",
    ),
    (
      "created_at: 2024-02-29",
      "created_at: 2023-02-29",
      "created_at",
    ),
    (
      "updated_at: 2026-10-02",
      "updated_at: 2026-13-01",
      "updated_at",
    ),
    ("tags: [policy, source-reading]", "tags: []", "tags"),
    (
      "tags: [policy, source-reading]",
      "tags: [policy, policy]",
      "tags",
    ),
    (
      "tags: [policy, source-reading]",
      "tags: [Policy, source_reading]",
      "tags",
    ),
    (
      "source: https://example.test/original",
      "source: true",
      "source",
    ),
    (
      "status: accepted",
      "status: accepted\nstatus: draft",
      "status",
    ),
    (
      "title: Policy\nstatus: accepted",
      "status: accepted\ntitle: Policy",
      "title",
    ),
    ("created_at: 2024-02-29\n", "", "created_at"),
  ] {
    p.write(
      "docs/policy.md",
      format!("---\n{}---\n# Policy\n", valid.replace(old, new)),
    );
    let (code, result) = report(&p, &["check"]);
    assert_eq!(code, 1, "{new}");
    assert_eq!(result["findings"][0]["code"], "INVALID_METADATA");
    assert_eq!(result["findings"][0]["field"], field, "{new}");
    assert_eq!(
      p.ok(&["read", "docs/policy.md"])["source"]["id"],
      "docs/policy.md"
    );
  }
  for header in [
    "",
    "---\ntitle: [\n---\n",
    "---\ndelivery: implemented\n---\n",
    "---\ntags: [[policy]]\n---\n",
  ] {
    p.write("docs/policy.md", format!("{header}# Policy\n"));
    assert_eq!(report(&p, &["check"]).1["status"], "failed");
  }
}

#[test]
fn metadata_findings_are_located_bounded_and_snapshot_paginated() {
  let p = Project::new();
  p.write("docs/policy.md", "\u{feff}---\r\ntitle: Policy\r\nstatus: Accepted\r\ncreated_at: 2024-02-30\r\ntags: [Policy]\r\n---\r\n# Policy\r\n");
  let (_, first) = report(&p, &["check", "--limit", "1"]);
  assert_eq!(first["totalFindings"], 3);
  assert_eq!(first["findings"][0]["line"], 3);
  assert_eq!(first["findings"][0]["column"], 1);
  let cursor = first["continuation"].as_str().unwrap();
  let (_, second) = report(&p, &["check", "--limit", "1", "--cursor", cursor]);
  assert_eq!(second["findings"][0]["field"], "created_at");
  let old_cursor = cursor.replacen("v2.", "v1.", 1);
  assert_eq!(
    p.error(&["check", "--limit", "1", "--cursor", &old_cursor])["error"]["code"],
    "INVALID_CURSOR"
  );
  p.write(
    "docs/policy.md",
    format!("---\n# {}\n---\n", "x".repeat(65536)),
  );
  assert_eq!(
    report(&p, &["check"]).1["findings"][0]["message"],
    "Outer metadata exceeds 65536 bytes"
  );
}
