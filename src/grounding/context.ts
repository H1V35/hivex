import { HivexError } from '../errors.ts';
import { rankLexically } from '../retrieval/lexical.ts';
import { readProjection } from '../graph/admission.ts';
import { createReviewContext } from '../graph/source-review.ts';
import { prepareKnowledgeSources } from '../graph/context.ts';

type Projection = ReturnType<typeof readProjection>;
function failure(message: string): never {
  throw new HivexError({ code: 'GROUND_CONTEXT_INVALID', message });
}
function relations(projection: Projection) {
  const local = projection.input.graph.edges.map((edge) => ({
    ...edge,
    scope: null,
    conditions: [],
    exceptions: [],
    evidence: edge.evidence.map((entry) => ({ ...entry, source: edge.source })),
  }));
  const cross = 'relationships' in projection.input ? projection.input.relationships : [];
  return [...local, ...cross];
}
function sourceDependencies(projection: Projection, edges: ReturnType<typeof relations>) {
  const links = new Map<string, Set<string>>();
  for (const edge of edges) {
    if (!['supersedes', 'exception-to', 'requires'].includes(edge.type)) continue;
    const from = projection.input.nodes.get(edge.from)?.source;
    const to = projection.input.nodes.get(edge.to)?.source;
    if (!from || !to) failure('A required relation has no source');
    const forward = links.get(from) ?? new Set<string>();
    forward.add(to);
    links.set(from, forward);
    const backward = links.get(to) ?? new Set<string>();
    backward.add(from);
    links.set(to, backward);
  }
  return links;
}

function closeSources(
  projection: Projection,
  edges: ReturnType<typeof relations>,
  seeds: Set<string>,
) {
  const links = sourceDependencies(projection, edges);
  const queue = [...seeds];
  for (let index = 0; index < queue.length; index++) {
    const required = [...(links.get(queue[index] ?? '') ?? [])];
    const included = edges.filter(
      (edge) =>
        seeds.has(projection.input.nodes.get(edge.from)?.source ?? '') &&
        seeds.has(projection.input.nodes.get(edge.to)?.source ?? ''),
    );
    for (const edge of included) required.push(...edge.evidence.map((entry) => entry.source));
    for (const source of required) {
      if (seeds.has(source)) continue;
      seeds.add(source);
      queue.push(source);
      if (seeds.size > 16)
        failure('The complete relationship and evidence context exceeds 16 sources');
    }
  }
  return seeds;
}
export function groundingContext(options: {
  root: string;
  input: string;
  claim: string;
  additionalSources: string[];
}) {
  const projection = readProjection(options.input, options.root);
  if (!projection.check.accepted)
    throw new HivexError({
      code: 'GROUND_REQUIRES_ADMISSION',
      message: 'Grounding requires a fresh admitted graph',
    });
  const context = createReviewContext({ input: projection.input, root: options.root });
  const ranked = rankLexically(
    projection.input.graph.nodes.map((node) => ({
      id: node.id,
      title: node.statement.text,
      content: [
        ...node.statement.conditions,
        ...node.statement.exceptions,
        ...node.statement.evidence.map((entry) => entry.quote),
      ].join('\n'),
    })),
    options.claim,
    8,
  );
  const selected = new Set(options.additionalSources);
  for (const match of ranked) {
    const node = projection.input.nodes.get(match.id);
    if (node) selected.add(node.source);
  }
  if (!selected.size) failure('No documentary claims were found for the requested review claim');
  if ([...selected].some((id) => !context.sources.has(id)))
    failure('An additional source is outside the admitted graph');
  const edges = relations(projection);
  closeSources(projection, edges, selected);
  if (selected.size > 16) failure('The selected context exceeds 16 complete sources');
  const prepared = prepareKnowledgeSources(context, [...selected]);
  if (prepared.nodes.size > 512) failure('The selected context exceeds 512 claims');
  const claimNames = new Map([...prepared.claimBindings].map(([name, id]) => [id, name]));
  const sourceNames = new Map([...prepared.bindings].map(([name, id]) => [id, name]));
  const included = edges.filter(
    (edge) => prepared.nodes.has(edge.from) && prepared.nodes.has(edge.to),
  );
  if (included.length > 256)
    failure('The complete selected relationship context exceeds 256 relationships');
  const relationshipBindings = new Map<string, (typeof included)[number]>();
  const relationships = included.map((edge, index) => {
    const id = `r${index + 1}`;
    relationshipBindings.set(id, edge);
    return {
      id,
      from: claimNames.get(edge.from),
      to: claimNames.get(edge.to),
      type: edge.type,
      scope: edge.scope,
      conditions: edge.conditions,
      exceptions: edge.exceptions,
      evidence: edge.evidence.map((entry) => ({ ...entry, source: sourceNames.get(entry.source) })),
    };
  });
  const unexpanded = edges
    .filter((edge) => prepared.nodes.has(edge.from) !== prepared.nodes.has(edge.to))
    .map((edge) => ({
      type: edge.type,
      source: projection.input.nodes.get(prepared.nodes.has(edge.from) ? edge.to : edge.from)
        ?.source,
    }));
  return {
    projection,
    context,
    ...prepared,
    relationshipBindings,
    packet: { ...prepared.packet, relationships, unexpandedRelationships: unexpanded },
    seeds: ranked.map((match) => match.id),
  };
}
