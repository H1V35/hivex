import { expect, test } from 'bun:test';
import {
  applyCheck,
  checkImpact,
  checkSchema,
  emptyGraph,
  graphSchema,
} from './knowledge-model.ts';
import type { Graph } from './knowledge-model.ts';

interface DecisionOptions {
  batch: string;
  document: string;
  id: string;
  localId: string;
  quality?: 'checked' | 'uncertain';
}

const decision = function decision(options: DecisionOptions) {
  return {
    batch: options.batch,
    conditions: [],
    document: options.document,
    exceptions: [],
    id: options.id,
    kind: 'decision' as const,
    lineEnd: 1,
    lineStart: 1,
    localId: options.localId,
    quality: options.quality ?? 'checked',
    reason: `Reason for ${options.id}.`,
    status: 'current' as const,
    text: `Decision ${options.id}.`,
    version: 'v1',
  };
};

interface RelationshipOptions {
  batch: string;
  from: string;
  id: string;
  localId: string;
  quality?: 'checked' | 'uncertain';
  to: string;
}

const relationship = function relationship(options: RelationshipOptions) {
  return {
    batch: options.batch,
    evidence: [{ document: 'current.md', lineEnd: 1, lineStart: 1, version: 'v1' }],
    from: options.from,
    id: options.id,
    localId: options.localId,
    quality: options.quality ?? 'checked',
    reason: `Reason for ${options.id}.`,
    to: options.to,
    type: 'supports' as const,
  };
};

const graphFor = function graphFor(
  options: { neighborQuality?: 'checked' | 'uncertain' } = {}
): Graph {
  return graphSchema.parse({
    ...emptyGraph(),
    decisions: [
      decision({
        batch: 'batch-current',
        document: 'current.md',
        id: 'd-current',
        localId: 'current-local',
      }),
      decision({
        batch: 'batch-current',
        document: 'current.md',
        id: 'd-other',
        localId: 'other-local',
      }),
      decision({
        batch: 'batch-previous',
        document: 'neighbor.md',
        id: 'd-neighbor',
        localId: 'neighbor-local',
        quality: options.neighborQuality,
      }),
    ],
    documents: { 'current.md': 'v1', 'neighbor.md': 'v1' },
    relationships: [
      relationship({
        batch: 'batch-current',
        from: 'd-current',
        id: 'r-current',
        localId: 'current-relationship',
        to: 'd-neighbor',
      }),
      relationship({
        batch: 'batch-previous',
        from: 'd-neighbor',
        id: 'r-preexisting',
        localId: 'previous-relationship',
        to: 'd-current',
      }),
    ],
  });
};

const checkFor = function checkFor(...targets: string[]) {
  return checkSchema.parse({
    findings: targets.map((target) => ({ reason: `Finding for ${target}.`, target })),
  });
};

test('checkImpact ignores inherited uncertainty while applyCheck preserves its propagation', () => {
  const graph = graphFor({ neighborQuality: 'uncertain' });
  const check = checkFor();

  expect(checkImpact(graph, check, { batch: 'batch-current' })).toEqual({
    decisionIds: new Set<string>(),
    isUncertainBatch: false,
    relationshipIds: new Set<string>(),
  });

  const applied = applyCheck(graph, check, { batch: 'batch-current' });
  expect(applied.decisions.find((entry) => entry.id === 'd-neighbor')?.quality).toBe('uncertain');
  expect(applied.relationships.find((entry) => entry.id === 'r-current')?.quality).toBe(
    'uncertain'
  );
});

test.each(['d-current', 'current-local'])(
  'resolves a current node finding to its links: %s',
  (target) => {
    const graph = graphFor();
    const impact = checkImpact(graph, checkFor(target), { batch: 'batch-current' });

    expect(impact.decisionIds).toEqual(new Set(['d-current']));
    expect(impact.relationshipIds).toEqual(new Set(['r-current', 'r-preexisting']));
    expect(impact.isUncertainBatch).toBe(false);

    const applied = applyCheck(graph, checkFor(target), { batch: 'batch-current' });
    expect(applied.relationships.find((entry) => entry.id === 'r-current')?.quality).toBe(
      'uncertain'
    );
    expect(applied.relationships.find((entry) => entry.id === 'r-preexisting')?.quality).toBe(
      'checked'
    );
  }
);

test.each(['unknown-target', 'batch'])('keeps %s conservative for the current batch', (target) => {
  const graph = graphFor();
  const check = checkFor(target);
  const impact = checkImpact(graph, check, { batch: 'batch-current' });

  expect(impact.decisionIds).toEqual(new Set<string>());
  expect(impact.relationshipIds).toEqual(new Set<string>());
  expect(impact.isUncertainBatch).toBe(true);

  const applied = applyCheck(graph, check, { batch: 'batch-current' });
  expect(
    applied.decisions
      .filter((entry) => entry.batch === 'batch-current')
      .every((entry) => entry.quality === 'uncertain')
  ).toBe(true);
  expect(applied.relationships.find((entry) => entry.id === 'r-current')?.quality).toBe(
    'uncertain'
  );
  expect(applied.relationships.find((entry) => entry.id === 'r-preexisting')?.quality).toBe(
    'checked'
  );
});

test('resolves current document findings and scoped known targets', () => {
  const graph = graphFor();
  const impact = checkImpact(graph, checkFor('current.md'), {
    batch: 'batch-current',
    scope: [{ document: 'context.md', lineEnd: 1, lineStart: 1, version: 'v1' }],
  });

  expect(impact.decisionIds).toEqual(new Set(['d-current', 'd-other']));
  expect(impact.relationshipIds).toEqual(new Set(['r-current', 'r-preexisting']));
  expect(impact.isUncertainBatch).toBe(false);
  expect(
    checkImpact(graph, checkFor('context.md'), {
      batch: 'batch-current',
      scope: [{ document: 'context.md', lineEnd: 1, lineStart: 1, version: 'v1' }],
    }).isUncertainBatch
  ).toBe(false);
});

test('applies a document finding to retained endpoints without changing their provenance', () => {
  const graph = graphFor();
  const check = checkFor('neighbor.md');
  const options = {
    batch: 'batch-current',
    scope: [{ document: 'neighbor.md', lineEnd: 1, lineStart: 1, version: 'v1' }],
  };
  const impact = checkImpact(graph, check, options);
  expect(impact.decisionIds).toEqual(new Set(['d-neighbor']));
  expect(impact.relationshipIds).toEqual(new Set(['r-current', 'r-preexisting']));
  expect(impact.isUncertainBatch).toBe(false);
  const applied = applyCheck(graph, check, options);
  expect(applied.decisions.find((entry) => entry.id === 'd-neighbor')).toMatchObject({
    batch: 'batch-previous',
    quality: 'uncertain',
  });
  expect(applied.relationships.find((entry) => entry.id === 'r-current')?.quality).toBe(
    'uncertain'
  );
  expect(applied.relationships.find((entry) => entry.id === 'r-preexisting')?.quality).toBe(
    'checked'
  );
});
