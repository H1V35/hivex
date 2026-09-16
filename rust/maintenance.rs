use crate::arguments;
use crate::error::{HivexError, Result};
use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, TransactionBehavior};
use serde_json::{Map, Value, json};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;

const DEFAULT_KEEP_COMPLETED: usize = 8;
const DEFAULT_KEEP_CACHES: usize = 64;
const LOCK_FILENAME: &str = "knowledge.lock";
const USAGE: &str = "Use recover [--acknowledge-uncertain] or prune [--keep-completed <count>] [--keep-caches <count>]";

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProcessState {
    Alive,
    Dead,
    Unknown,
}

struct LockInfo {
    pid: i64,
    raw: String,
}

struct UpdateLease {
    path: PathBuf,
    token: String,
    // Keeping the descriptor open gives the lock a lifetime bounded by the
    // operation, just like the TypeScript store's disposable lease.
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
        let contents = match fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(_) => return,
        };
        if contents == self.token {
            let _ = fs::remove_file(&self.path);
        }
    }
}

struct Storage {
    database: Connection,
    directory: PathBuf,
    _lease: Option<UpdateLease>,
}

impl Storage {
    fn open(root: &Path, update: bool) -> Result<Self> {
        let directory = root.join(".hivex");
        reject_symlink(&directory)?;
        let created = !directory.exists();
        fs::create_dir_all(&directory)?;
        if created {
            set_private_directory(&directory)?;
        }

        let database_path = directory.join("knowledge.sqlite");
        reject_symlink(&database_path)?;
        let database = Connection::open(&database_path)?;
        database.busy_timeout(Duration::from_millis(1000))?;
        database.execute_batch("PRAGMA max_page_count=16384;")?;
        database.execute_batch(
            "CREATE TABLE IF NOT EXISTS graph (id INTEGER PRIMARY KEY CHECK(id=1), data TEXT NOT NULL);\
             CREATE TABLE IF NOT EXISTS work (id TEXT PRIMARY KEY, kind TEXT NOT NULL, key TEXT NOT NULL, data TEXT NOT NULL);\
             CREATE TABLE IF NOT EXISTS model_cache (key TEXT PRIMARY KEY, value TEXT NOT NULL);\
             CREATE INDEX IF NOT EXISTS work_key ON work(kind,key);",
        )?;

        let lease = if update {
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

fn set_private_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

fn invalid_usage() -> HivexError {
    HivexError::new("INVALID_ARGUMENT", USAGE)
}

fn retention(value: Option<&String>, name: &str, fallback: usize) -> Result<usize> {
    let Some(value) = value else {
        return Ok(fallback);
    };
    // Node's Number() accepts the ordinary decimal and exponent spellings,
    // including an empty string (zero). Keep the same small CLI contract while
    // avoiding lossy values outside the permitted range.
    let number = parse_number(value).ok_or_else(|| invalid_retention(name))?;
    if !number.is_finite() || number.fract() != 0.0 || !(0.0..=4096.0).contains(&number) {
        return Err(invalid_retention(name));
    }
    Ok(number as usize)
}

fn parse_number(value: &str) -> Option<f64> {
    let value = value.trim();
    if value.is_empty() {
        return Some(0.0);
    }
    let (radix, digits) = if let Some(digits) = value.strip_prefix("0x") {
        (16, digits)
    } else if let Some(digits) = value.strip_prefix("0X") {
        (16, digits)
    } else if let Some(digits) = value.strip_prefix("0o") {
        (8, digits)
    } else if let Some(digits) = value.strip_prefix("0O") {
        (8, digits)
    } else if let Some(digits) = value.strip_prefix("0b") {
        (2, digits)
    } else if let Some(digits) = value.strip_prefix("0B") {
        (2, digits)
    } else {
        return value.parse::<f64>().ok();
    };
    (!digits.is_empty())
        .then(|| u64::from_str_radix(digits, radix).ok())
        .flatten()
        .map(|number| number as f64)
}

fn invalid_retention(name: &str) -> HivexError {
    HivexError::new(
        "INVALID_ARGUMENT",
        format!("{name} must be an integer between 0 and 4096"),
    )
}

pub fn command(args: &[String]) -> Result<Value> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(invalid_usage());
    };
    if command != "recover" && command != "prune" {
        return Err(invalid_usage());
    }

    let parsed = arguments::parse(
        &args[1..],
        &["root", "keep-completed", "keep-caches"],
        &["acknowledge-uncertain"],
    )?;
    if !parsed.positionals.is_empty() {
        return Err(invalid_usage());
    }
    if command == "recover"
        && (parsed.values.contains_key("keep-completed")
            || parsed.values.contains_key("keep-caches"))
    {
        return Err(HivexError::new(
            "INVALID_ARGUMENT",
            "recover does not accept retention options",
        ));
    }
    if command == "prune" && parsed.flags.contains("acknowledge-uncertain") {
        return Err(HivexError::new(
            "INVALID_ARGUMENT",
            "prune does not accept --acknowledge-uncertain",
        ));
    }

    let root = parsed
        .values
        .get("root")
        .map_or_else(std::env::current_dir, |value| Ok(PathBuf::from(value)))?;
    let mut storage = Storage::open(&root, command == "prune")?;
    if command == "recover" {
        let report = recover(
            &mut storage.database,
            &storage.directory,
            parsed.flags.contains("acknowledge-uncertain"),
        )?;
        return Ok(recovery_value(command, report));
    }

    let keep_completed = retention(
        parsed.values.get("keep-completed"),
        "--keep-completed",
        DEFAULT_KEEP_COMPLETED,
    )?;
    let keep_caches = retention(
        parsed.values.get("keep-caches"),
        "--keep-caches",
        DEFAULT_KEEP_CACHES,
    )?;
    let report = prune(&mut storage.database, keep_completed, keep_caches)?;
    Ok(json!({
        "command": command,
        "modelCalls": 0,
        "deletedCaches": report.deleted_caches,
        "deletedCompletedWorks": report.deleted_completed_works,
        "retainedCaches": report.retained_caches,
        "retainedCompletedWorks": report.retained_completed_works,
        "unfinishedWorks": report.unfinished_works,
    }))
}

fn recovery_value(command: &str, report: RecoveryReport) -> Value {
    let mut object = Map::new();
    object.insert("command".to_owned(), Value::String(command.to_owned()));
    object.insert("modelCalls".to_owned(), json!(0));
    object.insert(
        "acknowledgedWorks".to_owned(),
        json!(report.acknowledged_works),
    );
    object.insert(
        "interruptedWorks".to_owned(),
        json!(report.interrupted_works),
    );
    object.insert("lock".to_owned(), Value::String(report.lock.to_owned()));
    object.insert("status".to_owned(), Value::String(report.status.to_owned()));
    if let Some(guidance) = report.guidance {
        object.insert("guidance".to_owned(), Value::String(guidance));
    }
    Value::Object(object)
}

struct RecoveryReport {
    acknowledged_works: usize,
    interrupted_works: usize,
    lock: &'static str,
    status: &'static str,
    guidance: Option<String>,
}

struct PruneReport {
    deleted_caches: usize,
    deleted_completed_works: usize,
    retained_caches: usize,
    retained_completed_works: usize,
    unfinished_works: usize,
}

struct WorkSnapshot {
    rowid: i64,
    value: Value,
    id: String,
    status: String,
    calls: i64,
    owner_pid: Option<i64>,
    native_pid: Option<i64>,
}

struct WorkFields {
    id: String,
    status: String,
    calls: i64,
    owner_pid: Option<i64>,
    native_pid: Option<i64>,
}

struct RecoveryCandidate {
    index: usize,
    native_pid: Option<i64>,
}

fn recover(
    database: &mut Connection,
    directory: &Path,
    acknowledge_uncertain: bool,
) -> Result<RecoveryReport> {
    match recover_checked(database, directory, acknowledge_uncertain) {
        Ok(report) => Ok(report),
        Err(error) if error.code == "RECOVERY_UNSAFE" => Ok(blocked_recovery(error)),
        Err(error) => Err(error),
    }
}

fn recover_checked(
    database: &mut Connection,
    directory: &Path,
    acknowledge_uncertain: bool,
) -> Result<RecoveryReport> {
    let lock = read_recovery_lock(directory)?;
    if let Some(lock) = &lock {
        assert_owner_ended(lock.pid, "held", "The lock owner")?;
    }

    let works = read_works_for_recovery(database)?;
    let mut running = Vec::new();
    let mut uncertain_failed = Vec::new();
    for (index, work) in works.iter().enumerate() {
        if work.status == "running" {
            running.push(index);
        } else if work.status == "failed"
            && let Some(native_pid) = uncertain_failed_pid(work)
        {
            uncertain_failed.push(RecoveryCandidate { index, native_pid });
        }
    }

    if running.is_empty() && uncertain_failed.is_empty() {
        return release_recovery_lock(directory, lock, 0, 0);
    }

    let lock_state = if lock.is_some() { "held" } else { "absent" };
    for index in &running {
        let work = &works[*index];
        assert_recoverable(work, work.native_pid, lock_state)?;
    }
    for candidate in &uncertain_failed {
        let work = &works[candidate.index];
        assert_recoverable(work, candidate.native_pid, lock_state)?;
    }

    let uncertain = running
        .iter()
        .filter(|index| works[**index].native_pid.is_some())
        .count()
        + uncertain_failed.len();
    if uncertain > 0 && !acknowledge_uncertain {
        return Err(recovery_unsafe(
            lock_state,
            "Uncertain work is recoverable after its owner and native PIDs ended; rerun `hivex recover --acknowledge-uncertain --root <project>` to record an explicit acknowledgement.",
            0,
        ));
    }

    let mut candidates = Vec::with_capacity(running.len() + uncertain_failed.len());
    for index in running {
        candidates.push(RecoveryCandidate {
            index,
            native_pid: works[index].native_pid,
        });
    }
    candidates.extend(uncertain_failed);
    record_recovery(database, &works, &candidates, lock_state)?;
    release_recovery_lock(
        directory,
        lock,
        uncertain,
        candidates
            .iter()
            .filter(|candidate| works[candidate.index].status == "running")
            .count(),
    )
}

fn blocked_recovery(error: HivexError) -> RecoveryReport {
    let lock = error
        .details
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|details| details.get("lock"))
        .and_then(Value::as_str)
        .map(|lock| match lock {
            "absent" => "absent",
            "held" => "held",
            "unreadable" => "unreadable",
            "changed" => "changed",
            _ => "unreadable",
        })
        .unwrap_or("unreadable");
    let interrupted_works = error
        .details
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|details| details.get("interruptedWorks"))
        .and_then(as_nonnegative_integer)
        .unwrap_or(0) as usize;
    RecoveryReport {
        acknowledged_works: 0,
        interrupted_works,
        lock,
        status: "blocked",
        guidance: Some(error.message),
    }
}

