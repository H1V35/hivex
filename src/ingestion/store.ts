import { Database } from 'bun:sqlite';
import { lstatSync, mkdirSync, openSync, closeSync } from 'node:fs';
import { dirname } from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { z } from 'zod';
import { HivexError } from '../errors.ts';
import { hash } from '../sources/markdown.ts';
import type { createPlan } from './plan.ts';
import type { extractCommand, ExtractionCheckpoint } from './command.ts';
import { usageSchema } from '../model/transcript.ts';
import type { ExtractionAttempt } from './attempt.ts';
import { candidateSchema } from './claims.ts';

type Plan = ReturnType<typeof createPlan>;
type Result = Awaited<ReturnType<typeof extractCommand>>;
const maximumBytes = 128 * 1024 * 1024;
const reservationBytes = 16 * 1024 * 1024;
const applicationId = 0x48565849;
const reportSchema = z.looseObject({
  outcome: z.string().max(256),
  code: z.string().max(128).optional(),
  usage: usageSchema.nullable(),
  deadlineMilliseconds: z.number().int().min(100).max(900_000),
  promptHash: z.string().regex(/^[a-f0-9]{64}$/),
});
const attemptStateSchema = z.object({
  reports: z.array(reportSchema).max(3),
  active: z
    .object({
      attempt: z.number().int().min(1).max(3),
      promptHash: z.string().regex(/^[a-f0-9]{64}$/),
      deadlineMilliseconds: z.number().int().min(100).max(900_000),
    })
    .nullable(),
});
const emptyAttempts = JSON.stringify({ reports: [], active: null });
type StoredUnit = {
  id: string;
  state: string;
  owner: string | null;
  result: string | null;
  result_hash: string | null;
  attempts: string;
  attempts_hash: string;
};
const resultSchema = z.looseObject({
  command: z.literal('extract'),
  accepted: z.literal(false),
  status: z.enum(['candidate', 'failed']),
  source: z.looseObject({ id: z.string() }),
  attempts: z.array(reportSchema).min(1).max(3),
  candidate: candidateSchema.nullable(),
  candidateAttempt: z.number().int().min(1).max(3).nullable(),
});

function validatePhase(row: StoredUnit, state: z.infer<typeof attemptStateSchema>) {
  switch (row.state) {
    case 'pending':
      if (row.owner !== null || state.reports.length || state.active)
        failure('INVALID_INGESTION_STORE', 'A pending source cannot contain an invocation');
      break;
    case 'running':
      if (!row.owner) failure('INVALID_INGESTION_STORE', 'A claimed source must retain its owner');
      break;
    case 'candidate':
    case 'failed':
      if (row.owner !== null || state.active || !state.reports.length)
        failure('INVALID_INGESTION_STORE', 'A finished source must retain completed attempts');
      break;
    default:
      failure('INVALID_INGESTION_STORE', 'Unknown source state');
  }
  if (state.active && state.active.attempt !== state.reports.length + 1)
    failure('INVALID_INGESTION_STORE', 'The active attempt is inconsistent with its history');
}

function decodeUnit(row: StoredUnit) {
  if (hash(row.attempts) !== row.attempts_hash)
    failure('INVALID_INGESTION_STORE', 'A stored attempt report was altered');
  const checkpoint = attemptState(row.attempts);
  validatePhase(row, checkpoint);
  const finished = row.state === 'candidate' || row.state === 'failed';
  if (finished !== (row.result !== null) || (row.result === null && row.result_hash !== null))
    failure('INVALID_INGESTION_STORE', 'A source state is inconsistent with its retained result');
  if (row.result === null) return { result: null, checkpoint };
  if (hash(row.result) !== row.result_hash)
    failure('INVALID_INGESTION_STORE', 'A stored result was altered');
  const raw = z.record(z.string(), z.unknown()).parse(JSON.parse(row.result));
  const parsed = resultSchema.safeParse(raw);
  if (!parsed.success) failure('INVALID_INGESTION_STORE', 'The retained extraction is invalid');
  const result = parsed.data;
  if (result.status !== row.state || result.source.id !== row.id)
    failure('INVALID_INGESTION_STORE', 'The result does not match its source state');
  const candidate = result.status === 'candidate';
  if (
    candidate !== (result.candidate !== null) ||
    result.candidateAttempt !== (candidate ? result.attempts.length : null)
  )
    failure('INVALID_INGESTION_STORE', 'The candidate outcome is inconsistent');
  const originals = z.array(z.unknown()).parse(raw.attempts);
  const reports = result.attempts.map((report, index) =>
    boundedReport(report, JSON.stringify(originals[index])),
  );
  if (!isDeepStrictEqual(reports, checkpoint.reports))
    failure('INVALID_INGESTION_STORE', 'The result does not match its checkpointed attempts');
  return { result, checkpoint };
}

