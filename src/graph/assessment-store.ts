import { Database } from 'bun:sqlite';
import { closeSync, lstatSync, mkdirSync, openSync } from 'node:fs';
import { dirname } from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { HivexError } from '../errors.ts';
import { hash } from '../sources/markdown.ts';

export type AssessmentPlan = {
  graphHash: string;
  selectionHash?: string;
  contract: { nativeVersion: string; requestedPolicyHash: string; schemaHash: string };
  sources: { id: string; promptHash: string; schemaHash?: string }[];
};

export function assessmentSchemaHash(plan: AssessmentPlan, id: string) {
  return plan.sources.find((source) => source.id === id)?.schemaHash ?? plan.contract.schemaHash;
}

const maximumBytes = 128 * 1024 * 1024;
const resultLimit = 8 * 1024 * 1024;
const historyLimit = 2 * resultLimit;
export type AssessmentResult = {
  status: 'reviewed' | 'failed';
  graphHash: string;
  contract: {
    promptHash: string;
    schemaHash: string;
    nativeVersion: string;
    requestedPolicyHash: string;
  };
};
export type AssessmentContract<T extends AssessmentResult> = {
  applicationId: number;
  parse: (value: unknown) => T;
  unitId: (result: T) => string;
  binding?: (result: T) => AssessmentResult;
  retryable?: (result: T) => boolean;
};
type Row = {
  id: string;
  ordinal: number;
  state: string;
  owner: string | null;
  value: string | null;
  value_hash: string | null;
  previous_attempts?: string;
  previous_attempts_hash?: string;
};

function fail(code: string, message: string): never {
  throw new HivexError({ code, message });
}

function hasCode(error: unknown, code: string) {
  return error instanceof Error && 'code' in error && error.code === code;
}

function file(path: string, readonly: boolean) {
  const parent = dirname(path);
  try {
    const directory = lstatSync(parent);
    if (!directory.isDirectory() || directory.isSymbolicLink())
      fail('INVALID_REVIEW_STORE', 'Use a real store directory');
  } catch (error) {
    if (!hasCode(error, 'ENOENT')) throw error;
    if (readonly) throw error;
    mkdirSync(parent, { recursive: true, mode: 0o700 });
  }
  if (!readonly) {
    try {
      closeSync(openSync(path, 'wx', 0o600));
    } catch (error) {
      if (!hasCode(error, 'EEXIST')) throw error;
    }
  }
  const stat = lstatSync(path);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size > maximumBytes)
    fail('INVALID_REVIEW_STORE', 'Use a regular assessment store of at most 128 MiB');
  const db = new Database(path, { strict: true, readonly });
  db.run('PRAGMA busy_timeout=1000');
  return db;
}

function identity(db: Database, allowEmpty: boolean, applicationId: number) {
  const id = db
    .query<{ application_id: number }, []>('PRAGMA application_id')
    .get()?.application_id;
  const version = db.query<{ user_version: number }, []>('PRAGMA user_version').get()?.user_version;
  if (id === applicationId && (version === 1 || version === 2 || version === 3 || version === 4)) {
    if (version === 3 || version === 4)
      db.query('SELECT previous_attempts, previous_attempts_hash FROM reviews LIMIT 0').all();
    if (version === 4) db.query('SELECT previous_plan_hash FROM cohort LIMIT 0').all();
    return true;
  }
  const objects = db
    .query<{ count: number }, []>('SELECT count(*) AS count FROM sqlite_schema')
    .get()?.count;
  if (allowEmpty && id === 0 && version === 0 && objects === 0) return false;
  return fail('INVALID_REVIEW_STORE', 'Unsupported assessment store format');
}

function assertPlan(db: Database, plan: AssessmentPlan, applicationId: number) {
  identity(db, false, applicationId);
  const row = db
    .query<
      { value: string; value_hash: string; retired: number },
      []
    >('SELECT value, value_hash, retired FROM cohort WHERE id=1')
    .get();
  if (!row || hash(row.value) !== row.value_hash)
    fail('INVALID_REVIEW_STORE', 'Assessment plan is missing or altered');
  if (row.retired !== 0)
    fail('REVIEW_STORE_RETIRED', 'This assessment cohort was explicitly retired');
  if (row.value !== JSON.stringify(plan))
    fail(
      'REVIEW_PLAN_MISMATCH',
      'The retained assessment belongs to another graph or processing contract',
    );
}

