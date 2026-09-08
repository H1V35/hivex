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
import { revisionsSchema, validateHistory, type Revision } from './history.ts';
import { inputUnitSchema, processingSchema, sectionSchema } from '../graph/snapshot.ts';

type Plan = ReturnType<typeof createPlan>;
type Result = Awaited<ReturnType<typeof extractCommand>>;
const maximumBytes = 128 * 1024 * 1024;
const reservationBytes = 16 * 1024 * 1024;
const applicationId = 0x48565849;
const storeVersion = 4;
const supportedStoreVersions = [2, 3, storeVersion];
const digest = z.string().regex(/^[a-f0-9]{64}$/);
const snapshotSchema = z.strictObject({
  commit: z.string().regex(/^[a-f0-9]{40}$/),
  configHash: digest,
});
const associationSchema = z.strictObject({
  planHash: digest,
  snapshot: snapshotSchema,
  originalHash: digest,
});
const reportSchema = z.looseObject({
  outcome: z.string().max(256),
  code: z.string().max(128).optional(),
  usage: usageSchema.nullable(),
  deadlineMilliseconds: z.number().int().min(100).max(1_800_000),
  promptHash: z.string().regex(/^[a-f0-9]{64}$/),
});
const attemptStateSchema = z.object({
  reports: z.array(reportSchema).max(12),
  revisions: revisionsSchema.optional(),
  active: z
    .object({
      attempt: z.number().int().min(1).max(12),
      promptHash: z.string().regex(/^[a-f0-9]{64}$/),
      deadlineMilliseconds: z.number().int().min(100).max(1_800_000),
    })
    .nullable(),
});
const emptyAttempts = JSON.stringify({ reports: [], active: null });
const planSchema = z.strictObject({
  command: z.literal('plan'),
  accepted: z.literal(false),
  planHash: digest,
  version: z.literal(1),
  selection: z.strictObject({ collection: z.string().nullable() }),
  snapshot: snapshotSchema,
  processing: processingSchema,
  units: z.array(inputUnitSchema).max(2048),
  summary: z.strictObject({
    sourceCount: z.number().int().nonnegative(),
    sourceBytes: z.number().int().nonnegative(),
    promptBytes: z.number().int().nonnegative(),
    oversizedSources: z.number().int().nonnegative(),
    modelCalls: z.literal(0),
  }),
});
const sourceInputSchema = z.object({
  id: z.string(),
  path: z.string(),
  contentHash: digest,
  section: sectionSchema,
});
const resultInputSchema = z.object({
  snapshot: snapshotSchema,
  source: sourceInputSchema,
  model: processingSchema.shape.model,
  contract: z.object({
    nativeVersion: z.string(),
    requestedPolicyHash: digest,
    basePromptHash: digest,
    schemaHash: digest,
  }),
});
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
  attempts: z.array(reportSchema).min(1).max(12),
  revisions: revisionsSchema.optional(),
  candidate: candidateSchema.nullable(),
  candidateAttempt: z.number().int().min(1).max(12).nullable(),
  association: associationSchema.optional(),
});
const rowStateSchema = z.enum(['pending', 'running', 'candidate', 'failed']);
const rowSchema = z.strictObject({
  id: z.string(),
  state: rowStateSchema,
  result: resultSchema.nullable(),
  checkpoint: attemptStateSchema,
});
export type IngestionResult = z.infer<typeof resultSchema>;
type StoredResult = IngestionResult;
export type IngestionRow = z.infer<typeof rowSchema>;
export type IngestionCohort = { plan: Plan; rows: IngestionRow[]; transitionHash: string | null };

function expectedPlanSummary(plan: Pick<Plan, 'units'>) {
  return {
    sourceCount: plan.units.length,
    sourceBytes: plan.units.reduce((total, unit) => total + unit.sourceBytes, 0),
    promptBytes: plan.units.reduce((total, unit) => total + unit.promptBytes, 0),
    oversizedSources: plan.units.filter((unit) => unit.readiness === 'requires-section').length,
    modelCalls: 0 as const,
  };
}

