import { randomUUID } from 'node:crypto';
import {
  closeSync,
  lstatSync,
  mkdirSync,
  openSync,
  readFileSync,
  unlinkSync,
  writeFileSync,
} from 'node:fs';
import path from 'node:path';
import { Database } from 'bun:sqlite';
import { z } from 'zod';
import { HivexError } from './errors.ts';
import { sharedKnowledge } from './knowledge-snapshot.ts';
import { emptyGraph, extractionSchema, graphSchema } from './knowledge-model.ts';
import type { Graph } from './knowledge-model.ts';

const processIdSchema = z.number().int().positive();
const lockSchema = z.object({ id: z.string().min(1), pid: processIdSchema });
const recoveryAcknowledgementSchema = z.object({
  acknowledgedAt: z.string(),
  nativeProcessId: processIdSchema,
  type: z.literal('uncertain-invocation'),
});
const workConflictCode = 'WORK_CONFLICT';
const lockFilename = 'knowledge.lock';
const workByIdQuery = 'SELECT data FROM work WHERE id=?';
const attemptSchema = z.object({
  diagnostic: z.string().optional(),
  error: z.string().optional(),
  inputBytes: z.number(),
  inputHash: z.string(),
  outputHash: z.string().optional(),
  recoveryAcknowledgement: recoveryAcknowledgementSchema.optional(),
  report: z.unknown().optional(),
  result: z.unknown().optional(),
  stage: z.string(),
});
const workSchema = z.object({
  attempts: z.array(attemptSchema).max(4096),
  cacheHits: z.number().int().nonnegative().default(0),
  calls: z.number().int().nonnegative(),
  contextLimit: z
    .object({
      documents: z.array(z.string()),
      maxBytes: z.number(),
      requiredBytes: z.number(),
    })
    .optional(),
  id: z.string(),
  inputBytes: z.number().int().nonnegative(),
  key: z.string(),
  kind: z.enum(['update', 'ask', 'review']),
  materializedChecks: z.boolean().optional(),
  maxCalls: z.number().int().nonnegative(),
  maxInputBytes: z.number().int().positive(),
  nativeProcessId: processIdSchema.optional(),
  ownerPid: processIdSchema.optional(),
  pending: z
    .object({
      baseExtraction: z.string().nullable().optional(),
      batch: z.string(),
      context: z.array(z.string()).default([]),
      documents: z.array(z.string()),
      existing: z.array(z.string()).default([]),
      extraction: extractionSchema,
      materializedCheck: z.boolean().optional(),
      packet: z.record(z.string(), z.unknown()).optional(),
      protectedRelationships: z.array(z.string()).optional(),
      staged: z.boolean().optional(),
      units: z.array(z.string()).default([]),
    })
    .nullable(),
  phase: z.enum(['update', 'ask', 'review']).default('update'),
  plannedUnits: z.array(z.string()).default([]),
  remaining: z.array(z.string()),
  result: z.unknown().optional(),
  resultKey: z.string().optional(),
  snapshot: z.string(),
  status: z.enum(['pending', 'running', 'budget-exhausted', 'context-limit', 'failed', 'done']),
  totalTokens: z.number().int().nonnegative(),
});
export type Work = z.infer<typeof workSchema>;
interface StoreOptions {
  readonly?: boolean;
  update?: boolean;
}
interface BeginWork {
  key: string;
  kind: Work['kind'];
  maxCalls?: number;
  maxInputBytes?: number;
  remaining: string[];
  resultKey?: string;
  snapshot: string;
}
const recoveryLockValues = ['absent', 'released', 'held', 'unreadable', 'changed'] as const;
type RecoveryLockStatus = (typeof recoveryLockValues)[number];
export interface RecoveryReport {
  acknowledgedWorks: number;
  guidance?: string;
  interruptedWorks: number;
  lock: RecoveryLockStatus;
  status: 'clean' | 'recovered' | 'blocked';
}
export interface RecoveryOptions {
  acknowledgeUncertain?: boolean;
}
export interface PruneOptions {
  keepCaches: number;
  keepCompleted: number;
}
export interface PruneReport {
  deletedCaches: number;
  deletedCompletedWorks: number;
  retainedCaches: number;
  retainedCompletedWorks: number;
  unfinishedWorks: number;
}
type ProcessState = 'alive' | 'dead' | 'unknown';
interface RecoveryLock {
  pid: number;
  raw: string;
}
const errorCode = function errorCode(error: unknown) {
  return Error.isError(error) && 'code' in error && typeof error.code === 'string'
    ? error.code
    : undefined;
};
const processState = function processState(pid: number): ProcessState {
  try {
    process.kill(pid, 0);
    return 'alive';
  } catch (error) {
    const code = errorCode(error);
    if (code === 'ESRCH') {
      return 'dead';
    }
    if (code === 'EPERM') {
      return 'alive';
    }
    return 'unknown';
  }
};
const recordValue = function recordValue(value: unknown): Record<string, unknown> | null {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    return null;
  }
  return Object.fromEntries(Object.entries(value));
};
const interruptedReport = function interruptedReport(previous: unknown, nativeProcessId: number) {
  const report = recordValue(previous) ?? {};
  return {
    ...report,
    cleanup: 'not-observed',
    code: 'MODEL_INTERRUPTED_RECOVERED',
    interruption: 'unconfirmed',
    outcome: 'interrupted',
    recovery: {
      nativeProcessEnded: true,
      nativeProcessId,
      previousOutcome: typeof report.outcome === 'string' ? report.outcome : null,
    },
    turnAccepted: typeof report.turnAccepted === 'string' ? report.turnAccepted : 'unknown',
    usage: report.usage ?? null,
  };
};
const deleteRows = function deleteRows(
  database: Database,
  table: 'work' | 'model_cache',
  rowids: number[]
) {
  if (rowids.length === 0) {
    return;
  }
  const placeholders = rowids.map(() => '?').join(',');
  database.run(`DELETE FROM ${table} WHERE rowid IN (${placeholders})`, rowids);
};
const throwRecovery: (
  lock: RecoveryReport['lock'],
  guidance: string,
  interruptedWorks?: number
) => never = function throwRecovery(lock, guidance, interruptedWorks = 0) {
  throw new HivexError({
    code: 'RECOVERY_UNSAFE',
    details: { interruptedWorks, lock },
    message: guidance,
  });
};
const blockedRecovery = function blockedRecovery(error: HivexError): RecoveryReport {
  const lock = error.details?.lock;
  const knownLocks = recoveryLockValues.filter((candidate) => candidate !== 'released');
  return {
    acknowledgedWorks: 0,
    guidance: error.message,
    interruptedWorks:
      typeof error.details?.interruptedWorks === 'number' ? error.details.interruptedWorks : 0,
    lock: knownLocks.find((candidate) => candidate === lock) ?? 'unreadable',
    status: 'blocked',
  };
};
const releaseLockFile = function releaseLockFile(lockPath: string, token: string) {
  let contents: string;
  try {
    contents = readFileSync(lockPath, 'utf-8');
  } catch (error) {
    if (errorCode(error) === 'ENOENT') {
      return;
    }
    throw error;
  }
  if (contents === token) {
    unlinkSync(lockPath);
  }
};
const hasUnfinishedWork = function hasUnfinishedWork(database: Database) {
  return database
    .query<
      {
        data: string;
      },
      []
    >('SELECT data FROM work')
    .all()
    .some((row) => workSchema.parse(JSON.parse(row.data)).status !== 'done');
};
const saveWork = function saveWork(database: Database, work: Work) {
  if (work.status !== 'running') {
    delete work.nativeProcessId;
  }
  database.run(
    'INSERT INTO work VALUES(?,?,?,?) ON CONFLICT(id) DO UPDATE SET data=excluded.data',
    [work.id, work.kind, work.key, JSON.stringify(work)]
  );
};
const allWorks = function allWorks(database: Database) {
  return database
    .query<
      {
        data: string;
      },
      []
    >('SELECT data FROM work')
    .all()
    .map(({ data }) => workSchema.parse(JSON.parse(data)));
};
const runningWorks = function runningWorks(database: Database) {
  return allWorks(database).filter((work) => work.status === 'running');
};
const uncertainFailedWorks = function uncertainFailedWorks(database: Database) {
  return allWorks(database).flatMap((work) => {
    const attempt = work.attempts.at(-1);
    if (work.status !== 'failed' || attempt?.recoveryAcknowledgement !== undefined) {
      return [];
    }
    const report = recordValue(attempt?.report);
    if (
      report === null ||
      (report.interruption !== 'unconfirmed' && report.turnAccepted !== 'unknown')
    ) {
      return [];
    }
    const nativeProcessId = processIdSchema.safeParse(report.nativeProcessId);
    return [
      {
        nativeProcessId: nativeProcessId.success ? nativeProcessId.data : undefined,
        work,
      },
    ];
  });
};
const recoveryLock = function recoveryLock(directory: string): RecoveryLock | null {
  const lockPath = path.join(directory, lockFilename);
  let raw: string;
  try {
    raw = readFileSync(lockPath, 'utf-8');
  } catch (error) {
    if (errorCode(error) === 'ENOENT') {
      return null;
    }
    return throwRecovery(
      'unreadable',
      'knowledge.lock cannot be read safely; inspect the store before continuing.'
    );
  }
  try {
    const lock = lockSchema.parse(JSON.parse(raw));
    return { pid: lock.pid, raw };
  } catch {
    return throwRecovery(
      'unreadable',
      'knowledge.lock has no verifiable PID; do not delete it and inspect the process manually.'
    );
  }
};
const runningWorksOrBlock = function runningWorksOrBlock(database: Database) {
  try {
    return runningWorks(database);
  } catch {
    return throwRecovery(
      'unreadable',
      'Work state cannot be validated; preserve the store and inspect it manually.'
    );
  }
};
const assertOwnerEnded = function assertOwnerEnded(
  ownerPid: number,
  lock: RecoveryReport['lock'],
  label = 'The lock owner'
) {
  const state = processState(ownerPid);
  if (state === 'alive') {
    throwRecovery(
      lock,
      `${label} (PID ${ownerPid}) is still alive; no process was modified or terminated.`
    );
  }
  if (state !== 'dead') {
    throwRecovery(lock, `${label} (PID ${ownerPid}) cannot be proven dead; no state was modified.`);
  }
};
const assertRecoverable = function assertRecoverable(
  work: Work,
  nativeProcessId: number | undefined,
  lock: RecoveryReport['lock']
) {
  if (work.ownerPid === undefined) {
    throwRecovery(
      lock,
      `Work ${work.id} has no recorded owner PID; its recovery state is unchanged.`
    );
  }
  assertOwnerEnded(work.ownerPid, lock, `Work ${work.id} owner`);
  const attempt = work.attempts.at(-1);
  if (attempt === undefined) {
    throwRecovery(lock, `Work ${work.id} has no reserved attempt; no state was changed.`);
  }
  if (nativeProcessId === undefined && work.status === 'running') {
    return;
  }
  if (nativeProcessId === undefined) {
    throwRecovery(
      lock,
      `Work ${work.id} has no native PID for its uncertain result; no state was changed.`
    );
  }
  const state = processState(nativeProcessId);
  if (state === 'alive') {
    throwRecovery(
      lock,
      `Native process PID ${nativeProcessId} for work ${work.id} is still alive; no process was killed.`
    );
  }
  if (state !== 'dead') {
    throwRecovery(
      lock,
      `Native process PID ${nativeProcessId} for work ${work.id} cannot be checked; no state was changed.`
    );
  }
  const { report } = attempt;
  if (report !== undefined && recordValue(report) === null) {
    throwRecovery(
      lock,
      `Work ${work.id} has an unstructured running report; recovery left it unchanged.`
    );
  }
};
const recordRecovery = function recordRecovery(
  database: Database,
  works: {
    nativeProcessId: number | undefined;
    work: Work;
  }[]
) {
  database.transaction(() => {
    for (const { nativeProcessId, work } of works) {
      const row = database
        .query<
          {
            data: string;
          },
          [string]
        >(workByIdQuery)
        .get(work.id);
      const current = row?.data === undefined ? undefined : workSchema.parse(JSON.parse(row.data));
      const attempt = current?.attempts.at(-1);
      if (
        attempt === undefined ||
        current?.status !== work.status ||
        current.calls !== work.calls
      ) {
        throwRecovery('changed', `Work ${work.id} changed during recovery; run recover again.`);
      }
      attempt.report ??=
        nativeProcessId === undefined
          ? {
              cleanup: 'not-observed',
              code: 'MODEL_INTERRUPTED_BEFORE_TURN',
              outcome: 'interrupted',
              usage: null,
            }
          : interruptedReport(undefined, nativeProcessId);
      if (nativeProcessId !== undefined) {
        const now = new Date();
        const acknowledgedAt = now.toISOString();
        attempt.recoveryAcknowledgement = {
          acknowledgedAt,
          nativeProcessId,
          type: 'uncertain-invocation',
        };
      }
      current.status = 'failed';
      saveWork(database, current);
    }
  })();
};
const releaseLock = function releaseLock(directory: string, expected: string) {
  const lockPath = path.join(directory, lockFilename);
  let contents: string;
  try {
    contents = readFileSync(lockPath, 'utf-8');
  } catch (error) {
    if (errorCode(error) === 'ENOENT') {
      return true;
    }
    throw error;
  }
  if (contents !== expected) {
    return false;
  }
  unlinkSync(lockPath);
  return true;
};
const releaseRecoveryLock = function releaseRecoveryLock(
  directory: string,
  {
    acknowledgedWorks,
    interruptedWorks,
    raw,
  }: {
    acknowledgedWorks: number;
    interruptedWorks: number;
    raw: string | null;
  }
): RecoveryReport {
  if (raw === null) {
    return {
      acknowledgedWorks,
      ...(acknowledgedWorks > 0 && {
        guidance:
          'Recovery preserved the work. Run `hivex update --retry-failed --root <project>` to retry it explicitly; recovery made zero model calls.',
      }),
      interruptedWorks,
      lock: 'absent',
      status: 'recovered',
    };
  }
  let isReleased: boolean;
  try {
    isReleased = releaseLock(directory, raw);
  } catch {
    return throwRecovery(
      'unreadable',
      acknowledgedWorks > 0
        ? 'Work was acknowledged, but knowledge.lock could not be released.'
        : 'The owner is dead, but knowledge.lock could not be released atomically.',
      interruptedWorks
    );
  }
  if (!isReleased) {
    return throwRecovery(
      'changed',
      acknowledgedWorks > 0
        ? 'Work was acknowledged, but knowledge.lock changed; run recover again before continuing.'
        : 'knowledge.lock changed during recovery; inspect the store before continuing.',
      interruptedWorks
    );
  }
  return {
    acknowledgedWorks,
    ...(acknowledgedWorks > 0 && {
      guidance:
        'Recovery preserved the work. Run `hivex update --retry-failed --root <project>` to retry it explicitly; recovery made zero model calls.',
    }),
    interruptedWorks,
    lock: 'released',
    status: 'recovered',
  };
};
const recoverChecked = function recoverChecked(
  database: Database,
  directory: string,
  options: RecoveryOptions
): RecoveryReport {
  const lock = recoveryLock(directory);
  if (lock !== null) {
    assertOwnerEnded(lock.pid, 'held');
  }
  const running = runningWorksOrBlock(database);
  const failed = uncertainFailedWorks(database);
  if (running.length === 0 && failed.length === 0) {
    return releaseRecoveryLock(directory, {
      acknowledgedWorks: 0,
      interruptedWorks: 0,
      raw: lock?.raw ?? null,
    });
  }
  const lockState = lock === null ? 'absent' : 'held';
  for (const work of running) {
    assertRecoverable(work, work.nativeProcessId, lockState);
  }
  for (const entry of failed) {
    assertRecoverable(entry.work, entry.nativeProcessId, lockState);
  }
  const uncertain =
    running.filter((work) => work.nativeProcessId !== undefined).length + failed.length;
  if (uncertain > 0 && options.acknowledgeUncertain !== true) {
    throwRecovery(
      lockState,
      'Uncertain work is recoverable after its owner and native PIDs ended; rerun `hivex recover --acknowledge-uncertain --root <project>` to record an explicit acknowledgement.'
    );
  }
  const recoverableWorks = [
    ...running.map((work) => {
      const { nativeProcessId } = work;
      return { nativeProcessId, work };
    }),
    ...failed,
  ];
  recordRecovery(database, recoverableWorks);
  return releaseRecoveryLock(directory, {
    acknowledgedWorks: uncertain,
    interruptedWorks: running.length,
    raw: lock?.raw ?? null,
  });
};
const initializeStorage = function initializeStorage(
  database: Database,
  resources: DisposableStack,
  acquireUpdate?: () => Disposable
) {
  database.run('PRAGMA busy_timeout=1000');
  database.run('PRAGMA max_page_count=16384');
  database.run(
    'CREATE TABLE IF NOT EXISTS graph (id INTEGER PRIMARY KEY CHECK(id=1), data TEXT NOT NULL)'
  );
  database.run(
    'CREATE TABLE IF NOT EXISTS work (id TEXT PRIMARY KEY, kind TEXT NOT NULL, key TEXT NOT NULL, data TEXT NOT NULL)'
  );
  database.run(
    'CREATE TABLE IF NOT EXISTS model_cache (key TEXT PRIMARY KEY, value TEXT NOT NULL)'
  );
  database.run('CREATE INDEX IF NOT EXISTS work_key ON work(kind,key)');
  if (acquireUpdate !== undefined) {
    resources.use(acquireUpdate());
  }
};

