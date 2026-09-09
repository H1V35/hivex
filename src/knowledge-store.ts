import { Database } from 'bun:sqlite';
import {
  closeSync,
  lstatSync,
  mkdirSync,
  openSync,
  readFileSync,
  unlinkSync,
  writeFileSync,
} from 'node:fs';
import { join } from 'node:path';
import { randomUUID } from 'node:crypto';
import { z } from 'zod';
import { HivexError } from './errors.ts';
import { emptyGraph, extractionSchema, graphSchema, type Graph } from './knowledge-model.ts';

const processIdSchema = z.number().int().positive();
const lockSchema = z.object({ pid: processIdSchema, id: z.string().min(1) });
const recoveryAcknowledgementSchema = z.object({
  type: z.literal('uncertain-invocation'),
  acknowledgedAt: z.string(),
  nativeProcessId: processIdSchema,
});

const attemptSchema = z.object({
  stage: z.string(),
  inputHash: z.string(),
  inputBytes: z.number(),
  report: z.unknown().optional(),
  outputHash: z.string().optional(),
  diagnostic: z.string().optional(),
  error: z.string().optional(),
  result: z.unknown().optional(),
  recoveryAcknowledgement: recoveryAcknowledgementSchema.optional(),
});
const workSchema = z.object({
  id: z.string(),
  kind: z.enum(['update', 'ask']),
  key: z.string(),
  snapshot: z.string(),
  calls: z.number().int().nonnegative(),
  maxCalls: z.number().int().nonnegative(),
  inputBytes: z.number().int().nonnegative(),
  maxInputBytes: z.number().int().positive(),
  totalTokens: z.number().int().nonnegative(),
  status: z.enum(['pending', 'running', 'budget-exhausted', 'failed', 'done']),
  remaining: z.array(z.string()),
  cacheHits: z.number().int().nonnegative().default(0),
  ownerPid: processIdSchema.optional(),
  nativeProcessId: processIdSchema.optional(),
  pending: z
    .object({
      batch: z.string(),
      documents: z.array(z.string()),
      units: z.array(z.string()).default([]),
      packet: z.record(z.string(), z.unknown()).optional(),
      context: z.array(z.string()).default([]),
      existing: z.array(z.string()).default([]),
      extraction: extractionSchema,
    })
    .nullable(),
  attempts: z.array(attemptSchema).max(4096),
  result: z.unknown().optional(),
});
export type Work = z.infer<typeof workSchema>;

export type RecoveryReport = {
  status: 'clean' | 'recovered' | 'blocked';
  lock: 'absent' | 'released' | 'held' | 'unreadable' | 'changed';
  interruptedWorks: number;
  acknowledgedWorks: number;
  guidance?: string;
};

export type RecoveryOptions = {
  acknowledgeUncertain?: boolean;
};

export type PruneOptions = {
  keepCompleted: number;
  keepCaches: number;
};

export type PruneReport = {
  deletedCompletedWorks: number;
  deletedCaches: number;
  retainedCompletedWorks: number;
  retainedCaches: number;
  unfinishedWorks: number;
};

type ProcessState = 'alive' | 'dead' | 'unknown';
type RecoveryLock = { raw: string; pid: number };

function errorCode(error: unknown) {
  return error instanceof Error && 'code' in error && typeof error.code === 'string'
    ? error.code
    : undefined;
}

function processState(pid: number): ProcessState {
  try {
    process.kill(pid, 0);
    return 'alive';
  } catch (error) {
    const code = errorCode(error);
    if (code === 'ESRCH') return 'dead';
    if (code === 'EPERM') return 'alive';
    return 'unknown';
  }
}

