use crate::error::{HivexError, Result};
use crate::knowledge_model::{self, graph_value, parse_graph};
use crate::knowledge_warning_review::warning_baseline;
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use serde_json::{Map, Value, json};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;

pub(crate) const LOCK_FILENAME: &str = "knowledge.lock";

#[derive(Clone, Copy, Debug, Default)]
pub struct StoreOptions {
    pub readonly: bool,
    pub update: bool,
}

#[derive(Clone, Debug)]
pub struct BeginWork {
    pub key: String,
    pub kind: String,
    pub max_calls: Option<u64>,
    pub max_input_bytes: Option<u64>,
    pub remaining: Vec<String>,
    pub result_key: Option<String>,
    pub snapshot: String,
    pub warning_baseline: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct Work {
    row_id: String,
    value: Value,
}

impl Work {
    pub fn value(&self) -> &Value {
        &self.value
    }

    pub fn value_mut(&mut self) -> &mut Value {
        &mut self.value
    }

    pub fn id(&self) -> &str {
        self.value
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(&self.row_id)
    }

    pub fn row_id(&self) -> &str {
        &self.row_id
    }

    pub fn kind(&self) -> &str {
        self.value
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default()
    }

    pub fn key(&self) -> &str {
        self.value
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or_default()
    }

    pub fn status(&self) -> &str {
        self.value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default()
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
        self.value
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
        self.value
            .get("nativeProcessId")
            .and_then(positive_integer)
            .and_then(|pid| u32::try_from(pid).ok())
    }

    pub fn owner_pid(&self) -> Option<u32> {
        self.value
            .get("ownerPid")
            .and_then(positive_integer)
            .and_then(|pid| u32::try_from(pid).ok())
    }

    fn status_or_error(&self) -> Result<&str> {
        let status = self.status();
        if matches!(
            status,
            "pending" | "running" | "budget-exhausted" | "context-limit" | "failed" | "done"
        ) {
            Ok(status)
        } else {
            Err(HivexError::new(
                "READ_FAILED",
                format!("Work {} has an invalid status", self.id()),
            ))
        }
    }
}

pub struct Store {
    database: Connection,
    directory: PathBuf,
    _lease: Option<UpdateLease>,
}

struct UpdateLease {
    path: PathBuf,
    token: String,
    _file: File,
}

impl UpdateLease {
    fn acquire(directory: &Path) -> Result<Self> {
        let path = directory.join(LOCK_FILENAME);
        let token = json!({
            "id": Uuid::new_v4().to_string(),
            "pid": std::process::id(),
        })
        .to_string();
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path).map_err(|_| {
            HivexError::new(
                "KNOWLEDGE_LOCKED",
                "Cannot acquire the update lock; inspect any active or interrupted update before continuing",
            )
        })?;
        if let Err(error) = file.write_all(token.as_bytes()).and_then(|_| file.flush()) {
            let _ = fs::remove_file(&path);
            return Err(error.into());
        }
        Ok(Self {
            path,
            token,
            _file: file,
        })
    }
}

impl Drop for UpdateLease {
    fn drop(&mut self) {
        let Ok(contents) = fs::read_to_string(&self.path) else {
            return;
        };
        if contents == self.token {
            let _ = fs::remove_file(&self.path);
        }
    }
}

