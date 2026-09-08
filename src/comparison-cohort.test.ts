import { Database } from 'bun:sqlite';
import { createHash } from 'node:crypto';
import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync, existsSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { nativeProject, nativeSource } from '../test/native-project.ts';

const cli = join(import.meta.dirname, 'cli.ts');
function invoke(root: string, args: string[]) {
  return spawnSync(process.execPath, [cli, ...args, '--root', root], {
    encoding: 'utf8',
    timeout: 15000,
  });
}
const linkedSource = nativeSource + '\n[Related rule](second.md)\n';
const assessment = {
  assessments: ['s1', 's2'].map((source) => ({
    id: `${source}:c1`,
    verdict: 'reviewed',
    reason: 'The complete prohibition is preserved.',
    relations: [],
    evidence: [{ source, quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }],
  })),
  relations: [],
  coverage: { complete: true, reason: 'Both claims were assessed.' },
  context: { verdict: 'sufficient', reason: 'Both complete sources are supplied.' },
};

test('resumes selected comparisons and exports complete provenance without repeating model calls', async () => {
  await nativeProject(
    (paths) => {
      expect(
        invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
      ).toBe(0);
      const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
      expect(built.status).toBe(0);
      const input = join(dirname(paths.store), 'graph.json');
      const store = join(dirname(paths.store), 'comparisons.sqlite');
      writeFileSync(input, built.stdout);
      const args = [
        'graph',
        'compare',
        '--all',
        '--input',
        input,
        '--store',
        store,
        '--codex',
        paths.binary,
      ];
      const initialized = invoke(paths.root, [...args, '--max-units', '0']);
      expect(initialized.stderr).toBe('');
      expect(initialized.status).toBe(0);
      expect(JSON.parse(initialized.stdout)).toMatchObject({
        accepted: false,
        processed: 0,
        pending: 1,
      });
      writeFileSync(paths.candidate, JSON.stringify(assessment));
      const reviewed = invoke(paths.root, args);
      expect(reviewed.stderr).toBe('');
      expect(reviewed.status).toBe(0);
      expect(JSON.parse(reviewed.stdout)).toMatchObject({
        status: 'reviewed',
        accepted: false,
        completed: 1,
        processed: 1,
      });
      const calls = readFileSync(paths.calls, 'utf8');
      const repeated = invoke(paths.root, args);
      expect(JSON.parse(repeated.stdout)).toMatchObject({ completed: 1, processed: 0, reused: 1 });
      const exported = invoke(paths.root, [
        'graph',
        'compare',
        '--export',
        '--input',
        input,
        '--store',
        store,
        '--max-bytes',
        '65536',
      ]);
      expect(exported.status).toBe(0);
      expect(JSON.parse(exported.stdout)).toMatchObject({
        accepted: false,
        comparisons: [
          {
            state: 'reviewed',
            result: {
              sources: [{ id: 'first.md' }, { id: 'second.md' }],
              comparison: { coverage: { complete: true } },
            },
          },
        ],
      });
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
      expect(readFileSync(input, 'utf8')).toBe(built.stdout);
    },
    { source: linkedSource },
  );
});

type Paths = Parameters<Parameters<typeof nativeProject>[0]>[0];
function fixture(paths: Paths) {
  expect(
    invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
  ).toBe(0);
  const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
  expect(built.status).toBe(0);
  const input = join(dirname(paths.store), 'graph.json');
  const store = join(dirname(paths.store), 'comparisons.sqlite');
  writeFileSync(input, built.stdout);
  writeFileSync(paths.candidate, JSON.stringify(assessment));
  const args = [
    'graph',
    'compare',
    '--all',
    '--input',
    input,
    '--store',
    store,
    '--codex',
    paths.binary,
  ];
  const inspect = ['graph', 'compare', '--export', '--input', input, '--store', store];
  return { input, store, args, inspect };
}

