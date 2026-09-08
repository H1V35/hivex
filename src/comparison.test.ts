import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { nativeProject, nativeSource } from '../test/native-project.ts';
import type { GraphSnapshot } from './graph/snapshot.ts';

const cli = join(import.meta.dirname, 'cli.ts');
function invoke(root: string, args: string[]) {
  return spawnSync(process.execPath, [cli, ...args, '--root', root], {
    encoding: 'utf8',
    timeout: 15000,
  });
}

test('compares two complete sources and retains both sides of a candidate relationship', async () => {
  await nativeProject((paths) => {
    expect(
      invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
    ).toBe(0);
    const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
    expect(built.status).toBe(0);
    const graph: GraphSnapshot = JSON.parse(built.stdout);
    const from = graph.nodes.find((node) => node.source === 'first.md');
    const to = graph.nodes.find((node) => node.source === 'second.md');
    if (!from || !to) throw new Error('Expected one claim per source');
    const input = join(dirname(paths.store), 'graph.json');
    writeFileSync(input, built.stdout);
    const prepared = invoke(paths.root, [
      'graph',
      'compare',
      'first.md',
      'second.md',
      '--input',
      input,
      '--prepare',
      '--codex',
      '/must-not-run',
    ]);
    expect(prepared.status).toBe(0);
    const packet: { prompt: string; contract: { promptHash: string } } = JSON.parse(
      prepared.stdout,
    );
    expect(packet.prompt).toContain('s1:c1');
    expect(packet.prompt).not.toContain(from.id);
    expect(packet.prompt).not.toContain(graph.hash);
    const reversed = invoke(paths.root, [
      'graph',
      'compare',
      'second.md',
      'first.md',
      '--input',
      input,
      '--prepare',
    ]);
    expect(reversed.status).toBe(0);
    expect(JSON.parse(reversed.stdout).contract.promptHash).toBe(packet.contract.promptHash);
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
    const evidence = ['first.md', 'second.md'].map((source) => ({
      source,
      quote: 'Never treat a cache as authority.',
      lineStart: 3,
      lineEnd: 3,
    }));
    writeFileSync(
      paths.candidate,
      JSON.stringify({
        assessments: ['s1', 's2'].map((source) => ({
          id: `${source}:c1`,
          verdict: 'reviewed',
          reason: 'The other source states the same prohibition.',
          relations: ['r1'],
          evidence: [
            { source, quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 },
          ],
        })),
        relations: [
          {
            id: 'r1',
            from: 's1:c1',
            to: 's2:c1',
            type: 'equivalent',
            scope: {
              extent: 'whole-claim',
              description: 'Both claims prohibit treating caches as authority.',
            },
            conditions: [],
            exceptions: [],
            evidence: evidence.map((entry, index) => ({ ...entry, source: `s${index + 1}` })),
          },
        ],
        coverage: {
          complete: true,
          reason: 'Both complete sources and every claim were assessed.',
        },
        context: { verdict: 'sufficient', reason: 'Both sources state a self-contained rule.' },
      }),
    );
    const result = invoke(paths.root, [
      'graph',
      'compare',
      'first.md',
      'second.md',
      '--input',
      input,
      '--codex',
      paths.binary,
    ]);
    expect(result.stderr).toBe('');
    expect(result.status).toBe(0);
    expect(JSON.parse(result.stdout)).toMatchObject({
      command: 'graph',
      operation: 'compare',
      accepted: false,
      status: 'reviewed',
      graphHash: graph.hash,
      comparison: { relations: [{ type: 'equivalent', from: from.id, to: to.id, evidence }] },
      report: { outcome: 'completed', cleanup: 'confirmed' },
    });
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\ncalled\n');
    expect(readFileSync(input, 'utf8')).toBe(built.stdout);
  });
});

