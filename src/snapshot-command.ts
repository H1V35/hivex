import { parseArgs } from 'node:util';
import { HivexError } from './errors.ts';
import { loadProject, type Project } from './documents.ts';
import type { Graph } from './knowledge-model.ts';
import { ingestionUnits } from './ingestion-units.ts';
import { KnowledgeStore } from './knowledge-store.ts';
import { readKnowledgeSnapshot, writeKnowledgeSnapshot } from './knowledge-snapshot.ts';

function sourceVersions(project: Project, graph: Graph) {
  const references = [
    ...Object.entries(graph.documents).map(([document, version]) => ({ document, version })),
    ...Object.values(graph.units),
    ...graph.decisions,
    ...graph.relationships.flatMap((edge) => edge.evidence),
  ];
  const current = new Set<string>();
  const stale = new Set<string>();
  const unavailable = new Set<string>();
  for (const reference of references) {
    const source = project.documents.find((document) => document.id === reference.document);
    if (!source) unavailable.add(reference.document);
    else if (source.hash !== reference.version) stale.add(reference.document);
    else current.add(reference.document);
  }
  return {
    current: [...current].filter((id) => !stale.has(id)).sort(),
    stale: [...stale].sort(),
    unavailable: [...unavailable].sort(),
  };
}

function snapshotReport(project: Project, graph: Graph, operation: string) {
  const plan = ingestionUnits(project.documents);
  const pending = plan.units.filter(
    (unit) =>
      graph.units[unit.id]?.version !==
      project.documents.find((document) => document.id === unit.document)?.hash,
  );
  const sources = sourceVersions(project, graph);
  const warnings = [...graph.warnings, ...project.warnings, ...plan.warnings];
  const partial =
    pending.length ||
    warnings.length ||
    sources.stale.length ||
    sources.unavailable.length ||
    [...graph.decisions, ...graph.relationships].some((entry) => entry.quality !== 'checked');
  return {
    command: 'snapshot',
    operation,
    modelCalls: 0,
    path: '.hivex/graph.json',
    status: partial ? 'partial' : 'ready',
    decisions: graph.decisions.length,
    relationships: graph.relationships.length,
    sources,
    pendingUnits: pending.map((unit) => unit.id),
    warnings,
  };
}

export function snapshotCommand(args: string[]) {
  const { positionals, values } = parseArgs({
    args,
    allowPositionals: true,
    strict: true,
    options: { root: { type: 'string' } },
  });
  const operation = positionals[1];
  if (
    positionals.length !== 2 ||
    positionals[0] !== 'snapshot' ||
    (operation !== 'export' && operation !== 'import')
  )
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Use snapshot export | import [--root <project>]',
    });
  const project = loadProject(values.root ?? process.cwd());
  const incoming = operation === 'import' ? readKnowledgeSnapshot(project.root) : null;
  if (operation === 'import' && !incoming)
    throw new HivexError({
      code: 'SNAPSHOT_NOT_FOUND',
      message: 'No .hivex/graph.json snapshot is available.',
    });
  using store = new KnowledgeStore(project.root);
  using _lease = store.updateLease();
  const graph = incoming ?? store.graph();
  if (incoming) store.importGraph(incoming);
  else writeKnowledgeSnapshot(project.root, graph);
  return snapshotReport(project, graph, operation);
}
