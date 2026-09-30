use super::Work;
use super::store::{BeginWork, ExecutionBinding, parse_work};
use crate::error::{HivexError, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;

fn migrate_coverage(transaction: &Connection, work: &Work, next_key: &str) -> Result<()> {
  let data: String =
    transaction.query_row("SELECT data FROM graph WHERE id=1", [], |row| row.get(0))?;
  let mut graph: serde_json::Value = serde_json::from_str(&data)?;
  let units = graph["units"].as_object_mut().ok_or_else(invalid)?;
  if !units.values().any(|unit| unit["workKey"] == work.key()) {
    return Ok(());
  }
  let matching: i64 = transaction.query_row(
    "SELECT count(*) FROM work WHERE key=?",
    [work.key()],
    |row| row.get(0),
  )?;
  if matching != 1 {
    return Err(invalid());
  }
  for unit in units
    .values_mut()
    .filter(|unit| unit["workKey"] == work.key())
  {
    unit["workKey"] = json!(next_key);
  }
  transaction.execute(
    "UPDATE graph SET data=? WHERE id=1",
    [serde_json::to_string(&graph)?],
  )?;
  Ok(())
}

fn changed(work: &Work) -> HivexError {
  HivexError::new(
    "EXECUTION_PROFILE_CHANGED",
    format!(
      "Work {} is unfinished under a different execution profile. Use the same arguments and --resume-with-profile {} to explicitly choose the requested profile, or resume the previous profile. Its history and budget were preserved.",
      work.id(),
      work.id()
    ),
  )
}

fn invalid() -> HivexError {
  HivexError::new(
    "INVALID_PROFILE_RESUMPTION",
    "Profile resumption requires the exact work ID, operation, sources and arguments. Completed or ambiguous work cannot change profile. No model call was made.",
  )
}

fn conflicts(
  transaction: &Connection,
  options: &BeginWork,
  binding: &ExecutionBinding,
) -> Result<Vec<Work>> {
  transaction.prepare(
    "SELECT id,data FROM work WHERE kind=?1 AND key<>?2 AND (key=?3 OR json_extract(data,'$.operationKey')=?4) AND json_extract(data,'$.status')<>'done'",
  )?.query_map(params![options.kind,options.key,binding.legacy_key.as_deref(),binding.operation_key],
    |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?)))?
    .map(|row| { let (id,data)=row?; parse_work(id,&data) }).collect()
}

fn selected(transaction: &Connection, id: &str) -> Result<Work> {
  let row = transaction
    .query_row("SELECT id,data FROM work WHERE id=?", [id], |row| {
      Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })
    .optional()?;
  let (row_id, data) = row.ok_or_else(invalid)?;
  let work = parse_work(row_id, &data)?;
  if work.id() != id {
    return Err(invalid());
  }
  Ok(work)
}

fn explicit_work(
  transaction: &Connection,
  options: &BeginWork,
  binding: &ExecutionBinding,
  candidates: &[Work],
) -> Result<Work> {
  let work = selected(
    transaction,
    binding.resume_work_id.as_deref().ok_or_else(invalid)?,
  )?;
  if work.value()["kind"] != options.kind
    || (work.value()["operationKey"] != binding.operation_key && !legacy(&work, binding))
    || candidates
      .iter()
      .any(|other| other.row_id() != work.row_id() && other.unfinished())
  {
    return Err(invalid());
  }
  work.ensure_profile_change()?;
  if work.key() != options.key && !work.unfinished() {
    return Err(invalid());
  }
  Ok(work)
}

fn legacy(work: &Work, binding: &ExecutionBinding) -> bool {
  binding.legacy_key.as_deref() == Some(work.key())
    && binding.legacy_profile.as_ref().is_some_and(|profile| {
      work
        .value()
        .get("executionProfile")
        .is_none_or(|old| old == profile)
    })
}

fn replace(work: &mut Work, options: &BeginWork, binding: &ExecutionBinding) -> Result<()> {
  work.ensure_profile_change()?;
  let from = work
    .value()
    .get("executionProfile")
    .or(
      binding
        .legacy_profile
        .as_ref()
        .filter(|_| legacy(work, binding)),
    )
    .ok_or_else(invalid)?
    .clone();
  let previous_model = work
    .value()
    .get("executionIdentity")
    .cloned()
    .unwrap_or_else(|| {
      if legacy(work, binding) {
        binding
          .legacy_identity
          .clone()
          .unwrap_or_else(|| from.clone())
      } else {
        from.clone()
      }
    });
  let mut history = match work.value()["profileReplacement"].get("previousReplacements") {
    Some(value) => value.as_array().ok_or_else(invalid)?.clone(),
    None => Vec::new(),
  };
  if let Some(mut previous) = work.value().get("profileReplacement").cloned() {
    previous
      .as_object_mut()
      .ok_or_else(invalid)?
      .shift_remove("previousReplacements");
    history.push(previous);
  }
  let mut record = json!({"from":from,"to":binding.profile,"previousKey":work.key(),"previousModel":previous_model});
  if !history.is_empty() {
    record["previousReplacements"] = json!(history);
  }
  work.value["profileReplacement"] = record;
  work.value["key"] = json!(options.key);
  work.bind_execution(&binding.operation_key, &binding.profile, &binding.identity);
  work.execution_changed = true;
  Ok(())
}

pub(super) fn validate(
  transaction: &Connection,
  options: &BeginWork,
  binding: Option<&ExecutionBinding>,
) -> Result<Option<Work>> {
  let Some(binding) = binding else {
    return Ok(None);
  };
  let candidates = conflicts(transaction, options, binding)?;
  for work in &candidates {
    work.ensure_open()?;
  }
  let mut work = if binding.resume_work_id.is_some() {
    explicit_work(transaction, options, binding, &candidates)?
  } else {
    if candidates.len() > 1 {
      return Err(invalid());
    }
    let Some(work) = candidates.into_iter().find(Work::unfinished) else {
      return Ok(None);
    };
    if binding.replaced_profile.is_none() || !legacy(&work, binding) {
      return Err(changed(&work));
    }
    work
  };
  if work.key() == options.key && work.value()["executionProfile"] == binding.profile {
    return Ok(Some(work));
  }
  migrate_coverage(transaction, &work, &options.key)?;
  replace(&mut work, options, binding)?;
  transaction.execute(
    "UPDATE work SET key=?1 WHERE id=?2",
    params![options.key, work.row_id()],
  )?;
  Ok(Some(work))
}
