import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { nativeProject } from '../test/native-project.ts';
import type { GraphSnapshot } from './graph/snapshot.ts';
import { z } from 'zod';
import {
  createReviewContext,
  prepareSourceReview,
  reviewModelSchema,
  sourceReviewSchema,
} from './graph/source-review.ts';

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
          { id: 'c1', verdict: 'faithful', reason: 'The prohibition is preserved.', evidence },
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
      reviewBindings: { claims: { c1: node.id }, relations: {} },
    });
    const output = JSON.parse(result.stdout);
    expect(output.modelOutputHash).toMatch(/^[a-f0-9]{64}$/);
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\ncalled\n');
    expect(readFileSync(input, 'utf8')).toBe(built.stdout);
  });
});

test('prepares the compact per-source review schema without calling a model', async () => {
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
      const output: {
        prompt: string;
        schema: {
          properties: {
            claims: {
              minItems: number;
              maxItems: number;
              items: { properties: { id: { enum: string[] } } };
            };
            relations: { minItems: number; maxItems: number };
          };
        };
        reviewBindings: { claims: Record<string, string> };
      } = JSON.parse(result.stdout);
      expect(output).toMatchObject({ accepted: false, status: 'prepared' });
      expect(output.schema.properties.claims.minItems).toBe(1);
      expect(output.schema.properties.claims.maxItems).toBe(1);
      expect(output.schema.properties.claims.items.properties.id.enum).toEqual(['c1']);
      expect(output.schema.properties.relations.minItems).toBe(0);
      expect(output.schema.properties.relations.maxItems).toBe(0);
      expect(output.reviewBindings.claims).toEqual({ c1: expect.any(String) });
      expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
      expect(readFileSync(paths.store)).toEqual(before);
    },
    {
      source:
        '# Decision\n\nNever treat a cache as authority.\n\n## Amendment\n\nRebuild an incomplete cache from the sources.\n',
    },
  );
});