export class KnowledgeStore implements Disposable {
  private readonly db: Database;
  private readonly directory: string;
  private readonly resources = new DisposableStack();
  constructor(root: string, options: StoreOptions = {}) {
    if (options.readonly === true && options.update === true) {
      throw new HivexError({
        code: 'INVALID_STORE',
        message: 'Knowledge storage cannot be readonly and own an update lock',
      });
    }
    const directory = path.join(root, '.hivex');
    this.directory = directory;
    const databasePath = path.join(directory, 'knowledge.sqlite');
    for (const candidate of [directory, databasePath]) {
      if (lstatSync(candidate, { throwIfNoEntry: false })?.isSymbolicLink() === true) {
        throw new HivexError({
          code: 'INVALID_STORE',
          message: 'Knowledge storage cannot be a symlink',
        });
      }
    }
    if (options.readonly !== true) {
      mkdirSync(directory, { mode: 0o700, recursive: true });
    }
    this.db = this.resources.use(
      options.readonly === true
        ? new Database(databasePath, { readonly: true })
        : new Database(databasePath)
    );
    if (options.readonly === true) {
      return;
    }
    const acquireUpdate = options.update === true ? this.updateLease.bind(this) : undefined;
    try {
      initializeStorage(this.db, this.resources, acquireUpdate);
    } catch (error) {
      this.resources.dispose();
      throw error;
    }
  }
  updateLease(): Disposable {
    const lockPath = path.join(this.directory, lockFilename);
    const token = JSON.stringify({ id: randomUUID(), pid: process.pid });
    let fd: number;
    try {
      fd = openSync(lockPath, 'wx', 0o600);
    } catch {
      throw new HivexError({
        code: 'KNOWLEDGE_LOCKED',
        message:
          'Cannot acquire the update lock; inspect any active or interrupted update before continuing',
      });
    }
    writeFileSync(fd, token);
    return {
      [Symbol.dispose]() {
        closeSync(fd);
        releaseLockFile(lockPath, token);
      },
    };
  }
  graph(): Graph {
    const row = this.db
      .query<
        {
          data: string;
        },
        []
      >('SELECT data FROM graph WHERE id=1')
      .get();
    if (row !== null) {
      return graphSchema.parse(JSON.parse(row.data));
    }
    return hasUnfinishedWork(this.db)
      ? emptyGraph()
      : sharedKnowledge(path.join(this.directory, '..'));
  }
  saveGraph(graph: Graph) {
    this.db.run('INSERT INTO graph VALUES(1,?) ON CONFLICT(id) DO UPDATE SET data=excluded.data', [
      JSON.stringify(graph),
    ]);
  }
  importGraph(graph: Graph) {
    this.db.transaction(() => {
      if (hasUnfinishedWork(this.db)) {
        throw new HivexError({
          code: 'UNFINISHED_WORK',
          message:
            'Finish or recover existing work before importing a knowledge snapshot; its attempts and budgets are preserved.',
        });
      }
      this.saveGraph(graph);
    })();
  }
  begin(options: BeginWork): Work {
    const defaultMaxCalls = options.kind === 'update' ? 2 : 3;
    return this.db
      .transaction(() => {
        if (this.db.query('SELECT id FROM graph WHERE id=1').get() === null) {
          this.saveGraph(this.graph());
        }
        const row = this.db
          .query<
            {
              data: string;
            },
            [string, string]
          >('SELECT data FROM work WHERE kind=? AND key=? ORDER BY rowid DESC LIMIT 1')
          .get(options.kind, options.key);
        const previous = row === null ? null : workSchema.parse(JSON.parse(row.data));
        const isReusable =
          options.kind === 'update'
            ? options.remaining.length === 0
            : previous?.resultKey === options.resultKey;
        if (
          previous === null ||
          (!isReusable && options.kind === 'update' && previous.status === 'done')
        ) {
          const work: Work = {
            ...options,
            attempts: [],
            cacheHits: 0,
            calls: 0,
            id: randomUUID(),
            inputBytes: 0,
            materializedChecks: true,
            maxCalls: options.maxCalls ?? defaultMaxCalls,
            maxInputBytes: options.maxInputBytes ?? 131_072,
            pending: null,
            phase: 'update',
            plannedUnits: [...options.remaining],
            status: 'pending',
            totalTokens: 0,
          };
          this.save(work);
          return work;
        }
        if (isReusable && previous.status === 'done') {
          return previous;
        }
        if (previous.status === 'done') {
          previous.status = 'pending';
          delete previous.result;
        }
        if (previous.status === 'running') {
          throw new HivexError({
            code: 'WORK_RUNNING',
            message: `Work ${previous.id} has an unfinished invocation; inspect it before retrying`,
          });
        }
        if (options.maxCalls !== undefined) {
          previous.maxCalls = options.maxCalls;
        }
        if (options.maxInputBytes !== undefined) {
          previous.maxInputBytes = options.maxInputBytes;
        }
        this.save(previous);
        return previous;
      })
      .immediate();
  }
  save(work: Work) {
    saveWork(this.db, work);
  }
  commit(work: Work, graph: Graph) {
    this.db.transaction(() => {
      this.saveGraph(graph);
      this.save(work);
    })();
  }
  cached(key: string): unknown {
    const row = this.db
      .query<
        {
          value: string;
        },
        [string]
      >('SELECT value FROM model_cache WHERE key=?')
      .get(key);
    return row ? JSON.parse(row.value) : undefined;
  }
  cache(key: string, value: unknown) {
    this.db.run(
      'INSERT INTO model_cache VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value',
      [key, JSON.stringify(value)]
    );
  }
  recordNativeProcess(work: Work, nativeProcessId: number) {
    if (!processIdSchema.safeParse(nativeProcessId).success) {
      throw new HivexError({
        code: 'INVALID_PROCESS_ID',
        message: 'Native process ID must be a positive integer',
      });
    }
    this.db.transaction(() => {
      const stored = this.db
        .query<
          {
            data: string;
          },
          [string]
        >(workByIdQuery)
        .get(work.id);
      const current =
        stored?.data === undefined ? undefined : workSchema.parse(JSON.parse(stored.data));
      if (current?.calls !== work.calls || current.status !== 'running') {
        throw new HivexError({
          code: workConflictCode,
          message: 'Work was claimed or changed before the native process was recorded',
        });
      }
      work.nativeProcessId = nativeProcessId;
      this.save(work);
    })();
  }
  reserve(work: Work, input: { inputBytes: number; inputHash: string; stage: string }) {
    const { inputBytes, inputHash, stage } = input;
    this.db.transaction(() => {
      const stored = this.db
        .query<
          {
            data: string;
          },
          [string]
        >(workByIdQuery)
        .get(work.id);
      const current =
        stored?.data === undefined ? undefined : workSchema.parse(JSON.parse(stored.data));
      if (current?.calls !== work.calls || current.status === 'running') {
        throw new HivexError({
          code: workConflictCode,
          message: 'Work was claimed or changed by another operation',
        });
      }
      work.calls += 1;
      work.inputBytes += inputBytes;
      work.status = 'running';
      work.ownerPid = process.pid;
      delete work.nativeProcessId;
      work.attempts.push({ inputBytes, inputHash, stage });
      this.save(work);
    })();
  }
  recover(options: RecoveryOptions = {}): RecoveryReport {
    try {
      return recoverChecked(this.db, this.directory, options);
    } catch (error) {
      if (error instanceof HivexError && error.code === 'RECOVERY_UNSAFE') {
        return blockedRecovery(error);
      }
      throw error;
    }
  }
  prune(options: PruneOptions): PruneReport {
    if (
      !Number.isSafeInteger(options.keepCompleted) ||
      options.keepCompleted < 0 ||
      !Number.isSafeInteger(options.keepCaches) ||
      options.keepCaches < 0
    ) {
      throw new HivexError({
        code: 'INVALID_RETENTION',
        message: 'Retention counts must be non-negative integers',
      });
    }
    const works = this.db
      .query<
        {
          rowid: number;
          data: string;
        },
        []
      >('SELECT rowid,data FROM work ORDER BY rowid DESC')
      .all()
      .map((row) => {
        const work = workSchema.parse(JSON.parse(row.data));
        return { rowid: row.rowid, work };
      });
    const completed = works.filter(({ work }) => work.status === 'done');
    const workRowsToDelete = completed.slice(options.keepCompleted).map(({ rowid }) => rowid);
    const caches = this.db
      .query<
        {
          rowid: number;
        },
        []
      >('SELECT rowid FROM model_cache ORDER BY rowid DESC')
      .all()
      .map(({ rowid }) => rowid);
    const cacheRowsToDelete = caches.slice(options.keepCaches);
    this.db.transaction(() => {
      deleteRows(this.db, 'work', workRowsToDelete);
      deleteRows(this.db, 'model_cache', cacheRowsToDelete);
    })();
    return {
      deletedCaches: cacheRowsToDelete.length,
      deletedCompletedWorks: workRowsToDelete.length,
      retainedCaches: caches.length - cacheRowsToDelete.length,
      retainedCompletedWorks: completed.length - workRowsToDelete.length,
      unfinishedWorks: works.filter(({ work }) => work.status !== 'done').length,
    };
  }
  [Symbol.dispose]() {
    this.resources.dispose();
  }
}
