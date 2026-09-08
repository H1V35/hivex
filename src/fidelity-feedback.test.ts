import { expect, test } from 'bun:test';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { nativeSource, nativeProject } from '../test/native-project.ts';
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

function adverseFidelity() {
  return {
    coverage: { verdict: 'incomplete', reason: 'The retention rule is absent.', evidence },
    claims: [
      {
        id: 'c1',
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
      writeFileSync(paths.candidate, JSON.stringify(adverseFidelity()));
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
      const review = adverseFidelity();
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
      writeFileSync(paths.candidate, JSON.stringify(adverseFidelity()));
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
      for (const field of ['bindings', 'model-output']) {
        const changed = JSON.parse(reviewed.stdout);
        if (field === 'bindings') changed.reviewBindings.claims.c1 = '0'.repeat(64);
        if (field === 'model-output') changed.modelOutputHash = '0'.repeat(64);
        writeFileSync(retained, JSON.stringify(changed));
        expect(invoke(paths.root, args).status).toBe(1);
      }
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    },
    2,
    false,
    source,
  );
});

test('retains rejected feedback-review output without treating it as fidelity evidence', async () => {
  await projectWithReviews(
    (paths, fixture) => {
      const feedback = concern(paths, fixture);
      const response = adverseFidelity();
      response.claims = response.claims.map((claim) => ({ ...claim, id: 'c9' }));
      const raw = JSON.stringify(response);
      writeFileSync(paths.candidate, raw);
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
      expect(reviewed.status).toBe(1);
      const result = JSON.parse(reviewed.stdout);
      expect(result.report.outcome).toBe('invalid-output');
      expect(result.review).toBeNull();
      expect(result.rejectedOutput).toEqual({
        text: raw,
        hash: new Bun.CryptoHasher('sha256').update(raw).digest('hex'),
      });
      result.rejectedOutput.text += 'altered';
      const path = join(dirname(paths.store), 'rejected-fidelity.json');
      writeFileSync(path, JSON.stringify(result));
      const calls = readFileSync(paths.calls, 'utf8');
      const rejected = invoke(paths.root, [
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
      expect(rejected.status).toBe(1);
      expect(rejected.stderr).toContain('Rejected model output is altered');
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    },
    2,
    false,
    source,
  );
});

test('uses short source-local IDs and rebinds local relationships to their original graph identities', async () => {
  await nativeProject(
    (paths) => {
      const quotes = [
        { quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 },
        ...evidence,
      ];
      const candidate = {
        claims: quotes.map((quote, index) => ({
          id: `c${index + 1}`,
          text: quote.quote,
          kind: 'constraint',
          conditions: [],
          exceptions: [],
          evidence: [quote],
        })),
        relations: [{ from: 'c1', to: 'c2', type: 'requires', evidence: quotes }],
      };
      writeFileSync(paths.candidate, JSON.stringify(candidate));
      expect(
        invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
      ).toBe(0);
      const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
      expect(built.status).toBe(0);
      const graph: Fixture['graph'] = JSON.parse(built.stdout);
      const input = join(dirname(paths.store), 'graph.json');
      writeFileSync(input, built.stdout);
      writeFileSync(
        paths.candidate,
        JSON.stringify({
          assessments: ['s1', 's2'].flatMap((source) =>
            ['c1', 'c2'].map((claim) => ({
              id: `${source}:${claim}`,
              verdict: source === 's1' && claim === 'c1' ? 'unresolved' : 'reviewed',
              reason: 'Independently verify source fidelity.',
              relations: [],
              evidence: quotes.map((quote) => ({ ...quote, source })),
            })),
          ),
          relations: [],
          coverage: { complete: true, reason: 'Every supplied claim is assessed.' },
          context: { verdict: 'sufficient', reason: 'Both complete sources are available.' },
        }),
      );
      const comparison = invoke(paths.root, [
        'graph',
        'compare',
        'first.md',
        'second.md',
        '--input',
        input,
        '--codex',
        paths.binary,
      ]);
      expect(comparison.stderr).toBe('');
      expect(comparison.status).toBe(1);
      const feedback = join(dirname(paths.store), 'comparison.json');
      writeFileSync(feedback, comparison.stdout);
      const args = ['graph', 'review', 'first.md', '--input', input, '--feedback', feedback];
      const prepared = invoke(paths.root, [...args, '--prepare']);
      expect(prepared.stderr).toBe('');
      const request: {
        prompt: string;
        reviewBindings: { claims: Record<string, string>; relations: Record<string, string> };
      } = JSON.parse(prepared.stdout);
      const localNodes = graph.nodes.filter((node) => node.source === 'first.md');
      const localEdge = graph.edges.find((edge) => edge.source === 'first.md');
      expect(Object.values(request.reviewBindings.claims)).toEqual(
        localNodes.map((node) => node.id),
      );
      expect(request.reviewBindings.relations.r1).toBe(localEdge?.id);
      for (const node of localNodes) expect(request.prompt).not.toContain(node.id);
      writeFileSync(
        paths.candidate,
        JSON.stringify({
          coverage: {
            verdict: 'complete',
            reason: 'Both source rules are represented.',
            evidence: quotes,
          },
          claims: ['c1', 'c2'].map((id) => ({
            id,
            verdict: 'faithful',
            reason: 'The source preserves this rule.',
            evidence: quotes,
          })),
          relations: [
            {
              id: 'r1',
              verdict: 'distorted',
              reason: 'The source states both rules without that dependency.',
              evidence: quotes,
            },
          ],
          omissions: [],
          context: { verdict: 'sufficient', reason: 'The full source is available.' },
        }),
      );
      const reviewed = invoke(paths.root, [...args, '--codex', paths.binary]);
      expect(reviewed.stderr).toBe('');
      const result = JSON.parse(reviewed.stdout);
      expect(result.report.outcome).toBe('completed');
      expect(result.review.relations[0].id).toBe(localEdge?.id);
      expect(result.reviewBindings).toEqual(request.reviewBindings);
    },
    { source },
  );
});