function attemptState(value: string) {
  try {
    return attemptStateSchema.parse(JSON.parse(value));
  } catch {
    return failure('INVALID_INGESTION_STORE', 'A stored attempt report is invalid');
  }
}

function failure(code: string, message: string): never {
  throw new HivexError({ code, message });
}

function readCohort(db: Database) {
  const row = db
    .query<
      { value: string; value_hash: string },
      []
    >('SELECT value, value_hash FROM cohort WHERE id=1')
    .get();
  if (!row) return null;
  if (Buffer.byteLength(row.value) > 8 * 1024 * 1024 || hash(row.value) !== row.value_hash)
    failure('INVALID_INGESTION_STORE', 'The recorded cohort is oversized or altered');
  return row.value;
}

function validateDirectory(path: string) {
  try {
    const directory = lstatSync(dirname(path));
    if (directory.isSymbolicLink() || !directory.isDirectory())
      failure('INVALID_INGESTION_STORE', 'The store directory must be a real directory');
  } catch (error) {
    if (!(error instanceof Error && 'code' in error && error.code === 'ENOENT')) throw error;
  }
}

function boundedReport(report: ExtractionAttempt, original?: string) {
  const value = original ?? JSON.stringify(report);
  const bytes = Buffer.byteLength(value);
  if (bytes <= 1024 * 1024) return report;
  return {
    outcome: report.outcome,
    code: report.code,
    usage: report.usage,
    promptHash: report.promptHash,
    deadlineMilliseconds: report.deadlineMilliseconds,
    omittedDetails: { originalBytes: bytes, reportHash: hash(value) },
  };
}

function validateIdentity(db: Database) {
  const identity = db.query<{ application_id: number }, []>('PRAGMA application_id').get();
  const version = db.query<{ user_version: number }, []>('PRAGMA user_version').get();
  if (identity?.application_id === applicationId && version?.user_version === 1) return true;
  const objects = db
    .query<{ count: number }, []>('SELECT count(*) AS count FROM sqlite_schema')
    .get();
  if (identity?.application_id === 0 && version?.user_version === 0 && objects?.count === 0)
    return false;
  failure('INVALID_INGESTION_STORE', 'Use an empty database or a supported Hivex ingestion store');
}

function openFile(path: string) {
  validateDirectory(path);
  mkdirSync(dirname(path), { recursive: true, mode: 0o700 });
  try {
    closeSync(openSync(path, 'wx', 0o600));
  } catch (error) {
    if (!(error instanceof Error && 'code' in error && error.code === 'EEXIST')) throw error;
  }
  const file = lstatSync(path);
  if (!file.isFile() || file.isSymbolicLink() || file.size > maximumBytes)
    failure('INVALID_INGESTION_STORE', 'Use a regular local ingestion store of at most 128 MiB');
  return new Database(path, { strict: true });
}

export class IngestionStore {
  private readonly db: Database;
  private readonly plan: Plan;
  private readonly planText: string;

