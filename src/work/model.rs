use crate::compatibility::{nonnegative_integer, positive_integer};
use crate::error::{HivexError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum State {
  Pending,
  Running,
  BudgetExhausted,
  ContextLimit,
  Failed,
  Done,
}
impl State {
  pub fn as_str(self) -> &'static str {
    match self {
      Self::Pending => "pending",
      Self::Running => "running",
      Self::BudgetExhausted => "budget-exhausted",
      Self::ContextLimit => "context-limit",
      Self::Failed => "failed",
      Self::Done => "done",
    }
  }
  fn parse(text: &str) -> Option<Self> {
    Some(match text {
      "pending" => Self::Pending,
      "running" => Self::Running,
      "budget-exhausted" => Self::BudgetExhausted,
      "context-limit" => Self::ContextLimit,
      "failed" => Self::Failed,
      "done" => Self::Done,
      _ => return None,
    })
  }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
  Update,
  Ask,
  Review,
}
impl Phase {
  fn parse(text: &str) -> Option<Self> {
    Some(match text {
      "update" => Self::Update,
      "ask" => Self::Ask,
      "review" => Self::Review,
      _ => return None,
    })
  }
}

pub struct Budget {
  pub calls: u64,
  pub input_bytes: u64,
  pub max_calls: u64,
  pub max_input_bytes: u64,
}
impl Budget {
  pub fn admits(&self, bytes: u64) -> bool {
    self.calls < self.max_calls
      && self
        .input_bytes
        .checked_add(bytes)
        .is_some_and(|total| total <= self.max_input_bytes)
  }
}
#[derive(Clone, Copy)]
pub struct AttemptInput<'a> {
  pub bytes: u64,
  pub hash: &'a str,
  pub stage: &'a str,
}

pub struct Completion {
  pub report: Value,
  pub result: Option<Value>,
  pub output_hash: Option<String>,
  pub invalid_output: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Work {
  pub(super) row_id: String,
  pub(super) value: Value,
  pub(super) retry_authorized: bool,
}

impl Work {
  pub fn value(&self) -> &Value {
    &self.value
  }

  #[cfg(test)]
  pub(crate) fn value_mut(&mut self) -> &mut Value {
    &mut self.value
  }

  pub fn id(&self) -> &str {
    self
      .value
      .get("id")
      .and_then(Value::as_str)
      .unwrap_or(&self.row_id)
  }

  pub fn row_id(&self) -> &str {
    &self.row_id
  }

  pub fn kind(&self) -> Phase {
    Phase::parse(self.value["kind"].as_str().unwrap_or_default()).expect("validated kind")
  }
  pub fn key(&self) -> &str {
    self
      .value
      .get("key")
      .and_then(Value::as_str)
      .unwrap_or_default()
  }

  pub fn status(&self) -> State {
    State::parse(self.value["status"].as_str().unwrap_or_default()).expect("validated work state")
  }
  pub fn phase(&self) -> Phase {
    Phase::parse(self.value["phase"].as_str().unwrap_or("update")).expect("validated work phase")
  }
  pub fn cache_hits(&self) -> u64 {
    nonnegative_integer(self.value.get("cacheHits")).unwrap_or_default()
  }

  pub fn calls(&self) -> u64 {
    nonnegative_integer(self.value.get("calls")).unwrap_or_default()
  }

  pub fn input_bytes(&self) -> u64 {
    nonnegative_integer(self.value.get("inputBytes")).unwrap_or_default()
  }

  pub fn total_tokens(&self) -> u64 {
    nonnegative_integer(self.value.get("totalTokens")).unwrap_or_default()
  }

  pub fn max_calls(&self) -> u64 {
    nonnegative_integer(self.value.get("maxCalls")).unwrap_or_default()
  }

  pub fn max_input_bytes(&self) -> u64 {
    nonnegative_integer(self.value.get("maxInputBytes")).unwrap_or_default()
  }

  pub fn remaining(&self) -> Vec<String> {
    self
      .value
      .get("remaining")
      .and_then(Value::as_array)
      .map(|entries| {
        entries
          .iter()
          .filter_map(|entry| entry.as_str().map(ToOwned::to_owned))
          .collect()
      })
      .unwrap_or_default()
  }

  pub fn result_key(&self) -> Option<&str> {
    self.value.get("resultKey").and_then(Value::as_str)
  }

  pub fn attempts(&self) -> Option<&Vec<Value>> {
    self.value.get("attempts").and_then(Value::as_array)
  }

  pub fn native_process_id(&self) -> Option<u32> {
    self
      .value
      .get("nativeProcessId")
      .and_then(positive_integer)
      .and_then(|pid| u32::try_from(pid).ok())
  }

