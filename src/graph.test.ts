import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { nativeProject, nativeSource } from '../test/native-project.ts';
import type { GraphContent, GraphSnapshot } from './graph/snapshot.ts';

const cli = join(import.meta.dirname, 'cli.ts');

function invoke(root: string, args: string[]) {
  return spawnSync(process.execPath, [cli, ...args, '--root', root], {
    encoding: 'utf8',
    timeout: 15_000,
  });
}

function altered(graph: GraphSnapshot, change: (content: GraphContent) => void) {
  const { hash: _hash, ...content } = structuredClone(graph);
  change(content);
  return { ...content, hash: createHash('sha256').update(JSON.stringify(content)).digest('hex') };
}

test('assembles a reproducible source-bound graph from a complete cohort without accepting it', async () => {
  await nativeProject((paths) => {
    const ingest = spawnSync(
      process.execPath,
      [cli, 'ingest', '--root', paths.root, '--store', paths.store, '--codex', paths.binary],
      { encoding: 'utf8', timeout: 15_000 },
    );
    expect(ingest.status).toBe(0);
    const before = readFileSync(paths.store);
    const args = [cli, 'graph', 'build', '--root', paths.root, '--store', paths.store, '--export'];
    const built = spawnSync(process.execPath, args, { encoding: 'utf8', timeout: 5000 });
    expect(built.stderr).toBe('');
    expect(built.status).toBe(0);
    const graph: GraphSnapshot = JSON.parse(built.stdout);
    expect(graph).toMatchObject({ format: 'hivex-graph-candidate', version: 1, accepted: false });
    expect(graph.hash).toMatch(/^[a-f0-9]{64}$/);
    expect(graph.sources.map((item: { id: string }) => item.id)).toEqual(['first.md', 'second.md']);
    expect(graph.nodes).toHaveLength(2);
    expect(new Set(graph.nodes.map((node: { id: string }) => node.id)).size).toBe(2);
    for (const node of graph.nodes) {
      expect(node.statement).toMatchObject({
        kind: 'constraint',
        evidence: [{ quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }],
      });
      expect(['first.md', 'second.md']).toContain(node.source);
    }
    expect(graph.edges).toEqual([]);
    const repeated = spawnSync(process.execPath, args, { encoding: 'utf8', timeout: 5000 });
    expect(repeated.status).toBe(0);
    expect(repeated.stdout).toBe(built.stdout);
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
    expect(readFileSync(paths.store)).toEqual(before);
  });
});

test('keeps source-local relations separate and retains aliases for identical statements', async () => {
  await nativeProject((paths) => {
    const evidence = [{ quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }];
    const claim = {
      text: 'Caches are not authority.',
      kind: 'constraint',
      conditions: [],
      exceptions: [],
      evidence,
    };
    writeFileSync(
      paths.candidate,
      JSON.stringify({
        claims: [
          { id: 'c1', ...claim },
          { id: 'c2', ...claim, text: 'Cached content alone cannot establish a project rule.' },
          { id: 'c3', ...claim },
        ],
        relations: [{ from: 'c3', to: 'c2', type: 'supports', evidence }],
      }),
    );
    expect(
      invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
    ).toBe(0);
    const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
    expect(built.status).toBe(0);
    const graph: GraphSnapshot = JSON.parse(built.stdout);
    expect(graph.nodes).toHaveLength(4);
    expect(graph.edges).toHaveLength(2);
    const input = join(dirname(paths.store), 'relations.json');
    writeFileSync(input, built.stdout);
    for (const edge of graph.edges) {
      const from = graph.nodes.find((node: { id: string }) => node.id === edge.from);
      const to = graph.nodes.find((node: { id: string }) => node.id === edge.to);
      expect(from).toMatchObject({ source: edge.source, localIds: ['c1', 'c3'] });
      expect(to).toMatchObject({ source: edge.source, localIds: ['c2'] });
      expect(edge).toMatchObject({ type: 'supports', evidence });
      const neighbors = invoke(paths.root, ['graph', 'neighbors', edge.from, '--input', input]);
      expect(neighbors.status).toBe(0);
      expect(JSON.parse(neighbors.stdout)).toMatchObject({
        accepted: false,
        relations: [{ edge, neighbor: { id: edge.to, source: edge.source } }],
        continuation: null,
      });
    }
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
  });
});

test('refuses partial cohorts without finishing them implicitly or modifying their store', async () => {
  await nativeProject((paths) => {
    expect(
      invoke(paths.root, [
        'ingest',
        '--store',
        paths.store,
        '--codex',
        paths.binary,
        '--max-units',
        '1',
      ]).status,
    ).toBe(0);
    const before = readFileSync(paths.store);
    const built = invoke(paths.root, ['graph', 'build', '--store', paths.store]);
    expect(built.status).toBe(1);
    expect(JSON.parse(built.stderr)).toMatchObject({ error: { code: 'INGESTION_NOT_READY' } });
    expect(readFileSync(paths.store)).toEqual(before);
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\n');
  });
});

