import { expect, test } from 'bun:test';
import { readFileSync, writeFileSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { nativeSource } from '../test/native-project.ts';
import type { ComparisonResult } from './graph/comparison-cohort.ts';
import {
  invoke,
  projectWithReviews,
  compare,
  comparisonResponse,
  type Paths,
  type Fixture,
} from '../test/reviewed-project.ts';

function preserve(paths: Paths, fixture: Fixture, operation: 'review' | 'compare', neighbors = 1) {
  const archive = join(dirname(paths.store), `${operation}-${fixture.graph.hash}.json`);
  const result = invoke(paths.root, [
    'graph',
    operation,
    '--export',
    '--input',
    fixture.input,
    '--store',
    operation === 'review' ? fixture.reviews : fixture.comparisons,
    '--max-bytes',
    '1048576',
    ...(operation === 'compare' ? ['--neighbors', String(neighbors)] : []),
  ]);
  expect(result.stderr).toBe('');
  writeFileSync(archive, result.stdout);
  return archive;
}

function update(paths: Paths, fixture: Fixture, path = 'third.md', content = nativeSource) {
  writeFileSync(join(paths.root, path), content);
  paths.git(['add', path]);
  paths.git([
    '-c',
    'user.name=Test',
    '-c',
    'user.email=test@example.invalid',
    'commit',
    '-qm',
    'Update policy',
  ]);
  rmSync(paths.candidate, { force: true });
  const commit = paths.git(['rev-parse', 'HEAD']);
  const store = join(dirname(paths.store), `${commit}-ingestion.sqlite`);
  expect(invoke(paths.root, ['ingest', '--store', store, '--codex', paths.binary]).status).toBe(0);
  const built = invoke(paths.root, ['graph', 'build', '--store', store, '--export']);
  expect(built.status).toBe(0);
  const input = join(dirname(paths.store), `${commit}-graph.json`);
  writeFileSync(input, built.stdout);
  return {
    ...fixture,
    input,
    built: built.stdout,
    graph: JSON.parse(built.stdout) as Fixture['graph'],
  };
}

function transfer(previous: Fixture, current: Fixture, archive: string) {
  return [
    'graph',
    'compare',
    '--all',
    '--input',
    current.input,
    '--store',
    current.comparisons,
    '--from',
    previous.input,
    '--reuse',
    archive,
    '--neighbors',
    '2',
    '--max-units',
    '0',
  ];
}

function finishReviews(paths: Paths, fixture: Fixture) {
  const node = fixture.graph.nodes.find((entry) => entry.source === 'third.md');
  if (!node) throw new Error('Expected new source claim');
  const evidence = [{ quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }];
  writeFileSync(
    paths.candidate,
    JSON.stringify({
      coverage: { verdict: 'complete', reason: 'The complete prohibition is preserved.', evidence },
      claims: [
        { id: 'c1', verdict: 'faithful', reason: 'The prohibition is preserved.', evidence },
      ],
      relations: [],
      omissions: [],
      context: { verdict: 'sufficient', reason: 'The full source is supplied.' },
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
    ]).status,
  ).toBe(0);
}

test('reuses an unchanged pair across graph and selection changes, then admits without relabelling its receipt', async () => {
  await projectWithReviews((paths, original) => {
    expect(compare(paths, original).status).toBe(0);
    const archive = preserve(paths, original, 'compare');
    const oldExport = readFileSync(archive, 'utf8');
    const oldResult = JSON.parse(oldExport).comparisons[0].result;
    const reviews = preserve(paths, original, 'review');
    const current = update(paths, original);
    const calls = readFileSync(paths.calls, 'utf8');
    const args = transfer(original, current, archive);
    const reused = invoke(paths.root, args);
    expect(reused.stderr).toBe('');
    expect(reused.status).toBe(0);
    expect(JSON.parse(reused.stdout)).toMatchObject({
      completed: 1,
      pending: 2,
      processed: 0,
      reused: 1,
    });
    const transferred = preserve(paths, current, 'compare', 2);
    const transferredEvidence: { comparisons: { result: ComparisonResult | null }[] } = JSON.parse(
      readFileSync(transferred, 'utf8'),
    );
    const retained = transferredEvidence.comparisons.find((row) => row.result !== null)?.result;
    if (!retained) throw new Error('Expected a reused comparison');
    const { association, ...receipt } = retained;
    expect(receipt).toEqual(oldResult);
    expect(association).toMatchObject({
      graphHash: current.graph.hash,
      originalSelectionHash: JSON.parse(oldExport).selectionHash,
    });
    expect(invoke(paths.root, args).status).toBe(0);
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    expect(readFileSync(archive, 'utf8')).toBe(oldExport);
    expect(
      invoke(paths.root, [
        'graph',
        'review',
        '--all',
        '--input',
        current.input,
        '--store',
        current.reviews,
        '--from',
        original.input,
        '--reuse',
        reviews,
        '--max-units',
        '0',
      ]).status,
    ).toBe(0);
    finishReviews(paths, current);
    writeFileSync(paths.candidate, JSON.stringify(comparisonResponse()));
    const completed = invoke(paths.root, [
      'graph',
      'compare',
      '--all',
      '--input',
      current.input,
      '--store',
      current.comparisons,
      '--neighbors',
      '2',
      '--codex',
      paths.binary,
    ]);
    expect(completed.stderr).toBe('');
    expect(JSON.parse(completed.stdout)).toMatchObject({
      processed: 2,
      completed: 3,
      reportedTokens: 450,
    });
    const admitted = invoke(paths.root, [
      'graph',
      'admit',
      '--input',
      current.input,
      '--reviews',
      current.reviews,
      '--comparisons',
      current.comparisons,
      '--neighbors',
      '2',
      '--export',
    ]);
    expect(admitted.stderr).toBe('');
    expect(admitted.status).toBe(0);
    const artifact = join(dirname(paths.store), 'admitted.json');
    writeFileSync(artifact, admitted.stdout);
    const finalCalls = readFileSync(paths.calls, 'utf8');
    expect(invoke(paths.root, ['graph', 'check', '--input', artifact]).status).toBe(0);
    const changed: { hash: string; comparisons: ComparisonResult[] } = JSON.parse(admitted.stdout);
    const originalUsage = changed.comparisons.find((entry) => entry.association)?.report.usage;
    if (!originalUsage) throw new Error('Expected preserved invocation usage');
    originalUsage.totalTokens = 1;
    const { hash: _hash, ...content } = changed;
    changed.hash = new Bun.CryptoHasher('sha256').update(JSON.stringify(content)).digest('hex');
    writeFileSync(artifact, JSON.stringify(changed));
    expect(invoke(paths.root, ['graph', 'check', '--input', artifact]).status).toBe(1);
    expect(readFileSync(paths.calls, 'utf8')).toBe(finalCalls);
  });
});

test('invalidates pairs with changed source text and leaves their original evidence in the archive', async () => {
  await projectWithReviews((paths, original) => {
    expect(compare(paths, original).status).toBe(0);
    const archive = preserve(paths, original, 'compare');
    const bytes = readFileSync(archive);
    const current = update(
      paths,
      original,
      'second.md',
      nativeSource + 'Preserve the original source.\n',
    );
    const calls = readFileSync(paths.calls, 'utf8');
    const result = invoke(paths.root, transfer(original, current, archive));
    expect(result.stderr).toBe('');
    expect(JSON.parse(result.stdout)).toMatchObject({
      completed: 0,
      pending: 1,
      processed: 0,
      reused: 0,
    });
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    expect(readFileSync(archive)).toEqual(bytes);
  });
});

test('retains adverse pair findings across repeated associations without new model calls', async () => {
  await projectWithReviews((paths, original) => {
    expect(
      compare(paths, original, {
        ...comparisonResponse(),
        context: { verdict: 'insufficient', reason: 'An external authority is missing.' },
      }).status,
    ).toBe(1);
    const archive = preserve(paths, original, 'compare');
    const receipt = JSON.parse(readFileSync(archive, 'utf8')).comparisons[0].result;
    const current = update(paths, original);
    const firstCalls = readFileSync(paths.calls, 'utf8');
    const reused = invoke(paths.root, transfer(original, current, archive));
    expect(reused.stderr).toBe('');
    expect(JSON.parse(reused.stdout)).toMatchObject({
      failed: 1,
      pending: 2,
      processed: 0,
      reportedTokens: 150,
    });
    expect(readFileSync(paths.calls, 'utf8')).toBe(firstCalls);
    const nextArchive = preserve(paths, current, 'compare', 2);
    const next = update(
      paths,
      current,
      'third.md',
      nativeSource + 'Preserve the original source.\n',
    );
    const calls = readFileSync(paths.calls, 'utf8');
    const repeated = invoke(paths.root, transfer(current, next, nextArchive));
    expect(repeated.stderr).toBe('');
    expect(JSON.parse(repeated.stdout)).toMatchObject({ failed: 1, pending: 2, processed: 0 });
    const exported = preserve(paths, next, 'compare', 2);
    const evidence: { comparisons: { state: string; result: ComparisonResult | null }[] } =
      JSON.parse(readFileSync(exported, 'utf8'));
    const result = evidence.comparisons.find((row) => row.state === 'failed')?.result;
    if (!result?.association) throw new Error('Expected the retained adverse comparison');
    const { association, ...retained } = result;
    expect(retained).toEqual(receipt);
    expect(association.originalSelectionHash).toBe(
      JSON.parse(readFileSync(archive, 'utf8')).selectionHash,
    );
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
  });
});

test('rejects altered archive receipts and selection before changing the working store', async () => {
  await projectWithReviews((paths, original) => {
    expect(compare(paths, original).status).toBe(0);
    const archive = preserve(paths, original, 'compare');
    const bytes = readFileSync(archive, 'utf8');
    const current = update(paths, original);
    const store = readFileSync(original.comparisons);
    const calls = readFileSync(paths.calls, 'utf8');
    for (const field of ['receipt', 'selection', 'coverage', 'contract']) {
      const altered = JSON.parse(bytes);
      if (field === 'receipt') altered.comparisons[0].result.report.usage.totalTokens = 1;
      if (field === 'selection') altered.selection.pairs[0].reasons = [];
      if (field === 'coverage') altered.comparisons = [];
      if (field === 'contract') altered.plan.contract.schemaHash = '0'.repeat(64);
      writeFileSync(archive, JSON.stringify(altered));
      const result = invoke(paths.root, transfer(original, current, archive));
      expect(result.status).toBe(1);
      expect(result.stderr).toContain('REVIEW_ARCHIVE_MISMATCH');
      expect(readFileSync(original.comparisons)).toEqual(store);
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    }
  });
});

test('blocks replacement after an uncertain comparison start and preserves all recorded usage', async () => {
  await projectWithReviews((paths, original) => {
    writeFileSync(paths.scenario, 'start-unconfirmed');
    writeFileSync(paths.candidate, JSON.stringify(comparisonResponse()));
    expect(
      invoke(paths.root, [
        'graph',
        'compare',
        '--all',
        '--input',
        original.input,
        '--store',
        original.comparisons,
        '--neighbors',
        '1',
        '--codex',
        paths.binary,
        '--deadline-ms',
        '100',
      ]).status,
    ).toBe(1);
    rmSync(paths.scenario);
    const archive = preserve(paths, original, 'compare');
    const current = update(paths, original);
    const store = readFileSync(original.comparisons);
    const calls = readFileSync(paths.calls, 'utf8');
    const result = invoke(paths.root, transfer(original, current, archive));
    expect(result.status).toBe(1);
    expect(result.stderr).toContain('REVIEW_UNRESOLVED');
    expect(readFileSync(original.comparisons)).toEqual(store);
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
  });
});

test('rejects an unrecorded destination and reauthenticates the archive when repeating a transition', async () => {
  await projectWithReviews((paths, original) => {
    expect(compare(paths, original).status).toBe(0);
    const archive = preserve(paths, original, 'compare');
    const current = update(paths, original);
    const unrelated = { ...current, comparisons: join(dirname(paths.store), 'unrelated.sqlite') };
    expect(
      invoke(paths.root, [
        'graph',
        'compare',
        '--all',
        '--input',
        unrelated.input,
        '--store',
        unrelated.comparisons,
        '--neighbors',
        '2',
        '--max-units',
        '0',
      ]).status,
    ).toBe(0);
    const bytes = readFileSync(unrelated.comparisons);
    const calls = readFileSync(paths.calls, 'utf8');
    const rejected = invoke(paths.root, [
      ...transfer(original, unrelated, archive).slice(0, -2),
      '--max-units',
      '1',
      '--codex',
      paths.binary,
    ]);
    expect(rejected.status).toBe(1);
    expect(rejected.stderr).toContain('REVIEW_ARCHIVE_MISMATCH');
    expect(readFileSync(unrelated.comparisons)).toEqual(bytes);
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);

    const args = transfer(original, current, archive);
    expect(invoke(paths.root, args).status).toBe(0);
    expect(invoke(paths.root, args).status).toBe(0);
    const retainedStore = readFileSync(current.comparisons);
    const altered = JSON.parse(readFileSync(archive, 'utf8'));
    altered.comparisons[0].result.report.usage.totalTokens = 1;
    writeFileSync(archive, JSON.stringify(altered));
    const repeated = invoke(paths.root, args);
    expect(repeated.status).toBe(1);
    expect(repeated.stderr).toContain('REVIEW_ARCHIVE_MISMATCH');
    expect(readFileSync(current.comparisons)).toEqual(retainedStore);
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
  });
});
