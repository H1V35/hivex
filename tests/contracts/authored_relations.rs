use crate::support::Project;
use serde_json::json;
use std::fmt::Write;
use std::fs;

fn connected() -> Project {
  let project = Project::new();
  project.write("a.md","# A\n\n## Relationships\n- Depends on [B](b.md#policy): respect retention\n- Exception to [Local](#a): limited case\n");
  project.write(
    "b.md",
    "# B\n\n## Policy\nKeep pending work.\n\n## Other\nOther rule.\n",
  );
  project
}

#[test]
fn navigation_preserves_direction_ranges_and_works_without_graph_or_model() {
  let p = connected();
  p.write("c.md", "# C\n");
  p.write("b.md", "# B\n\n## Policy\nKeep pending work.\n\n## Other\nOther rule.\n\n## Relationships\n- Extends [C](c.md): limited addition\n");
  p.write(".hivex/knowledge.sqlite", "invalid sqlite must not be read");
  p.write(".hivex/graph.json", "invalid graph must not be read");
  let first = p.raw(&["relations", "a.md", "--direction", "outgoing"]);
  let second = p.raw(&["relations", "a.md", "--direction", "outgoing"]);
  assert!(first.status.success());
  assert_eq!(first.stdout, second.stdout);
  let output: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
  assert_eq!(output["modelCalls"], 0);
  assert_eq!(output["relations"][0]["kind"], "depends-on");
  assert_eq!(output["relations"][0]["from"]["document"], "a.md");
  assert_eq!(output["relations"][0]["to"]["lineStart"], 3);
  assert_eq!(output["relations"][0]["to"]["lineEnd"], 5);
  let incoming = p.ok(&["relations", "b.md", "--direction", "incoming"]);
  assert_eq!(incoming["totalRelations"], 1);
  assert_eq!(incoming["relations"][0]["from"]["document"], "a.md");
  assert_eq!(incoming["relations"][0]["to"]["document"], "b.md");
  assert_eq!(incoming["relations"][0]["navigation"], "incoming");
  let both = p.ok(&["relations", "a.md"]);
  assert_eq!(both["totalRelations"], 2);
  assert_eq!(both["relations"][1]["navigation"], "self");
  assert_eq!(
    p.ok(&["relations", "a.md", "--direction", "outgoing"])["totalRelations"],
    2
  );
  assert_eq!(
    fs::read_to_string(p.path(".hivex/knowledge.sqlite")).unwrap(),
    "invalid sqlite must not be read"
  );
}

#[test]
fn malformed_declarations_and_ambiguous_anchors_are_diagnostics() {
  let p = connected();
  for entry in [
    "- Requires [B](b.md): reason",
    "- depends on [B](b.md): reason",
    "- **Depends on** [B](b.md): reason",
    "- Depends on [B](b.md): ",
    "- Depends on [B](b.md): [extra](a.md)",
    "free prose",
    "- Depends on [B](b.md): reason\n- Depends on [B](./b.md): reason",
    "- Depends on [B](b.md): reason\n## Relationships\n",
  ] {
    p.write("a.md", format!("# A\n## Relationships\n{entry}\n"));
    let error = p.error(&["relations", "a.md", "--direction", "outgoing"]);
    assert_eq!(error["error"]["code"], "INVALID_RELATION");
  }
  p.write(
    "a.md",
    "## Relationships\n- Depends on [B](b.md#policy): reason\n",
  );
  p.write("b.md", "# B\n## Policy\n<a id=\"policy\"></a>\n");
  assert_eq!(
    p.error(&["relations", "a.md", "--direction", "outgoing"])["error"]["code"],
    "AMBIGUOUS_ANCHOR"
  );
  p.write("b.md", "## Policy\nFirst\n## Policy\nSecond\n");
  p.write(
    "a.md",
    "## Relationships\n- Depends on [B](b.md#policy-1): reason\n",
  );
  assert_eq!(
    p.ok(&["relations", "a.md", "--direction", "outgoing"])["relations"][0]["to"]["lineStart"],
    3
  );
  p.write(
    "a.md",
    "## **Relationships**\n- Depends on [B](b.md): reason\n",
  );
  assert_eq!(
    p.error(&["relations", "a.md", "--direction", "outgoing"])["error"]["details"]["line"],
    1
  );
}

