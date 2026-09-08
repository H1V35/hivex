import { createHash } from 'node:crypto';
import { expect, test } from 'bun:test';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { nativeProject } from '../test/native-project.ts';
import {
  invoke,
  projectWithReviews,
  compare,
  admit,
  retained,
  comparisonResponse,
} from '../test/reviewed-project.ts';

test('admits complete reviewed evidence and exposes a cross-source relationship without model calls', async () => {
  await projectWithReviews((paths, fixture) => {
    expect(compare(paths, fixture).status).toBe(0);
    const calls = readFileSync(paths.calls, 'utf8');
    const result = admit(paths, fixture);
    expect(result.stderr).toBe('');
    expect(result.status).toBe(0);
    const admitted = JSON.parse(result.stdout);
    expect(admitted).toMatchObject({
      format: 'hivex-admitted-graph',
      version: 1,
      accepted: true,
      graph: { hash: fixture.graph.hash },
      relationships: [{ type: 'equivalent', scope: { extent: 'whole-claim' } }],
      manifest: { globalConsistency: 'not-proven' },
    });
    expect(admit(paths, fixture).stdout).toBe(result.stdout);
    const input = retained(paths, result.stdout);
    const node = fixture.graph.nodes.find((node) => node.source === 'first.md');
    if (!node) throw new Error('Expected first claim');
    const expanded = invoke(paths.root, ['graph', 'neighbors', node.id, '--input', input]);
    expect(expanded.status).toBe(0);
    expect(JSON.parse(expanded.stdout)).toMatchObject({
      accepted: true,
      hash: admitted.hash,
      relations: [{ edge: { type: 'equivalent' } }],
    });
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    expect(readFileSync(fixture.input, 'utf8')).toBe(fixture.built);
  });
});

test('cannot admit pending source fidelity or repeat model work to complete it implicitly', async () => {
  await projectWithReviews((paths, fixture) => {
    const calls = readFileSync(paths.calls, 'utf8');
    const result = admit(paths, fixture);
    expect(result.status).toBe(1);
    expect(JSON.parse(result.stderr)).toMatchObject({ error: { code: 'GRAPH_ADMISSION_INVALID' } });
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
  }, 0);
});

test('retains a negative comparison as an admission blocker', async () => {
  await projectWithReviews((paths, fixture) => {
    const response = comparisonResponse();
    response.context = { verdict: 'insufficient', reason: 'An applicable amendment is missing.' };
    expect(compare(paths, fixture, response).status).toBe(1);
    const calls = readFileSync(paths.calls, 'utf8');
    expect(admit(paths, fixture).status).toBe(1);
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
  });
});

test('rejects circular precedence even when each pair assessment reports success', async () => {
  await projectWithReviews((paths, fixture) => {
    const response = comparisonResponse();
    const relation = response.relations[0];
    if (!relation) throw new Error('Expected relationship');
    response.relations = [
      { ...relation, type: 'supersedes' },
      { ...relation, id: 'r2', from: 's2:c1', to: 's1:c1', type: 'supersedes' },
    ];
    response.assessments = response.assessments.map((entry) => ({
      ...entry,
      relations: ['r1', 'r2'],
    }));
    expect(compare(paths, fixture, response).status).toBe(0);
    const result = admit(paths, fixture);
    expect(result.status).toBe(1);
    expect(JSON.parse(result.stderr)).toMatchObject({
      error: {
        code: 'GRAPH_ADMISSION_INVALID',
        message: 'Precedence contains a cycle whose applicability remains unresolved',
      },
    });
  });
});