fn read_recovery_lock(directory: &Path) -> Result<Option<LockInfo>> {
    let path = directory.join(LOCK_FILENAME);
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => {
            return Err(recovery_unsafe(
                "unreadable",
                "knowledge.lock cannot be read safely; inspect the store before continuing.",
                0,
            ));
        }
    };
    let value: Value = serde_json::from_str(&raw).map_err(|_| {
        recovery_unsafe(
            "unreadable",
            "knowledge.lock has no verifiable PID; do not delete it and inspect the process manually.",
            0,
        )
    })?;
    let object = value.as_object().ok_or_else(|| {
        recovery_unsafe(
            "unreadable",
            "knowledge.lock has no verifiable PID; do not delete it and inspect the process manually.",
            0,
        )
    })?;
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty());
    let pid = object.get("pid").and_then(as_positive_integer);
    if id.is_none() || pid.is_none() {
        return Err(recovery_unsafe(
            "unreadable",
            "knowledge.lock has no verifiable PID; do not delete it and inspect the process manually.",
            0,
        ));
    }
    Ok(Some(LockInfo {
        pid: pid.expect("checked above"),
        raw,
    }))
}

fn release_recovery_lock(
    directory: &Path,
    lock: Option<LockInfo>,
    acknowledged_works: usize,
    interrupted_works: usize,
) -> Result<RecoveryReport> {
    let guidance = (acknowledged_works > 0).then(|| {
        "Recovery preserved the work. Run `hivex update --retry-failed --root <project>` to retry it explicitly; recovery made zero model calls."
            .to_owned()
    });
    let Some(lock) = lock else {
        return Ok(RecoveryReport {
            acknowledged_works,
            interrupted_works,
            lock: "absent",
            status: "recovered",
            guidance,
        });
    };

    let path = directory.join(LOCK_FILENAME);
    let released = match fs::read_to_string(&path) {
        Ok(contents) if contents == lock.raw => fs::remove_file(&path).map(|_| true),
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error),
    };
    match released {
        Ok(true) => Ok(RecoveryReport {
            acknowledged_works,
            interrupted_works,
            lock: "released",
            status: "recovered",
            guidance,
        }),
        Ok(false) => Err(recovery_unsafe(
            "changed",
            if acknowledged_works > 0 {
                "Work was acknowledged, but knowledge.lock changed; run recover again before continuing."
            } else {
                "knowledge.lock changed during recovery; inspect the store before continuing."
            },
            interrupted_works,
        )),
        Err(_) => Err(recovery_unsafe(
            "unreadable",
            if acknowledged_works > 0 {
                "Work was acknowledged, but knowledge.lock could not be released."
            } else {
                "The owner is dead, but knowledge.lock could not be released atomically."
            },
            interrupted_works,
        )),
    }
}