  static selection(path: string) {
    validateDirectory(path);
    try {
      const file = lstatSync(path);
      if (!file.isFile() || file.isSymbolicLink() || file.size > maximumBytes)
        failure(
          'INVALID_INGESTION_STORE',
          'Use a regular local ingestion store of at most 128 MiB',
        );
      if (file.size === 0) return null;
    } catch (error) {
      if (error instanceof Error && 'code' in error && error.code === 'ENOENT') return null;
      throw error;
    }
    using db = new Database(path, { readonly: true, strict: true });
    db.run('PRAGMA busy_timeout=1000');
    return db.transaction(() => {
      if (!validateIdentity(db)) return null;
      const value = readCohort(db);
      if (value === null) {
        const remaining = db
          .query<{ count: number }, []>('SELECT count(*) AS count FROM units')
          .get();
        if (remaining?.count === 0) return null;
        failure('INVALID_INGESTION_STORE', 'The recorded cohort is missing');
      }
      const parsed = z
        .object({
          snapshot: z.object({ commit: z.string().regex(/^[a-f0-9]{40}$/) }),
          selection: z.object({
            collection: z
              .string()
              .regex(/^[a-z][a-z0-9-]{0,47}$/)
              .nullable(),
          }),
        })
        .safeParse(JSON.parse(value));
      if (!parsed.success)
        failure('INVALID_INGESTION_STORE', 'The recorded source selection is invalid');
      return { ref: parsed.data.snapshot.commit, collection: parsed.data.selection.collection };
    })();
  }

  static result(path: string, id: string, maxBytes: number) {
    if (!IngestionStore.selection(path))
      failure('INGESTION_STORE_EMPTY', 'The store has no recorded ingestion cohort');
    using db = new Database(path, { readonly: true, strict: true });
    db.run('PRAGMA busy_timeout=1000');
    return db.transaction(() => {
      validateIdentity(db);
      const cohort = readCohort(db);
      if (cohort === null) failure('INVALID_INGESTION_STORE', 'The recorded cohort is missing');
      const plan = z
        .object({ planHash: z.string().regex(/^[a-f0-9]{64}$/) })
        .parse(JSON.parse(cohort));
      const row = db
        .query<
          StoredUnit,
          [string]
        >('SELECT id, state, owner, result, result_hash, attempts, attempts_hash FROM units WHERE id=?')
        .get(id);
      if (!row) failure('SOURCE_NOT_FOUND', 'The source does not belong to this ingestion cohort');
      const { result, checkpoint } = decodeUnit(row);
      const response = {
        command: 'ingest',
        mode: 'result',
        accepted: false,
        planHash: plan.planHash,
        source: id,
        state: row.state,
        result,
        checkpoint: result === null ? checkpoint : undefined,
      };
      const requiredBytes = Buffer.byteLength(JSON.stringify(response)) + 1;
      if (requiredBytes > maxBytes)
        throw new HivexError({
          code: 'INGESTION_RESULT_EXCEEDS_BUDGET',
          message: 'Increase --max-bytes to open the complete retained result',
          details: { requiredBytes, maximumBytes: 8 * 1024 * 1024 },
        });
      return response;
    })();
  }

  static discard(path: string, expectedHash: string) {
    if (!/^[a-f0-9]{64}$/.test(expectedHash))
      failure('INVALID_ARGUMENT', 'Discard requires the exact recorded plan hash');
    if (!IngestionStore.selection(path))
      failure('INGESTION_STORE_EMPTY', 'The store has no recorded cohort to discard');
    using db = new Database(path, { strict: true });
    db.run('PRAGMA busy_timeout=1000');
    db.run('PRAGMA trusted_schema=OFF');
    db.run('PRAGMA secure_delete=ON');
    db.transaction(() => {
      validateIdentity(db);
      const value = readCohort(db);
      const plan = z
        .object({ planHash: z.string() })
        .safeParse(value === null ? null : JSON.parse(value));
      if (!plan.success || plan.data.planHash !== expectedHash)
        failure('INGESTION_PLAN_MISMATCH', 'Discard must identify the exact recorded cohort');
      const unresolved = db
        .query<
          { count: number },
          []
        >("SELECT count(*) AS count FROM units WHERE state NOT IN ('pending','candidate','failed') OR owner IS NOT NULL")
        .get();
      if (unresolved?.count !== 0)
        failure(
          'INGESTION_UNRESOLVED',
          'Resolve every claimed invocation before discarding this cohort',
        );
      db.run('DELETE FROM units');
      db.run('DELETE FROM cohort');
    }).immediate();
    return {
      command: 'ingest',
      mode: 'discard',
      accepted: false,
      discarded: true,
      planHash: expectedHash,
    };
  }

  constructor(path: string, plan: Plan) {
    this.plan = plan;
    this.planText = JSON.stringify(plan);
    this.db = openFile(path);
    try {
      this.initialize(plan);
    } catch (error) {
      this.db.close();
      throw error;
    }
  }

