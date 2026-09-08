import { expect, test } from 'bun:test';
import { Database } from 'bun:sqlite';
import { readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { nativeProject } from '../test/native-project.ts';
import { invoke } from '../test/reviewed-project.ts';
import { hash } from './sources/markdown.ts';

const cli = join(import.meta.dirname, 'cli.ts');
type Paths = Parameters<Parameters<typeof nativeProject>[0]>[0];

function ingest(paths: Paths, extra: string[] = []) {
  const inspection = ['--export', '--reuse', '--show', '--discard'].some((flag) =>
    extra.includes(flag),
  );
  return invoke(paths.root, [
    'ingest',
    '--store',
    paths.store,
    ...(inspection ? [] : ['--codex', paths.binary]),
    ...extra,
  ]);
}

function commit(paths: Paths, name: string) {
  writeFileSync(join(paths.root, name), 'unrelated change\n');
  paths.git(['add', name]);
  paths.git([
    '-c',
    'user.name=Test',
    '-c',
    'user.email=test@example.invalid',
    'commit',
    '-qm',
    name,
  ]);
}

test('exports a complete cohort and reuses every compatible extraction after an unrelated commit', async () => {
  await nativeProject((paths) => {
    expect(ingest(paths, ['--max-units', '2']).status).toBe(0);
    const exported = ingest(paths, ['--export', '--max-bytes', '65536']);
    expect(exported.status).toBe(0);
    const archive = join(dirname(paths.store), 'ingestion.json');
    writeFileSync(archive, exported.stdout);
    const old = JSON.parse(exported.stdout);
    expect(old.units).toHaveLength(2);
    commit(paths, 'unrelated.txt');
    const calls = readFileSync(paths.calls, 'utf8');
    const args = ['--reuse', archive, '--ref', 'HEAD', '--max-units', '0'];
    const reused = ingest(paths, args);
    expect(reused.stderr).toBe('');
    expect(reused.status).toBe(0);
    expect(JSON.parse(reused.stdout)).toMatchObject({
      operation: 'reuse',
      processed: 0,
      reused: 2,
      completed: 2,
      pending: 0,
    });
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    const result = JSON.parse(
      ingest(paths, ['--show', 'first.md', '--max-bytes', '65536']).stdout,
    ).result;
    const { association, ...original } = result;
    expect(original).toEqual(old.units[0].result);
    expect(association).toMatchObject({ planHash: JSON.parse(reused.stdout).planHash });
    expect(association.snapshot.commit).toBe(JSON.parse(reused.stdout).snapshot.commit);
    expect(invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']).status).toBe(
      0,
    );
    expect(ingest(paths, args).status).toBe(0);
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    expect(readFileSync(archive, 'utf8')).toBe(exported.stdout);
  });
});

test('reuses only unchanged units and leaves a changed source pending for ordinary ingestion', async () => {
  await nativeProject((paths) => {
    expect(ingest(paths, ['--max-units', '2']).status).toBe(0);
    const exported = ingest(paths, ['--export', '--max-bytes', '65536']);
    expect(exported.status).toBe(0);
    const archive = join(dirname(paths.store), 'ingestion.json');
    writeFileSync(archive, exported.stdout);
    writeFileSync(
      join(paths.root, 'second.md'),
      '# Changed\n\nNever treat a cache as authority.\n',
    );
    paths.git(['add', 'second.md']);
    paths.git([
      '-c',
      'user.name=Test',
      '-c',
      'user.email=test@example.invalid',
      'commit',
      '-qm',
      'change source',
    ]);
    const calls = readFileSync(paths.calls, 'utf8');
    const reused = ingest(paths, ['--reuse', archive, '--ref', 'HEAD', '--max-units', '0']);
    expect(reused.status).toBe(0);
    expect(JSON.parse(reused.stdout)).toMatchObject({ reused: 1, completed: 1, pending: 1 });
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    const changed = JSON.parse(
      ingest(paths, ['--show', 'second.md', '--max-bytes', '65536']).stdout,
    );
    expect(changed.state).toBe('pending');
    expect(changed.result).toBeNull();
    expect(ingest(paths, ['--max-units', '1']).status).toBe(0);
    const current = JSON.parse(
      ingest(paths, ['--show', 'second.md', '--max-bytes', '65536']).stdout,
    );
    expect(current.result.association).toBeUndefined();
    expect(readFileSync(paths.calls, 'utf8')).toBe(`${calls}called\n`);
  });
});

test('preserves failed extraction evidence and rejects an altered archive on a repeated transition', async () => {
  await nativeProject((paths) => {
    expect(ingest(paths, ['--max-units', '1']).status).toBe(0);
    writeFileSync(paths.scenario, 'changed-effort');
    expect(ingest(paths, ['--max-units', '1', '--attempts', '1']).status).toBe(1);
    const exported = ingest(paths, ['--export', '--max-bytes', '65536']);
    expect(exported.status).toBe(1);
    const archive = join(dirname(paths.store), 'ingestion.json');
    writeFileSync(archive, exported.stdout);
    const old = JSON.parse(exported.stdout);
    commit(paths, 'unrelated.txt');
    const reused = ingest(paths, ['--reuse', archive, '--ref', 'HEAD', '--max-units', '0']);
    expect(reused.status).toBe(1);
    expect(JSON.parse(reused.stdout)).toMatchObject({ completed: 1, failed: 1, reused: 2 });
    const failed = JSON.parse(
      ingest(paths, ['--show', 'second.md', '--max-bytes', '65536']).stdout,
    );
    expect(failed.result.attempts).toEqual(old.units[1].result.attempts);
    expect(failed.result.attempts[0].usage).toBeNull();
    const before = readFileSync(paths.store);
    const altered = JSON.parse(readFileSync(archive, 'utf8'));
    altered.units[0].result.attempts[0].usage.totalTokens = 1;
    writeFileSync(archive, JSON.stringify(altered));
    const repeated = ingest(paths, ['--reuse', archive, '--ref', 'HEAD', '--max-units', '0']);
    expect(repeated.status).toBe(1);
    expect(JSON.parse(repeated.stderr)).toMatchObject({
      error: { code: 'INGESTION_ARCHIVE_MISMATCH' },
    });
    expect(readFileSync(paths.store)).toEqual(before);
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\n');
  });
});

test('aborts reuse atomically when the archived cohort contains an uncertain invocation', async () => {
  await nativeProject(async (paths) => {
    expect(ingest(paths, ['--max-units', '0']).status).toBe(0);
    writeFileSync(paths.hold, 'hold native responses');
    const active = Bun.spawn(
      [
        process.execPath,
        cli,
        'ingest',
        '--root',
        paths.root,
        '--store',
        paths.store,
        '--codex',
        paths.binary,
        '--max-units',
        '1',
      ],
      { stdout: 'pipe', stderr: 'pipe' },
    );
    try {
      const deadline = performance.now() + 3000;
      while (readFileSync(paths.calls, 'utf8') !== 'called\n') {
        if (performance.now() > deadline)
          throw new Error('The fake model did not receive its request');
        await Bun.sleep(10);
      }
      const exported = ingest(paths, ['--export', '--max-bytes', '65536']);
      expect(exported.status).toBe(0);
      const archive = join(dirname(paths.store), 'uncertain.json');
      writeFileSync(archive, exported.stdout);
      commit(paths, 'unrelated.txt');
      const before = readFileSync(paths.store);
      const reused = ingest(paths, ['--reuse', archive, '--ref', 'HEAD', '--max-units', '0']);
      expect(reused.status).toBe(1);
      expect(JSON.parse(reused.stderr)).toMatchObject({ error: { code: 'INGESTION_UNRESOLVED' } });
      expect(readFileSync(paths.store)).toEqual(before);
      expect(readFileSync(paths.calls, 'utf8')).toBe('called\n');
    } finally {
      rmSync(paths.hold, { force: true });
      active.kill('SIGKILL');
      await active.exited;
    }
  });
});

test('rejects an altered reused association while reading the store', async () => {
  await nativeProject((paths) => {
    expect(ingest(paths, ['--max-units', '2']).status).toBe(0);
    const exported = ingest(paths, ['--export', '--max-bytes', '65536']);
    const archive = join(dirname(paths.store), 'ingestion.json');
    writeFileSync(archive, exported.stdout);
    commit(paths, 'unrelated.txt');
    expect(ingest(paths, ['--reuse', archive, '--ref', 'HEAD', '--max-units', '0']).status).toBe(0);
    const db = new Database(paths.store);
    const row = db
      .query<{ result: string }, [string]>('SELECT result FROM units WHERE id=?')
      .get('first.md');
    if (!row?.result) throw new Error('Expected a reused result');
    const result = JSON.parse(row.result);
    result.association.originalHash = '0'.repeat(64);
    const value = JSON.stringify(result);
    db.run('UPDATE units SET result=?, result_hash=? WHERE id=?', [value, hash(value), 'first.md']);
    db.close();
    const checked = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
    expect(checked.status).toBe(1);
    expect(JSON.parse(checked.stderr)).toMatchObject({
      error: { code: 'INVALID_INGESTION_STORE' },
    });
  });
});

test('does not reuse a cohort when the archived processing contract is incompatible', async () => {
  await nativeProject((paths) => {
    expect(ingest(paths, ['--max-units', '2']).status).toBe(0);
    const exported = ingest(paths, ['--export', '--max-bytes', '65536']);
    const archive = join(dirname(paths.store), 'ingestion.json');
    const altered = JSON.parse(exported.stdout) as {
      plan: {
        planHash: string;
        processing: { schemaHash: string };
        [key: string]: unknown;
      };
      planHash: string;
      [key: string]: unknown;
    };
    altered.plan.processing.schemaHash = '0'.repeat(64);
    const {
      command: _command,
      accepted: _accepted,
      planHash: _planHash,
      summary: _summary,
      ...cohort
    } = altered.plan;
    altered.planHash = hash(JSON.stringify(cohort));
    altered.plan.planHash = altered.planHash;
    writeFileSync(archive, JSON.stringify(altered));
    commit(paths, 'unrelated.txt');
    const before = readFileSync(paths.store);
    const result = ingest(paths, ['--reuse', archive, '--ref', 'HEAD', '--max-units', '0']);
    expect(result.status).toBe(1);
    expect(JSON.parse(result.stderr)).toMatchObject({
      error: { code: 'INGESTION_ARCHIVE_MISMATCH' },
    });
    expect(readFileSync(paths.store)).toEqual(before);
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
  });
});

test('revises a reused candidate with its original usage and revision budget intact', async () => {
  const source = '# Cache\n\nNever treat a cache as authority.\nPreserve the original source.\n';
  const evidence = [{ quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }];
  const omitted = [{ quote: 'Preserve the original source.', lineStart: 4, lineEnd: 4 }];
  await nativeProject(
    (paths) => {
      expect(ingest(paths, ['--max-units', '2']).status).toBe(0);
      const original = JSON.parse(
        ingest(paths, ['--show', 'first.md', '--max-bytes', '65536']).stdout,
      ).result;
      const archive = join(dirname(paths.store), 'ingestion.json');
      writeFileSync(archive, ingest(paths, ['--export', '--max-bytes', '65536']).stdout);
      commit(paths, 'unrelated.txt');
      const reused = ingest(paths, ['--reuse', archive, '--ref', 'HEAD', '--max-units', '0']);
      expect(reused.status).toBe(0);
      const currentSnapshot = JSON.parse(reused.stdout).snapshot;

      const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
      expect(built.status).toBe(0);
      const input = join(dirname(paths.store), 'current-graph.json');
      const feedback = join(dirname(paths.store), 'feedback.json');
      writeFileSync(input, built.stdout);
      const graph = JSON.parse(built.stdout) as {
        nodes: { id: string; source: string }[];
      };
      const node = graph.nodes.find((item: { source: string }) => item.source === 'first.md');
      if (!node) throw new Error('Expected the reused source claim');
      writeFileSync(
        paths.candidate,
        JSON.stringify({
          coverage: { verdict: 'incomplete', reason: 'The retention rule was omitted.', evidence },
          claims: [
            { id: node.id, verdict: 'faithful', reason: 'The prohibition is preserved.', evidence },
          ],
          relations: [],
          omissions: [{ text: 'Preserve the original source.', evidence: omitted }],
          context: { verdict: 'sufficient', reason: 'Both rules are present in the source.' },
        }),
      );
      const reviewed = invoke(paths.root, [
        'graph',
        'review',
        'first.md',
        '--input',
        input,
        '--codex',
        paths.binary,
      ]);
      expect(reviewed.status).toBe(1);
      writeFileSync(feedback, reviewed.stdout);
      const replacement = {
        ...original.candidate,
        claims: [
          ...original.candidate.claims,
          {
            id: 'c2',
            text: 'Preserve the original source.',
            kind: 'constraint',
            conditions: [],
            exceptions: [],
            evidence: omitted,
          },
        ],
      };
      writeFileSync(paths.candidate, JSON.stringify(replacement));
      const revised = ingest(paths, [
        '--revise',
        'first.md',
        '--input',
        input,
        '--feedback',
        feedback,
        '--attempts',
        '1',
      ]);
      expect(revised.status).toBe(0);
      expect(JSON.parse(revised.stdout)).toMatchObject({ processed: 1, attempts: { recorded: 3 } });
      const current = JSON.parse(
        ingest(paths, ['--show', 'first.md', '--max-bytes', '65536']).stdout,
      ).result;
      expect(current.candidate).toEqual(replacement);
      expect(current.attempts[0]).toEqual(original.attempts[0]);
      expect(current.attempts[0].usage.totalTokens).toBe(150);
      expect(current.revisions).toHaveLength(1);
      expect(current.association).toBeUndefined();
      expect(current.snapshot.commit).toBe(currentSnapshot.commit);
    },
    { source },
  );
});
