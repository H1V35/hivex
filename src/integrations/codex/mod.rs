use num_traits::ToPrimitive;
mod process;
mod profile;
mod protocol;
use crate::error::{HivexError, Result};
use chrono::{SecondsFormat, Utc};
use process::{NativeServer, SignalGuard, start_server, stop_after_error, unique_workspace};
use profile::{admit_profile, knowledge_turn, launch_arguments, start_thread};
use protocol::{ResponseWait, Rpc, Terminal, Transcript};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::execution::{Integration, Invocation, Profile as ExecutionProfile, Receipt};

pub fn default_profile() -> ExecutionProfile {
  ExecutionProfile {
    integration: "codex".into(),
    provider: "openai".into(),
    model: "gpt-6-luna".into(),
    options: std::collections::BTreeMap::from([("effort".into(), "max".into())]),
  }
}
pub struct Codex {
  pub binary: String,
  pub profile: ExecutionProfile,
}
impl Integration for Codex {
  fn model_summary(&self) -> Value {
    json!({"effort":self.profile.options["effort"],"name":self.profile.model,"provider":self.profile.provider})
  }
  fn profile(&self) -> &ExecutionProfile {
    &self.profile
  }
  fn cache_identity(&self) -> Value {
    if self.model_summary() == legacy_identity() {
      legacy_identity()
    } else {
      json!(self.profile)
    }
  }
  fn legacy_cache_identity(&self) -> Option<Value> {
    Some(legacy_identity())
  }
  fn replaced_profile(&self) -> Option<ExecutionProfile> {
    (self.profile == default_profile()).then(|| ExecutionProfile {
      model: "gpt-5.6-luna".into(),
      ..default_profile()
    })
  }
  fn invoke(
    &self,
    request: Invocation,
    on_process: &mut dyn FnMut(u32) -> Result<()>,
  ) -> Result<Receipt> {
    let result = invoke(
      &InvocationOptions {
        binary: self.binary.clone(),
        profile: self.profile.clone(),
        prompt: request.prompt,
        schema: request.schema,
        deadline_ms: request.deadline_ms,
      },
      on_process,
    )?;
    let effective_profile =
      (result.report["turnAccepted"] == "confirmed").then(|| self.profile.clone());
    Ok(Receipt {
      effective_profile,
      value: result.value,
      report: result.report,
    })
  }
}
fn legacy_identity() -> Value {
  json!({"name":"gpt-5.6-luna","effort":"max","provider":"openai"})
}

const APP_CLIENT_VERSION: &str = "0.1.0";
const RPC_FRAME_LIMIT: usize = 4_194_304;
const RPC_STREAM_LIMIT: usize = 33_554_432;
const MAX_VERSION_BYTES: usize = 65_536;
const MAX_CATALOG_CONTINUATIONS: usize = 20;
const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(30);
const THREAD_START_TIMEOUT: Duration = Duration::from_secs(90);
const INTERRUPT_TIMEOUT: Duration = Duration::from_secs(5);
const DISABLED_FEATURES: [&str; 16] = [
  "apps",
  "plugins",
  "remote_plugin",
  "multi_agent",
  "shell_tool",
  "unified_exec",
  "code_mode_host",
  "code_mode",
  "in_app_browser",
  "image_generation",
  "view_image",
  "skill_search",
  "skill_mcp_dependency_install",
  "tool_suggest",
  "sleep_tool",
  "memories",
];
const ENVIRONMENT_KEYS: [&str; 12] = [
  "HOME",
  "CODEX_HOME",
  "PATH",
  "LANG",
  "USER",
  "LOGNAME",
  "SHELL",
  "TMPDIR",
  "TMP",
  "TEMP",
  "CODEX_SANDBOX",
  "CODEX_SANDBOX_NETWORK_DISABLED",
];

static CANCELLED: AtomicBool = AtomicBool::new(false);
static INVOCATION: Mutex<()> = Mutex::new(());

#[derive(Debug)]
struct InvocationOptions {
  profile: ExecutionProfile,
  pub binary: String,
  pub prompt: String,
  pub schema: Value,
  pub deadline_ms: u64,
}

#[derive(Debug)]
struct InvocationResult {
  pub value: Value,
  pub report: Value,
}

