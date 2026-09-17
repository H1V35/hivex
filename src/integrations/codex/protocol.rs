use super::{
  Arc, AtomicOrdering, CANCELLED, ChildStdin, ChildStdout, Duration, Instant, JoinHandle, Map,
  Mutex, NativeError, RPC_FRAME_LIMIT, RPC_STREAM_LIMIT, Read, Value, Write, as_integer, json,
  mpsc, optional_nonnegative_integer, required_nonnegative_integer, required_object,
  required_string, thread,
};

#[derive(Debug)]
pub(super) enum Incoming {
  Frame(Value),
  Failure(String),
  Eof,
}

#[derive(Clone, Debug)]
pub(super) struct RpcId(Value);

impl RpcId {
  pub(super) fn number(value: i64) -> Self {
    Self(Value::Number(value.into()))
  }
}

pub(super) struct ResponseWait<'a> {
  pub(super) timeout: Duration,
  pub(super) transcript: &'a mut Transcript,
  pub(super) honor_cancel: bool,
}

pub(super) struct Rpc {
  pub(super) writer: Option<Arc<Mutex<ChildStdin>>>,
  pub(super) incoming: mpsc::Receiver<Incoming>,
  pub(super) reader: Option<JoinHandle<()>>,
  pub(super) next_id: i64,
}

