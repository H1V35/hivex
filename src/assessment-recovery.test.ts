import { expect, test } from 'bun:test';
import { Database } from 'bun:sqlite';
import { hash } from './sources/markdown.ts';
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
function cohort(paths: Paths, fixture: Fixture, operation: Operation) {
  let id: string;
  let response: object;
  if (operation === 'compare') {
    const plan = invoke(paths.root, [
      'graph',
      'compare-plan',
      '--input',
      fixture.input,
      '--neighbors',
      '1',
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
        { id: node.id, verdict: 'faithful', reason: 'The prohibition is preserved.', evidence },
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
    ...(operation === 'compare' ? ['--neighbors', '1'] : []),
  ];
  return {
    id,
    response,
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

test('preserves failed comparison history and its remaining budget across a graph transition', async () => {
  await projectWithReviews((paths, fixture) => {
    const task = cohort(paths, fixture, 'compare');
    writeFileSync(paths.scenario, 'timeout-unmeasured');
    expect(task.run(['--deadline-ms', '100']).status).toBe(1);
    const first = JSON.parse(task.show().stdout).result;
    expect(
      task.run(['--retry-failed', task.id, '--attempts', '2', '--deadline-ms', '100']).status,
    ).toBe(1);
    const second = JSON.parse(task.show().stdout).result;
    const archived = invoke(paths.root, [
      'graph',
      'compare',
      '--export',
      '--input',
      fixture.input,
      '--store',
      fixture.comparisons,
      '--neighbors',
      '1',
      '--max-bytes',
      '1048576',
    ]);
    expect(archived.stderr).toBe('');
    const archive = join(dirname(paths.store), 'comparison-history.json');
    writeFileSync(archive, archived.stdout);
    rmSync(paths.scenario);
    rmSync(paths.candidate);
    writeFileSync(join(paths.root, 'implementation.ts'), 'export const value = 1;\n');
    paths.git(['add', 'implementation.ts']);
    paths.git([
      '-c',
      'user.name=Test',
      '-c',
      'user.email=test@example.invalid',
      'commit',
      '-qm',
      'Add unrelated implementation',
    ]);
    const store = join(dirname(paths.store), 'new-ingestion.sqlite');
    expect(invoke(paths.root, ['ingest', '--store', store, '--codex', paths.binary]).status).toBe(
      0,
    );
    const built = invoke(paths.root, ['graph', 'build', '--store', store, '--export']);
    expect(built.status).toBe(0);
    const input = join(dirname(paths.store), 'new-graph.json');
    writeFileSync(input, built.stdout);
    const args = [
      'graph',
      'compare',
      '--all',
      '--input',
      input,
      '--store',
      fixture.comparisons,
      '--neighbors',
      '1',
    ];
    const transition = [...args, '--from', fixture.input, '--reuse', archive, '--max-units', '0'];
    const calls = readFileSync(paths.calls, 'utf8');
    const transferred = invoke(paths.root, transition);
    expect(transferred.stderr).toBe('');
    expect(JSON.parse(transferred.stdout)).toMatchObject({
      failed: 1,
      processed: 0,
      recordedAttempts: 2,
      unmeasuredResults: 2,
    });
    const exhausted = invoke(paths.root, [
      ...args,
      '--retry-failed',
      task.id,
      '--attempts',
      '2',
      '--max-units',
      '1',
      '--codex',
      paths.binary,
    ]);
    expect(exhausted.status).toBe(1);
    expect(exhausted.stderr).toContain('ASSESSMENT_RETRY_EXHAUSTED');
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    writeFileSync(paths.candidate, JSON.stringify(comparisonResponse()));
    const recovered = invoke(paths.root, [
      ...args,
      '--retry-failed',
      task.id,
      '--attempts',
      '3',
      '--max-units',
      '1',
      '--codex',
      paths.binary,
    ]);
    expect(recovered.stderr).toBe('');
    expect(JSON.parse(recovered.stdout)).toMatchObject({
      completed: 1,
      recordedAttempts: 3,
      reportedTokens: 150,
      unmeasuredResults: 2,
    });
    const shown = invoke(paths.root, [
      'graph',
      'compare',
      '--show',
      task.id,
      '--input',
      input,
      '--store',
      fixture.comparisons,
      '--neighbors',
      '1',
      '--max-bytes',
      '1048576',
    ]);
    const retained = JSON.parse(shown.stdout);
    expect(retained.previousAttempts[0]).toEqual(first);
    expect(retained.previousAttempts[1].report).toEqual(second.report);
    expect(invoke(paths.root, transition).status).toBe(0);
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls + 'called\n');
  }, 0);
});

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

test('rejects altered historical fidelity diagnostics after a successful retry', async () => {
  await projectWithReviews((paths, fixture) => {
    const task = cohort(paths, fixture, 'review');
    writeFileSync(paths.candidate, '{}');
    expect(task.run([]).status).toBe(1);
    const failed = JSON.parse(task.show().stdout).result;
    expect(failed.rejectedOutput).toEqual({ text: '{}', hash: hash('{}') });
    writeFileSync(paths.candidate, JSON.stringify(task.response));
    expect(task.run(['--retry-failed', task.id, '--attempts', '2']).status).toBe(0);
    expect(JSON.parse(task.show().stdout).previousAttempts).toEqual([failed]);
    const calls = readFileSync(paths.calls, 'utf8');
    const db = new Database(fixture.reviews);
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
          expect(inspected.stderr).toContain('Rejected fidelity output is altered');
      }
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    } finally {
      db.close();
    }
  }, 0);
});
