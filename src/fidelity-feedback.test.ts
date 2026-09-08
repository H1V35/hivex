import { expect, test } from 'bun:test';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { nativeSource } from '../test/native-project.ts';
import {
  invoke,
  projectWithReviews,
  compare,
  comparisonResponse,
  type Paths,
  type Fixture,
} from '../test/reviewed-project.ts';

const source = nativeSource + 'Preserve the original source.\n';
const evidence = [{ quote: 'Preserve the original source.', lineStart: 4, lineEnd: 4 }];

function concern(paths: Paths, fixture: Fixture) {
  const response = comparisonResponse();
  const first = response.assessments[0];
  if (!first) throw new Error('Expected first assessment');
  first.verdict = 'unresolved';
  first.reason = 'The extraction omits the source retention requirement.';
  first.evidence.push({
    source: 's1',
    quote: 'Preserve the original source.',
    lineStart: 4,
    lineEnd: 4,
  });
  response.context = {
    verdict: 'insufficient',
    reason:
      'External context is missing for currentness, but the first source contains the retention requirement.',
  };
  expect(compare(paths, fixture, response).status).toBe(1);
  const exported = invoke(paths.root, [
    'graph',
    'compare',
    '--export',
    '--input',
    fixture.input,
    '--store',
    fixture.comparisons,
    '--neighbors',
    '1',
    '--max-bytes',
    '1048576',
  ]);
  const parsed: { comparisons: { id: string; result: unknown }[] } = JSON.parse(exported.stdout);
  const pair = parsed.comparisons[0];
  if (!pair) throw new Error('Expected comparison');
  const shown = invoke(paths.root, [
    'graph',
    'compare',
    '--show',
    pair.id,
    '--input',
    fixture.input,
    '--store',
    fixture.comparisons,
    '--neighbors',
    '1',
    '--max-bytes',
    '1048576',
  ]);
  expect(shown.stderr).toBe('');
  const path = join(dirname(paths.store), 'comparison-feedback.json');
  writeFileSync(path, shown.stdout);
  return { path, raw: pair.result };
}

function adverseFidelity(fixture: Fixture) {
  const node = fixture.graph.nodes.find((entry) => entry.source === 'first.md');
  if (!node) throw new Error('Expected claim');
  return {
    coverage: { verdict: 'incomplete', reason: 'The retention rule is absent.', evidence },
    claims: [
      {
        id: node.id,
        verdict: 'faithful',
        reason: 'The cache prohibition is preserved.',
        evidence: [{ quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }],
      },
    ],
    relations: [],
    omissions: [{ text: 'Preserve the original source.', evidence }],
    context: { verdict: 'sufficient', reason: 'The full source contains the missing rule.' },
  };
}

test('reassesses fidelity against concrete comparison feedback and revises a candidate without rewriting its original reviews', async () => {
  await projectWithReviews(
    (paths, fixture) => {
      const feedback = concern(paths, fixture);
      const sourceReviews = readFileSync(fixture.reviews);
      const comparisons = readFileSync(fixture.comparisons);
      const originalGraph = readFileSync(fixture.input);
      const calls = readFileSync(paths.calls, 'utf8');
      const args = [
        'graph',
        'review',
        'first.md',
        '--input',
        fixture.input,
        '--feedback',
        feedback.path,
      ];
      const prepared = invoke(paths.root, [...args, '--prepare']);
      expect(prepared.stderr).toBe('');
      expect(prepared.status).toBe(0);
      expect(JSON.parse(prepared.stdout).prompt).toContain(
        'The extraction omits the source retention requirement.',
      );
      expect(JSON.parse(prepared.stdout).prompt).toContain(JSON.stringify(source));
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
      writeFileSync(paths.candidate, JSON.stringify(adverseFidelity(fixture)));
      const reviewed = invoke(paths.root, [...args, '--codex', paths.binary]);
      expect(reviewed.stderr).toBe('');
      expect(reviewed.status).toBe(1);
      const result = JSON.parse(reviewed.stdout);
      expect(result.feedback.comparison).toEqual(feedback.raw);
      expect(result.review.context.verdict).toBe('sufficient');
      const review = join(dirname(paths.store), 'challenged-fidelity.json');
      writeFileSync(review, reviewed.stdout);
      const revision = [
        'ingest',
        '--revise',
        'first.md',
        '--input',
        fixture.input,
        '--store',
        paths.store,
        '--feedback',
        review,
      ];
      expect(invoke(paths.root, [...revision, '--prepare']).status).toBe(0);
      writeFileSync(
        paths.candidate,
        JSON.stringify({
          claims: [
            {
              id: 'c1',
              text: 'Never treat a cache as authority.',
              kind: 'constraint',
              conditions: [],
              exceptions: [],
              evidence: [{ quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }],
            },
            {
              id: 'c2',
              text: 'Preserve the original source.',
              kind: 'constraint',
              conditions: [],
              exceptions: [],
              evidence,
            },
          ],
          relations: [],
        }),
      );
      expect(
        invoke(paths.root, [...revision, '--codex', paths.binary, '--attempts', '1']).status,
      ).toBe(0);
      const retained = invoke(paths.root, [
        'ingest',
        '--show',
        'first.md',
        '--store',
        paths.store,
        '--max-bytes',
        '1048576',
      ]);
      expect(JSON.parse(retained.stdout).result.revisions[0].feedback.feedback.comparison).toEqual(
        feedback.raw,
      );
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls + 'called\ncalled\n');
      expect(readFileSync(fixture.reviews)).toEqual(sourceReviews);
      expect(readFileSync(fixture.comparisons)).toEqual(comparisons);
      expect(readFileSync(fixture.input)).toEqual(originalGraph);
    },
    2,
    false,
    source,
  );
});