fn recovery_unsafe(lock: &str, message: &str, interrupted_works: usize) -> HivexError {
    HivexError::new("RECOVERY_UNSAFE", message).with_details(json!({
        "interruptedWorks": interrupted_works,
        "lock": lock,
    }))
}

fn assert_owner_ended(pid: i64, lock: &str, label: &str) -> Result<()> {
    match process_state(pid) {
        ProcessState::Alive => Err(recovery_unsafe(
            lock,
            &format!("{label} (PID {pid}) is still alive; no process was modified or terminated."),
            0,
        )),
        ProcessState::Dead => Ok(()),
        ProcessState::Unknown => Err(recovery_unsafe(
            lock,
            &format!("{label} (PID {pid}) cannot be proven dead; no state was modified."),
            0,
        )),
    }
}

fn assert_recoverable(work: &WorkSnapshot, native_pid: Option<i64>, lock: &str) -> Result<()> {
    let owner_pid = work.owner_pid.ok_or_else(|| {
        recovery_unsafe(
            lock,
            &format!(
                "Work {} has no recorded owner PID; its recovery state is unchanged.",
                work.id
            ),
            0,
        )
    })?;
    assert_owner_ended(owner_pid, lock, &format!("Work {} owner", work.id))?;
    let Some(attempt) = last_attempt(&work.value) else {
        return Err(recovery_unsafe(
            lock,
            &format!(
                "Work {} has no reserved attempt; no state was changed.",
                work.id
            ),
            0,
        ));
    };
    if native_pid.is_none() && work.status == "running" {
        return Ok(());
    }
    let Some(native_pid) = native_pid else {
        return Err(recovery_unsafe(
            lock,
            &format!(
                "Work {} has no native PID for its uncertain result; no state was changed.",
                work.id
            ),
            0,
        ));
    };
    match process_state(native_pid) {
        ProcessState::Alive => {
            return Err(recovery_unsafe(
                lock,
                &format!(
                    "Native process PID {native_pid} for work {} is still alive; no process was killed.",
                    work.id
                ),
                0,
            ));
        }
        ProcessState::Unknown => {
            return Err(recovery_unsafe(
                lock,
                &format!(
                    "Native process PID {native_pid} for work {} cannot be checked; no state was changed.",
                    work.id
                ),
                0,
            ));
        }
        ProcessState::Dead => {}
    }
    if let Some(report) = attempt.get("report")
        && !report.is_object()
    {
        return Err(recovery_unsafe(
            lock,
            &format!(
                "Work {} has an unstructured running report; recovery left it unchanged.",
                work.id
            ),
            0,
        ));
    }
    Ok(())
}

