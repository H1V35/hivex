import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import nodePath from 'node:path';
import { expect, test } from 'bun:test';
import {
  emptyGraph,
  parseGraph,
  readGraph,
  readLatestWork,
  seedWork,
  updateWork,
  writeGraph,
} from './rust-fixtures.ts';
import type { GraphRecord } from './rust-fixtures.ts';

interface Document {
  hash: string;
  historical: boolean;
  id: string;
  path: string;
  text: string;
}
type Graph = GraphRecord;
interface WarningScope {
  document: string;
  lineEnd: number;
  lineStart: number;
  version: string;
}

const rustBinary = process.env.HIVEX_TEST_BINARY;
if (rustBinary === undefined) {
  throw new Error('HIVEX_TEST_BINARY is required');
}
const nodeWarningsKey = 'NODE_NO_WARNINGS';

const invokeRust = function invokeRust(argumentsList: string[]): unknown {
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
  return invokeRust(argumentsList);
};

const runSnapshotCommand = function runSnapshotCommand(argumentsList: string[]): unknown {
  return invokeRust(argumentsList);
};

const runKnowledgeCommand = function runKnowledgeCommand(argumentsList: string[]): unknown {
  return invokeRust(argumentsList);
};

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const arrayField = function arrayField(value: unknown, name: string): unknown[] {
  if (!isRecord(value) || !Array.isArray(value[name])) {
    throw new Error(`Expected array field ${name}`);
  }
  return value[name];
};