function parsePlan(value: unknown): Plan {
  const parsed = planSchema.safeParse(value);
  if (!parsed.success) failure('INVALID_INGESTION_STORE', 'The recorded ingestion plan is invalid');
  const raw = z.record(z.string(), z.unknown()).parse(value);
  const { command: _command, accepted: _accepted, planHash, summary, ...cohort } = raw;
  if (
    planHash !== hash(JSON.stringify(cohort)) ||
    !isDeepStrictEqual(summary, expectedPlanSummary(parsed.data))
  )
    failure('INVALID_INGESTION_STORE', 'The recorded ingestion plan is altered');
  return value as Plan;
}

export function parseIngestionPlan(value: unknown) {
  return parsePlan(value);
}

export function originalExtractionResult(result: IngestionResult) {
  const { association: _association, ...original } = result;
  return original;
}

export function validateResultForPlan(value: unknown, id: string, plan: Plan): StoredResult {
  const parsed = resultSchema.safeParse(value);
  if (!parsed.success || parsed.data.source.id !== id)
    failure('INVALID_INGESTION_STORE', 'The retained extraction is invalid');
  const inputs = resultInputSchema.safeParse(parsed.data);
  const unit = plan.units.find((item) => item.id === id);
  if (!inputs.success || !unit)
    failure('INVALID_INGESTION_STORE', 'The retained extraction inputs are invalid');
  const association = parsed.data.association;
  if (
    (association &&
      (association.planHash !== plan.planHash ||
        !isDeepStrictEqual(association.snapshot, plan.snapshot) ||
        association.originalHash !==
          hash(JSON.stringify(originalExtractionResult(parsed.data))))) ||
    (!association && !isDeepStrictEqual(inputs.data.snapshot, plan.snapshot)) ||
    !isDeepStrictEqual(inputs.data.model, plan.processing.model) ||
    !isDeepStrictEqual(inputs.data.source, {
      id: unit.id,
      path: unit.path,
      contentHash: unit.contentHash,
      section: unit.section,
    }) ||
    !isDeepStrictEqual(inputs.data.contract, {
      nativeVersion: plan.processing.nativeVersion,
      requestedPolicyHash: plan.processing.requestedPolicyHash,
      basePromptHash: unit.basePromptHash,
      schemaHash: plan.processing.schemaHash,
    })
  )
    failure('INVALID_INGESTION_STORE', 'The retained extraction differs from its planned inputs');
  return parsed.data;
}

