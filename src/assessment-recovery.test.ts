import { expect, test } from 'bun:test';
import { Database } from 'bun:sqlite';
import { hash } from './sources/markdown.ts';
import { AssessmentStore, type AssessmentPlan } from './graph/assessment-store.ts';
import { reviewContract } from './graph/review-cohort.ts';
import { comparisonContract } from './graph/comparison-cohort.ts';
import { readFileSync, writeFileSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import {
  invoke,
  projectWithReviews,
  comparisonResponse,
  type Paths,
  type Fixture,
} from '../test/reviewed-project.ts';

type Operation = 'review' | 'compare';
function cohort(paths: Paths, fixture: Fixture, operation: Operation, neighbors = 1) {
  let id: string;
  let response: object;
  if (operation === 'compare') {
    const plan = invoke(paths.root, [
      'graph',
      'compare-plan',
      '--input',
      fixture.input,
      '--neighbors',
      String(neighbors),
    ]);
    const selected: { pairs: { id: string }[] } = JSON.parse(plan.stdout);
    id = selected.pairs[0]?.id ?? '';
    response = comparisonResponse();
  } else {
    id = fixture.graph.sources[0]?.id ?? '';
    const node = fixture.graph.nodes.find((entry) => entry.source === id);
    if (!node) throw new Error('Expected source claim');
    const evidence = [{ quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }];
    response = {
      coverage: { verdict: 'complete', reason: 'The complete prohibition is preserved.', evidence },
      claims: [
        { id: 'c1', verdict: 'faithful', reason: 'The prohibition is preserved.', evidence },
      ],
      relations: [],
      omissions: [],
      context: { verdict: 'sufficient', reason: 'The full source is supplied.' },
    };
  }
  writeFileSync(paths.candidate, JSON.stringify(response));
  const args = [
    'graph',
    operation,
    '--input',
    fixture.input,
    '--store',
    operation === 'compare' ? fixture.comparisons : fixture.reviews,
    ...(operation === 'compare' ? ['--neighbors', String(neighbors)] : []),
  ];
  return {
    id,
    response,
    args,
    export: () => invoke(paths.root, [...args, '--export', '--max-bytes', '1048576']),
    run: (extra: string[]) =>
      invoke(paths.root, [...args, '--all', '--codex', paths.binary, '--max-units', '1', ...extra]),
    show: () => invoke(paths.root, [...args, '--show', id, '--max-bytes', '1048576']),
  };
}

test.each<Operation>(['review', 'compare'])(
  'explicitly recovers a safely interrupted %s without losing its original receipt or usage',
  async (operation) => {
    await projectWithReviews((paths, fixture) => {
      const task = cohort(paths, fixture, operation);
      writeFileSync(paths.scenario, 'timeout');
      const timedOut = task.run(['--deadline-ms', '100']);
      expect(timedOut.status).toBe(1);
      const previous = JSON.parse(task.show().stdout).result;
      expect(previous.report).toMatchObject({
        outcome: 'timeout',
        turnAccepted: 'confirmed',
        interruption: 'confirmed',
        cleanup: 'confirmed',
        usage: { totalTokens: 125 },
      });
      rmSync(paths.scenario);
      const calls = readFileSync(paths.calls, 'utf8');
      const recovered = task.run(['--retry-failed', task.id, '--attempts', '2']);
      expect(recovered.stderr).toBe('');
      expect(recovered.status).toBe(0);
      expect(JSON.parse(recovered.stdout)).toMatchObject({
        processed: 1,
        completed: 1,
        failed: 0,
        recordedAttempts: 2,
        reportedTokens: 275,
      });
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls + 'called\n');
      expect(JSON.parse(task.show().stdout)).toMatchObject({
        state: 'reviewed',
        previousAttempts: [previous],
        result: { report: { outcome: 'completed' } },
      });
      const finalCalls = readFileSync(paths.calls, 'utf8');
      expect(task.run(['--retry-failed', task.id, '--attempts', '3']).status).toBe(1);
      expect(readFileSync(paths.calls, 'utf8')).toBe(finalCalls);
    }, 0);
  },
);

