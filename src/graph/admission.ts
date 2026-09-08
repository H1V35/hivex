import { lstatSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { isDeepStrictEqual, parseArgs } from 'node:util';
import { z } from 'zod';
import { HivexError } from '../errors.ts';
import { parseLimit } from '../cli/arguments.ts';
import { hash } from '../sources/markdown.ts';
import { digest } from './snapshot.ts';
import { checkGraph, decodeGraph, parseGraphDocument } from './verify.ts';
import { createReviewContext } from './source-review.ts';
import {
  AssessmentStore,
  assessmentSchemaHash,
  validateAssessmentBinding,
  type AssessmentContract,
  type AssessmentPlan,
  type AssessmentResult,
} from './assessment-store.ts';
import {
  reviewContract,
  reviewResultSchema,
  sourceReviewPlan,
  validateSourceReviews,
} from './review-cohort.ts';
import {
  comparisonContract,
  comparisonResultSchema,
  prepareComparisonCohort,
  validateComparisons,
  type ComparisonResult,
} from './comparison-cohort.ts';
import { comparisonSchema } from './comparison.ts';

const relationshipSchema = comparisonSchema.shape.relations.element.omit({ id: true }).extend({
  id: digest,
  comparison: z.strictObject({ pair: digest, hash: digest, relation: z.string() }),
});
export type ReviewedRelationship = z.infer<typeof relationshipSchema>;
const contentSchema = z.strictObject({
  format: z.literal('hivex-admitted-graph'),
  version: z.literal(1),
  accepted: z.literal(true),
  graph: z.unknown(),
  neighbors: z.number().int().min(0).max(8),
  manifest: z.unknown(),
  sourceReviews: z.array(reviewResultSchema).max(2048),
  comparisonSelection: z.unknown(),
  comparisons: z.array(comparisonResultSchema).max(2048),
  relationships: z.array(relationshipSchema).max(262144),
});
const snapshotSchema = contentSchema.extend({ hash: digest });
const maximumBytes = 256 * 1024 * 1024;
type Context = ReturnType<typeof createReviewContext>;

function invalid(message: string): never {
  throw new HivexError({ code: 'GRAPH_ADMISSION_INVALID', message });
}

function records<T extends AssessmentResult>(
  values: T[],
  plan: AssessmentPlan,
  contract: AssessmentContract<T>,
) {
  if (values.length !== plan.sources.length)
    invalid('Admission requires every planned assessment exactly once');
  return values.map((result, index) => {
    if (Buffer.byteLength(JSON.stringify(result)) > 8 * 1024 * 1024)
      invalid('An embedded assessment exceeds its 8 MiB retention limit');
    const source = plan.sources[index];
    if (!source || result.status !== 'reviewed')
      invalid('Admission requires complete successful assessments');
    validateAssessmentBinding(
      contract.binding?.(result) ?? result,
      {
        actualId: contract.unitId(result),
        id: source.id,
        promptHash: source.promptHash,
        schemaHash: assessmentSchemaHash(plan, source.id),
      },
      plan,
    );
    return { id: source.id, state: 'reviewed', result } satisfies {
      id: string;
      state: 'reviewed';
      result: T;
    };
  });
}

function completed<T extends AssessmentResult>(rows: ReturnType<AssessmentStore<T>['snapshot']>) {
  return rows.map((row) => {
    if (row.state !== 'reviewed' || row.result === null)
      invalid('Pending, failed or unresolved work prevents admission');
    return row.result;
  });
}

function relationships(comparisons: ComparisonResult[]) {
  const entries: ReviewedRelationship[] = [];
  for (const result of comparisons) {
    if (!result.comparison) invalid('An admitted comparison must retain its complete assessment');
    for (const { id: relation, ...body } of result.comparison.relations) {
      const content = {
        ...body,
        comparison: {
          pair: comparisonContract.unitId(result),
          hash: digest.parse(result.comparisonHash),
          relation,
        },
      };
      entries.push({ ...content, id: hash(JSON.stringify(content)) });
    }
  }
  return entries.sort((a, b) => (a.id < b.id ? -1 : Number(a.id !== b.id)));
}

function checkPrecedence(edges: { from: string; to: string; type: string }[]) {
  const incoming = new Map<string, number>();
  const outgoing = new Map<string, string[]>();
  if (edges.some((edge) => edge.type === 'contradicts'))
    invalid('An explicit unresolved contradiction prevents graph admission');
  const precedence = edges.filter((edge) => ['supersedes', 'exception-to'].includes(edge.type));
  for (const edge of precedence) {
    incoming.set(edge.from, incoming.get(edge.from) ?? 0);
    incoming.set(edge.to, (incoming.get(edge.to) ?? 0) + 1);
    const targets = outgoing.get(edge.from) ?? [];
    targets.push(edge.to);
    outgoing.set(edge.from, targets);
  }
  const pending = [...incoming].filter(([, count]) => count === 0).map(([id]) => id);
  for (let index = 0; index < pending.length; index++) {
    for (const target of outgoing.get(pending[index] ?? '') ?? []) {
      const remaining = (incoming.get(target) ?? 0) - 1;
      incoming.set(target, remaining);
      if (remaining === 0) pending.push(target);
    }
  }
  if (pending.length !== incoming.size)
    invalid('Precedence contains a cycle whose applicability remains unresolved');
}

function evidence(context: Context, neighbors: number) {
  if (context.input.graph.nodes.length === 0) invalid('An empty graph cannot be admitted');
  const reviewPlan = sourceReviewPlan(context);
  const comparisonContext = prepareComparisonCohort(context, neighbors);
  if (!comparisonContext.selection.pairs.length && context.nodesBySource.size > 1)
    invalid('Multiple claim sources need a nonempty comparison selection before admission');
  return { reviewPlan, comparisonContext };
}

function validateEvidence(
  context: Context,
  values: {
    sourceReviews: z.infer<typeof reviewResultSchema>[];
    comparisons: ComparisonResult[];
    neighbors: number;
  },
  prepared = evidence(context, values.neighbors),
) {
  const reviews = records(values.sourceReviews, prepared.reviewPlan, reviewContract);
  const comparisons = records(
    values.comparisons,
    prepared.comparisonContext.plan,
    comparisonContract,
  );
  validateSourceReviews(reviews, context);
  validateComparisons(comparisons, prepared.comparisonContext);
  const derived = relationships(values.comparisons);
  checkPrecedence([...context.input.graph.edges, ...derived]);
  const manifest = {
    graphHash: context.input.graph.hash,
    sourceSnapshot: context.input.graph.sourceSnapshot,
    sourceReviewPlanHash: hash(JSON.stringify(prepared.reviewPlan)),
    comparisonPlanHash: hash(JSON.stringify(prepared.comparisonContext.plan)),
    selectionHash: prepared.comparisonContext.selection.planHash,
    coverage: {
      sources: reviews.length,
      claims: context.input.graph.nodes.length,
      comparedPairs: comparisons.length,
      possiblePairs: prepared.comparisonContext.selection.coverage.possiblePairs,
      policy: prepared.comparisonContext.selection.policy,
      neighbors: values.neighbors,
    },
    globalConsistency: 'not-proven',
    limitations: prepared.comparisonContext.selection.limitations,
  };
  return { manifest, selection: prepared.comparisonContext.selection, relationships: derived };
}

function readSnapshot(path: string) {
  const file = lstatSync(path);
  if (!file.isFile() || file.isSymbolicLink() || file.size > maximumBytes)
    invalid('Use a regular graph artifact of at most 256 MiB');
  const bytes = readFileSync(path);
  if (bytes.length > maximumBytes) invalid('The graph artifact exceeds 256 MiB');
  const value = parseGraphDocument(bytes);
  return { bytes: bytes.length, value };
}

export function readProjection(path: string, root: string, against?: string) {
  const raw = readSnapshot(path);
  const parsedFormat = z
    .object({ format: z.enum(['hivex-graph-candidate', 'hivex-admitted-graph']) })
    .safeParse(raw.value);
  if (!parsedFormat.success)
    throw new HivexError({
      code: 'GRAPH_INVALID',
      message: 'The graph artifact format is unsupported or malformed',
    });
  const envelope = parsedFormat.data;
  if (envelope.format === 'hivex-graph-candidate') {
    if (raw.bytes > 64 * 1024 * 1024) invalid('A candidate graph exceeds its 64 MiB limit');
    const input = decodeGraph(raw.value);
    return { input, check: checkGraph(input, root, against) };
  }
  const parsed = snapshotSchema.safeParse(raw.value);
  if (!parsed.success) invalid('The admitted snapshot format is malformed');
  const { hash: expectedHash, ...content } = parsed.data;
  if (hash(JSON.stringify(content)) !== expectedHash)
    invalid('The admission manifest or its evidence was altered');
  const input = decodeGraph(content.graph);
  const context = createReviewContext({ input, root, against: input.graph.sourceSnapshot.commit });
  const validated = validateEvidence(context, content);
  if (
    !isDeepStrictEqual(content.manifest, validated.manifest) ||
    !isDeepStrictEqual(content.comparisonSelection, validated.selection) ||
    !isDeepStrictEqual(content.relationships, validated.relationships)
  )
    invalid('The admitted relationships, selection or manifest differ from their evidence');
  const check = checkGraph(input, root, against);
  const fresh = check.freshness.status === 'fresh';
  return {
    input: { ...input, relationships: validated.relationships },
    check: {
      ...check,
      accepted: fresh,
      status: fresh ? 'admitted' : 'failed',
      hash: expectedHash,
      sourceGraphHash: input.graph.hash,
      semanticReview: 'complete-within-declared-selection',
      admission: validated.manifest,
    },
  };
}

function argumentsFor(args: string[]) {
  let values;
  try {
    values = parseArgs({
      args,
      strict: true,
      options: {
        root: { type: 'string' },
        input: { type: 'string' },
        reviews: { type: 'string' },
        comparisons: { type: 'string' },
        neighbors: { type: 'string' },
        export: { type: 'boolean' },
        'max-bytes': { type: 'string' },
      },
    }).values;
  } catch (error) {
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: error instanceof Error ? error.message : 'Invalid admission arguments',
    });
  }
  if (!values.input || Object.values(values).some((value) => value === ''))
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Admission requires --input and nonempty options',
    });
  const root = values.root ?? process.cwd();
  return {
    ...values,
    root,
    input: values.input,
    reviews: values.reviews ?? resolve(root, '.hivex/reviews.sqlite'),
    comparisons: values.comparisons ?? resolve(root, '.hivex/comparisons.sqlite'),
    neighbors: parseLimit(values.neighbors, { fallback: 0, minimum: 0, maximum: 8 }),
    maxBytes: parseLimit(values['max-bytes'], {
      fallback: maximumBytes,
      minimum: 1024,
      maximum: maximumBytes,
    }),
  };
}