  pub fn owner_pid(&self) -> Option<u32> {
    self
      .value
      .get("ownerPid")
      .and_then(positive_integer)
      .and_then(|pid| u32::try_from(pid).ok())
  }

  pub(super) fn status_or_error(&self) -> Result<State> {
    State::parse(self.value["status"].as_str().unwrap_or_default()).ok_or_else(|| {
      HivexError::new(
        "READ_FAILED",
        format!("Work {} has an invalid status", self.id()),
      )
    })
  }
}

impl Work {
  pub fn budget(&self) -> Budget {
    Budget {
      calls: self.calls(),
      input_bytes: self.input_bytes(),
      max_calls: self.max_calls(),
      max_input_bytes: self.max_input_bytes(),
    }
  }
  fn resume(&mut self) {
    self.value["status"] = json!(State::Pending);
  }
  pub fn budget_exhausted(&mut self) {
    self.value["status"] = json!(State::BudgetExhausted);
  }
  pub fn cache_hit(&mut self) -> Result<()> {
    let hits = self
      .cache_hits()
      .checked_add(1)
      .ok_or_else(|| HivexError::new("INVALID_WORK", "Cache count exceeds integer range"))?;
    self.value["cacheHits"] = json!(hits);
    self.resume();
    Ok(())
  }
  pub fn complete(&mut self, result: Value, key: String) -> Result<()> {
    if self.kind() == Phase::Update
      || self.phase() != self.kind()
      || matches!(self.status(), State::Running | State::Failed)
      || !self.remaining().is_empty()
      || !self.value["pending"].is_null()
    {
      return Err(HivexError::new(
        "INVALID_WORK_TRANSITION",
        "A consultation cannot complete before its pending maintenance and invocation finish",
      ));
    }
    self.value["result"] = result;
    self.value["resultKey"] = Value::String(key);
    self.value["status"] = json!(State::Done);
    Ok(())
  }
  pub fn set_plan(&mut self, units: &[String]) -> Result<()> {
    if self.calls() != 0 || !self.value["pending"].is_null() {
      return Err(HivexError::new(
        "INVALID_WORK_TRANSITION",
        "Executed work keeps its original plan and accounting",
      ));
    }
    self.value["plannedUnits"] = json!(units);
    Ok(())
  }
  pub fn set_remaining(&mut self, units: Vec<String>) {
    self.value["remaining"] = Value::from(units);
  }
  pub fn set_pending(&mut self, pending: Value) {
    self.value["pending"] = pending;
  }
  pub fn finish_round(&mut self) {
    self.set_pending(Value::Null);
    if self.remaining().is_empty() {
      if self.kind() == Phase::Update {
        self.value["status"] = json!(State::Done);
      } else {
        self.resume();
        self.value["phase"] = json!(self.kind());
      }
    }
  }
  pub fn limit_context(&mut self, documents: &[String], max_bytes: usize, required_bytes: usize) {
    self.value["contextLimit"] =
      json!({"documents":documents,"maxBytes":max_bytes,"requiredBytes":required_bytes});
    self.value["status"] = json!(State::ContextLimit);
  }
  pub fn clear_context_limit(&mut self) {
    self
      .value
      .as_object_mut()
      .expect("validated work")
      .shift_remove("contextLimit");
  }
  pub fn record_candidate_resolution(&mut self, warnings: &Value) {
    self.value["candidateResolution"] = json!({
      "checkInputHash":self.attempts().and_then(|attempts| attempts.last()).map(|attempt| &attempt["inputHash"]),
      "warnings":warnings
    });
  }
  pub fn assess_retained_check(&mut self, accepted: bool) {
    self.value["retainedCheckAssessment"] = json!(if accepted { "accepted" } else { "blocked" });
  }
  pub fn reject_check(&mut self, record_failure: bool, diagnostic: String) {
    self.value["status"] = json!(State::Failed);
    if record_failure
      && let Some(attempt) = self.value["attempts"]
        .as_array_mut()
        .and_then(|values| values.last_mut())
    {
      attempt["error"] = json!("RELATIONSHIP_LOSS");
      attempt["diagnostic"] = Value::String(diagnostic);
    }
  }
  pub fn record_completion(&mut self, completion: Completion) -> Result<()> {
    let Completion {
      report,
      result,
      output_hash,
      invalid_output,
    } = completion;
    if self.status() != State::Running {
      return Err(HivexError::new(
        "INVALID_WORK_TRANSITION",
        "An execution result requires a running reserved attempt",
      ));
    }
    let total = self
      .total_tokens()
      .checked_add(report["usage"]["totalTokens"].as_u64().unwrap_or(0))
      .ok_or_else(|| HivexError::new("INVALID_WORK", "Token count exceeds integer range"))?;
    self.value["totalTokens"] = json!(total);
    self.resume();
    let attempt = self.value["attempts"]
      .as_array_mut()
      .and_then(|values| values.last_mut())
      .ok_or_else(|| {
        HivexError::new("INVALID_WORK", "An invocation requires a reserved attempt")
      })?;
    attempt["report"] = report;
    if let Some(hash) = output_hash {
      attempt["outputHash"] = json!(hash);
    }
    let missing_result = result.is_none();
    if let Some(value) = result {
      attempt["result"] = value;
    }
    if let Some(diagnostic) = invalid_output {
      attempt["error"] = json!("INVALID_KNOWLEDGE_OUTPUT");
      attempt["diagnostic"] = Value::String(diagnostic);
    }
    if missing_result {
      self.value["status"] = json!(State::Failed);
    }
    Ok(())
  }
  pub(super) fn recovery_record_mut(&mut self) -> &mut Value {
    &mut self.value
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AttemptStage {
  Extract,
  Check,
  Ask,
  Review,
}
impl AttemptStage {
  fn parse(text: &str) -> Option<Self> {
    Some(match text {
      "extract" => Self::Extract,
      "check" => Self::Check,
      "ask" => Self::Ask,
      "review" => Self::Review,
      _ => return None,
    })
  }
}
impl Work {
  pub(super) fn reserve_attempt(
    &mut self,
    input_bytes: u64,
    input_hash: &str,
    stage: &str,
  ) -> Result<()> {
    if matches!(self.status(), State::Running | State::Done)
      || (self.status() == State::Failed && !self.retry_authorized)
    {
      return Err(HivexError::new(
        "INVALID_WORK_TRANSITION",
        "This work cannot reserve an attempt before an explicit admissible resumption",
      ));
    }
    let stage = AttemptStage::parse(stage)
      .ok_or_else(|| HivexError::new("INVALID_WORK", "Attempt stage is invalid"))?;
    if !self.budget().admits(input_bytes) {
      return Err(HivexError::new(
        "WORK_BUDGET_EXHAUSTED",
        "The retained work budget cannot admit this attempt",
      ));
    }
    self.retry_authorized = false;
    self.value["calls"] = json!(self.calls() + 1);
    self.value["inputBytes"] = json!(self.input_bytes() + input_bytes);
    self.value["status"] = json!(State::Running);
    self.value["ownerPid"] = json!(std::process::id());
    self
      .value
      .as_object_mut()
      .unwrap()
      .shift_remove("nativeProcessId");
    self
      .value
      .as_object_mut()
      .unwrap()
      .shift_remove("retainedCheckAssessment");
    self.value["attempts"]
      .as_array_mut()
      .unwrap()
      .push(json!({"inputBytes":input_bytes,"inputHash":input_hash,"stage":stage}));
    Ok(())
  }
}

impl Work {
  pub fn retry_failed(&mut self, requested: bool) -> Result<bool> {
    let last = self.attempts().and_then(|attempts| attempts.last());
    if !requested || self.status() != State::Failed {
      return Ok(false);
    }
    if last.is_some_and(|attempt| attempt["error"] == "RELATIONSHIP_LOSS") {
      self.retry_authorized = true;
      return Ok(false);
    }
    let last = last.cloned().unwrap_or(Value::Null);
    let report = &last["report"];
    let confirmed = report["outcome"].is_string()
      && report.get("usage").is_some()
      && report["cleanup"] == "confirmed"
      && report["turnAccepted"] != "unknown"
      && report["interruption"] != "unconfirmed";
    let legacy = crate::compatibility::legacy_unstarted_invocation(report);
    let before = report["code"] == "MODEL_INTERRUPTED_BEFORE_TURN" || legacy;
    if !confirmed && last["recoveryAcknowledgement"]["type"] != "uncertain-invocation" && !before {
      return Err(HivexError::new(
        "WORK_UNCERTAIN",
        format!(
          "Work {} has an unresolved invocation. Use recover to inspect it; keep its budget and unknown usage.",
          self.id()
        ),
      ));
    }
    self.resume();
    Ok(true)
  }
  pub fn accept_check(&mut self) {
    self.resume();
  }
}

impl Work {
  pub(super) fn bind_execution(&mut self, key: &str, profile: &Value) {
    self.value["operationKey"] = json!(key);
    self.value["executionProfile"] = profile.clone();
  }
}

impl Work {
  pub fn ensure_execution_profile(&self, profile: &Value) -> Result<()> {
    if self
      .value
      .get("executionProfile")
      .is_some_and(|saved| saved != profile)
    {
      return Err(HivexError::new(
        "EXECUTION_PROFILE_CHANGED",
        "The execution profile differs from the retained work; its budget and history were preserved",
      ));
    }
    Ok(())
  }
}
