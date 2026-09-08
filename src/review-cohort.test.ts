import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { nativeProject } from '../test/native-project.ts';
import { Database } from 'bun:sqlite';
import { createHash } from 'node:crypto';
import type { GraphSnapshot } from './graph/snapshot.ts';

const cli = join(import.meta.dirname, 'cli.ts');
function invoke(root: string, args: string[]) {
  return spawnSync(process.execPath, [cli, ...args, '--root', root], {
    encoding: 'utf8',
    timeout: 15_000,
  });
}

test('resumes a graph review cohort without repeating completed model work', async () => {
  await nativeProject((paths) => {
    expect(
      invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
    ).toBe(0);
    const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
    const graph: GraphSnapshot = JSON.parse(built.stdout);
    const input = join(dirname(paths.store), 'graph.json');
    const store = join(dirname(paths.store), 'reviews.sqlite');
    writeFileSync(input, built.stdout);
    const args = [
      'graph',
      'review',
      '--all',
      '--input',
      input,
      '--store',
      store,
      '--codex',
      paths.binary,
      '--max-units',
      '1',
    ];
    const evidence = [{ quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }];
    for (const [index, source] of graph.sources.entries()) {
      const node = graph.nodes.find((node) => node.source === source.id);
      if (!node) throw new Error('Expected a claim');
      writeFileSync(
        paths.candidate,
        JSON.stringify({
          coverage: { verdict: 'complete', reason: 'All knowledge is represented.', evidence },
          claims: [
            { id: 'c1', verdict: 'faithful', reason: 'The prohibition is preserved.', evidence },
          ],
          relations: [],
          omissions: [],
          context: { verdict: 'sufficient', reason: 'No external context is required.' },
        }),
      );
      const result = invoke(paths.root, args);
      expect(result.stderr).toBe('');
      expect(result.status).toBe(0);
      expect(JSON.parse(result.stdout)).toMatchObject({
        accepted: false,
        processed: 1,
        completed: index + 1,
        reused: index,
        pending: graph.sources.length - index - 1,
      });
    }
    const before = readFileSync(paths.calls, 'utf8');
    const repeated = invoke(paths.root, args);
    expect(repeated.status).toBe(0);
    expect(JSON.parse(repeated.stdout)).toMatchObject({
      status: 'reviewed',
      completed: 2,
      processed: 0,
      reused: 2,
      pending: 0,
      accepted: false,
    });
    expect(readFileSync(paths.calls, 'utf8')).toBe(before);
    const inspected = invoke(paths.root, [
      'graph',
      'review',
      '--show',
      'first.md',
      '--input',
      input,
      '--store',
      store,
    ]);
    expect(inspected.status).toBe(0);
    expect(JSON.parse(inspected.stdout)).toMatchObject({
      accepted: false,
      result: { status: 'reviewed', graphHash: graph.hash },
    });
    expect(readFileSync(paths.calls, 'utf8')).toBe(before);
    expect(readFileSync(input, 'utf8')).toBe(built.stdout);
    const exported = invoke(paths.root, [
      'graph',
      'review',
      '--export',
      '--input',
      input,
      '--store',
      store,
      '--max-bytes',
      '65536',
    ]);
    expect(exported.status).toBe(0);
    const evidenceBundle: { planHash: string; reviews: unknown[] } = JSON.parse(exported.stdout);
    expect(evidenceBundle.reviews).toHaveLength(2);
    const originalStore = readFileSync(store);
    const wrong = invoke(paths.root, [
      'graph',
      'review',
      '--discard',
      '0'.repeat(64),
      '--store',
      store,
    ]);
    expect(wrong.status).toBe(1);
    expect(readFileSync(store)).toEqual(originalStore);
    const retired = invoke(paths.root, [
      'graph',
      'review',
      '--discard',
      evidenceBundle.planHash,
      '--store',
      store,
    ]);
    expect(retired.status).toBe(0);
    expect(JSON.parse(retired.stdout)).toMatchObject({
      retired: true,
      planHash: evidenceBundle.planHash,
    });
    const reset = invoke(paths.root, [
      'graph',
      'review',
      '--all',
      '--input',
      input,
      '--store',
      store,
      '--max-units',
      '0',
    ]);
    expect(reset.status).toBe(0);
    expect(JSON.parse(reset.stdout)).toMatchObject({
      accepted: false,
      completed: 0,
      reused: 0,
      pending: 2,
    });
    expect(readFileSync(paths.calls, 'utf8')).toBe(before);
  });
});

