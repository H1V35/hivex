import { HivexError } from '../errors.ts';
import { rankLexically } from '../retrieval/lexical.ts';
import type { checkGraph, readGraph } from './verify.ts';

type LoadedGraph = ReturnType<typeof readGraph>;
type Check = ReturnType<typeof checkGraph>;

export function budgeted<T>(response: T, maxBytes: number) {
  const requiredBytes = Buffer.byteLength(JSON.stringify(response)) + 1;
  if (requiredBytes > maxBytes)
    throw new HivexError({
      code: 'GRAPH_OUTPUT_BUDGET',
      message: 'The complete graph response does not fit; increase --max-bytes',
      details: { requiredBytes },
    });
  return response;
}

export function readNode(input: LoadedGraph, id: string, check: Check, maxBytes: number) {
  const node = input.nodes.get(id);
  if (!node)
    throw new HivexError({
      code: 'GRAPH_NODE_NOT_FOUND',
      message: 'The claim is not in this graph snapshot',
    });
  const source = input.sources.get(node.source);
  if (!source)
    throw new HivexError({ code: 'GRAPH_INVALID', message: 'The claim source is missing' });
  return budgeted(
    {
      ...check,
      operation: 'read',
      node,
      source: {
        id: source.id,
        path: source.path,
        contentHash: source.contentHash,
        section: source.section,
        authority: source.authority,
      },
      evidenceCompleteness: 'complete-node',
    },
    maxBytes,
  );
}

export function searchGraph(options: {
  input: LoadedGraph;
  query: string;
  check: Check;
  limit: number;
  maxBytes: number;
}) {
  const records = options.input.graph.nodes.map((node) => ({
    id: node.id,
    title: node.statement.text,
    content: [
      node.source,
      ...node.statement.conditions,
      ...node.statement.exceptions,
      ...node.statement.evidence.map((evidence) => evidence.quote),
    ].join('\n'),
  }));
  const ranked = rankLexically(records, options.query);
  const results: {
    id: string;
    source: string;
    kind: string;
    preview: string | null;
    bm25: number;
    evidenceCompleteness: string;
  }[] = [];
  const response = () => ({
    ...options.check,
    operation: 'search',
    results,
    truncated: results.length < ranked.length,
  });
  for (const match of ranked.slice(0, options.limit)) {
    const node = options.input.nodes.get(match.id);
    if (!node)
      throw new HivexError({ code: 'GRAPH_INVALID', message: 'A ranked claim is missing' });
    results.push({
      id: node.id,
      source: node.source,
      kind: node.statement.kind,
      preview: Buffer.byteLength(node.statement.text) <= 384 ? node.statement.text : null,
      bm25: match.score,
      evidenceCompleteness: 'not-opened',
    });
    if (Buffer.byteLength(JSON.stringify(response())) + 1 > options.maxBytes) {
      results.pop();
      break;
    }
  }
  return budgeted(response(), options.maxBytes);
}

function neighborOffset(cursor: string | undefined, hash: string, id: string, length: number) {
  if (cursor === undefined) return 0;
  const match = /^n1\.([a-f0-9]{64})\.([a-f0-9]{64})\.([0-9]+)$/.exec(cursor);
  const offset = Number(match?.[3]);
  if (
    !match ||
    match[1] !== hash ||
    match[2] !== id ||
    !Number.isSafeInteger(offset) ||
    offset >= length
  )
    throw new HivexError({
      code: 'INVALID_CURSOR',
      message: 'Resume the same node and graph snapshot',
    });
  return offset;
}

export function neighbors(options: {
  input: LoadedGraph;
  id: string;
  check: Check;
  limit: number;
  maxBytes: number;
  cursor?: string;
}) {
  if (!options.input.nodes.has(options.id))
    throw new HivexError({
      code: 'GRAPH_NODE_NOT_FOUND',
      message: 'The claim is not in this graph snapshot',
    });
  const matches = options.input.graph.edges.filter(
    (edge) => edge.from === options.id || edge.to === options.id,
  );
  const start = neighborOffset(
    options.cursor,
    options.input.graph.hash,
    options.id,
    matches.length,
  );
  const relations: {
    edge: LoadedGraph['graph']['edges'][number];
    neighbor: { id: string; source: string; preview: string | null };
    neighborEvidence: string;
  }[] = [];
  const response = () => ({
    ...options.check,
    operation: 'neighbors',
    relations,
    continuation:
      start + relations.length < matches.length
        ? `n1.${options.input.graph.hash}.${options.id}.${start + relations.length}`
        : null,
  });
  for (const edge of matches.slice(start, start + options.limit)) {
    const id = edge.from === options.id ? edge.to : edge.from;
    const node = options.input.nodes.get(id);
    if (!node)
      throw new HivexError({ code: 'GRAPH_INVALID', message: 'A related claim is missing' });
    relations.push({
      edge,
      neighbor: {
        id,
        source: node.source,
        preview: Buffer.byteLength(node.statement.text) <= 384 ? node.statement.text : null,
      },
      neighborEvidence: 'not-opened',
    });
    if (Buffer.byteLength(JSON.stringify(response())) + 1 > options.maxBytes) {
      if (relations.length === 1) return budgeted(response(), options.maxBytes);
      relations.pop();
      break;
    }
  }
  return budgeted(response(), options.maxBytes);
}
