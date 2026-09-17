import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import nodePath from 'node:path';
import { Database } from 'bun:sqlite';
import { expect, test } from 'bun:test';
import {
  emptyGraph,
  readGraph,
  readWork,
  seedWork,
  updateWork,
  writeCache,
  writeGraph,
} from './rust-fixtures.ts';

type JsonRecord = Record<string, unknown>;

interface StoredAttempt {
  recoveryAcknowledgement?: JsonRecord;
  report?: JsonRecord;
  result?: unknown;
}

interface StoredWork {
  attempts: StoredAttempt[];
  calls: number;
  inputBytes: number;
  nativeProcessId?: number;
  remaining: string[];
  status: string;
  totalTokens: number;
}

const isRecord = (value: unknown): value is JsonRecord =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const knowledgeMaintenance = function knowledgeMaintenance(argumentsList: string[]): unknown {
  const executable = process.env.HIVEX_TEST_BINARY;
  if (executable === undefined) {
    throw new Error('HIVEX_TEST_BINARY is required');
  }
  const result = spawnSync(executable, argumentsList, {
    encoding: 'utf-8',
    timeout: 10_000,
  });
  if (!result.stdout) {
    throw new Error(result.stderr || 'Expected maintenance CLI output');
  }
  const report: unknown = JSON.parse(result.stdout);
  expect(result.status).toBe(isRecord(report) && report.status === 'blocked' ? 1 : 0);
  return report;
};

const isStringArray = (value: unknown): value is string[] =>
  Array.isArray(value) && value.every((entry) => typeof entry === 'string');

const numberField = (value: unknown, name: string): number => {
  if (!isRecord(value) || typeof value[name] !== 'number') {
    throw new Error(`Expected numeric field ${name}`);
  }
  return value[name];
};

const isStoredAttempt = function isStoredAttempt(value: unknown): value is StoredAttempt {
  if (!isRecord(value)) {
    return false;
  }
  if (value.report !== undefined && !isRecord(value.report)) {
    return false;
  }
  return value.recoveryAcknowledgement === undefined || isRecord(value.recoveryAcknowledgement);
};

const isStoredWork = function isStoredWork(value: unknown): value is StoredWork {
  if (!isRecord(value)) {
    return false;
  }
  if (!Array.isArray(value.attempts) || !value.attempts.every(isStoredAttempt)) {
    return false;
  }
  if (typeof value.calls !== 'number' || typeof value.inputBytes !== 'number') {
    return false;
  }
  if (value.nativeProcessId !== undefined && typeof value.nativeProcessId !== 'number') {
    return false;
  }
  return (
    isStringArray(value.remaining) &&
    typeof value.status === 'string' &&
    typeof value.totalTokens === 'number'
  );
};

const temporaryProject = function temporaryProject(run: (root: string) => void) {
  const root = mkdtempSync(nodePath.join(tmpdir(), 'hivex-maintenance-'));
  try {
    run(root);
  } finally {
    rmSync(root, { force: true, recursive: true });
  }
};

const deadPid = function deadPid() {
  for (let processId = 2_000_000_000; processId > 1_000_000; processId -= 1) {
    try {
      process.kill(processId, 0);
    } catch (error) {
      if (Error.isError(error) && 'code' in error && error.code === 'ESRCH') {
        return processId;
      }
    }
  }
  throw new Error('Could not find a dead PID for the fixture');
};

const storedWork = function storedWork(root: string, id: string): StoredWork {
  const value = readWork(root, id);
  if (!isStoredWork(value)) {
    throw new Error(`Malformed fixture work ${id}`);
  }
  return value;
};

