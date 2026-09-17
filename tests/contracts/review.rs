use crate::support::{Project, decision, list, subset};
use serde_json::{Value, json};
use std::fs;

const BEFORE: &str = "pub const PURGE_ON_REVOCATION: bool = true;\n";
const AFTER: &str = "pub const PURGE_ON_REVOCATION: bool = false;\n";
fn finding(path: &str, line: u64) -> Value {
  json!({"assessment":"conflict","code":[{"path":path,"lineStart":line,"lineEnd":line,"side":"after"}],"documents":[{"document":"privacy.md","lineStart":3,"lineEnd":3}],"explanation":"Private cache survives revocation, contradicting the purge rule."})
}
fn setup(p: &Project) {
  p.write(
    ".gitignore",
    ".hivex/\nresponses.json*\ncodex\ncalls.log\npids.json\nreport.json\n",
  );
  p.write("cache.rs", BEFORE);
  p.git_init();
  p.git(&["add", "."]);
  p.git(&["commit", "-qm", "Code baseline"]);
  p.write("cache.rs", AFTER);
  let mut r = p.read_json("responses.json");
  r["review"] = json!({"findings":[finding("cache.rs",1)],"uncertainties":[]});
  p.json("responses.json", &r);
}

#[test]
fn review_shares_budget_cites_original_code_and_detects_code_and_document_changes() {
  let p = Project::policy();
  setup(&p);
  let first = p.model_cli(&[
    "review",
    "change cache behavior",
    "--base",
    "HEAD",
    "--max-calls",
    "2",
  ]);
  assert_eq!(first["status"], "budget-exhausted");
  let result = p.model_cli(&[
    "review",
    "change cache behavior",
    "--base",
    "HEAD",
    "--max-calls",
    "3",
  ]);
  subset(&result, &json!({"status":"ready","work":{"calls":3}}));
  assert_eq!(result["work"]["id"], first["work"]["id"]);
  subset(
    &result["findings"][0],
    &json!({"assessment":"conflict","code":[{"path":"cache.rs","text":AFTER.trim_end()}],"documents":[{"document":"privacy.md","text":"Revoking access immediately removes cached private data."}]}),
  );
  p.json("report.json", &result);
  assert_eq!(
    p.ok(&["review", "--check", "report.json"])["status"],
    "current"
  );
  let reused = p.cli(&[
    "review",
    "change cache behavior",
    "--base",
    "HEAD",
    "--max-calls",
    "0",
    "--codex",
    "/no-model",
  ]);
  assert_eq!(reused["work"]["calls"], 3);
  assert_eq!(p.calls(), 3);
  p.write("cache.rs", BEFORE);
  subset(
    &p.ok(&["review", "--check", "report.json"]),
    &json!({"status":"stale","documentsChanged":false,"implementationChanged":true}),
  );
  p.write(
    "privacy.md",
    "\u{feff}# Access\n\nRevoking access immediately removes cached private data.\n",
  );
  assert_eq!(
    p.ok(&["review", "--check", "report.json"])["documentsChanged"],
    true
  );
}

#[test]
fn review_keeps_historical_provenance_and_invalid_findings_local() {
  let p = Project::policy();
  p.write(
    "archive/replaced.md",
    "# Replaced\n\nThe old rule required immediate purge.\n",
  );
  p.json(
    "hivex.json",
    &json!({"archive":["archive/**/*.md"],"include":["*.md"]}),
  );
  setup(&p);
  let mut r = p.read_json("responses.json");
  r["byDocument"] = json!({"archive/replaced.md":{"decisions":[decision("archive/replaced.md","historical-purge",3,"The old rule required immediate purge.")],"relationships":[]}});
  r["review"]["findings"][0]["documents"] =
    json!([{"document":"archive/replaced.md","lineStart":3,"lineEnd":3}]);
  p.json("responses.json", &r);
  let result = p.model_cli(&[
    "review",
    "historical cache behavior",
    "--base",
    "HEAD",
    "--source",
    "archive/replaced.md",
  ]);
  assert_eq!(result["status"], "ready");
  assert_eq!(result["findings"][0]["documents"][0]["historical"], true);
  let p = Project::policy();
  setup(&p);
  let mut r = p.read_json("responses.json");
  let mut supported = finding("cache.rs", 1);
  supported["assessment"] = json!("exception");
  r["review"] = json!({"findings":[finding("cache.rs",999),supported],"uncertainties":["Deployment size is not documented."]});
  p.json("responses.json", &r);
  let result = p.model_cli(&["review", "cache", "--base", "HEAD"]);
  assert_eq!(result["status"], "partial");
  subset(
    &result["findings"][0],
    &json!({"assessment":"uncertain","code":[],"referencesVerified":false}),
  );
  subset(
    &result["findings"][1],
    &json!({"assessment":"exception","referencesVerified":true}),
  );
  assert_eq!(
    result["uncertainties"],
    json!(["Deployment size is not documented."])
  );
}

