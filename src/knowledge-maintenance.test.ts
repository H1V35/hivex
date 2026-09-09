import { Database } from 'bun:sqlite';
import { expect, test } from 'bun:test';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { emptyGraph } from './knowledge-model.ts';
import { knowledgeMaintenance } from './knowledge-maintenance.ts';
import { KnowledgeStore, type Work } from './knowledge-store.ts';

function temporaryProject(run: (root: string) => void) {
  const root = mkdtempSync(join(tmpdir(), 'hivex-maintenance-'));
  try {
    run(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

function deadPid() {
  for (let pid = 2_000_000_000; pid > 1_000_000; pid -= 1) {
    try {
      process.kill(pid, 0);
    } catch (error) {
      if (error instanceof Error && 'code' in error && error.code === 'ESRCH') return pid;
    }
  }
  throw new Error('Could not find a dead PID for the fixture');
}

function storedWork(root: string, id: string): Work {
  const database = new Database(join(root, '.hivex', 'knowledge.sqlite'), { readonly: true });
  try {
    const row = database
      .query<{ data: string }, [string]>('SELECT data FROM work WHERE id=?')
      .get(id);
    if (!row) throw new Error(`Missing fixture work ${id}`);
    return JSON.parse(row.data) as Work;
  } finally {
    database.close();
  }
}

test('recover marks a dead native invocation failed and preserves its work state', () => {
  temporaryProject((root) => {
    let id: string;
    {
      using store = new KnowledgeStore(root);
      const work = store.begin({
        kind: 'ask',
        key: 'fixture',
        snapshot: 'fixture',
        remaining: [],
      });
      store.reserve(work, 'extract', 'input-hash', 123);
      const attempt = work.attempts.at(-1);
      if (!attempt) throw new Error('Fixture did not reserve an attempt');
      attempt.result = { retained: true };
      work.ownerPid = deadPid();
      store.recordNativeProcess(work, deadPid());
      id = work.id;
    }

    const inspection = knowledgeMaintenance(['recover', '--root', root]);
    expect(inspection).toMatchObject({
      command: 'recover',
      modelCalls: 0,
      status: 'blocked',
      lock: 'absent',
      interruptedWorks: 0,
    });

    const result = knowledgeMaintenance(['recover', '--acknowledge-uncertain', '--root', root]);
    expect(result).toMatchObject({
      command: 'recover',
      modelCalls: 0,
      status: 'recovered',
      lock: 'absent',
      interruptedWorks: 1,
      acknowledgedWorks: 1,
    });
    expect(existsSync(join(root, '.hivex', 'knowledge.lock'))).toBe(false);
    const work = storedWork(root, id);
    expect(work).toMatchObject({
      status: 'failed',
      calls: 1,
      inputBytes: 123,
      remaining: [],
    });
    expect(work.nativeProcessId).toBeUndefined();
    expect(work.attempts[0]?.recoveryAcknowledgement).toMatchObject({
      type: 'uncertain-invocation',
      nativeProcessId: expect.any(Number),
    });
    expect(work.attempts).toHaveLength(1);
    expect(work.attempts[0]?.result).toEqual({ retained: true });
    expect(work.attempts[0]?.report).toMatchObject({
      outcome: 'interrupted',
      usage: null,
      turnAccepted: 'unknown',
      cleanup: 'not-observed',
    });
  });
});

test('acknowledges a saved uncertain failure without rewriting its report', () => {
  temporaryProject((root) => {
    let id: string;
    let originalReport: Record<string, unknown>;
    {
      using store = new KnowledgeStore(root);
      const work = store.begin({
        kind: 'ask',
        key: 'saved-failure',
        snapshot: 'fixture',
        remaining: [],
      });
      store.reserve(work, 'ask', 'input-hash', 17);
      const attempt = work.attempts.at(-1);
      if (!attempt) throw new Error('Fixture did not reserve an attempt');
      work.ownerPid = deadPid();
      originalReport = {
        outcome: 'failed',
        code: 'MODEL_TIMEOUT',
        interruption: 'unconfirmed',
        nativeProcessId: deadPid(),
        cleanup: 'not-observed',
        turnAccepted: 'unknown',
        usage: null,
      };
      attempt.report = originalReport;
      work.status = 'failed';
      store.save(work);
      id = work.id;
    }

    expect(knowledgeMaintenance(['recover', '--root', root])).toMatchObject({
      status: 'blocked',
      lock: 'absent',
      acknowledgedWorks: 0,
    });
    const result = knowledgeMaintenance(['recover', '--acknowledge-uncertain', '--root', root]);
    expect(result).toMatchObject({
      status: 'recovered',
      lock: 'absent',
      interruptedWorks: 0,
      acknowledgedWorks: 1,
      modelCalls: 0,
    });
    const work = storedWork(root, id);
    expect(work.attempts[0]?.report).toEqual(originalReport);
    expect(work.attempts[0]?.recoveryAcknowledgement).toMatchObject({
      type: 'uncertain-invocation',
      nativeProcessId: originalReport.nativeProcessId,
    });
  });
});

test('recover leaves a live owner and its work untouched', () => {
  temporaryProject((root) => {
    let id: string;
    {
      using store = new KnowledgeStore(root);
      const work = store.begin({
        kind: 'update',
        key: 'fixture',
        snapshot: 'fixture',
        remaining: ['unit-1'],
      });
      store.reserve(work, 'extract', 'input-hash', 10);
      store.recordNativeProcess(work, process.pid);
      id = work.id;
    }
    writeFileSync(
      join(root, '.hivex', 'knowledge.lock'),
      JSON.stringify({ pid: process.pid, id: 'live-owner' }),
    );

    const result = knowledgeMaintenance(['recover', '--root', root]);
    expect(result).toMatchObject({
      status: 'blocked',
      lock: 'held',
      interruptedWorks: 0,
      modelCalls: 0,
    });
    expect(existsSync(join(root, '.hivex', 'knowledge.lock'))).toBe(true);
    expect(storedWork(root, id)).toMatchObject({ status: 'running', calls: 1 });
  });
});

test('prune removes only old completed work and caches', () => {
  temporaryProject((root) => {
    let unfinishedId: string;
    {
      using store = new KnowledgeStore(root);
      store.saveGraph(emptyGraph());
      const unfinished = store.begin({
        kind: 'update',
        key: 'unfinished',
        snapshot: 'fixture',
        remaining: ['unit-1'],
      });
      unfinished.calls = 4;
      unfinished.inputBytes = 321;
      unfinished.totalTokens = 19;
      unfinished.status = 'failed';
      unfinished.attempts.push({
        stage: 'extract',
        inputHash: 'retained',
        inputBytes: 7,
        result: { retained: true },
      });
      store.save(unfinished);
      unfinishedId = unfinished.id;

      for (const key of ['done-1', 'done-2', 'done-3']) {
        const completed = store.begin({
          kind: 'ask',
          key,
          snapshot: 'fixture',
          remaining: [],
        });
        completed.status = 'done';
        completed.result = { key };
        store.save(completed);
      }
      store.cache('cache-1', { value: 1 });
      store.cache('cache-2', { value: 2 });
      store.cache('cache-3', { value: 3 });
    }

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
      modelCalls: 0,
      deletedCompletedWorks: 2,
      deletedCaches: 2,
      retainedCompletedWorks: 1,
      retainedCaches: 1,
      unfinishedWorks: 1,
    });
    expect(storedWork(root, unfinishedId)).toMatchObject({
      status: 'failed',
      calls: 4,
      inputBytes: 321,
      totalTokens: 19,
      remaining: ['unit-1'],
      attempts: [expect.objectContaining({ result: { retained: true } })],
    });
    using store = new KnowledgeStore(root);
    expect(store.graph()).toEqual(emptyGraph());
  });
});

test('recover handles a dead owner before the native turn starts, without an acknowledgement', () => {
  temporaryProject((root) => {
    let id: string;
    {
      using store = new KnowledgeStore(root);
      const work = store.begin({
        kind: 'ask',
        key: 'before-turn',
        snapshot: 'fixture',
        remaining: [],
      });
      store.reserve(work, 'ask', 'input', 40);
      work.ownerPid = deadPid();
      store.save(work);
      id = work.id;
    }
    const result = knowledgeMaintenance(['recover', '--root', root]);
    expect(result).toMatchObject({
      status: 'recovered',
      modelCalls: 0,
      interruptedWorks: 1,
      acknowledgedWorks: 0,
    });
    expect(storedWork(root, id)).toMatchObject({
      status: 'failed',
      calls: 1,
      inputBytes: 40,
      attempts: [
        expect.objectContaining({
          report: expect.objectContaining({ code: 'MODEL_INTERRUPTED_BEFORE_TURN', usage: null }),
        }),
      ],
    });
  });
});
