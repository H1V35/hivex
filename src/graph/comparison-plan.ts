import { parseArgs } from 'node:util';
import { posix } from 'node:path';
import { HivexError } from '../errors.ts';
import { parseLimit } from '../cli/arguments.ts';
import { hash, isMarkdownPath, type Source } from '../sources/markdown.ts';
import { createReviewContext } from './source-review.ts';

type Reason = {
  kind: 'markdown-link';
  source: string;
  document: string;
  target: string;
  evidence: Source['references'][number]['evidence'];
  definition?: Source['references'][number]['definition'];
};
type Pair = { id: string; sources: string[]; reasons: Reason[] };

function argumentsFor(args: string[]) {
  let parsed;
  try {
    parsed = parseArgs({
      args,
      strict: true,
      options: {
        root: { type: 'string' },
        input: { type: 'string' },
        against: { type: 'string' },
        'max-bytes': { type: 'string' },
      },
    }).values;
  } catch (error) {
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: error instanceof Error ? error.message : 'Invalid comparison plan options',
    });
  }
  if (!parsed.input || Object.values(parsed).some((value) => value === ''))
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Comparison planning requires --input',
    });
  return {
    root: parsed.root ?? process.cwd(),
    input: parsed.input,
    against: parsed.against,
    maxBytes: parseLimit(parsed['max-bytes'], {
      fallback: 16384,
      minimum: 1024,
      maximum: 8 * 1024 * 1024,
    }),
  };
}

function localTarget(source: Source, url: string) {
  if (/^(?:[a-z][a-z0-9+.-]*:|\/\/)/i.test(url)) return { kind: 'external' } as const;
  try {
    const separator = url.indexOf('#');
    const rawPath = separator < 0 ? url : url.slice(0, separator);
    const path = decodeURIComponent(rawPath.split('?')[0] ?? '');
    const anchor = separator < 0 ? null : decodeURIComponent(url.slice(separator + 1));
    if (path.startsWith('/') || path.includes('\\')) return { kind: 'unsupported-path' } as const;
    const target = path
      ? posix.normalize(posix.join(posix.dirname(source.path), path))
      : source.path;
    if (target === '..' || target.startsWith('../')) return { kind: 'outside-repository' } as const;
    return { kind: 'local', path: target, anchor } as const;
  } catch {
    return { kind: 'invalid-url' } as const;
  }
}

function targetsFor(sources: Source[], target: { path: string; anchor: string | null }) {
  return sources.filter((source) => {
    if (!target.anchor) return true;
    return (
      source.blocks.some((block) => block.anchor === target.anchor) ||
      (source.section === null && source.containedAnchors.includes(target.anchor))
    );
  });
}

type Unresolved = {
  source: string;
  document: string;
  url: string;
  reason: string;
  evidence: Reason['evidence'];
  definition?: Reason['definition'];
};

class ComparisonPlan {
  readonly pairs = new Map<string, Pair>();
  readonly unresolved: Unresolved[] = [];
  readonly sourcesByPath: Map<string, Source[]>;
  readonly context: ReturnType<typeof createReviewContext>;
  external = 0;
  nonMarkdown = 0;
  links = 0;
  private retainedBytes = 0;

  constructor(context: ReturnType<typeof createReviewContext>) {
    this.context = context;
    this.sourcesByPath = Map.groupBy(context.sources.values(), (source) => source.path);
  }

  private reserve(value: unknown) {
    this.retainedBytes += Buffer.byteLength(JSON.stringify(value)) + 1;
    if (this.retainedBytes > 8 * 1024 * 1024)
      throw new HivexError({
        code: 'COMPARISON_PLAN_TOO_LARGE',
        message: 'The comparison plan exceeds its bounded working set',
      });
  }

  private unresolvedReference(
    source: Source,
    reference: Source['references'][number],
    reason: string,
  ) {
    const value = {
      source: source.id,
      document: source.path,
      url: reference.url,
      reason,
      evidence: reference.evidence,
      definition: reference.definition,
    };
    this.reserve(value);
    this.unresolved.push(value);
  }