#[derive(Debug)]
enum NativeError {
  Admission(String),
  PreSpawn(Box<NativeError>),
  InvalidFields(Vec<String>),
  AdmissionProcess {
    cause: Box<NativeError>,
    process_id: u32,
    admission: Value,
    cleanup_confirmed: bool,
  },
  Cancelled,
  Io(String),
  Protocol(String),
  Remote {
    code: i64,
    message: String,
  },
  Timeout,
}

impl NativeError {
  fn message(&self) -> String {
    match self {
      Self::AdmissionProcess { cause, .. } | Self::PreSpawn(cause) => cause.message(),
      Self::InvalidFields(fields) => format!("Invalid native fields: {}", fields.join(", ")),
      Self::Cancelled => "request cancelled".to_owned(),
      Self::Admission(message)
      | Self::Io(message)
      | Self::Protocol(message)
      | Self::Remote { message, .. } => message.clone(),
      Self::Timeout => "request timed out".to_owned(),
    }
  }

  fn diagnostic(&self) -> Value {
    match self {
      Self::AdmissionProcess { cause, .. } | Self::PreSpawn(cause) => cause.diagnostic(),
      Self::InvalidFields(fields) => {
        json!({"kind": "invalid-native-response", "fields": fields})
      }
      Self::Remote { code, .. } => json!({"kind": "rpc-rejection", "code": code}),
      _ => json!({"kind": "native-admission", "message": self.message()}),
    }
  }

  fn at(self, prefix: &str) -> Self {
    match self {
      Self::InvalidFields(fields) => Self::InvalidFields(
        fields
          .into_iter()
          .map(|field| {
            if field.is_empty() {
              prefix.to_owned()
            } else {
              format!("{prefix}.{field}")
            }
          })
          .collect(),
      ),
      error => error,
    }
  }

  fn process_id(&self) -> Option<u32> {
    match self {
      Self::AdmissionProcess { process_id, .. } => Some(*process_id),
      _ => None,
    }
  }

  fn admission(&self) -> Option<Value> {
    match self {
      Self::AdmissionProcess { admission, .. } if !admission.is_null() => Some(admission.clone()),
      _ => None,
    }
  }

  fn cleanup_confirmed(&self) -> bool {
    match self {
      Self::AdmissionProcess {
        cleanup_confirmed, ..
      } => *cleanup_confirmed,
      Self::PreSpawn(_) => true,
      _ => false,
    }
  }

  fn is_cancelled(&self) -> bool {
    match self {
      Self::Cancelled => true,
      Self::AdmissionProcess { cause, .. } | Self::PreSpawn(cause) => cause.is_cancelled(),
      _ => false,
    }
  }
}

impl From<std::io::Error> for NativeError {
  fn from(error: std::io::Error) -> Self {
    Self::Io(error.to_string())
  }
}

impl From<serde_json::Error> for NativeError {
  fn from(error: serde_json::Error) -> Self {
    Self::Protocol(error.to_string())
  }
}

#[derive(Clone, Copy)]
struct TurnContext<'a> {
  thread_id: &'a str,
  turn_id: &'a str,
  deadline_ms: u64,
}

fn run_turn(
  server: &mut NativeServer,
  transcript: &mut Transcript,
  thread_id: &str,
  options: &InvocationOptions,
) -> (Value, Value) {
  let begin = Instant::now();
  let deadline = Duration::from_millis(options.deadline_ms);
  let turn_id = match start_turn(server, transcript, thread_id, options) {
    Ok(turn_id) => turn_id,
    Err(error) => {
      return (
        Value::Null,
        report_map(ReportInput {
          outcome: "failed",
          code: if matches!(error, NativeError::Cancelled) {
            "MODEL_CANCELLED"
          } else {
            "MODEL_START_UNCONFIRMED"
          },
          deadline_ms: options.deadline_ms,
          thread_id: Some(thread_id),
          turn_id: None,
          usage: transcript.measured(thread_id, ""),
          turn_accepted: Some("unknown"),
          diagnostic: Some(error.diagnostic()),
        }),
      );
    }
  };
  let turn = TurnContext {
    thread_id,
    turn_id: &turn_id,
    deadline_ms: options.deadline_ms,
  };
  let terminal = server.rpc.wait_terminal(
    thread_id,
    &turn_id,
    ResponseWait {
      timeout: deadline.saturating_sub(begin.elapsed()),
      transcript,
      honor_cancel: true,
    },
  );
  let completion = match terminal {
    Ok(terminal) if terminal.status == "completed" => complete_turn(server, transcript, &turn),
    Ok(terminal) => {
      let error = NativeError::Protocol(format!("Model completion status was {}", terminal.status));
      let interrupted = interrupt_turn(server, transcript, thread_id, &turn_id);
      return (
        Value::Null,
        report_turn_failure(turn, transcript, &error, interrupted),
      );
    }
    Err(error) => {
      let interrupted = interrupt_turn(server, transcript, thread_id, &turn_id);
      return (
        Value::Null,
        report_turn_failure(turn, transcript, &error, interrupted),
      );
    }
  };
  match completion {
    Ok(text) => (
      Value::String(text),
      report_map(ReportInput {
        outcome: "completed",
        code: "",
        deadline_ms: options.deadline_ms,
        thread_id: Some(thread_id),
        turn_id: Some(&turn_id),
        usage: transcript.measured(thread_id, &turn_id),
        turn_accepted: Some("confirmed"),
        diagnostic: None,
      }),
    ),
    Err(error) => (
      Value::Null,
      report_turn_failure(turn, transcript, &error, false),
    ),
  }
}