test.each<Operation>(['review', 'compare'])(
  'keeps unmeasured %s consumption and enforces the total retry budget across restarts',
  async (operation) => {
    await projectWithReviews((paths, fixture) => {
      const task = cohort(paths, fixture, operation);
      writeFileSync(paths.scenario, 'timeout-unmeasured');
      expect(task.run(['--deadline-ms', '100']).status).toBe(1);
      const first = JSON.parse(task.show().stdout).result;
      expect(first.report).toMatchObject({
        outcome: 'timeout',
        usage: null,
        interruption: 'confirmed',
        cleanup: 'confirmed',
      });
      expect(
        task.run(['--retry-failed', task.id, '--attempts', '2', '--deadline-ms', '100']).status,
      ).toBe(1);
      const second = JSON.parse(task.show().stdout).result;
      rmSync(paths.scenario);
      const calls = readFileSync(paths.calls, 'utf8');
      const exhausted = task.run(['--retry-failed', task.id, '--attempts', '2']);
      expect(exhausted.status).toBe(1);
      expect(exhausted.stderr).toContain('ASSESSMENT_RETRY_EXHAUSTED');
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
      const recovered = task.run(['--retry-failed', task.id, '--attempts', '3']);
      expect(recovered.stderr).toBe('');
      expect(JSON.parse(recovered.stdout)).toMatchObject({
        processed: 1,
        recordedAttempts: 3,
        reportedTokens: 150,
        unmeasuredResults: 2,
      });
      expect(JSON.parse(task.show().stdout).previousAttempts).toEqual([first, second]);
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls + 'called\n');
    }, 0);
  },
);

test.each<Operation>(['review', 'compare'])(
  'does not retry an adverse or uncertain %s assessment',
  async (operation) => {
    await projectWithReviews((paths, fixture) => {
      const task = cohort(paths, fixture, operation);
      writeFileSync(paths.scenario, 'start-unconfirmed');
      expect(task.run(['--deadline-ms', '100']).status).toBe(1);
      rmSync(paths.scenario);
      const calls = readFileSync(paths.calls, 'utf8');
      const before = task.show().stdout;
      expect(task.run(['--retry-failed', task.id, '--attempts', '2']).status).toBe(1);
      expect(task.show().stdout).toBe(before);
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    }, 0);
    await projectWithReviews((paths, fixture) => {
      const task = cohort(paths, fixture, operation);
      const response = task.response;
      writeFileSync(
        paths.candidate,
        JSON.stringify({
          ...response,
          context: { verdict: 'insufficient', reason: 'An external authority is absent.' },
        }),
      );
      expect(task.run([]).status).toBe(1);
      const calls = readFileSync(paths.calls, 'utf8');
      const before = task.show().stdout;
      expect(task.run(['--retry-failed', task.id, '--attempts', '2']).status).toBe(1);
      expect(task.show().stdout).toBe(before);
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    }, 0);
  },
);

test.each<Operation>(['review', 'compare'])(
  'recovers malformed %s output and preserves the validation failure',
  async (operation) => {
    await projectWithReviews((paths, fixture) => {
      const task = cohort(paths, fixture, operation);
      writeFileSync(paths.candidate, '{}');
      expect(task.run([]).status).toBe(1);
      const failed = JSON.parse(task.show().stdout).result;
      expect(failed.report.outcome).toBe('invalid-output');
      writeFileSync(paths.candidate, JSON.stringify(task.response));
      expect(task.run(['--retry-failed', task.id, '--attempts', '2']).status).toBe(0);
      expect(JSON.parse(task.show().stdout).previousAttempts).toEqual([failed]);
    }, 0);
  },
);

function rebuildSnapshot(
  paths: Paths,
  fixture: Fixture,
  name: string,
  candidate?: object,
): Fixture {
  if (candidate) writeFileSync(paths.candidate, JSON.stringify(candidate));
  else rmSync(paths.candidate, { force: true });
  writeFileSync(join(paths.root, 'implementation.ts'), `export const revision = '${name}';\n`);
  paths.git(['add', 'implementation.ts']);
  paths.git([
    '-c',
    'user.name=Test',
    '-c',
    'user.email=test@example.invalid',
    'commit',
    '-qm',
    name,
  ]);
  const store = join(dirname(paths.store), `${name}-ingestion.sqlite`);
  expect(invoke(paths.root, ['ingest', '--store', store, '--codex', paths.binary]).status).toBe(0);
  const built = invoke(paths.root, ['graph', 'build', '--store', store, '--export']);
  expect(built.status).toBe(0);
  const input = join(dirname(paths.store), `${name}-graph.json`);
  writeFileSync(input, built.stdout);
  return { ...fixture, input, graph: JSON.parse(built.stdout), built: built.stdout };
}