#[test]
fn targets_do_not_escape_selection_and_history_is_explicit() {
  let p = connected();
  for target in [
    "../outside.md",
    "https://example.test/a.md",
    "b.md#missing",
    "b.md?query",
    "%2Fetc/a.md",
  ] {
    p.write(
      "a.md",
      format!("## Relationships\n- Depends on [Target]({target}): reason\n"),
    );
    let error = p.error(&["relations", "a.md", "--direction", "outgoing"]);
    assert_eq!(error["error"]["code"], "INVALID_RELATION");
    assert_eq!(error["error"]["details"]["document"], "a.md");
    assert_eq!(error["error"]["details"]["line"], 2);
  }
  p.write(
    "history/old.md",
    "# Old\n\n## Relationships\n- Implements [B](../b.md): legacy case\n",
  );
  p.json(
    "hivex.json",
    &json!({"include":["*.md"],"archive":["history/*.md"]}),
  );
  p.write(
    "a.md",
    "## Relationships\n- Supersedes [Old](history/old.md): replaced scope\n",
  );
  let current = p.ok(&["relations", "a.md", "--direction", "outgoing"]);
  assert_eq!(current["relations"][0]["to"]["historical"], true);
  assert_eq!(
    p.ok(&["relations", "b.md", "--direction", "incoming"])["totalRelations"],
    0
  );
  assert_eq!(
    p.ok(&["relations", "history/old.md", "--direction", "outgoing"])["totalRelations"],
    1
  );
  p.json(
    "hivex.json",
    &json!({"include":["*.md"],"exclude":["b.md"]}),
  );
  p.write("a.md", "## Relationships\n- Depends on [B](b.md): reason\n");
  assert_eq!(
    p.error(&["relations", "a.md", "--direction", "outgoing"])["error"]["code"],
    "INVALID_RELATION"
  );
}

#[test]
fn pagination_binds_sources_options_and_whole_json_bytes() {
  let p = connected();
  let page = p.ok(&[
    "relations",
    "a.md",
    "--direction",
    "outgoing",
    "--limit",
    "1",
  ]);
  let cursor = page["continuation"].as_str().unwrap();
  let next = p.ok(&[
    "relations",
    "a.md",
    "--direction",
    "outgoing",
    "--limit",
    "1",
    "--cursor",
    cursor,
  ]);
  assert_eq!(next["relations"][0]["kind"], "exception-to");
  let full = p.raw(&["relations", "a.md", "--direction", "outgoing"]);
  let bytes = full
    .stdout
    .strip_suffix(b"\n")
    .unwrap_or(&full.stdout)
    .len();
  let exact = bytes.to_string();
  let below = (bytes - 1).to_string();
  let exact_page = p.raw(&[
    "relations",
    "a.md",
    "--direction",
    "outgoing",
    "--max-bytes",
    &exact,
  ]);
  assert!(exact_page.status.success());
  assert_eq!(exact_page.stdout, full.stdout);
  let bounded_page = p.raw(&[
    "relations",
    "a.md",
    "--direction",
    "outgoing",
    "--max-bytes",
    &below,
  ]);
  assert!(bounded_page.status.success());
  assert!(bounded_page.stdout.len() - 1 < bytes);
  let bounded: serde_json::Value = serde_json::from_slice(&bounded_page.stdout).unwrap();
  assert_eq!(bounded["relations"].as_array().unwrap().len(), 1);
  assert!(bounded["continuation"].is_string());
  for options in [
    ["--direction", "incoming"],
    ["--limit", "2"],
    ["--max-bytes", "65536"],
  ] {
    let mut args = vec![
      "relations",
      "a.md",
      "--direction",
      "outgoing",
      "--limit",
      "1",
      "--cursor",
      cursor,
    ];
    if let Some(index) = args.iter().position(|argument| *argument == options[0]) {
      args[index + 1] = options[1];
    } else {
      args.extend(options);
    }
    assert_eq!(p.error(&args)["error"]["code"], "INVALID_CURSOR");
  }
  p.write("b.md", "# B\n## Policy\nChanged\n");
  assert_eq!(
    p.error(&[
      "relations",
      "a.md",
      "--direction",
      "outgoing",
      "--limit",
      "1",
      "--cursor",
      cursor
    ])["error"]["code"],
    "INVALID_CURSOR"
  );
  assert_eq!(
    p.error(&[
      "relations",
      "a.md",
      "--direction",
      "outgoing",
      "--max-bytes",
      "1"
    ])["error"]["code"],
    "OUTPUT_LIMIT"
  );
}