function validatePhase(row: StoredUnit, state: z.infer<typeof attemptStateSchema>) {
  validateHistory(state.revisions ?? [], state.reports.length);
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

function decodeUnit(row: StoredUnit, plan: Plan): IngestionRow {
  if (hash(row.attempts) !== row.attempts_hash)
    failure('INVALID_INGESTION_STORE', 'A stored attempt report was altered');
  const checkpoint = attemptState(row.attempts);
  validatePhase(row, checkpoint);
  const finished = row.state === 'candidate' || row.state === 'failed';
  if (finished !== (row.result !== null) || (row.result === null && row.result_hash !== null))
    failure('INVALID_INGESTION_STORE', 'A source state is inconsistent with its retained result');
  if (row.result === null)
    return { id: row.id, state: rowStateSchema.parse(row.state), result: null, checkpoint };
  if (hash(row.result) !== row.result_hash)
    failure('INVALID_INGESTION_STORE', 'A stored result was altered');
  const raw = z.record(z.string(), z.unknown()).parse(JSON.parse(row.result));
  const parsed = resultSchema.safeParse(raw);
  if (!parsed.success) failure('INVALID_INGESTION_STORE', 'The retained extraction is invalid');
  const result = validateResultForPlan(parsed.data, row.id, plan);
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
  if (!isDeepStrictEqual(result.revisions, checkpoint.revisions))
    failure('INVALID_INGESTION_STORE', 'The result differs from its retained revision history');
  return { id: row.id, state: rowStateSchema.parse(row.state), result, checkpoint };
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

function retryIsSafe(report: z.infer<typeof reportSchema>) {
  if (report.outcome === 'completed') return false;
  if (report.cleanup === 'failed' || report.turnAccepted === 'unknown') return false;
  if (report.code === 'MODEL_ADMISSION_FAILED')
    return (
      report.turnAccepted === undefined &&
      ['confirmed', 'not-observed'].includes(String(report.cleanup))
    );
  if (report.cleanup !== 'confirmed' || report.turnAccepted !== 'confirmed') return false;
  return report.outcome === 'invalid-output' || report.interruption === 'confirmed';
}

export function unresolvedInvocation(report: z.infer<typeof reportSchema>) {
  const value = report as Record<string, unknown>;
  if (
    value.turnAccepted === 'unknown' ||
    value.cleanup === 'failed' ||
    value.interruption === 'unconfirmed'
  )
    return true;
  if (value.turnAccepted !== 'confirmed')
    return !(
      value.turnAccepted === undefined &&
      value.code === 'MODEL_ADMISSION_FAILED' &&
      ['confirmed', 'not-observed'].includes(String(value.cleanup))
    );
  return (
    value.cleanup !== 'confirmed' ||
    (!['completed', 'invalid-output'].includes(String(value.outcome)) &&
      value.interruption !== 'confirmed')
  );
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

function readUnits(db: Database, plan: Plan) {
  const rows = db
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
  return rows.map((row) => decodeUnit(row, plan));
}

export function validateRows(plan: Plan, value: unknown[]): IngestionRow[] {
  const parsed = z.array(rowSchema).max(2048).safeParse(value);
  if (!parsed.success)
    failure('INVALID_INGESTION_STORE', 'The exported ingestion rows are invalid');
  if (
    parsed.data.length !== plan.units.length ||
    parsed.data.some((row, index) => row.id !== plan.units[index]?.id)
  )
    failure('INVALID_INGESTION_STORE', 'Exported sources do not match the immutable plan');
  return parsed.data.map((row) =>
    decodeUnit(
      {
        id: row.id,
        state: row.state,
        owner: row.state === 'running' ? 'archive' : null,
        result: row.result === null ? null : JSON.stringify(row.result),
        result_hash: row.result === null ? null : hash(JSON.stringify(row.result)),
        attempts: JSON.stringify(row.checkpoint),
        attempts_hash: hash(JSON.stringify(row.checkpoint)),
      },
      plan,
    ),
  );
}

export function summarizeRows(rows: IngestionRow[]) {
  const completed = rows.filter((row) => row.state === 'candidate').length;
  const failed = rows.filter((row) => row.state === 'failed').length;
  const pending = rows.filter((row) => row.state === 'pending').length;
  const unresolved = rows.filter((row) => row.state === 'running').length;
  const reports = rows.flatMap((row) => row.checkpoint.reports);
  let status = 'partial';
  if (failed) status = 'failed';
  else if (!pending && !unresolved) status = 'candidates-ready';
  return {
    completed,
    failed,
    pending,
    unresolved,
    status,
    attempts: {
      recorded: reports.length,
      unresolved: rows.filter((row) => row.checkpoint.active !== null).length,
      unknownUsage: reports.filter((report) => report.usage === null).length,
      knownTotalTokens: reports.reduce(
        (total, report) => total + (report.usage?.totalTokens ?? 0),
        0,
      ),
    },
  };
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
  if (
    identity?.application_id === applicationId &&
    supportedStoreVersions.includes(version?.user_version ?? 0)
  )
    return true;
  const objects = db
    .query<{ count: number }, []>('SELECT count(*) AS count FROM sqlite_schema')
    .get();
  if (identity?.application_id === 0 && version?.user_version === 0 && objects?.count === 0)
    return false;
  failure('INVALID_INGESTION_STORE', 'Use an empty database or a supported Hivex ingestion store');
}

function hasTransitionColumn(db: Database) {
  return db
    .query<{ name: string }, []>('PRAGMA table_info(cohort)')
    .all()
    .some((column) => column.name === 'transition_hash');
}

function readTransitionHash(db: Database) {
  if (!hasTransitionColumn(db)) return null;
  return (
    db
      .query<
        { transition_hash: string | null },
        []
      >('SELECT transition_hash FROM cohort WHERE id=1')
      .get()?.transition_hash ?? null
  );
}

function ensureTransitionColumn(db: Database) {
  if (!hasTransitionColumn(db)) db.run('ALTER TABLE cohort ADD COLUMN transition_hash TEXT');
}

function recordTransition(db: Database, transition: string) {
  ensureTransitionColumn(db);
  db.run('UPDATE cohort SET transition_hash=? WHERE id=1', [transition]);
  db.run(`PRAGMA user_version=${storeVersion}`);
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

function transitionDigest(previous: { plan: Plan; rows: IngestionRow[] }) {
  return hash(JSON.stringify(previous));
}

export function assertTransferSafe(rows: IngestionRow[]) {
  if (
    rows.some(
      (row) =>
        row.state === 'running' ||
        row.checkpoint.active !== null ||
        row.checkpoint.reports.some((report) => unresolvedInvocation(report)),
    )
  )
    failure(
      'INGESTION_UNRESOLVED',
      'Resolve every claimed or uncertain invocation before replacing this cohort',
    );
}

function assertArchivedRows(
  currentPlan: Plan,
  currentRows: IngestionRow[],
  previous: { plan: Plan; rows: IngestionRow[] },
) {
  if (
    !isDeepStrictEqual(currentPlan, previous.plan) ||
    !isDeepStrictEqual(currentRows, previous.rows)
  )
    failure(
      'INGESTION_ARCHIVE_MISMATCH',
      'Preserve an exact complete export of the retained ingestion cohort before replacement',
    );
  assertTransferSafe(currentRows);
}

function validateReuseResults(plan: Plan, results: Map<string, IngestionResult>) {
  for (const [id, result] of results) {
    if (!plan.units.some((unit) => unit.id === id))
      failure('INGESTION_ARCHIVE_MISMATCH', 'A reused source is outside the new ingestion plan');
    validateResultForPlan(result, id, plan);
  }
}

function replaceCohort(
  db: Database,
  previous: { plan: Plan; rows: IngestionRow[] },
  next: { plan: Plan; results: Map<string, IngestionResult> },
  transition: string,
) {
  const nextText = JSON.stringify(next.plan);
  ensureTransitionColumn(db);
  db.run('DELETE FROM units');
  db.run('UPDATE cohort SET value=?, value_hash=?, transition_hash=? WHERE id=1', [
    nextText,
    hash(nextText),
    transition,
  ]);
  for (const [ordinal, unit] of next.plan.units.entries())
    db.run(
      "INSERT INTO units (id, ordinal, state, attempts, attempts_hash) VALUES (?, ?, 'pending', ?, ?)",
      [unit.id, ordinal, emptyAttempts, hash(emptyAttempts)],
    );
  for (const [id, result] of next.results) {
    const archived = previous.rows.find((row) => row.id === id);
    if (!archived)
      failure('INGESTION_ARCHIVE_MISMATCH', 'A reused source is missing its archive row');
    const resultText = JSON.stringify(result);
    if (Buffer.byteLength(resultText) > reservationBytes / 2)
      failure('INGESTION_RESULT_TOO_LARGE', 'A reused result exceeds 8 MiB');
    const attempts = JSON.stringify(archived.checkpoint);
    db.run(
      'UPDATE units SET state=?, result=?, result_hash=?, attempts=?, attempts_hash=? WHERE id=?',
      [result.status, resultText, hash(resultText), attempts, hash(attempts), id],
    );
  }
  recordTransition(db, transition);
}

export class IngestionStore {
  private readonly db: Database;
  private readonly plan: Plan;
  private readonly planText: string;

  static read(path: string): IngestionCohort {
    validateDirectory(path);
    try {
      const file = lstatSync(path);
      if (!file.isFile() || file.isSymbolicLink() || file.size > maximumBytes)
        failure(
          'INVALID_INGESTION_STORE',
          'Use a regular local ingestion store of at most 128 MiB',
        );
      if (file.size === 0)
        failure('INGESTION_STORE_EMPTY', 'The store has no recorded ingestion cohort');
    } catch (error) {
      if (error instanceof Error && 'code' in error && error.code === 'ENOENT')
        failure('INGESTION_STORE_EMPTY', 'The store has no recorded ingestion cohort');
      throw error;
    }
    using db = new Database(path, { readonly: true, strict: true });
    db.run('PRAGMA busy_timeout=1000');
    return db.transaction(() => {
      validateIdentity(db);
      const value = readCohort(db);
      if (value === null) failure('INVALID_INGESTION_STORE', 'The recorded cohort is missing');
      const plan = parsePlan(JSON.parse(value));
      const rows = readUnits(db, plan);
      return { plan, rows, transitionHash: readTransitionHash(db) };
    })();
  }

  static export(path: string, maxBytes: number) {
    const cohort = IngestionStore.read(path);
    const result = {
      command: 'ingest',
      mode: 'export',
      accepted: false,
      planHash: cohort.plan.planHash,
      plan: cohort.plan,
      units: cohort.rows,
      ...summarizeRows(cohort.rows),
    };
    const requiredBytes = Buffer.byteLength(JSON.stringify(result)) + 1;
    if (requiredBytes > maxBytes)
      throw new HivexError({
        code: 'INGESTION_EXPORT_EXCEEDS_BUDGET',
        message: 'The complete ingestion export exceeds the output budget',
        details: { requiredBytes, maximumBytes },
      });
    return result;
  }

  static candidates(path: string, plan: Plan) {
    if (!IngestionStore.selection(path))
      failure('INGESTION_NOT_READY', 'A complete ingestion cohort is required');
    using db = new Database(path, { readonly: true, strict: true });
    db.run('PRAGMA busy_timeout=1000');
    return db.transaction(() => {
      validateIdentity(db);
      if (readCohort(db) !== JSON.stringify(plan))
        failure(
          'INGESTION_PLAN_MISMATCH',
          'The retained cohort does not match its source and processing inputs',
        );
      return readUnits(db, plan).map((unit) => {
        if (unit.result?.status !== 'candidate')
          failure('INGESTION_NOT_READY', 'Every declared source needs a retained candidate');
        return unit.result;
      });
    })();
  }

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
      const plan = parsePlan(JSON.parse(value));
      return { ref: plan.snapshot.commit, collection: plan.selection.collection };
    })();
  }

  static result(path: string, id: string, maxBytes: number) {
    if (!IngestionStore.selection(path))
      failure('INGESTION_STORE_EMPTY', 'The store has no recorded ingestion cohort');
    using db = new Database(path, { readonly: true, strict: true });
    db.run('PRAGMA busy_timeout=1000');
    const response = db.transaction(() => {
      validateIdentity(db);
      const value = readCohort(db);
      if (value === null) failure('INVALID_INGESTION_STORE', 'The recorded cohort is missing');
      const plan = parsePlan(JSON.parse(value));
      const row = db
        .query<
          StoredUnit,
          [string]
        >('SELECT id, state, owner, result, result_hash, attempts, attempts_hash FROM units WHERE id=?')
        .get(id);
      if (!row) failure('SOURCE_NOT_FOUND', 'The source does not belong to this ingestion cohort');
      const decoded = decodeUnit(row, plan);
      return {
        command: 'ingest',
        mode: 'result',
        accepted: false,
        planHash: plan.planHash,
        source: id,
        state: row.state,
        resultHash: row.result_hash,
        result: decoded.result,
        checkpoint: decoded.result === null ? decoded.checkpoint : undefined,
      };
    })();
    const requiredBytes = Buffer.byteLength(JSON.stringify(response)) + 1;
    if (requiredBytes > maxBytes)
      throw new HivexError({
        code: 'INGESTION_RESULT_EXCEEDS_BUDGET',
        message: 'Increase --max-bytes to open the complete retained result',
        details: { requiredBytes, maximumBytes: 8 * 1024 * 1024 },
      });
    return response;
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
      const plan = value === null ? null : parsePlan(JSON.parse(value));
      if (!plan || plan.planHash !== expectedHash)
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

  static refresh(options: {
    path: string;
    previous: { plan: Plan; rows: IngestionRow[] };
    next: { plan: Plan; results: Map<string, IngestionResult> };
  }): IngestionCohort {
    try {
      lstatSync(options.path);
    } catch (error) {
      if (error instanceof Error && 'code' in error && error.code === 'ENOENT')
        failure('INGESTION_STORE_EMPTY', 'Reuse requires the existing ingestion store');
      throw error;
    }
    using db = openFile(options.path);
    db.run('PRAGMA busy_timeout=1000');
    db.run('PRAGMA trusted_schema=OFF');
    db.run('PRAGMA synchronous=FULL');
    db.run('PRAGMA max_page_count=32768');
    return db
      .transaction(() => {
        validateIdentity(db);
        const value = readCohort(db);
        if (value === null) failure('INVALID_INGESTION_STORE', 'The recorded cohort is missing');
        const currentPlan = parsePlan(JSON.parse(value));
        const currentRows = readUnits(db, currentPlan);
        const previousText = JSON.stringify(options.previous.plan);
        const nextText = JSON.stringify(options.next.plan);
        const transition = transitionDigest(options.previous);

        if (value === nextText && nextText !== previousText) {
          if (readTransitionHash(db) !== transition)
            failure(
              'INGESTION_ARCHIVE_MISMATCH',
              'Reuse requires the retained old cohort or its exact recorded transition archive',
            );
          return { plan: currentPlan, rows: currentRows, transitionHash: transition };
        }
        if (value !== previousText)
          failure(
            'INGESTION_PLAN_MISMATCH',
            'The retained store is neither the archived cohort nor the requested destination',
          );
        assertArchivedRows(currentPlan, currentRows, options.previous);
        if (value === nextText)
          return { plan: currentPlan, rows: currentRows, transitionHash: null };
        if (Buffer.byteLength(nextText) > 8 * 1024 * 1024 || options.next.plan.units.length > 2048)
          failure('INGESTION_PLAN_TOO_LARGE', 'The complete ingestion plan exceeds 8 MiB');
        validateReuseResults(options.next.plan, options.next.results);
        replaceCohort(db, options.previous, options.next, transition);
        const rows = readUnits(db, options.next.plan);
        return { plan: options.next.plan, rows, transitionHash: transition };
      })
      .immediate();
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
    validateIdentity(this.db);
    const current = this.db
      .query<
        { value: string; value_hash: string },
        []
      >('SELECT value, value_hash FROM cohort WHERE id=1')
      .get();
    if (current?.value !== this.planText || current.value_hash !== hash(this.planText))
      failure('INGESTION_PLAN_MISMATCH', "The store no longer contains this invocation's cohort");
    parsePlan(JSON.parse(current.value));
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
        id INTEGER PRIMARY KEY CHECK (id=1), value TEXT NOT NULL, value_hash TEXT NOT NULL,
        transition_hash TEXT
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
        ensureTransitionColumn(this.db);
        if (previous === null) {
          this.db.run(
            'INSERT INTO cohort (id, value, value_hash, transition_hash) VALUES (1, ?, ?, NULL)',
            [value, hash(value)],
          );
          for (const [ordinal, unit] of plan.units.entries())
            this.db.run(
              "INSERT INTO units (id, ordinal, state, attempts, attempts_hash) VALUES (?, ?, 'pending', ?, ?)",
              [unit.id, ordinal, emptyAttempts, hash(emptyAttempts)],
            );
        }
        this.db.run(`PRAGMA application_id=${applicationId}`);
        this.db.run(`PRAGMA user_version=${storeVersion}`);
        this.validate(plan);
      })
      .immediate();
  }

  private validate(plan: Plan) {
    readUnits(this.db, plan);
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
        this.reserve(1);
        this.db.run("UPDATE units SET state='running', owner=? WHERE id=? AND state='pending'", [
          owner,
          next.id,
        ]);
        return next.id;
      })
      .immediate();
  }

  retryFailed(id: string, owner: string, maximumAttempts: number) {
    return this.db
      .transaction(() => {
        this.assertPlan();
        const row = this.db.query<StoredUnit, [string]>('SELECT * FROM units WHERE id=?').get(id);
        if (!row || row.state !== 'failed')
          failure(
            'INGESTION_RETRY_NOT_ALLOWED',
            'Only a retained failed source can be explicitly retried',
          );
        const { result, checkpoint } = decodeUnit(row, this.plan);
        const reports = result?.attempts;
        const last = reports?.at(-1);
        if (!reports || !last || checkpoint.active || !retryIsSafe(last))
          failure(
            'INGESTION_RETRY_UNSAFE',
            'The previous invocation must have a confirmed safe end before retry',
          );
        if (reports.length >= (checkpoint.revisions?.at(-1)?.afterAttempt ?? 0) + maximumAttempts)
          failure(
            'INGESTION_ATTEMPTS_EXHAUSTED',
            'The source has exhausted its total attempt budget',
          );
        this.db.run(
          "UPDATE units SET state='running', owner=?, result=NULL, result_hash=NULL WHERE id=?",
          [owner, id],
        );
        this.reserve(0);
        return { previousAttempts: reports, revisions: checkpoint.revisions };
      })
      .immediate();
  }

  revise(id: string, owner: string, revision: Revision) {
    return this.db
      .transaction(() => {
        this.assertPlan();
        const row = this.db.query<StoredUnit, [string]>('SELECT * FROM units WHERE id=?').get(id);
        if (!row || row.state !== 'candidate' || row.result_hash !== revision.previousResultHash)
          failure('INGESTION_REVISION_MISMATCH', 'Revision requires the exact retained candidate');
        const { result, checkpoint } = decodeUnit(row, this.plan);
        if (
          !result ||
          !isDeepStrictEqual(result.candidate, revision.candidate) ||
          result.attempts.length !== revision.afterAttempt
        )
          failure(
            'INGESTION_REVISION_MISMATCH',
            'The revision does not preserve the candidate and its attempts',
          );
        const last = result.attempts.at(-1);
        if (
          last?.outcome !== 'completed' ||
          last.cleanup !== 'confirmed' ||
          last.turnAccepted !== 'confirmed'
        )
          failure(
            'INGESTION_REVISION_UNSAFE',
            'The previous extraction must have a confirmed completed invocation',
          );
        const revisions = revisionsSchema.parse([...(checkpoint.revisions ?? []), revision]);
        validateHistory(revisions, checkpoint.reports.length);
        const value = JSON.stringify({ ...checkpoint, revisions });
        if (Buffer.byteLength(value) > 4 * 1024 * 1024)
          failure('INGESTION_REPORT_TOO_LARGE', 'Revision evidence exceeds the checkpoint budget');
        this.reserve(1);
        this.db.run(
          "UPDATE units SET state='running', owner=?, result=NULL, result_hash=NULL, attempts=?, attempts_hash=? WHERE id=?",
          [owner, value, hash(value), id],
        );
        return { previousAttempts: result.attempts, revisions };
      })
      .immediate();
  }

  private reserve(additionalClaims: number) {
    const pages =
      this.db.query<{ page_count: number }, []>('PRAGMA page_count').get()?.page_count ?? Infinity;
    const free =
      this.db.query<{ freelist_count: number }, []>('PRAGMA freelist_count').get()
        ?.freelist_count ?? 0;
    const running =
      this.db
        .query<{ count: number }, []>("SELECT count(*) AS count FROM units WHERE state='running'")
        .get()?.count ?? Infinity;
    if ((pages - free) * 4096 + (running + additionalClaims) * reservationBytes > maximumBytes)
      failure('INGESTION_STORE_FULL', 'The store has no reserved capacity for another extraction');
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
