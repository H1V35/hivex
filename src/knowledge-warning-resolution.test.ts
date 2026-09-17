import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import nodePath from 'node:path';
import { expect, test } from 'bun:test';
import { loadProject } from './documents.ts';
import { ingestionUnits } from './ingestion-units.ts';
import {
  applyCheck,
  applyExtraction,
  checkSchema,
  emptyGraph,
  extractionSchema,
  graphSchema,
  warningId,
} from './knowledge-model.ts';
import { readKnowledgeSnapshot, writeKnowledgeSnapshot } from './knowledge-snapshot.ts';
import { knowledgeCommand as referenceKnowledgeCommand } from './knowledge.ts';
import { KnowledgeStore } from './knowledge-store.ts';
import { snapshotCommand as referenceSnapshotCommand } from './snapshot-command.ts';
import { warningCommand as referenceWarningCommand } from './knowledge-warnings.ts';
import type { Document } from './documents.ts';
import type { Graph, WarningScope } from './knowledge-model.ts';

const rustBinary = process.env.HIVEX_TEST_BINARY;
const nodeWarningsKey = 'NODE_NO_WARNINGS';

const invokeRust = function invokeRust(argumentsList: string[]): unknown {
  if (rustBinary === undefined) {
    throw new Error('HIVEX_TEST_BINARY is required');
  }
  const result = spawnSync(rustBinary, argumentsList, {
    encoding: 'utf-8',
    env: { ...process.env, [nodeWarningsKey]: '1' },
    timeout: 10_000,
  });
  if (result.status !== 0 || result.stdout === '') {
    throw new Error(result.stderr || 'Rust command failed');
  }
  const value: unknown = JSON.parse(result.stdout);
  return value;
};

const runWarningCommand = function runWarningCommand(argumentsList: string[]): unknown {
  return rustBinary === undefined
    ? referenceWarningCommand(argumentsList)
    : invokeRust(argumentsList);
};

const runSnapshotCommand = function runSnapshotCommand(argumentsList: string[]): unknown {
  return rustBinary === undefined
    ? referenceSnapshotCommand(argumentsList)
    : invokeRust(argumentsList);
};

const runKnowledgeCommand = function runKnowledgeCommand(argumentsList: string[]): unknown {
  return rustBinary === undefined
    ? referenceKnowledgeCommand(argumentsList)
    : invokeRust(argumentsList);
};

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const arrayField = function arrayField(value: unknown, name: string): unknown[] {
  if (!isRecord(value) || !Array.isArray(value[name])) {
    throw new Error(`Expected array field ${name}`);
  }
  return value[name];
};

interface Resolution {
  evidence: WarningScope[];
  id: string;
  reason: string;
}

interface Fixture {
  document: Document;
  graph: Graph;
  root: string;
  snapshot: string;
}

const workArguments = function workArguments(snapshot: string) {
  return {
    key: 'synthetic-work',
    kind: 'update' as const,
    maxCalls: 3,
    maxInputBytes: 131_072,
    remaining: [],
    snapshot,
  };
};

const workCounts = function workCounts(root: string, snapshot: string) {
  using store = new KnowledgeStore(root, { update: true });
  const work = store.begin(workArguments(snapshot));
  return {
    cacheHits: work.cacheHits,
    calls: work.calls,
    id: work.id,
    inputBytes: work.inputBytes,
    maxCalls: work.maxCalls,
    maxInputBytes: work.maxInputBytes,
    totalTokens: work.totalTokens,
  };
};

const createFixture = function createFixture(root: string): Fixture {
  writeFileSync(
    nodePath.join(root, 'notes.md'),
    '# Notes\n\nThe first rule preserves evidence.\n\nThe second rule preserves evidence.\n',
    'utf-8'
  );
  const project = loadProject(root);
  const document = project.currentDocuments.at(0);
  if (document === undefined) {
    throw new Error('Expected the synthetic source document');
  }
  const extraction = extractionSchema.parse({
    decisions: [
      {
        conditions: [],
        document: document.id,
        exceptions: [],
        id: 'first-rule',
        kind: 'decision',
        lineEnd: 3,
        lineStart: 3,
        reason: 'The first rule is sourced.',
        status: 'current',
        text: 'The first rule preserves evidence.',
      },
      {
        conditions: [],
        document: document.id,
        exceptions: [],
        id: 'second-rule',
        kind: 'decision',
        lineEnd: 5,
        lineStart: 5,
        reason: 'The second rule is sourced.',
        status: 'current',
        text: 'The second rule preserves evidence.',
      },
    ],
    relationships: [
      {
        evidence: [{ document: document.id, lineEnd: 3, lineStart: 3 }],
        from: 'first-rule',
        id: 'supports-second-rule',
        reason: 'The first rule supports the second.',
        to: 'second-rule',
        type: 'supports',
      },
    ],
    uncertainties: ['The first warning needs closure.', 'The second warning stays active.'],
  });
  const extracted = applyExtraction({
    batch: 'synthetic-batch',
    documents: [document],
    extraction,
    graph: emptyGraph(),
  });
  const checked = applyCheck(extracted, checkSchema.parse({ findings: [] }), {
    batch: 'synthetic-batch',
  });
  const plan = ingestionUnits(project.currentDocuments);
  const graph = graphSchema.parse({
    ...checked,
    documents: Object.fromEntries(project.documents.map((entry) => [entry.id, entry.hash])),
    units: Object.fromEntries(
      plan.units.map((unit) => [unit.id, { document: unit.document, version: document.hash }])
    ),
  });
  {
    using store = new KnowledgeStore(root, { update: true });
    store.saveGraph(graph);
    const work = store.begin(workArguments(project.snapshot));
    work.cacheHits = 2;
    work.calls = 3;
    work.inputBytes = 1234;
    work.status = 'done';
    work.totalTokens = 17;
    store.save(work);
  }
  return { document, graph, root, snapshot: project.snapshot };
};