test('checks documentation freshness independently of unrelated code commits', async () => {
  await nativeProject((paths) => {
    expect(
      invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
    ).toBe(0);
    const exported = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
    expect(exported.status).toBe(0);
    const graph: GraphSnapshot = JSON.parse(exported.stdout);
    const input = join(dirname(paths.store), 'graph.json');
    writeFileSync(input, exported.stdout);
    const initial = invoke(paths.root, ['graph', 'check', '--input', input]);
    expect(initial.status).toBe(0);
    expect(JSON.parse(initial.stdout)).toMatchObject({
      accepted: false,
      freshness: { status: 'fresh' },
    });
    writeFileSync(join(paths.root, 'code.ts'), 'export const value = 1;\n');
    paths.git(['add', 'code.ts']);
    paths.git([
      '-c',
      'user.name=Test',
      '-c',
      'user.email=test@example.invalid',
      'commit',
      '-qm',
      'code change',
    ]);
    const codeOnly = invoke(paths.root, ['graph', 'check', '--input', input]);
    expect(codeOnly.status).toBe(0);
    expect(JSON.parse(codeOnly.stdout)).toMatchObject({ freshness: { status: 'fresh' } });
    expect(JSON.parse(codeOnly.stdout).freshness.comparedCommit).not.toBe(
      graph.sourceSnapshot.commit,
    );
    writeFileSync(join(paths.root, 'second.md'), nativeSource + '\nA new documented rule.\n');
    paths.git(['add', 'second.md']);
    paths.git([
      '-c',
      'user.name=Test',
      '-c',
      'user.email=test@example.invalid',
      'commit',
      '-qm',
      'source change',
    ]);
    const stale = invoke(paths.root, ['graph', 'check', '--input', input]);
    expect(stale.status).toBe(1);
    expect(JSON.parse(stale.stdout)).toMatchObject({
      status: 'failed',
      accepted: false,
      freshness: { status: 'stale' },
    });
    const original = invoke(paths.root, [
      'graph',
      'check',
      '--input',
      input,
      '--against',
      graph.sourceSnapshot.commit,
    ]);
    expect(original.status).toBe(0);
    expect(JSON.parse(original.stdout)).toMatchObject({
      freshness: { status: 'fresh', comparedCommit: graph.sourceSnapshot.commit },
    });
    expect(readFileSync(input, 'utf8')).toBe(exported.stdout);
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
  });
});

test('rejects missing claims even when the outer snapshot checksum is recomputed', async () => {
  await nativeProject((paths) => {
    expect(
      invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
    ).toBe(0);
    const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
    const graph: GraphSnapshot = JSON.parse(built.stdout);
    const input = join(dirname(paths.store), 'changed.json');
    writeFileSync(
      input,
      JSON.stringify(
        altered(graph, (content) => {
          content.nodes.pop();
        }),
      ),
    );
    const checked = invoke(paths.root, ['graph', 'check', '--input', input]);
    expect(checked.status).toBe(1);
    expect(JSON.parse(checked.stderr)).toMatchObject({ error: { code: 'GRAPH_INVALID' } });
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
  });
});

test('rejects a reported model that conflicts with the source processing contract', async () => {
  await nativeProject((paths) => {
    expect(
      invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
    ).toBe(0);
    const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
    const graph: GraphSnapshot = JSON.parse(built.stdout);
    const input = join(dirname(paths.store), 'wrong-profile.json');
    writeFileSync(
      input,
      JSON.stringify(
        altered(graph, (content) => {
          const first = content.sources[0];
          if (!first) throw new Error('Expected a source');
          first.extraction.reportedProfile.model = 'different-model';
        }),
      ),
    );
    const checked = invoke(paths.root, ['graph', 'check', '--input', input]);
    expect(checked.status).toBe(1);
    expect(JSON.parse(checked.stderr)).toMatchObject({ error: { code: 'GRAPH_INVALID' } });
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
  });
});

test('finds and opens complete graph evidence without losing conditions or exceptions', async () => {
  await nativeProject((paths) => {
    const evidence = [{ quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }];
    writeFileSync(
      paths.candidate,
      JSON.stringify({
        claims: [
          {
            id: 'c1',
            text: 'Caches are not authority.',
            kind: 'constraint',
            conditions: ['When consulting derived knowledge'],
            exceptions: ['Source evidence must still be opened'],
            evidence,
          },
        ],
        relations: [],
      }),
    );
    expect(
      invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
    ).toBe(0);
    const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
    expect(built.status).toBe(0);
    const input = join(dirname(paths.store), 'query.json');
    writeFileSync(input, built.stdout);
    const found = invoke(paths.root, [
      'graph',
      'search',
      'derived knowledge',
      '--input',
      input,
      '--limit',
      '1',
    ]);
    expect(found.status).toBe(0);
    const result: { results: { id: string }[]; truncated: boolean } = JSON.parse(found.stdout);
    expect(result.results).toHaveLength(1);
    expect(result.truncated).toBe(true);
    const id = result.results[0]?.id;
    if (!id) throw new Error('Expected a graph claim');
    const opened = invoke(paths.root, ['graph', 'read', id, '--input', input]);
    expect(opened.status).toBe(0);
    expect(JSON.parse(opened.stdout)).toMatchObject({
      accepted: false,
      node: {
        id,
        statement: {
          conditions: ['When consulting derived knowledge'],
          exceptions: ['Source evidence must still be opened'],
          evidence,
        },
      },
    });
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
    expect(readFileSync(input, 'utf8')).toBe(built.stdout);
  });
});