impl Rpc {
  pub(super) fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
    let (sender, incoming) = mpsc::channel();
    let reader = thread::spawn(move || read_frames(stdout, &sender));
    Self {
      writer: Some(Arc::new(Mutex::new(stdin))),
      incoming,
      reader: Some(reader),
      next_id: 1,
    }
  }

  pub(super) fn close_input(&mut self) {
    self.writer.take();
  }

  pub(super) fn notify(
    &self,
    method: &str,
    params: Option<Value>,
  ) -> std::result::Result<(), NativeError> {
    self.send_frame(None, method, params)
  }

  pub(super) fn request(
    &mut self,
    method: &str,
    params: Option<Value>,
    wait: ResponseWait<'_>,
  ) -> std::result::Result<Value, NativeError> {
    let ResponseWait {
      timeout,
      transcript,
      honor_cancel,
    } = wait;
    let id = self.next_id;
    self.next_id = self
      .next_id
      .checked_add(1)
      .ok_or_else(|| NativeError::Protocol("RPC request ID overflow".to_owned()))?;
    let rpc_id = RpcId::number(id);
    self.send_frame(Some(&rpc_id), method, params)?;
    let deadline = Instant::now() + timeout;
    loop {
      if honor_cancel && CANCELLED.load(AtomicOrdering::SeqCst) {
        return Err(NativeError::Cancelled);
      }
      let remaining = deadline.saturating_duration_since(Instant::now());
      if remaining.is_zero() {
        return Err(NativeError::Timeout);
      }
      match self
        .incoming
        .recv_timeout(remaining.min(Duration::from_millis(50)))
      {
        Ok(Incoming::Frame(frame)) => {
          if let Some(result) = self.handle_frame(&frame, &rpc_id, transcript)? {
            return Ok(result);
          }
        }
        Ok(Incoming::Failure(message)) => return Err(NativeError::Protocol(message)),
        Ok(Incoming::Eof) | Err(mpsc::RecvTimeoutError::Disconnected) => {
          return Err(NativeError::Protocol(
            "app-server connection closed".to_owned(),
          ));
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {}
      }
    }
  }

  pub(super) fn wait_terminal(
    &mut self,
    expected_thread: &str,
    expected_turn: &str,
    wait: ResponseWait<'_>,
  ) -> std::result::Result<Terminal, NativeError> {
    let ResponseWait {
      timeout,
      transcript,
      honor_cancel,
    } = wait;
    if let Some(terminal) = transcript.terminals.clone() {
      return terminal.validate_identity(expected_thread, expected_turn);
    }
    let deadline = Instant::now() + timeout;
    loop {
      if honor_cancel && CANCELLED.load(AtomicOrdering::SeqCst) {
        return Err(NativeError::Cancelled);
      }
      let remaining = deadline.saturating_duration_since(Instant::now());
      if remaining.is_zero() {
        return Err(NativeError::Timeout);
      }
      match self
        .incoming
        .recv_timeout(remaining.min(Duration::from_millis(50)))
      {
        Ok(Incoming::Frame(frame)) => {
          if let Some(terminal) = self.handle_notification_frame(&frame, transcript)? {
            return terminal.validate_identity(expected_thread, expected_turn);
          }
        }
        Ok(Incoming::Failure(message)) => return Err(NativeError::Protocol(message)),
        Ok(Incoming::Eof) | Err(mpsc::RecvTimeoutError::Disconnected) => {
          return Err(NativeError::Protocol(
            "app-server connection closed".to_owned(),
          ));
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {}
      }
    }
  }

  pub(super) fn drain(
    &mut self,
    transcript: &mut Transcript,
    grace: Duration,
  ) -> std::result::Result<(), NativeError> {
    let deadline = Instant::now() + grace;
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
      if remaining.is_zero() {
        break;
      }
      match self
        .incoming
        .recv_timeout(remaining.min(Duration::from_millis(10)))
      {
        Ok(Incoming::Frame(frame)) => {
          let _ = self.handle_notification_frame(&frame, transcript)?;
        }
        Ok(Incoming::Failure(message)) => return Err(NativeError::Protocol(message)),
        Ok(Incoming::Eof)
        | Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => break,
      }
    }
    Ok(())
  }

  pub(super) fn send_frame(
    &self,
    id: Option<&RpcId>,
    method: &str,
    params: Option<Value>,
  ) -> std::result::Result<(), NativeError> {
    let writer = self
      .writer
      .as_ref()
      .ok_or_else(|| NativeError::Protocol("app-server input is closed".to_owned()))?;
    let mut frame = Map::new();
    if let Some(id) = id {
      frame.insert("id".to_owned(), id.0.clone());
    }
    frame.insert("method".to_owned(), Value::String(method.to_owned()));
    if let Some(params) = params {
      frame.insert("params".to_owned(), params);
    }
    let line = serde_json::to_vec(&Value::Object(frame))?;
    let mut writer = writer
      .lock()
      .map_err(|_| NativeError::Protocol("app-server input lock was poisoned".to_owned()))?;
    writer.write_all(&line)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
  }

  pub(super) fn send_interaction_response(
    &self,
    id: RpcId,
    method: &str,
  ) -> std::result::Result<(), NativeError> {
    if method == "item/commandExecution/requestApproval"
      || method == "item/fileChange/requestApproval"
    {
      let mut frame = Map::new();
      frame.insert("id".to_owned(), id.0);
      frame.insert("result".to_owned(), json!({"decision": "decline"}));
      return self.send_raw(&Value::Object(frame));
    }
    let mut frame = Map::new();
    frame.insert(
      "error".to_owned(),
      json!({
          "code": -32_601,
          "message": "interactive requests are outside this pilot"
      }),
    );
    frame.insert("id".to_owned(), id.0);
    self.send_raw(&Value::Object(frame))
  }

  pub(super) fn send_raw(&self, frame: &Value) -> std::result::Result<(), NativeError> {
    let writer = self
      .writer
      .as_ref()
      .ok_or_else(|| NativeError::Protocol("app-server input is closed".to_owned()))?;
    let mut writer = writer
      .lock()
      .map_err(|_| NativeError::Protocol("app-server input lock was poisoned".to_owned()))?;
    writer.write_all(&serde_json::to_vec(&frame)?)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
  }

  pub(super) fn handle_frame(
    &self,
    frame: &Value,
    expected_id: &RpcId,
    transcript: &mut Transcript,
  ) -> std::result::Result<Option<Value>, NativeError> {
    let object = frame
      .as_object()
      .ok_or_else(|| NativeError::Protocol("app-server frame is not an object".to_owned()))?;
    if let Some(id) = object.get("id")
      && !is_rpc_id(id)
    {
      return Err(NativeError::Protocol(
        "app-server frame ID is invalid".to_owned(),
      ));
    }
    if object
      .get("method")
      .is_some_and(|method| !method.is_string())
    {
      return Err(NativeError::Protocol(
        "app-server frame method is invalid".to_owned(),
      ));
    }
    let method = object.get("method").and_then(Value::as_str);
    if let Some(method) = method {
      if object.contains_key("result") || object.contains_key("error") {
        return Err(NativeError::Protocol(
          "a method frame cannot also be a response".to_owned(),
        ));
      }
      if let Some(id) = object.get("id") {
        self.send_interaction_response(RpcId(id.clone()), method)?;
      } else {
        transcript.notification(method, object.get("params").cloned())?;
      }
      return Ok(None);
    }
    let id = object
      .get("id")
      .ok_or_else(|| NativeError::Protocol("a response requires an id".to_owned()))?;
    if object.contains_key("result") == object.contains_key("error") {
      return Err(NativeError::Protocol(
        "a response requires exactly one outcome".to_owned(),
      ));
    }
    if !expected_id_matches(id, expected_id) {
      return Ok(None);
    }
    if let Some(error) = object.get("error") {
      return Err(parse_remote_error(error));
    }
    Ok(object.get("result").cloned())
  }

  pub(super) fn handle_notification_frame(
    &self,
    frame: &Value,
    transcript: &mut Transcript,
  ) -> std::result::Result<Option<Terminal>, NativeError> {
    let object = frame
      .as_object()
      .ok_or_else(|| NativeError::Protocol("app-server frame is not an object".to_owned()))?;
    if let Some(id) = object.get("id")
      && !is_rpc_id(id)
    {
      return Err(NativeError::Protocol(
        "app-server frame ID is invalid".to_owned(),
      ));
    }
    if object
      .get("method")
      .is_some_and(|method| !method.is_string())
    {
      return Err(NativeError::Protocol(
        "app-server frame method is invalid".to_owned(),
      ));
    }
    if let Some(method) = object.get("method").and_then(Value::as_str) {
      if object.contains_key("result") || object.contains_key("error") {
        return Err(NativeError::Protocol(
          "a method frame cannot also be a response".to_owned(),
        ));
      }
      if let Some(id) = object.get("id") {
        self.send_interaction_response(RpcId(id.clone()), method)?;
        return Ok(None);
      }
      let params = object.get("params").cloned().unwrap_or(Value::Null);
      let terminal = transcript.notification(method, Some(params))?;
      return Ok(terminal);
    }
    if object.get("id").is_none() || object.contains_key("result") == object.contains_key("error") {
      return Err(NativeError::Protocol(
        "a response requires exactly one outcome".to_owned(),
      ));
    }
    Ok(None)
  }
}

