import { expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { nativeProject } from './native-project.ts';
import type { GraphSnapshot } from '../src/graph/snapshot.ts';
const cli = join(import.meta.dirname, '../src/cli.ts');
export function invoke(root: string, args: string[]) {
  const separator = args.indexOf('--');
  const forwarded =
    separator < 0
      ? [...args, '--root', root]
      : [...args.slice(0, separator), '--root', root, ...args.slice(separator)];
  return spawnSync(process.execPath, [cli, ...forwarded], {
    encoding: 'utf8',
    timeout: 15000,
    maxBuffer: 4 * 1024 * 1024,
  });
}

export type Paths = Parameters<Parameters<typeof nativeProject>[0]>[0];
export type Fixture = {
  input: string;
  reviews: string;
  comparisons: string;
  graph: GraphSnapshot;
  built: string;
};
export async function projectWithReviews(
  run: (paths: Paths, fixture: Fixture) => void | Promise<void>,
  reviewedSources = 2,
  singleSource = false,
  source?: string,
) {
  await nativeProject(
    async (paths) => {
      if (singleSource) {
        paths.git(['rm', 'second.md']);
        paths.git([
          '-c',
          'user.name=Test',
          '-c',
          'user.email=test@example.invalid',
          'commit',
          '-qm',
          'One source',
        ]);
      }
      expect(
        invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
      ).toBe(0);
      const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
      expect(built.status).toBe(0);
      const graph: GraphSnapshot = JSON.parse(built.stdout);
      const input = join(dirname(paths.store), 'graph.json');
      const reviews = join(dirname(paths.store), 'reviews.sqlite');
      const comparisons = join(dirname(paths.store), 'comparisons.sqlite');
      writeFileSync(input, built.stdout);
      expect(
        invoke(paths.root, [
          'graph',
          'review',
          '--all',
          '--input',
          input,
          '--store',
          reviews,
          '--max-units',
          '0',
        ]).status,
      ).toBe(0);
      const evidence = [{ quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }];
      for (const source of graph.sources.slice(0, reviewedSources)) {
        const node = graph.nodes.find((node) => node.source === source.id);
        if (!node) throw new Error('Expected source claim');
        writeFileSync(
          paths.candidate,
          JSON.stringify({
            coverage: {
              verdict: 'complete',
              reason: 'All source knowledge is preserved.',
              evidence,
            },
            claims: [
              {
                id: 'c1',
                verdict: 'faithful',
                reason: 'The complete prohibition is preserved.',
                evidence,
              },
            ],
            relations: [],
            omissions: [],
            context: { verdict: 'sufficient', reason: 'The rule is self-contained.' },
          }),
        );
        expect(
          invoke(paths.root, [
            'graph',
            'review',
            '--all',
            '--input',
            input,
            '--store',
            reviews,
            '--codex',
            paths.binary,
            '--max-units',
            '1',
          ]).status,
        ).toBe(0);
      }
      await run(paths, { input, reviews, comparisons, graph, built: built.stdout });
    },
    { source },
  );
}

export function comparisonResponse() {
  const evidence = [{ quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }];
  return {
    assessments: ['s1', 's2'].map((source) => ({
      id: source + ':c1',
      verdict: 'reviewed',
      reason: 'Both sources preserve the same rule.',
      relations: ['r1'],
      evidence: evidence.map((entry) => ({ ...entry, source })),
    })),
    relations: [
      {
        id: 'r1',
        from: 's1:c1',
        to: 's2:c1',
        type: 'equivalent',
        scope: { extent: 'whole-claim', description: 'The complete cache prohibition.' },
        conditions: [],
        exceptions: [],
        evidence: ['s1', 's2'].flatMap((source) => evidence.map((entry) => ({ ...entry, source }))),
      },
    ],
    coverage: { complete: true, reason: 'Both claims and their relationship are covered.' },
    context: { verdict: 'sufficient', reason: 'Both complete sources are present.' },
  };
}
export function compare(paths: Paths, fixture: Fixture, response: unknown = comparisonResponse()) {
  writeFileSync(paths.candidate, JSON.stringify(response));
  return invoke(paths.root, [
    'graph',
    'compare',
    '--all',
    '--input',
    fixture.input,
    '--store',
    fixture.comparisons,
    '--codex',
    paths.binary,
    '--neighbors',
    '1',
  ]);
}
export function admit(paths: Paths, fixture: Fixture, extra: string[] = []) {
  return invoke(paths.root, [
    'graph',
    'admit',
    '--input',
    fixture.input,
    '--reviews',
    fixture.reviews,
    '--comparisons',
    fixture.comparisons,
    '--neighbors',
    '1',
    '--export',
    ...extra,
  ]);
}
export function retained(paths: Paths, output: string) {
  const input = join(dirname(paths.store), 'admitted.json');
  writeFileSync(input, output);
  return input;
}