test('recover marks a dead native invocation failed and preserves its work state', () => {
  temporaryProject((root) => {
    const id = seedWork(root, {
      key: 'fixture',
      kind: 'ask',
      remaining: [],
      snapshot: 'fixture',
    });
    updateWork(root, id, (work) => {
      work.calls = 1;
      work.inputBytes = 123;
      work.status = 'running';
      work.ownerPid = deadPid();
      work.nativeProcessId = deadPid();
      work.attempts.push({
        inputBytes: 123,
        inputHash: 'input-hash',
        result: { retained: true },
        stage: 'extract',
      });
    });

    const inspection = knowledgeMaintenance(['recover', '--root', root]);
    expect(inspection).toMatchObject({
      command: 'recover',
      interruptedWorks: 0,
      lock: 'absent',
      modelCalls: 0,
      status: 'blocked',
    });

    const result = knowledgeMaintenance(['recover', '--acknowledge-uncertain', '--root', root]);
    expect(result).toMatchObject({
      acknowledgedWorks: 1,
      command: 'recover',
      interruptedWorks: 1,
      lock: 'absent',
      modelCalls: 0,
      status: 'recovered',
    });
    expect(existsSync(nodePath.join(root, '.hivex', 'knowledge.lock'))).toBe(false);
    const work = storedWork(root, id);
    expect(work).toMatchObject({
      calls: 1,
      inputBytes: 123,
      remaining: [],
      status: 'failed',
    });
    expect(work.nativeProcessId).toBeUndefined();
    const [firstAttempt] = work.attempts;
    expect(firstAttempt?.recoveryAcknowledgement).toMatchObject({
      type: 'uncertain-invocation',
    });
    expect(numberField(firstAttempt?.recoveryAcknowledgement, 'nativeProcessId')).toBeGreaterThan(
      0
    );
    const second = knowledgeMaintenance(['recover', '--root', root]);
    expect(second).toMatchObject({ acknowledgedWorks: 0, modelCalls: 0 });
    writeFileSync(
      nodePath.join(root, '.hivex', 'knowledge.lock'),
      JSON.stringify({ id: 'later-dead-owner', pid: deadPid() })
    );
    expect(knowledgeMaintenance(['recover', '--root', root])).toMatchObject({
      acknowledgedWorks: 0,
      status: 'recovered',
    });
    expect(existsSync(nodePath.join(root, '.hivex', 'knowledge.lock'))).toBe(false);
    expect(work.attempts).toHaveLength(1);
    expect(firstAttempt?.result).toEqual({ retained: true });
    expect(firstAttempt?.report).toMatchObject({
      cleanup: 'not-observed',
      outcome: 'interrupted',
      turnAccepted: 'unknown',
      usage: null,
    });
  });
});

test('acknowledges a saved uncertain failure without rewriting its report', () => {
  temporaryProject((root) => {
    const id = seedWork(root, {
      key: 'saved-failure',
      kind: 'ask',
      remaining: [],
      snapshot: 'fixture',
    });
    const originalReport: Record<string, unknown> = {
      cleanup: 'not-observed',
      code: 'MODEL_TIMEOUT',
      interruption: 'unconfirmed',
      nativeProcessId: deadPid(),
      outcome: 'failed',
      turnAccepted: 'unknown',
      usage: null,
    };
    updateWork(root, id, (work) => {
      work.calls = 1;
      work.inputBytes = 17;
      work.ownerPid = deadPid();
      work.status = 'failed';
      work.attempts.push({
        inputBytes: 17,
        inputHash: 'input-hash',
        report: originalReport,
        stage: 'ask',
      });
    });

    expect(knowledgeMaintenance(['recover', '--root', root])).toMatchObject({
      acknowledgedWorks: 0,
      lock: 'absent',
      status: 'blocked',
    });
    const result = knowledgeMaintenance(['recover', '--acknowledge-uncertain', '--root', root]);
    expect(result).toMatchObject({
      acknowledgedWorks: 1,
      interruptedWorks: 0,
      lock: 'absent',
      modelCalls: 0,
      status: 'recovered',
    });
    const work = storedWork(root, id);
    const [firstAttempt] = work.attempts;
    expect(firstAttempt?.report).toEqual(originalReport);
    expect(firstAttempt?.recoveryAcknowledgement).toMatchObject({
      nativeProcessId: originalReport.nativeProcessId,
      type: 'uncertain-invocation',
    });
  });
});

test('recover leaves a live owner and its work untouched', () => {
  temporaryProject((root) => {
    const id = seedWork(root, {
      key: 'fixture',
      kind: 'update',
      remaining: ['unit-1'],
      snapshot: 'fixture',
    });
    updateWork(root, id, (work) => {
      work.calls = 1;
      work.inputBytes = 10;
      work.status = 'running';
      work.ownerPid = process.pid;
      work.nativeProcessId = process.pid;
      work.attempts.push({
        inputBytes: 10,
        inputHash: 'input-hash',
        stage: 'extract',
      });
    });
    writeFileSync(
      nodePath.join(root, '.hivex', 'knowledge.lock'),
      JSON.stringify({ id: 'live-owner', pid: process.pid })
    );

    const result = knowledgeMaintenance(['recover', '--root', root]);
    expect(result).toMatchObject({
      interruptedWorks: 0,
      lock: 'held',
      modelCalls: 0,
      status: 'blocked',
    });
    expect(existsSync(nodePath.join(root, '.hivex', 'knowledge.lock'))).toBe(true);
    expect(storedWork(root, id)).toMatchObject({ calls: 1, status: 'running' });
  });
});