impl Drop for Rpc {
  fn drop(&mut self) {
    self.close_input();
    self.reader.take();
  }
}

pub(super) fn expected_id_matches(value: &Value, expected: &RpcId) -> bool {
  match (&expected.0, value) {
    (Value::Number(expected), Value::Number(actual)) => expected == actual,
    (Value::String(expected), Value::String(actual)) => expected == actual,
    _ => false,
  }
}

pub(super) fn is_rpc_id(value: &Value) -> bool {
  value.is_string() || as_integer(value).is_some()
}

pub(super) fn parse_remote_error(value: &Value) -> NativeError {
  let Some(object) = value.as_object() else {
    return NativeError::Protocol("remote error is not an object".to_owned());
  };
  let Some(code) = object.get("code").and_then(as_integer) else {
    return NativeError::Protocol("remote error code is invalid".to_owned());
  };
  let Some(message) = object.get("message").and_then(Value::as_str) else {
    return NativeError::Protocol("remote error message is invalid".to_owned());
  };
  NativeError::Remote {
    code,
    message: message.to_owned(),
  }
}

fn read_frames(stdout: ChildStdout, sender: &mpsc::Sender<Incoming>) {
  let ending = match read_stream(stdout, sender) {
    Ok(()) => Incoming::Eof,
    Err(error) => Incoming::Failure(error),
  };
  let _ = sender.send(ending);
}

fn read_stream(
  mut stdout: ChildStdout,
  sender: &mpsc::Sender<Incoming>,
) -> std::result::Result<(), String> {
  let mut chunk = [0_u8; 8192];
  let mut frame = Vec::new();
  let mut stream_bytes = 0_usize;
  loop {
    let read = stdout.read(&mut chunk).map_err(|error| error.to_string())?;
    if read == 0 {
      if !frame.is_empty() {
        send_parsed_frame(&frame, sender)?;
      }
      return Ok(());
    }
    stream_bytes = stream_bytes.saturating_add(read);
    if stream_bytes > RPC_STREAM_LIMIT {
      return Err("app-server stream exceeded 32 MiB".to_owned());
    }
    for byte in &chunk[..read] {
      if *byte == b'\n' {
        send_parsed_frame(&frame, sender)?;
        frame.clear();
        continue;
      }
      frame.push(*byte);
      if frame.len() > RPC_FRAME_LIMIT {
        return Err("app-server frame exceeded 4 MiB".to_owned());
      }
    }
  }
}

fn send_parsed_frame(
  frame: &[u8],
  sender: &mpsc::Sender<Incoming>,
) -> std::result::Result<(), String> {
  if frame.len() > RPC_FRAME_LIMIT {
    return Err("app-server frame exceeded 4 MiB".to_owned());
  }
  let value = serde_json::from_slice(frame).map_err(|_| "invalid app-server frame".to_owned())?;
  sender
    .send(Incoming::Frame(value))
    .map_err(|error| error.to_string())
}