test('paginates complete relations with a cursor bound to the graph and seed claim', async () => {
  await nativeProject((paths) => {
    const evidence = [{ quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }];
    writeFileSync(
      paths.candidate,
      JSON.stringify({
        claims: ['One cache rule', 'A related cache rule', 'Another cache rule'].map(
          (text, index) => ({
            id: `c${index + 1}`,
            text,
            kind: 'constraint',
            conditions: [],
            exceptions: [],
            evidence,
          }),
        ),
        relations: [
          { from: 'c1', to: 'c2', type: 'supports', evidence },
          { from: 'c1', to: 'c3', type: 'requires', evidence },
        ],
      }),
    );
    expect(
      invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
    ).toBe(0);
    const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
    const graph: GraphSnapshot = JSON.parse(built.stdout);
    const input = join(dirname(paths.store), 'neighbors.json');
    writeFileSync(input, built.stdout);
    const seed = graph.nodes.find(
      (node) => node.source === 'first.md' && node.localIds.includes('c1'),
    );
    if (!seed) throw new Error('Expected the seed claim');
    const first = invoke(paths.root, [
      'graph',
      'neighbors',
      seed.id,
      '--input',
      input,
      '--limit',
      '1',
    ]);
    expect(first.status).toBe(0);
    const page: { relations: { edge: { id: string } }[]; continuation: string } = JSON.parse(
      first.stdout,
    );
    expect(page.relations).toHaveLength(1);
    expect(page.continuation).toBeString();
    const next = invoke(paths.root, [
      'graph',
      'neighbors',
      seed.id,
      '--input',
      input,
      '--limit',
      '1',
      '--cursor',
      page.continuation,
    ]);
    expect(next.status).toBe(0);
    const second: { relations: { edge: { id: string } }[]; continuation: null } = JSON.parse(
      next.stdout,
    );
    expect(second.relations).toHaveLength(1);
    expect(second.continuation).toBeNull();
    expect(second.relations[0]?.edge.id).not.toBe(page.relations[0]?.edge.id);
    const other = graph.nodes.find(
      (node) => node.source === 'second.md' && node.localIds.includes('c1'),
    );
    if (!other) throw new Error('Expected another source claim');
    const mismatched = invoke(paths.root, [
      'graph',
      'neighbors',
      other.id,
      '--input',
      input,
      '--cursor',
      page.continuation,
    ]);
    expect(mismatched.status).toBe(1);
    expect(JSON.parse(mismatched.stderr)).toMatchObject({ error: { code: 'INVALID_CURSOR' } });
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
  });
});

test('refuses to shorten a claim when its complete evidence exceeds the requested budget', async () => {
  const quote = 'A complete source clause must remain available. '.repeat(50).trim();
  await nativeProject(
    (paths) => {
      writeFileSync(
        paths.candidate,
        JSON.stringify({
          claims: [
            {
              id: 'c1',
              text: 'Keep complete evidence.',
              kind: 'constraint',
              conditions: [],
              exceptions: [],
              evidence: [{ quote, lineStart: 5, lineEnd: 5 }],
            },
          ],
          relations: [],
        }),
      );
      expect(
        invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
      ).toBe(0);
      const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
      const graph: GraphSnapshot = JSON.parse(built.stdout);
      const input = join(dirname(paths.store), 'large-claim.json');
      writeFileSync(input, built.stdout);
      const id = graph.nodes[0]?.id;
      if (!id) throw new Error('Expected a claim');
      const limited = invoke(paths.root, [
        'graph',
        'read',
        id,
        '--input',
        input,
        '--max-bytes',
        '1024',
      ]);
      expect(limited.status).toBe(1);
      expect(limited.stdout).toBe('');
      expect(JSON.parse(limited.stderr)).toMatchObject({ error: { code: 'GRAPH_OUTPUT_BUDGET' } });
      const complete = invoke(paths.root, [
        'graph',
        'read',
        id,
        '--input',
        input,
        '--max-bytes',
        '16384',
      ]);
      expect(complete.status).toBe(0);
      expect(JSON.parse(complete.stdout)).toMatchObject({
        node: { statement: { evidence: [{ quote }] } },
      });
    },
    { source: nativeSource + '\n' + quote + '\n' },
  );
});