fn record_recovery(
    database: &mut Connection,
    works: &[WorkSnapshot],
    candidates: &[RecoveryCandidate],
    lock: &str,
) -> Result<()> {
    let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let result = (|| -> Result<()> {
        for candidate in candidates {
            let index = candidate.index;
            let work = &works[index];
            let current_data: String = transaction
                .query_row("SELECT data FROM work WHERE id=?1", [&work.id], |row| {
                    row.get(0)
                })
                .map_err(|error| match error {
                    rusqlite::Error::QueryReturnedNoRows => recovery_unsafe(
                        "changed",
                        &format!(
                            "Work {} changed during recovery; run recover again.",
                            work.id
                        ),
                        0,
                    ),
                    other => HivexError::from(other),
                })?;
            let current_value: Value = serde_json::from_str(&current_data)?;
            let current = validate_work(current_value, work.rowid)?;
            if current.status != work.status || current.calls != work.calls {
                return Err(recovery_unsafe(
                    "changed",
                    &format!(
                        "Work {} changed during recovery; run recover again.",
                        work.id
                    ),
                    0,
                ));
            }
            let mut value = current.value;
            apply_recovery(&mut value, candidate.native_pid)?;
            transaction.execute(
                "UPDATE work SET data=?1 WHERE id=?2",
                rusqlite::params![serde_json::to_string(&value)?, work.id],
            )?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => transaction.commit().map_err(Into::into),
        Err(error) if error.code == "RECOVERY_UNSAFE" && error.details.is_none() => {
            Err(recovery_unsafe(lock, &error.message, 0))
        }
        Err(error) => Err(error),
    }
}

fn apply_recovery(value: &mut Value, native_pid: Option<i64>) -> Result<()> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| HivexError::new("READ_FAILED", "Work record is not a JSON object"))?;
    let attempts = object
        .get_mut("attempts")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| HivexError::new("READ_FAILED", "Work attempts are not an array"))?;
    let attempt = attempts
        .last_mut()
        .and_then(Value::as_object_mut)
        .ok_or_else(|| HivexError::new("READ_FAILED", "Work has no reserved attempt"))?;
    let report_missing = attempt.get("report").is_none_or(Value::is_null);
    if report_missing {
        attempt.insert(
            "report".to_owned(),
            match native_pid {
                None => json!({
                    "cleanup": "not-observed",
                    "code": "MODEL_INTERRUPTED_BEFORE_TURN",
                    "outcome": "interrupted",
                    "usage": null,
                }),
                Some(native_pid) => json!({
                    "cleanup": "not-observed",
                    "code": "MODEL_INTERRUPTED_RECOVERED",
                    "interruption": "unconfirmed",
                    "outcome": "interrupted",
                    "recovery": {
                        "nativeProcessEnded": true,
                        "nativeProcessId": native_pid,
                        "previousOutcome": null,
                    },
                    "turnAccepted": "unknown",
                    "usage": null,
                }),
            },
        );
    }
    if let Some(native_pid) = native_pid {
        let acknowledged_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        attempt.insert(
            "recoveryAcknowledgement".to_owned(),
            json!({
                "acknowledgedAt": acknowledged_at,
                "nativeProcessId": native_pid,
                "type": "uncertain-invocation",
            }),
        );
    }
    object.insert("status".to_owned(), Value::String("failed".to_owned()));
    object.remove("nativeProcessId");
    Ok(())
}

