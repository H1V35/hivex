//! Synthetic app-server used only by CLI contract tests; never contacts a provider.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::process::Command;
use std::time::Duration;

fn environment(name: &str) -> String {
  std::env::var(name).unwrap_or_default()
}
fn emit(frame: &Value) {
  let mut output = io::stdout().lock();
  writeln!(output, "{frame}").unwrap();
  output.flush().unwrap();
}
fn append(path: &str, text: &str) {
  if !path.is_empty() {
    OpenOptions::new()
      .create(true)
      .append(true)
      .open(path)
      .unwrap()
      .write_all(text.as_bytes())
      .unwrap();
  }
}
fn notify(method: &str, params: Value) -> Value {
  let mut frame = json!({"method":method});
  frame["params"] = params;
  frame
}
fn usage(total: u64) -> Value {
  notify(
    "thread/tokenUsage/updated",
    json!({"threadId":"thread1","turnId":"turn1","tokenUsage":{"total":{"cachedInputTokens":20,"inputTokens":if total==140 {90}else{100},"outputTokens":if total==125 {25}else{50},"reasoningOutputTokens":if total==125 {25}else{30},"totalTokens":total}}}),
  )
}
fn completed(status: &str) -> Value {
  notify(
    "turn/completed",
    json!({"threadId":"thread1","turn":{"id":"turn1","status":status,"error":null}}),
  )
}

fn response(prompt: &str) -> Value {
  let file = environment("HIVEX_TEST_RESPONSES");
  if file.is_empty() {
    return Value::Null;
  }
  let responses: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
  let packet: Value = prompt
    .rsplit_once("\n\n")
    .and_then(|(_, text)| serde_json::from_str(text).ok())
    .unwrap_or(Value::Null);
  if responses["capturePackets"] == true {
    append(&format!("{file}.packets"), &format!("{packet}\n"));
  }
  if packet["operation"] == "extract" && responses["fromVisibleRules"] == true {
    return visible_rules(&packet, &responses);
  }
  if packet["operation"] == "extract" && responses["byDocument"].is_object() {
    return document_response(&packet, &responses);
  }
  let mut result = responses[packet["operation"].as_str().unwrap_or_default()].clone();
  resolve_check_references(&packet, &mut result);
  result
}

struct Server {
  scenario: String,
  options: BTreeMap<String, String>,
  disabled: Vec<String>,
  arguments: Vec<String>,
}
impl Server {
  fn handle(&self, method: &str, params: &Value, events: &mut Vec<Value>) -> Option<Value> {
    let scenario = self.scenario.as_str();
    Some(match method {
      "initialize" => json!({"userAgent":"fixture"}),
      "account/read" => json!({"account":{"type":"chatgpt"}}),
      "configRequirements/read" => {
        json!({"requirements":if scenario=="empty-managed-origin"{json!({"chatgptBaseUrl":""})}else{Value::Null}})
      }
      "mcpServerStatus/list" => json!({"data":[],"nextCursor":null}),
      "config/read" => self.configuration(),
      "model/list" => self.catalog(params),
      "thread/start" => self.start_thread(params),
      "turn/interrupt" => {
        events.push(completed("interrupted"));
        json!({})
      }
      "turn/start" => return self.start_turn(params, events),
      _ => panic!("unsupported fixture request {method}"),
    })
  }
  fn configuration(&self) -> Value {
    let scenario = self.scenario.as_str();
    let mut features = serde_json::Map::new();
    for feature in &self.disabled {
      features.insert(feature.clone(), json!(false));
    }
    features.insert(
      "skip_host_skill_discovery".into(),
      json!(
        self
          .arguments
          .iter()
          .any(|value| value == "skip_host_skill_discovery")
      ),
    );
    let mcp = if ["configured-mcp", "ignored-mcp", "abort-isolation"].contains(&scenario) {
      json!({"hivex_extra":{"command":"/must-not-start","enabled":scenario=="ignored-mcp"||self.options.get("mcp_servers.hivex_extra.enabled").map(String::as_str)!=Some("false")}})
    } else {
      json!({})
    };
    let endpoint = match scenario {
      "redirected-provider" => "https://not-openai.invalid",
      "normalized-endpoint" => "HTTPS://CHATGPT.COM:443/./",
      _ => "https://chatgpt.com",
    };
    let option = |key: &str| {
      self
        .options
        .get(key)
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .unwrap_or(Value::Null)
    };
    let config = json!({"chatgpt_base_url":endpoint,"features":features,"mcp_servers":mcp,"memories":{"generate_memories":false,"use_memories":false},"model":option("model"),"model_provider":"openai","model_providers":{},"model_reasoning_effort":option("model_reasoning_effort"),"openai_base_url":null,"project_doc_max_bytes":option("project_doc_max_bytes"),"web_search":option("web_search")});
    let mut origins = serde_json::Map::new();
    for key in [
      "model",
      "model_provider",
      "model_reasoning_effort",
      "chatgpt_base_url",
    ] {
      origins.insert(
        key.into(),
        json!({"name":{"type":"sessionFlags"},"version":"fixture-config-v1"}),
      );
    }
    json!({"config":config,"origins":origins})
  }