test('a fidelity reassessment may uphold the candidate and still cannot authorize revision', async () => {
  await projectWithReviews(
    (paths, fixture) => {
      const feedback = concern(paths, fixture);
      const review = adverseFidelity(fixture);
      review.coverage = {
        verdict: 'complete',
        reason: 'The comparison concern is not supported as an omission.',
        evidence,
      };
      review.omissions = [];
      writeFileSync(paths.candidate, JSON.stringify(review));
      const checked = invoke(paths.root, [
        'graph',
        'review',
        'first.md',
        '--input',
        fixture.input,
        '--feedback',
        feedback.path,
        '--codex',
        paths.binary,
      ]);
      expect(checked.stderr).toBe('');
      expect(checked.status).toBe(0);
      const path = join(dirname(paths.store), 'upheld-fidelity.json');
      writeFileSync(path, checked.stdout);
      const calls = readFileSync(paths.calls, 'utf8');
      const revision = invoke(paths.root, [
        'ingest',
        '--revise',
        'first.md',
        '--input',
        fixture.input,
        '--store',
        paths.store,
        '--feedback',
        path,
        '--prepare',
      ]);
      expect(revision.status).toBe(1);
      expect(revision.stderr).toContain('INGESTION_REVISION_INVALID');
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    },
    2,
    false,
    source,
  );
});

test('rejects unrelated, stale, incomplete and oversized feedback before invoking fidelity', async () => {
  await projectWithReviews(
    (paths, fixture) => {
      const feedback = concern(paths, fixture);
      const calls = readFileSync(paths.calls, 'utf8');
      const args = [
        'graph',
        'review',
        'first.md',
        '--input',
        fixture.input,
        '--feedback',
        feedback.path,
        '--codex',
        paths.binary,
      ];
      const original = JSON.stringify(feedback.raw);
      for (const change of ['graph', 'outcome', 'passing', 'claims', 'oversized']) {
        const altered = JSON.parse(original);
        if (change === 'graph') altered.graphHash = '0'.repeat(64);
        if (change === 'outcome') altered.report.outcome = 'timeout';
        if (change === 'passing') altered.status = 'reviewed';
        if (change === 'claims') altered.comparison.assessments = [];
        if (change === 'oversized') altered.padding = 'x'.repeat(262144);
        writeFileSync(feedback.path, JSON.stringify(altered));
        expect(invoke(paths.root, args).status).toBe(1);
        expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
      }
      writeFileSync(feedback.path, original);
      const unrelated = invoke(paths.root, [
        'graph',
        'review',
        'second.md',
        '--input',
        fixture.input,
        '--feedback',
        feedback.path,
        '--codex',
        paths.binary,
      ]);
      expect(unrelated.status).toBe(1);
      expect(unrelated.stderr).toContain('REVIEW_FEEDBACK_INVALID');
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    },
    2,
    false,
    source,
  );
});

test('binds the complete comparison receipt and feedback prompt when admitting fidelity as revision evidence', async () => {
  await projectWithReviews(
    (paths, fixture) => {
      const feedback = concern(paths, fixture);
      writeFileSync(paths.candidate, JSON.stringify(adverseFidelity(fixture)));
      const reviewed = invoke(paths.root, [
        'graph',
        'review',
        'first.md',
        '--input',
        fixture.input,
        '--feedback',
        feedback.path,
        '--codex',
        paths.binary,
      ]);
      expect(reviewed.stderr).toBe('');
      expect(reviewed.status).toBe(1);
      const retained = join(dirname(paths.store), 'tampered-fidelity.json');
      const altered = JSON.parse(reviewed.stdout);
      altered.feedback.comparison.report.usage.totalTokens += 1;
      writeFileSync(retained, JSON.stringify(altered));
      const calls = readFileSync(paths.calls, 'utf8');
      const args = [
        'ingest',
        '--revise',
        'first.md',
        '--input',
        fixture.input,
        '--store',
        paths.store,
        '--feedback',
        retained,
        '--prepare',
      ];
      expect(invoke(paths.root, args).status).toBe(1);
      altered.feedback.hash = new Bun.CryptoHasher('sha256')
        .update(JSON.stringify(altered.feedback.comparison))
        .digest('hex');
      writeFileSync(retained, JSON.stringify(altered));
      expect(invoke(paths.root, args).status).toBe(1);
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    },
    2,
    false,
    source,
  );
});
