import { createReviewContext, prepareSourceReview } from './source-review.ts';
import { lstatSync, readFileSync } from 'node:fs';
import { z } from 'zod';
import { HivexError } from '../errors.ts';
import { digest } from './snapshot.ts';
import { parseGraphDocument } from './verify.ts';
import type { Source } from '../sources/markdown.ts';

const sourceId = z.string().min(1).max(4096);
const contextPair = z.strictObject({
  sources: z.array(sourceId).length(2),
  context: z.array(sourceId).min(1).max(2),
});
export const comparisonContextSchema = z.strictObject({
  version: z.literal(1),
  pairs: z.array(contextPair).max(2048),
});
export type ComparisonContext = z.infer<typeof comparisonContextSchema>;
export const boundComparisonContextSchema = comparisonContextSchema.extend({
  pairs: z
    .array(
      contextPair.extend({
        context: z
          .array(z.strictObject({ id: sourceId, contentHash: digest }))
          .min(1)
          .max(2),
      }),
    )
    .max(2048),
});

function invalidContext(message: string): never {
  throw new HivexError({ code: 'COMPARISON_CONTEXT_INVALID', message });
}

export function normalizeComparisonContext(value: unknown): ComparisonContext | undefined {
  const config = comparisonContextSchema.parse(value);
  const seen = new Set<string>();
  const pairs = config.pairs
    .map((entry) => {
      const sources = [...entry.sources].sort();
      const context = [...entry.context].sort();
      const key = JSON.stringify(sources);
      if (
        sources[0] === sources[1] ||
        seen.has(key) ||
        new Set(context).size !== context.length ||
        context.some((id) => sources.includes(id))
      )
        invalidContext('Context requires distinct primary/supporting sources and unique pairs');
      seen.add(key);
      return { sources, context };
    })
    .sort((a, b) => (JSON.stringify(a.sources) < JSON.stringify(b.sources) ? -1 : 1));
  return pairs.length ? { version: 1, pairs } : undefined;
}

export function readComparisonContext(path?: string) {
  if (path === undefined) return undefined;
  const stat = lstatSync(path);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size > 131072)
    invalidContext('Use a regular comparison-context JSON file of at most 128 KiB');
  const bytes = readFileSync(path);
  if (bytes.length > 131072) invalidContext('Comparison context exceeds 128 KiB');
  return normalizeComparisonContext(parseGraphDocument(bytes));
}

export function comparisonContextFromSelection(selection: unknown) {
  const value = z
    .object({ comparisonContext: boundComparisonContextSchema.optional() })
    .parse(selection);
  if (!value.comparisonContext) return undefined;
  return normalizeComparisonContext({
    version: 1,
    pairs: value.comparisonContext.pairs.map((pair) => ({
      sources: pair.sources,
      context: pair.context.map((source) => source.id),
    })),
  });
}

export function supportingSource(context: ReturnType<typeof createReviewContext>, id: string) {
  const source = context.sources.get(id);
  if (!source || !context.input.sources.has(id)) invalidContext(`Unknown graph source: ${id}`);
  return source;
}

export function sourceDescriptor(source: Source) {
  return {
    id: source.id,
    path: source.path,
    contentHash: source.contentHash,
    section: source.section,
    authority: source.authority,
  };
}

export function bindComparisonContext(
  context: ReturnType<typeof createReviewContext>,
  config: ComparisonContext | undefined,
  selected: { sources: string[] }[],
) {
  if (!config) return undefined;
  const normalized = normalizeComparisonContext(config);
  if (!normalized) return undefined;
  const pairs = new Set(selected.map((pair) => JSON.stringify([...pair.sources].sort())));
  return {
    version: 1 as const,
    pairs: normalized.pairs.map((pair) => {
      for (const id of pair.sources) supportingSource(context, id);
      if (!pairs.has(JSON.stringify(pair.sources)))
        invalidContext('Comparison context names an unused primary pair');
      return {
        sources: pair.sources,
        context: pair.context.map((id) => ({
          id,
          contentHash: supportingSource(context, id).contentHash,
        })),
      };
    }),
  };
}

export function pairContext(config: ComparisonContext | undefined, ids: string[]) {
  const key = JSON.stringify([...ids].sort());
  return config?.pairs.find((pair) => JSON.stringify(pair.sources) === key)?.context ?? [];
}

export function prepareKnowledgeSources(
  context: ReturnType<typeof createReviewContext>,
  ids: string[],
) {
  const sources = [...ids].sort().map((id) => prepareSourceReview(context, id));
  const bindings = new Map<string, string>();
  const claimBindings = new Map<string, string>();
  const packet = {
    sources: sources.map((source, index) => {
      const id = `s${index + 1}`;
      bindings.set(id, source.source.id);
      const names = new Map(source.nodes.map((node) => [node.id, `${id}:${node.localIds[0]}`]));
      for (const [node, name] of names) claimBindings.set(name, node);
      return {
        id,
        sourceId: source.source.id,
        section: source.source.section,
        authority: source.source.authority,
        firstLine: source.packet.firstLine,
        markdown: source.source.content,
        claims: source.nodes.map((node) => ({ id: names.get(node.id), ...node.statement })),
        relations: source.edges.map((edge) => ({
          from: names.get(edge.from),
          to: names.get(edge.to),
          type: edge.type,
          evidence: edge.evidence,
        })),
      };
    }),
  };
  return {
    sources,
    packet,
    bindings,
    claimBindings,
    nodes: new Map(
      sources.flatMap((source) => source.nodes.map((node) => [node.id, node] as const)),
    ),
  };
}
