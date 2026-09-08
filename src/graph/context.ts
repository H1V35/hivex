import { createReviewContext, prepareSourceReview } from './source-review.ts';

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
