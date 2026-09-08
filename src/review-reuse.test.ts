import { expect, test } from 'bun:test';
import { readFileSync, writeFileSync, rmSync } from 'node:fs';
import { basename, dirname, join } from 'node:path';
import { invoke, projectWithReviews } from '../test/reviewed-project.ts';
import { nativeSource } from '../test/native-project.ts';
import type { Fixture, Paths } from '../test/reviewed-project.ts';

test('reuses unchanged source fidelity after another source changes without relabelling the original invocation', async () => {
  await projectWithReviews((paths, fixture) => {
    const archive = join(dirname(paths.store), 'reviews.json');
    const exported = invoke(paths.root, [
      'graph',
      'review',
      '--input',
      fixture.input,
      '--store',
      fixture.reviews,
      '--export',
      '--max-bytes',
      '65536',
    ]);
    expect(exported.status).toBe(0);
    writeFileSync(archive, exported.stdout);
    const original = JSON.parse(exported.stdout).reviews[0].result;
    writeFileSync(join(paths.root, 'second.md'), nativeSource + 'Preserve the original source.\n');
    paths.git(['add', 'second.md']);
    paths.git([
      '-c',
      'user.name=Test',
      '-c',
      'user.email=test@example.invalid',
      'commit',
      '-qm',
      'Extend the second policy',
    ]);
    rmSync(paths.candidate);
    const store = join(dirname(paths.store), 'updated-ingestion.sqlite');
    expect(invoke(paths.root, ['ingest', '--store', store, '--codex', paths.binary]).status).toBe(
      0,
    );
    const built = invoke(paths.root, ['graph', 'build', '--store', store, '--export']);
    expect(built.status).toBe(0);
    const input = join(dirname(paths.store), 'updated-graph.json');
    writeFileSync(input, built.stdout);
    const calls = readFileSync(paths.calls, 'utf8');
    const args = [
      'graph',
      'review',
      '--all',
      '--input',
      input,
      '--store',
      fixture.reviews,
      '--from',
      fixture.input,
      '--reuse',
      archive,
      '--max-units',
      '0',
    ];
    const reused = invoke(paths.root, args);
    expect(reused.stderr).toBe('');
    expect(reused.status).toBe(0);
    expect(JSON.parse(reused.stdout)).toMatchObject({
      completed: 1,
      pending: 1,
      processed: 0,
      reused: 1,
    });
    const current = invoke(paths.root, [
      'graph',
      'review',
      '--input',
      input,
      '--store',
      fixture.reviews,
      '--export',
      '--max-bytes',
      '65536',
    ]);
    expect(current.status).toBe(0);
    const retained = JSON.parse(current.stdout).reviews[0].result;
    const { association, ...receipt } = retained;
    expect(receipt).toEqual(original);
    expect(association).toMatchObject({ graphHash: JSON.parse(built.stdout).hash });
    expect(retained.graphHash).toBe(fixture.graph.hash);
    expect(retained.contract.promptHash).toBe(original.contract.promptHash);
    expect(invoke(paths.root, args).status).toBe(0);
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    expect(readFileSync(archive, 'utf8')).toBe(exported.stdout);
  });
});

function preserveReviews(paths: Paths, fixture: Fixture, status = 0) {
  const path = join(dirname(paths.store), basename(fixture.input, '.json') + '-reviews.json');
  const result = invoke(paths.root, [
    'graph',
    'review',
    '--input',
    fixture.input,
    '--store',
    fixture.reviews,
    '--export',
    '--max-bytes',
    '65536',
  ]);
  expect(result.stderr).toBe('');
  expect(result.status).toBe(status);
  writeFileSync(path, result.stdout);
  return path;
}

function removeSecondSource(paths: Paths) {
  paths.git(['rm', 'second.md']);
  paths.git([
    '-c',
    'user.name=Test',
    '-c',
    'user.email=test@example.invalid',
    'commit',
    '-qm',
    'Keep one policy',
  ]);
  rmSync(paths.candidate, { force: true });
  const store = join(dirname(paths.store), 'updated-ingestion.sqlite');
  expect(invoke(paths.root, ['ingest', '--store', store, '--codex', paths.binary]).status).toBe(0);
  const built = invoke(paths.root, ['graph', 'build', '--store', store, '--export']);
  expect(built.status).toBe(0);
  const input = join(dirname(paths.store), 'updated-graph.json');
  writeFileSync(input, built.stdout);
  return input;
}

function transfer(fixture: Fixture, input: string, archive: string) {
  return [
    'graph',
    'review',
    '--all',
    '--input',
    input,
    '--store',
    fixture.reviews,
    '--from',
    fixture.input,
    '--reuse',
    archive,
    '--max-units',
    '0',
  ];
}