function initializePlan(db: Database, plan: AssessmentPlan) {
  const value = JSON.stringify(plan);
  db.run('INSERT OR REPLACE INTO cohort (id, value, value_hash, retired) VALUES (1, ?, ?, 0)', [
    value,
    hash(value),
  ]);
  for (const [ordinal, source] of plan.sources.entries())
    db.run(
      'INSERT INTO reviews (id, ordinal, state, owner, value, value_hash) VALUES (?, ?, ?, NULL, NULL, NULL)',
      [source.id, ordinal, 'pending'],
    );
}

function previousAttempts<T extends AssessmentResult>(
  row: Row,
  contract: AssessmentContract<T>,
  plan: AssessmentPlan,
) {
  if (row.previous_attempts === undefined && row.previous_attempts_hash === undefined) return [];
  const value = row.previous_attempts;
  if (
    value === undefined ||
    Buffer.byteLength(value) > historyLimit ||
    hash(value) !== row.previous_attempts_hash
  )
    fail('INVALID_REVIEW_STORE', 'Previous assessment attempts are oversized or altered');
  const parsed: unknown = JSON.parse(value);
  if (!Array.isArray(parsed) || parsed.length > 2)
    fail('INVALID_REVIEW_STORE', 'An assessment retains at most two previous attempts');
  return parsed.map((entry: unknown) => {
    const result = contract.parse(entry);
    validateAssessmentContract(result, {
      ...plan.contract,
      schemaHash: assessmentSchemaHash(plan, row.id),
    });
    if (
      Buffer.byteLength(JSON.stringify(result)) > resultLimit ||
      result.status !== 'failed' ||
      contract.unitId(result) !== row.id ||
      !contract.retryable?.(result)
    )
      fail(
        'INVALID_REVIEW_STORE',
        'Previous attempts must preserve safe failures of this assessment',
      );
    return result;
  });
}

function decode<T extends AssessmentResult>(
  row: Row,
  plan: AssessmentPlan,
  contract: AssessmentContract<T>,
) {
  const source = plan.sources[row.ordinal];
  if (!source || source.id !== row.id)
    fail('INVALID_REVIEW_STORE', 'Assessment sources differ from the retained plan');
  const attempts = previousAttempts(row, contract, plan);
  const history = attempts.length ? { previousAttempts: attempts } : {};
  if (row.state === 'pending' && attempts.length)
    fail('INVALID_REVIEW_STORE', 'A pending assessment cannot conceal earlier attempts');
  const finished = row.state === 'reviewed' || row.state === 'failed';
  if (
    !['pending', 'running', 'reviewed', 'failed'].includes(row.state) ||
    (row.state === 'running') !== (row.owner !== null) ||
    finished !== (row.value !== null) ||
    finished !== (row.value_hash !== null)
  )
    fail('INVALID_REVIEW_STORE', 'Assessment state is inconsistent');
  if (!finished || row.value === null)
    return { id: row.id, state: row.state, result: null, ...history };
  if (Buffer.byteLength(row.value) > resultLimit || hash(row.value) !== row.value_hash)
    fail('INVALID_REVIEW_STORE', 'A retained assessment is oversized or altered');
  const result = contract.parse(JSON.parse(row.value));
  if (result.status !== row.state)
    fail('INVALID_REVIEW_STORE', 'The result status differs from its retained row');
  validateAssessmentBinding(
    contract.binding?.(result) ?? result,
    {
      actualId: contract.unitId(result),
      id: row.id,
      promptHash: source.promptHash,
      schemaHash: assessmentSchemaHash(plan, row.id),
    },
    plan,
  );
  return { id: row.id, state: row.state, result, ...history };
}