test('two CLI processes can safely initialize the same empty review store', async () => {
  await nativeProject(async (paths) => {
    expect(
      invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
    ).toBe(0);
    const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
    const input = join(dirname(paths.store), 'graph.json');
    const store = join(dirname(paths.store), 'reviews.sqlite');
    writeFileSync(input, built.stdout);
    using db = new Database(store);
    db.run('BEGIN IMMEDIATE');
    const start = () =>
      Bun.spawn(
        [
          process.execPath,
          cli,
          'graph',
          'review',
          '--all',
          '--input',
          input,
          '--store',
          store,
          '--root',
          paths.root,
          '--max-units',
          '0',
        ],
        { stdout: 'pipe', stderr: 'pipe' },
      );
    const first = start();
    const second = start();
    try {
      await Bun.sleep(400);
      db.run('COMMIT');
      const results = await Promise.all(
        [first, second].map(async (child) => ({
          code: await child.exited,
          stdout: await new Response(child.stdout).text(),
          stderr: await new Response(child.stderr).text(),
        })),
      );
      for (const result of results) {
        expect(result.stderr).toBe('');
        expect(result.code).toBe(0);
        expect(JSON.parse(result.stdout)).toMatchObject({
          accepted: false,
          pending: 2,
          completed: 0,
          processed: 0,
        });
      }
      expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
    } finally {
      first.kill('SIGKILL');
      second.kill('SIGKILL');
      await Promise.all([first.exited, second.exited]);
    }
  });
});

test('reserves result capacity for an in-flight review before allowing another model request', async () => {
  await nativeProject(async (paths) => {
    expect(
      invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
    ).toBe(0);
    const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
    const input = join(dirname(paths.store), 'graph.json');
    const store = join(dirname(paths.store), 'reviews.sqlite');
    writeFileSync(input, built.stdout);
    const args = [
      'graph',
      'review',
      '--all',
      '--input',
      input,
      '--store',
      store,
      '--codex',
      paths.binary,
    ];
    expect(invoke(paths.root, [...args, '--max-units', '0']).status).toBe(0);
    writeFileSync(paths.hold, 'hold');
    const start = () =>
      Bun.spawn([process.execPath, cli, ...args, '--root', paths.root, '--max-units', '1'], {
        stdout: 'pipe',
        stderr: 'pipe',
      });
    const first = start();
    let second: ReturnType<typeof start> | undefined;
    try {
      let deadline = Date.now() + 5000;
      while (
        readFileSync(paths.calls, 'utf8').split('\n').filter(Boolean).length < 3 &&
        Date.now() < deadline
      )
        await Bun.sleep(20);
      expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\ncalled\n');
      using db = new Database(store);
      db.run('CREATE TABLE occupied_capacity (value BLOB)');
      db.run('INSERT INTO occupied_capacity VALUES (zeroblob(?))', [105 * 1024 * 1024]);
      second = start();
      deadline = Date.now() + 5000;
      while (
        second.exitCode === null &&
        readFileSync(paths.calls, 'utf8') === 'called\ncalled\ncalled\n' &&
        Date.now() < deadline
      )
        await Bun.sleep(20);
      expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\ncalled\n');
      expect(await second.exited).toBe(1);
      expect(JSON.parse(await new Response(second.stderr).text())).toMatchObject({
        error: { code: 'REVIEW_STORE_FULL' },
      });
    } finally {
      first.kill('SIGKILL');
      second?.kill('SIGKILL');
      await Promise.all([first.exited, second?.exited]);
      rmSync(paths.hold);
    }
  });
});

