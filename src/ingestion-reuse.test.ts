import { expect, test } from 'bun:test';
import { Database } from 'bun:sqlite';
import { readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { nativeProject } from '../test/native-project.ts';
import { invoke } from '../test/reviewed-project.ts';
import { hash } from './sources/markdown.ts';
import { IngestionStore, validateResultForPlan } from './ingestion/store.ts';
import { createPlan } from './ingestion/plan.ts';
import { loadSnapshot } from './workspace/snapshot.ts';

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

test.each(['invalid-json', 'start-unconfirmed'])(
  'checks the full oversized invocation report before transfer: %s',
  async (scenario) => {
    await nativeProject((paths) => {
      const uncertain = scenario === 'start-unconfirmed';
      writeFileSync(paths.scenario, scenario);
      const extracted = invoke(paths.root, [
        'extract',
        'first.md',
        '--codex',
        paths.binary,
        '--attempts',
        '1',
        '--deadline-ms',
        uncertain ? '100' : '600000',
      ]);
      expect(extracted.stderr).toBe('');
      expect(extracted.status).toBe(1);
      const result: Parameters<IngestionStore['complete']>[2] = JSON.parse(extracted.stdout);
      const plan = createPlan(loadSnapshot({ root: paths.root, ref: 'HEAD' }), null);
      const first = validateResultForPlan(result, 'first.md', plan).attempts[0];
      if (!first) throw new Error('Expected the fixture invocation report');
      const report = { ...first, diagnostic: { detail: 'x'.repeat(1024 * 1024) } };
      {
        using store = new IngestionStore(paths.store, plan);
        const owner = 'oversized-report-fixture';
        expect(store.claim(owner)).toBe('first.md');
        store.checkpoint('first.md', owner, {
          state: 'started',
          attempt: 1,
          promptHash: report.promptHash,
          deadlineMilliseconds: report.deadlineMilliseconds,
        });
        store.checkpoint('first.md', owner, { state: 'recorded', attempt: 1, report });
        store.complete('first.md', owner, { ...result, attempts: [report] });
      }
      const exported = ingest(paths, ['--export', '--max-bytes', '4194304']);
      expect(exported.stderr).toBe('');
      expect(exported.status).toBe(1);
      const cohort: ReturnType<typeof IngestionStore.export> = JSON.parse(exported.stdout);
      const row = cohort.units[0];
      if (!row) throw new Error('Expected the exported fixture row');
      expect(row.checkpoint.reports[0]).toHaveProperty('omittedDetails');
      expect(row.checkpoint.reports[0]?.rejectedOutput).toEqual(first.rejectedOutput);
      expect(row.checkpoint.reports[0]).not.toHaveProperty('turnAccepted');
      expect(row.checkpoint.reports[0]).not.toHaveProperty('cleanup');
      expect(row.result?.attempts[0]).toEqual(report);
      expect(row.result?.attempts[0]?.turnAccepted).toBe(uncertain ? 'unknown' : 'confirmed');
      expect(row.result?.attempts[0]?.cleanup).toBe('confirmed');
      const archive = join(dirname(paths.store), 'large-report.json');
      writeFileSync(archive, exported.stdout);
      commit(paths, 'unrelated.txt');
      const before = readFileSync(paths.store);
      const calls = readFileSync(paths.calls, 'utf8');
      const reused = ingest(paths, ['--reuse', archive, '--ref', 'HEAD', '--max-units', '0']);
      if (uncertain) {
        expect(reused.status).toBe(1);
        expect(reused.stderr).toContain('INGESTION_UNRESOLVED');
        expect(readFileSync(paths.store)).toEqual(before);
      } else {
        expect(reused.stderr).toBe('');
        expect(reused.status).toBe(1);
        expect(JSON.parse(reused.stdout)).toMatchObject({
          reused: 1,
          completed: 0,
          failed: 1,
          pending: 1,
          processed: 0,
          attempts: { recorded: 1, knownTotalTokens: 150 },
        });
        const current: ReturnType<typeof IngestionStore.result> = JSON.parse(
          ingest(paths, ['--show', 'first.md', '--max-bytes', '4194304']).stdout,
        );
        expect(current.result?.attempts).toEqual([report]);
      }
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
      expect(readFileSync(archive, 'utf8')).toBe(exported.stdout);
    });
  },
);

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
            { id: 'c1', verdict: 'faithful', reason: 'The prohibition is preserved.', evidence },
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
      expect(JSON.parse(reviewed.stdout)).toMatchObject({
        report: { outcome: 'completed' },
        review: { claims: [{ id: node.id, verdict: 'faithful' }] },
      });
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

test.each([false, true])(
  'preserves rejected extraction history through retry and reuse (legacy: %s)',
  async (legacy) => {
    await nativeProject((paths) => {
      writeFileSync(paths.scenario, 'invalid-json');
      const failed = ingest(paths, ['--attempts', '1']);
      expect(failed.status).toBe(1);
      expect(JSON.parse(failed.stdout)).toMatchObject({
        failed: 1,
        unresolved: 0,
        attempts: { recorded: 1, knownTotalTokens: 150 },
      });
      const inspected = ingest(paths, ['--show', 'first.md', '--max-bytes', '65536']);
      expect(inspected.stderr).toBe('');
      expect(JSON.parse(inspected.stdout).result).toMatchObject({
        candidate: null,
        attempts: [
          {
            outcome: 'invalid-output',
            usage: { totalTokens: 150 },
            rejectedOutput: {
              text: '{broken',
              hash: hash('{broken'),
              bytes: 7,
              omittedReason: null,
            },
          },
        ],
      });
      if (legacy) {
        using db = new Database(paths.store);
        const row = db
          .query<
            { result: string; attempts: string },
            []
          >("SELECT result, attempts FROM units WHERE id='first.md'")
          .get();
        if (!row) throw new Error('Expected failed extraction');
        const result = JSON.parse(row.result);
        const checkpoint = JSON.parse(row.attempts);
        delete result.attempts[0].rejectedOutput;
        delete checkpoint.reports[0].rejectedOutput;
        const resultText = JSON.stringify(result);
        const checkpointText = JSON.stringify(checkpoint);
        db.run(
          "UPDATE units SET result=?, result_hash=?, attempts=?, attempts_hash=? WHERE id='first.md'",
          [resultText, hash(resultText), checkpointText, hash(checkpointText)],
        );
      }
      const exported = ingest(paths, ['--export', '--max-bytes', '65536']);
      expect(exported.stderr).toBe('');
      const previous = JSON.parse(exported.stdout).units[0].result;
      const archive = join(dirname(paths.store), 'rejected.json');
      writeFileSync(archive, exported.stdout);
      commit(paths, 'unrelated.txt');
      const calls = readFileSync(paths.calls, 'utf8');
      const reused = ingest(paths, ['--reuse', archive, '--ref', 'HEAD', '--max-units', '0']);
      expect(reused.stderr).toBe('');
      expect(JSON.parse(reused.stdout)).toMatchObject({ failed: 1, reused: 1, processed: 0 });
      const current = JSON.parse(
        ingest(paths, ['--show', 'first.md', '--max-bytes', '65536']).stdout,
      ).result;
      const { association, ...original } = current;
      expect(original).toEqual(previous);
      expect(association.originalHash).toBe(hash(JSON.stringify(previous)));
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
      rmSync(paths.scenario);
      const retried = ingest(paths, ['--retry-failed', 'first.md', '--attempts', '2']);
      expect(retried.stderr).toBe('');
      expect(retried.status).toBe(0);
      expect(JSON.parse(retried.stdout)).toMatchObject({
        completed: 2,
        unresolved: 0,
        attempts: { recorded: 3, knownTotalTokens: 450 },
      });
      const recovered = JSON.parse(
        ingest(paths, ['--show', 'first.md', '--max-bytes', '65536']).stdout,
      ).result;
      expect(recovered.attempts[0]).toEqual(previous.attempts[0]);
      expect(recovered.attempts[1].rejectedOutput).toBeUndefined();
      writeFileSync(archive, ingest(paths, ['--export', '--max-bytes', '65536']).stdout);
      commit(paths, 'next.txt');
      expect(ingest(paths, ['--reuse', archive, '--ref', 'HEAD', '--max-units', '0']).status).toBe(
        0,
      );
      const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
      expect(built.stderr).toBe('');
      expect(built.status).toBe(0);
      const input = join(dirname(paths.store), 'recovered-graph.json');
      writeFileSync(input, built.stdout);
      expect(invoke(paths.root, ['graph', 'check', '--input', input]).status).toBe(0);
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls + 'called\ncalled\n');
    });
  },
  15000,
);

