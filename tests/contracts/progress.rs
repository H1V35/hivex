use crate::support::{Project, bounded, quote, subset};
use serde_json::{Value, json};
use std::fs::{self, File};
use std::process::Output;
use std::time::{Duration, Instant};

fn run(p: &Project, args: &[&str]) -> (Value, String) {
  let Output { stdout, stderr, .. } = bounded({
    let mut command = p.command(args);
    command.arg("--codex").arg(p.path("codex"));
    command
  });
  (
    serde_json::from_slice(&stdout).unwrap(),
    String::from_utf8(stderr).unwrap(),
  )
}

#[test]
fn progress_precedes_json_and_reports_wait_without_exposing_content() {
  let p = Project::policy();
  p.write("hold", "hold");
  let wrapper = fs::read_to_string(p.path("codex")).unwrap().replace(
    "exec ",
    &format!(
      "export HIVEX_TEST_HOLD_PATH={}\nexec ",
      quote(p.path("hold").to_str().unwrap())
    ),
  );
  p.write("codex", wrapper);
  let mut child = p
    .command(&["update", "--progress", "always"])
    .arg("--codex")
    .arg(p.path("codex"))
    .stdout(File::create(p.path("stdout")).unwrap())
    .stderr(File::create(p.path("stderr")).unwrap())
    .spawn()
    .unwrap();
  let started = Instant::now();
  let observed = loop {
    let progress = fs::read_to_string(p.path("stderr")).unwrap();
    if progress
      .split_inclusive('\n')
      .any(|line| line.ends_with('\n') && line.contains("waiting for model"))
    {
      break progress;
    }
    if started.elapsed() > Duration::from_secs(25) {
      fs::remove_file(p.path("hold")).unwrap();
      let _ = child.kill();
      let _ = child.wait();
      panic!("No visible wait checkpoint: {progress}");
    }
    std::thread::sleep(Duration::from_millis(20));
  };
  let running = child.try_wait().unwrap().is_none();
  let stdout_before = fs::read(p.path("stdout")).unwrap();
  fs::remove_file(p.path("hold")).unwrap();
  let deadline = Instant::now();
  while child.try_wait().unwrap().is_none() {
    if deadline.elapsed() > Duration::from_secs(10) {
      let _ = child.kill();
      let _ = child.wait();
      panic!("Released model did not finish");
    }
    std::thread::sleep(Duration::from_millis(20));
  }
  assert!(running);
  assert_eq!(stdout_before.len(), 0);
  assert!(observed.contains("extraction: started"));
  assert!(observed.contains("internal progress unknown"));
  let result = p.read_json("stdout");
  assert_eq!(result["status"], "ready");
  assert_eq!(p.calls(), 2);
  let progress = fs::read_to_string(p.path("stderr")).unwrap();
  assert!(progress.contains("extraction finished; check pending"));
  assert!(progress.contains("admission finished"));
  assert!(progress.contains("completed=2, pending=0"));
  assert!(!progress.contains("seven days"));
  assert!(!progress.contains('\u{1b}'));
}

#[test]
fn progress_preserves_budget_resumption_cache_and_silence() {
  let p = Project::policy();
  let (first, progress) = run(&p, &["update", "--max-calls", "1", "--progress", "always"]);
  subset(
    &first,
    &json!({"status":"budget-exhausted","work":{"calls":1}}),
  );
  assert!(progress.contains("check pending"));
  assert!(progress.contains("completed=0, pending=2"));
  assert!(!progress.contains("admission finished"));
  let (second, progress) = run(&p, &["update", "--max-calls", "2", "--progress", "always"]);
  assert_eq!(second["status"], "ready");
  assert_eq!(first["work"]["id"], second["work"]["id"]);
  assert!(progress.contains("work resumed"));
  assert!(!progress.contains("extraction: started"));
  assert_eq!(p.calls(), 2);
  for mode in ["never", "auto"] {
    let (again, progress) = run(&p, &["update", "--progress", mode]);
    assert_eq!(again, second);
    assert_eq!(progress.len(), 0);
  }
  // Reset only the synthetic graph/work: preserve the cache to exercise reuse.
  p.db()
    .execute_batch("DELETE FROM work; DELETE FROM graph;")
    .unwrap();
  let (cached, progress) = run(&p, &["update", "--max-calls", "0", "--progress", "always"]);
  subset(
    &cached,
    &json!({"status":"ready","work":{"calls":0,"cacheHits":2}}),
  );
  assert!(progress.contains("cache hit; no model call"));
  assert_eq!(p.calls(), 2);
}