test('uses exact compact coverage and literal multiline citations for ordinary fidelity', async () => {
  await nativeProject(
    (paths) => {
      const lineQuote = { quote: 'First line.\nSecond line.', lineStart: 3, lineEnd: 4 };
      const singleQuotes = [
        { quote: 'First line.', lineStart: 3, lineEnd: 3 },
        { quote: 'Third line.', lineStart: 6, lineEnd: 6 },
      ];
      writeFileSync(
        paths.candidate,
        JSON.stringify({
          claims: [
            {
              id: 'c1',
              text: 'First line.',
              kind: 'constraint',
              conditions: [],
              exceptions: [],
              evidence: [singleQuotes[0]],
            },
            {
              id: 'c2',
              text: 'Third line.',
              kind: 'constraint',
              conditions: [],
              exceptions: [],
              evidence: [singleQuotes[1]],
            },
          ],
          relations: [{ from: 'c1', to: 'c2', type: 'supports', evidence: [lineQuote] }],
        }),
      );
      expect(
        invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
      ).toBe(0);
      const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
      expect(built.status).toBe(0);
      const graph: GraphSnapshot = JSON.parse(built.stdout);
      const input = join(dirname(paths.store), 'graph.json');
      writeFileSync(input, built.stdout);
      const response = {
        coverage: {
          verdict: 'complete',
          reason: 'Every claim and relation is represented.',
          evidence: singleQuotes,
        },
        claims: ['c1', 'c2'].map((id, index) => ({
          id,
          verdict: 'faithful',
          reason: 'The source preserves this item.',
          evidence: [singleQuotes[index]],
        })),
        relations: [
          {
            id: 'r1',
            verdict: 'faithful',
            reason: 'The source preserves the relationship.',
            evidence: [lineQuote],
          },
        ],
        omissions: [],
        context: { verdict: 'sufficient', reason: 'The complete source is supplied.' },
      };
      writeFileSync(paths.candidate, JSON.stringify(response));
      const reviewed = invoke(paths.root, [
        'graph',
        'review',
        'first.md',
        '--input',
        input,
        '--codex',
        paths.binary,
      ]);
      expect(reviewed.stderr).toBe('');
      expect(reviewed.status).toBe(0);
      const output = JSON.parse(reviewed.stdout) as {
        review: { claims: { id: string }[]; relations: { id: string }[] };
        reviewBindings: { claims: Record<string, string>; relations: Record<string, string> };
      };
      expect(output.review.claims).toHaveLength(2);
      expect(output.review.relations).toHaveLength(1);
      expect(output.review.claims.map((claim: { id: string }) => claim.id)).toEqual(
        Object.values(output.reviewBindings.claims),
      );
      const boundRelation = output.review.relations[0];
      const relationId = output.reviewBindings.relations.r1;
      if (!boundRelation || !relationId) throw new Error('Expected the bound relation');
      expect(boundRelation.id).toBe(relationId);

      writeFileSync(paths.candidate, JSON.stringify({ ...response, claims: [response.claims[0]] }));
      const missing = invoke(paths.root, [
        'graph',
        'review',
        'first.md',
        '--input',
        input,
        '--codex',
        paths.binary,
      ]);
      expect(missing.status).toBe(1);
      expect(JSON.parse(missing.stdout)).toMatchObject({
        status: 'failed',
        review: null,
        report: { outcome: 'invalid-output' },
      });

      const collapsed = structuredClone(response);
      const collapsedRelation = collapsed.relations[0];
      if (!collapsedRelation) throw new Error('Expected a relation');
      const citation = collapsedRelation.evidence[0];
      if (!citation) throw new Error('Expected relation evidence');
      citation.quote = 'First line. Second line.';
      writeFileSync(paths.candidate, JSON.stringify(collapsed));
      const alteredCitation = invoke(paths.root, [
        'graph',
        'review',
        'first.md',
        '--input',
        input,
        '--codex',
        paths.binary,
      ]);
      expect(alteredCitation.status).toBe(1);
      expect(JSON.parse(alteredCitation.stdout)).toMatchObject({
        status: 'failed',
        review: null,
        report: { outcome: 'invalid-output' },
      });
      expect(graph.nodes.filter((node) => node.source === 'first.md')).toHaveLength(2);
      expect(graph.edges.filter((edge) => edge.source === 'first.md')).toHaveLength(1);
    },
    { source: '# Rules\n\nFirst line.\nSecond line.\n\nThird line.\n' },
  );
});