impl Store {
    pub fn open(root: &Path, options: StoreOptions) -> Result<Self> {
        if options.readonly && options.update {
            return Err(HivexError::new(
                "INVALID_STORE",
                "Knowledge storage cannot be readonly and own an update lock",
            ));
        }
        let directory = root.join(".hivex");
        reject_symlink(&directory)?;
        if options.readonly {
            if !directory.is_dir() {
                return Err(HivexError::new(
                    "READ_FAILED",
                    "Knowledge storage directory does not exist",
                ));
            }
        } else {
            let created = !directory.exists();
            fs::create_dir_all(&directory)?;
            if created {
                private_directory(&directory)?;
            }
        }
        let database_path = directory.join("knowledge.sqlite");
        reject_symlink(&database_path)?;
        let database = if options.readonly {
            Connection::open_with_flags(&database_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?
        } else {
            Connection::open(&database_path)?
        };
        if options.readonly {
            return Ok(Self {
                database,
                directory,
                _lease: None,
            });
        }
        database.busy_timeout(Duration::from_millis(1000))?;
        database.execute_batch("PRAGMA max_page_count=16384;")?;
        database.execute_batch(
            "CREATE TABLE IF NOT EXISTS graph (id INTEGER PRIMARY KEY CHECK(id=1), data TEXT NOT NULL);\
             CREATE TABLE IF NOT EXISTS work (id TEXT PRIMARY KEY, kind TEXT NOT NULL, key TEXT NOT NULL, data TEXT NOT NULL);\
             CREATE TABLE IF NOT EXISTS model_cache (key TEXT PRIMARY KEY, value TEXT NOT NULL);\
             CREATE INDEX IF NOT EXISTS work_key ON work(kind,key);",
        )?;
        let lease = if options.update {
            Some(UpdateLease::acquire(&directory)?)
        } else {
            None
        };
        Ok(Self {
            database,
            directory,
            _lease: lease,
        })
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub(crate) fn database_mut(&mut self) -> &mut Connection {
        &mut self.database
    }

    pub fn graph(&self) -> Result<Value> {
        let data = self
            .database
            .query_row("SELECT data FROM graph WHERE id=1", [], |row| {
                row.get::<_, String>(0)
            })
            .optional()?;
        let Some(data) = data else {
            if self.works()?.iter().any(|work| work.status() != "done") {
                return Ok(Self::empty_graph());
            }
            let root = self.directory.parent().unwrap_or(Path::new("."));
            return crate::knowledge_snapshot::shared_knowledge(root);
        };
        let value: Value = serde_json::from_str(&data)?;
        let graph = parse_graph(&value, false).ok_or_else(|| {
            HivexError::new(
                "INVALID_SNAPSHOT",
                "Stored graph is not a supported knowledge graph",
            )
        })?;
        Ok(graph_value(&graph, false))
    }

    pub fn save_graph(&self, graph: &Value) -> Result<()> {
        let graph = normalize_graph(graph)?;
        self.database.execute(
            "INSERT INTO graph VALUES(1,?) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
            [serde_json::to_string(&graph)?],
        )?;
        Ok(())
    }

    pub fn import_graph(&mut self, graph: &Value) -> Result<()> {
        let transaction = self
            .database
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let unfinished = read_work_rows(&transaction)?
            .iter()
            .any(|work| work.status() != "done");
        if unfinished {
            return Err(HivexError::new(
                "UNFINISHED_WORK",
                "Finish or recover existing work before importing a knowledge snapshot; its attempts and budgets are preserved.",
            ));
        }
        let graph = normalize_graph(graph)?;
        transaction.execute(
            "INSERT INTO graph VALUES(1,?) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
            [serde_json::to_string(&graph)?],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn cached(&self, key: &str) -> Result<Option<Value>> {
        let data = self
            .database
            .query_row("SELECT value FROM model_cache WHERE key=?1", [key], |row| {
                row.get::<_, String>(0)
            })
            .optional()?;
        data.map(|data| serde_json::from_str(&data).map_err(Into::into))
            .transpose()
    }

    pub fn cache(&self, key: &str, value: &Value) -> Result<()> {
        self.database.execute(
            "INSERT INTO model_cache VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, serde_json::to_string(value)?],
        )?;
        Ok(())
    }

    pub fn works(&self) -> Result<Vec<Work>> {
        read_work_rows(&self.database)
    }

    pub fn begin(&mut self, options: BeginWork) -> Result<Work> {
        let initial_graph = self
            .database
            .query_row("SELECT 1 FROM graph WHERE id=1", [], |row| {
                row.get::<_, i64>(0)
            })
            .optional()?
            .is_none()
            .then(|| self.graph())
            .transpose()?;
        let transaction = self
            .database
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if transaction
            .query_row("SELECT 1 FROM graph WHERE id=1", [], |row| {
                row.get::<_, i64>(0)
            })
            .optional()?
            .is_none()
            && let Some(graph) = &initial_graph
        {
            transaction.execute(
                "INSERT INTO graph VALUES(1,?)",
                [serde_json::to_string(graph)?],
            )?;
        }
        let previous = transaction
            .prepare(
                "SELECT id,data FROM work WHERE kind=?1 AND key=?2 ORDER BY rowid DESC LIMIT 1",
            )?
            .query_row(params![options.kind, options.key], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .optional()?;
        let previous = previous
            .map(|(row_id, data)| parse_work(row_id, &data))
            .transpose()?;
        let previous_reusable = previous.as_ref().is_some_and(|previous| {
            if options.kind == "update" {
                options.remaining.is_empty()
            } else {
                previous.result_key() == options.result_key.as_deref()
            }
        });
        if previous.is_none()
            || (!previous_reusable
                && options.kind == "update"
                && previous
                    .as_ref()
                    .is_some_and(|work| work.status() == "done"))
        {
            let mut new_options = options.clone();
            if new_options.warning_baseline.is_none() {
                let graph = if let Some(graph) = &initial_graph {
                    graph.clone()
                } else {
                    let data: String =
                        transaction
                            .query_row("SELECT data FROM graph WHERE id=1", [], |row| row.get(0))?;
                    serde_json::from_str(&data)?
                };
                new_options.warning_baseline = Some(graph_warning_baseline(&graph)?);
            }
            let mut work = new_work(&new_options)?;
            insert_work(&transaction, &mut work)?;
            transaction.commit()?;
            return Ok(work);
        }
        let mut work = previous.expect("previous exists after new-work branch");
        if previous_reusable && work.status() == "done" {
            transaction.commit()?;
            return Ok(work);
        }
        if work.status() == "running" {
            return Err(HivexError::new(
                "WORK_RUNNING",
                format!(
                    "Work {} has an unfinished invocation; inspect it before retrying",
                    work.id()
                ),
            ));
        }
        if work.status() == "done" {
            remove_field(&mut work.value, "result");
            set_string(&mut work.value, "status", "pending")?;
        }
        if let Some(max_calls) = options.max_calls {
            set_u64(&mut work.value, "maxCalls", max_calls)?;
        }
        if let Some(max_input_bytes) = options.max_input_bytes {
            set_u64(&mut work.value, "maxInputBytes", max_input_bytes)?;
        }
        save_work(&transaction, &mut work)?;
        transaction.commit()?;
        Ok(work)
    }

    pub fn save(&mut self, work: &mut Work) -> Result<()> {
        let transaction = self
            .database
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        save_work(&transaction, work)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn commit(&mut self, work: &mut Work, graph: &Value) -> Result<()> {
        let graph = normalize_graph(graph)?;
        let transaction = self
            .database
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO graph VALUES(1,?) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
            [serde_json::to_string(&graph)?],
        )?;
        save_work(&transaction, work)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn reserve(
        &mut self,
        work: &mut Work,
        input_bytes: u64,
        input_hash: &str,
        stage: &str,
    ) -> Result<()> {
        let transaction = self
            .database
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_work_by_id(&transaction, work.row_id())?;
        if current.calls() != work.calls() || current.status() == "running" {
            return Err(HivexError::new(
                "WORK_CONFLICT",
                "Work was claimed or changed by another operation",
            ));
        }
        let mut next = work.clone();
        let next_calls = next.calls().checked_add(1).ok_or_else(|| {
            HivexError::new("INVALID_WORK", "Work call count exceeds integer range")
        })?;
        let next_input_bytes = next.input_bytes().checked_add(input_bytes).ok_or_else(|| {
            HivexError::new("INVALID_WORK", "Work input bytes exceed integer range")
        })?;
        set_u64(&mut next.value, "calls", next_calls)?;
        set_u64(&mut next.value, "inputBytes", next_input_bytes)?;
        set_string(&mut next.value, "status", "running")?;
        set_u64(&mut next.value, "ownerPid", u64::from(std::process::id()))?;
        remove_field(&mut next.value, "nativeProcessId");
        remove_field(&mut next.value, "retainedCheckAssessment");
        let attempts = next
            .value
            .get_mut("attempts")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| HivexError::new("READ_FAILED", "Work attempts are not an array"))?;
        attempts.push(json!({
            "inputBytes": input_bytes,
            "inputHash": input_hash,
            "stage": stage,
        }));
        save_work(&transaction, &mut next)?;
        transaction.commit()?;
        *work = next;
        Ok(())
    }

    pub fn record_native_process(&mut self, work: &mut Work, native_process_id: u32) -> Result<()> {
        if native_process_id == 0 {
            return Err(HivexError::new(
                "INVALID_PROCESS_ID",
                "Native process ID must be a positive integer",
            ));
        }
        let transaction = self
            .database
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read_work_by_id(&transaction, work.row_id())?;
        if current.calls() != work.calls() || current.status() != "running" {
            return Err(HivexError::new(
                "WORK_CONFLICT",
                "Work was claimed or changed before the native process was recorded",
            ));
        }
        let mut next = work.clone();
        set_u64(
            &mut next.value,
            "nativeProcessId",
            u64::from(native_process_id),
        )?;
        save_work(&transaction, &mut next)?;
        transaction.commit()?;
        *work = next;
        Ok(())
    }

    pub fn unfinished(&self) -> Result<bool> {
        Ok(self.works()?.iter().any(|work| work.status() != "done"))
    }

    pub fn empty_graph() -> Value {
        graph_value(&knowledge_model::empty_graph(), false)
    }
}

fn normalize_graph(value: &Value) -> Result<Value> {
    let graph = parse_graph(value, false).ok_or_else(|| {
        HivexError::new(
            "INVALID_SNAPSHOT",
            "Stored graph is not a supported knowledge graph",
        )
    })?;
    Ok(graph_value(&graph, false))
}

fn graph_warning_baseline(value: &Value) -> Result<Value> {
    let graph = parse_graph(value, false).ok_or_else(|| {
        HivexError::new(
            "INVALID_SNAPSHOT",
            "Stored graph is not a supported knowledge graph",
        )
    })?;
    Ok(Value::Object(warning_baseline(&graph)))
}

fn new_work(options: &BeginWork) -> Result<Work> {
    if !matches!(options.kind.as_str(), "update" | "ask" | "review") {
        return Err(HivexError::new("INVALID_ARGUMENT", "Work kind is invalid"));
    }
    let max_calls = options
        .max_calls
        .unwrap_or(if options.kind == "update" { 2 } else { 3 });
    let max_input_bytes = options.max_input_bytes.unwrap_or(131_072);
    if max_input_bytes == 0 {
        return Err(HivexError::new(
            "INVALID_ARGUMENT",
            "Maximum input bytes must be positive",
        ));
    }
    let id = Uuid::new_v4().to_string();
    let mut value = Map::new();
    value.insert("id".to_owned(), Value::String(id.clone()));
    value.insert("kind".to_owned(), Value::String(options.kind.clone()));
    value.insert("key".to_owned(), Value::String(options.key.clone()));
    if let Some(result_key) = &options.result_key {
        value.insert("resultKey".to_owned(), Value::String(result_key.clone()));
    }
    value.insert(
        "snapshot".to_owned(),
        Value::String(options.snapshot.clone()),
    );
    value.insert("maxCalls".to_owned(), json!(max_calls));
    value.insert("maxInputBytes".to_owned(), json!(max_input_bytes));
    value.insert(
        "remaining".to_owned(),
        Value::Array(
            options
                .remaining
                .iter()
                .cloned()
                .map(Value::String)
                .collect(),
        ),
    );
    value.insert(
        "plannedUnits".to_owned(),
        Value::Array(
            options
                .remaining
                .iter()
                .cloned()
                .map(Value::String)
                .collect(),
        ),
    );
    value.insert("phase".to_owned(), Value::String("update".to_owned()));
    value.insert("calls".to_owned(), json!(0));
    value.insert("cacheHits".to_owned(), json!(0));
    value.insert("inputBytes".to_owned(), json!(0));
    value.insert("totalTokens".to_owned(), json!(0));
    value.insert("status".to_owned(), Value::String("pending".to_owned()));
    value.insert("pending".to_owned(), Value::Null);
    value.insert("attempts".to_owned(), Value::Array(Vec::new()));
    value.insert("materializedChecks".to_owned(), Value::Bool(true));
    if let Some(baseline) = &options.warning_baseline {
        value.insert("warningBaseline".to_owned(), baseline.clone());
    }
    Ok(Work {
        row_id: id,
        value: Value::Object(value),
    })
}

fn read_work_rows(database: &Connection) -> Result<Vec<Work>> {
    let mut statement = database.prepare("SELECT id,data FROM work ORDER BY rowid DESC")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    rows.map(|row| {
        let (row_id, data) = row?;
        parse_work(row_id, &data)
    })
    .collect()
}

fn read_work_by_id(database: &Connection, row_id: &str) -> Result<Work> {
    let data = database
        .query_row("SELECT data FROM work WHERE id=?1", [row_id], |row| {
            row.get::<_, String>(0)
        })
        .optional()?
        .ok_or_else(|| HivexError::new("WORK_CONFLICT", "Work was removed during the operation"))?;
    parse_work(row_id.to_owned(), &data)
}

pub(crate) fn parse_work(row_id: String, data: &str) -> Result<Work> {
    let value: Value = serde_json::from_str(data)?;
    validate_work_value(&value, &row_id)?;
    let work = Work { row_id, value };
    work.status_or_error()?;
    Ok(work)
}

pub(crate) fn save_work(database: &Connection, work: &mut Work) -> Result<()> {
    if work.status() != "running" {
        remove_field(&mut work.value, "nativeProcessId");
    }
    let kind = work
        .value
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| HivexError::new("READ_FAILED", "Work kind is invalid"))?;
    let key = work
        .value
        .get("key")
        .and_then(Value::as_str)
        .ok_or_else(|| HivexError::new("READ_FAILED", "Work key is invalid"))?;
    database.execute(
        "INSERT INTO work VALUES(?,?,?,?) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
        params![
            work.row_id(),
            kind,
            key,
            serde_json::to_string(&work.value)?
        ],
    )?;
    Ok(())
}

fn insert_work(database: &Connection, work: &mut Work) -> Result<()> {
    save_work(database, work)
}

fn validate_work_value(value: &Value, row_id: &str) -> Result<()> {
    let object = value
        .as_object()
        .ok_or_else(|| malformed_work(row_id, "record is not an object"))?;
    required_string(object, "id", row_id)?;
    let kind = required_string(object, "kind", row_id)?;
    if !matches!(kind.as_str(), "update" | "ask" | "review") {
        return Err(malformed_work(row_id, "kind is invalid"));
    }
    required_string(object, "key", row_id)?;
    required_string(object, "snapshot", row_id)?;
    let status = required_string(object, "status", row_id)?;
    if !matches!(
        status.as_str(),
        "pending" | "running" | "budget-exhausted" | "context-limit" | "failed" | "done"
    ) {
        return Err(malformed_work(row_id, "status is invalid"));
    }
    required_nonnegative_integer(object, "calls", row_id)?;
    required_nonnegative_integer(object, "inputBytes", row_id)?;
    required_nonnegative_integer(object, "maxCalls", row_id)?;
    required_positive_integer(object, "maxInputBytes", row_id)?;
    required_nonnegative_integer(object, "totalTokens", row_id)?;
    string_array(object, "remaining", row_id, true)?;
    array_of_attempts(object, row_id)?;
    if !object.contains_key("pending") {
        return Err(malformed_work(row_id, "pending is missing"));
    }
    if let Some(pending) = object.get("pending") {
        validate_pending(pending, row_id)?;
    }
    optional_pid(object, "ownerPid", row_id)?;
    optional_pid(object, "nativeProcessId", row_id)?;
    optional_string(object, "resultKey", row_id)?;
    optional_enum(object, "phase", &["update", "ask", "review"], row_id)?;
    optional_enum(
        object,
        "retainedCheckAssessment",
        &["accepted", "blocked"],
        row_id,
    )?;
    optional_string_array(object, "plannedUnits", row_id)?;
    optional_nonnegative_integer(object, "cacheHits", row_id)?;
    if let Some(materialized) = object.get("materializedChecks")
        && !materialized.is_boolean()
    {
        return Err(malformed_work(row_id, "materializedChecks is invalid"));
    }
    validate_context_limit(object, row_id)?;
    validate_warning_baseline(object, row_id)?;
    Ok(())
}

fn malformed_work(row_id: &str, detail: &str) -> HivexError {
    HivexError::new(
        "READ_FAILED",
        format!("Malformed work row {row_id}: {detail}"),
    )
}

fn required_string(object: &Map<String, Value>, name: &str, row_id: &str) -> Result<String> {
    object
        .get(name)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| malformed_work(row_id, &format!("{name} is missing or invalid")))
}

fn optional_string(object: &Map<String, Value>, name: &str, row_id: &str) -> Result<()> {
    if let Some(value) = object.get(name)
        && !value.is_string()
    {
        return Err(malformed_work(row_id, &format!("{name} is invalid")));
    }
    Ok(())
}

fn optional_enum(
    object: &Map<String, Value>,
    name: &str,
    values: &[&str],
    row_id: &str,
) -> Result<()> {
    if let Some(value) = object.get(name) {
        let valid = value
            .as_str()
            .is_some_and(|candidate| values.contains(&candidate));
        if !valid {
            return Err(malformed_work(row_id, &format!("{name} is invalid")));
        }
    }
    Ok(())
}

fn string_array(
    object: &Map<String, Value>,
    name: &str,
    row_id: &str,
    required: bool,
) -> Result<()> {
    let Some(value) = object.get(name) else {
        if required {
            return Err(malformed_work(row_id, &format!("{name} is missing")));
        }
        return Ok(());
    };
    let valid = value
        .as_array()
        .is_some_and(|values| values.iter().all(Value::is_string));
    if !valid {
        return Err(malformed_work(row_id, &format!("{name} is invalid")));
    }
    Ok(())
}

fn optional_string_array(object: &Map<String, Value>, name: &str, row_id: &str) -> Result<()> {
    string_array(object, name, row_id, false)
}

fn required_nonnegative_integer(
    object: &Map<String, Value>,
    name: &str,
    row_id: &str,
) -> Result<i64> {
    object
        .get(name)
        .and_then(as_nonnegative_integer)
        .ok_or_else(|| malformed_work(row_id, &format!("{name} is missing or invalid")))
}

fn optional_nonnegative_integer(
    object: &Map<String, Value>,
    name: &str,
    row_id: &str,
) -> Result<()> {
    if let Some(value) = object.get(name)
        && as_nonnegative_integer(value).is_none()
    {
        return Err(malformed_work(row_id, &format!("{name} is invalid")));
    }
    Ok(())
}

fn required_positive_integer(object: &Map<String, Value>, name: &str, row_id: &str) -> Result<i64> {
    object
        .get(name)
        .and_then(as_positive_integer)
        .ok_or_else(|| malformed_work(row_id, &format!("{name} is missing or invalid")))
}

fn optional_pid(object: &Map<String, Value>, name: &str, row_id: &str) -> Result<Option<i64>> {
    let Some(value) = object.get(name) else {
        return Ok(None);
    };
    as_positive_integer(value)
        .map(Some)
        .ok_or_else(|| malformed_work(row_id, &format!("{name} is invalid")))
}

pub(crate) fn as_nonnegative_integer(value: &Value) -> Option<i64> {
    let number = as_integer(value)?;
    (number >= 0).then_some(number)
}

pub(crate) fn as_positive_integer(value: &Value) -> Option<i64> {
    let number = as_integer(value)?;
    (number > 0).then_some(number)
}

fn as_integer(value: &Value) -> Option<i64> {
    const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
    if let Some(number) = value.as_i64() {
        return (-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER)
            .contains(&number)
            .then_some(number);
    }
    let number = value.as_f64()?;
    if !number.is_finite()
        || number.fract() != 0.0
        || number < -(MAX_SAFE_INTEGER as f64)
        || number > MAX_SAFE_INTEGER as f64
    {
        return None;
    }
    Some(number as i64)
}

fn array_of_attempts(object: &Map<String, Value>, row_id: &str) -> Result<()> {
    let Some(attempts) = object.get("attempts").and_then(Value::as_array) else {
        return Err(malformed_work(row_id, "attempts is missing or invalid"));
    };
    if attempts.len() > 4096 {
        return Err(malformed_work(
            row_id,
            "attempts exceeds the maximum length",
        ));
    }
    for attempt in attempts {
        let Some(attempt) = attempt.as_object() else {
            return Err(malformed_work(row_id, "attempt is not an object"));
        };
        if attempt
            .get("inputBytes")
            .and_then(Value::as_f64)
            .is_none_or(|value| !value.is_finite())
        {
            return Err(malformed_work(row_id, "attempt inputBytes is invalid"));
        }
        if attempt.get("inputHash").and_then(Value::as_str).is_none()
            || attempt.get("stage").and_then(Value::as_str).is_none()
        {
            return Err(malformed_work(row_id, "attempt identity is invalid"));
        }
        for name in ["diagnostic", "error", "outputHash"] {
            optional_string(attempt, name, row_id)?;
        }
        if let Some(acknowledgement) = attempt.get("recoveryAcknowledgement") {
            let Some(acknowledgement) = acknowledgement.as_object() else {
                return Err(malformed_work(
                    row_id,
                    "recovery acknowledgement is invalid",
                ));
            };
            if acknowledgement
                .get("acknowledgedAt")
                .and_then(Value::as_str)
                .is_none()
                || acknowledgement.get("type").and_then(Value::as_str)
                    != Some("uncertain-invocation")
                || acknowledgement
                    .get("nativeProcessId")
                    .and_then(as_positive_integer)
                    .is_none()
            {
                return Err(malformed_work(
                    row_id,
                    "recovery acknowledgement is invalid",
                ));
            }
        }
    }
    Ok(())
}

fn validate_pending(value: &Value, row_id: &str) -> Result<()> {
    let Some(pending) = value.as_object() else {
        if value.is_null() {
            return Ok(());
        }
        return Err(malformed_work(row_id, "pending is invalid"));
    };
    required_string(pending, "batch", row_id)?;
    string_array(pending, "documents", row_id, true)?;
    string_array(pending, "context", row_id, false)?;
    string_array(pending, "existing", row_id, false)?;
    string_array(pending, "protectedRelationships", row_id, false)?;
    string_array(pending, "units", row_id, false)?;
    let extraction = pending
        .get("extraction")
        .ok_or_else(|| malformed_work(row_id, "pending extraction is invalid"))?;
    validate_extraction(extraction, row_id)?;
    optional_string_or_null(pending, "baseExtraction", row_id)?;
    for name in ["materializedCheck", "staged"] {
        if let Some(value) = pending.get(name)
            && !value.is_boolean()
        {
            return Err(malformed_work(
                row_id,
                &format!("pending {name} is invalid"),
            ));
        }
    }
    if let Some(packet) = pending.get("packet")
        && !packet.is_object()
    {
        return Err(malformed_work(row_id, "pending packet is invalid"));
    }
    Ok(())
}

fn validate_extraction(value: &Value, row_id: &str) -> Result<()> {
    let Some(extraction) = value.as_object() else {
        return Err(malformed_work(row_id, "pending extraction is invalid"));
    };
    let Some(decisions) = extraction.get("decisions").and_then(Value::as_array) else {
        return Err(malformed_work(row_id, "pending decisions are invalid"));
    };
    if decisions.len() > 64 {
        return Err(malformed_work(
            row_id,
            "pending decisions exceed the maximum length",
        ));
    }
    for decision in decisions {
        let Some(decision) = decision.as_object() else {
            return Err(malformed_work(row_id, "pending decision is invalid"));
        };
        for name in ["document", "id", "reason", "text"] {
            nonempty_string(decision, name, row_id, "pending decision")?;
        }
        required_explanation(decision, "reason", row_id, "pending decision")?;
        required_explanation(decision, "text", row_id, "pending decision")?;
        required_enum(
            decision,
            "kind",
            &["decision", "constraint", "definition", "lesson"],
            row_id,
        )?;
        required_enum(
            decision,
            "status",
            &["current", "proposed", "historical", "uncertain"],
            row_id,
        )?;
        required_positive_integer(decision, "lineStart", row_id)?;
        required_positive_integer(decision, "lineEnd", row_id)?;
        explanation_array(decision, "conditions", row_id, 16, "pending decision")?;
        explanation_array(decision, "exceptions", row_id, 16, "pending decision")?;
    }

    let Some(relationships) = extraction.get("relationships").and_then(Value::as_array) else {
        return Err(malformed_work(row_id, "pending relationships are invalid"));
    };
    if relationships.len() > 128 {
        return Err(malformed_work(
            row_id,
            "pending relationships exceed the maximum length",
        ));
    }
    for relationship in relationships {
        let Some(relationship) = relationship.as_object() else {
            return Err(malformed_work(row_id, "pending relationship is invalid"));
        };
        for name in ["from", "id", "reason", "to"] {
            nonempty_string(relationship, name, row_id, "pending relationship")?;
        }
        required_explanation(relationship, "reason", row_id, "pending relationship")?;
        required_enum(
            relationship,
            "type",
            &[
                "requires",
                "exception-to",
                "supersedes",
                "supports",
                "contradicts",
            ],
            row_id,
        )?;
        let Some(evidence) = relationship.get("evidence").and_then(Value::as_array) else {
            return Err(malformed_work(
                row_id,
                "pending relationship evidence is invalid",
            ));
        };
        if evidence.is_empty() || evidence.len() > 8 {
            return Err(malformed_work(
                row_id,
                "pending relationship evidence is invalid",
            ));
        }
        for citation in evidence {
            let Some(citation) = citation.as_object() else {
                return Err(malformed_work(row_id, "pending citation is invalid"));
            };
            nonempty_string(citation, "document", row_id, "pending citation")?;
            required_positive_integer(citation, "lineStart", row_id)?;
            required_positive_integer(citation, "lineEnd", row_id)?;
        }
    }

    let Some(uncertainties) = extraction.get("uncertainties").and_then(Value::as_array) else {
        return Err(malformed_work(row_id, "pending uncertainties are invalid"));
    };
    if uncertainties.len() > 32 {
        return Err(malformed_work(row_id, "pending uncertainties are invalid"));
    }
    for uncertainty in uncertainties {
        validate_explanation(uncertainty, row_id, "pending uncertainty")?;
    }
    Ok(())
}

fn required_explanation(
    object: &Map<String, Value>,
    name: &str,
    row_id: &str,
    prefix: &str,
) -> Result<()> {
    let value = object
        .get(name)
        .ok_or_else(|| malformed_work(row_id, &format!("{prefix} {name} is invalid")))?;
    validate_explanation(value, row_id, &format!("{prefix} {name}"))
}

fn explanation_array(
    object: &Map<String, Value>,
    name: &str,
    row_id: &str,
    maximum: usize,
    prefix: &str,
) -> Result<()> {
    let values = object
        .get(name)
        .and_then(Value::as_array)
        .ok_or_else(|| malformed_work(row_id, &format!("{prefix} {name} is invalid")))?;
    if values.len() > maximum {
        return Err(malformed_work(
            row_id,
            &format!("{prefix} {name} is invalid"),
        ));
    }
    for value in values {
        validate_explanation(value, row_id, &format!("{prefix} {name}"))?;
    }
    Ok(())
}

fn validate_explanation(value: &Value, row_id: &str, label: &str) -> Result<()> {
    let Some(value) = value.as_str() else {
        return Err(malformed_work(row_id, &format!("{label} is invalid")));
    };
    let length = value.encode_utf16().count();
    if !(1..=2048).contains(&length) {
        return Err(malformed_work(row_id, &format!("{label} is invalid")));
    }
    Ok(())
}

fn nonempty_string(
    object: &Map<String, Value>,
    name: &str,
    row_id: &str,
    prefix: &str,
) -> Result<String> {
    object
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| malformed_work(row_id, &format!("{prefix} {name} is invalid")))
}

fn optional_string_or_null(object: &Map<String, Value>, name: &str, row_id: &str) -> Result<()> {
    if let Some(value) = object.get(name)
        && !value.is_string()
        && !value.is_null()
    {
        return Err(malformed_work(row_id, &format!("{name} is invalid")));
    }
    Ok(())
}

fn required_enum(
    object: &Map<String, Value>,
    name: &str,
    values: &[&str],
    row_id: &str,
) -> Result<()> {
    let valid = object
        .get(name)
        .and_then(Value::as_str)
        .is_some_and(|candidate| values.contains(&candidate));
    if !valid {
        return Err(malformed_work(row_id, &format!("{name} is invalid")));
    }
    Ok(())
}

fn validate_context_limit(object: &Map<String, Value>, row_id: &str) -> Result<()> {
    let Some(value) = object.get("contextLimit") else {
        return Ok(());
    };
    let Some(limit) = value.as_object() else {
        return Err(malformed_work(row_id, "contextLimit is invalid"));
    };
    string_array(limit, "documents", row_id, true)?;
    for name in ["maxBytes", "requiredBytes"] {
        if limit.get(name).and_then(Value::as_f64).is_none() {
            return Err(malformed_work(
                row_id,
                &format!("contextLimit {name} is invalid"),
            ));
        }
    }
    Ok(())
}

fn validate_warning_baseline(object: &Map<String, Value>, row_id: &str) -> Result<()> {
    let Some(value) = object.get("warningBaseline") else {
        return Ok(());
    };
    let Some(baseline) = value.as_object() else {
        return Err(malformed_work(row_id, "warningBaseline is invalid"));
    };
    if baseline
        .values()
        .any(|value| !matches!(value.as_str(), Some("active") | Some("resolved")))
    {
        return Err(malformed_work(row_id, "warningBaseline is invalid"));
    }
    Ok(())
}

fn reject_symlink(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(HivexError::new(
            "INVALID_STORE",
            "Knowledge storage cannot be a symlink",
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn private_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

fn remove_field(value: &mut Value, field: &str) {
    if let Some(object) = value.as_object_mut() {
        object.remove(field);
    }
}

fn set_string(value: &mut Value, field: &str, content: &str) -> Result<()> {
    value
        .as_object_mut()
        .ok_or_else(|| HivexError::new("READ_FAILED", "Work is not a JSON object"))?
        .insert(field.to_owned(), Value::String(content.to_owned()));
    Ok(())
}

fn set_u64(value: &mut Value, field: &str, number: u64) -> Result<()> {
    value
        .as_object_mut()
        .ok_or_else(|| HivexError::new("READ_FAILED", "Work is not a JSON object"))?
        .insert(field.to_owned(), json!(number));
    Ok(())
}

fn nonnegative_integer(value: Option<&Value>) -> Option<u64> {
    value?.as_u64().or_else(|| {
        let number = value?.as_f64()?;
        (number.is_finite() && number.fract() == 0.0 && number >= 0.0).then_some(number as u64)
    })
}

fn positive_integer(value: &Value) -> Option<u64> {
    nonnegative_integer(Some(value)).filter(|number| *number > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("hivex-store-{}", Uuid::new_v4()));
        fs::create_dir(&root).expect("temporary root");
        root
    }

    fn cleanup(root: &Path) {
        let _ = fs::remove_dir_all(root);
    }

    fn empty_graph() -> Value {
        Store::empty_graph()
    }

    #[test]
    fn preserves_v1_fixture_rows_and_cache_values() {
        let root = root();
        let directory = root.join(".hivex");
        fs::create_dir(&directory).expect("store directory");
        let database_path = directory.join("knowledge.sqlite");
        let database = Connection::open(&database_path).expect("fixture database");
        let fixture = include_str!("../test/fixtures/knowledge-cache-v1.sql");
        database.execute_batch(fixture).expect("load fixture");
        let before: String = database
            .query_row("SELECT data FROM work", [], |row| row.get(0))
            .expect("work row");
        let mut work = Store::open(&root, StoreOptions::default())
            .expect("open store")
            .works()
            .expect("read work")
            .pop()
            .expect("one work");
        assert_eq!(work.status(), "done");
        assert_eq!(work.calls(), 1);
        assert_eq!(
            work.result_key(),
            Some("224de5769c9cfd624d910bb5afb0f1463fc42f70f36dc4122ea65717b6da836d")
        );
        let store = Store::open(&root, StoreOptions::default()).expect("reopen store");
        assert!(
            store
                .cached("366df8482c75d92fe8a0b0e5b446cc86e8c28bae27d2bae37fff4a62ffc6a4f5")
                .expect("cache read")
                .is_some()
        );
        drop(store);
        let mut store = Store::open(&root, StoreOptions::default()).expect("reopen store");
        store.save(&mut work).expect("save unchanged work");
        let after: String = database
            .query_row("SELECT data FROM work", [], |row| row.get(0))
            .expect("work row");
        assert_eq!(
            serde_json::from_str::<Value>(&before).expect("before JSON"),
            serde_json::from_str::<Value>(&after).expect("after JSON")
        );
        cleanup(&root);
    }

    #[test]
    fn preserves_all_v1_fixture_rows_when_round_tripped() {
        for name in [
            "knowledge-cache-v1.sql",
            "knowledge-update-cache-v1.sql",
            "knowledge-multiround-cache-v1.sql",
        ] {
            let root = root();
            let directory = root.join(".hivex");
            fs::create_dir(&directory).expect("store directory");
            let database_path = directory.join("knowledge.sqlite");
            let database = Connection::open(&database_path).expect("fixture database");
            let fixture = fs::read_to_string(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("test/fixtures")
                    .join(name),
            )
            .expect("fixture");
            database.execute_batch(&fixture).expect("load fixture");
            let before = database
                .prepare("SELECT id,data FROM work ORDER BY rowid")
                .expect("work query")
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .expect("work rows")
                .collect::<std::result::Result<Vec<_>, _>>()
                .expect("work data");
            let graph: String = database
                .query_row("SELECT data FROM graph WHERE id=1", [], |row| row.get(0))
                .expect("graph data");
            let caches = database
                .prepare("SELECT key,value FROM model_cache ORDER BY rowid")
                .expect("cache query")
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .expect("cache rows")
                .collect::<std::result::Result<Vec<_>, _>>()
                .expect("cache data");
            drop(database);
            let mut store = Store::open(&root, StoreOptions::default()).expect("open store");
            for mut work in store.works().expect("works") {
                store.save(&mut work).expect("round trip work");
            }
            drop(store);
            let database = Connection::open(&database_path).expect("reopen fixture database");
            let after = database
                .prepare("SELECT id,data FROM work ORDER BY rowid")
                .expect("work query")
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .expect("work rows")
                .collect::<std::result::Result<Vec<_>, _>>()
                .expect("work data");
            let graph_after: String = database
                .query_row("SELECT data FROM graph WHERE id=1", [], |row| row.get(0))
                .expect("graph data");
            let caches_after = database
                .prepare("SELECT key,value FROM model_cache ORDER BY rowid")
                .expect("cache query")
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .expect("cache rows")
                .collect::<std::result::Result<Vec<_>, _>>()
                .expect("cache data");
            assert_eq!(before, after, "fixture {name}");
            assert_eq!(graph, graph_after, "fixture {name}");
            assert_eq!(caches, caches_after, "fixture {name}");
            cleanup(&root);
        }
    }

    #[test]
    fn validates_graphs_and_reads_shared_snapshot_when_local_graph_is_absent() {
        let root = root();
        let graph = empty_graph();
        crate::knowledge_snapshot::write_knowledge_snapshot(&root, &graph)
            .expect("write shared snapshot");
        let mut store = Store::open(&root, StoreOptions::default()).expect("open store");
        assert_eq!(store.graph().expect("read shared snapshot"), graph);
        let invalid = store
            .save_graph(&json!({"version": 1}))
            .expect_err("reject malformed graph");
        assert_eq!(invalid.code, "INVALID_SNAPSHOT");

        let work = store
            .begin(BeginWork {
                key: "initial".to_owned(),
                kind: "update".to_owned(),
                max_calls: Some(1),
                max_input_bytes: Some(1024),
                remaining: vec!["notes.md:1-1".to_owned()],
                result_key: None,
                snapshot: "snapshot".to_owned(),
                warning_baseline: None,
            })
            .expect("begin initializes local graph");
        assert_eq!(work.status(), "pending");
        assert_eq!(store.graph().expect("read initialized graph"), graph);
        drop(store);
        cleanup(&root);
    }

    #[test]
    fn returns_empty_graph_when_pending_work_precedes_a_local_snapshot() {
        let root = root();
        let mut store = Store::open(&root, StoreOptions::default()).expect("open store");
        let mut work = new_work(&BeginWork {
            key: "pending".to_owned(),
            kind: "update".to_owned(),
            max_calls: Some(1),
            max_input_bytes: Some(1024),
            remaining: vec!["notes.md:1-1".to_owned()],
            result_key: None,
            snapshot: "snapshot".to_owned(),
            warning_baseline: None,
        })
        .expect("new work");
        save_work(store.database_mut(), &mut work).expect("persist pending work");
        assert_eq!(
            store.graph().expect("empty graph for pending work"),
            empty_graph()
        );
        drop(store);
        cleanup(&root);
    }

    #[test]
    fn update_lease_is_exclusive_and_releases_without_touching_foreign_lock() {
        let root = root();
        let store = Store::open(
            &root,
            StoreOptions {
                update: true,
                ..StoreOptions::default()
            },
        )
        .expect("first update lease");
        let blocked = match Store::open(
            &root,
            StoreOptions {
                update: true,
                ..StoreOptions::default()
            },
        ) {
            Ok(_) => panic!("second update lease"),
            Err(error) => error,
        };
        assert_eq!(blocked.code, "KNOWLEDGE_LOCKED");
        let lock_path = root.join(".hivex/knowledge.lock");
        assert!(lock_path.is_file());
        drop(store);
        assert!(!lock_path.exists());
        let store = Store::open(
            &root,
            StoreOptions {
                update: true,
                ..StoreOptions::default()
            },
        )
        .expect("lease after release");
        fs::write(&lock_path, "foreign-lock").expect("replace lock");
        drop(store);
        assert_eq!(
            fs::read_to_string(&lock_path).expect("foreign lock"),
            "foreign-lock"
        );
        fs::remove_file(&lock_path).expect("remove fixture lock");
        cleanup(&root);
    }

    #[test]
    fn resumes_work_without_resetting_counters_or_optional_field_shape() {
        let root = root();
        let mut store = Store::open(&root, StoreOptions::default()).expect("open store");
        store.save_graph(&empty_graph()).expect("graph");
        let options = BeginWork {
            key: "fixture".to_owned(),
            kind: "update".to_owned(),
            max_calls: Some(2),
            max_input_bytes: None,
            remaining: vec!["notes.md:1-3".to_owned()],
            result_key: None,
            snapshot: "snapshot".to_owned(),
            warning_baseline: Some(json!({"w1": "resolved"})),
        };
        let mut work = store.begin(options.clone()).expect("begin");
        assert_eq!(work.calls(), 0);
        store
            .reserve(&mut work, 40, "input-1", "extract")
            .expect("reserve");
        store
            .record_native_process(&mut work, std::process::id())
            .expect("record pid");
        work.value_mut()
            .as_object_mut()
            .expect("object")
            .insert("status".to_owned(), Value::String("failed".to_owned()));
        work.value_mut()
            .as_object_mut()
            .expect("object")
            .insert("futureField".to_owned(), json!({"kept": true}));
        store.save(&mut work).expect("save failed work");
        let resumed = store.begin(options).expect("resume");
        assert_eq!(resumed.id(), work.id());
        assert_eq!(resumed.calls(), 1);
        assert_eq!(resumed.input_bytes(), 40);
        assert_eq!(
            resumed.value().get("futureField"),
            Some(&json!({"kept": true}))
        );
        let mut resumed = resumed;
        store
            .reserve(&mut resumed, 20, "input-2", "check")
            .expect("resume reserve");
        assert_eq!(resumed.calls(), 2);
        assert_eq!(resumed.input_bytes(), 60);
        assert_eq!(resumed.attempts().expect("attempts").len(), 2);
        cleanup(&root);
    }

    #[test]
    fn reuses_completed_update_only_for_the_same_empty_plan_and_resets_ask_result() {
        let root = root();
        let mut store = Store::open(&root, StoreOptions::default()).expect("open store");
        store.save_graph(&empty_graph()).expect("graph");
        let update = BeginWork {
            key: "completed".to_owned(),
            kind: "update".to_owned(),
            max_calls: None,
            max_input_bytes: None,
            remaining: Vec::new(),
            result_key: None,
            snapshot: "snapshot".to_owned(),
            warning_baseline: None,
        };
        let mut completed = store.begin(update.clone()).expect("begin update");
        completed
            .value_mut()
            .as_object_mut()
            .expect("object")
            .insert("status".to_owned(), Value::String("done".to_owned()));
        completed
            .value_mut()
            .as_object_mut()
            .expect("object")
            .insert("result".to_owned(), json!({"answer": "kept"}));
        store.save(&mut completed).expect("save completed update");
        let reused = store.begin(update).expect("reuse completed update");
        assert_eq!(reused.id(), completed.id());
        assert_eq!(
            reused.value().get("result"),
            Some(&json!({"answer": "kept"}))
        );
        let mut new_plan = Store::open(&root, StoreOptions::default())
            .expect("reopen store")
            .begin(BeginWork {
                key: "completed".to_owned(),
                kind: "update".to_owned(),
                max_calls: None,
                max_input_bytes: None,
                remaining: vec!["unit".to_owned()],
                result_key: None,
                snapshot: "snapshot".to_owned(),
                warning_baseline: None,
            })
            .expect("new update plan");
        assert_ne!(new_plan.id(), completed.id());
        new_plan
            .value_mut()
            .as_object_mut()
            .expect("object")
            .insert("status".to_owned(), Value::String("done".to_owned()));
        store.save(&mut new_plan).expect("save new plan");

        let ask = BeginWork {
            key: "consultation".to_owned(),
            kind: "ask".to_owned(),
            max_calls: None,
            max_input_bytes: None,
            remaining: Vec::new(),
            result_key: Some("packet-1".to_owned()),
            snapshot: "snapshot".to_owned(),
            warning_baseline: None,
        };
        let mut answer = store.begin(ask.clone()).expect("begin ask");
        answer
            .value_mut()
            .as_object_mut()
            .expect("object")
            .insert("status".to_owned(), Value::String("done".to_owned()));
        answer
            .value_mut()
            .as_object_mut()
            .expect("object")
            .insert("result".to_owned(), json!({"answer": "old"}));
        store.save(&mut answer).expect("save answer");
        let reset = store
            .begin(BeginWork {
                result_key: Some("packet-2".to_owned()),
                ..ask
            })
            .expect("begin changed ask");
        assert_eq!(reset.id(), answer.id());
        assert_eq!(reset.status(), "pending");
        assert_eq!(reset.value().get("result"), None);
        cleanup(&root);
    }

    #[test]
    fn import_refuses_unfinished_work_without_replacing_graph() {
        let root = root();
        let mut store = Store::open(&root, StoreOptions::default()).expect("open store");
        store.save_graph(&empty_graph()).expect("graph");
        let options = BeginWork {
            key: "unfinished".to_owned(),
            kind: "update".to_owned(),
            max_calls: None,
            max_input_bytes: None,
            remaining: vec!["unit".to_owned()],
            result_key: None,
            snapshot: "snapshot".to_owned(),
            warning_baseline: None,
        };
        let _ = store.begin(options).expect("begin");
        let error = store
            .import_graph(&json!({"replacement": true}))
            .expect_err("unfinished import");
        assert_eq!(error.code, "UNFINISHED_WORK");
        assert_eq!(store.graph().expect("graph"), empty_graph());
        cleanup(&root);
    }

    #[test]
    fn old_optional_fields_are_not_introduced_when_saved() {
        let root = root();
        let directory = root.join(".hivex");
        fs::create_dir(&directory).expect("store directory");
        let database = Connection::open(directory.join("knowledge.sqlite")).expect("database");
        database.execute_batch(
            "CREATE TABLE graph (id INTEGER PRIMARY KEY CHECK(id=1), data TEXT NOT NULL);\
             CREATE TABLE work (id TEXT PRIMARY KEY, kind TEXT NOT NULL, key TEXT NOT NULL, data TEXT NOT NULL);\
             CREATE TABLE model_cache (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        ).expect("schema");
        let old = json!({
            "id": "old",
            "kind": "ask",
            "key": "key",
            "snapshot": "snapshot",
            "maxCalls": 3,
            "maxInputBytes": 131072,
            "remaining": [],
            "plannedUnits": [],
            "calls": 0,
            "inputBytes": 0,
            "totalTokens": 0,
            "status": "failed",
            "pending": null,
            "attempts": [],
            "futureField": {"retained": 1}
        });
        database
            .execute(
                "INSERT INTO work VALUES(?,?,?,?)",
                params!["old", "ask", "key", old.to_string()],
            )
            .expect("old work");
        drop(database);
        let mut store = Store::open(&root, StoreOptions::default()).expect("open store");
        let mut work = store.works().expect("works").pop().expect("work");
        assert_eq!(work.value().get("materializedChecks"), None);
        assert_eq!(work.value().get("warningBaseline"), None);
        store.save(&mut work).expect("save");
        let database = Connection::open(directory.join("knowledge.sqlite")).expect("database");
        let data: String = database
            .query_row("SELECT data FROM work", [], |row| row.get(0))
            .expect("data");
        let saved: Value = serde_json::from_str(&data).expect("JSON");
        assert_eq!(saved.get("materializedChecks"), None);
        assert_eq!(saved.get("warningBaseline"), None);
        assert_eq!(saved.get("futureField"), Some(&json!({"retained": 1})));
        cleanup(&root);
    }
}