test('prune removes only old completed work and caches', () => {
  temporaryProject((root) => {
    writeGraph(root, emptyGraph());
    const unfinishedId = seedWork(root, {
      key: 'unfinished',
      kind: 'update',
      remaining: ['unit-1'],
      snapshot: 'fixture',
    });
    updateWork(root, unfinishedId, (work) => {
      work.calls = 4;
      work.inputBytes = 321;
      work.totalTokens = 19;
      work.status = 'failed';
      work.attempts.push({
        inputBytes: 7,
        inputHash: 'retained',
        result: { retained: true },
        stage: 'extract',
      });
    });
    for (const key of ['done-1', 'done-2', 'done-3']) {
      const id = seedWork(root, {
        key,
        kind: 'ask',
        remaining: [],
        snapshot: 'fixture',
      });
      updateWork(root, id, (work) => {
        work.status = 'done';
        work.result = { key };
      });
    }
    writeCache(root, 'cache-1', { value: 1 });
    writeCache(root, 'cache-2', { value: 2 });
    writeCache(root, 'cache-3', { value: 3 });

    const result = knowledgeMaintenance([
      'prune',
      '--root',
      root,
      '--keep-completed',
      '1',
      '--keep-caches',
      '1',
    ]);
    expect(result).toMatchObject({
      command: 'prune',
      deletedCaches: 2,
      deletedCompletedWorks: 2,
      modelCalls: 0,
      retainedCaches: 1,
      retainedCompletedWorks: 1,
      unfinishedWorks: 1,
    });
    expect(storedWork(root, unfinishedId)).toMatchObject({
      attempts: [expect.objectContaining({ result: { retained: true } })],
      calls: 4,
      inputBytes: 321,
      remaining: ['unit-1'],
      status: 'failed',
      totalTokens: 19,
    });
    expect(readGraph(root)).toEqual(emptyGraph());
  });
});

test('recover handles a dead owner before the native turn starts, without an acknowledgement', () => {
  temporaryProject((root) => {
    const id = seedWork(root, {
      key: 'before-turn',
      kind: 'ask',
      remaining: [],
      snapshot: 'fixture',
    });
    updateWork(root, id, (work) => {
      work.calls = 1;
      work.inputBytes = 40;
      work.ownerPid = deadPid();
      work.status = 'running';
      work.attempts.push({
        inputBytes: 40,
        inputHash: 'input',
        stage: 'ask',
      });
    });
    const result = knowledgeMaintenance(['recover', '--root', root]);
    expect(result).toMatchObject({
      acknowledgedWorks: 0,
      interruptedWorks: 1,
      modelCalls: 0,
      status: 'recovered',
    });
    expect(storedWork(root, id)).toMatchObject({
      attempts: [
        {
          report: {
            code: 'MODEL_INTERRUPTED_BEFORE_TURN',
            usage: null,
          },
        },
      ],
      calls: 1,
      inputBytes: 40,
      status: 'failed',
    });
  });
});

test('recover keeps an uncertain failure without a native PID blocked and unchanged', () => {
  temporaryProject((root) => {
    const id = seedWork(root, {
      key: 'missing-native-pid',
      kind: 'ask',
      remaining: [],
      snapshot: 'fixture',
    });
    updateWork(root, id, (work) => {
      work.calls = 1;
      work.inputBytes = 25;
      work.ownerPid = deadPid();
      work.status = 'failed';
      work.attempts.push({
        inputBytes: 25,
        inputHash: 'retained-input',
        report: {
          interruption: 'unconfirmed',
          turnAccepted: 'unknown',
          usage: null,
        },
        stage: 'ask',
      });
    });
    const before = storedWork(root, id);
    expect(
      knowledgeMaintenance(['recover', '--acknowledge-uncertain', '--root', root])
    ).toMatchObject({
      acknowledgedWorks: 0,
      interruptedWorks: 0,
      status: 'blocked',
    });
    expect(storedWork(root, id)).toEqual(before);
  });
});

test('recover refuses an unreadable lock and preserves it', () => {
  temporaryProject((root) => {
    writeGraph(root, emptyGraph());
    const lockPath = nodePath.join(root, '.hivex', 'knowledge.lock');
    writeFileSync(lockPath, '{"id":"unknown-owner"}');
    expect(knowledgeMaintenance(['recover', '--root', root])).toMatchObject({
      acknowledgedWorks: 0,
      interruptedWorks: 0,
      lock: 'unreadable',
      status: 'blocked',
    });
    expect(existsSync(lockPath)).toBe(true);
  });
});

