use crate::support::{Project, list};
use serde_json::{Value, json};
use std::fs;

fn source() -> String {
  (0..6)
    .map(|n| {
      format!(
        "## Section {n}\n\nRule {n} requires bounded work. {}\n",
        "Detail ".repeat(850)
      )
    })
    .collect::<Vec<_>>()
    .join("\n")
}

fn accepted() -> (Project, Value) {
  let p = Project::new();
  p.write("rules.md", source());
  p.model("");
  let mut response = p.read_json("responses.json");
  response["fromVisibleRules"] = json!(true);
  p.json("responses.json", &response);
  let result = p.model_cli(&["update", "--max-calls", "6", "--max-input-bytes", "1048576"]);
  assert_eq!(result["status"], "ready");
  assert_eq!(result["decisions"], 6);
  (p, result)
}

fn packets(p: &Project) -> Vec<Value> {
  fs::read_to_string(p.path("responses.json.packets"))
    .unwrap()
    .lines()
    .map(|line| serde_json::from_str(line).unwrap())
    .collect()
}

#[test]
fn unchanged_units_reuse_extraction_but_check_current_context_and_resume_budget() {
  let (p, initial) = accepted();
  let old_id = initial["work"]["id"].as_str().unwrap();
  let old_work = p.work(old_id);
  let graph = p.graph();
  p.write(
    "rules.md",
    source().replace("Rule 5 requires bounded", "Rule 5 requires audited"),
  );
  let paused = p.model_cli(&["update", "--max-calls", "0", "--max-input-bytes", "1048576"]);
  assert_eq!(paused["status"], "budget-exhausted");
  assert_eq!(paused["work"]["calls"], 0);
  assert_eq!(p.graph(), graph);
  let id = paused["work"]["id"].as_str().unwrap();
  assert_eq!(p.work(id)["pending"]["reusedExtraction"], true);
  let first = p.model_cli(&["update", "--max-calls", "1", "--max-input-bytes", "1048576"]);
  assert_eq!(first["work"]["id"], id);
  assert_eq!(first["work"]["calls"], 1);
  let check = packets(&p).pop().unwrap();
  assert_eq!(check["operation"], "check");
  assert!(list(&check["documents"][0], "lines").iter().any(|line| {
    line[1]
      .as_str()
      .unwrap()
      .contains("Rule 5 requires audited")
  }));
  let result = p.model_cli(&["update", "--max-calls", "3", "--max-input-bytes", "1048576"]);
  assert_eq!(result["status"], "ready");
  assert_eq!(result["work"]["id"], id);
  assert_eq!(result["work"]["calls"], 3);
  assert_eq!(result["decisions"], 6);
  assert!(
    result["work"]["inputBytes"].as_u64().unwrap()
      < initial["work"]["inputBytes"].as_u64().unwrap()
  );
  println!(
    "reuse benchmark: initial calls={} inputBytes={}; localized calls={} inputBytes={}",
    initial["work"]["calls"],
    initial["work"]["inputBytes"],
    result["work"]["calls"],
    result["work"]["inputBytes"]
  );
  assert_eq!(p.work(old_id), old_work);
  let current = p.graph();
  assert!(list(&current, "decisions").iter().all(
    |node| node["quality"] == "checked" && node["version"] == current["documents"]["rules.md"]
  ));
  assert!(
    list(&current, "decisions")
      .iter()
      .any(|node| node["text"].as_str().unwrap().contains("audited"))
  );
  let new_packets = packets(&p).into_iter().skip(6).collect::<Vec<_>>();
  assert_eq!(
    new_packets
      .iter()
      .filter(|packet| packet["operation"] == "extract")
      .count(),
    1
  );
  assert_eq!(
    new_packets
      .iter()
      .filter(|packet| packet["operation"] == "check")
      .count(),
    2
  );
}

#[test]
fn unchanged_text_does_not_certify_a_changed_general_condition() {
  let (p, _) = accepted();
  p.write(
    "rules.md",
    source().replace(
      "Rule 5 requires bounded work.",
      "Rule 5 requires audited work. All earlier rules now apply only to public data.",
    ),
  );
  let mut response = p.read_json("responses.json");
  response["check"] = json!({"findings":[{"target":"c1",
    "reason":"The unchanged interpretation omits the new public-data condition."}]});
  p.json("responses.json", &response);
  let result = p.model_cli(&["update", "--max-calls", "1", "--max-input-bytes", "1048576"]);
  assert_eq!(result["work"]["calls"], 1);
  let graph = p.graph();
  assert!(
    list(&graph, "decisions")
      .iter()
      .any(|node| node["quality"] == "uncertain")
  );
  assert!(list(&graph, "warnings").iter().any(|warning| {
    warning["message"]
      .as_str()
      .is_some_and(|message| message.contains("public-data"))
  }));
  let check = packets(&p).pop().unwrap();
  assert_eq!(check["operation"], "check");
  assert!(
    list(&check["documents"][0], "lines")
      .iter()
      .any(|line| line[1].as_str().unwrap().contains("All earlier rules"))
  );
}