test('preserves a conditional partial exception without superseding the whole rule or document', async () => {
  const general = 'All modal forms save when closed.';
  const exception =
    'For an unchanged profile form, closing does not save; this is an exception to the modal close rule.';
  await nativeProject(
    (paths) => {
      writeFileSync(join(paths.root, 'second.md'), nativeSource + '\n' + exception + '\n');
      paths.git(['add', 'second.md']);
      paths.git([
        '-c',
        'user.name=Test',
        '-c',
        'user.email=test@example.invalid',
        'commit',
        '-qm',
        'documented exception',
      ]);
      const quote = (source: string, text: string, line: number) => ({
        source,
        quote: text,
        lineStart: line,
        lineEnd: line,
      });
      for (const text of [general, exception]) {
        writeFileSync(
          paths.candidate,
          JSON.stringify({
            claims: [
              {
                id: 'c1',
                text: 'Caches are not authority.',
                kind: 'constraint',
                conditions: [],
                exceptions: [],
                evidence: [
                  { quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 },
                ],
              },
              {
                id: 'c2',
                text,
                kind: 'constraint',
                conditions: [],
                exceptions: [],
                evidence: [{ quote: text, lineStart: 5, lineEnd: 5 }],
              },
            ],
            relations: [],
          }),
        );
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
      }
      const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
      expect(built.status).toBe(0);
      const graph: GraphSnapshot = JSON.parse(built.stdout);
      const input = join(dirname(paths.store), 'graph.json');
      writeFileSync(input, built.stdout);
      const condition = 'The profile form is unchanged.';
      const assessments = ['s1', 's2'].flatMap((source) => [
        {
          id: `${source}:c1`,
          verdict: 'reviewed',
          reason: 'Both preserve the cache rule.',
          relations: ['r1'],
          evidence: [quote(source, 'Never treat a cache as authority.', 3)],
        },
        {
          id: `${source}:c2`,
          verdict: 'reviewed',
          reason: 'The explicit exception narrows the general rule.',
          relations: ['r2'],
          evidence: [quote(source, source === 's1' ? general : exception, 5)],
        },
      ]);
      writeFileSync(
        paths.candidate,
        JSON.stringify({
          assessments,
          relations: [
            {
              id: 'r1',
              from: 's1:c1',
              to: 's2:c1',
              type: 'equivalent',
              scope: { extent: 'whole-claim', description: 'The cache prohibition.' },
              conditions: [],
              exceptions: [],
              evidence: ['s1', 's2'].map((source) =>
                quote(source, 'Never treat a cache as authority.', 3),
              ),
            },
            {
              id: 'r2',
              from: 's2:c2',
              to: 's1:c2',
              type: 'exception-to',
              scope: {
                extent: 'partial-claim',
                description: 'Only closing unchanged profile forms.',
              },
              conditions: [condition],
              exceptions: [],
              evidence: [quote('s2', exception, 5), quote('s1', general, 5)],
            },
          ],
          coverage: { complete: true, reason: 'All four claims are assessed.' },
          context: {
            verdict: 'sufficient',
            reason: 'The exception explicitly identifies its target.',
          },
        }),
      );
      const result = invoke(paths.root, [
        'graph',
        'compare',
        'first.md',
        'second.md',
        '--input',
        input,
        '--codex',
        paths.binary,
      ]);
      expect(result.stderr).toBe('');
      expect(result.status).toBe(0);
      const output = JSON.parse(result.stdout);
      expect(output).toMatchObject({
        accepted: false,
        status: 'reviewed',
        comparison: {
          relations: [
            { type: 'equivalent' },
            {
              type: 'exception-to',
              scope: { extent: 'partial-claim' },
              conditions: [condition],
              from: graph.nodes.find(
                (node) => node.source === 'second.md' && node.localIds.includes('c2'),
              )?.id,
              to: graph.nodes.find(
                (node) => node.source === 'first.md' && node.localIds.includes('c2'),
              )?.id,
            },
          ],
        },
      });
      expect(readFileSync(input, 'utf8')).toBe(built.stdout);
    },
    { source: nativeSource + '\n' + general + '\n' },
  );
});

test('keeps reported contradictions and incomplete comparisons from becoming approval', async () => {
  await nativeProject((paths) => {
    expect(
      invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
    ).toBe(0);
    const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
    const input = join(dirname(paths.store), 'graph.json');
    writeFileSync(input, built.stdout);
    const quote = 'Never treat a cache as authority.';
    const evidence = ['s1', 's2'].map((source) => ({ source, quote, lineStart: 3, lineEnd: 3 }));
    const base = {
      assessments: ['s1', 's2'].map((source) => ({
        id: `${source}:c1`,
        verdict: 'reviewed',
        reason: 'Claim assessed.',
        relations: ['r1'],
        evidence: evidence.filter((entry) => entry.source === source),
      })),
      relations: [
        {
          id: 'r1',
          from: 's1:c1',
          to: 's2:c1',
          type: 'equivalent',
          scope: { extent: 'whole-claim', description: 'Both complete claims.' },
          conditions: [],
          exceptions: [],
          evidence,
        },
      ],
      coverage: { complete: true, reason: 'Complete pair assessed.' },
      context: { verdict: 'sufficient', reason: 'Self-contained sources.' },
    };
    const cases = [
      { ...base, relations: [{ ...base.relations[0], type: 'contradicts' }] },
      {
        ...base,
        coverage: { complete: false, reason: 'The schema limit prevents complete coverage.' },
      },
      {
        ...base,
        context: { verdict: 'insufficient', reason: 'A referenced amendment is not supplied.' },
      },
      {
        ...base,
        relations: [
          {
            ...base.relations[0],
            scope: { extent: 'unspecified', description: 'The affected scope is ambiguous.' },
          },
        ],
      },
    ];
    for (const value of cases) {
      writeFileSync(paths.candidate, JSON.stringify(value));
      const result = invoke(paths.root, [
        'graph',
        'compare',
        'first.md',
        'second.md',
        '--input',
        input,
        '--codex',
        paths.binary,
      ]);
      expect(result.status).toBe(1);
      expect(JSON.parse(result.stdout)).toMatchObject({
        accepted: false,
        status: 'failed',
        report: { outcome: 'completed' },
        comparison: { coverage: value.coverage, context: value.context },
      });
    }
    for (const value of [
      { ...base, assessments: [] },
      { ...base, relations: [{ ...base.relations[0], to: 's2:c99' }] },
      { ...base, relations: [{ ...base.relations[0], evidence: [evidence[0], evidence[0]] }] },
      {
        ...base,
        relations: [
          {
            ...base.relations[0],
            evidence: evidence.map((entry) => ({ ...entry, quote: 'Invented source evidence.' })),
          },
        ],
      },
    ]) {
      writeFileSync(paths.candidate, JSON.stringify(value));
      const result = invoke(paths.root, [
        'graph',
        'compare',
        'first.md',
        'second.md',
        '--input',
        input,
        '--codex',
        paths.binary,
      ]);
      expect(result.status).toBe(1);
      expect(JSON.parse(result.stdout)).toMatchObject({
        accepted: false,
        status: 'failed',
        comparison: null,
        report: {
          outcome: 'invalid-output',
          code: 'INVALID_COMPARISON_OUTPUT',
          usage: { totalTokens: 150 },
        },
      });
    }
    expect(readFileSync(input, 'utf8')).toBe(built.stdout);
  });
});
