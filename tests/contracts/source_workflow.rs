use crate::support::Project;
use serde_json::json;
use std::time::Instant;

#[test]
fn bounded_source_workflow_recovers_conditions_exceptions_history_and_uncertainty() {
  let p = Project::new();
  p.write("policy.md","# Retention\nCached records expire after seven days.\nPrivate records cannot survive access revocation.\n");
  p.write("procedure.md","# Cleanup\nApply the retention rule before deleting cache records.\n\n## Relationships\n- Implements [Retention](policy.md): enforce expiry and revocation together.\n");
  p.write("emergency.md","# Emergency\nExtend expiry for public incident records only; private access revocation still applies.\n\n## Relationships\n- Exception to [Retention](policy.md): only public incident records may remain longer.\n");
  p.write("feature.md","---\nstatus: accepted\n---\n# Future search\nImplementation is not started and requires separate owner GO.\n\n## Relationships\n- Depends on [Cleanup](procedure.md): cache cleanup must apply current retention.\n");
  p.write(
    "archive/old.md",
    "# Former retention\nRecords expired after thirty days.\n",
  );
  p.write("replacement.md","# Replacement\nSeven days replaces the former default; privacy conditions remain live.\n\n## Relationships\n- Supersedes [Former rule](archive/old.md): replaces default expiry only.\n");
  p.json("hivex.json", &json!({"archive":["archive/*.md"]}));
  assert_eq!(p.ok(&["check"])["status"], "ready");
  let started = Instant::now();
  let results = [
    p.ok(&["search", "revocation", "--source", "policy.md"]),
    p.ok(&["relations", "feature.md", "--direction", "outgoing"]),
    p.ok(&["relations", "procedure.md", "--direction", "outgoing"]),
    p.ok(&["relations", "policy.md", "--direction", "incoming"]),
    p.ok(&["read", "emergency.md"]),
    p.ok(&["read", "feature.md"]),
    p.ok(&["relations", "replacement.md", "--direction", "outgoing"]),
    p.ok(&["read", "archive/old.md"]),
    p.ok(&["search", "unrecordedquasarpolicy"]),
  ];
  assert!(
    results[0]["matches"][0]["text"]
      .as_str()
      .unwrap()
      .contains("cannot survive access revocation")
  );
  assert_eq!(results[1]["relations"][0]["to"]["document"], "procedure.md");
  assert_eq!(results[2]["relations"][0]["to"]["document"], "policy.md");
  assert!(
    results[3]["relations"]
      .as_array()
      .unwrap()
      .iter()
      .any(|edge| edge["kind"] == "exception-to")
  );
  assert!(
    results[4]["text"]
      .as_str()
      .unwrap()
      .contains("public incident records only")
  );
  assert!(results[5]["text"].as_str().unwrap().contains("not started"));
  assert_eq!(results[6]["relations"][0]["to"]["historical"], true);
  assert_eq!(results[7]["source"]["historical"], true);
  assert_eq!(results[8]["totalMatches"], 0);
  assert!(
    results
      .iter()
      .filter_map(|value| value.get("modelCalls"))
      .all(|calls| calls == 0)
  );
  let context_bytes: usize = results.iter().map(|value| value.to_string().len()).sum();
  println!(
    "source workflow: 9 CLI operations, {context_bytes} JSON context bytes, {} ms, 0 model calls; tests verify evidence exposure, not semantic reasoning",
    started.elapsed().as_millis()
  );
  p.write(
    "policy.md",
    "# Retention\nCurrent expiry is now five days.\n",
  );
  let changed = p.ok(&["search", "expiry", "--source", "policy.md"]);
  assert_ne!(
    changed["matches"][0]["version"],
    results[0]["matches"][0]["version"]
  );
  assert!(
    changed["matches"][0]["text"]
      .as_str()
      .unwrap()
      .contains("five days")
  );
}