export function admitCommand(args: string[]) {
  const options = argumentsFor(args);
  const context = createReviewContext(options);
  const prepared = evidence(context, options.neighbors);
  const sourceReviews = completed(
    AssessmentStore.read(options.reviews, prepared.reviewPlan, reviewContract),
  );
  const comparisons = prepared.comparisonContext.selection.pairs.length
    ? completed(
        AssessmentStore.read(
          options.comparisons,
          prepared.comparisonContext.plan,
          comparisonContract,
        ),
      )
    : [];
  const validated = validateEvidence(
    context,
    {
      sourceReviews,
      comparisons,
      neighbors: options.neighbors,
    },
    prepared,
  );
  const content = contentSchema.parse({
    format: 'hivex-admitted-graph',
    version: 1,
    accepted: true,
    graph: context.input.graph,
    neighbors: options.neighbors,
    manifest: validated.manifest,
    sourceReviews,
    comparisonSelection: validated.selection,
    comparisons,
    relationships: validated.relationships,
  });
  const snapshot = { ...content, hash: hash(JSON.stringify(content)) };
  const bytes = Buffer.byteLength(JSON.stringify(snapshot)) + 1;
  if (bytes > options.maxBytes)
    throw new HivexError({
      code: 'GRAPH_ADMISSION_BUDGET',
      message: 'The complete admitted snapshot exceeds its output budget',
    });
  if (options.export) return snapshot;
  return {
    command: 'graph',
    operation: 'admit',
    accepted: true,
    status: 'admitted',
    hash: snapshot.hash,
    manifest: validated.manifest,
    bytes,
    persistence: 'caller-owned; export and retain the complete snapshot',
  };
}