function records<T extends AssessmentResult>(
  db: Database,
  plan: AssessmentPlan,
  contract: AssessmentContract<T>,
) {
  assertPlan(db, plan, contract.applicationId);
  const rows = db.query<Row, []>('SELECT * FROM reviews ORDER BY ordinal').all();
  if (rows.length !== plan.sources.length || rows.some((row, index) => row.ordinal !== index))
    fail('INVALID_REVIEW_STORE', 'Assessment coverage differs from the retained plan');
  return rows.map((row) => decode(row, plan, contract));
}

function transitionHash(db: Database) {
  const version = db.query<{ user_version: number }, []>('PRAGMA user_version').get()?.user_version;
  if (version === 1) return null;
  return db
    .query<{ transition_hash: string | null }, []>('SELECT transition_hash FROM cohort WHERE id=1')
    .get()?.transition_hash;
}

function recordTransition(db: Database, expectedHash: string, previousPlanHash: string) {
  enableRecovery(db);
  const version = db.query<{ user_version: number }, []>('PRAGMA user_version').get()?.user_version;
  if (version === 3) {
    const running = db
      .query<{ count: number }, []>("SELECT count(*) AS count FROM reviews WHERE state='running'")
      .get()?.count;
    if (running !== 0)
      fail('REVIEW_UNRESOLVED', 'Finish all active claims before upgrading the transition receipt');
    db.run('ALTER TABLE cohort ADD COLUMN previous_plan_hash TEXT');
    db.run('PRAGMA user_version=4');
  }
  db.run('UPDATE cohort SET transition_hash=?, previous_plan_hash=? WHERE id=1', [
    expectedHash,
    previousPlanHash,
  ]);
}

function verifyTransitionOrigin(db: Database, previousPlanHash: string) {
  const version = db.query<{ user_version: number }, []>('PRAGMA user_version').get()?.user_version;
  const origin =
    version === 4
      ? db
          .query<
            { previous_plan_hash: string | null },
            []
          >('SELECT previous_plan_hash FROM cohort WHERE id=1')
          .get()?.previous_plan_hash
      : null;
  if (origin !== previousPlanHash || !/^[a-f0-9]{64}$/.test(transitionHash(db) ?? ''))
    fail(
      'REVIEW_ARCHIVE_MISMATCH',
      'Changing context requires an explicit transition from the checkpointed comparison plan',
    );
}

function authenticateRecordedTransition(
  db: Database,
  archiveHash: string,
  previousPlanHash: string,
) {
  if (transitionHash(db) !== archiveHash)
    fail(
      'REVIEW_ARCHIVE_MISMATCH',
      'Reuse requires the retained old cohort or its exact recorded transition archive',
    );
  const version = db.query<{ user_version: number }, []>('PRAGMA user_version').get()?.user_version;
  if (version !== 4) recordTransition(db, archiveHash, previousPlanHash);
  verifyTransitionOrigin(db, previousPlanHash);
}

function enableRecovery(db: Database) {
  const version = db.query<{ user_version: number }, []>('PRAGMA user_version').get()?.user_version;
  if (version === 3 || version === 4) return;
  const running = db
    .query<{ count: number }, []>("SELECT count(*) AS count FROM reviews WHERE state='running'")
    .get()?.count;
  if (running !== 0)
    fail('REVIEW_UNRESOLVED', 'Finish all active claims before upgrading this store for recovery');
  if (version === 1) db.run('ALTER TABLE cohort ADD COLUMN transition_hash TEXT');
  db.run("ALTER TABLE reviews ADD COLUMN previous_attempts TEXT NOT NULL DEFAULT '[]'");
  db.run(
    `ALTER TABLE reviews ADD COLUMN previous_attempts_hash TEXT NOT NULL DEFAULT '${hash('[]')}'`,
  );
  db.run('PRAGMA user_version=3');
}

export class AssessmentStore<T extends AssessmentResult> {
  private readonly contract: AssessmentContract<T>;
  private readonly db: Database;
  private readonly plan: AssessmentPlan;