#[test]
fn legacy_coverage_without_hashes_keeps_the_ordinary_extraction_path() {
  let (p, _) = accepted();
  let mut graph = p.graph();
  for unit in graph["units"].as_object_mut().unwrap().values_mut() {
    unit.as_object_mut().unwrap().remove("unitHash");
  }
  p.set_graph(&graph);
  p.write(
    "rules.md",
    source().replace("Rule 5 requires bounded", "Rule 5 requires audited"),
  );
  let result = p.model_cli(&["update", "--max-calls", "6", "--max-input-bytes", "1048576"]);
  assert_eq!(result["status"], "ready");
  assert_eq!(result["work"]["calls"], 6);
  assert_eq!(
    packets(&p)
      .into_iter()
      .skip(6)
      .filter(|packet| packet["operation"] == "extract")
      .count(),
    3
  );
}

#[test]
fn complete_reuse_context_over_limit_falls_back_to_bounded_extraction() {
  let (p, _) = accepted();
  p.write(
    "rules.md",
    source().replace("Rule 5 requires bounded", "Rule 5 requires audited"),
  );
  let result = p.model_cli(&[
    "update",
    "--max-calls",
    "2",
    "--max-input-bytes",
    "1048576",
    "--max-context-bytes",
    "32768",
  ]);
  assert_eq!(result["status"], "budget-exhausted");
  assert_eq!(result["work"]["calls"], 2);
  let actual = packets(&p);
  assert_eq!(actual[6]["operation"], "extract");
  assert_eq!(actual[7]["operation"], "check");
  assert_eq!(actual[6]["units"].as_array().unwrap().len(), 2);
}

#[test]
fn final_reuse_check_must_fit_context_and_remaining_input_budget() {
  for input_limit in ["32768", "1048576"] {
    let (p, _) = accepted();
    if input_limit == "1048576" {
      let mut graph = p.graph();
      for node in graph["decisions"].as_array_mut().unwrap() {
        node["text"] = json!(format!(
          "{} {}",
          node["text"].as_str().unwrap(),
          "Detail ".repeat(250)
        ));
        node["reason"] = json!("Detail ".repeat(280));
        node["conditions"] = json!(["Detail ".repeat(250)]);
      }
      p.set_graph(&graph);
    }
    p.write(
      "rules.md",
      source().replace("Rule 5 requires bounded", "Rule 5 requires audited"),
    );
    let result = p.model_cli(&[
      "update",
      "--max-calls",
      "2",
      "--max-input-bytes",
      input_limit,
    ]);
    assert_eq!(result["status"], "budget-exhausted");
    assert_eq!(result["work"]["calls"], 2);
    assert_eq!(packets(&p)[6]["operation"], "extract");
    assert_eq!(packets(&p)[7]["operation"], "check");
  }
}

#[test]
fn dependency_on_a_pending_unit_falls_back_without_discarding_the_edge() {
  let (p, _) = accepted();
  let mut graph = p.graph();
  let nodes = list(&graph, "decisions");
  graph["relationships"] = json!([{"id":"bridge","localId":"bridge","batch":"seed",
    "from":nodes[0]["id"],"to":nodes[5]["id"],"type":"requires","reason":"The first rule requires the last.",
    "quality":"checked","evidence":[{"document":"rules.md","lineStart":3,"lineEnd":3,"version":nodes[0]["version"]},
    {"document":"rules.md","lineStart":23,"lineEnd":23,"version":nodes[5]["version"]}]}]);
  p.set_graph(&graph);
  p.write(
    "rules.md",
    source().replace("Rule 5 requires bounded", "Rule 5 requires audited"),
  );
  let result = p.model_cli(&["update", "--max-calls", "2", "--max-input-bytes", "1048576"]);
  assert_eq!(result["status"], "budget-exhausted");
  assert_eq!(result["work"]["calls"], 2);
  assert_eq!(packets(&p)[6]["operation"], "extract");
  assert_eq!(p.graph()["relationships"], graph["relationships"]);
}
