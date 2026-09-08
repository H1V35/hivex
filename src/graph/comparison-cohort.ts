import { isDeepStrictEqual } from 'node:util';
import { z } from 'zod';
import { HivexError } from '../errors.ts';
import { hash } from '../sources/markdown.ts';
import { knowledgeModel, nativeVersion, requestedPolicyHash } from '../model/profile.ts';
import { usageSchema } from '../model/transcript.ts';
import { digest } from './snapshot.ts';
import { createReviewContext } from './source-review.ts';
import { buildComparisonPlan } from './comparison-plan.ts';
import {
  comparisonSchema,
  modelComparisonSchema,
  prepareComparison,
  runComparison,
  satisfactoryComparison,
  validateComparison,
} from './comparison.ts';
import {
  assessmentArguments,
  summarize,
  validateCompletedInvocation,
} from './assessment-cohort.ts';
import { AssessmentStore, type AssessmentPlan } from './assessment-store.ts';

const resultSchema = z.looseObject({
  command: z.literal('graph'),
  operation: z.literal('compare'),
  accepted: z.literal(false),
  status: z.enum(['reviewed', 'failed']),
  graphHash: digest,
  sources: z.array(z.looseObject({ id: z.string() })).length(2),
  contract: z.looseObject({
    promptHash: digest,
    schemaHash: digest,
    nativeVersion: z.string(),
    requestedPolicyHash: digest,
  }),
  report: z.looseObject({ outcome: z.string(), usage: usageSchema.nullable() }),
  comparison: comparisonSchema.nullable(),
});
type ComparisonResult = z.infer<typeof resultSchema>;
const comparisonContract = {
  applicationId: 0x48565843,
  parse: (value: unknown) => resultSchema.parse(value),
  unitId: (result: ComparisonResult) =>
    hash(JSON.stringify(result.sources.map((source) => source.id))),
};
type Rows = ReturnType<AssessmentStore<ComparisonResult>['snapshot']>;
type Context = ReturnType<typeof createContext>;

function createContext(options: ReturnType<typeof assessmentArguments>) {
  const context = createReviewContext(options);
  const selection = buildComparisonPlan(context, 8 * 1024 * 1024, options.neighbors);
  if (selection.status !== 'planned' || selection.pairs.length === 0)
    throw new HivexError({
      code: 'COMPARISON_PLAN_UNRESOLVED',
      message:
        'Resolve the authored-link plan and select at least one pair before cohort comparison',
    });
  if (selection.pairs.length > 2048)
    throw new HivexError({
      code: 'REVIEW_PLAN_TOO_LARGE',
      message: 'A comparison cohort supports at most 2048 source pairs',
    });
  const pairs = new Map(selection.pairs.map((pair) => [pair.id, pair.sources]));
  const plan: AssessmentPlan = {
    graphHash: context.input.graph.hash,
    selectionHash: selection.planHash,
    contract: {
      nativeVersion,
      requestedPolicyHash: requestedPolicyHash(),
      schemaHash: hash(JSON.stringify(z.toJSONSchema(modelComparisonSchema))),
    },
    sources: selection.pairs.map((pair) => ({
      id: pair.id,
      promptHash: hash(prepareComparison(context, pair.sources).prompt),
    })),
  };
  return { context, selection, pairs, plan };
}

function prepare(context: Context, id: string) {
  const sources = context.pairs.get(id);
  if (!sources)
    throw new HivexError({
      code: 'INVALID_COMPARISON_STORE',
      message: 'A retained pair is outside the selection',
    });
  return prepareComparison(context.context, sources);
}

function validateResult(result: ComparisonResult, prepared: ReturnType<typeof prepareComparison>) {
  if (
    !isDeepStrictEqual(
      result.sources,
      prepared.sources.map((source) => source.packet.source),
    ) ||
    !isDeepStrictEqual(result.sourceSnapshot, prepared.context.input.graph.sourceSnapshot) ||
    !isDeepStrictEqual(result.model, knowledgeModel) ||
    !isDeepStrictEqual(result.sourceBindings, Object.fromEntries(prepared.bindings))
  )
    throw new HivexError({
      code: 'INVALID_COMPARISON_STORE',
      message: 'Comparison provenance differs from its planned sources or model',
    });
  validateCompletedInvocation(result.report);
  if (result.comparison) {
    validateComparison(result.comparison, prepared);
    validateHashes(result, prepared);
  }
  const passed =
    result.comparison !== null &&
    satisfactoryComparison(result.comparison) &&
    result.report.outcome === 'completed';
  if ((result.status === 'reviewed') !== passed)
    throw new HivexError({
      code: 'INVALID_COMPARISON_STORE',
      message: 'The retained verdict differs from its complete comparison',
    });
}