test.each<Operation>(['review', 'compare'])(
  'preserves original %s history across reuse, retry and another snapshot transition',
  async (operation) => {
    await projectWithReviews((paths, fixture) => {
      const task = cohort(paths, fixture, operation);
      expect(task.run(['--codex', join(paths.root, 'missing-codex')]).status).toBe(1);
      const first = JSON.parse(task.show().stdout).result;
      writeFileSync(paths.candidate, '{}');
      expect(task.run(['--retry-failed', task.id, '--attempts', '2']).status).toBe(1);
      const second = JSON.parse(task.show().stdout).result;
      const archive = join(dirname(paths.store), 'history.json');
      writeFileSync(archive, task.export().stdout);

      const next = rebuildSnapshot(paths, fixture, 'next');
      expect(next.graph.sourceSnapshot.commit).not.toBe(fixture.graph.sourceSnapshot.commit);
      const current = cohort(paths, next, operation, 2);
      const transition = ['--from', fixture.input, '--reuse', archive, '--max-units', '0'];
      const calls = readFileSync(paths.calls, 'utf8');
      const transferred = current.run(transition);
      expect(transferred.stderr).toBe('');
      expect(JSON.parse(transferred.stdout)).toMatchObject({
        failed: 1,
        processed: 0,
        recordedAttempts: 2,
        reportedTokens: 150,
        unmeasuredResults: 1,
      });
      const associated = JSON.parse(current.show().stdout).result;
      const { association, ...original } = associated;
      expect(original).toEqual(second);
      expect(association.graphHash).toBe(next.graph.hash);
      expect(current.run(['--retry-failed', task.id, '--attempts', '2']).stderr).toContain(
        'ASSESSMENT_RETRY_EXHAUSTED',
      );
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);

      const recovered = current.run(['--retry-failed', task.id, '--attempts', '3']);
      expect(recovered.stderr).toBe('');
      expect(recovered.status).toBe(0);
      expect(JSON.parse(recovered.stdout)).toMatchObject({
        completed: 1,
        recordedAttempts: 3,
        reportedTokens: 300,
        unmeasuredResults: 1,
      });
      const retained = JSON.parse(current.show().stdout);
      expect(retained.previousAttempts).toEqual([first, associated]);
      const nextArchive = join(dirname(paths.store), 'next-history.json');
      writeFileSync(nextArchive, current.export().stdout);

      const last = rebuildSnapshot(paths, next, 'last');
      const final = cohort(paths, last, operation);
      const before = readFileSync(paths.calls, 'utf8');
      const reused = final.run(['--from', next.input, '--reuse', nextArchive, '--max-units', '0']);
      expect(reused.stderr).toBe('');
      expect(reused.status).toBe(0);
      expect(JSON.parse(reused.stdout)).toMatchObject({
        completed: 1,
        processed: 0,
        recordedAttempts: 3,
        reportedTokens: 300,
        unmeasuredResults: 1,
      });
      expect(JSON.parse(final.show().stdout).previousAttempts).toEqual([first, associated]);
      const exported = JSON.parse(final.export().stdout);
      const rows = operation === 'review' ? exported.reviews : exported.comparisons;
      expect(rows[0].previousAttempts).toEqual([first, associated]);
      expect(rows[0].result.graphHash).toBe(next.graph.hash);
      expect(rows[0].result.association.graphHash).toBe(last.graph.hash);
      expect(final.run(['--max-units', '0']).status).toBe(0);
      expect(readFileSync(paths.calls, 'utf8')).toBe(before);
      expect(first.sourceSnapshot.commit).not.toBe(last.graph.sourceSnapshot.commit);
      expect(associated.association.sourceSnapshot.commit).not.toBe(
        last.graph.sourceSnapshot.commit,
      );

      const archiveText = final.export().stdout;
      const finalArchive = join(dirname(paths.store), 'final-history.json');
      writeFileSync(finalArchive, archiveText);
      using db = new Database(operation === 'review' ? fixture.reviews : fixture.comparisons);
      const altered = structuredClone(associated);
      altered.association.originalHash = '0'.repeat(64);
      const history = JSON.stringify([first, altered]);
      db.run('UPDATE reviews SET previous_attempts=?, previous_attempts_hash=? WHERE id=?', [
        history,
        hash(history),
        task.id,
      ]);
      expect(final.show().stderr).toContain('original');
      expect(final.export().stderr).toContain('original');
      const originalHistory = JSON.stringify([first, associated]);
      db.run('UPDATE reviews SET previous_attempts=?, previous_attempts_hash=? WHERE id=?', [
        originalHistory,
        hash(originalHistory),
        task.id,
      ]);

      const extraction = invoke(paths.root, [
        'ingest',
        '--show',
        'first.md',
        '--store',
        paths.store,
      ]);
      const candidate = JSON.parse(extraction.stdout).result.candidate;
      candidate.claims[0].text = 'Cached content does not establish authority.';
      const changed = rebuildSnapshot(paths, last, 'changed-candidate', candidate);
      const changedTask = cohort(paths, changed, operation);
      const changedCalls = readFileSync(paths.calls, 'utf8');
      const pending = changedTask.run([
        '--from',
        last.input,
        '--reuse',
        finalArchive,
        '--max-units',
        '0',
      ]);
      expect(pending.stderr).toBe('');
      expect(JSON.parse(pending.stdout)).toMatchObject({ completed: 0, recordedAttempts: 0 });
      expect(JSON.parse(changedTask.show().stdout)).toMatchObject({
        state: 'pending',
        result: null,
      });
      expect(JSON.parse(changedTask.show().stdout).previousAttempts).toBeUndefined();
      expect(readFileSync(paths.calls, 'utf8')).toBe(changedCalls);
      expect(readFileSync(finalArchive, 'utf8')).toBe(archiveText);
    }, 0);
  },
);