fn process_state(pid: i64) -> ProcessState {
    if pid <= 0 || pid > libc::pid_t::MAX as i64 {
        return ProcessState::Unknown;
    }
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if result == 0 {
        return ProcessState::Alive;
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::ESRCH) => ProcessState::Dead,
        Some(libc::EPERM) => ProcessState::Alive,
        _ => ProcessState::Unknown,
    }
}

fn uncertain_failed_pid(work: &WorkSnapshot) -> Option<Option<i64>> {
    let attempt = last_attempt(&work.value)?;
    if attempt.contains_key("recoveryAcknowledgement") {
        return None;
    }
    let report = attempt.get("report")?.as_object()?;
    let interruption = report.get("interruption").and_then(Value::as_str);
    let turn_accepted = report.get("turnAccepted").and_then(Value::as_str);
    if interruption != Some("unconfirmed") && turn_accepted != Some("unknown") {
        return None;
    }
    Some(report.get("nativeProcessId").and_then(as_positive_integer))
}

fn last_attempt(value: &Value) -> Option<&Map<String, Value>> {
    value
        .as_object()?
        .get("attempts")?
        .as_array()?
        .last()?
        .as_object()
}

fn read_works_for_recovery(database: &Connection) -> Result<Vec<WorkSnapshot>> {
    read_work_rows(database).map_err(|_| {
        recovery_unsafe(
            "unreadable",
            "Work state cannot be validated; preserve the store and inspect it manually.",
            0,
        )
    })
}

fn read_work_rows(database: &Connection) -> Result<Vec<WorkSnapshot>> {
    let mut statement = database.prepare("SELECT rowid,data FROM work ORDER BY rowid DESC")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut works = Vec::new();
    for row in rows {
        let (rowid, data) = row?;
        let value: Value = serde_json::from_str(&data)?;
        works.push(validate_work(value, rowid)?);
    }
    Ok(works)
}

fn validate_work(value: Value, rowid: i64) -> Result<WorkSnapshot> {
    let fields = validate_work_fields(&value, rowid)?;
    Ok(WorkSnapshot {
        rowid,
        value,
        id: fields.id,
        status: fields.status,
        calls: fields.calls,
        owner_pid: fields.owner_pid,
        native_pid: fields.native_pid,
    })
}

fn validate_work_fields(value: &Value, rowid: i64) -> Result<WorkFields> {
    let object = value
        .as_object()
        .ok_or_else(|| malformed_work(rowid, "record is not an object"))?;
    let id = required_string(object, "id", rowid)?;
    let kind = required_string(object, "kind", rowid)?;
    if !matches!(kind.as_str(), "update" | "ask" | "review") {
        return Err(malformed_work(rowid, "kind is invalid"));
    }
    let _key = required_string(object, "key", rowid)?;
    let _snapshot = required_string(object, "snapshot", rowid)?;
    let status = required_string(object, "status", rowid)?;
    if !matches!(
        status.as_str(),
        "pending" | "running" | "budget-exhausted" | "context-limit" | "failed" | "done"
    ) {
        return Err(malformed_work(rowid, "status is invalid"));
    }
    let calls = required_nonnegative_integer(object, "calls", rowid)?;
    let _input_bytes = required_nonnegative_integer(object, "inputBytes", rowid)?;
    let _max_calls = required_nonnegative_integer(object, "maxCalls", rowid)?;
    let _max_input_bytes = required_positive_integer(object, "maxInputBytes", rowid)?;
    let _total_tokens = required_nonnegative_integer(object, "totalTokens", rowid)?;
    string_array(object, "remaining", rowid, true)?;
    array_of_attempts(object, rowid)?;
    if !object.contains_key("pending") {
        return Err(malformed_work(rowid, "pending is missing"));
    }
    if let Some(pending) = object.get("pending") {
        validate_pending(pending, rowid)?;
    }
    optional_pid(object, "ownerPid", rowid)?;
    let native_pid = optional_pid(object, "nativeProcessId", rowid)?;
    optional_string(object, "resultKey", rowid)?;
    optional_enum(object, "phase", &["update", "ask", "review"], rowid)?;
    optional_enum(
        object,
        "retainedCheckAssessment",
        &["accepted", "blocked"],
        rowid,
    )?;
    optional_string_array(object, "plannedUnits", rowid)?;
    optional_nonnegative_integer(object, "cacheHits", rowid)?;
    if let Some(materialized) = object.get("materializedChecks")
        && !materialized.is_boolean()
    {
        return Err(malformed_work(rowid, "materializedChecks is invalid"));
    }
    validate_context_limit(object, rowid)?;
    validate_warning_baseline(object, rowid)?;
    let owner_pid = optional_pid(object, "ownerPid", rowid)?;
    Ok(WorkFields {
        id,
        status,
        calls,
        owner_pid,
        native_pid,
    })
}