#[test]
fn review_binds_deleted_and_untracked_files_and_never_increases_omitted_limit() {
  let p = Project::policy();
  setup(&p);
  fs::remove_file(p.path("cache.rs")).unwrap();
  p.write("new cache.rs", AFTER);
  let mut r = p.read_json("responses.json");
  r["review"]["findings"][0]["code"] = json!([{"path":"cache.rs","lineStart":1,"lineEnd":1,"side":"before"},{"path":"new cache.rs","lineStart":1,"lineEnd":1,"side":"after"}]);
  p.json("responses.json", &r);
  let first = p.model_cli(&["review", "cache", "--base", "HEAD", "--max-calls", "1"]);
  let held = p.model_cli(&["review", "cache", "--base", "HEAD"]);
  assert_eq!(held["work"]["id"], first["work"]["id"]);
  assert_eq!(held["work"]["maxCalls"], 1);
  assert_eq!(held["work"]["calls"], 1);
  let done = p.model_cli(&["review", "cache", "--base", "HEAD", "--max-calls", "3"]);
  assert_eq!(done["status"], "ready");
  subset(
    &done["findings"][0]["code"],
    &json!([{"path":"cache.rs","side":"before","text":BEFORE.trim_end()},{"path":"new cache.rs","side":"after","text":AFTER.trim_end()}]),
  );
  p.json("report.json", &done);
  p.write("new cache.rs", format!("\u{feff}{AFTER}"));
  assert_eq!(
    p.ok(&["review", "--check", "report.json"])["implementationChanged"],
    true
  );
}

#[test]
fn review_bounds_oversized_and_binary_implementation_before_spending() {
  let p = Project::policy();
  setup(&p);
  p.write("cache.rs", "x".repeat(262_145));
  assert_eq!(
    p.error(&[
      "review",
      "cache",
      "--base",
      "HEAD",
      "--codex",
      p.path("codex").to_str().unwrap()
    ])["error"]["code"],
    "IMPLEMENTATION_TOO_LARGE"
  );
  assert_eq!(p.calls(), 0);
  p.write("cache.rs", AFTER);
  p.write("asset.bin", [0, 1, 2]);
  let result = p.model_cli(&["review", "cache", "--base", "HEAD"]);
  subset(&result, &json!({"status":"partial","work":{"calls":3}}));
  assert!(
    result["warnings"]
      .to_string()
      .contains("Unsupported binary")
  );
  let p = Project::policy();
  p.write("asset.bin", vec![0; 200 * 1024]);
  setup(&p);
  p.write("asset.bin", vec![255; 200 * 1024]);
  let result = p.model_cli(&["review", "cache", "--base", "HEAD"]);
  assert_eq!(result["status"], "partial");
  let text = result.to_string();
  for hash in [
    "13f85ed26dc953b0410f9b1ab4ada10cc9f1719924804a2662cd46f8977e76e0",
    "1b49c45eb2cce0c9af787939a85d848590b8383da07333bf8ecc56d57b5dfd75",
  ] {
    assert!(text.contains(hash));
  }
  assert!(text.len() < 256 * 1024);
}

