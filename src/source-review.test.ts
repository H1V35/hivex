import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { nativeProject } from '../test/native-project.ts';
import type { GraphSnapshot } from './graph/snapshot.ts';

const cli = join(import.meta.dirname, 'cli.ts');

function invoke(root: string, args: string[]) {
  return spawnSync(process.execPath, [cli, ...args, '--root', root], {
    encoding: 'utf8',
    timeout: 15_000,
  });
}

test('reviews the complete source and every extracted claim without admitting the graph', async () => {
  await nativeProject((paths) => {
    expect(
      invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
    ).toBe(0);
    const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
    expect(built.status).toBe(0);
    const graph: GraphSnapshot = JSON.parse(built.stdout);
    const input = join(dirname(paths.store), 'graph.json');
    writeFileSync(input, built.stdout);
    const node = graph.nodes.find((item) => item.source === 'first.md');
    if (!node) throw new Error('Expected a source claim');
    const evidence = [{ quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }];
    writeFileSync(
      paths.candidate,
      JSON.stringify({
        coverage: {
          verdict: 'complete',
          reason: 'The only documented rule is represented.',
          evidence,
        },
        claims: [
          { id: node.id, verdict: 'faithful', reason: 'The prohibition is preserved.', evidence },
        ],
        relations: [],
        omissions: [],
        context: { verdict: 'sufficient', reason: 'The source defines a self-contained rule.' },
      }),
    );
    const result = invoke(paths.root, [
      'graph',
      'review',
      'first.md',
      '--input',
      input,
      '--codex',
      paths.binary,
    ]);
    expect(result.stderr).toBe('');
    expect(result.status).toBe(0);
    expect(JSON.parse(result.stdout)).toMatchObject({
      command: 'graph',
      operation: 'review',
      accepted: false,
      status: 'reviewed',
      graphHash: graph.hash,
      source: { id: 'first.md' },
      model: { name: 'gpt-5.6-luna', effort: 'max' },
      report: { outcome: 'completed', cleanup: 'confirmed' },
      review: { claims: [{ id: node.id, verdict: 'faithful' }] },
    });
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\ncalled\n');
    expect(readFileSync(input, 'utf8')).toBe(built.stdout);
  });
});

test('prepares the complete versioned source in authored order without calling a model', async () => {
  await nativeProject(
    (paths) => {
      expect(
        invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
      ).toBe(0);
      const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
      const input = join(dirname(paths.store), 'graph.json');
      writeFileSync(input, built.stdout);
      const before = readFileSync(paths.store);
      const result = invoke(paths.root, [
        'graph',
        'review',
        'first.md',
        '--input',
        input,
        '--prepare',
        '--codex',
        '/must-not-run',
      ]);
      expect(result.status).toBe(0);
      const output: { prompt: string } = JSON.parse(result.stdout);
      expect(output).toMatchObject({ accepted: false, status: 'prepared' });
      expect(output.prompt).toContain('Never treat a cache as authority.');
      expect(output.prompt).toContain('## Amendment');
      expect(output.prompt.indexOf('# Decision')).toBeLessThan(
        output.prompt.indexOf('## Amendment'),
      );
      expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
      expect(readFileSync(paths.store)).toEqual(before);
    },
    {
      source:
        '# Decision\n\nNever treat a cache as authority.\n\n## Amendment\n\nRebuild an incomplete cache from the sources.\n',
    },
  );
});