test('prune rejects a corrupt retained extraction before deleting any rows', () => {
  temporaryProject((root) => {
    const id = seedWork(root, {
      key: 'corrupt-extraction',
      kind: 'update',
      remaining: [],
      snapshot: 'fixture',
    });
    updateWork(root, id, (work) => {
      work.status = 'done';
    });
    writeCache(root, 'retained-cache', { useful: true });
    using database = new Database(nodePath.join(root, '.hivex', 'knowledge.sqlite'));
    database.run("UPDATE work SET data=json_set(data,'$.pending',json(?)) WHERE id=?", [
      JSON.stringify({ batch: 'batch', documents: [], extraction: {} }),
      id,
    ]);
    const before = database.query('SELECT * FROM work').all();
    expect(() =>
      knowledgeMaintenance(['prune', '--keep-completed', '0', '--keep-caches', '0', '--root', root])
    ).toThrow();
    expect(database.query('SELECT * FROM work').all()).toEqual(before);
    expect(database.query('SELECT * FROM model_cache').all()).toHaveLength(1);
    expect(existsSync(nodePath.join(root, '.hivex', 'knowledge.lock'))).toBe(false);
  });
});

test('prune rejects invalid extraction explanations before deleting any rows', () => {
  temporaryProject((root) => {
    const id = seedWork(root, {
      key: 'invalid-explanation',
      kind: 'update',
      remaining: [],
      snapshot: 'fixture',
    });
    updateWork(root, id, (work) => {
      work.status = 'done';
    });
    writeCache(root, 'retained-cache', { useful: true });

    using database = new Database(nodePath.join(root, '.hivex', 'knowledge.sqlite'));
    const decision = {
      conditions: [],
      document: 'notes.md',
      exceptions: [],
      id: 'd1',
      kind: 'decision',
      lineEnd: 1,
      lineStart: 1,
      reason: 'A valid explanation.',
      status: 'current',
      text: 'A valid decision.',
    };
    const relationship = {
      evidence: [{ document: 'notes.md', lineEnd: 1, lineStart: 1 }],
      from: 'd1',
      id: 'r1',
      reason: 'A valid explanation.',
      to: 'd1',
      type: 'supports',
    };
    const extractions = [
      {
        decisions: [{ ...decision, reason: '' }],
        relationships: [],
        uncertainties: [],
      },
      {
        decisions: [{ ...decision, text: '😀'.repeat(1025) }],
        relationships: [],
        uncertainties: [],
      },
      {
        decisions: [{ ...decision, lineEnd: Number.MAX_SAFE_INTEGER + 1 }],
        relationships: [],
        uncertainties: [],
      },
      {
        decisions: [
          {
            ...decision,
            conditions: Array.from({ length: 17 }, () => 'condition'),
          },
        ],
        relationships: [],
        uncertainties: [],
      },
      {
        decisions: [],
        relationships: [{ ...relationship, reason: '' }],
        uncertainties: [],
      },
      {
        decisions: [],
        relationships: [],
        uncertainties: [''],
      },
    ];
    for (const extraction of extractions) {
      database.run("UPDATE work SET data=json_set(data,'$.pending',json(?)) WHERE id=?", [
        JSON.stringify({ batch: 'batch', documents: [], extraction }),
        id,
      ]);
      expect(
        knowledgeMaintenance.bind(null, [
          'prune',
          '--keep-completed',
          '0',
          '--keep-caches',
          '0',
          '--root',
          root,
        ])
      ).toThrow();
      expect(database.query('SELECT * FROM work').all()).toHaveLength(1);
      expect(database.query('SELECT * FROM model_cache').all()).toHaveLength(1);
      expect(existsSync(nodePath.join(root, '.hivex', 'knowledge.lock'))).toBe(false);
    }
  });
});

test('prune preserves JavaScript whitespace coercion before deleting caches', () => {
  temporaryProject((root) => {
    writeCache(root, 'retained', { answer: 'Keep this result.' });
    expect(
      knowledgeMaintenance.bind(null, ['prune', '--root', root, '--keep-caches', '\u{85}'])
    ).toThrow(/integer/u);
    using database = new Database(nodePath.join(root, '.hivex', 'knowledge.sqlite'));
    expect(database.query('SELECT * FROM model_cache').all()).toHaveLength(1);
    expect(
      knowledgeMaintenance(['prune', '--root', root, '--keep-caches', '\u{FEFF}0'])
    ).toMatchObject({ deletedCaches: 1 });
  });
});