const temporaryProject = async function temporaryProject(
  run: (fixture: Fixture) => void | Promise<void>
) {
  const root = mkdtempSync(nodePath.join(tmpdir(), 'hivex-warning-resolution-'));
  try {
    await run(createFixture(root));
  } finally {
    rmSync(root, { force: true, recursive: true });
  }
};

const messagesOf = function messagesOf(warnings: unknown[]) {
  return warnings.map((warning) => {
    if (typeof warning === 'string') {
      return warning;
    }
    if (isRecord(warning) && typeof warning.message === 'string') {
      return warning.message;
    }
    throw new Error('Expected a warning message');
  });
};

const warningFor = function warningFor(graph: Graph, message: string) {
  const warning = graph.warnings.find(
    (entry) => typeof entry !== 'string' && entry.message === message
  );
  if (warning === undefined || typeof warning === 'string') {
    throw new Error(`Expected warning ${message}`);
  }
  return warning;
};

const citationFor = function citationFor(
  document: Document,
  lineStart = 3,
  lineEnd = lineStart
): WarningScope {
  return { document: document.id, lineEnd, lineStart, version: document.hash };
};

const writeManifest = function writeManifest(root: string, resolutions: Resolution[]) {
  const file = nodePath.join(root, 'resolutions.json');
  writeFileSync(file, JSON.stringify(resolutions), 'utf-8');
  return file;
};

test('resolves current evidence while preserving knowledge and reactivating after a source change', async () => {
  await temporaryProject(async ({ document, graph, root, snapshot }) => {
    const firstMessage = 'The first warning needs closure.';
    const secondMessage = 'The second warning stays active.';
    const first = warningFor(graph, firstMessage);
    const second = warningFor(graph, secondMessage);
    const firstId = warningId(first);
    const secondId = warningId(second);
    const evidence = [citationFor(document)];
    const reason = 'Current source evidence closes the first warning.';
    const beforeWork = workCounts(root, snapshot);

    expect(runWarningCommand(['warnings', '--root', root])).toMatchObject({
      warningSummary: { limitations: 2, resolved: 0 },
    });
    const manifest = writeManifest(root, [{ evidence, id: firstId, reason }]);
    const resolved = runWarningCommand(['warnings', '--resolve', manifest, '--root', root]);

    expect(resolved).toMatchObject({
      resolved: 1,
      warningSummary: { limitations: 1, resolved: 1 },
    });
    expect(arrayField(resolved, 'warnings')).toHaveLength(1);
    expect(
      arrayField(resolved, 'warnings').find(
        (warning) => isRecord(warning) && warning.id === secondId
      )
    ).toMatchObject({
      id: secondId,
      message: secondMessage,
      state: 'active',
    });
    const all = runWarningCommand(['warnings', '--all', '--root', root]);
    expect(
      arrayField(all, 'warnings').find((warning) => isRecord(warning) && warning.id === firstId)
    ).toMatchObject({
      id: firstId,
      kind: 'limitation',
      message: firstMessage,
      resolution: { evidence, reason },
      scope: first.scope,
      state: 'resolved',
    });

    using store = new KnowledgeStore(root, { readonly: true });
    const after = store.graph();
    expect(after.decisions).toEqual(graph.decisions);
    expect(after.relationships).toEqual(graph.relationships);
    const reextracted = applyExtraction({
      batch: 'later-batch',
      documents: [document],
      extraction: extractionSchema.parse({
        decisions: [],
        relationships: [],
        uncertainties: [firstMessage, secondMessage],
      }),
      graph: after,
    });
    expect(reextracted.warnings).toHaveLength(2);
    expect(warningFor(reextracted, firstMessage).resolution).toEqual({ evidence, reason });
    expect(workCounts(root, snapshot)).toEqual(beforeWork);

    const status = await runKnowledgeCommand(['status', '--root', root]);
    expect(status).toMatchObject({
      warningSummary: { limitations: 1, resolved: 1, sources: 0 },
    });
    expect(messagesOf(arrayField(status, 'warnings'))).toContain(secondMessage);
    expect(messagesOf(arrayField(status, 'warnings'))).not.toContain(firstMessage);

    const search = await runKnowledgeCommand(['search', 'evidence', '--root', root]);
    expect(messagesOf(arrayField(search, 'warnings'))).toContain(secondMessage);
    expect(messagesOf(arrayField(search, 'warnings'))).not.toContain(firstMessage);

    const exported = runSnapshotCommand(['snapshot', 'export', '--root', root]);
    expect(exported).toMatchObject({
      status: 'partial',
      warningSummary: { limitations: 1, resolved: 1 },
    });
    expect(messagesOf(arrayField(exported, 'warnings'))).toContain(secondMessage);
    expect(messagesOf(arrayField(exported, 'warnings'))).not.toContain(firstMessage);

    writeFileSync(nodePath.join(root, document.path), `${document.text}Changed source.\n`, 'utf-8');
    const reactivated = runWarningCommand(['warnings', '--all', '--root', root]);
    expect(reactivated).toMatchObject({ warningSummary: { limitations: 2, resolved: 0 } });
    expect(
      arrayField(reactivated, 'warnings').find(
        (warning) => isRecord(warning) && warning.id === firstId
      )
    ).toMatchObject({
      id: firstId,
      resolution: { evidence, reason },
      state: 'active',
    });
  });
});

