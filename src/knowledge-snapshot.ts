import { lstatSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { randomUUID } from 'node:crypto';
import { HivexError } from './errors.ts';
import { isMarkdownPath } from './markdown.ts';
import { emptyGraph, graphSchema, type Graph } from './knowledge-model.ts';

const MAX_BYTES = 64 * 1024 * 1024;

function portableDocument(id: string) {
  return (
    isMarkdownPath(id) &&
    !id.startsWith('/') &&
    !id.includes('\\') &&
    !id.includes('\0') &&
    !id.split('/').some((part) => ['..', '.', '.git', '.hivex', 'node_modules', ''].includes(part))
  );
}

function validateGraph(graph: Graph) {
  const decisions = new Set(graph.decisions.map((entry) => entry.id));
  const relationships = new Set(graph.relationships.map((entry) => entry.id));
  const invalid =
    decisions.size !== graph.decisions.length ||
    relationships.size !== graph.relationships.length ||
    graph.decisions.some(
      (entry) => entry.quality === 'checked' && entry.lineStart > entry.lineEnd,
    ) ||
    graph.relationships.some(
      (edge) =>
        !decisions.has(edge.from) ||
        !decisions.has(edge.to) ||
        !edge.evidence.length ||
        edge.evidence.some((entry) => entry.lineStart > entry.lineEnd),
    );
  const references = [
    ...Object.keys(graph.documents),
    ...Object.values(graph.units).map((unit) => unit.document),
    ...graph.decisions.map((entry) => entry.document),
    ...graph.relationships.flatMap((edge) => edge.evidence.map((entry) => entry.document)),
    ...graph.warnings.flatMap((warning) =>
      typeof warning === 'string' ? [] : warning.scope.map((entry) => entry.document),
    ),
  ];
  if (invalid || references.some((id) => !portableDocument(id)))
    throw new HivexError({
      code: 'INVALID_SNAPSHOT',
      message:
        'Knowledge snapshot has invalid identities, relationships or project-relative sources.',
    });
  return graph;
}

function snapshotPath(root: string) {
  const directory = join(root, '.hivex');
  const path = join(directory, 'graph.json');
  for (const candidate of [directory, path]) {
    const stat = lstatSync(candidate, { throwIfNoEntry: false });
    if (
      stat?.isSymbolicLink() ||
      (stat && !(candidate === directory ? stat.isDirectory() : stat.isFile()))
    )
      throw new HivexError({
        code: 'INVALID_SNAPSHOT',
        message: 'Knowledge snapshot must use regular project-local files.',
      });
  }
  return path;
}

export function readKnowledgeSnapshot(root: string): Graph | null {
  const path = snapshotPath(root);
  const stat = lstatSync(path, { throwIfNoEntry: false });
  if (!stat) return null;
  if (stat.size > MAX_BYTES)
    throw new HivexError({
      code: 'INVALID_SNAPSHOT',
      message: 'Knowledge snapshot exceeds 64 MiB.',
    });
  try {
    const bytes = readFileSync(path);
    if (bytes.length > MAX_BYTES) throw new Error('Snapshot exceeds size limit');
    const data: unknown = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
    return validateGraph(graphSchema.strict().parse(data));
  } catch {
    throw new HivexError({
      code: 'INVALID_SNAPSHOT',
      message: 'Knowledge snapshot is not a supported graph JSON document.',
    });
  }
}

function ordered(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(ordered);
  if (value && typeof value === 'object')
    return Object.fromEntries(
      Object.entries(value)
        .sort(([a], [b]) => (a < b ? -1 : Number(a > b)))
        .map(([key, entry]) => [key, ordered(entry)]),
    );
  return value;
}

function byId(a: { id: string }, b: { id: string }) {
  if (a.id < b.id) return -1;
  return Number(a.id > b.id);
}

export function writeKnowledgeSnapshot(root: string, value: Graph) {
  const path = snapshotPath(root);
  const graph = validateGraph(graphSchema.parse(value));
  const text =
    JSON.stringify(
      ordered({
        ...graph,
        decisions: graph.decisions.toSorted(byId),
        relationships: graph.relationships.toSorted(byId),
      }),
      null,
      2,
    ) + '\n';
  if (Buffer.byteLength(text) > MAX_BYTES)
    throw new HivexError({
      code: 'INVALID_SNAPSHOT',
      message: 'Knowledge snapshot exceeds 64 MiB.',
    });
  mkdirSync(join(root, '.hivex'), { recursive: true, mode: 0o700 });
  const temporary = join(root, '.hivex', `graph-${randomUUID()}.tmp`);
  try {
    writeFileSync(temporary, text, { flag: 'wx', mode: 0o600 });
    renameSync(temporary, path);
  } finally {
    rmSync(temporary, { force: true });
  }
  return path;
}

export function sharedKnowledge(root: string) {
  return readKnowledgeSnapshot(root) ?? emptyGraph();
}