test('rejects altered or misplaced extraction diagnostics despite recomputed store hashes', async () => {
  await nativeProject((paths) => {
    writeFileSync(paths.scenario, 'retry-success');
    expect(ingest(paths).status).toBe(0);
    const exported = ingest(paths, ['--export', '--max-bytes', '65536']);
    expect(exported.stderr).toBe('');
    const archive = join(dirname(paths.store), 'altered-rejection.json');
    const calls = readFileSync(paths.calls, 'utf8');
    using db = new Database(paths.store);
    for (const field of ['text', 'hash', 'bytes', 'omittedReason', 'successful']) {
      const cohort = JSON.parse(exported.stdout);
      const row = cohort.units[0];
      const rejected = row.result.attempts[0].rejectedOutput;
      if (field === 'text') rejected.text += 'altered';
      if (field === 'hash') rejected.hash = '0'.repeat(64);
      if (field === 'bytes') rejected.bytes += 1;
      if (field === 'omittedReason') rejected.omittedReason = 'retention-limit';
      if (field === 'successful') row.result.attempts[1].rejectedOutput = rejected;
      row.checkpoint.reports = row.result.attempts;
      const resultText = JSON.stringify(row.result);
      const checkpointText = JSON.stringify(row.checkpoint);
      db.run(
        "UPDATE units SET result=?, result_hash=?, attempts=?, attempts_hash=? WHERE id='first.md'",
        [resultText, hash(resultText), checkpointText, hash(checkpointText)],
      );
      for (const args of [['--show', 'first.md'], ['--export'], ['--max-units', '0']]) {
        const checked = ingest(paths, args);
        expect(checked.status).toBe(1);
        expect(JSON.parse(checked.stderr).error.code).toBe('INVALID_INGESTION_STORE');
      }
      const built = invoke(paths.root, ['graph', 'build', '--store', paths.store]);
      expect(built.status).toBe(1);
      expect(JSON.parse(built.stderr).error.code).toBe('INVALID_INGESTION_STORE');
      writeFileSync(archive, JSON.stringify(cohort));
      const reused = ingest(paths, ['--reuse', archive, '--ref', 'HEAD', '--max-units', '0']);
      expect(reused.status).toBe(1);
      expect(JSON.parse(reused.stderr).error.code).toBe('INGESTION_ARCHIVE_MISMATCH');
    }
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
  });
}, 15000);