#[test]
fn progress_reports_interruption_and_failure_without_admission_or_retry() {
  for scenario in ["cancel", "invalid-json"] {
    let p = Project::policy();
    p.model(scenario);
    let (failed, progress) = run(&p, &["update", "--progress", "always"]);
    assert_eq!(failed["status"], "failed");
    assert!(!progress.contains("admission finished"));
    assert!(progress.contains("work retained"));
    if scenario == "cancel" {
      assert!(progress.contains("MODEL_CANCELLED"), "{progress}");
    }
    let (held, _) = run(&p, &["update", "--max-calls", "3", "--progress", "always"]);
    assert_eq!(failed["work"]["id"], held["work"]["id"]);
    assert_eq!(p.calls(), 1);
    p.model("");
    let (recovered, progress) = run(
      &p,
      &[
        "update",
        "--retry-failed",
        "--max-calls",
        "3",
        "--progress",
        "always",
      ],
    );
    assert_eq!(recovered["status"], "ready");
    assert_eq!(recovered["work"]["id"], failed["work"]["id"]);
    assert!(progress.contains("work resumed"));
    assert_eq!(p.calls(), 3);
  }
}

#[test]
fn progress_keeps_context_limits_and_rejected_checks_distinct_from_admission() {
  let p = Project::policy();
  let (limited, progress) = run(
    &p,
    &[
      "update",
      "--max-context-bytes",
      "1024",
      "--progress",
      "always",
    ],
  );
  assert_eq!(limited["status"], "context-limit");
  assert!(progress.contains("context-limit"));
  assert!(!progress.contains("admission finished"));
  assert_eq!(p.calls(), 0);
  assert_eq!(p.model_cli(&["update"])["status"], "ready");
  let graph = p.graph();
  let mut responses = p.read_json("responses.json");
  responses["extract"]["relationships"] = json!([]);
  responses["check"]["relationshipChanges"] = json!([]);
  p.json("responses.json", &responses);
  let (rejected, progress) = run(
    &p,
    &[
      "update",
      "--repair",
      "cache.md",
      "--reason",
      "Revisit the source",
      "--progress",
      "always",
    ],
  );
  assert_eq!(rejected["status"], "failed");
  assert_eq!(rejected["work"]["lastAttempt"]["code"], "RELATIONSHIP_LOSS");
  assert!(progress.contains("check failed; admission blocked"));
  assert!(!progress.contains("admission finished"));
  assert_eq!(p.graph(), graph);
}

#[test]
fn consultation_progress_includes_shared_maintenance_and_retained_answers() {
  for command in ["ask", "review"] {
    let p = Project::policy();
    p.write(
      ".gitignore",
      ".hivex/\nresponses.json*\ncodex\ncalls.log\npids.json\n",
    );
    p.git_init();
    let mut responses = p.read_json("responses.json");
    responses["review"] = json!({"findings":[],"uncertainties":[]});
    p.json("responses.json", &responses);
    let mut args = vec![
      command,
      "private cached data",
      "--max-calls",
      "3",
      "--progress",
      "always",
    ];
    if command == "review" {
      args.extend(["--base", "HEAD"]);
    }
    let (result, progress) = run(&p, &args);
    assert_eq!(result["status"], "ready");
    assert!(progress.contains("extraction finished; check pending"));
    assert!(progress.contains("admission finished"));
    assert!(progress.contains(&format!("{command}: started")));
    let (retained, progress) = run(&p, &args);
    assert_eq!(retained, result);
    assert!(progress.contains("reusing retained result; no model call"));
    assert_eq!(p.calls(), 3);
  }
}