function validateHashes(result: ComparisonResult, prepared: ReturnType<typeof prepareComparison>) {
  const comparison = result.comparison;
  if (!comparison) return;
  const names = new Map([...prepared.claimBindings].map(([name, id]) => [id, name]));
  const sources = new Map([...prepared.bindings].map(([name, id]) => [id, name]));
  const evidence = (entries: (typeof comparison.assessments)[number]['evidence']) =>
    entries.map((entry) => ({ ...entry, source: sources.get(entry.source) }));
  const normalized = modelComparisonSchema.parse({
    ...comparison,
    assessments: comparison.assessments.map((entry) => ({
      ...entry,
      id: names.get(entry.id),
      evidence: evidence(entry.evidence),
    })),
    relations: comparison.relations.map((entry) => ({
      ...entry,
      from: names.get(entry.from),
      to: names.get(entry.to),
      evidence: evidence(entry.evidence),
    })),
  });
  if (
    result.comparisonHash !== hash(JSON.stringify(comparison)) ||
    result.modelOutputHash !== hash(JSON.stringify(normalized))
  )
    throw new HivexError({
      code: 'INVALID_COMPARISON_STORE',
      message: 'Comparison hashes differ from the expanded or normalized assessment',
    });
}

function validateRows(rows: Rows, context: Context) {
  for (const row of rows) if (row.result) validateResult(row.result, prepare(context, row.id));
}

function inspect(rows: Rows, context: Context, options: ReturnType<typeof assessmentArguments>) {
  const envelope = {
    command: 'graph',
    operation: 'comparison-cohort',
    accepted: false,
    graphHash: context.plan.graphHash,
    planHash: hash(JSON.stringify(context.plan)),
    selectionHash: context.selection.planHash,
  };
  const row = rows.find((row) => row.id === options.show);
  if (!options.export && !row)
    throw new HivexError({
      code: 'PAIR_NOT_FOUND',
      message: 'The requested pair is outside this comparison cohort',
    });
  const result = options.export
    ? {
        ...envelope,
        plan: context.plan,
        selection: context.selection,
        comparisons: rows,
        ...summarize(rows),
      }
    : { ...envelope, pair: row?.id, state: row?.state, result: row?.result };
  if (Buffer.byteLength(JSON.stringify(result)) + 1 > options.maxBytes)
    throw new HivexError({
      code: 'COMPARISON_OUTPUT_BUDGET',
      message: 'The complete comparison evidence exceeds the output budget',
    });
  return result;
}

async function comparePending(
  options: ReturnType<typeof assessmentArguments>,
  context: Context,
  store: AssessmentStore<ComparisonResult>,
) {
  const owner = crypto.randomUUID();
  let processed = 0;
  while (processed < options.maxUnits) {
    const id = store.claim(owner);
    if (id === null) break;
    const result = await runComparison(prepare(context, id), options);
    store.complete(id, owner, result);
    processed++;
    if (result.status === 'failed') break;
  }
  return processed;
}

export async function comparisonCohortCommand(args: string[]) {
  const options = assessmentArguments(args, 'compare');
  if (options.discard !== undefined)
    return {
      ...AssessmentStore.discard(options.store, options.discard, comparisonContract.applicationId),
      operation: 'comparison-cohort',
    };
  const context = createContext(options);
  if (options.show !== undefined || options.export) {
    const rows = AssessmentStore.read(options.store, context.plan, comparisonContract);
    validateRows(rows, context);
    return inspect(rows, context, options);
  }
  using store = new AssessmentStore(options.store, context.plan, comparisonContract);
  const previous = store.snapshot();
  validateRows(previous, context);
  const reused = previous.filter((row) => row.state === 'reviewed').length;
  const processed = await comparePending(options, context, store);
  const rows = store.snapshot();
  validateRows(rows, context);
  return {
    command: 'graph',
    operation: 'comparison-cohort',
    accepted: false,
    graphHash: context.plan.graphHash,
    planHash: hash(JSON.stringify(context.plan)),
    selectionHash: context.selection.planHash,
    processed,
    reused,
    ...summarize(rows),
    limitations: context.selection.limitations,
  };
}