  fn catalog(&self, params: &Value) -> Value {
    let scenario = self.scenario.as_str();
    let pages = match scenario {
      "catalog-21-pages" => 20,
      "catalog-22-pages" => 21,
      _ => 0,
    };
    let page = params["cursor"]
      .as_str()
      .unwrap_or("0")
      .parse::<usize>()
      .unwrap();
    if page < pages {
      json!({"data":[],"nextCursor":(page+1).to_string()})
    } else {
      json!({"data":[{"model":"gpt-6-luna","supportedReasoningEfforts":[{"reasoningEffort":"max"},{"reasoningEffort":"medium"}]},{"model":"fixture-model","supportedReasoningEfforts":[{"reasoningEffort":"high"}]}],"nextCursor":null})
    }
  }

  fn start_thread(&self, params: &Value) -> Value {
    let scenario = self.scenario.as_str();
    assert_eq!(
      params["model"],
      serde_json::from_str::<Value>(&self.options["model"]).unwrap()
    );
    assert_eq!(params["allowProviderModelFallback"], false);
    if scenario == "configured-mcp" {
      assert_eq!(
        self
          .options
          .get("mcp_servers.hivex_extra.enabled")
          .map(String::as_str),
        Some("false")
      );
    }
    json!({"cwd":std::env::current_dir().unwrap(),"instructionSources":if scenario=="instruction-source-metadata"{json!(["/example/.codex/AGENTS.md"])}else{json!([])},"model":params["model"],"modelProvider":"openai","reasoningEffort":if scenario=="changed-effort"{json!("high")}else{serde_json::from_str::<Value>(&self.options["model_reasoning_effort"]).unwrap()},"sandbox":{"type":"readOnly"},"thread":{"ephemeral":true,"id":"thread1"}})
  }

  fn start_turn(&self, params: &Value, events: &mut Vec<Value>) -> Option<Value> {
    let scenario = self.scenario.as_str();
    append(&environment("HIVEX_TEST_CALLS_PATH"), "called\n");
    let hold = environment("HIVEX_TEST_HOLD_PATH");
    while !hold.is_empty()
      && let Ok(contents) = fs::read_to_string(&hold)
    {
      if contents == "after-first"
        && fs::read_to_string(environment("HIVEX_TEST_CALLS_PATH")).unwrap_or_default()
          == "called\n"
      {
        break;
      }
      std::thread::sleep(Duration::from_millis(10));
    }
    append(&environment("HIVEX_TEST_CALLS_FILE"), "called\n");
    assert_eq!(
      params["model"],
      serde_json::from_str::<Value>(&self.options["model"]).unwrap()
    );
    assert_eq!(
      params["effort"],
      serde_json::from_str::<Value>(&self.options["model_reasoning_effort"]).unwrap()
    );
    assert_eq!(params["sandboxPolicy"]["networkAccess"], false);
    let prompt = params["input"][0]["text"].as_str().unwrap();
    assert!(!prompt.trim().is_empty());
    if ["start-unconfirmed", "update-uncertain"].contains(&scenario) {
      return None;
    }
    if scenario == "unconfirmed-interrupt" {
      std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(20));
        std::process::exit(0);
      });
      return Some(json!({"turn":{"id":"turn1","status":"inProgress"}}));
    }
    self.interaction_events(events);
    if ["timeout", "timeout-unmeasured", "cancel"].contains(&scenario) {
      if scenario != "timeout-unmeasured" {
        events.push(usage(125));
      }
      if scenario == "cancel" {
        unsafe {
          libc::kill(libc::getppid(), libc::SIGINT);
        }
      }
    } else {
      self.answer_events(prompt, events);
    }
    Some(json!({"turn":{"id":"turn1","status":"inProgress"}}))
  }
  fn interaction_events(&self, events: &mut Vec<Value>) {
    let scenario = self.scenario.as_str();
    if scenario == "interactions" {
      for (id, method) in [
        ("approval-command", "item/commandExecution/requestApproval"),
        ("approval-file", "item/fileChange/requestApproval"),
        ("outside-tool", "fixture/requestInput"),
      ] {
        events.push(json!({"id":id,"method":method,"params":{}}));
      }
    }
    if scenario == "oversized-stream" {
      for _ in 0..34 {
        emit(&notify(
          "fixture/unknown",
          json!({"text":"x".repeat(1_048_576)}),
        ));
      }
    }
    if scenario == "oversized-frame" {
      emit(&notify(
        "fixture/unknown",
        json!({"text":"x".repeat(4_194_304)}),
      ));
    }
  }

  fn answer_events(&self, prompt: &str, events: &mut Vec<Value>) {
    let scenario = self.scenario.as_str();
    if scenario == "wrong-identity" {
      events.push(notify("item/completed",json!({"threadId":"other-thread","turnId":"other-turn","item":{"id":"wrong","type":"agentMessage","text":"wrong","phase":"final_answer"}})));
    }
    if scenario == "inconsistent-usage" {
      let mut bad = usage(150);
      bad["params"]["tokenUsage"]["total"]["totalTokens"] = json!(151);
      events.push(bad);
    }
    events.push(usage(150));
    if scenario == "usage-regression" {
      events.push(usage(140));
    }
    let text = if scenario == "invalid-json" {
      "{broken".into()
    } else {
      response(prompt).to_string()
    };
    events.push(notify("item/completed",json!({"threadId":"thread1","turnId":"turn1","item":{"id":"message1","phase":"final_answer","text":text,"type":"agentMessage"}})));
    events.push(completed("completed"));
    if scenario == "duplicate-terminal" {
      events.push(completed("completed"));
    }
  }
}