test('keeps ordinary no-knowledge valid with zero compact IDs and required evidence', async () => {
  await nativeProject(
    (paths) => {
      writeFileSync(paths.candidate, JSON.stringify({ claims: [], relations: [] }));
      expect(
        invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
      ).toBe(0);
      const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
      expect(built.status).toBe(0);
      const input = join(dirname(paths.store), 'graph.json');
      writeFileSync(input, built.stdout);
      const prepared = invoke(paths.root, [
        'graph',
        'review',
        'first.md',
        '--input',
        input,
        '--prepare',
        '--codex',
        '/must-not-run',
      ]);
      expect(prepared.status).toBe(0);
      const schema = JSON.parse(prepared.stdout).schema;
      expect(schema.properties.claims.minItems).toBe(0);
      expect(schema.properties.claims.maxItems).toBe(0);
      expect(schema.properties.claims.items.properties.id.enum).toEqual(['c1']);
      expect(schema.properties.relations.minItems).toBe(0);
      expect(schema.properties.relations.maxItems).toBe(0);
      expect(schema.properties.relations.items.properties.id.enum).toEqual(['r1']);
      expect(schema.properties.coverage.properties.evidence.items.properties).toMatchObject({
        lineStart: { minimum: 1, maximum: 4 },
        lineEnd: { minimum: 1, maximum: 4 },
      });

      const evidence = [{ quote: '# Cache', lineStart: 1, lineEnd: 1 }];
      writeFileSync(
        paths.candidate,
        JSON.stringify({
          coverage: {
            verdict: 'no-knowledge',
            reason: 'The source contains no project knowledge.',
            evidence,
          },
          claims: [],
          relations: [],
          omissions: [],
          context: { verdict: 'sufficient', reason: 'The complete source is supplied.' },
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
      expect(reviewed.stderr).toBe('');
      expect(reviewed.status).toBe(0);
      expect(JSON.parse(reviewed.stdout)).toMatchObject({
        status: 'reviewed',
        review: { claims: [], relations: [], coverage: { verdict: 'no-knowledge' } },
        reviewBindings: { claims: {}, relations: {} },
      });
    },
    { source: '# Cache\n\nNo project rule is declared here.\n' },
  );
});

test.each([false, true])(
  'bounds every review evidence coordinate to its source (section: %s)',
  async (section) => {
    await nativeProject(
      (paths) => {
        writeFileSync(
          join(paths.root, 'hivex.json'),
          JSON.stringify({
            version: 1,
            collections: [
              {
                id: 'project',
                include: [section ? { path: 'first.md', anchor: 'rules' } : 'first.md'],
              },
            ],
          }),
        );
        paths.git(['add', 'hivex.json']);
        paths.git([
          '-c',
          'user.name=Test',
          '-c',
          'user.email=test@example.invalid',
          'commit',
          '-qm',
          'Select review source',
        ]);
        const quote = { quote: 'Never treat a cache as authority.', lineStart: 7, lineEnd: 7 };
        writeFileSync(
          paths.candidate,
          JSON.stringify({
            claims: [
              {
                id: 'c1',
                text: quote.quote,
                kind: 'constraint',
                conditions: [],
                exceptions: [],
                evidence: [quote],
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
        const input = join(dirname(paths.store), 'graph.json');
        writeFileSync(input, built.stdout);
        const prepared = prepareSourceReview(
          createReviewContext({ input, root: paths.root }),
          section ? 'first.md#rules' : 'first.md',
        );
        const modelSchema = reviewModelSchema(prepared);
        const minimum = section ? 5 : 1;
        const maximum = section ? 8 : 13;
        const schemas = [
          modelSchema.shape.coverage.shape.evidence,
          modelSchema.shape.claims.element.shape.evidence,
          modelSchema.shape.relations.element.shape.evidence,
          modelSchema.shape.omissions.element.shape.evidence,
        ];
        for (const schema of schemas) {
          expect(z.toJSONSchema(schema.element)).toMatchObject({
            properties: {
              lineStart: { type: 'integer', minimum, maximum },
              lineEnd: { type: 'integer', minimum, maximum },
            },
          });
          const boundary = [{ ...quote, lineStart: minimum, lineEnd: maximum }];
          expect(schema.parse(boundary)).toEqual(boundary);
          for (const coordinate of ['lineStart', 'lineEnd']) {
            for (const value of [minimum - 1, maximum + 1, 1669000000000000]) {
              expect(schema.safeParse([{ ...quote, [coordinate]: value }]).success).toBe(false);
            }
          }
        }
        expect(
          sourceReviewSchema.shape.coverage.shape.evidence.safeParse([
            { ...quote, lineEnd: 1669000000000000 },
          ]).success,
        ).toBe(true);
      },
      {
        source:
          '# Document\n\nIntroduction.\n\n## Rules\n\nNever treat a cache as authority.\nKeep the source.\n\n## Other\n\nOutside.\n',
      },
    );
  },
);

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
      claims: [{ id: 'c1', verdict: 'faithful', reason: 'The rule matches.', evidence }],
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
      const output = JSON.parse(result.stdout);
      expect(output).toMatchObject({
        accepted: false,
        status: 'failed',
        report: { outcome: 'completed' },
      });
      expect(output.review.claims[0].id).toBe(node.id);
      expect(output.review.claims[0].verdict).toBe(response.claims[0]?.verdict);
    }
    expect(readFileSync(input, 'utf8')).toBe(built.stdout);
  });
});