  static read<T extends AssessmentResult>(
    path: string,
    plan: AssessmentPlan,
    contract: AssessmentContract<T>,
  ) {
    using db = file(path, true);
    return db.transaction(() => records(db, plan, contract))();
  }

  static readRefreshed<T extends AssessmentResult>(
    path: string,
    plan: AssessmentPlan,
    previous: AssessmentPlan,
    contract: AssessmentContract<T>,
  ) {
    using db = file(path, true);
    return db.transaction(() => {
      const rows = records(db, plan, contract);
      verifyTransitionOrigin(db, hash(JSON.stringify(previous)));
      return rows;
    })();
  }

  static refresh<T extends AssessmentResult>(
    options: {
      path: string;
      previous: { plan: AssessmentPlan; rows: ReturnType<AssessmentStore<T>['snapshot']> };
      next: { plan: AssessmentPlan; results: Map<string, T> };
    },
    contract: AssessmentContract<T>,
  ) {
    lstatSync(options.path);
    using db = file(options.path, false);
    return db
      .transaction(() => {
        identity(db, false, contract.applicationId);
        const current = db
          .query<{ value: string }, []>('SELECT value FROM cohort WHERE id=1')
          .get();
        const value = JSON.stringify(options.next.plan);
        const transition = hash(
          JSON.stringify({ plan: options.previous.plan, rows: options.previous.rows }),
        );
        const previousPlanHash = hash(JSON.stringify(options.previous.plan));
        if (current?.value === value && value !== JSON.stringify(options.previous.plan)) {
          records(db, options.next.plan, contract);
          authenticateRecordedTransition(db, transition, previousPlanHash);
          return;
        }
        const rows = records(db, options.previous.plan, contract);
        if (rows.some((row) => row.state === 'running'))
          fail('REVIEW_UNRESOLVED', 'Resolve every claimed invocation before replacing its cohort');
        if (!isDeepStrictEqual(rows, options.previous.rows))
          fail(
            'REVIEW_ARCHIVE_MISMATCH',
            'Preserve an exact complete export of the retained cohort before replacement',
          );
        if (current?.value === value) return;
        if (Buffer.byteLength(value) > 1024 * 1024 || options.next.plan.sources.length > 2048)
          fail('REVIEW_PLAN_TOO_LARGE', 'An assessment cohort must fit 1 MiB and 2048 sources');
        db.run('PRAGMA max_page_count=32768');
        db.run('DELETE FROM reviews');
        initializePlan(db, options.next.plan);
        recordTransition(db, transition, previousPlanHash);
        for (const [id, result] of options.next.results) {
          const encoded = JSON.stringify(result);
          if (Buffer.byteLength(encoded) > resultLimit)
            fail('REVIEW_RESULT_TOO_LARGE', 'A reused assessment exceeds 8 MiB');
          db.run('UPDATE reviews SET state=?, value=?, value_hash=? WHERE id=?', [
            result.status,
            encoded,
            hash(encoded),
            id,
          ]);
          const attempts = rows.find((row) => row.id === id)?.previousAttempts;
          if (attempts?.length) {
            const history = JSON.stringify(attempts);
            db.run('UPDATE reviews SET previous_attempts=?, previous_attempts_hash=? WHERE id=?', [
              history,
              hash(history),
              id,
            ]);
          }
        }
        records(db, options.next.plan, contract);
      })
      .immediate();
  }

  static discard(path: string, expectedHash: string, applicationId: number) {
    lstatSync(path);
    using db = file(path, false);
    return db
      .transaction(() => {
        identity(db, false, applicationId);
        const row = db
          .query<
            { value: string; value_hash: string; retired: number },
            []
          >('SELECT * FROM cohort WHERE id=1')
          .get();
        if (!row || hash(row.value) !== row.value_hash || row.retired !== 0)
          fail(
            'INVALID_REVIEW_STORE',
            'The retained assessment plan is invalid or already retired',
          );
        if (expectedHash !== row.value_hash)
          fail('REVIEW_PLAN_MISMATCH', 'Retirement requires the exact current plan hash');
        const claimed = db
          .query<
            { count: number },
            []
          >("SELECT count(*) AS count FROM reviews WHERE state='running' OR owner IS NOT NULL")
          .get()?.count;
        if (claimed)
          fail(
            'REVIEW_UNRESOLVED',
            'Claimed work cannot be discarded while its outcome is unresolved',
          );
        db.run('DELETE FROM reviews');
        db.run('UPDATE cohort SET retired=1 WHERE id=1');
        return {
          command: 'graph',
          operation: 'review-cohort',
          accepted: false,
          retired: true,
          planHash: expectedHash,
        };
      })
      .immediate();
  }