fn malformed_work(rowid: i64, detail: &str) -> HivexError {
    HivexError::new(
        "READ_FAILED",
        format!("Malformed work row {rowid}: {detail}"),
    )
}

fn required_string(object: &Map<String, Value>, name: &str, rowid: i64) -> Result<String> {
    object
        .get(name)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| malformed_work(rowid, &format!("{name} is missing or invalid")))
}

fn optional_string(object: &Map<String, Value>, name: &str, rowid: i64) -> Result<()> {
    if let Some(value) = object.get(name)
        && !value.is_string()
    {
        return Err(malformed_work(rowid, &format!("{name} is invalid")));
    }
    Ok(())
}

fn optional_enum(
    object: &Map<String, Value>,
    name: &str,
    values: &[&str],
    rowid: i64,
) -> Result<()> {
    if let Some(value) = object.get(name) {
        let valid = value
            .as_str()
            .is_some_and(|candidate| values.contains(&candidate));
        if !valid {
            return Err(malformed_work(rowid, &format!("{name} is invalid")));
        }
    }
    Ok(())
}

fn required_enum(
    object: &Map<String, Value>,
    name: &str,
    values: &[&str],
    rowid: i64,
) -> Result<()> {
    let valid = object
        .get(name)
        .and_then(Value::as_str)
        .is_some_and(|candidate| values.contains(&candidate));
    if !valid {
        return Err(malformed_work(rowid, &format!("{name} is invalid")));
    }
    Ok(())
}

fn string_array(object: &Map<String, Value>, name: &str, rowid: i64, required: bool) -> Result<()> {
    let Some(value) = object.get(name) else {
        if required {
            return Err(malformed_work(rowid, &format!("{name} is missing")));
        }
        return Ok(());
    };
    let valid = value
        .as_array()
        .is_some_and(|values| values.iter().all(Value::is_string));
    if !valid {
        return Err(malformed_work(rowid, &format!("{name} is invalid")));
    }
    Ok(())
}

fn optional_string_array(object: &Map<String, Value>, name: &str, rowid: i64) -> Result<()> {
    string_array(object, name, rowid, false)
}

fn required_nonnegative_integer(
    object: &Map<String, Value>,
    name: &str,
    rowid: i64,
) -> Result<i64> {
    object
        .get(name)
        .and_then(as_nonnegative_integer)
        .ok_or_else(|| malformed_work(rowid, &format!("{name} is missing or invalid")))
}

fn optional_nonnegative_integer(object: &Map<String, Value>, name: &str, rowid: i64) -> Result<()> {
    if let Some(value) = object.get(name)
        && as_nonnegative_integer(value).is_none()
    {
        return Err(malformed_work(rowid, &format!("{name} is invalid")));
    }
    Ok(())
}

fn required_positive_integer(object: &Map<String, Value>, name: &str, rowid: i64) -> Result<i64> {
    object
        .get(name)
        .and_then(as_positive_integer)
        .ok_or_else(|| malformed_work(rowid, &format!("{name} is missing or invalid")))
}

fn optional_pid(object: &Map<String, Value>, name: &str, rowid: i64) -> Result<Option<i64>> {
    let Some(value) = object.get(name) else {
        return Ok(None);
    };
    as_positive_integer(value)
        .map(Some)
        .ok_or_else(|| malformed_work(rowid, &format!("{name} is invalid")))
}

fn as_nonnegative_integer(value: &Value) -> Option<i64> {
    let number = as_integer(value)?;
    (number >= 0).then_some(number)
}