#[test]
fn review_context_expansion_preserves_work_budget_and_graph_quotes() {
  let p = Project::policy();
  p.write(
    "cache.md",
    format!(
      "# Cache\n\nCached data expires after seven days.\n\n{}\n",
      "Supporting rationale. ".repeat(270)
    ),
  );
  setup(&p);
  assert_eq!(p.model_cli(&["update"])["status"], "ready");
  let first = p.model_cli(&[
    "review",
    "cache",
    "--base",
    "HEAD",
    "--max-calls",
    "1",
    "--max-context-bytes",
    "5000",
  ]);
  subset(
    &first,
    &json!({"status":"partial","omittedUnits":1,"work":{"calls":1}}),
  );
  let held = p.model_cli(&[
    "review",
    "cache",
    "--base",
    "HEAD",
    "--max-calls",
    "1",
    "--max-context-bytes",
    "30000",
  ]);
  assert_eq!(held["status"], "budget-exhausted");
  assert_eq!(held["work"]["id"], first["work"]["id"]);
  let done = p.model_cli(&[
    "review",
    "cache",
    "--base",
    "HEAD",
    "--max-calls",
    "2",
    "--max-context-bytes",
    "30000",
  ]);
  subset(
    &done,
    &json!({"status":"ready","omittedUnits":0,"work":{"calls":2}}),
  );
  assert_eq!(done["work"]["id"], first["work"]["id"]);
  let p = Project::policy();
  p.write(
    "cache.md",
    format!(
      "# Cache\n\nCached data expires after seven days.\n\n{}\n",
      "Supporting rationale. ".repeat(270)
    ),
  );
  setup(&p);
  let mut r = p.read_json("responses.json");
  r["extract"]["decisions"][0]["lineEnd"] = json!(4);
  r["ask"]["evidence"] = json!([{"document":"cache.md","lineStart":3,"lineEnd":4}]);
  let mut good = finding("cache.rs", 1);
  good["documents"] = r["ask"]["evidence"].clone();
  let mut bad = good.clone();
  bad["documents"] = json!([{"document":"cache.md","lineStart":5,"lineEnd":5}]);
  r["review"]["findings"] = json!([good, bad]);
  p.json("responses.json", &r);
  assert_eq!(p.model_cli(&["update"])["status"], "ready");
  let answer = p.model_cli(&["ask", "cache", "--max-context-bytes", "5000"]);
  assert_eq!(answer["omittedUnits"], 1);
  assert_eq!(
    answer["evidence"][0]["text"],
    "Cached data expires after seven days.\n"
  );
  let review = p.model_cli(&[
    "review",
    "cache",
    "--base",
    "HEAD",
    "--max-context-bytes",
    "5000",
  ]);
  assert_eq!(review["findings"][0]["referencesVerified"], true);
  subset(
    &review["findings"][1],
    &json!({"assessment":"uncertain","documents":[],"referencesVerified":false}),
  );
}

#[test]
fn review_uses_new_code_terminology_and_exact_changed_ranges_of_large_files() {
  let p = Project::policy();
  setup(&p);
  p.write("cache.rs", BEFORE);
  p.write(
    "worker.rs",
    "pub const CACHE_EXPIRES_AFTER_DAYS: u32 = 90;\n",
  );
  let result = p.model_cli(&[
    "review",
    "Implement worker",
    "--base",
    "HEAD",
    "--max-calls",
    "0",
  ]);
  assert_eq!(result["status"], "budget-exhausted");
  assert!(
    list(&result, "documents")
      .iter()
      .any(|d| d["id"] == "cache.md")
  );
  let p = Project::policy();
  setup(&p);
  let context = "// Unchanged generated implementation context.\n".repeat(14000);
  p.write("large.rs", format!("{context}{BEFORE}"));
  p.git(&["add", "large.rs"]);
  p.git(&["commit", "-qm", "Large baseline"]);
  p.write("large.rs", format!("{context}{AFTER}"));
  let mut r = p.read_json("responses.json");
  r["review"]["findings"] = json!([finding("large.rs", 14001), finding("large.rs", 1)]);
  p.json("responses.json", &r);
  let result = p.model_cli(&["review", "cache purge", "--base", "HEAD"]);
  assert_eq!(result["status"], "partial");
  subset(
    &result["findings"][0],
    &json!({"referencesVerified":true,"code":[{"path":"large.rs","lineStart":14001,"text":AFTER.trim_end()}]}),
  );
  subset(
    &result["findings"][1],
    &json!({"referencesVerified":false,"code":[],"assessment":"uncertain"}),
  );
  assert!(
    result["warnings"]
      .to_string()
      .contains("unchanged code is omitted")
  );
}