test('preserves partial scope, conditions and exceptions when exposing admitted relationships', async () => {
  await projectWithReviews((paths, fixture) => {
    const response = comparisonResponse();
    const scoped = {
      ...response,
      relations: response.relations.map((relation) => ({
        ...relation,
        scope: { extent: 'partial-claim', description: 'The cache authority clause only.' },
        conditions: ['When a projection is used as a cache.'],
        exceptions: ['Authored decisions retain their authority.'],
      })),
    };
    expect(compare(paths, fixture, scoped).status).toBe(0);
    const result = admit(paths, fixture);
    expect(result.status).toBe(0);
    const input = retained(paths, result.stdout);
    const node = fixture.graph.nodes[0];
    if (!node) throw new Error('Expected a claim');
    const expanded = invoke(paths.root, ['graph', 'neighbors', node.id, '--input', input]);
    expect(JSON.parse(expanded.stdout)).toMatchObject({
      relations: [
        {
          edge: {
            scope: { extent: 'partial-claim' },
            conditions: ['When a projection is used as a cache.'],
            exceptions: ['Authored decisions retain their authority.'],
          },
        },
      ],
    });
    expect(JSON.parse(result.stdout).graph.nodes).toHaveLength(2);
  });
});

test('an admitted snapshot remains historical evidence but cannot approve changed documentary inputs', async () => {
  await projectWithReviews((paths, fixture) => {
    expect(compare(paths, fixture).status).toBe(0);
    const result = admit(paths, fixture);
    expect(result.status).toBe(0);
    const input = retained(paths, result.stdout);
    const commit = () => {
      paths.git(['add', '.']);
      paths.git([
        '-c',
        'user.name=Test',
        '-c',
        'user.email=test@example.invalid',
        'commit',
        '-qm',
        'Changed inputs',
      ]);
    };
    writeFileSync(join(paths.root, 'implementation.ts'), 'export const answer = 42;\n');
    commit();
    expect(
      JSON.parse(invoke(paths.root, ['graph', 'check', '--input', input]).stdout),
    ).toMatchObject({ accepted: true, freshness: { status: 'fresh' } });
    writeFileSync(
      join(paths.root, 'second.md'),
      readFileSync(join(paths.root, 'second.md'), 'utf8') + '\nAn additional authored rule.\n',
    );
    commit();
    const stale = invoke(paths.root, ['graph', 'check', '--input', input]);
    expect(stale.status).toBe(1);
    expect(JSON.parse(stale.stdout)).toMatchObject({
      accepted: false,
      freshness: { status: 'stale' },
    });
    const historical = invoke(paths.root, [
      'graph',
      'check',
      '--input',
      input,
      '--against',
      fixture.graph.sourceSnapshot.commit,
    ]);
    expect(historical.status).toBe(0);
    expect(JSON.parse(historical.stdout)).toMatchObject({
      accepted: true,
      freshness: { status: 'fresh' },
    });
    expect(admit(paths, fixture).status).toBe(1);
    expect(admit(paths, fixture, ['--against', fixture.graph.sourceSnapshot.commit]).status).toBe(
      1,
    );
    expect(readFileSync(input, 'utf8')).toBe(result.stdout);
  });
});

test('revalidates semantic coverage after a tampered snapshot receives a recomputed outer hash', async () => {
  await projectWithReviews((paths, fixture) => {
    expect(compare(paths, fixture).status).toBe(0);
    const result = admit(paths, fixture);
    const value = JSON.parse(result.stdout);
    value.sourceReviews[0].review.claims = [];
    const { hash: _hash, ...content } = value;
    value.hash = createHash('sha256').update(JSON.stringify(content)).digest('hex');
    const input = retained(paths, JSON.stringify(value));
    const calls = readFileSync(paths.calls, 'utf8');
    const rejected = invoke(paths.root, ['graph', 'check', '--input', input]);
    expect(rejected.status).toBe(1);
    expect(JSON.parse(rejected.stderr)).toMatchObject({ error: { code: 'INVALID_REVIEW_OUTPUT' } });
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
  });
});

test('fails a complete admission export that exceeds its requested budget', async () => {
  await projectWithReviews((paths, fixture) => {
    expect(compare(paths, fixture).status).toBe(0);
    const result = admit(paths, fixture, ['--max-bytes', '1024']);
    expect(result.status).toBe(1);
    expect(result.stdout).toBe('');
    expect(JSON.parse(result.stderr)).toMatchObject({ error: { code: 'GRAPH_ADMISSION_BUDGET' } });
  });
});