fn start_turn(
  server: &mut NativeServer,
  transcript: &mut Transcript,
  thread_id: &str,
  options: &InvocationOptions,
) -> std::result::Result<String, NativeError> {
  let timeout = Duration::from_millis(options.deadline_ms).min(Duration::from_secs(30));
  let value = server.rpc.request(
    "turn/start",
    Some(knowledge_turn(
      thread_id,
      &options.prompt,
      &options.schema,
      &options.profile,
    )),
    ResponseWait {
      timeout: timeout.max(Duration::from_millis(1)),
      transcript,
      honor_cancel: false,
    },
  )?;
  value
    .as_object()
    .and_then(|object| object.get("turn"))
    .and_then(Value::as_object)
    .and_then(|turn| turn.get("id"))
    .and_then(Value::as_str)
    .map(str::to_owned)
    .ok_or_else(|| NativeError::Protocol("turn start response is invalid".to_owned()))
}

fn complete_turn(
  server: &mut NativeServer,
  transcript: &mut Transcript,
  turn: &TurnContext<'_>,
) -> std::result::Result<String, NativeError> {
  server.rpc.drain(transcript, Duration::from_millis(20))?;
  let text = transcript
    .final_text(turn.thread_id, turn.turn_id)
    .ok_or_else(|| NativeError::Protocol("Structured model output is missing".to_owned()))?;
  transcript.assert_valid(turn.thread_id, turn.turn_id)?;
  if fs::read_dir(&server.workspace)?.next().is_some() {
    return Err(NativeError::Protocol(
      "Knowledge workspace was mutated".to_owned(),
    ));
  }
  Ok(text.to_owned())
}

fn interrupt_turn(
  server: &mut NativeServer,
  transcript: &mut Transcript,
  thread_id: &str,
  turn_id: &str,
) -> bool {
  if server
    .rpc
    .request(
      "turn/interrupt",
      Some(json!({"threadId": thread_id, "turnId": turn_id})),
      ResponseWait {
        timeout: INTERRUPT_TIMEOUT,
        transcript,
        honor_cancel: false,
      },
    )
    .is_err()
  {
    return false;
  }
  matches!(
      server
          .rpc
          .wait_terminal(thread_id, turn_id, ResponseWait { timeout: INTERRUPT_TIMEOUT, transcript, honor_cancel: false }),
      Ok(Terminal { status, .. }) if status == "interrupted"
  )
}

fn report_turn_failure(
  turn: TurnContext<'_>,
  transcript: &Transcript,
  error: &NativeError,
  interrupted: bool,
) -> Value {
  let TurnContext {
    thread_id,
    turn_id,
    deadline_ms,
  } = turn;
  let (outcome, code) = match error {
    NativeError::Cancelled => ("failed", "MODEL_CANCELLED"),
    NativeError::Timeout => ("timeout", "MODEL_TIMEOUT"),
    _ => ("failed", "MODEL_PROTOCOL_FAILED"),
  };
  let mut report = report_map(ReportInput {
    outcome,
    code,
    deadline_ms,
    thread_id: Some(thread_id),
    turn_id: Some(turn_id),
    usage: transcript.measured(thread_id, turn_id),
    turn_accepted: Some("confirmed"),
    diagnostic: Some(error.diagnostic()),
  });
  if let Some(object) = report.as_object_mut() {
    object.insert(
      "interruption".to_owned(),
      Value::String(
        if interrupted {
          "confirmed"
        } else {
          "unconfirmed"
        }
        .to_owned(),
      ),
    );
  }
  report
}