test('retains a negative comparison and requires exact explicit retirement before repeating it', async () => {
  await nativeProject(
    (paths) => {
      const f = fixture(paths);
      writeFileSync(
        paths.candidate,
        JSON.stringify({
          ...assessment,
          context: {
            verdict: 'insufficient',
            reason: 'The linked external authority was not supplied.',
          },
        }),
      );
      const failed = invoke(paths.root, f.args);
      expect(failed.status).toBe(1);
      const result: { planHash: string } = JSON.parse(failed.stdout);
      expect(JSON.parse(failed.stdout)).toMatchObject({ failed: 1, completed: 0, processed: 1 });
      const calls = readFileSync(paths.calls, 'utf8');
      expect(JSON.parse(invoke(paths.root, f.args).stdout)).toMatchObject({
        failed: 1,
        processed: 0,
      });
      const exported = invoke(paths.root, [...f.inspect, '--max-bytes', '65536']);
      expect(exported.status).toBe(1);
      expect(JSON.parse(exported.stdout)).toMatchObject({
        comparisons: [{ result: { comparison: { context: { verdict: 'insufficient' } } } }],
      });
      expect(
        invoke(paths.root, ['graph', 'compare', '--discard', '0'.repeat(64), '--store', f.store])
          .status,
      ).toBe(1);
      expect(
        invoke(paths.root, ['graph', 'compare', '--discard', result.planHash, '--store', f.store])
          .status,
      ).toBe(0);
      expect(JSON.parse(invoke(paths.root, [...f.args, '--max-units', '0']).stdout)).toMatchObject({
        pending: 1,
        completed: 0,
        processed: 0,
      });
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    },
    { source: linkedSource },
  );
});

test.each([nativeSource, linkedSource + '\n[Missing](missing.md)\n'])(
  'does not interpret absent or unresolved pair coverage as success',
  async (source) => {
    await nativeProject(
      (paths) => {
        const f = fixture(paths);
        const calls = readFileSync(paths.calls, 'utf8');
        const result = invoke(paths.root, f.args);
        expect(result.status).toBe(1);
        expect(JSON.parse(result.stderr)).toMatchObject({
          error: { code: 'COMPARISON_PLAN_UNRESOLVED' },
        });
        expect(existsSync(f.store)).toBe(false);
        expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
      },
      { source },
    );
  },
);

test('inspection rejects truncated exports and mismatched stores without writing or invoking a model', async () => {
  await nativeProject(
    (paths) => {
      const f = fixture(paths);
      expect(invoke(paths.root, f.args).status).toBe(0);
      const bytes = readFileSync(f.store);
      const calls = readFileSync(paths.calls, 'utf8');
      const tiny = invoke(paths.root, [...f.inspect, '--max-bytes', '1024']);
      expect(tiny.status).toBe(1);
      expect(JSON.parse(tiny.stderr)).toMatchObject({
        error: { code: 'COMPARISON_OUTPUT_BUDGET' },
      });
      const wrongKind = invoke(paths.root, [
        'graph',
        'review',
        '--export',
        '--input',
        f.input,
        '--store',
        f.store,
      ]);
      expect(wrongKind.status).toBe(1);
      expect(JSON.parse(wrongKind.stderr)).toMatchObject({
        error: { code: 'INVALID_REVIEW_STORE' },
      });
      const exported: { comparisons: { id: string }[] } = JSON.parse(
        invoke(paths.root, [...f.inspect, '--max-bytes', '65536']).stdout,
      );
      const shown = invoke(paths.root, [
        'graph',
        'compare',
        '--show',
        exported.comparisons[0]?.id ?? '',
        '--input',
        f.input,
        '--store',
        f.store,
        '--max-bytes',
        '65536',
      ]);
      expect(shown.status).toBe(0);
      expect(JSON.parse(shown.stdout)).toMatchObject({
        state: 'reviewed',
        result: { status: 'reviewed' },
      });
      expect(readFileSync(f.store)).toEqual(bytes);
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    },
    { source: linkedSource },
  );
});