fn main() {
  let arguments: Vec<String> = std::env::args().collect();
  if arguments
    .iter()
    .any(|argument| argument == "fixture-descendant")
  {
    if arguments.iter().any(|argument| argument == "ignore-term") {
      unsafe {
        libc::signal(libc::SIGTERM, libc::SIG_IGN);
      }
    }
    println!("ready");
    io::stdout().flush().unwrap();
    loop {
      std::thread::sleep(Duration::from_secs(1));
    }
  }
  let scenario = environment("HIVEX_TEST_SCENARIO");
  if scenario == "secret-environment"
    && [
      "GH_TOKEN",
      "DATABASE_URL",
      "HTTP_PROXY",
      "HTTPS_PROXY",
      "ALL_PROXY",
    ]
    .iter()
    .any(|name| !environment(name).is_empty())
  {
    std::process::exit(19);
  }
  if arguments.iter().any(|argument| argument == "--version") {
    if scenario == "unreadable-version" {
      std::process::exit(1);
    }
    println!(
      "codex-cli {}",
      if scenario == "future-version" {
        "9.99.0"
      } else {
        "0.153.2"
      }
    );
    return;
  }
  if scenario == "descendant" {
    spawn_descendants();
  }
  let mut options = BTreeMap::new();
  let mut disabled = Vec::new();
  for pair in arguments.windows(2) {
    if pair[0] == "-c" {
      let (key, value) = pair[1].split_once('=').unwrap();
      options.insert(key.to_owned(), value.to_owned());
    }
    if pair[0] == "--disable" {
      disabled.push(pair[1].clone());
    }
  }
  let server = Server {
    scenario,
    options,
    disabled,
    arguments,
  };
  serve(&server);
  if server.scenario == "abort-isolation" {
    unsafe {
      libc::kill(libc::getppid(), libc::SIGINT);
    }
  }
}

fn visible_rules(packet: &Value, responses: &Value) -> Value {
  let mut decisions = Vec::new();
  for document in packet["documents"].as_array().unwrap() {
    if !packet["targets"]
      .as_array()
      .unwrap()
      .contains(&document["id"])
    {
      continue;
    }
    for line in document["lines"].as_array().unwrap() {
      let number = line[0].as_u64().unwrap();
      let text = line[1].as_str().unwrap();
      let visible = packet["units"].as_array().is_none_or(|units| {
        units.iter().any(|unit| {
          unit["document"] == document["id"]
            && unit["lineStart"].as_u64().unwrap() <= number
            && unit["lineEnd"].as_u64().unwrap() >= number
        })
      });
      if !visible || !text.starts_with("Rule ") || !text.contains(" requires") {
        continue;
      }
      let mut decision = responses["extract"]["decisions"][0].clone();
      decision["document"] = document["id"].clone();
      decision["id"] = json!(format!("c{number}"));
      decision["lineStart"] = json!(number);
      decision["lineEnd"] = json!(number);
      decision["text"] = json!(format!("{}.", text.split(". ").next().unwrap()));
      decisions.push(decision);
    }
  }
  json!({"decisions":decisions,"relationships":[],"uncertainties":[]})
}