test('rejects invalid IDs, citations, and versions without partially applying a manifest', async () => {
  await temporaryProject(({ document, graph, root, snapshot }) => {
    const first = warningFor(graph, 'The first warning needs closure.');
    const second = warningFor(graph, 'The second warning stays active.');
    const firstResolution = {
      evidence: [citationFor(document)],
      id: warningId(first),
      reason: 'This valid entry must not be committed alone.',
    };
    const validEvidence = [citationFor(document)];
    const invalidManifests: Resolution[][] = [
      [firstResolution, { evidence: validEvidence, id: 'missing-warning', reason: 'Unknown ID.' }],
      [firstResolution, { ...firstResolution }],
      [
        firstResolution,
        {
          evidence: [citationFor(document, 99)],
          id: warningId(second),
          reason: 'Out of range.',
        },
      ],
      [
        firstResolution,
        {
          evidence: [{ ...citationFor(document), version: 'stale-version' }],
          id: warningId(second),
          reason: 'Stale version.',
        },
      ],
    ];
    const beforeWork = workCounts(root, snapshot);

    for (const resolutions of invalidManifests) {
      const manifest = writeManifest(root, resolutions);
      expect(() =>
        runWarningCommand(['warnings', '--resolve', manifest, '--root', root])
      ).toThrow();
      using store = new KnowledgeStore(root, { readonly: true });
      expect(store.graph()).toEqual(graph);
      expect(workCounts(root, snapshot)).toEqual(beforeWork);
    }
  });
});

test('roundtrips resolutions through snapshots and accepts a legacy warning without resolution', async () => {
  await temporaryProject(({ document, graph, root }) => {
    const firstMessage = 'The first warning needs closure.';
    const first = warningFor(graph, firstMessage);
    const evidence = [citationFor(document)];
    const reason = 'Snapshot evidence closes the first warning.';
    const manifest = writeManifest(root, [{ evidence, id: warningId(first), reason }]);
    runWarningCommand(['warnings', '--resolve', manifest, '--root', root]);

    const exported = runSnapshotCommand(['snapshot', 'export', '--root', root]);
    expect(exported).toMatchObject({ warningSummary: { limitations: 1, resolved: 1 } });
    const snapshot = readKnowledgeSnapshot(root);
    if (snapshot === null) {
      throw new Error('Expected the exported snapshot');
    }
    expect(warningFor(snapshot, firstMessage)).toMatchObject({
      resolution: { evidence, reason },
    });

    const clone = mkdtempSync(nodePath.join(tmpdir(), 'hivex-warning-clone-'));
    try {
      writeFileSync(nodePath.join(clone, document.id), document.text);
      writeKnowledgeSnapshot(clone, snapshot);
      expect(runWarningCommand(['warnings', '--root', clone])).toMatchObject({
        warningSummary: { limitations: 1, resolved: 1 },
      });
      expect(existsSync(nodePath.join(clone, '.hivex', 'knowledge.sqlite'))).toBe(false);
    } finally {
      rmSync(clone, { force: true, recursive: true });
    }
    const legacy = structuredClone(snapshot);
    const legacyWarning = warningFor(legacy, firstMessage);
    delete legacyWarning.resolution;
    writeFileSync(
      nodePath.join(root, '.hivex', 'graph.json'),
      `${JSON.stringify(legacy)}\n`,
      'utf-8'
    );

    const imported = runSnapshotCommand(['snapshot', 'import', '--root', root]);
    expect(imported).toMatchObject({
      operation: 'import',
      warningSummary: { limitations: 2, resolved: 0 },
    });
    expect(messagesOf(arrayField(imported, 'warnings'))).toContain(firstMessage);
    expect(readKnowledgeSnapshot(root)).toEqual(legacy);
    using store = new KnowledgeStore(root, { readonly: true });
    expect(warningFor(store.graph(), firstMessage)).not.toHaveProperty('resolution');
  });
});
