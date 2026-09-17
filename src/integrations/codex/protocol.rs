use super::*;

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

pub(super) struct Rpc {
    pub(super) writer: Option<Arc<Mutex<ChildStdin>>>,
    pub(super) incoming: mpsc::Receiver<Incoming>,
    pub(super) reader: Option<JoinHandle<()>>,
    pub(super) next_id: i64,
}

impl Rpc {
    pub(super) fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
        let (sender, incoming) = mpsc::channel();
        let reader = thread::spawn(move || read_frames(stdout, sender));
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
        timeout: Duration,
        transcript: &mut Transcript,
        honor_cancel: bool,
    ) -> std::result::Result<Value, NativeError> {
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
                    if let Some(result) = self.handle_frame(frame, &rpc_id, transcript)? {
                        return Ok(result);
                    }
                }
                Ok(Incoming::Failure(message)) => return Err(NativeError::Protocol(message)),
                Ok(Incoming::Eof) => {
                    return Err(NativeError::Protocol(
                        "app-server connection closed".to_owned(),
                    ));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(NativeError::Protocol(
                        "app-server connection closed".to_owned(),
                    ));
                }
            }
        }
    }

    pub(super) fn wait_terminal(
        &mut self,
        expected_thread: &str,
        expected_turn: &str,
        timeout: Duration,
        transcript: &mut Transcript,
        honor_cancel: bool,
    ) -> std::result::Result<Terminal, NativeError> {
        if let Some(terminal) = transcript.terminals.clone() {
            if terminal.thread_id != expected_thread || terminal.turn_id != expected_turn {
                return Err(NativeError::Protocol(
                    "Model completion used the wrong thread or turn identity".to_owned(),
                ));
            }
            return Ok(terminal);
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
                    if let Some(terminal) = self.handle_notification_frame(frame, transcript)? {
                        if terminal.thread_id != expected_thread
                            || terminal.turn_id != expected_turn
                        {
                            return Err(NativeError::Protocol(
                                "Model completion used the wrong thread or turn identity"
                                    .to_owned(),
                            ));
                        }
                        return Ok(terminal);
                    }
                }
                Ok(Incoming::Failure(message)) => return Err(NativeError::Protocol(message)),
                Ok(Incoming::Eof) => {
                    return Err(NativeError::Protocol(
                        "app-server connection closed".to_owned(),
                    ));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(NativeError::Protocol(
                        "app-server connection closed".to_owned(),
                    ));
                }
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
                    let _ = self.handle_notification_frame(frame, transcript)?;
                }
                Ok(Incoming::Failure(message)) => return Err(NativeError::Protocol(message)),
                Ok(Incoming::Eof) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
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
            return self.send_raw(Value::Object(frame));
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
        self.send_raw(Value::Object(frame))
    }

    pub(super) fn send_raw(&self, frame: Value) -> std::result::Result<(), NativeError> {
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
        frame: Value,
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
        frame: Value,
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
        if object.get("id").is_none()
            || object.contains_key("result") == object.contains_key("error")
        {
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

pub(super) fn read_frames(mut stdout: ChildStdout, sender: mpsc::Sender<Incoming>) {
    let mut chunk = [0_u8; 8192];
    let mut frame = Vec::new();
    let mut stream_bytes = 0_usize;
    loop {
        let read = match stdout.read(&mut chunk) {
            Ok(read) => read,
            Err(error) => {
                let _ = sender.send(Incoming::Failure(error.to_string()));
                return;
            }
        };
        if read == 0 {
            if !frame.is_empty() {
                if frame.len() > RPC_FRAME_LIMIT {
                    let _ = sender.send(Incoming::Failure(
                        "app-server frame exceeded 4 MiB".to_owned(),
                    ));
                    return;
                }
                match serde_json::from_slice::<Value>(&frame) {
                    Ok(value) => {
                        let _ = sender.send(Incoming::Frame(value));
                    }
                    Err(_) => {
                        let _ =
                            sender.send(Incoming::Failure("invalid app-server frame".to_owned()));
                        return;
                    }
                }
            }
            let _ = sender.send(Incoming::Eof);
            return;
        }
        stream_bytes = stream_bytes.saturating_add(read);
        if stream_bytes > RPC_STREAM_LIMIT {
            let _ = sender.send(Incoming::Failure(
                "app-server stream exceeded 32 MiB".to_owned(),
            ));
            return;
        }
        for byte in &chunk[..read] {
            if *byte == b'\n' {
                if frame.len() > RPC_FRAME_LIMIT {
                    let _ = sender.send(Incoming::Failure(
                        "app-server frame exceeded 4 MiB".to_owned(),
                    ));
                    return;
                }
                let parsed = serde_json::from_slice::<Value>(&frame);
                frame.clear();
                match parsed {
                    Ok(value) => {
                        if sender.send(Incoming::Frame(value)).is_err() {
                            return;
                        }
                    }
                    Err(_) => {
                        let _ =
                            sender.send(Incoming::Failure("invalid app-server frame".to_owned()));
                        return;
                    }
                }
            } else {
                frame.push(*byte);
                if frame.len() > RPC_FRAME_LIMIT {
                    let _ = sender.send(Incoming::Failure(
                        "app-server frame exceeded 4 MiB".to_owned(),
                    ));
                    return;
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Usage {
    pub(super) cache_write_input_tokens: Option<i64>,
    pub(super) cached_input_tokens: i64,
    pub(super) input_tokens: i64,
    pub(super) output_tokens: i64,
    pub(super) reasoning_output_tokens: i64,
    pub(super) total_tokens: i64,
}

impl Usage {
    pub(super) fn value(&self) -> Value {
        let mut object = Map::new();
        if let Some(value) = self.cache_write_input_tokens {
            object.insert("cacheWriteInputTokens".to_owned(), json!(value));
        }
        object.insert(
            "cachedInputTokens".to_owned(),
            json!(self.cached_input_tokens),
        );
        object.insert("inputTokens".to_owned(), json!(self.input_tokens));
        object.insert("outputTokens".to_owned(), json!(self.output_tokens));
        object.insert(
            "reasoningOutputTokens".to_owned(),
            json!(self.reasoning_output_tokens),
        );
        object.insert("totalTokens".to_owned(), json!(self.total_tokens));
        Value::Object(object)
    }

    pub(super) fn consistent_with(&self, previous: Option<&Self>) -> bool {
        let Some(input_plus_output) = self.input_tokens.checked_add(self.output_tokens) else {
            return false;
        };
        if self.total_tokens != input_plus_output
            || self.cached_input_tokens > self.input_tokens
            || self.reasoning_output_tokens > self.output_tokens
            || self
                .cache_write_input_tokens
                .is_some_and(|value| value > self.input_tokens)
        {
            return false;
        }
        let Some(previous) = previous else {
            return true;
        };
        self.input_tokens >= previous.input_tokens
            && self.output_tokens >= previous.output_tokens
            && self.total_tokens >= previous.total_tokens
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
        let result = self.receive(method, params.unwrap_or(Value::Null));
        if result.is_err() {
            self.invalid = true;
        }
        result
    }

    pub(super) fn receive(
        &mut self,
        method: &str,
        params: Value,
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
            "turn/completed" => {
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
            "thread/tokenUsage/updated" => {
                let thread_id = required_string(object, "threadId")?;
                let turn_id = required_string(object, "turnId")?;
                self.track_identity(&thread_id, &turn_id)?;
                let usage_object = required_object(object, "tokenUsage")?;
                let total = required_object(usage_object, "total")
                    .map_err(|error| error.at("tokenUsage"))?;
                let usage = Usage {
                    cache_write_input_tokens: optional_nonnegative_integer(
                        total,
                        "cacheWriteInputTokens",
                    )
                    .map_err(|error| error.at("tokenUsage.total"))?,
                    cached_input_tokens: required_nonnegative_integer(total, "cachedInputTokens")
                        .map_err(|error| error.at("tokenUsage.total"))?,
                    input_tokens: required_nonnegative_integer(total, "inputTokens")
                        .map_err(|error| error.at("tokenUsage.total"))?,
                    output_tokens: required_nonnegative_integer(total, "outputTokens")
                        .map_err(|error| error.at("tokenUsage.total"))?,
                    reasoning_output_tokens: required_nonnegative_integer(
                        total,
                        "reasoningOutputTokens",
                    )
                    .map_err(|error| error.at("tokenUsage.total"))?,
                    total_tokens: required_nonnegative_integer(total, "totalTokens")
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
                    self.items
                        .push((item_id, thread_id, turn_id, text.to_owned()));
                }
                Ok(None)
            }
            _ => Ok(None),
        }
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
        if self.invalid
            || self.identity.as_ref() != Some(&(thread_id.to_owned(), turn_id.to_owned()))
        {
            return Err(NativeError::Protocol(
                "Native evidence did not retain one consistent invocation identity".to_owned(),
            ));
        }
        Ok(())
    }

    pub(super) fn final_text(&self, thread_id: &str, turn_id: &str) -> Option<&str> {
        self.items
            .iter()
            .rev()
            .find(|(_, item_thread, item_turn, _)| item_thread == thread_id && item_turn == turn_id)
            .map(|(_, _, _, text)| text.as_str())
            .filter(|text| !text.is_empty())
    }
}