function recordValue(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function interruptedReport(previous: unknown, nativeProcessId: number) {
  const report = recordValue(previous) ?? {};
  return {
    ...report,
    outcome: 'interrupted',
    code: 'MODEL_INTERRUPTED_RECOVERED',
    interruption: 'unconfirmed',
    turnAccepted: typeof report.turnAccepted === 'string' ? report.turnAccepted : 'unknown',
    cleanup: 'not-observed',
    usage: report.usage ?? null,
    recovery: {
      nativeProcessId,
      nativeProcessEnded: true,
      previousOutcome: typeof report.outcome === 'string' ? report.outcome : null,
    },
  };
}

function deleteRows(db: Database, table: 'work' | 'model_cache', rowids: number[]) {
  if (!rowids.length) return;
  const placeholders = rowids.map(() => '?').join(',');
  db.run(`DELETE FROM ${table} WHERE rowid IN (${placeholders})`, rowids);
}

function throwRecovery(
  lock: RecoveryReport['lock'],
  guidance: string,
  interruptedWorks = 0,
): never {
  throw new HivexError({
    code: 'RECOVERY_UNSAFE',
    message: guidance,
    details: { lock, interruptedWorks },
  });
}

function blockedRecovery(error: HivexError): RecoveryReport {
  const lock = error.details?.lock;
  return {
    status: 'blocked',
    lock:
      lock === 'absent' || lock === 'held' || lock === 'unreadable' || lock === 'changed'
        ? lock
        : 'unreadable',
    interruptedWorks:
      typeof error.details?.interruptedWorks === 'number' ? error.details.interruptedWorks : 0,
    acknowledgedWorks: 0,
    guidance: error.message,
  };
}

export class KnowledgeStore implements Disposable {
  private readonly db: Database;
  private readonly directory: string;

  constructor(root: string, options: { readonly?: boolean } = {}) {
    const directory = join(root, '.hivex');
    this.directory = directory;
    const path = join(directory, 'knowledge.sqlite');
    for (const candidate of [directory, path]) {
      if (lstatSync(candidate, { throwIfNoEntry: false })?.isSymbolicLink())
        throw new HivexError({
          code: 'INVALID_STORE',
          message: 'Knowledge storage cannot be a symlink',
        });
    }
    if (!options.readonly) mkdirSync(directory, { recursive: true, mode: 0o700 });
    this.db = options.readonly ? new Database(path, { readonly: true }) : new Database(path);
    if (options.readonly) return;
    this.db.run('PRAGMA busy_timeout=1000');
    this.db.run('PRAGMA max_page_count=16384');
    this.db.run(
      'CREATE TABLE IF NOT EXISTS graph (id INTEGER PRIMARY KEY CHECK(id=1), data TEXT NOT NULL)',
    );
    this.db.run(
      'CREATE TABLE IF NOT EXISTS work (id TEXT PRIMARY KEY, kind TEXT NOT NULL, key TEXT NOT NULL, data TEXT NOT NULL)',
    );
    this.db.run(
      'CREATE TABLE IF NOT EXISTS model_cache (key TEXT PRIMARY KEY, value TEXT NOT NULL)',
    );
    this.db.run('CREATE INDEX IF NOT EXISTS work_key ON work(kind,key)');
  }

  updateLease(): Disposable {
    const path = join(this.directory, 'knowledge.lock');
    const token = JSON.stringify({ pid: process.pid, id: randomUUID() });
    let fd: number;
    try {
      fd = openSync(path, 'wx', 0o600);
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
        try {
          if (readFileSync(path, 'utf8') === token) unlinkSync(path);
        } catch (error) {
          if (!(error instanceof Error && 'code' in error && error.code === 'ENOENT')) throw error;
        }
      },
    };
  }

  graph(): Graph {
    const row = this.db.query<{ data: string }, []>('SELECT data FROM graph WHERE id=1').get();
    return row ? graphSchema.parse(JSON.parse(row.data)) : emptyGraph();
  }

  saveGraph(graph: Graph) {
    this.db.run('INSERT INTO graph VALUES(1,?) ON CONFLICT(id) DO UPDATE SET data=excluded.data', [
      JSON.stringify(graph),
    ]);
  }

  begin(options: {
    kind: Work['kind'];
    key: string;
    snapshot: string;
    maxCalls?: number;
    maxInputBytes?: number;
    remaining: string[];
  }): Work {
    return this.db
      .transaction(() => {
        const row = this.db
          .query<
            { data: string },
            [string, string]
          >('SELECT data FROM work WHERE kind=? AND key=? ORDER BY rowid DESC LIMIT 1')
          .get(options.kind, options.key);
        const previous = row ? workSchema.parse(JSON.parse(row.data)) : null;
        if (previous && (previous.status !== 'done' || options.remaining.length === 0)) {
          const work = previous;
          if (work.status === 'done') return work;
          if (work.status === 'running')
            throw new HivexError({
              code: 'WORK_RUNNING',
              message: `Work ${work.id} has an unfinished invocation; inspect it before retrying`,
            });
          if (options.maxCalls !== undefined) work.maxCalls = options.maxCalls;
          if (options.maxInputBytes !== undefined) work.maxInputBytes = options.maxInputBytes;
          this.save(work);
          return work;
        }
        const work: Work = {
          id: randomUUID(),
          ...options,
          maxCalls: options.maxCalls ?? 2,
          maxInputBytes: options.maxInputBytes ?? 131072,
          calls: 0,
          cacheHits: 0,
          inputBytes: 0,
          totalTokens: 0,
          status: 'pending',
          pending: null,
          attempts: [],
        };
        this.save(work);
        return work;
      })
      .immediate();
  }

  save(work: Work) {
    if (work.status !== 'running') delete work.nativeProcessId;
    this.db.run(
      'INSERT INTO work VALUES(?,?,?,?) ON CONFLICT(id) DO UPDATE SET data=excluded.data',
      [work.id, work.kind, work.key, JSON.stringify(work)],
    );
  }

  commit(work: Work, graph: Graph) {
    this.db.transaction(() => {
      this.saveGraph(graph);
      this.save(work);
    })();
  }

  cached(key: string): unknown {
    const row = this.db
      .query<{ value: string }, [string]>('SELECT value FROM model_cache WHERE key=?')
      .get(key);
    return row ? JSON.parse(row.value) : undefined;
  }

  cache(key: string, value: unknown) {
    this.db.run(
      'INSERT INTO model_cache VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value',
      [key, JSON.stringify(value)],
    );
  }

  recordNativeProcess(work: Work, nativeProcessId: number) {
    if (!processIdSchema.safeParse(nativeProcessId).success)
      throw new HivexError({
        code: 'INVALID_PROCESS_ID',
        message: 'Native process ID must be a positive integer',
      });
    this.db.transaction(() => {
      const stored = this.db
        .query<{ data: string }, [string]>('SELECT data FROM work WHERE id=?')
        .get(work.id);
      const current = stored && workSchema.parse(JSON.parse(stored.data));
      if (!current || current.calls !== work.calls || current.status !== 'running')
        throw new HivexError({
          code: 'WORK_CONFLICT',
          message: 'Work was claimed or changed before the native process was recorded',
        });
      work.nativeProcessId = nativeProcessId;
      this.save(work);
    })();
  }

  reserve(work: Work, stage: string, inputHash: string, inputBytes: number) {
    this.db.transaction(() => {
      const stored = this.db
        .query<{ data: string }, [string]>('SELECT data FROM work WHERE id=?')
        .get(work.id);
      const current = stored && workSchema.parse(JSON.parse(stored.data));
      if (!current || current.calls !== work.calls || current.status === 'running')
        throw new HivexError({
          code: 'WORK_CONFLICT',
          message: 'Work was claimed or changed by another operation',
        });
      work.calls += 1;
      work.inputBytes += inputBytes;
      work.status = 'running';
      work.ownerPid = process.pid;
      delete work.nativeProcessId;
      work.attempts.push({ stage, inputHash, inputBytes });
      this.save(work);
    })();
  }

  recover(options: RecoveryOptions = {}): RecoveryReport {
    try {
      return this.recoverChecked(options);
    } catch (error) {
      if (error instanceof HivexError && error.code === 'RECOVERY_UNSAFE')
        return blockedRecovery(error);
      throw error;
    }
  }

  prune(options: PruneOptions): PruneReport {
    if (
      !Number.isInteger(options.keepCompleted) ||
      options.keepCompleted < 0 ||
      !Number.isInteger(options.keepCaches) ||
      options.keepCaches < 0
    )
      throw new HivexError({
        code: 'INVALID_RETENTION',
        message: 'Retention counts must be non-negative integers',
      });
    const works = this.db
      .query<{ rowid: number; data: string }, []>('SELECT rowid,data FROM work ORDER BY rowid DESC')
      .all()
      .map((row) => ({ rowid: row.rowid, work: workSchema.parse(JSON.parse(row.data)) }));
    const completed = works.filter(({ work }) => work.status === 'done');
    const workRowsToDelete = completed.slice(options.keepCompleted).map(({ rowid }) => rowid);
    const caches = this.db
      .query<{ rowid: number }, []>('SELECT rowid FROM model_cache ORDER BY rowid DESC')
      .all()
      .map(({ rowid }) => rowid);
    const cacheRowsToDelete = caches.slice(options.keepCaches);
    this.db.transaction(() => {
      deleteRows(this.db, 'work', workRowsToDelete);
      deleteRows(this.db, 'model_cache', cacheRowsToDelete);
    })();
    return {
      deletedCompletedWorks: workRowsToDelete.length,
      deletedCaches: cacheRowsToDelete.length,
      retainedCompletedWorks: completed.length - workRowsToDelete.length,
      retainedCaches: caches.length - cacheRowsToDelete.length,
      unfinishedWorks: works.filter(({ work }) => work.status !== 'done').length,
    };
  }

  private allWorks() {
    return this.db
      .query<{ data: string }, []>('SELECT data FROM work')
      .all()
      .map(({ data }) => workSchema.parse(JSON.parse(data)));
  }

  private runningWorks() {
    return this.allWorks().filter((work) => work.status === 'running');
  }

  private uncertainFailedWorks() {
    return this.allWorks().flatMap((work) => {
      if (work.status !== 'failed' || work.attempts.at(-1)?.recoveryAcknowledgement) return [];
      const report = recordValue(work.attempts.at(-1)?.report);
      if (report?.interruption !== 'unconfirmed' && report?.turnAccepted !== 'unknown') return [];
      const nativeProcessId = processIdSchema.safeParse(report.nativeProcessId);
      return [
        { work, nativeProcessId: nativeProcessId.success ? nativeProcessId.data : undefined },
      ];
    });
  }

  private recoverChecked(options: RecoveryOptions): RecoveryReport {
    const lock = this.recoveryLock();
    if (lock !== null) this.assertOwnerEnded(lock.pid, 'held');
    const running = this.runningWorksOrBlock();
    const failed = this.uncertainFailedWorks();
    if (!running.length && !failed.length) return this.releaseRecoveryLock(lock?.raw ?? null, 0, 0);
    const lockState = lock === null ? 'absent' : 'held';
    for (const work of running) this.assertRecoverable(work, work.nativeProcessId, lockState);
    for (const entry of failed)
      this.assertRecoverable(entry.work, entry.nativeProcessId, lockState);
    const uncertain =
      running.filter((work) => work.nativeProcessId !== undefined).length + failed.length;
    if (uncertain && !options.acknowledgeUncertain)
      throwRecovery(
        lockState,
        'Uncertain work is recoverable after its owner and native PIDs ended; rerun `hivex recover --acknowledge-uncertain --root <project>` to record an explicit acknowledgement.',
      );
    this.recordRecovery([
      ...running.map((work) => ({ work, nativeProcessId: work.nativeProcessId })),
      ...failed,
    ]);
    return this.releaseRecoveryLock(lock?.raw ?? null, running.length, uncertain);
  }

  private recoveryLock(): RecoveryLock | null {
    const path = join(this.directory, 'knowledge.lock');
    let raw: string;
    try {
      raw = readFileSync(path, 'utf8');
    } catch (error) {
      if (errorCode(error) === 'ENOENT') return null;
      throwRecovery(
        'unreadable',
        'knowledge.lock cannot be read safely; inspect the store before continuing.',
      );
    }
    try {
      const lock = lockSchema.parse(JSON.parse(raw));
      return { raw, pid: lock.pid };
    } catch {
      throwRecovery(
        'unreadable',
        'knowledge.lock has no verifiable PID; do not delete it and inspect the process manually.',
      );
    }
  }

  private runningWorksOrBlock() {
    try {
      return this.runningWorks();
    } catch {
      throwRecovery(
        'unreadable',
        'Work state cannot be validated; preserve the store and inspect it manually.',
      );
    }
  }

  private assertOwnerEnded(
    ownerPid: number,
    lock: RecoveryReport['lock'],
    label = 'The lock owner',
  ) {
    const state = processState(ownerPid);
    if (state !== 'dead')
      throwRecovery(
        lock,
        state === 'alive'
          ? `${label} (PID ${ownerPid}) is still alive; no process was modified or terminated.`
          : `${label} (PID ${ownerPid}) cannot be proven dead; no state was modified.`,
      );
  }

  private assertRecoverable(
    work: Work,
    nativeProcessId: number | undefined,
    lock: RecoveryReport['lock'],
  ) {
    if (work.ownerPid === undefined)
      throwRecovery(
        lock,
        `Work ${work.id} has no recorded owner PID; its recovery state is unchanged.`,
      );
    this.assertOwnerEnded(work.ownerPid, lock, `Work ${work.id} owner`);
    const attempt = work.attempts.at(-1);
    if (!attempt)
      throwRecovery(lock, `Work ${work.id} has no reserved attempt; no state was changed.`);
    if (nativeProcessId === undefined && work.status === 'running') return;
    if (nativeProcessId === undefined)
      throwRecovery(
        lock,
        `Work ${work.id} has no native PID for its uncertain result; no state was changed.`,
      );
    const state = processState(nativeProcessId);
    if (state !== 'dead')
      throwRecovery(
        lock,
        state === 'alive'
          ? `Native process PID ${nativeProcessId} for work ${work.id} is still alive; no process was killed.`
          : `Native process PID ${nativeProcessId} for work ${work.id} cannot be checked; no state was changed.`,
      );
    const report = work.attempts.at(-1)?.report;
    if (report !== undefined && recordValue(report) === null)
      throwRecovery(
        lock,
        `Work ${work.id} has an unstructured running report; recovery left it unchanged.`,
      );
  }

  private recordRecovery(works: Array<{ work: Work; nativeProcessId: number | undefined }>) {
    this.db.transaction(() => {
      for (const { work, nativeProcessId } of works) {
        const row = this.db
          .query<{ data: string }, [string]>('SELECT data FROM work WHERE id=?')
          .get(work.id);
        const current = row && workSchema.parse(JSON.parse(row.data));
        const attempt = current?.attempts.at(-1);
        if (!current || current.status !== work.status || current.calls !== work.calls || !attempt)
          throwRecovery('changed', `Work ${work.id} changed during recovery; run recover again.`);
        attempt.report ??=
          nativeProcessId === undefined
            ? {
                outcome: 'interrupted',
                code: 'MODEL_INTERRUPTED_BEFORE_TURN',
                cleanup: 'not-observed',
                usage: null,
              }
            : interruptedReport(undefined, nativeProcessId);
        if (nativeProcessId !== undefined)
          attempt.recoveryAcknowledgement = {
            type: 'uncertain-invocation',
            acknowledgedAt: new Date().toISOString(),
            nativeProcessId,
          };
        current.status = 'failed';
        this.save(current);
      }
    })();
  }

  private releaseRecoveryLock(
    raw: string | null,
    interruptedWorks: number,
    acknowledgedWorks: number,
  ): RecoveryReport {
    if (raw === null)
      return {
        status: 'recovered',
        lock: 'absent',
        interruptedWorks,
        acknowledgedWorks,
        ...(acknowledgedWorks
          ? {
              guidance:
                'Recovery preserved the work. Run `hivex update --retry-failed --root <project>` to retry it explicitly; recovery made zero model calls.',
            }
          : {}),
      };
    let released: boolean;
    try {
      released = this.releaseLock(raw);
    } catch {
      throwRecovery(
        'unreadable',
        acknowledgedWorks
          ? 'Work was acknowledged, but knowledge.lock could not be released.'
          : 'The owner is dead, but knowledge.lock could not be released atomically.',
        interruptedWorks,
      );
    }
    if (!released)
      throwRecovery(
        'changed',
        acknowledgedWorks
          ? 'Work was acknowledged, but knowledge.lock changed; run recover again before continuing.'
          : 'knowledge.lock changed during recovery; inspect the store before continuing.',
        interruptedWorks,
      );
    return {
      status: 'recovered',
      lock: 'released',
      interruptedWorks,
      acknowledgedWorks,
      ...(acknowledgedWorks
        ? {
            guidance:
              'Recovery preserved the work. Run `hivex update --retry-failed --root <project>` to retry it explicitly; recovery made zero model calls.',
          }
        : {}),
    };
  }

  private releaseLock(expected: string) {
    const path = join(this.directory, 'knowledge.lock');
    try {
      if (readFileSync(path, 'utf8') !== expected) return false;
      unlinkSync(path);
      return true;
    } catch (error) {
      if (errorCode(error) === 'ENOENT') return true;
      throw error;
    }
  }

  [Symbol.dispose]() {
    this.db.close();
  }
}
