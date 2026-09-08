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
  retryableAssessment,
  rejectedOutputSchema,
  validateRejectedOutput,
} from './assessment-cohort.ts';
import { AssessmentStore, type AssessmentPlan } from './assessment-store.ts';

export const comparisonResultSchema = z.looseObject({
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
  rejectedOutput: rejectedOutputSchema.nullable().optional(),
  association: z
    .strictObject({
      graphHash: digest,
      sourceSnapshot: z.strictObject({
        commit: z.string().regex(/^[a-f0-9]{40}$/),
        configHash: digest,
      }),
      selectionHash: digest,
      originalSelectionHash: digest,
      originalHash: digest,
    })
    .optional(),
});
export type ComparisonResult = z.infer<typeof comparisonResultSchema>;
export const comparisonContract = {
  applicationId: 0x48565843,
  parse: (value: unknown) => comparisonResultSchema.parse(value),
  unitId: (result: ComparisonResult) =>
    hash(JSON.stringify(result.sources.map((source) => source.id))),
  binding: comparisonBinding,
  retryable: retryableAssessment,
};
type Rows = ReturnType<AssessmentStore<ComparisonResult>['snapshot']>;
type Context = ReturnType<typeof prepareComparisonCohort>;

export function originalComparison(result: ComparisonResult) {
  const { association: _association, ...original } = result;
  return original;
}

export function comparisonBinding(result: ComparisonResult) {
  return result.association ? { ...result, graphHash: result.association.graphHash } : result;
}

function createContext(options: ReturnType<typeof assessmentArguments>) {
  const context = createReviewContext(options);
  const prepared = prepareComparisonCohort(context, options.neighbors);
  if (!prepared.selection.pairs.length)
    throw new HivexError({
      code: 'COMPARISON_PLAN_UNRESOLVED',
      message: 'Select at least one pair before cohort comparison',
    });
  return prepared;
}

export function prepareComparisonCohort(
  context: ReturnType<typeof createReviewContext>,
  neighbors: number,
) {
  const selection = buildComparisonPlan(context, 8 * 1024 * 1024, neighbors);
  if (selection.status !== 'planned')
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

export function validateComparisonResult(
  result: ComparisonResult,
  prepared: ReturnType<typeof prepareComparison>,
  selectionHash?: string,
) {
  validateRejectedOutput(result.rejectedOutput, result.report.outcome, result.comparison);
  if (
    result.association &&
    (result.association.selectionHash !== selectionHash ||
      result.association.originalHash !==
        hash(
          JSON.stringify({
            result: originalComparison(result),
            selectionHash: result.association.originalSelectionHash,
          }),
        ) ||
      result.contract.promptHash !== hash(prepared.prompt))
  )
    throw new HivexError({
      code: 'INVALID_COMPARISON_STORE',
      message: 'The original comparison or its complete inputs differ from the reused evidence',
    });
  if (
    !isDeepStrictEqual(
      result.sources,
      prepared.sources.map((source) => source.packet.source),
    ) ||
    !isDeepStrictEqual(
      result.association?.sourceSnapshot ?? result.sourceSnapshot,
      prepared.context.input.graph.sourceSnapshot,
    ) ||
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

export function validateComparisons(rows: Rows, context: Context) {
  for (const row of rows) {
    for (const previous of row.previousAttempts ?? [])
      validateRejectedOutput(previous.rejectedOutput, previous.report.outcome, previous.comparison);
    if (row.result)
      validateComparisonResult(row.result, prepare(context, row.id), context.selection.planHash);
  }
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
    : {
        ...envelope,
        pair: row?.id,
        state: row?.state,
        result: row?.result,
        previousAttempts: row?.previousAttempts,
      };
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
  const retry = options.retry
    ? store.retryFailed(options.retry, owner, options.maximumAttempts)
    : null;
  let processed = 0;
  while (processed < options.maxUnits) {
    const id = processed === 0 && retry !== null ? retry : store.claim(owner);
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
    validateComparisons(rows, context);
    return inspect(rows, context, options);
  }
  if (options.reuse && options.from) {
    const { reuseComparisons } = await import('./comparison-reuse.ts');
    reuseComparisons(options, context);
  }
  using store = new AssessmentStore(options.store, context.plan, comparisonContract);
  const previous = store.snapshot();
  validateComparisons(previous, context);
  const reused = previous.filter((row) => row.state === 'reviewed').length;
  const processed = await comparePending(options, context, store);
  const rows = store.snapshot();
  validateComparisons(rows, context);
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