struct ReportInput<'a> {
  outcome: &'a str,
  code: &'a str,
  deadline_ms: u64,
  thread_id: Option<&'a str>,
  turn_id: Option<&'a str>,
  usage: Value,
  turn_accepted: Option<&'a str>,
  diagnostic: Option<Value>,
}

fn report_map(input: ReportInput<'_>) -> Value {
  let ReportInput {
    outcome,
    code,
    deadline_ms,
    thread_id,
    turn_id,
    usage,
    turn_accepted,
    diagnostic,
  } = input;
  let mut report = Map::new();
  report.insert("outcome".to_owned(), Value::String(outcome.to_owned()));
  if !code.is_empty() {
    report.insert("code".to_owned(), Value::String(code.to_owned()));
  }
  if let Some(thread_id) = thread_id {
    report.insert("threadId".to_owned(), Value::String(thread_id.to_owned()));
  }
  if let Some(turn_id) = turn_id {
    report.insert("turnId".to_owned(), Value::String(turn_id.to_owned()));
  }
  if let Some(turn_accepted) = turn_accepted {
    report.insert(
      "turnAccepted".to_owned(),
      Value::String(turn_accepted.to_owned()),
    );
  }
  report.insert("deadlineMilliseconds".to_owned(), json!(deadline_ms));
  report.insert("usage".to_owned(), usage);
  if let Some(diagnostic) = diagnostic {
    report.insert("diagnostic".to_owned(), diagnostic);
  }
  Value::Object(report)
}

struct InvocationState {
  report: Value,
  value: Value,
  admission: Option<Value>,
  native_pid: Option<u32>,
  server: Option<NativeServer>,
  remove_workspace: bool,
  server_cleanup_confirmed: bool,
  server_cleanup_failed: bool,
}

fn invoke<F>(options: &InvocationOptions, mut callback_pid: F) -> Result<InvocationResult>
where
  F: FnMut(u32) -> Result<()>,
{
  // Signal handlers are process-wide. Serialize this internal CLI transport.
  let _invocation = INVOCATION
    .lock()
    .map_err(|_| HivexError::new("READ_FAILED", "Native invocation lock was poisoned"))?;
  let _signals = SignalGuard::install()?;
  let started_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
  let began = Instant::now();
  let workspace = unique_workspace()?;
  let mut state = InvocationState::new(options.deadline_ms);
  if let Err(error) = state.execute(options, &workspace, &mut callback_pid) {
    state.record_failure(&error, options.deadline_ms);
  }
  state.stop();
  Ok(state.finish(&workspace, started_at, began))
}

impl InvocationState {
  fn new(deadline_ms: u64) -> Self {
    Self {
      report: report_map(ReportInput {
        outcome: "failed",
        code: "MODEL_ADMISSION_FAILED",
        deadline_ms,
        thread_id: None,
        turn_id: None,
        usage: Value::Null,
        turn_accepted: None,
        diagnostic: None,
      }),
      value: Value::Null,
      admission: None,
      native_pid: None,
      server: None,
      remove_workspace: true,
      server_cleanup_confirmed: false,
      server_cleanup_failed: false,
    }
  }

  fn execute(
    &mut self,
    options: &InvocationOptions,
    workspace: &Path,
    callback_pid: &mut dyn FnMut(u32) -> Result<()>,
  ) -> std::result::Result<(), NativeError> {
    let mut native = start_server(&options.binary, workspace, &options.profile)?;
    self.native_pid = Some(native.pid);
    self.admission = Some(native.admission.clone());
    if let Err(error) = callback_pid(native.pid) {
      let (error, confirmed) = stop_after_error(native, NativeError::Io(error.message));
      self.server_cleanup_confirmed = confirmed;
      self.server_cleanup_failed = !confirmed;
      return Err(error);
    }
    let mut transcript = Transcript::new();
    let thread_id = match start_thread(
      &mut native.rpc,
      &mut transcript,
      &workspace.to_string_lossy(),
      &options.profile,
    ) {
      Ok(thread_id) => thread_id,
      Err(error) => {
        let (error, confirmed) = stop_after_error(native, error);
        self.server_cleanup_confirmed = confirmed;
        self.server_cleanup_failed = !confirmed;
        return Err(error);
      }
    };
    let (turn_value, turn_report) = run_turn(&mut native, &mut transcript, &thread_id, options);
    self.value = turn_value;
    self.report = turn_report;
    self.server = Some(native);
    Ok(())
  }