  constructor(path: string, plan: AssessmentPlan, contract: AssessmentContract<T>) {
    this.contract = contract;
    const applicationId = contract.applicationId;
    this.plan = plan;
    if (Buffer.byteLength(JSON.stringify(plan)) > 1024 * 1024 || plan.sources.length > 2048)
      fail('REVIEW_PLAN_TOO_LARGE', 'An assessment cohort must fit 1 MiB and 2048 sources');
    this.db = file(path, false);
    try {
      if (!identity(this.db, true, applicationId)) {
        this.db.run('PRAGMA page_size=4096');
        this.db.run('PRAGMA journal_mode=DELETE');
        this.db.run('PRAGMA synchronous=FULL');
        this.db
          .transaction(() => {
            if (identity(this.db, true, applicationId)) return;
            this.db.run(`PRAGMA application_id=${applicationId}`);
            this.db.run('PRAGMA user_version=1');
            this.db.run(
              'CREATE TABLE cohort (id INTEGER PRIMARY KEY CHECK(id=1), value TEXT NOT NULL, value_hash TEXT NOT NULL, retired INTEGER NOT NULL)',
            );
            this.db.run(
              'CREATE TABLE reviews (id TEXT PRIMARY KEY, ordinal INTEGER UNIQUE NOT NULL, state TEXT NOT NULL, owner TEXT, value TEXT, value_hash TEXT)',
            );
            this.db.run('CREATE INDEX review_queue ON reviews (state, ordinal)');
            initializePlan(this.db, plan);
          })
          .immediate();
      }
      this.db
        .transaction(() => {
          const retired = this.db
            .query<{ retired: number }, []>('SELECT retired FROM cohort WHERE id=1')
            .get()?.retired;
          if (retired !== 1) return;
          const remaining = this.db
            .query<{ count: number }, []>('SELECT count(*) AS count FROM reviews')
            .get()?.count;
          if (remaining !== 0)
            fail('INVALID_REVIEW_STORE', 'A retired cohort cannot retain work rows');
          initializePlan(this.db, plan);
        })
        .immediate();
      const pageSize = this.db
        .query<{ page_size: number }, []>('PRAGMA page_size')
        .get()?.page_size;
      if (pageSize !== 4096) fail('INVALID_REVIEW_STORE', 'Unsupported assessment store page size');
      this.db.run('PRAGMA max_page_count=32768');
      records(this.db, plan, this.contract);
    } catch (error) {
      this.db.close();
      throw error;
    }
  }

  snapshot() {
    return this.db.transaction(() => records(this.db, this.plan, this.contract))();
  }

  private reserve() {
    const pages =
      this.db.query<{ page_count: number }, []>('PRAGMA page_count').get()?.page_count ?? 32768;
    const free =
      this.db.query<{ freelist_count: number }, []>('PRAGMA freelist_count').get()
        ?.freelist_count ?? 0;
    const running =
      this.db
        .query<{ count: number }, []>("SELECT count(*) AS count FROM reviews WHERE state='running'")
        .get()?.count ?? Infinity;
    if ((pages - free) * 4096 + (running + 1) * 2 * resultLimit > maximumBytes)
      fail(
        'REVIEW_STORE_FULL',
        'Export or retire retained evidence before another model invocation',
      );
  }