test('a killed retry retains its previous receipt and cannot be retried or retired', async () => {
  await projectWithReviews(async (paths, fixture) => {
    const task = cohort(paths, fixture, 'compare');
    writeFileSync(paths.scenario, 'timeout');
    const first = task.run(['--deadline-ms', '100']);
    expect(first.status).toBe(1);
    const planHash: string = JSON.parse(first.stdout).planHash;
    const receipt = JSON.parse(task.show().stdout).result;
    rmSync(paths.scenario);
    const calls = readFileSync(paths.calls, 'utf8');
    writeFileSync(paths.hold, 'hold');
    const child = Bun.spawn(
      [
        process.execPath,
        join(import.meta.dirname, 'cli.ts'),
        'graph',
        'compare',
        '--all',
        '--root',
        paths.root,
        '--input',
        fixture.input,
        '--store',
        fixture.comparisons,
        '--neighbors',
        '1',
        '--codex',
        paths.binary,
        '--retry-failed',
        task.id,
        '--attempts',
        '2',
        '--max-units',
        '1',
      ],
      { stdout: 'pipe', stderr: 'pipe' },
    );
    try {
      const deadline = Date.now() + 5000;
      while (readFileSync(paths.calls, 'utf8') === calls && Date.now() < deadline)
        await Bun.sleep(20);
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls + 'called\n');
      child.kill('SIGKILL');
      await child.exited;
      rmSync(paths.hold);
      expect(JSON.parse(task.show().stdout)).toMatchObject({
        state: 'running',
        result: null,
        previousAttempts: [receipt],
      });
      expect(task.run(['--retry-failed', task.id, '--attempts', '3']).status).toBe(1);
      const retired = invoke(paths.root, [
        'graph',
        'compare',
        '--discard',
        planHash,
        '--store',
        fixture.comparisons,
      ]);
      expect(retired.status).toBe(1);
      expect(retired.stderr).toContain('REVIEW_UNRESOLVED');
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls + 'called\n');
    } finally {
      child.kill('SIGKILL');
      await child.exited;
    }
  }, 0);
});

test('recovers a preflight failure without counting it as another model call', async () => {
  await projectWithReviews((paths, fixture) => {
    const task = cohort(paths, fixture, 'compare');
    const calls = readFileSync(paths.calls, 'utf8');
    expect(task.run(['--codex', join(paths.root, 'missing-codex')]).status).toBe(1);
    const receipt = JSON.parse(task.show().stdout).result;
    expect(receipt.report).toMatchObject({
      code: 'MODEL_ADMISSION_FAILED',
      cleanup: 'not-observed',
    });
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    const recovered = task.run(['--retry-failed', task.id, '--attempts', '2']);
    expect(recovered.stderr).toBe('');
    expect(JSON.parse(recovered.stdout)).toMatchObject({
      completed: 1,
      recordedAttempts: 2,
      reportedTokens: 150,
    });
    expect(JSON.parse(task.show().stdout).previousAttempts).toEqual([receipt]);
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls + 'called\n');
  }, 0);
});

