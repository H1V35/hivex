import { lstatSync, readFileSync } from 'node:fs';
import { isDeepStrictEqual } from 'node:util';
import { HivexError } from '../errors.ts';
import { candidateSchema } from '../ingestion/claims.ts';
import { validateCandidateEvidence } from '../ingestion/evidence.ts';
import { createPlan } from '../ingestion/plan.ts';
import { hash } from '../sources/markdown.ts';
import { loadSnapshot } from '../workspace/snapshot.ts';
import { inputHash } from './build.ts';
import {
  contentSchema,
  digest,
  sealGraph,
  sourceInputHash,
  type GraphSnapshot,
} from './snapshot.ts';

type Node = GraphSnapshot['nodes'][number];
type Edge = GraphSnapshot['edges'][number];
type Source = GraphSnapshot['sources'][number];
const graphSchema = contentSchema.extend({ hash: digest });

function invalid(message: string): never {
  throw new HivexError({ code: 'GRAPH_INVALID', message });
}

function validateReceipts(graph: GraphSnapshot) {
  for (const source of graph.sources) {
    const { reportedProfile: profile, attempts, candidateAttempt } = source.extraction;
    if (
      profile.model !== graph.processing.model.name ||
      profile.effort !== graph.processing.model.effort ||
      profile.modelProvider !== graph.processing.model.provider ||
      candidateAttempt !== attempts.length ||
      attempts[candidateAttempt - 1]?.outcome !== 'completed' ||
      source.sourceBytes > graph.processing.maximumSourceBytes
    )
      invalid('A source execution record does not match the processing contract');
  }
}

function indexGraph(graph: GraphSnapshot) {
  const sources = new Map(graph.sources.map((source) => [source.id, source]));
  const nodes = new Map<string, Node>();
  const edges = new Map<string, Edge>();
  const aliases = new Map<string, Map<string, Node>>();
  if (sources.size !== graph.sources.length) invalid('Graph source identities must be unique');
  for (const node of graph.nodes) {
    if (
      !sources.has(node.source) ||
      nodes.has(node.id) ||
      hash(JSON.stringify({ source: node.source, statement: node.statement })) !== node.id
    )
      invalid('A graph claim identity or source is invalid');
    nodes.set(node.id, node);
    const local = aliases.get(node.source) ?? new Map<string, Node>();
    for (const alias of node.localIds) {
      if (local.has(alias)) invalid('A local claim identity belongs to multiple graph claims');
      local.set(alias, node);
    }
    aliases.set(node.source, local);
  }
  for (const edge of graph.edges) {
    const { id, ...body } = edge;
    if (
      edges.has(id) ||
      hash(JSON.stringify(body)) !== id ||
      nodes.get(edge.from)?.source !== edge.source ||
      nodes.get(edge.to)?.source !== edge.source
    )
      invalid('A graph relation identity or endpoint is invalid');
    edges.set(id, edge);
  }
  return { sources, nodes, edges, aliases };
}

function recoverCandidate(source: Source, index: ReturnType<typeof indexGraph>, used: Set<string>) {
  const aliases = index.aliases.get(source.id) ?? new Map<string, Node>();
  const order = source.extraction.claimOrder;
  if (new Set(order).size !== order.length || order.length !== aliases.size)
    invalid('The graph does not preserve every source-local claim');
  const claims = order.map((id) => {
    const node = aliases.get(id);
    if (!node) invalid('The graph omitted an extracted claim');
    return { id, ...node.statement };
  });
  const relations = source.extraction.relationOrder.map((reference) => {
    const edge = index.edges.get(reference.edgeId);
    if (
      !edge ||
      edge.source !== source.id ||
      edge.from !== aliases.get(reference.fromLocalId)?.id ||
      edge.to !== aliases.get(reference.toLocalId)?.id
    )
      invalid('The graph does not preserve an extracted relation');
    used.add(edge.id);
    return {
      from: reference.fromLocalId,
      to: reference.toLocalId,
      type: edge.type,
      evidence: edge.evidence,
    };
  });
  const candidate = candidateSchema.parse({ claims, relations });
  const encoded = JSON.stringify(candidate);
  const bytes = Buffer.byteLength(encoded);
  if (bytes > 8 * 1024 * 1024) invalid('The expanded source candidate exceeds its size limit');
  if (hash(encoded) !== source.extraction.candidateHash)
    invalid('The graph differs from its retained extraction');
  return { candidate, bytes };
}