test.each([32768, 32770, 1048576])(
  'bounds rejected UTF-8 extraction text without losing usage (%i bytes)',
  async (bytes) => {
    await nativeProject((paths) => {
      const text = JSON.stringify('é'.repeat((bytes - 2) / 2));
      writeFileSync(paths.candidate, text);
      const failed = ingest(paths, ['--max-units', '1']);
      expect(failed.stderr).toBe('');
      expect(failed.status).toBe(1);
      expect(JSON.parse(failed.stdout)).toMatchObject({
        failed: 1,
        unresolved: 0,
        attempts: { recorded: 3, unresolved: 0, unknownUsage: 0, knownTotalTokens: 450 },
      });
      const shown = ingest(paths, ['--show', 'first.md', '--max-bytes', '131072']);
      expect(shown.stderr).toBe('');
      const result: ReturnType<typeof IngestionStore.result> = JSON.parse(shown.stdout);
      expect(result.state).toBe('failed');
      expect(result.result?.candidate).toBeNull();
      for (const report of result.result?.attempts ?? []) {
        expect(report.usage?.totalTokens).toBe(150);
        expect(report.rejectedOutput).toEqual({
          text: bytes === 32768 ? text : null,
          hash: hash(text),
          bytes,
          omittedReason: bytes === 32768 ? null : 'retention-limit',
        });
      }
      const exported = ingest(paths, ['--export', '--max-bytes', '262144']);
      expect(exported.stderr).toBe('');
      expect(JSON.parse(exported.stdout).units[0].result).toEqual(result.result);
      expect(readFileSync(paths.calls, 'utf8')).toBe('called\n'.repeat(3));
    });
  },
);
