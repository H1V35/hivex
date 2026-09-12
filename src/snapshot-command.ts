import { parseArgs } from 'node:util';
import { compareSerializedStrings } from './ordering.ts';
import { HivexError } from './errors.ts';
import { ingestionUnits } from './ingestion-units.ts';
import { KnowledgeStore } from './knowledge-store.ts';
import { readKnowledgeSnapshot, writeKnowledgeSnapshot } from './knowledge-snapshot.ts';
import { loadProject } from './documents.ts';
import type { Graph } from './knowledge-model.ts';
import type { Project } from './documents.ts';

const sourceVersion = function sourceVersion([document, version]: [string, string]) {
  return { document, version };
};

const sourceVersions = function sourceVersions(project: Project, graph: Graph) {
  const references = [
    ...Object.entries(graph.documents).map(sourceVersion),
    ...Object.values(graph.units),
    ...graph.decisions,
    ...graph.relationships.flatMap((edge) => edge.evidence),
  ];
  const current = new Set<string>();
  const stale = new Set<string>();
  const unavailable = new Set<string>();
  for (const reference of references) {
    const source = project.documents.find((document) => document.id === reference.document);
    if (source === undefined) {
      unavailable.add(reference.document);
    } else if (source.hash === reference.version) {
      current.add(reference.document);
    } else {
      stale.add(reference.document);
    }
  }
  return {
    current: [...current].filter((id) => !stale.has(id)).toSorted(compareSerializedStrings),
    stale: [...stale].toSorted(compareSerializedStrings),
    unavailable: [...unavailable].toSorted(compareSerializedStrings),
  };
};

const snapshotReport = function snapshotReport(project: Project, graph: Graph, operation: string) {
  const plan = ingestionUnits(project.currentDocuments);
  const pending = plan.units.filter((unit) => {
    const source = project.documents.find((document) => document.id === unit.document);
    return graph.units[unit.id]?.version !== source?.hash;
  });
  const sources = sourceVersions(project, graph);
  const warnings = [...graph.warnings, ...project.warnings, ...plan.warnings];
  const isPartial = [
    pending.length > 0,
    warnings.length > 0,
    sources.stale.length > 0,
    sources.unavailable.length > 0,
    [...graph.decisions, ...graph.relationships].some((entry) => entry.quality !== 'checked'),
  ].includes(true);
  return {
    command: 'snapshot',
    decisions: graph.decisions.length,
    modelCalls: 0,
    operation,
    path: '.hivex/graph.json',
    pendingUnits: pending.map((unit) => unit.id),
    relationships: graph.relationships.length,
    sources,
    status: isPartial ? 'partial' : 'ready',
    warnings,
  };
};

export const snapshotCommand = function snapshotCommand(argumentsList: string[]) {
  const { positionals, values } = parseArgs({
    allowPositionals: true,
    args: argumentsList,
    options: { root: { type: 'string' } },
    strict: true,
  });
  const [, operation] = positionals;
  if (
    positionals.length !== 2 ||
    positionals[0] !== 'snapshot' ||
    (operation !== 'export' && operation !== 'import')
  ) {
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Use snapshot export | import [--root <project>]',
    });
  }
  const project = loadProject(values.root ?? process.cwd());
  const incoming = operation === 'import' ? readKnowledgeSnapshot(project.root) : null;
  if (operation === 'import' && incoming === null) {
    throw new HivexError({
      code: 'SNAPSHOT_NOT_FOUND',
      message: 'No .hivex/graph.json snapshot is available.',
    });
  }
  using store = new KnowledgeStore(project.root, { update: true });
  const graph = incoming ?? store.graph();
  if (incoming === null) {
    writeKnowledgeSnapshot(project.root, graph);
  } else {
    store.importGraph(incoming);
  }
  return snapshotReport(project, graph, operation);
};