test('admission revalidates reused source evidence and rejects alterations to its original receipt', async () => {
  await projectWithReviews((paths, fixture) => {
    const archive = preserveReviews(paths, fixture);
    const input = removeSecondSource(paths);
    const calls = readFileSync(paths.calls, 'utf8');
    expect(invoke(paths.root, transfer(fixture, input, archive)).status).toBe(0);
    const admitted = invoke(paths.root, [
      'graph',
      'admit',
      '--input',
      input,
      '--reviews',
      fixture.reviews,
      '--export',
    ]);
    expect(admitted.status).toBe(0);
    const artifact = join(dirname(paths.store), 'admitted.json');
    writeFileSync(artifact, admitted.stdout);
    expect(invoke(paths.root, ['graph', 'check', '--input', artifact]).status).toBe(0);
    const changed = JSON.parse(admitted.stdout);
    changed.sourceReviews[0].report.usage.totalTokens = 1;
    const { hash: _hash, ...content } = changed;
    changed.hash = new Bun.CryptoHasher('sha256').update(JSON.stringify(content)).digest('hex');
    writeFileSync(artifact, JSON.stringify(changed));
    expect(invoke(paths.root, ['graph', 'check', '--input', artifact]).status).toBe(1);
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
  });
});

test('keeps an unchanged adverse review negative after reassociation and does not spend another call', async () => {
  await projectWithReviews(
    (paths, fixture) => {
      const node = fixture.graph.nodes.find((node) => node.source === 'first.md');
      if (!node) throw new Error('Expected a claim');
      const evidence = [{ quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }];
      writeFileSync(
        paths.candidate,
        JSON.stringify({
          coverage: {
            verdict: 'incomplete',
            reason: 'The source retention rule is absent.',
            evidence,
          },
          claims: [
            {
              id: node.id,
              verdict: 'faithful',
              reason: 'The cache prohibition is preserved.',
              evidence,
            },
          ],
          relations: [],
          omissions: [
            {
              text: 'Preserve the original source.',
              evidence: [{ quote: 'Preserve the original source.', lineStart: 4, lineEnd: 4 }],
            },
          ],
          context: { verdict: 'sufficient', reason: 'The full source contains both rules.' },
        }),
      );
      expect(
        invoke(paths.root, [
          'graph',
          'review',
          '--all',
          '--input',
          fixture.input,
          '--store',
          fixture.reviews,
          '--codex',
          paths.binary,
          '--max-units',
          '1',
        ]).status,
      ).toBe(1);
      const archive = preserveReviews(paths, fixture, 1);
      const input = removeSecondSource(paths);
      const calls = readFileSync(paths.calls, 'utf8');
      const result = invoke(paths.root, transfer(fixture, input, archive));
      expect(result.status).toBe(1);
      expect(JSON.parse(result.stdout)).toMatchObject({
        failed: 1,
        completed: 0,
        pending: 0,
        processed: 0,
        reportedTokens: 150,
      });
      expect(
        invoke(paths.root, ['graph', 'admit', '--input', input, '--reviews', fixture.reviews])
          .status,
      ).toBe(1);
      expect(
        invoke(paths.root, [
          'graph',
          'review',
          '--all',
          '--input',
          input,
          '--store',
          fixture.reviews,
          '--codex',
          paths.binary,
        ]).status,
      ).toBe(1);
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
      const currentEvidence = preserveReviews(paths, { ...fixture, input }, 1);
      const correction = invoke(paths.root, [
        'ingest',
        '--revise',
        'first.md',
        '--input',
        input,
        '--store',
        join(dirname(paths.store), 'updated-ingestion.sqlite'),
        '--feedback',
        currentEvidence,
        '--prepare',
      ]);
      expect(correction.stderr).toBe('');
      expect(correction.status).toBe(0);
      expect(JSON.parse(correction.stdout).prompt).toContain('Preserve the original source.');
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    },
    0,
    false,
    nativeSource + 'Preserve the original source.\n',
  );
});

test('requires a complete matching archive before replacing the retained cohort', async () => {
  await projectWithReviews((paths, fixture) => {
    const archive = preserveReviews(paths, fixture);
    const input = removeSecondSource(paths);
    const original = readFileSync(archive, 'utf8');
    const store = readFileSync(fixture.reviews);
    const calls = readFileSync(paths.calls, 'utf8');
    const changed = JSON.parse(original);
    changed.reviews[0].result.report.usage.totalTokens = 1;
    writeFileSync(archive, JSON.stringify(changed));
    const result = invoke(paths.root, transfer(fixture, input, archive));
    expect(result.status).toBe(1);
    expect(JSON.parse(result.stderr).error.code).toBe('REVIEW_ARCHIVE_MISMATCH');
    expect(readFileSync(fixture.reviews)).toEqual(store);
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
  });
});

test('does not replace a cohort that retains an uncertain model start even after its process ended', async () => {
  await projectWithReviews((paths, fixture) => {
    writeFileSync(paths.scenario, 'start-unconfirmed');
    const review = invoke(paths.root, [
      'graph',
      'review',
      '--all',
      '--input',
      fixture.input,
      '--store',
      fixture.reviews,
      '--codex',
      paths.binary,
      '--max-units',
      '1',
      '--deadline-ms',
      '100',
    ]);
    expect(review.status).toBe(1);
    rmSync(paths.scenario);
    const archive = preserveReviews(paths, fixture, 1);
    const input = removeSecondSource(paths);
    const store = readFileSync(fixture.reviews);
    const calls = readFileSync(paths.calls, 'utf8');
    const result = invoke(paths.root, transfer(fixture, input, archive));
    expect(result.status).toBe(1);
    expect(result.stderr).toContain('REVIEW_UNRESOLVED');
    expect(readFileSync(fixture.reviews)).toEqual(store);
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
  }, 0);
});