  fn record_failure(&mut self, error: &NativeError, deadline_ms: u64) {
    if self.native_pid.is_none() {
      self.native_pid = error.process_id();
    }
    if self.admission.is_none() {
      self.admission = error.admission();
    }
    let failure = report_map(ReportInput {
      outcome: "failed",
      code: if error.is_cancelled() {
        "MODEL_CANCELLED"
      } else {
        "MODEL_ADMISSION_FAILED"
      },
      deadline_ms,
      thread_id: None,
      turn_id: None,
      usage: Value::Null,
      turn_accepted: None,
      diagnostic: Some(error.diagnostic()),
    });
    if let Some(native) = self.server.as_ref() {
      self.admission = Some(native.admission.clone());
    }
    self.server_cleanup_confirmed |= error.cleanup_confirmed();
    self.server_cleanup_failed |= error.process_id().is_some() && !error.cleanup_confirmed();
    if self.server_cleanup_failed {
      self.remove_workspace = false;
    }
    self.report = failure;
  }

  fn stop(&mut self) {
    if let Some(native) = self.server.take() {
      match native.stop() {
        Ok(()) => {
          if let Some(object) = self.report.as_object_mut() {
            object.insert("cleanup".to_owned(), Value::String("confirmed".to_owned()));
          }
        }
        Err(error) => {
          self.remove_workspace = false;
          if let Some(object) = self.report.as_object_mut() {
            object.insert("outcome".to_owned(), Value::String("failed".to_owned()));
            object.insert(
              "code".to_owned(),
              Value::String("MODEL_CLEANUP_FAILED".to_owned()),
            );
            object.insert("diagnostic".to_owned(), error.diagnostic());
            object.insert("cleanup".to_owned(), Value::String("failed".to_owned()));
          }
        }
      }
      return;
    }
    if let Some(object) = self.report.as_object_mut() {
      object.insert(
        "cleanup".to_owned(),
        Value::String(
          match (self.server_cleanup_failed, self.server_cleanup_confirmed) {
            (true, _) => "failed",
            (false, true) => "confirmed",
            (false, false) => "not-observed",
          }
          .to_owned(),
        ),
      );
    }
  }

  fn finish(self, workspace: &Path, started_at: String, began: Instant) -> InvocationResult {
    let Self {
      mut value,
      mut report,
      admission,
      native_pid,
      remove_workspace,
      ..
    } = self;
    if let Some(admission) = admission
      && let Some(object) = report.as_object_mut()
    {
      object.insert("admission".to_owned(), admission);
    }
    if let Some(native_pid) = native_pid
      && let Some(object) = report.as_object_mut()
    {
      object.insert("nativeProcessId".to_owned(), json!(native_pid));
    }
    if let Some(object) = report.as_object_mut() {
      object.insert("startedAt".to_owned(), Value::String(started_at));
      object.insert(
        "durationMilliseconds".to_owned(),
        json!(u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX)),
      );
    }
    if remove_workspace
      && let Err(error) = fs::remove_dir_all(workspace)
      && let Some(object) = report.as_object_mut()
    {
      object.insert("outcome".to_owned(), Value::String("failed".to_owned()));
      object.insert(
        "code".to_owned(),
        Value::String("MODEL_CLEANUP_FAILED".to_owned()),
      );
      object.insert("cleanup".to_owned(), Value::String("failed".to_owned()));
      object.insert(
        "diagnostic".to_owned(),
        json!({"kind": "native-admission", "message": error.to_string()}),
      );
    }
    if report["cleanup"] == "failed" {
      value = Value::Null;
    }
    InvocationResult { value, report }
  }
}

fn required_object<'a>(
  object: &'a Map<String, Value>,
  name: &str,
) -> std::result::Result<&'a Map<String, Value>, NativeError> {
  object
    .get(name)
    .and_then(Value::as_object)
    .ok_or_else(|| NativeError::InvalidFields(vec![name.to_owned()]))
}

