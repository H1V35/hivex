import { expect, test } from 'bun:test';
import { readFileSync, writeFileSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { nativeProject } from '../test/native-project.ts';
import { invoke } from '../test/reviewed-project.ts';
import type { GraphSnapshot } from './graph/snapshot.ts';
import type { Paths } from '../test/reviewed-project.ts';

const source = '# Cache\n\nNever treat a cache as authority.\nPreserve the original source.\n';
const evidence = [{ quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }];
const omitted = [{ quote: 'Preserve the original source.', lineStart: 4, lineEnd: 4 }];

test('revises only the candidate with adverse fidelity and preserves prior evidence and usage', async () => {
  await nativeProject(
    (paths) => {
      const { ingest, inspect, original, review, args } = prepare(paths);
      const untouched = inspect('second.md').stdout;
      const prepared = invoke(paths.root, [...args, '--prepare']);
      expect(prepared.stderr).toBe('');
      expect(prepared.status).toBe(0);
      expect(JSON.parse(prepared.stdout).prompt).toContain('Preserve the original source.');
      expect(readFileSync(paths.calls, 'utf8')).toBe('called\n'.repeat(3));
      const candidate = {
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
      writeFileSync(paths.candidate, JSON.stringify(candidate));
      const revised = invoke(paths.root, args);
      expect(revised.stderr).toBe('');
      expect(revised.status).toBe(0);
      expect(JSON.parse(revised.stdout)).toMatchObject({
        completed: 2,
        processed: 1,
        attempts: { recorded: 3, knownTotalTokens: 450 },
      });
      const result = JSON.parse(inspect('first.md').stdout).result;
      expect(result.candidate).toEqual(candidate);
      expect(result.attempts[0]).toEqual(original.attempts[0]);
      expect(result.revisions).toMatchObject([
        { afterAttempt: 1, candidate: original.candidate, feedback: review },
      ]);
      expect(result.candidateAttempt).toBe(2);
      expect(inspect('second.md').stdout).toBe(untouched);
      expect(invoke(paths.root, ingest).status).toBe(0);
      expect(readFileSync(paths.calls, 'utf8')).toBe('called\n'.repeat(4));
      const rebuilt = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
      expect(rebuilt.status).toBe(0);
      expect(JSON.parse(rebuilt.stdout).nodes).toHaveLength(3);
      const staleFeedback = invoke(paths.root, args);
      expect(staleFeedback.status).toBe(1);
      expect(readFileSync(paths.calls, 'utf8')).toBe('called\n'.repeat(4));
    },
    { source },
  );
});

function prepare(paths: Paths) {
  const ingest = ['ingest', '--store', paths.store, '--codex', paths.binary];
  expect(invoke(paths.root, ingest).status).toBe(0);
  const inspect = (id: string) =>
    invoke(paths.root, ['ingest', '--store', paths.store, '--show', id, '--max-bytes', '1048576']);
  const original = JSON.parse(inspect('first.md').stdout).result;
  const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
  const graph: GraphSnapshot = JSON.parse(built.stdout);
  const input = join(dirname(paths.store), 'graph.json');
  const feedback = join(dirname(paths.store), 'feedback.json');
  writeFileSync(input, built.stdout);
  writeFileSync(
    paths.candidate,
    JSON.stringify({
      coverage: { verdict: 'incomplete', reason: 'The retention rule was omitted.', evidence },
      claims: graph.nodes
        .filter((node) => node.source === 'first.md')
        .map((node) => ({
          id: node.id,
          verdict: 'faithful',
          reason: 'The prohibition is preserved.',
          evidence,
        })),
      relations: [],
      omissions: [{ text: 'Preserve the original source.', evidence: omitted }],
      context: { verdict: 'sufficient', reason: 'Both rules are present in the supplied source.' },
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
  const review = JSON.parse(reviewed.stdout);
  expect(review.report.outcome).toBe('completed');
  writeFileSync(feedback, reviewed.stdout);
  const args = [
    ...ingest,
    '--revise',
    'first.md',
    '--input',
    input,
    '--feedback',
    feedback,
    '--attempts',
    '1',
  ];

  return { ingest, inspect, original, graph, input, feedback, review, args };
}

test('retains the revision request and earlier candidate across a safe failed invocation and retry', async () => {
  await nativeProject(
    (paths) => {
      const { ingest, inspect, original, args } = prepare(paths);
      writeFileSync(paths.scenario, 'changed-effort');
      const failed = invoke(paths.root, args);
      expect(failed.status).toBe(1);
      expect(JSON.parse(failed.stdout)).toMatchObject({
        failed: 1,
        attempts: { recorded: 3, unknownUsage: 1 },
      });
      const checkpointed = JSON.parse(inspect('first.md').stdout).result;
      expect(checkpointed.revisions[0].candidate).toEqual(original.candidate);
      expect(checkpointed.candidate).toBeNull();
      rmSync(paths.scenario);
      writeFileSync(
        paths.candidate,
        JSON.stringify({
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
        }),
      );
      const retried = invoke(paths.root, [
        ...ingest,
        '--retry-failed',
        'first.md',
        '--attempts',
        '2',
      ]);
      expect(retried.stderr).toBe('');
      expect(retried.status).toBe(0);
      const result = JSON.parse(inspect('first.md').stdout).result;
      expect(result.revisions).toEqual(checkpointed.revisions);
      expect(result.candidateAttempt).toBe(3);
      expect(result.attempts[2].promptHash).toBe(result.revisions[0].promptHash);
      expect(result.attempts[1]).toEqual(checkpointed.attempts[1]);
      expect(JSON.parse(retried.stdout)).toMatchObject({
        completed: 2,
        attempts: { recorded: 4, knownTotalTokens: 450, unknownUsage: 1 },
      });
      expect(readFileSync(paths.calls, 'utf8')).toBe('called\n'.repeat(4));
    },
    { source },
  );
});

test('rejects insufficient, altered and non-adverse feedback without changing state or calling the model', async () => {
  await nativeProject(
    (paths) => {
      const { args, feedback, review } = prepare(paths);
      const originalStore = readFileSync(paths.store);
      const cases = [
        { ...review, graphHash: '0'.repeat(64) },
        { ...review, contract: { ...review.contract, promptHash: '0'.repeat(64) } },
        { ...review, report: { ...review.report, turnAccepted: 'unknown' } },
        {
          ...review,
          review: {
            ...review.review,
            context: { verdict: 'insufficient', reason: 'Missing the linked policy.' },
          },
        },
        {
          ...review,
          review: {
            ...review.review,
            omissions: [
              {
                text: 'Invented rule.',
                evidence: [{ quote: 'Invented rule.', lineStart: 3, lineEnd: 3 }],
              },
            ],
          },
        },
        {
          ...review,
          status: 'reviewed',
          review: {
            ...review.review,
            coverage: { verdict: 'complete', reason: 'Both rules are preserved.', evidence },
            omissions: [],
          },
        },
      ];
      for (const value of cases) {
        writeFileSync(feedback, JSON.stringify(value));
        expect(invoke(paths.root, args).status).toBe(1);
        expect(readFileSync(paths.store)).toEqual(originalStore);
        expect(readFileSync(paths.calls, 'utf8')).toBe('called\n'.repeat(3));
      }
    },
    { source },
  );
});

test('an unchanged response preserves the adverse finding instead of repeating review or approving it', async () => {
  await nativeProject(
    (paths) => {
      const { args, original, inspect, ingest } = prepare(paths);
      writeFileSync(paths.candidate, JSON.stringify(original.candidate));
      const unchanged = invoke(paths.root, args);
      expect(unchanged.status).toBe(1);
      const result = JSON.parse(inspect('first.md').stdout).result;
      expect(result.status).toBe('failed');
      expect(result.attempts[1]).toMatchObject({
        code: 'UNCHANGED_REVISION',
        usage: { totalTokens: 150 },
      });
      expect(result.revisions[0].candidate).toEqual(original.candidate);
      expect(invoke(paths.root, ingest).status).toBe(1);
      expect(readFileSync(paths.calls, 'utf8')).toBe('called\n'.repeat(4));
    },
    { source },
  );
});