const stringField = function stringField(value: unknown, name: string): string {
  if (!isRecord(value) || typeof value[name] !== 'string') {
    throw new Error(`Expected string field ${name}`);
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

const workCounts = function workCounts(root: string, _snapshot: string) {
  const work = readLatestWork(root);
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
  const source = arrayField(invokeRust(['sources', '--root', root]), 'documents').at(0);
  if (!isRecord(source)) {
    throw new Error('Expected the synthetic source document');
  }
  const document: Document = {
    hash: stringField(source, 'hash'),
    historical: false,
    id: stringField(source, 'id'),
    path: stringField(source, 'path'),
    text: readFileSync(nodePath.join(root, stringField(source, 'path')), 'utf-8'),
  };
  const graph = emptyGraph();
  graph.documents[document.id] = document.hash;
  graph.units[`${document.id}:1-5`] = {
    document: document.id,
    version: document.hash,
  };
  graph.decisions = [
    {
      batch: 'synthetic-batch',
      conditions: [],
      document: document.id,
      exceptions: [],
      id: 'first-rule',
      kind: 'decision',
      lineEnd: 3,
      lineStart: 3,
      localId: 'first-rule',
      quality: 'checked',
      reason: 'The first rule is sourced.',
      status: 'current',
      text: 'The first rule preserves evidence.',
      version: document.hash,
    },
    {
      batch: 'synthetic-batch',
      conditions: [],
      document: document.id,
      exceptions: [],
      id: 'second-rule',
      kind: 'decision',
      lineEnd: 5,
      lineStart: 5,
      localId: 'second-rule',
      quality: 'checked',
      reason: 'The second rule is sourced.',
      status: 'current',
      text: 'The second rule preserves evidence.',
      version: document.hash,
    },
  ];
  graph.relationships = [
    {
      batch: 'synthetic-batch',
      evidence: [
        {
          document: document.id,
          lineEnd: 3,
          lineStart: 3,
          version: document.hash,
        },
      ],
      from: 'first-rule',
      id: 'supports-second-rule',
      localId: 'supports-second-rule',
      quality: 'checked',
      reason: 'The first rule supports the second.',
      to: 'second-rule',
      type: 'supports',
    },
  ];
  graph.warnings = [
    {
      kind: 'limitation',
      message: 'The first warning needs closure.',
      scope: [
        {
          document: document.id,
          lineEnd: 3,
          lineStart: 3,
          version: document.hash,
        },
      ],
    },
    {
      kind: 'limitation',
      message: 'The second warning stays active.',
      scope: [
        {
          document: document.id,
          lineEnd: 5,
          lineStart: 5,
          version: document.hash,
        },
      ],
    },
  ];
  writeGraph(root, graph);
  const snapshot = stringField(invokeRust(['sources', '--root', root]), 'snapshot');
  const workId = seedWork(root, {
    key: 'synthetic-work',
    kind: 'update',
    remaining: [],
    snapshot,
  });
  updateWork(root, workId, (work) => {
    work.cacheHits = 2;
    work.calls = 3;
    work.inputBytes = 1234;
    work.status = 'done';
    work.totalTokens = 17;
  });
  return { document, graph, root, snapshot };
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

const warningIdFor = function warningIdFor(root: string, message: string) {
  const report = runWarningCommand(['warnings', '--all', '--root', root]);
  const warning = arrayField(report, 'warnings').find(
    (entry) => isRecord(entry) && entry.message === message
  );
  if (!isRecord(warning)) {
    throw new Error(`Expected warning ${message}`);
  }
  return stringField(warning, 'id');
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
    warningFor(graph, secondMessage);
    const firstId = warningIdFor(root, firstMessage);
    const secondId = warningIdFor(root, secondMessage);
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

    const after = readGraph(root);
    expect(after.decisions).toEqual(graph.decisions);
    expect(after.relationships).toEqual(graph.relationships);
    expect(after.warnings).toHaveLength(2);
    expect(warningFor(after, firstMessage).resolution).toEqual({
      evidence,
      reason,
    });
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
    expect(reactivated).toMatchObject({
      warningSummary: { limitations: 2, resolved: 0 },
    });
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
    const firstResolution = {
      evidence: [citationFor(document)],
      id: warningIdFor(root, 'The first warning needs closure.'),
      reason: 'This valid entry must not be committed alone.',
    };
    const validEvidence = [citationFor(document)];
    const invalidManifests: Resolution[][] = [
      [
        firstResolution,
        {
          evidence: validEvidence,
          id: 'missing-warning',
          reason: 'Unknown ID.',
        },
      ],
      [firstResolution, { ...firstResolution }],
      [
        firstResolution,
        {
          evidence: [citationFor(document, 99)],
          id: warningIdFor(root, 'The second warning stays active.'),
          reason: 'Out of range.',
        },
      ],
      [
        firstResolution,
        {
          evidence: [{ ...citationFor(document), version: 'stale-version' }],
          id: warningIdFor(root, 'The second warning stays active.'),
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
      expect(readGraph(root)).toEqual(graph);
      expect(workCounts(root, snapshot)).toEqual(beforeWork);
    }
  });
});

test('roundtrips resolutions through snapshots and accepts a legacy warning without resolution', async () => {
  await temporaryProject(({ document, graph, root }) => {
    const firstMessage = 'The first warning needs closure.';
    warningFor(graph, firstMessage);
    const evidence = [citationFor(document)];
    const reason = 'Snapshot evidence closes the first warning.';
    const manifest = writeManifest(root, [
      { evidence, id: warningIdFor(root, firstMessage), reason },
    ]);
    runWarningCommand(['warnings', '--resolve', manifest, '--root', root]);

    const exported = runSnapshotCommand(['snapshot', 'export', '--root', root]);
    expect(exported).toMatchObject({
      warningSummary: { limitations: 1, resolved: 1 },
    });
    const snapshot = parseGraph(
      JSON.parse(readFileSync(nodePath.join(root, '.hivex', 'graph.json'), 'utf-8'))
    );
    expect(warningFor(snapshot, firstMessage)).toMatchObject({
      resolution: { evidence, reason },
    });

    const clone = mkdtempSync(nodePath.join(tmpdir(), 'hivex-warning-clone-'));
    try {
      writeFileSync(nodePath.join(clone, document.id), document.text);
      mkdirSync(nodePath.join(clone, '.hivex'), { recursive: true });
      writeFileSync(
        nodePath.join(clone, '.hivex', 'graph.json'),
        `${JSON.stringify(snapshot)}\n`,
        'utf-8'
      );
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
    expect(JSON.parse(readFileSync(nodePath.join(root, '.hivex', 'graph.json'), 'utf-8'))).toEqual(
      legacy
    );
    expect(warningFor(readGraph(root), firstMessage)).not.toHaveProperty('resolution');
  });
});