fn required_string(
  object: &Map<String, Value>,
  name: &str,
) -> std::result::Result<String, NativeError> {
  object
    .get(name)
    .and_then(Value::as_str)
    .map(ToOwned::to_owned)
    .ok_or_else(|| NativeError::InvalidFields(vec![name.to_owned()]))
}

fn string_array(object: &Map<String, Value>, name: &str) -> std::result::Result<(), NativeError> {
  let values = object
    .get(name)
    .and_then(Value::as_array)
    .ok_or_else(|| NativeError::InvalidFields(vec![name.to_owned()]))?;
  if values.iter().any(|value| !value.is_string()) {
    return Err(NativeError::InvalidFields(vec![name.to_owned()]));
  }
  Ok(())
}

fn required_nonnegative_integer(
  object: &Map<String, Value>,
  name: &str,
) -> std::result::Result<i64, NativeError> {
  object
    .get(name)
    .and_then(as_nonnegative_integer)
    .ok_or_else(|| NativeError::InvalidFields(vec![name.to_owned()]))
}

fn optional_nonnegative_integer(
  object: &Map<String, Value>,
  name: &str,
) -> std::result::Result<Option<i64>, NativeError> {
  match object.get(name) {
    None => Ok(None),
    Some(value) => as_nonnegative_integer(value)
      .map(Some)
      .ok_or_else(|| NativeError::InvalidFields(vec![name.to_owned()])),
  }
}

fn as_integer(value: &Value) -> Option<i64> {
  if let Some(value) = value.as_i64() {
    return (-9_007_199_254_740_991..=9_007_199_254_740_991)
      .contains(&value)
      .then_some(value);
  }
  let value = value.as_f64()?;
  (value.is_finite()
    && value.fract() == 0.0
    && (-9_007_199_254_740_991.0..=9_007_199_254_740_991.0).contains(&value))
  .then(|| value.to_i64())
  .flatten()
}

fn as_nonnegative_integer(value: &Value) -> Option<i64> {
  as_integer(value).filter(|value| *value >= 0)
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::os::unix::fs::PermissionsExt;

  #[test]
  fn leaves_structured_output_parsing_to_the_knowledge_layer() {
    let directory =
      std::env::temp_dir().join(format!("hivex-native-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&directory).unwrap();
    let binary = directory.join("codex-fixture");
    let fixture = std::env::var_os("HIVEX_TEST_CODEX_BINARY").map_or_else(
      || {
        std::env::current_exe()
          .unwrap()
          .parent()
          .unwrap()
          .parent()
          .unwrap()
          .join("hivex-test-codex")
      },
      PathBuf::from,
    );
    let quoted = fixture.to_string_lossy().replace('\'', "'\"'\"'");
    fs::write(
      &binary,
      format!("#!/bin/sh\nHIVEX_TEST_SCENARIO=invalid-json exec '{quoted}' \"$@\"\n"),
    )
    .unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let result = invoke(
      &InvocationOptions {
        binary: binary.to_string_lossy().into_owned(),
        profile: default_profile(),
        prompt:
          "Return JSON.\n\n{\"operation\":\"extract\",\"targets\":[],\"units\":[],\"documents\":[]}"
            .to_owned(),
        schema: json!({"type": "object"}),
        deadline_ms: 30_000,
      },
      |_| Ok(()),
    )
    .unwrap();
    fs::remove_dir_all(directory).unwrap();
    assert_eq!(result.value, Value::String("{broken".to_owned()));
    assert_eq!(result.report["outcome"], "completed");
    assert_eq!(result.report["cleanup"], "confirmed");
  }

  #[test]
  fn ignores_unknown_notifications_and_rejects_malformed_item_text() {
    let mut transcript = Transcript::new();
    assert!(
      transcript
        .notification("future/event", None)
        .unwrap()
        .is_none()
    );
    assert!(
      transcript
        .notification("future/event", Some(json!(3)))
        .unwrap()
        .is_none()
    );
    let error = transcript.notification("item/completed", Some(json!({"threadId":"thread1","turnId":"turn1","item":{"id":"item1","type":"agentMessage","text":3}}))).unwrap_err();
    assert_eq!(
      error.diagnostic(),
      json!({"kind":"invalid-native-response","fields":["item.text"]})
    );
  }
}