  private pair(source: Source, destination: Source, reference: Source['references'][number]) {
    if (destination.id === source.id) return;
    if (
      !this.context.nodesBySource.get(source.id)?.length ||
      !this.context.nodesBySource.get(destination.id)?.length
    ) {
      this.unresolvedReference(source, reference, 'target-pair-requires-claims');
      return;
    }
    const ids = [source.id, destination.id].sort();
    const id = hash(JSON.stringify(ids));
    let pair = this.pairs.get(id);
    if (!pair) {
      if (this.pairs.size >= 32768)
        throw new HivexError({
          code: 'COMPARISON_PLAN_TOO_LARGE',
          message: 'A plan may contain at most 32768 source pairs',
        });
      pair = { id, sources: ids, reasons: [] };
      this.reserve(pair);
      this.pairs.set(id, pair);
    }
    const reason: Reason = {
      kind: 'markdown-link',
      source: source.id,
      document: source.path,
      target: destination.id,
      evidence: reference.evidence,
      definition: reference.definition,
    };
    this.reserve(reason);
    pair.reasons.push(reason);
  }

  add(source: Source, reference: Source['references'][number]) {
    this.links++;
    if (this.links > 10000)
      throw new HivexError({
        code: 'COMPARISON_PLAN_TOO_LARGE',
        message: 'A plan may inspect at most 10000 Markdown links',
      });
    const target = localTarget(source, reference.url);
    if (target.kind === 'external') {
      this.external++;
      return;
    }
    if (target.kind !== 'local') {
      this.unresolvedReference(source, reference, target.kind);
      return;
    }
    const candidates = this.sourcesByPath.get(target.path) ?? [];
    if (!candidates.length && !isMarkdownPath(target.path)) {
      this.nonMarkdown++;
      return;
    }
    const targets = targetsFor(candidates, target);
    if (!targets.length)
      this.unresolvedReference(source, reference, 'target-not-in-graph-or-anchor-missing');
    for (const destination of targets) this.pair(source, destination, reference);
  }
}

export function comparisonPlanCommand(args: string[]) {
  const options = argumentsFor(args);
  const context = createReviewContext(options);
  return buildComparisonPlan(context, options.maxBytes);
}

export function buildComparisonPlan(
  context: ReturnType<typeof createReviewContext>,
  maxBytes = 8 * 1024 * 1024,
) {
  const sources = [...context.sources.values()];
  const plan = new ComparisonPlan(context);
  for (const source of sources)
    for (const reference of source.references) plan.add(source, reference);
  const content = {
    format: 'hivex-comparison-plan',
    version: 1,
    accepted: false,
    graphHash: context.input.graph.hash,
    policy: 'authored-markdown-links-v1',
    sourceSnapshot: context.input.graph.sourceSnapshot,
    pairs: [...plan.pairs.values()].sort((a, b) => (a.id < b.id ? -1 : Number(a.id !== b.id))),
    unresolved: plan.unresolved,
    coverage: {
      sources: sources.length,
      links: plan.links,
      externalLinks: plan.external,
      nonMarkdownLinks: plan.nonMarkdown,
      possiblePairs: (sources.length * (sources.length - 1)) / 2,
      selectedPairs: plan.pairs.size,
    },
    semanticRelationships: 'not-established',
    limitations: [
      'Markdown links only. No inferred relationships, external content, whole-graph semantic coverage or admission.',
    ],
  };
  const result = {
    ...content,
    status: plan.unresolved.length ? 'failed' : 'planned',
    planHash: hash(JSON.stringify(content)),
  };
  if (Buffer.byteLength(JSON.stringify(result)) + 1 > maxBytes)
    throw new HivexError({
      code: 'COMPARISON_PLAN_BUDGET',
      message: 'The complete plan exceeds the requested output budget',
    });
  return result;
}
