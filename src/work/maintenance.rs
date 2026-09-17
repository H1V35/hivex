use crate::cli::arguments;
use crate::error::{HivexError, Result};
use crate::work::store::{self as store, LOCK_FILENAME, Store, StoreOptions};
use chrono::{SecondsFormat, Utc};
use rusqlite::TransactionBehavior;
use serde_json::{Map, Value, json};
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_KEEP_COMPLETED: usize = 8;
const DEFAULT_KEEP_CACHES: usize = 64;
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
    let value = crate::compatibility::trim_js_whitespace(value);
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
    let mut storage = Store::open(
        &root,
        StoreOptions {
            update: command == "prune",
            ..StoreOptions::default()
        },
    )?;
    if command == "recover" {
        let report = recover(&mut storage, parsed.flags.contains("acknowledge-uncertain"))?;
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
    let report = prune(&mut storage, keep_completed, keep_caches)?;
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
    row_id: String,
    value: Value,
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

fn recover(store: &mut Store, acknowledge_uncertain: bool) -> Result<RecoveryReport> {
    match recover_checked(store, acknowledge_uncertain) {
        Ok(report) => Ok(report),
        Err(error) if error.code == "RECOVERY_UNSAFE" => Ok(blocked_recovery(error)),
        Err(error) => Err(error),
    }
}

fn recover_checked(store: &mut Store, acknowledge_uncertain: bool) -> Result<RecoveryReport> {
    let lock = read_recovery_lock(store.directory())?;
    if let Some(lock) = &lock {
        assert_owner_ended(lock.pid, "held", "The lock owner")?;
    }

    let works = read_works_for_recovery(store)?;
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
        return release_recovery_lock(store.directory(), lock, 0, 0);
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
    record_recovery(store, &works, &candidates, lock_state)?;
    release_recovery_lock(
        store.directory(),
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
        .and_then(store::as_nonnegative_integer)
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
    let pid = object.get("pid").and_then(store::as_positive_integer);
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
    store: &mut Store,
    works: &[WorkSnapshot],
    candidates: &[RecoveryCandidate],
    lock: &str,
) -> Result<()> {
    let transaction = store
        .database_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let result = (|| -> Result<()> {
        for candidate in candidates {
            let index = candidate.index;
            let work = &works[index];
            let current_data: String = transaction
                .query_row("SELECT data FROM work WHERE id=?1", [&work.row_id], |row| {
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
            let mut current = store::parse_work(work.row_id.clone(), &current_data)?;
            if current.status() != work.status || current.calls() as i64 != work.calls {
                return Err(recovery_unsafe(
                    "changed",
                    &format!(
                        "Work {} changed during recovery; run recover again.",
                        work.id
                    ),
                    0,
                ));
            }
            apply_recovery(current.value_mut(), candidate.native_pid)?;
            store::save_work(&transaction, &mut current)?;
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
    Some(
        report
            .get("nativeProcessId")
            .and_then(store::as_positive_integer),
    )
}

fn last_attempt(value: &Value) -> Option<&Map<String, Value>> {
    value
        .as_object()?
        .get("attempts")?
        .as_array()?
        .last()?
        .as_object()
}

fn read_works_for_recovery(store: &Store) -> Result<Vec<WorkSnapshot>> {
    store
        .works()
        .map(|works| {
            works
                .into_iter()
                .map(|work| WorkSnapshot {
                    row_id: work.row_id().to_owned(),
                    value: work.value().clone(),
                    id: work.id().to_owned(),
                    status: work.status().to_owned(),
                    calls: i64::try_from(work.calls()).expect("validated work calls fit i64"),
                    owner_pid: work.owner_pid().map(i64::from),
                    native_pid: work.native_process_id().map(i64::from),
                })
                .collect()
        })
        .map_err(|_| {
            recovery_unsafe(
                "unreadable",
                "Work state cannot be validated; preserve the store and inspect it manually.",
                0,
            )
        })
}

fn prune(store: &mut Store, keep_completed: usize, keep_caches: usize) -> Result<PruneReport> {
    // Parse and validate every work row before opening the write transaction.
    // A malformed old row must never be silently removed by retention.
    let works = store.works()?;
    let completed: Vec<String> = works
        .iter()
        .filter(|work| work.status() == "done")
        .map(|work| work.row_id().to_owned())
        .collect();
    let work_rows_to_delete = completed
        .iter()
        .skip(keep_completed)
        .cloned()
        .collect::<Vec<_>>();

    let cache_rows: Vec<i64> = {
        let database = store.database_mut();
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

    let transaction = store
        .database_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let result = (|| -> Result<()> {
        for row_id in &work_rows_to_delete {
            transaction.execute("DELETE FROM work WHERE id=?1", [row_id])?;
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
        unfinished_works: works.iter().filter(|work| work.status() != "done").count(),
    })
}