#[derive(Clone, Debug)]
pub(super) struct Usage {
  pub(super) cache_write_input: Option<i64>,
  pub(super) cached_input: i64,
  pub(super) input: i64,
  pub(super) output: i64,
  pub(super) reasoning_output: i64,
  pub(super) total: i64,
}

impl Usage {
  pub(super) fn value(&self) -> Value {
    let mut object = Map::new();
    if let Some(value) = self.cache_write_input {
      object.insert("cacheWriteInputTokens".to_owned(), json!(value));
    }
    object.insert("cachedInputTokens".to_owned(), json!(self.cached_input));
    object.insert("inputTokens".to_owned(), json!(self.input));
    object.insert("outputTokens".to_owned(), json!(self.output));
    object.insert(
      "reasoningOutputTokens".to_owned(),
      json!(self.reasoning_output),
    );
    object.insert("totalTokens".to_owned(), json!(self.total));
    Value::Object(object)
  }

  pub(super) fn consistent_with(&self, previous: Option<&Self>) -> bool {
    let Some(input_plus_output) = self.input.checked_add(self.output) else {
      return false;
    };
    if self.total != input_plus_output
      || self.cached_input > self.input
      || self.reasoning_output > self.output
      || self
        .cache_write_input
        .is_some_and(|value| value > self.input)
    {
      return false;
    }
    let Some(previous) = previous else {
      return true;
    };
    self.input >= previous.input && self.output >= previous.output && self.total >= previous.total
  }
}

#[derive(Clone, Debug)]
pub(super) struct Terminal {
  pub(super) thread_id: String,
  pub(super) turn_id: String,
  pub(super) status: String,
}

pub(super) struct Transcript {
  pub(super) identity: Option<(String, String)>,
  pub(super) terminal_seen: bool,
  pub(super) invalid: bool,
  pub(super) usage_invalid: bool,
  pub(super) usage: Option<Usage>,
  pub(super) terminals: Option<Terminal>,
  pub(super) items: Vec<(String, String, String, String)>,
}

impl Transcript {
  pub(super) fn new() -> Self {
    Self {
      identity: None,
      terminal_seen: false,
      invalid: false,
      usage_invalid: false,
      usage: None,
      terminals: None,
      items: Vec::new(),
    }
  }

  pub(super) fn notification(
    &mut self,
    method: &str,
    params: Option<Value>,
  ) -> std::result::Result<Option<Terminal>, NativeError> {
    let result = self.receive(method, &params.unwrap_or(Value::Null));
    if result.is_err() {
      self.invalid = true;
    }
    result
  }

  pub(super) fn receive(
    &mut self,
    method: &str,
    params: &Value,
  ) -> std::result::Result<Option<Terminal>, NativeError> {
    if !matches!(
      method,
      "turn/completed" | "thread/tokenUsage/updated" | "item/completed" | "item/started"
    ) {
      return Ok(None);
    }
    let object = params
      .as_object()
      .ok_or_else(|| NativeError::InvalidFields(vec![String::new()]))?;
    match method {
      "turn/completed" => self.receive_terminal(object),
      "thread/tokenUsage/updated" => self.receive_usage(object),
      "item/completed" | "item/started" => {
        let thread_id = required_string(object, "threadId")?;
        let turn_id = required_string(object, "turnId")?;
        self.track_identity(&thread_id, &turn_id)?;
        let item = required_object(object, "item")?;
        let item_id = required_string(item, "id").map_err(|error| error.at("item"))?;
        let item_type = required_string(item, "type").map_err(|error| error.at("item"))?;
        if !matches!(
          item_type.as_str(),
          "userMessage" | "reasoning" | "agentMessage"
        ) {
          return Err(NativeError::Protocol(
            "Unexpected knowledge-model tool activity".to_owned(),
          ));
        }
        if item.get("text").is_some_and(|text| !text.is_string()) {
          return Err(NativeError::InvalidFields(vec!["item.text".to_owned()]));
        }
        if method == "item/completed" && item_type == "agentMessage" {
          let text = item.get("text").and_then(Value::as_str).unwrap_or_default();
          self
            .items
            .push((item_id, thread_id, turn_id, text.to_owned()));
        }
        Ok(None)
      }
      _ => Ok(None),
    }
  }

