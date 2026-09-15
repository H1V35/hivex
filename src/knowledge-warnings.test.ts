import { expect, test } from 'bun:test';
import {
  applyCheck,
  applyExtraction,
  checkSchema,
  emptyGraph,
  extractionSchema,
  graphSchema,
  warningSummary,
} from './knowledge-model.ts';
import type { Document } from './documents.ts';

const document: Document = {
  hash: 'v1',
  historical: false,
  id: 'notes.md',
  links: [],
  path: 'notes.md',
  status: null,
  text: '# Notes\n\nKeep evidence.\n',
  title: 'Notes',
};
const evidence = { document: document.id, lineEnd: 3, lineStart: 3 };
const decision = function decision(id: string) {
  return {
    conditions: [],
    document: document.id,
    exceptions: [],
    id,
    kind: 'decision' as const,
    lineEnd: 3,
    lineStart: 3,
    reason: 'Keep the evidence.',
    status: 'current' as const,
    text: `Decision ${id}.`,
  };
};

test('graphSchema roundtrips warning metadata while retaining legacy shapes', () => {
  const scope = [{ ...evidence, version: document.hash }];
  const graph = {
    ...emptyGraph(),
    warnings: [
      { kind: 'limitation' as const, message: 'Need context.', scope },
      { kind: 'finding' as const, message: 'Review this.', scope, target: 'd1' },
      { kind: 'validation' as const, message: 'Invalid evidence.', scope },
      'Legacy warning',
      { message: 'Legacy object warning.', scope },
    ],
  };

  const roundTripped = graphSchema.parse(structuredClone(graph));

  expect(roundTripped.warnings).toEqual(graph.warnings);
});

test('warningSummary counts legacy warnings as unknown without deduplicating text', () => {
  const graph = graphSchema.parse({
    ...emptyGraph(),
    warnings: [
      'same warning',
      'same warning',
      { message: 'same warning', scope: [] },
      { kind: 'limitation', message: 'same warning', scope: [] },
      { kind: 'limitation', message: 'same warning', scope: [] },
      { kind: 'finding', message: 'same warning', scope: [], target: 'd1' },
      { kind: 'validation', message: 'same warning', scope: [] },
    ],
  });

  expect(warningSummary(graph.warnings)).toEqual({
    findings: 1,
    limitations: 2,
    resolved: 0,
    unknown: 3,
    validation: 1,
  });
});

test('applyExtraction labels limitations and separates validation diagnostics', () => {
  const extraction = extractionSchema.parse({
    decisions: [decision('d1'), decision('d2')],
    relationships: [
      {
        evidence: [evidence],
        from: 'd1',
        id: 'unknown-endpoint',
        reason: 'Connect the decisions.',
        to: 'missing',
        type: 'supports',
      },
      {
        evidence: [evidence],
        from: 'd1',
        id: 'duplicate',
        reason: 'Connect the decisions.',
        to: 'd2',
        type: 'supports',
      },
      {
        evidence: [evidence],
        from: 'd1',
        id: 'duplicate',
        reason: 'Connect the decisions.',
        to: 'd2',
        type: 'supports',
      },
      {
        evidence: [{ document: document.id, lineEnd: 99, lineStart: 99 }],
        from: 'd1',
        id: 'invalid-evidence',
        reason: 'Connect the decisions.',
        to: 'd2',
        type: 'supports',
      },
    ],
    uncertainties: ['Need more context.'],
  });
  const graph = applyExtraction({
    batch: 'batch-1',
    documents: [document],
    extraction,
    graph: emptyGraph(),
  });

  const warnings = graph.warnings.flatMap((warning) =>
    typeof warning === 'string' ? [] : [`${warning.kind}:${warning.message}`]
  );
  expect(warnings).toEqual([
    'limitation:Need more context.',
    'validation:Relationship unknown-endpoint has an unknown endpoint.',
    'validation:Relationship duplicate has a duplicate ID.',
    'validation:Relationship invalid-evidence has invalid evidence.',
  ]);
  expect(graph.relationships).toHaveLength(1);
});

test('applyCheck records a finding warning with its target', () => {
  const extraction = extractionSchema.parse({
    decisions: [decision('d1')],
    relationships: [],
    uncertainties: [],
  });
  const graph = applyExtraction({
    batch: 'batch-1',
    documents: [document],
    extraction,
    graph: emptyGraph(),
  });
  const check = checkSchema.parse({
    findings: [{ reason: 'Decision needs review.', target: 'd1' }],
  });

  const checked = applyCheck(graph, check, { batch: 'batch-1' });

  expect(checked.warnings.at(-1)).toMatchObject({
    kind: 'finding',
    message: 'Decision needs review.',
    target: 'd1',
  });
  expect(checked.decisions[0]?.quality).toBe('uncertain');
});