fn as_positive_integer(value: &Value) -> Option<i64> {
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

fn array_of_attempts(object: &Map<String, Value>, rowid: i64) -> Result<()> {
    let Some(attempts) = object.get("attempts").and_then(Value::as_array) else {
        return Err(malformed_work(rowid, "attempts is missing or invalid"));
    };
    if attempts.len() > 4096 {
        return Err(malformed_work(rowid, "attempts exceeds the maximum length"));
    }
    for attempt in attempts {
        let Some(attempt) = attempt.as_object() else {
            return Err(malformed_work(rowid, "attempt is not an object"));
        };
        if attempt
            .get("inputBytes")
            .and_then(Value::as_f64)
            .is_none_or(|value| !value.is_finite())
        {
            return Err(malformed_work(rowid, "attempt inputBytes is invalid"));
        }
        if attempt.get("inputHash").and_then(Value::as_str).is_none()
            || attempt.get("stage").and_then(Value::as_str).is_none()
        {
            return Err(malformed_work(rowid, "attempt identity is invalid"));
        }
        for name in ["diagnostic", "error", "outputHash"] {
            optional_string(attempt, name, rowid)?;
        }
        if let Some(acknowledgement) = attempt.get("recoveryAcknowledgement") {
            let Some(acknowledgement) = acknowledgement.as_object() else {
                return Err(malformed_work(rowid, "recovery acknowledgement is invalid"));
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
                return Err(malformed_work(rowid, "recovery acknowledgement is invalid"));
            }
        }
    }
    Ok(())
}

fn validate_pending(value: &Value, rowid: i64) -> Result<()> {
    let Some(pending) = value.as_object() else {
        if value.is_null() {
            return Ok(());
        }
        return Err(malformed_work(rowid, "pending is invalid"));
    };
    required_string(pending, "batch", rowid)?;
    string_array(pending, "documents", rowid, true)?;
    string_array(pending, "context", rowid, false)?;
    string_array(pending, "existing", rowid, false)?;
    string_array(pending, "protectedRelationships", rowid, false)?;
    string_array(pending, "units", rowid, false)?;
    let extraction = pending
        .get("extraction")
        .ok_or_else(|| malformed_work(rowid, "pending extraction is invalid"))?;
    validate_extraction(extraction, rowid)?;
    optional_string_or_null(pending, "baseExtraction", rowid)?;
    for name in ["materializedCheck", "staged"] {
        if let Some(value) = pending.get(name)
            && !value.is_boolean()
        {
            return Err(malformed_work(rowid, &format!("pending {name} is invalid")));
        }
    }
    if let Some(packet) = pending.get("packet")
        && !packet.is_object()
    {
        return Err(malformed_work(rowid, "pending packet is invalid"));
    }
    Ok(())
}

fn validate_extraction(value: &Value, rowid: i64) -> Result<()> {
    let Some(extraction) = value.as_object() else {
        return Err(malformed_work(rowid, "pending extraction is invalid"));
    };
    let Some(decisions) = extraction.get("decisions").and_then(Value::as_array) else {
        return Err(malformed_work(rowid, "pending decisions are invalid"));
    };
    if decisions.len() > 64 {
        return Err(malformed_work(
            rowid,
            "pending decisions exceed the maximum length",
        ));
    }
    for decision in decisions {
        let Some(decision) = decision.as_object() else {
            return Err(malformed_work(rowid, "pending decision is invalid"));
        };
        for name in ["document", "id", "reason", "text"] {
            nonempty_string(decision, name, rowid, "pending decision")?;
        }
        required_explanation(decision, "reason", rowid, "pending decision")?;
        required_explanation(decision, "text", rowid, "pending decision")?;
        required_enum(
            decision,
            "kind",
            &["decision", "constraint", "definition", "lesson"],
            rowid,
        )?;
        required_enum(
            decision,
            "status",
            &["current", "proposed", "historical", "uncertain"],
            rowid,
        )?;
        required_positive_integer(decision, "lineStart", rowid)?;
        required_positive_integer(decision, "lineEnd", rowid)?;
        explanation_array(decision, "conditions", rowid, 16, "pending decision")?;
        explanation_array(decision, "exceptions", rowid, 16, "pending decision")?;
    }

    let Some(relationships) = extraction.get("relationships").and_then(Value::as_array) else {
        return Err(malformed_work(rowid, "pending relationships are invalid"));
    };
    if relationships.len() > 128 {
        return Err(malformed_work(
            rowid,
            "pending relationships exceed the maximum length",
        ));
    }
    for relationship in relationships {
        let Some(relationship) = relationship.as_object() else {
            return Err(malformed_work(rowid, "pending relationship is invalid"));
        };
        for name in ["from", "id", "reason", "to"] {
            nonempty_string(relationship, name, rowid, "pending relationship")?;
        }
        required_explanation(relationship, "reason", rowid, "pending relationship")?;
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
            rowid,
        )?;
        let Some(evidence) = relationship.get("evidence").and_then(Value::as_array) else {
            return Err(malformed_work(
                rowid,
                "pending relationship evidence is invalid",
            ));
        };
        if evidence.is_empty() || evidence.len() > 8 {
            return Err(malformed_work(
                rowid,
                "pending relationship evidence is invalid",
            ));
        }
        for citation in evidence {
            let Some(citation) = citation.as_object() else {
                return Err(malformed_work(rowid, "pending citation is invalid"));
            };
            nonempty_string(citation, "document", rowid, "pending citation")?;
            required_positive_integer(citation, "lineStart", rowid)?;
            required_positive_integer(citation, "lineEnd", rowid)?;
        }
    }

    let Some(uncertainties) = extraction.get("uncertainties").and_then(Value::as_array) else {
        return Err(malformed_work(rowid, "pending uncertainties are invalid"));
    };
    if uncertainties.len() > 32 {
        return Err(malformed_work(rowid, "pending uncertainties are invalid"));
    }
    for uncertainty in uncertainties {
        validate_explanation(uncertainty, rowid, "pending uncertainty")?;
    }
    Ok(())
}

