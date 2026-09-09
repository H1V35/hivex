import { expect, test } from 'bun:test';
import { existsSync, linkSync, readFileSync, symlinkSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { nativeProject } from '../test/native-project.ts';
import { invoke, type Paths } from '../test/reviewed-project.ts';
import { hash } from './sources/markdown.ts';
import { comparisonResultSchema } from './graph/comparison-cohort.ts';
import { z } from 'zod';

type Selection = { pairs: { id: string; sources: string[] }[] };
const cohortSchema = z.object({
  plan: z.object({ contract: z.unknown() }),
  selection: z.object({ comparisonContext: z.unknown().optional() }),
  comparisons: z.array(
    z.object({
      id: z.string(),
      state: z.string(),
      result: comparisonResultSchema.nullable(),
      previousAttempts: z.array(comparisonResultSchema).optional(),
    }),
  ),
});

const documents = [
  {
    id: 'first.md',
    title: 'Retention',
    rule: 'Keep records for 30 days.',
    link: '[Policy](second.md)',
  },
  {
    id: 'second.md',
    title: 'Specific policy',
    rule: 'Delete temporary records after 7 days.',
    link: '[Override](third.md)',
  },
  {
    id: 'third.md',
    title: 'Approved override',
    rule: 'The approved temporary-record policy is an exception to the 30-day retention rule.',
    link: '',
  },
];
const markdown = (doc: (typeof documents)[number]) =>
  `# ${doc.title}\n\n${doc.rule}\n\n${doc.link}\n`;
const citation = (quote: string) => ({ quote, lineStart: 3, lineEnd: 3 });
const mapping = {
  version: 1,
  pairs: [{ sources: ['second.md', 'first.md'], context: ['third.md'] }],
};

test('requires an existing verified candidate before update can attach supporting context', async () => {
  await nativeProject((paths) => {
    const config = join(dirname(paths.root), 'context.json');
    writeFileSync(config, JSON.stringify(mapping));
    const output = join(dirname(paths.root), 'managed.json');
    const result = invoke(paths.root, [
      'update',
      '--output',
      output,
      '--comparison-context',
      config,
      '--codex',
      paths.binary,
      '--max-units',
      '0',
    ]);
    expect(result.status).toBe(1);
    expect(existsSync(join(paths.root, '.hivex'))).toBe(false);
    expect(readFileSync(paths.calls, 'utf8')).toBe('');
    expect(existsSync(output)).toBe(false);
  });
});

test('rejects context on an old managed candidate before changing target or stores', async () => {
  await contextProject((paths, input, config) => {
    const output = join(dirname(paths.root), 'managed.json');
    const candidate = readFileSync(input);
    const store = join(paths.root, '.hivex/ingestion.sqlite');
    const before = readFileSync(store);
    writeFileSync(
      join(paths.root, 'third.md'),
      markdown(documents[2]!) + '\nA changed override.\n',
    );
    paths.git(['add', 'third.md']);
    paths.git([
      '-c',
      'user.name=Test',
      '-c',
      'user.email=test@example.invalid',
      'commit',
      '-qm',
      'changed supporting source',
    ]);
    const calls = readFileSync(paths.calls, 'utf8');
    const result = invoke(paths.root, [
      'update',
      '--output',
      output,
      '--comparison-context',
      config,
      '--ref',
      'HEAD',
      '--codex',
      paths.binary,
      '--max-units',
      '0',
    ]);
    expect(result.status).toBe(1);
    expect(readFileSync(input)).toEqual(candidate);
    expect(readFileSync(store)).toEqual(before);
    expect(existsSync(join(paths.root, '.hivex/update.json'))).toBe(false);
    expect(existsSync(join(paths.root, '.hivex/update.lock'))).toBe(false);
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
  });
});

test('rejects invalid, unused, oversized or aliased context inputs before calls', async () => {
  await contextProject((paths, input, config) => {
    const calls = readFileSync(paths.calls, 'utf8');
    const args = [
      'graph',
      'compare',
      'first.md',
      'second.md',
      '--input',
      input,
      '--codex',
      paths.binary,
      '--comparison-context',
      config,
    ];
    const entry = mapping.pairs[0]!;
    for (const pairs of [
      [entry, { ...entry, sources: ['first.md', 'second.md'] }],
      [{ ...entry, context: ['missing.md'] }],
      [{ ...entry, sources: ['first.md', 'missing.md'] }],
      [{ ...entry, sources: ['first.md', 'first.md'] }],
      [{ ...entry, context: ['first.md'] }],
      [{ ...entry, context: ['third.md', 'third.md'] }],
      [{ ...entry, context: ['third.md', 'fourth.md', 'fifth.md'] }],
      [{ sources: ['first.md', 'third.md'], context: ['second.md'] }],
    ]) {
      writeFileSync(config, JSON.stringify({ version: 1, pairs }));
      expect(invoke(paths.root, args).status).toBe(1);
      expect(
        invoke(paths.root, [
          'graph',
          'compare-plan',
          '--input',
          input,
          '--comparison-context',
          config,
        ]).status,
      ).toBe(1);
    }
    writeFileSync(config, ' '.repeat(131073));
    expect(invoke(paths.root, args).status).toBe(1);
    const alias = join(dirname(paths.root), 'context-alias.json');
    symlinkSync(config, alias);
    expect(invoke(paths.root, [...args.slice(0, -1), alias]).status).toBe(1);
    writeFileSync(config, JSON.stringify(mapping));
    const output = join(dirname(paths.root), 'managed.json');
    linkSync(config, `${output}.pending`);
    expect(
      invoke(paths.root, [
        'update',
        '--output',
        output,
        '--comparison-context',
        config,
        '--max-units',
        '0',
      ]).status,
    ).toBe(1);
    expect(readFileSync(config, 'utf8')).toBe(JSON.stringify(mapping));
    expect(
      invoke(paths.root, [
        'update',
        '--output',
        config,
        '--comparison-context',
        config,
        '--max-units',
        '0',
      ]).status,
    ).toBe(1);
    expect(
      invoke(paths.root, [
        'update',
        '--output',
        output,
        '--comparison-context',
        join(paths.root, '.hivex/ingestion.sqlite'),
        '--max-units',
        '0',
      ]).status,
    ).toBe(1);
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
  });
}, 15000);

test('validates supporting citations without extending primary coverage or endpoints', async () => {
  await contextProject((paths, input, config) => {
    const good = pairResponse(['first.md', 'second.md'], true);
    const relation = good.relations[0]!;
    const invalid = [
      {
        ...good,
        relations: [
          {
            ...relation,
            evidence: [
              ...relation.evidence.slice(0, 2),
              { ...relation.evidence[2], quote: 'An invented override.' },
            ],
          },
        ],
      },
      {
        ...good,
        relations: [
          {
            ...relation,
            evidence: [
              ...relation.evidence.slice(0, 2),
              { ...relation.evidence[2], lineStart: 1, lineEnd: 1 },
            ],
          },
        ],
      },
      {
        ...good,
        relations: [
          {
            ...relation,
            evidence: [...relation.evidence.slice(0, 2), { ...relation.evidence[2], source: 'x2' }],
          },
        ],
      },
      { ...good, relations: [{ ...relation, from: 'x1:c1' }] },
      { ...good, relations: [{ ...relation, evidence: relation.evidence.slice(1) }] },
      { ...good, assessments: [...good.assessments, { ...good.assessments[0], id: 'x1:c1' }] },
    ];
    for (const response of invalid) {
      writeFileSync(paths.candidate, JSON.stringify(response));
      const result = invoke(paths.root, [
        'graph',
        'compare',
        'first.md',
        'second.md',
        '--input',
        input,
        '--comparison-context',
        config,
        '--codex',
        paths.binary,
      ]);
      expect(result.status).toBe(1);
      expect(JSON.parse(result.stdout)).toMatchObject({
        accepted: false,
        status: 'failed',
        comparison: null,
        report: { outcome: 'invalid-output' },
      });
    }
  });
}, 15000);

function pairResponse(ids: string[], contextual = false, adverse = false) {
  const evidence = ids.map((id, index) => ({
    ...citation(documents.find((doc) => doc.id === id)!.rule),
    source: `s${index + 1}`,
  }));
  return {
    assessments: ids.map((_, index) => ({
      id: `s${index + 1}:c1`,
      verdict: adverse ? 'unresolved' : 'reviewed',
      reason: adverse
        ? 'The exception authority is not in this pair.'
        : 'The supplied scope is supported.',
      relations: contextual ? ['r1'] : [],
      evidence: [evidence[index]],
    })),
    relations: contextual
      ? [
          {
            id: 'r1',
            from: 's2:c1',
            to: 's1:c1',
            type: 'exception-to',
            scope: { extent: 'partial-claim', description: 'Temporary records only.' },
            conditions: ['Only the approved temporary-record policy.'],
            exceptions: [],
            evidence: [...evidence, { ...citation(documents[2]!.rule), source: 'x1' }],
          },
        ]
      : [],
    coverage: { complete: true, reason: 'Both primary claims assessed.' },
    context: {
      verdict: adverse ? 'insufficient' : 'sufficient',
      reason: 'Only supplied evidence determines the result.',
    },
  };
}

function reviewSources(paths: Paths, input: string) {
  for (const doc of documents) {
    const evidence = [citation(doc.rule)];
    writeFileSync(
      paths.candidate,
      JSON.stringify({
        coverage: { verdict: 'complete', reason: 'Rule preserved.', evidence },
        claims: [{ id: 'c1', verdict: 'faithful', reason: 'Rule preserved.', evidence }],
        relations: [],
        omissions: [],
        context: { verdict: 'sufficient', reason: 'Source complete.' },
      }),
    );
    expect(
      invoke(paths.root, [
        'graph',
        'review',
        '--all',
        '--input',
        input,
        '--codex',
        paths.binary,
        '--max-units',
        '1',
      ]).status,
    ).toBe(0);
  }
}

async function contextProject(
  run: (paths: Paths, input: string, config: string) => void | Promise<void>,
  extra: typeof documents = [],
) {
  await nativeProject(
    async (paths) => {
      writeFileSync(join(paths.root, '.gitignore'), '.hivex/\n');
      const sources = [...documents, ...extra];
      for (const doc of sources) writeFileSync(join(paths.root, doc.id), markdown(doc));
      paths.git(['add', '.']);
      paths.git([
        '-c',
        'user.name=Test',
        '-c',
        'user.email=test@example.invalid',
        'commit',
        '-qm',
        'synthetic rules',
      ]);
      for (const doc of sources) {
        writeFileSync(
          paths.candidate,
          JSON.stringify({
            claims: [
              {
                id: 'c1',
                text: doc.rule,
                kind: 'constraint',
                conditions: [],
                exceptions: [],
                evidence: [citation(doc.rule)],
              },
            ],
            relations: [],
          }),
        );
        expect(
          invoke(paths.root, ['ingest', '--codex', paths.binary, '--max-units', '1']).status,
        ).toBe(0);
      }
      const built = invoke(paths.root, ['graph', 'build', '--export']);
      expect(built.status).toBe(0);
      const input = join(paths.root, '.hivex/update-candidate.json');
      writeFileSync(input, built.stdout);
      const config = join(dirname(paths.root), 'comparison-context.json');
      writeFileSync(config, JSON.stringify(mapping));
      await run(paths, input, config);
    },
    { source: markdown(documents[0]!) },
  );
}

test('prepares third-source evidence without adding claim targets or changing the default request', async () => {
  await contextProject((paths, input, config) => {
    const args = ['graph', 'compare', 'first.md', 'second.md', '--input', input, '--prepare'];
    const baseline = JSON.parse(invoke(paths.root, args).stdout);
    expect(hash(baseline.prompt)).toBe(
      '144292b37515754aef8090b806759fe8b77921e713eade73470841f62be54a84',
    );
    expect(hash(JSON.stringify(baseline.schema))).toBe(
      '233586447db0c013b132966188e566346d6195b29a427ee9757c1a47b3f55572',
    );
    const calls = readFileSync(paths.calls, 'utf8');
    const result = invoke(paths.root, [...args, '--comparison-context', config]);
    expect(result.stderr).toBe('');
    expect(result.status).toBe(0);
    const prepared = JSON.parse(result.stdout);
    const prompt = String(prepared.prompt);
    const packet = JSON.parse(prompt.slice(prompt.lastIndexOf('\n\n') + 2));
    expect(packet.sources).toHaveLength(2);
    expect(packet.supportingSources).toEqual([
      {
        id: 'x1',
        sourceId: 'third.md',
        path: 'third.md',
        contentHash: expect.any(String),
        section: null,
        authority: expect.any(Object),
        firstLine: 1,
        markdown: markdown(documents[2]!),
      },
    ]);
    expect(packet.supportingSources[0]).not.toHaveProperty('claims');
    expect(prepared.sourceBindings).toMatchObject({
      s1: 'first.md',
      s2: 'second.md',
      x1: 'third.md',
    });
    expect(prepared.schema.properties.assessments.items.properties.id).toEqual(
      baseline.schema.properties.assessments.items.properties.id,
    );
    expect(
      prepared.schema.properties.assessments.items.properties.evidence.items.properties.source.enum,
    ).toEqual(['s1', 's2', 'x1']);
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
  });
});

test('accepts exactly two sorted supporting sources as citation-only x1/x2', async () => {
  const extra = {
    id: 'zz-addendum.md',
    title: 'Scope',
    rule: 'The approved override applies only to temporary records.',
    link: '',
  };
  await contextProject(
    (paths, input, config) => {
      writeFileSync(
        config,
        JSON.stringify({
          version: 1,
          pairs: [{ sources: ['second.md', 'first.md'], context: [extra.id, 'third.md'] }],
        }),
      );
      const response = pairResponse(['first.md', 'second.md'], true);
      response.relations[0]!.evidence.push({ ...citation(extra.rule), source: 'x2' });
      writeFileSync(paths.candidate, JSON.stringify(response));
      const result = invoke(paths.root, [
        'graph',
        'compare',
        'second.md',
        'first.md',
        '--input',
        input,
        '--comparison-context',
        config,
        '--codex',
        paths.binary,
      ]);
      expect(result.status).toBe(0);
      expect(JSON.parse(result.stdout)).toMatchObject({
        status: 'reviewed',
        accepted: false,
        sources: [{ id: 'first.md' }, { id: 'second.md' }],
        sourceBindings: { s1: 'first.md', s2: 'second.md', x1: 'third.md', x2: extra.id },
        supportingSources: [{ id: 'third.md' }, { id: extra.id }],
      });
    },
    [extra],
  );
});

test.each(['exception-to', 'supports'])(
  'transitions one adverse pair and grounds self-contained contextual %s evidence',
  async (relationType) => {
    await contextProject((paths, input, config) => {
      const common = ['graph', 'compare', '--input', input];
      const selection: Selection = JSON.parse(
        invoke(paths.root, ['graph', 'compare-plan', '--input', input]).stdout,
      );
      for (const pair of selection.pairs) {
        const adverse = pair.sources.includes('first.md');
        if (!adverse) {
          writeFileSync(paths.candidate, '{}');
          expect(
            invoke(paths.root, [...common, '--all', '--codex', paths.binary, '--max-units', '1'])
              .status,
          ).toBe(1);
        }
        writeFileSync(paths.candidate, JSON.stringify(pairResponse(pair.sources, false, adverse)));
        expect(
          invoke(paths.root, [
            ...common,
            '--all',
            '--codex',
            paths.binary,
            '--max-units',
            '1',
            ...(!adverse ? ['--retry-failed', pair.id, '--attempts', '2'] : []),
          ]).stderr,
        ).toBe('');
      }
      const exported = invoke(paths.root, [...common, '--export', '--max-bytes', '134217728']);
      const old = cohortSchema.parse(JSON.parse(exported.stdout));
      const archive = join(dirname(paths.root), 'comparisons-before.json');
      writeFileSync(archive, exported.stdout);
      const calls = readFileSync(paths.calls, 'utf8');
      const contextual = [...common, '--comparison-context', config];
      expect(invoke(paths.root, [...contextual, '--all', '--codex', paths.binary]).status).toBe(1);
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
      expect(
        invoke(paths.root, [
          ...contextual,
          '--all',
          '--from',
          input,
          '--reuse',
          archive,
          '--max-units',
          '1',
          '--codex',
          paths.binary,
        ]).status,
      ).toBe(1);
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
      const transfer = invoke(paths.root, [
        ...contextual,
        '--all',
        '--from',
        input,
        '--reuse',
        archive,
        '--max-units',
        '0',
      ]);
      expect(transfer.stderr).toBe('');
      expect(transfer.status).toBe(0);
      expect(JSON.parse(transfer.stdout)).toMatchObject({ completed: 1, pending: 1, processed: 0 });
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
      const current = cohortSchema.parse(
        JSON.parse(
          invoke(paths.root, [...contextual, '--export', '--max-bytes', '134217728']).stdout,
        ),
      );
      expect(current.plan.contract).toEqual(old.plan.contract);
      const unchanged = current.comparisons.find(
        (row: { state: string }) => row.state === 'reviewed',
      );
      if (!unchanged?.result) throw new Error('Expected unchanged retained pair');
      const original = old.comparisons.find((row: { id: string }) => row.id === unchanged.id);
      if (!original?.result) throw new Error('Expected original pair receipt');
      expect(unchanged.result.report).toEqual(original.result.report);
      expect(unchanged.result.comparison).toEqual(original.result.comparison);
      expect(unchanged.previousAttempts).toEqual(original.previousAttempts);
      expect(unchanged.previousAttempts).toHaveLength(1);
      expect(current.selection.comparisonContext).toMatchObject({
        version: 1,
        pairs: [
          {
            sources: ['first.md', 'second.md'],
            context: [{ id: 'third.md', contentHash: expect.any(String) }],
          },
        ],
      });
      const contextualResponse = pairResponse(['first.md', 'second.md'], true);
      contextualResponse.relations[0]!.type = relationType;
      writeFileSync(paths.candidate, JSON.stringify(contextualResponse));
      expect(invoke(paths.root, [...contextual, '--all', '--codex', paths.binary]).status).toBe(0);
      reviewSources(paths, input);
      const admitted = invoke(paths.root, [
        'graph',
        'admit',
        '--input',
        input,
        '--comparison-context',
        config,
        '--export',
      ]);
      expect(admitted.stderr).toBe('');
      expect(admitted.status).toBe(0);
      const output = join(dirname(paths.root), 'admitted.json');
      writeFileSync(output, admitted.stdout);
      const snapshot: { relationships: { evidence: { source: string }[] }[] } = JSON.parse(
        admitted.stdout,
      );
      expect(snapshot.relationships[0]?.evidence.map((entry) => entry.source)).toEqual([
        'first.md',
        'second.md',
        'third.md',
      ]);
      expect(snapshot.relationships[0]).toMatchObject({
        type: relationType,
        scope: { extent: 'partial-claim' },
      });
      expect(admitted.stdout).not.toContain(config);
      writeFileSync(config, '{invalid');
      expect(invoke(paths.root, ['graph', 'check', '--input', output]).status).toBe(0);
      expect(readFileSync(archive, 'utf8')).toBe(exported.stdout);
      const base = paths.git(['rev-parse', 'HEAD']);
      writeFileSync(join(paths.root, 'expiry.ts'), 'export const ttl = 7;\n');
      paths.git(['add', 'expiry.ts']);
      paths.git([
        '-c',
        'user.name=Test',
        '-c',
        'user.email=test@example.invalid',
        'commit',
        '-qm',
        'synthetic expiry',
      ]);
      const beforeGround = readFileSync(paths.calls, 'utf8');
      const grounded = invoke(paths.root, [
        'ground',
        'ttl equals seven',
        '--input',
        output,
        '--base',
        base,
        '--source',
        'first.md',
        '--source',
        'second.md',
        '--prepare',
      ]);
      expect(grounded.stderr).toBe('');
      expect(grounded.status).toBe(0);
      const grounding = JSON.parse(grounded.stdout);
      const bindings: Record<string, string> = grounding.sourceBindings;
      const contextAlias = Object.entries(bindings).find(
        ([, source]) => source === 'third.md',
      )?.[0];
      expect(contextAlias).toBeDefined();
      expect(grounding.prompt).toContain(JSON.stringify(markdown(documents[2]!)));
      expect(grounding.prompt).toContain(
        JSON.stringify({ ...citation(documents[2]!.rule), source: contextAlias }),
      );
      expect(readFileSync(paths.calls, 'utf8')).toBe(beforeGround);
    });
  },
  15000,
);

test('freezes supporting context across update/resume after an explicit comparison transition', async () => {
  await contextProject((paths, input, config) => {
    reviewSources(paths, input);
    const selection: Selection = JSON.parse(
      invoke(paths.root, ['graph', 'compare-plan', '--input', input]).stdout,
    );
    const common = ['graph', 'compare', '--input', input];
    for (const pair of selection.pairs) {
      writeFileSync(paths.candidate, JSON.stringify(pairResponse(pair.sources)));
      expect(
        invoke(paths.root, [...common, '--all', '--codex', paths.binary, '--max-units', '1'])
          .status,
      ).toBe(0);
    }
    const output = join(dirname(paths.root), 'managed.json');
    const update = ['update', '--output', output, '--codex', paths.binary];
    expect(invoke(paths.root, [...update, '--max-units', '0']).status).toBe(0);
    const original = readFileSync(output, 'utf8');
    const calls = readFileSync(paths.calls, 'utf8');
    expect(
      invoke(paths.root, [...update, '--comparison-context', config, '--max-units', '0']).status,
    ).toBe(1);
    const archive = join(dirname(paths.root), 'old-comparisons.json');
    writeFileSync(
      archive,
      invoke(paths.root, [...common, '--export', '--max-bytes', '134217728']).stdout,
    );
    expect(
      invoke(paths.root, [
        ...common,
        '--all',
        '--from',
        input,
        '--reuse',
        archive,
        '--comparison-context',
        config,
        '--max-units',
        '0',
      ]).status,
    ).toBe(0);
    const paused = invoke(paths.root, [
      ...update,
      '--comparison-context',
      config,
      '--max-units',
      '0',
    ]);
    expect(paused.stderr).toBe('');
    expect(JSON.parse(paused.stdout)).toMatchObject({ status: 'partial', phase: 'compare' });
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
    expect(readFileSync(output, 'utf8')).toBe(original);
    writeFileSync(config, '{invalid');
    writeFileSync(paths.candidate, JSON.stringify(pairResponse(['first.md', 'second.md'], true)));
    const resumed = invoke(paths.root, update);
    expect(resumed.stderr).toBe('');
    expect(JSON.parse(resumed.stdout)).toMatchObject({ status: 'admitted' });
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls + 'called\n');
    expect(JSON.parse(invoke(paths.root, [...update, '--max-units', '0']).stdout)).toMatchObject({
      status: 'unchanged',
    });
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls + 'called\n');
  });
}, 15000);