export function readGraph(path: string) {
  const file = lstatSync(path);
  if (!file.isFile() || file.isSymbolicLink() || file.size > 64 * 1024 * 1024)
    invalid('Use a regular graph snapshot of at most 64 MiB');
  const buffer = readFileSync(path);
  if (buffer.length > 64 * 1024 * 1024) invalid('The graph snapshot exceeds its size limit');
  let graph: GraphSnapshot;
  try {
    graph = graphSchema.parse(JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(buffer)));
  } catch {
    return invalid('The graph snapshot format is unsupported or malformed');
  }
  const { hash: expectedHash, ...content } = graph;
  const canonical = sealGraph(content);
  if (canonical.hash !== expectedHash)
    throw new HivexError({
      code: 'GRAPH_HASH_MISMATCH',
      message: 'The graph snapshot was altered',
    });
  const units = canonical.sources.map(({ extraction: _extraction, ...unit }) => unit);
  const expectedInput = sourceInputHash({
    selection: graph.selection,
    configHash: graph.sourceSnapshot.configHash,
    processing: graph.processing,
    units,
  });
  if (expectedInput !== graph.inputHash)
    invalid('The recorded source input manifest is inconsistent');
  const index = indexGraph(canonical);
  validateReceipts(canonical);
  const used = new Set<string>();
  const candidates = new Map<string, ReturnType<typeof recoverCandidate>['candidate']>();
  let expandedBytes = 0;
  for (const source of canonical.sources) {
    const recovered = recoverCandidate(source, index, used);
    expandedBytes += recovered.bytes;
    if (expandedBytes > 128 * 1024 * 1024)
      invalid('Expanded candidates exceed the ingestion store limit');
    candidates.set(source.id, recovered.candidate);
  }
  if (used.size !== index.edges.size)
    invalid('The graph includes a relation without extraction provenance');
  return { graph: canonical, ...index, candidates };
}

export function checkGraph(input: ReturnType<typeof readGraph>, root: string, against = 'HEAD') {
  const { graph, candidates } = input;
  const selection = { collection: graph.selection.collection ?? undefined };
  const original = loadSnapshot({ root, ref: graph.sourceSnapshot.commit, selection });
  if (
    original.configHash !== graph.sourceSnapshot.configHash ||
    original.sources.length !== graph.sources.length
  )
    invalid('The graph source snapshot is inconsistent');
  const originals = new Map(original.sources.map((source) => [source.id, source]));
  for (const source of graph.sources) {
    const actual = originals.get(source.id);
    const candidate = candidates.get(source.id);
    if (
      !actual ||
      !candidate ||
      source.path !== actual.path ||
      source.contentHash !== actual.contentHash ||
      !isDeepStrictEqual(source.section, actual.section) ||
      !isDeepStrictEqual(source.authority, actual.authority)
    )
      invalid('A graph source differs from its versioned document');
    validateCandidateEvidence({ candidate, source: actual });
  }
  const current = loadSnapshot({ root, ref: against, selection });
  const currentInput = inputHash(createPlan(current, graph.selection.collection));
  const fresh = currentInput === graph.inputHash;
  return {
    command: 'graph',
    operation: 'check',
    accepted: false,
    status: fresh ? 'candidate' : 'failed',
    hash: graph.hash,
    sourceSnapshot: graph.sourceSnapshot,
    sourceEvidence: 'verified',
    freshness: {
      status: fresh ? 'fresh' : 'stale',
      comparedCommit: current.commit,
      inputHash: currentInput,
    },
    semanticReview: 'not-established',
  };
}