test('a single reviewed claim source needs no invented cross-source comparison', async () => {
  await projectWithReviews(
    (paths, fixture) => {
      expect(existsSync(fixture.comparisons)).toBe(false);
      const calls = readFileSync(paths.calls, 'utf8');
      const result = admit(paths, fixture);
      expect(result.stderr).toBe('');
      expect(result.status).toBe(0);
      expect(JSON.parse(result.stdout)).toMatchObject({
        accepted: true,
        comparisons: [],
        relationships: [],
        manifest: { coverage: { sources: 1, comparedPairs: 0, possiblePairs: 0 } },
      });
      expect(existsSync(fixture.comparisons)).toBe(false);
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    },
    2,
    true,
  );
});

test('an empty extraction graph cannot become accepted knowledge', async () => {
  await nativeProject((paths) => {
    writeFileSync(paths.candidate, JSON.stringify({ claims: [], relations: [] }));
    expect(
      invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
    ).toBe(0);
    const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
    expect(built.status).toBe(0);
    const input = join(dirname(paths.store), 'graph.json');
    writeFileSync(input, built.stdout);
    const result = invoke(paths.root, ['graph', 'admit', '--input', input]);
    expect(result.status).toBe(1);
    expect(JSON.parse(result.stderr)).toMatchObject({
      error: { code: 'GRAPH_ADMISSION_INVALID', message: 'An empty graph cannot be admitted' },
    });
  });
});

test.each(['sourceReviews', 'comparisons'])(
  'reapplies the retained assessment limit to embedded %s',
  async (field) => {
    await projectWithReviews((paths, fixture) => {
      expect(compare(paths, fixture).status).toBe(0);
      const result = admit(paths, fixture);
      expect(result.status).toBe(0);
      const snapshot = JSON.parse(result.stdout);
      snapshot[field][0].extraPayload = 'x'.repeat(8 * 1024 * 1024);
      const { hash: _hash, ...content } = snapshot;
      snapshot.hash = createHash('sha256').update(JSON.stringify(content)).digest('hex');
      const input = retained(paths, JSON.stringify(snapshot));
      const checked = invoke(paths.root, ['graph', 'check', '--input', input]);
      expect(checked.status).toBe(1);
      expect(JSON.parse(checked.stderr)).toMatchObject({
        error: { code: 'GRAPH_ADMISSION_INVALID' },
      });
    });
  },
);

test('checks the embedded candidate size before expanding its source evidence', async () => {
  await projectWithReviews((paths, fixture) => {
    expect(compare(paths, fixture).status).toBe(0);
    const result = admit(paths, fixture);
    const snapshot = JSON.parse(result.stdout);
    snapshot.graph.sources[0].authority.basis = 'x'.repeat(65 * 1024 * 1024);
    const { hash: _hash, ...content } = snapshot;
    snapshot.hash = createHash('sha256').update(JSON.stringify(content)).digest('hex');
    const input = retained(paths, JSON.stringify(snapshot));
    const checked = invoke(paths.root, ['graph', 'check', '--input', input]);
    expect(checked.status).toBe(1);
    expect(JSON.parse(checked.stderr)).toMatchObject({
      error: { code: 'GRAPH_INVALID', details: { maximumBytes: 64 * 1024 * 1024 } },
    });
  });
});

test('reads an admitted snapshot whose comparison plan includes inline Markdown links', async () => {
  await projectWithReviews(
    (paths, fixture) => {
      expect(compare(paths, fixture).status).toBe(0);
      const admitted = admit(paths, fixture);
      expect(admitted.status).toBe(0);
      const result = invoke(paths.root, [
        'graph',
        'check',
        '--input',
        retained(paths, admitted.stdout),
      ]);
      expect(result.stderr).toBe('');
      expect(result.status).toBe(0);
      expect(JSON.parse(result.stdout).accepted).toBe(true);
    },
    2,
    false,
    '# Cache\n\nNever treat a cache as authority.\n\nSee [the other source](second.md).\n',
  );
});