  private assertPlan() {
    const current = this.db
      .query<
        { value: string; value_hash: string },
        []
      >('SELECT value, value_hash FROM cohort WHERE id=1')
      .get();
    if (current?.value !== this.planText || current.value_hash !== hash(this.planText))
      failure('INGESTION_PLAN_MISMATCH', "The store no longer contains this invocation's cohort");
  }

  private initialize(plan: Plan) {
    this.db.run('PRAGMA busy_timeout=1000');
    this.db.transaction(() => validateIdentity(this.db))();
    this.db.run('PRAGMA trusted_schema=OFF');
    this.db.run('PRAGMA journal_mode=DELETE');
    this.db.run('PRAGMA synchronous=FULL');
    this.db.run('PRAGMA page_size=4096');
    const pageSize = this.db.query<{ page_size: number }, []>('PRAGMA page_size').get()?.page_size;
    if (pageSize !== 4096)
      failure('INVALID_INGESTION_STORE', 'The ingestion format requires 4096-byte SQLite pages');
    this.db.run('PRAGMA max_page_count=32768');
    this.db
      .transaction(() => {
        validateIdentity(this.db);
        this.db.run(`CREATE TABLE IF NOT EXISTS cohort (
        id INTEGER PRIMARY KEY CHECK (id=1), value TEXT NOT NULL, value_hash TEXT NOT NULL
      ) STRICT`);
        this.db.run(`CREATE TABLE IF NOT EXISTS units (
        id TEXT PRIMARY KEY, ordinal INTEGER NOT NULL UNIQUE,
        state TEXT NOT NULL CHECK (state IN ('pending','running','candidate','failed')),
        owner TEXT, result TEXT, result_hash TEXT,
        attempts TEXT NOT NULL, attempts_hash TEXT NOT NULL
      ) STRICT`);
        const previous = readCohort(this.db);
        const value = this.planText;
        if (Buffer.byteLength(value) > 8 * 1024 * 1024)
          failure('INGESTION_PLAN_TOO_LARGE', 'The complete ingestion plan exceeds 8 MiB');
        if (previous !== null && previous !== value)
          failure(
            'INGESTION_PLAN_MISMATCH',
            'Resume the same committed cohort and processing contract',
          );
        if (previous === null) {
          this.db.run('INSERT INTO cohort VALUES (1, ?, ?)', [value, hash(value)]);
          for (const [ordinal, unit] of plan.units.entries())
            this.db.run(
              "INSERT INTO units (id, ordinal, state, attempts, attempts_hash) VALUES (?, ?, 'pending', ?, ?)",
              [unit.id, ordinal, emptyAttempts, hash(emptyAttempts)],
            );
        }
        this.db.run(`PRAGMA application_id=${applicationId}`);
        this.db.run('PRAGMA user_version=1');
        this.validate(plan);
      })
      .immediate();
  }

  private validate(plan: Plan) {
    const rows = this.db
      .query<
        StoredUnit & { ordinal: number },
        []
      >('SELECT id, ordinal, state, owner, result, result_hash, attempts, attempts_hash FROM units ORDER BY ordinal')
      .all();
    if (
      rows.length !== plan.units.length ||
      rows.some((row, index) => row.id !== plan.units[index]?.id || row.ordinal !== index)
    )
      failure('INVALID_INGESTION_STORE', 'Stored sources do not match the immutable plan');
    for (const row of rows) decodeUnit(row);
  }

  claim(owner: string) {
    return this.db
      .transaction(() => {
        this.assertPlan();
        const next = this.db
          .query<
            { id: string },
            []
          >("SELECT id FROM units WHERE state='pending' ORDER BY ordinal LIMIT 1")
          .get();
        if (!next) return null;
        const pages =
          this.db.query<{ page_count: number }, []>('PRAGMA page_count').get()?.page_count ??
          Infinity;
        const free =
          this.db.query<{ freelist_count: number }, []>('PRAGMA freelist_count').get()
            ?.freelist_count ?? 0;
        const running =
          this.db
            .query<
              { count: number },
              []
            >("SELECT count(*) AS count FROM units WHERE state='running'")
            .get()?.count ?? Infinity;
        if ((pages - free) * 4096 + (running + 1) * reservationBytes > maximumBytes)
          failure(
            'INGESTION_STORE_FULL',
            'The store has no reserved capacity for another extraction',
          );
        this.db.run("UPDATE units SET state='running', owner=? WHERE id=? AND state='pending'", [
          owner,
          next.id,
        ]);
        return next.id;
      })
      .immediate();
  }