test('revalidates complete claim coverage even when a modified result has a recomputed storage checksum', async () => {
  await nativeProject(
    (paths) => {
      const f = fixture(paths);
      expect(invoke(paths.root, f.args).status).toBe(0);
      const exported: { comparisons: { result: { comparison: { assessments: unknown[] } } }[] } =
        JSON.parse(invoke(paths.root, [...f.inspect, '--max-bytes', '65536']).stdout);
      const result = exported.comparisons[0]?.result;
      if (!result) throw new Error('Expected a retained comparison');
      result.comparison.assessments = [];
      const value = JSON.stringify(result);
      using db = new Database(f.store);
      db.run('UPDATE reviews SET value=?, value_hash=?', [
        value,
        createHash('sha256').update(value).digest('hex'),
      ]);
      const calls = readFileSync(paths.calls, 'utf8');
      const rejected = invoke(paths.root, f.args);
      expect(rejected.status).toBe(1);
      expect(JSON.parse(rejected.stderr)).toMatchObject({
        error: { code: 'INVALID_COMPARISON_OUTPUT' },
      });
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    },
    { source: linkedSource },
  );
});

test('a killed comparison remains unresolved and cannot be silently retried or retired', async () => {
  await nativeProject(
    async (paths) => {
      const f = fixture(paths);
      const initial: { planHash: string } = JSON.parse(
        invoke(paths.root, [...f.args, '--max-units', '0']).stdout,
      );
      writeFileSync(paths.hold, 'hold');
      const child = Bun.spawn([process.execPath, cli, ...f.args, '--root', paths.root], {
        stdout: 'pipe',
        stderr: 'pipe',
      });
      try {
        const deadline = Date.now() + 5000;
        while (readFileSync(paths.calls, 'utf8') === 'called\ncalled\n' && Date.now() < deadline)
          await Bun.sleep(20);
        expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\ncalled\n');
        child.kill('SIGKILL');
        await child.exited;
        rmSync(paths.hold);
        const resumed = invoke(paths.root, f.args);
        expect(resumed.status).toBe(1);
        expect(JSON.parse(resumed.stdout)).toMatchObject({
          unresolved: 1,
          processed: 0,
          completed: 0,
        });
        const discard = invoke(paths.root, [
          'graph',
          'compare',
          '--discard',
          initial.planHash,
          '--store',
          f.store,
        ]);
        expect(discard.status).toBe(1);
        expect(JSON.parse(discard.stderr)).toMatchObject({ error: { code: 'REVIEW_UNRESOLVED' } });
        expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\ncalled\n');
      } finally {
        child.kill('SIGKILL');
        await child.exited;
      }
    },
    { source: linkedSource },
  );
});

test('binds lexical discovery settings to retained comparison work and requires the same selection for inspection', async () => {
  await nativeProject(
    (paths) => {
      const f = fixture(paths);
      expect(invoke(paths.root, [...f.args, '--neighbors', '1']).status).toBe(0);
      const calls = readFileSync(paths.calls, 'utf8');
      const changed = invoke(paths.root, f.args);
      expect(changed.status).toBe(1);
      expect(JSON.parse(changed.stderr)).toMatchObject({ error: { code: 'REVIEW_PLAN_MISMATCH' } });
      const exported = invoke(paths.root, [
        ...f.inspect,
        '--neighbors',
        '1',
        '--max-bytes',
        '65536',
      ]);
      expect(exported.status).toBe(0);
      expect(JSON.parse(exported.stdout)).toMatchObject({
        selection: { policy: 'authored-links-and-lexical-v1', lexical: { neighbors: 1 } },
        completed: 1,
      });
      const invalidReview = invoke(paths.root, [
        'graph',
        'review',
        '--all',
        '--input',
        f.input,
        '--neighbors',
        '1',
      ]);
      expect(invalidReview.status).toBe(1);
      expect(JSON.parse(invalidReview.stderr)).toMatchObject({
        error: { code: 'INVALID_ARGUMENT' },
      });
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    },
    { source: linkedSource },
  );
});