fn document_response(packet: &Value, responses: &Value) -> Value {
  let targets = packet["targets"].as_array().cloned().unwrap_or_else(|| {
    packet["documents"]
      .as_array()
      .unwrap()
      .iter()
      .map(|document| document["id"].clone())
      .collect()
  });
  let mut decisions = Vec::new();
  let mut relationships = Vec::new();
  for target in targets {
    let part = &responses["byDocument"][target.as_str().unwrap()];
    decisions.extend(part["decisions"].as_array().cloned().unwrap_or_default());
    for mut relationship in part["relationships"]
      .as_array()
      .cloned()
      .unwrap_or_default()
    {
      if let Some(required) = relationship["requiresEvidenceDocument"].as_str()
        && !packet["documents"]
          .as_array()
          .unwrap()
          .iter()
          .any(|document| document["id"] == required)
      {
        continue;
      }
      resolve_endpoints(packet, &mut relationship);
      relationships.push(relationship);
    }
  }
  json!({"decisions":decisions,"relationships":relationships,"uncertainties":[]})
}

fn serve(server: &Server) {
  for line in io::stdin().lock().lines() {
    let Ok(line) = line else { break };
    let frame: Value = serde_json::from_str(&line).unwrap();
    if !handle_frame(server, &frame) {
      return;
    }
  }
}

fn handle_frame(server: &Server, frame: &Value) -> bool {
  if frame["id"].is_null() {
    return true;
  }
  if frame["method"].is_null() {
    record_interaction_response(frame);
    return true;
  }
  if frame["method"] == "initialize" {
    let malformed = match server.scenario.as_str() {
      "rpc-non-object" => Some(json!([])),
      "rpc-invalid-id" => Some(json!({"id":true,"result":{}})),
      "rpc-invalid-method" => Some(json!({"method":3})),
      "rpc-two-outcomes" => {
        Some(json!({"id":frame["id"],"result":{},"error":{"code":-1,"message":"failure"}}))
      }
      "rpc-remote-error" => {
        Some(json!({"id":frame["id"],"error":{"code":-32602,"message":"synthetic failure"}}))
      }
      "rpc-partial-eof" => {
        print!("{{broken");
        io::stdout().flush().unwrap();
        return false;
      }
      _ => None,
    };
    if let Some(value) = malformed {
      emit(&value);
      return true;
    }
    if server.scenario == "rpc-out-of-order" {
      emit(&json!({"id":"unrelated-response","result":{}}));
    }
  }
  let mut events = Vec::new();
  let result = server.handle(
    frame["method"].as_str().unwrap(),
    &frame["params"],
    &mut events,
  );
  if server.scenario == "terminal-before-response" {
    for event in events.drain(..) {
      emit(&event);
    }
  }
  if let Some(value) = result {
    emit(&json!({"id":frame["id"],"result":value}));
  }
  for event in events {
    emit(&event);
  }
  true
}

fn spawn_descendants() {
  let mut children = Vec::new();
  for ignore in [false, true] {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
      .arg("fixture-descendant")
      .stdin(std::process::Stdio::null())
      .stdout(std::process::Stdio::piped());
    if ignore {
      command.arg("ignore-term");
    }
    let mut child = command.spawn().unwrap();
    let mut ready = String::new();
    io::BufReader::new(child.stdout.take().unwrap())
      .read_line(&mut ready)
      .unwrap();
    assert_eq!(ready, "ready\n");
    children.push(child.id());
    std::thread::spawn(move || {
      let _ = child.wait();
    });
  }
  fs::write(
    environment("HIVEX_TEST_PID_PATH"),
    json!(children).to_string(),
  )
  .unwrap();
}

fn resolve_check_references(packet: &Value, result: &mut Value) {
  if packet["operation"] != "check" {
    return;
  }
  let Some(changes) = result["relationshipChanges"].as_array_mut() else {
    return;
  };
  for change in changes {
    if change["previousId"] == "@removed:0" {
      change["previousId"] = packet["removedRelationships"][0]["id"].clone();
    }
    for replacement in change["replacements"].as_array_mut().unwrap() {
      if *replacement == "@candidate:0" {
        *replacement = packet["extraction"]["relationships"][0]["id"].clone();
      }
    }
  }
}

fn resolve_endpoints(packet: &Value, relationship: &mut Value) {
  for key in ["from", "to"] {
    if let Some(document) = relationship[key]
      .as_str()
      .and_then(|text| text.strip_prefix("@existing:"))
      && let Some(existing) = packet["existing"]
        .as_array()
        .and_then(|values| values.iter().find(|value| value["document"] == document))
    {
      relationship[key] = existing["id"].clone();
    }
  }
}

fn record_interaction_response(frame: &Value) {
  let id = frame["id"].as_str().unwrap();
  if id.starts_with("approval-") {
    assert_eq!(frame["result"]["decision"], "decline");
  } else {
    assert_eq!(frame["error"]["code"], -32601);
  }
  append(
    &format!("{}.interactions", environment("HIVEX_TEST_RESPONSES")),
    &format!("{frame}\n"),
  );
}