test('keeps failed native review accounting and refuses stale inputs before another model call', async () => {
  await nativeProject((paths) => {
    expect(
      invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
    ).toBe(0);
    const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
    const input = join(dirname(paths.store), 'graph.json');
    writeFileSync(input, built.stdout);
    writeFileSync(paths.scenario, 'timeout');
    const failed = invoke(paths.root, [
      'graph',
      'review',
      'first.md',
      '--input',
      input,
      '--codex',
      paths.binary,
      '--deadline-ms',
      '100',
    ]);
    expect(failed.status).toBe(1);
    expect(JSON.parse(failed.stdout)).toMatchObject({
      accepted: false,
      status: 'failed',
      review: null,
      report: {
        outcome: 'timeout',
        interruption: 'confirmed',
        cleanup: 'confirmed',
        usage: { totalTokens: 125 },
      },
    });
    const calls = readFileSync(paths.calls, 'utf8');
    writeFileSync(join(paths.root, 'second.md'), '# Changed rule\n\nUse the current source.\n');
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
    const stale = invoke(paths.root, [
      'graph',
      'review',
      'first.md',
      '--input',
      input,
      '--codex',
      paths.binary,
    ]);
    expect(stale.status).toBe(1);
    expect(JSON.parse(stale.stderr)).toMatchObject({ error: { code: 'GRAPH_STALE' } });
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    expect(readFileSync(input, 'utf8')).toBe(built.stdout);
  });
});

test('refuses incomplete assessments, invented citations and unresolved semantic findings', async () => {
  await nativeProject((paths) => {
    expect(
      invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
    ).toBe(0);
    const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
    const graph: GraphSnapshot = JSON.parse(built.stdout);
    const node = graph.nodes.find((item) => item.source === 'first.md');
    if (!node) throw new Error('Expected a claim');
    const input = join(dirname(paths.store), 'graph.json');
    writeFileSync(input, built.stdout);
    const evidence = [{ quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }];
    const base = {
      coverage: { verdict: 'complete', reason: 'All source knowledge is represented.', evidence },
      claims: [{ id: node.id, verdict: 'faithful', reason: 'The rule matches.', evidence }],
      relations: [],
      omissions: [],
      context: { verdict: 'sufficient', reason: 'The source is self-contained.' },
    };
    const invalid = [
      { ...base, claims: [] },
      { ...base, claims: [...base.claims, ...base.claims] },
      {
        ...base,
        coverage: {
          ...base.coverage,
          evidence: [{ quote: 'Absent evidence.', lineStart: 3, lineEnd: 3 }],
        },
      },
      { ...base, coverage: { ...base.coverage, verdict: 'no-knowledge' } },
    ];
    for (const response of invalid) {
      writeFileSync(paths.candidate, JSON.stringify(response));
      const result = invoke(paths.root, [
        'graph',
        'review',
        'first.md',
        '--input',
        input,
        '--codex',
        paths.binary,
      ]);
      expect(result.status).toBe(1);
      expect(JSON.parse(result.stdout)).toMatchObject({
        accepted: false,
        status: 'failed',
        review: null,
        report: { code: 'INVALID_REVIEW_OUTPUT', usage: { totalTokens: 150 } },
      });
    }
    const unresolved = [
      {
        ...base,
        claims: [
          {
            ...base.claims[0],
            verdict: 'distorted',
            reason: 'The candidate reverses the documented prohibition.',
          },
        ],
      },
      {
        ...base,
        claims: [
          { ...base.claims[0], verdict: 'unresolved', reason: 'Applicability is ambiguous.' },
        ],
      },
      {
        ...base,
        context: { verdict: 'insufficient', reason: 'A referenced amendment is missing.' },
      },
      {
        ...base,
        coverage: { ...base.coverage, verdict: 'incomplete' },
        omissions: [{ text: 'A condition is absent from the extraction.', evidence }],
      },
    ];
    for (const response of unresolved) {
      writeFileSync(paths.candidate, JSON.stringify(response));
      const result = invoke(paths.root, [
        'graph',
        'review',
        'first.md',
        '--input',
        input,
        '--codex',
        paths.binary,
      ]);
      expect(result.status).toBe(1);
      expect(JSON.parse(result.stdout)).toMatchObject({
        accepted: false,
        status: 'failed',
        review: response,
        report: { outcome: 'completed' },
      });
    }
    expect(readFileSync(input, 'utf8')).toBe(built.stdout);
  });
});