#[test]
fn omitted_sources_and_symlinks_never_claim_complete_navigation() {
  let p = connected();
  p.write("invalid.md", [0xff]);
  let response = p.ok(&["relations", "a.md", "--direction", "outgoing"]);
  assert_eq!(response["status"], "partial");
  assert_eq!(response["coverage"], "partial");
  assert_eq!(response["warnings"][0]["path"], "invalid.md");
  std::os::unix::fs::symlink(p.path("b.md"), p.path("alias.md")).unwrap();
  p.write(
    "a.md",
    "## Relationships\n- Depends on [Alias](alias.md): reason\n",
  );
  assert_eq!(
    p.error(&["relations", "a.md", "--direction", "outgoing"])["error"]["code"],
    "INVALID_RELATION"
  );
}

#[test]
fn comments_and_raw_html_text_do_not_create_destination_anchors() {
  let p = connected();
  p.write(
    "a.md",
    "## Relationships\n- Depends on [B](b.md#phantom): reason\n",
  );
  for hidden in [
    "<!-- <a id=\"phantom\"></a> -->",
    "<script>\n<a id=\"phantom\"></a>\n</script>",
    "<script>\n</scripture><a id=\"phantom\"></a>\n</script>",
    "<style>\n<a id=\"phantom\"></a>\n</style>",
    "<textarea><a id=\"phantom\"></a></textarea>",
    "<div title='<fake> <a id=\"phantom\"></a>'></div>",
  ] {
    p.write("b.md", format!("# B\n{hidden}\n\n<a id=\"real\"></a>\n"));
    assert_eq!(
      p.error(&["relations", "a.md", "--direction", "outgoing"])["error"]["code"],
      "INVALID_RELATION"
    );
    p.write(
      "a.md",
      "## Relationships\n- Depends on [B](b.md#real): reason\n",
    );
    assert_eq!(
      p.ok(&["relations", "a.md", "--direction", "outgoing"])["relations"][0]["to"]["anchor"],
      "real"
    );
    p.write(
      "a.md",
      "## Relationships\n- Depends on [B](b.md#phantom): reason\n",
    );
  }
}

#[test]
fn excessive_declarations_fail_explicitly_before_result_materialization() {
  let p = connected();
  let mut entries = String::new();
  for i in 0..2049 {
    writeln!(entries, "- Depends on [B](b.md): scope {i}").unwrap();
  }
  p.write("a.md", format!("## Relationships\n{entries}"));
  let error = p.error(&[
    "relations",
    "a.md",
    "--direction",
    "outgoing",
    "--limit",
    "1",
  ]);
  assert_eq!(error["error"]["code"], "RELATION_LIMIT");
  assert_eq!(error["error"]["details"]["line"], 2050);
  let first = entries.lines().take(1025).collect::<Vec<_>>().join("\n");
  let second = entries.lines().skip(1025).collect::<Vec<_>>().join("\n");
  p.write("a.md", format!("## Relationships\n{first}\n"));
  p.write("c.md", format!("## Relationships\n{second}\n"));
  assert_eq!(
    p.error(&[
      "relations",
      "b.md",
      "--direction",
      "incoming",
      "--limit",
      "1"
    ])["error"]["code"],
    "RELATION_LIMIT"
  );
}