test('refuses to reuse a review store for a different graph even when its source files are unchanged', async () => {
  await nativeProject((paths) => {
    expect(
      invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
    ).toBe(0);
    const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
    const input = join(dirname(paths.store), 'graph.json');
    const store = join(dirname(paths.store), 'reviews.sqlite');
    writeFileSync(input, built.stdout);
    const initial = invoke(paths.root, [
      'graph',
      'review',
      '--all',
      '--input',
      input,
      '--store',
      store,
      '--max-units',
      '0',
    ]);
    expect(initial.status).toBe(0);
    const before = readFileSync(store);
    writeFileSync(
      paths.candidate,
      JSON.stringify({
        claims: [
          {
            id: 'c1',
            text: 'Cached projections do not establish authority.',
            kind: 'constraint',
            conditions: [],
            exceptions: [],
            evidence: [{ quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }],
          },
        ],
        relations: [],
      }),
    );
    const otherIngestion = join(dirname(paths.store), 'other-ingestion.sqlite');
    expect(
      invoke(paths.root, ['ingest', '--store', otherIngestion, '--codex', paths.binary]).status,
    ).toBe(0);
    const other = invoke(paths.root, ['graph', 'build', '--store', otherIngestion, '--export']);
    expect(other.status).toBe(0);
    writeFileSync(input, other.stdout);
    const calls = readFileSync(paths.calls, 'utf8');
    const rejected = invoke(paths.root, [
      'graph',
      'review',
      '--all',
      '--input',
      input,
      '--store',
      store,
      '--codex',
      paths.binary,
    ]);
    expect(rejected.status).toBe(1);
    expect(JSON.parse(rejected.stderr)).toMatchObject({ error: { code: 'REVIEW_PLAN_MISMATCH' } });
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    expect(readFileSync(store)).toEqual(before);
  });
});

test('rejects an oversized complete review before creating a store or requesting a model', async () => {
  const quote = 'Keep source evidence complete. '.repeat(130).trim();
  await nativeProject(
    (paths) => {
      writeFileSync(
        paths.candidate,
        JSON.stringify({
          claims: Array.from({ length: 9 }, (_, index) => ({
            id: `c${index + 1}`,
            text: `Evidence requirement ${index + 1}.`,
            kind: 'constraint',
            conditions: [],
            exceptions: [],
            evidence: Array.from({ length: 8 }, () => ({ quote, lineStart: 5, lineEnd: 5 })),
          })),
          relations: [],
        }),
      );
      expect(
        invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
      ).toBe(0);
      const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
      expect(built.status).toBe(0);
      const input = join(dirname(paths.store), 'graph.json');
      const store = join(dirname(paths.store), 'reviews.sqlite');
      writeFileSync(input, built.stdout);
      const calls = readFileSync(paths.calls, 'utf8');
      const rejected = invoke(paths.root, [
        'graph',
        'review',
        '--all',
        '--input',
        input,
        '--store',
        store,
        '--codex',
        paths.binary,
      ]);
      expect(rejected.status).toBe(1);
      expect(JSON.parse(rejected.stderr)).toMatchObject({
        error: { code: 'REVIEW_INPUT_TOO_LARGE' },
      });
      expect(existsSync(store)).toBe(false);
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    },
    { source: '# Cache\n\nNever treat a cache as authority.\n\n' + quote + '\n' },
  );
});