fn required_explanation(
    object: &Map<String, Value>,
    name: &str,
    rowid: i64,
    prefix: &str,
) -> Result<()> {
    let value = object
        .get(name)
        .ok_or_else(|| malformed_work(rowid, &format!("{prefix} {name} is invalid")))?;
    validate_explanation(value, rowid, &format!("{prefix} {name}"))
}

fn explanation_array(
    object: &Map<String, Value>,
    name: &str,
    rowid: i64,
    maximum: usize,
    prefix: &str,
) -> Result<()> {
    let values = object
        .get(name)
        .and_then(Value::as_array)
        .ok_or_else(|| malformed_work(rowid, &format!("{prefix} {name} is invalid")))?;
    if values.len() > maximum {
        return Err(malformed_work(
            rowid,
            &format!("{prefix} {name} is invalid"),
        ));
    }
    for value in values {
        validate_explanation(value, rowid, &format!("{prefix} {name}"))?;
    }
    Ok(())
}

fn validate_explanation(value: &Value, rowid: i64, label: &str) -> Result<()> {
    let Some(value) = value.as_str() else {
        return Err(malformed_work(rowid, &format!("{label} is invalid")));
    };
    let length = value.encode_utf16().count();
    if !(1..=2048).contains(&length) {
        return Err(malformed_work(rowid, &format!("{label} is invalid")));
    }
    Ok(())
}

fn nonempty_string(
    object: &Map<String, Value>,
    name: &str,
    rowid: i64,
    prefix: &str,
) -> Result<String> {
    let value = object
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| malformed_work(rowid, &format!("{prefix} {name} is invalid")))?;
    Ok(value)
}

fn optional_string_or_null(object: &Map<String, Value>, name: &str, rowid: i64) -> Result<()> {
    if let Some(value) = object.get(name)
        && !value.is_string()
        && !value.is_null()
    {
        return Err(malformed_work(rowid, &format!("{name} is invalid")));
    }
    Ok(())
}

fn validate_context_limit(object: &Map<String, Value>, rowid: i64) -> Result<()> {
    let Some(value) = object.get("contextLimit") else {
        return Ok(());
    };
    let Some(limit) = value.as_object() else {
        return Err(malformed_work(rowid, "contextLimit is invalid"));
    };
    string_array(limit, "documents", rowid, true)?;
    for name in ["maxBytes", "requiredBytes"] {
        if limit.get(name).and_then(Value::as_f64).is_none() {
            return Err(malformed_work(
                rowid,
                &format!("contextLimit {name} is invalid"),
            ));
        }
    }
    Ok(())
}

fn validate_warning_baseline(object: &Map<String, Value>, rowid: i64) -> Result<()> {
    let Some(value) = object.get("warningBaseline") else {
        return Ok(());
    };
    let Some(baseline) = value.as_object() else {
        return Err(malformed_work(rowid, "warningBaseline is invalid"));
    };
    if baseline
        .values()
        .any(|value| !matches!(value.as_str(), Some("active") | Some("resolved")))
    {
        return Err(malformed_work(rowid, "warningBaseline is invalid"));
    }
    Ok(())
}

fn prune(
    database: &mut Connection,
    keep_completed: usize,
    keep_caches: usize,
) -> Result<PruneReport> {
    // Parse and validate every work row before opening the write transaction.
    // A malformed old row must never be silently removed by retention.
    let works = read_work_rows(database)?;
    let completed: Vec<i64> = works
        .iter()
        .filter(|work| work.status == "done")
        .map(|work| work.rowid)
        .collect();
    let work_rows_to_delete = completed
        .iter()
        .skip(keep_completed)
        .copied()
        .collect::<Vec<_>>();

    let cache_rows: Vec<i64> = {
        let mut statement =
            database.prepare("SELECT rowid FROM model_cache ORDER BY rowid DESC")?;
        statement
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<Vec<i64>, rusqlite::Error>>()?
    };
    let cache_rows_to_delete = cache_rows
        .iter()
        .skip(keep_caches)
        .copied()
        .collect::<Vec<_>>();

    let transaction = database.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let result = (|| -> Result<()> {
        for rowid in &work_rows_to_delete {
            transaction.execute("DELETE FROM work WHERE rowid=?1", [rowid])?;
        }
        for rowid in &cache_rows_to_delete {
            transaction.execute("DELETE FROM model_cache WHERE rowid=?1", [rowid])?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => transaction.commit()?,
        Err(error) => return Err(error),
    }
    Ok(PruneReport {
        deleted_caches: cache_rows_to_delete.len(),
        deleted_completed_works: work_rows_to_delete.len(),
        retained_caches: cache_rows.len() - cache_rows_to_delete.len(),
        retained_completed_works: completed.len() - work_rows_to_delete.len(),
        unfinished_works: works.iter().filter(|work| work.status != "done").count(),
    })
}