  retryFailed(id: string, owner: string, maximumAttempts: number) {
    return this.db
      .transaction(() => {
        assertPlan(this.db, this.plan, this.contract.applicationId);
        const row = this.db.query<Row, [string]>('SELECT * FROM reviews WHERE id=?').get(id);
        if (!row || row.state !== 'failed')
          fail('ASSESSMENT_RETRY_NOT_ALLOWED', 'Retry requires a retained failed assessment');
        const retained = decode(row, this.plan, this.contract);
        if (!retained.result || !this.contract.retryable?.(retained.result))
          fail(
            'ASSESSMENT_RETRY_NOT_ALLOWED',
            'Only confirmed safe invocation failures can be retried',
          );
        const previous = [...(retained.previousAttempts ?? []), retained.result];
        if (previous.length >= maximumAttempts || maximumAttempts > 3)
          fail(
            'ASSESSMENT_RETRY_EXHAUSTED',
            'The total attempt budget is exhausted; at most three attempts are allowed',
          );
        const history = JSON.stringify(previous);
        if (Buffer.byteLength(history) > historyLimit)
          fail('REVIEW_RESULT_TOO_LARGE', 'The complete previous attempts exceed 16 MiB');
        this.reserve();
        enableRecovery(this.db);
        this.db.run(
          "UPDATE reviews SET state='running', owner=?, value=NULL, value_hash=NULL, previous_attempts=?, previous_attempts_hash=? WHERE id=?",
          [owner, history, hash(history), id],
        );
        return id;
      })
      .immediate();
  }

  claim(owner: string) {
    return this.db
      .transaction(() => {
        assertPlan(this.db, this.plan, this.contract.applicationId);
        const next = this.db
          .query<Row, []>("SELECT * FROM reviews WHERE state='pending' ORDER BY ordinal LIMIT 1")
          .get();
        if (!next) return null;
        decode(next, this.plan, this.contract);
        this.reserve();
        this.db.run('UPDATE reviews SET state=?, owner=? WHERE id=? AND state=?', [
          'running',
          owner,
          next.id,
          'pending',
        ]);
        return next.id;
      })
      .immediate();
  }

  complete(id: string, owner: string, output: unknown) {
    const value = JSON.stringify(output);
    if (Buffer.byteLength(value) > resultLimit)
      fail(
        'REVIEW_RESULT_TOO_LARGE',
        'The full assessment exceeds 8 MiB; its claim remains unresolved',
      );
    const result = this.contract.parse(output);
    this.db
      .transaction(() => {
        assertPlan(this.db, this.plan, this.contract.applicationId);
        const row = this.db.query<Row, [string]>('SELECT * FROM reviews WHERE id=?').get(id);
        if (!row || row.state !== 'running' || row.owner !== owner)
          fail('REVIEW_CLAIM_LOST', 'Only the recorded owner can complete a assessment claim');
        decode(
          { ...row, state: result.status, owner: null, value, value_hash: hash(value) },
          this.plan,
          this.contract,
        );
        this.db.run('UPDATE reviews SET state=?, owner=NULL, value=?, value_hash=? WHERE id=?', [
          result.status,
          value,
          hash(value),
          id,
        ]);
      })
      .immediate();
  }

  [Symbol.dispose]() {
    this.db.close();
  }
}

export function validateAssessmentBinding(
  result: AssessmentResult,
  binding: { actualId: string; id: string; promptHash: string; schemaHash: string },
  plan: AssessmentPlan,
) {
  validateAssessmentContract(result, { ...plan.contract, schemaHash: binding.schemaHash });
  if (
    binding.actualId !== binding.id ||
    result.graphHash !== plan.graphHash ||
    result.contract.promptHash !== binding.promptHash
  )
    fail('INVALID_REVIEW_STORE', 'An assessment result differs from its planned inputs');
}

export function validateAssessmentContract(
  result: AssessmentResult,
  contract: AssessmentPlan['contract'],
) {
  if (
    result.contract.schemaHash !== contract.schemaHash ||
    result.contract.nativeVersion !== contract.nativeVersion ||
    result.contract.requestedPolicyHash !== contract.requestedPolicyHash
  )
    fail('INVALID_REVIEW_STORE', 'An assessment result differs from its processing contract');
}