  fn receive_terminal(
    &mut self,
    object: &Map<String, Value>,
  ) -> std::result::Result<Option<Terminal>, NativeError> {
    let thread_id = required_string(object, "threadId")?;
    let turn = required_object(object, "turn")?;
    let turn_id = required_string(turn, "id").map_err(|error| error.at("turn"))?;
    let status = required_string(turn, "status").map_err(|error| error.at("turn"))?;
    if !matches!(status.as_str(), "completed" | "failed" | "interrupted") {
      return Err(NativeError::Protocol(
        "native terminal status is invalid".to_owned(),
      ));
    }
    self.track_identity(&thread_id, &turn_id)?;
    if self.terminal_seen {
      return Err(NativeError::Protocol(
        "Duplicate native terminal event".to_owned(),
      ));
    }
    self.terminal_seen = true;
    let terminal = Terminal {
      thread_id,
      turn_id,
      status,
    };
    self.terminals = Some(terminal.clone());
    Ok(Some(terminal))
  }

  fn receive_usage(
    &mut self,
    object: &Map<String, Value>,
  ) -> std::result::Result<Option<Terminal>, NativeError> {
    let thread_id = required_string(object, "threadId")?;
    let turn_id = required_string(object, "turnId")?;
    self.track_identity(&thread_id, &turn_id)?;
    let usage_object = required_object(object, "tokenUsage")?;
    let total = required_object(usage_object, "total").map_err(|error| error.at("tokenUsage"))?;
    let usage = Usage {
      cache_write_input: optional_nonnegative_integer(total, "cacheWriteInputTokens")
        .map_err(|error| error.at("tokenUsage.total"))?,
      cached_input: required_nonnegative_integer(total, "cachedInputTokens")
        .map_err(|error| error.at("tokenUsage.total"))?,
      input: required_nonnegative_integer(total, "inputTokens")
        .map_err(|error| error.at("tokenUsage.total"))?,
      output: required_nonnegative_integer(total, "outputTokens")
        .map_err(|error| error.at("tokenUsage.total"))?,
      reasoning_output: required_nonnegative_integer(total, "reasoningOutputTokens")
        .map_err(|error| error.at("tokenUsage.total"))?,
      total: required_nonnegative_integer(total, "totalTokens")
        .map_err(|error| error.at("tokenUsage.total"))?,
    };
    if !usage.consistent_with(self.usage.as_ref()) {
      self.usage_invalid = true;
      return Err(NativeError::Protocol(
        "Native usage is inconsistent or regressed".to_owned(),
      ));
    }
    self.usage = Some(usage);
    Ok(None)
  }

  pub(super) fn track_identity(
    &mut self,
    thread_id: &str,
    turn_id: &str,
  ) -> std::result::Result<(), NativeError> {
    if let Some((current_thread, current_turn)) = &self.identity {
      if current_thread != thread_id || current_turn != turn_id {
        return Err(NativeError::Protocol(
          "Native evidence mixed thread or turn identities".to_owned(),
        ));
      }
    } else {
      self.identity = Some((thread_id.to_owned(), turn_id.to_owned()));
    }
    Ok(())
  }

  pub(super) fn measured(&self, thread_id: &str, turn_id: &str) -> Value {
    if self.usage_invalid
      || self.identity.as_ref() != Some(&(thread_id.to_owned(), turn_id.to_owned()))
    {
      return Value::Null;
    }
    self.usage.as_ref().map_or(Value::Null, Usage::value)
  }

  pub(super) fn assert_valid(
    &self,
    thread_id: &str,
    turn_id: &str,
  ) -> std::result::Result<(), NativeError> {
    if self.invalid || self.identity.as_ref() != Some(&(thread_id.to_owned(), turn_id.to_owned())) {
      return Err(NativeError::Protocol(
        "Native evidence did not retain one consistent invocation identity".to_owned(),
      ));
    }
    Ok(())
  }

  pub(super) fn final_text(&self, thread_id: &str, turn_id: &str) -> Option<&str> {
    self
      .items
      .iter()
      .rev()
      .find(|(_, item_thread, item_turn, _)| item_thread == thread_id && item_turn == turn_id)
      .map(|(_, _, _, text)| text.as_str())
      .filter(|text| !text.is_empty())
  }
}

impl Terminal {
  fn validate_identity(self, thread: &str, turn: &str) -> std::result::Result<Self, NativeError> {
    if self.thread_id != thread || self.turn_id != turn {
      return Err(NativeError::Protocol(
        "Model completion used the wrong thread or turn identity".to_owned(),
      ));
    }
    Ok(self)
  }
}