test.each<Operation>(['review', 'compare'])(
  'rejects altered historical %s diagnostics after a successful retry',
  async (operation) => {
    await projectWithReviews((paths, fixture) => {
      const task = cohort(paths, fixture, operation);
      writeFileSync(paths.candidate, '{}');
      expect(task.run([]).status).toBe(1);
      const failed = JSON.parse(task.show().stdout).result;
      expect(failed.rejectedOutput).toEqual({ text: '{}', hash: hash('{}') });
      writeFileSync(paths.candidate, JSON.stringify(task.response));
      expect(task.run(['--retry-failed', task.id, '--attempts', '2']).status).toBe(0);
      expect(JSON.parse(task.show().stdout).previousAttempts).toEqual([failed]);
      const calls = readFileSync(paths.calls, 'utf8');
      const db = new Database(operation === 'compare' ? fixture.comparisons : fixture.reviews);
      try {
        for (const field of ['text', 'outcome']) {
          const altered = structuredClone(failed);
          if (field === 'text') altered.rejectedOutput.text += 'altered';
          else altered.report.outcome = 'timeout';
          const history = JSON.stringify([altered]);
          db.run('UPDATE reviews SET previous_attempts=?, previous_attempts_hash=? WHERE id=?', [
            history,
            hash(history),
            task.id,
          ]);
          const inspected = task.show();
          expect(inspected.status).toBe(1);
          expect(inspected.stderr).toContain('INVALID_REVIEW_STORE');
          if (field === 'text')
            expect(inspected.stderr).toContain('Rejected model output is altered');
        }
        expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
      } finally {
        db.close();
      }
    }, 0);
  },
);

test.each<[Operation, boolean]>([
  ['review', false],
  ['review', true],
  ['compare', false],
  ['compare', true],
])(
  'rejects incompatible %s history with consistent outer checksums (running retry: %s)',
  async (operation, running) => {
    await projectWithReviews((paths, fixture) => {
      const task = cohort(paths, fixture, operation);
      writeFileSync(paths.candidate, '{}');
      expect(task.run([]).status).toBe(1);
      expect(task.run(['--retry-failed', task.id, '--attempts', '2']).status).toBe(1);
      const path = operation === 'compare' ? fixture.comparisons : fixture.reviews;
      const originalExport = JSON.parse(task.export().stdout);
      const plan: AssessmentPlan = originalExport.plan;
      const archive = join(dirname(paths.store), 'altered-history.json');
      const calls = readFileSync(paths.calls, 'utf8');
      const edits: Record<string, (receipt: any) => void> = {
        schema: (receipt) => {
          receipt.contract.schemaHash =
            operation === 'review' ? plan.contract.schemaHash : '0'.repeat(64);
        },
        native: (receipt) => {
          receipt.contract.nativeVersion = 'incompatible';
        },
        policy: (receipt) => {
          receipt.contract.requestedPolicyHash = '0'.repeat(64);
        },
        prompt: (receipt) => {
          receipt.contract.promptHash = '0'.repeat(64);
        },
        bindings: (receipt) => {
          if (operation === 'review') receipt.reviewBindings.claims.c1 = '0'.repeat(64);
          else receipt.sourceBindings.s1 = 'second.md';
        },
        output: (receipt) => {
          receipt.modelOutputHash = '0'.repeat(64);
        },
      };
      using db = new Database(path);
      if (running) {
        using store =
          operation === 'compare'
            ? new AssessmentStore(path, plan, comparisonContract)
            : new AssessmentStore(path, plan, reviewContract);
        expect(store.retryFailed(task.id, 'fixture-owner', 3)).toBe(task.id);
      }
      const exported = JSON.parse(task.export().stdout);
      const rows = operation === 'review' ? exported.reviews : exported.comparisons;
      const original = JSON.stringify(rows[0].previousAttempts);
      expect(rows[0].state).toBe(running ? 'running' : 'failed');
      if (running) expect(rows[0].result).toBeNull();
      for (const [field, edit] of Object.entries(edits)) {
        const changed = structuredClone(exported);
        const changedRows = operation === 'review' ? changed.reviews : changed.comparisons;
        edit(changedRows[0].previousAttempts[0]);
        const history = JSON.stringify(changedRows[0].previousAttempts);
        db.run('UPDATE reviews SET previous_attempts=?, previous_attempts_hash=? WHERE id=?', [
          history,
          hash(history),
          task.id,
        ]);
        writeFileSync(archive, JSON.stringify(changed));
        const before = readFileSync(path);
        const checks = [
          task.show(),
          task.export(),
          task.run(['--max-units', '0']),
          task.run(['--from', fixture.input, '--reuse', archive, '--max-units', '0']),
        ];
        for (const result of checks) {
          expect(result.status, `${operation}/${running}/${field}`).toBe(1);
          expect(result.stderr).toMatch(/INVALID_(REVIEW_STORE|REVIEW_OUTPUT|COMPARISON_STORE)/);
        }
        expect(readFileSync(path)).toEqual(before);
      }
      db.run('UPDATE reviews SET previous_attempts=?, previous_attempts_hash=? WHERE id=?', [
        original,
        hash(original),
        task.id,
      ]);
      expect(task.show().stderr).toBe('');
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    }, 0);
  },
  15000,
);