test('detects missing retained assessments even if their storage checksum is recomputed', async () => {
  await nativeProject((paths) => {
    expect(
      invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
    ).toBe(0);
    const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
    const graph: GraphSnapshot = JSON.parse(built.stdout);
    const node = graph.nodes.find((node) => node.source === 'first.md');
    if (!node) throw new Error('Expected a claim');
    const input = join(dirname(paths.store), 'graph.json');
    const store = join(dirname(paths.store), 'reviews.sqlite');
    writeFileSync(input, built.stdout);
    const evidence = [{ quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }];
    writeFileSync(
      paths.candidate,
      JSON.stringify({
        coverage: { verdict: 'complete', reason: 'The source is covered.', evidence },
        claims: [
          { id: 'c1', verdict: 'faithful', reason: 'The prohibition is preserved.', evidence },
        ],
        relations: [],
        omissions: [],
        context: { verdict: 'sufficient', reason: 'A self-contained rule.' },
      }),
    );
    const reviewed = invoke(paths.root, [
      'graph',
      'review',
      '--all',
      '--input',
      input,
      '--store',
      store,
      '--codex',
      paths.binary,
      '--max-units',
      '1',
    ]);
    expect(reviewed.status).toBe(0);
    const inspected = invoke(paths.root, [
      'graph',
      'review',
      '--show=first.md',
      '--input',
      input,
      '--store',
      store,
    ]);
    expect(inspected.status).toBe(0);
    const output: { result: { review: { claims: unknown[] } } } = JSON.parse(inspected.stdout);
    output.result.review.claims = [];
    const value = JSON.stringify(output.result);
    using db = new Database(store);
    db.run('UPDATE reviews SET value=?, value_hash=? WHERE id=?', [
      value,
      createHash('sha256').update(value).digest('hex'),
      'first.md',
    ]);
    const calls = readFileSync(paths.calls, 'utf8');
    const rejected = invoke(paths.root, [
      'graph',
      'review',
      '--all',
      '--input',
      input,
      '--store',
      store,
      '--codex',
      paths.binary,
    ]);
    expect(rejected.status).toBe(1);
    expect(JSON.parse(rejected.stderr)).toMatchObject({ error: { code: 'INVALID_REVIEW_OUTPUT' } });
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
  });
});

test('never repeats or discards an invocation whose CLI was interrupted before retaining its result', async () => {
  await nativeProject(async (paths) => {
    expect(
      invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
    ).toBe(0);
    const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
    const input = join(dirname(paths.store), 'graph.json');
    const store = join(dirname(paths.store), 'reviews.sqlite');
    writeFileSync(input, built.stdout);
    const args = [
      'graph',
      'review',
      '--all',
      '--input',
      input,
      '--store',
      store,
      '--codex',
      paths.binary,
    ];
    const initialized = invoke(paths.root, [...args, '--max-units', '0']);
    expect(initialized.status).toBe(0);
    const initial: { planHash: string } = JSON.parse(initialized.stdout);
    writeFileSync(paths.hold, 'hold');
    const child = Bun.spawn([process.execPath, cli, ...args, '--root', paths.root], {
      stdout: 'pipe',
      stderr: 'pipe',
    });
    try {
      const deadline = Date.now() + 5000;
      while (
        readFileSync(paths.calls, 'utf8').split('\n').filter(Boolean).length < 3 &&
        Date.now() < deadline
      )
        await Bun.sleep(20);
      expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\ncalled\n');
      child.kill('SIGKILL');
      await child.exited;
      rmSync(paths.hold);
      const calls = readFileSync(paths.calls, 'utf8');
      const continued = invoke(paths.root, [...args, '--max-units', '0']);
      expect(continued.status).toBe(1);
      expect(JSON.parse(continued.stdout)).toMatchObject({
        accepted: false,
        completed: 0,
        unresolved: 1,
        pending: 1,
        processed: 0,
      });
      const before = readFileSync(store);
      const retired = invoke(paths.root, [
        'graph',
        'review',
        '--discard',
        initial.planHash,
        '--store',
        store,
      ]);
      expect(retired.status).toBe(1);
      expect(JSON.parse(retired.stderr)).toMatchObject({ error: { code: 'REVIEW_UNRESOLVED' } });
      expect(readFileSync(store)).toEqual(before);
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    } finally {
      child.kill('SIGKILL');
      await child.exited;
    }
  });
});