  complete(id: string, owner: string, result: Result) {
    const value = JSON.stringify(result);
    if (Buffer.byteLength(value) > reservationBytes / 2)
      failure(
        'INGESTION_RESULT_TOO_LARGE',
        'The result exceeds 8 MiB; its invocation remains unresolved',
      );
    this.db
      .transaction(() => {
        this.assertPlan();
        const updated = this.db.run(
          "UPDATE units SET state=?, result=?, result_hash=?, owner=NULL WHERE id=? AND owner=? AND state='running'",
          [result.status, value, hash(value), id, owner],
        );
        if (updated.changes !== 1)
          failure('INGESTION_CLAIM_LOST', 'The extraction no longer owns its reserved source');
      })
      .immediate();
  }

  checkpoint(id: string, owner: string, event: ExtractionCheckpoint) {
    this.db
      .transaction(() => {
        this.assertPlan();
        const row = this.db
          .query<
            { attempts: string },
            [string, string]
          >("SELECT attempts FROM units WHERE id=? AND owner=? AND state='running'")
          .get(id, owner);
        if (!row) failure('INGESTION_CLAIM_LOST', 'The attempt no longer owns its source');
        const previous = attemptState(row.attempts);
        if (event.attempt !== previous.reports.length + 1)
          failure('INVALID_INGESTION_ATTEMPT', 'Attempt ordering is inconsistent');
        if (event.state === 'started') {
          if (previous.active !== null)
            failure('INVALID_INGESTION_ATTEMPT', 'The previous invocation is unresolved');
          previous.active = event;
        } else {
          if (!previous.active || previous.active.promptHash !== event.report.promptHash)
            failure('INVALID_INGESTION_ATTEMPT', 'The report does not match its invocation');
          previous.reports.push(boundedReport(event.report));
          previous.active = null;
        }
        const value = JSON.stringify(previous);
        if (Buffer.byteLength(value) > 4 * 1024 * 1024)
          failure('INGESTION_REPORT_TOO_LARGE', 'The attempt report exceeds its reserved size');
        this.db.run('UPDATE units SET attempts=?, attempts_hash=? WHERE id=?', [
          value,
          hash(value),
          id,
        ]);
      })
      .immediate();
  }

  private attempts() {
    const summary = { recorded: 0, unresolved: 0, unknownUsage: 0, knownTotalTokens: 0 };
    const rows = this.db.query<{ attempts: string }, []>('SELECT attempts FROM units').all();
    for (const row of rows) {
      const state = attemptState(row.attempts);
      if (state.active !== null) {
        summary.unresolved += 1;
        summary.unknownUsage += 1;
      }
      summary.recorded += state.reports.length;
      for (const report of state.reports) {
        if (report.usage === null) summary.unknownUsage += 1;
        else summary.knownTotalTokens += report.usage.totalTokens;
      }
    }
    return summary;
  }

  private counts() {
    const counts = { completed: 0, failed: 0, pending: 0, unresolved: 0 };
    const rows = this.db
      .query<
        { state: string; count: number },
        []
      >('SELECT state, count(*) AS count FROM units GROUP BY state')
      .all();
    for (const row of rows) {
      if (row.state === 'candidate') counts.completed = row.count;
      if (row.state === 'failed') counts.failed = row.count;
      if (row.state === 'pending') counts.pending = row.count;
      if (row.state === 'running') counts.unresolved = row.count;
    }
    return counts;
  }

  progress() {
    return this.db.transaction(() => {
      this.assertPlan();
      this.validate(this.plan);
      const counts = this.counts();
      let status = 'partial';
      if (counts.failed) status = 'failed';
      else if (!counts.pending && !counts.unresolved) status = 'candidates-ready';
      return { ...counts, status, attempts: this.attempts() };
    })();
  }

  [Symbol.dispose]() {
    this.db.close();
  }
}
