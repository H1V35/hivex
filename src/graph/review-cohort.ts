import {
  assessmentArguments,
  summarize,
  validateCompletedInvocation,
  retryableAssessment,
} from './assessment-cohort.ts';
import { isDeepStrictEqual } from 'node:util';
import { z } from 'zod';
import { HivexError } from '../errors.ts';
import { hash } from '../sources/markdown.ts';
import { knowledgeModel, nativeVersion, requestedPolicyHash } from '../model/profile.ts';
import {
  createReviewContext,
  prepareSourceReview,
  runSourceReview,
  sourceReviewSchema,
  satisfactory,
  validateReview,
  sourceReviewPrompt,
} from './source-review.ts';
import { AssessmentStore, type AssessmentPlan } from './assessment-store.ts';
import { usageSchema } from '../model/transcript.ts';
import { digest } from './snapshot.ts';

export const reviewResultSchema = z.looseObject({
  command: z.literal('graph'),
  operation: z.literal('review'),
  accepted: z.literal(false),
  status: z.enum(['reviewed', 'failed']),
  graphHash: digest,
  source: z.looseObject({ id: z.string() }),
  contract: z.looseObject({
    promptHash: digest,
    schemaHash: digest,
    nativeVersion: z.string(),
    requestedPolicyHash: digest,
  }),
  report: z.looseObject({ outcome: z.string(), usage: usageSchema.nullable() }),
  review: sourceReviewSchema.nullable(),
  association: z
    .strictObject({
      graphHash: digest,
      promptHash: digest,
      sourceSnapshot: z.strictObject({
        commit: z.string().regex(/^[a-f0-9]{40}$/),
        configHash: digest,
      }),
      originalHash: digest,
    })
    .optional(),
});
export type ReviewResult = z.infer<typeof reviewResultSchema>;

export const reviewContract = {
  applicationId: 0x48565852,
  parse: (value: unknown) => reviewResultSchema.parse(value),
  unitId: (result: ReviewResult) => result.source.id,
  binding: reviewBinding,
  retryable: retryableAssessment,
};

export function originalReview(result: ReviewResult) {
  const { association: _association, ...original } = result;
  return original;
}

export function reviewBinding(result: ReviewResult) {
  if (!result.association) return result;
  return {
    ...result,
    graphHash: result.association.graphHash,
    contract: { ...result.contract, promptHash: result.association.promptHash },
  };
}

function validateReviewProvenance(
  result: ReviewResult,
  prepared: ReturnType<typeof prepareSourceReview>,
) {
  if (
    result.association &&
    (result.association.originalHash !== hash(JSON.stringify(originalReview(result))) ||
      result.contract.promptHash !==
        hash(sourceReviewPrompt({ ...prepared.packet, graphHash: result.graphHash })))
  )
    throw new HivexError({
      code: 'INVALID_REVIEW_STORE',
      message:
        'The original receipt or its complete source-review inputs differ from the reused evidence',
    });
  if (
    !isDeepStrictEqual(result.source, prepared.packet.source) ||
    !isDeepStrictEqual(
      result.association?.sourceSnapshot ?? result.sourceSnapshot,
      prepared.input.graph.sourceSnapshot,
    ) ||
    !isDeepStrictEqual(result.model, knowledgeModel)
  )
    throw new HivexError({
      code: 'INVALID_REVIEW_STORE',
      message: 'Review provenance differs from its source or admitted model',
    });
}

export function validateSourceReviews(
  rows: ReturnType<AssessmentStore<ReviewResult>['snapshot']>,
  context: ReturnType<typeof createReviewContext>,
) {
  for (const row of rows) {
    const result = row.result;
    if (!result) continue;
    const prepared = prepareSourceReview(context, row.id);
    validateReviewProvenance(result, prepared);
    validateCompletedInvocation(result.report);
    if (result.review) validateReview(result.review, prepared);
    const passed =
      result.review !== null &&
      satisfactory(result.review) &&
      result.report.outcome === 'completed';
    if ((result.status === 'reviewed') !== passed)
      throw new HivexError({
        code: 'INVALID_REVIEW_STORE',
        message: 'A retained verdict is inconsistent with its assessment',
      });
  }
}

async function reviewPending(
  options: ReturnType<typeof assessmentArguments>,
  context: ReturnType<typeof createReviewContext>,
  store: AssessmentStore<ReviewResult>,
) {
  const owner = crypto.randomUUID();
  const retry = options.retry ? store.retryFailed(options.retry, owner, options.attempts) : null;
  let processed = 0;
  while (processed < options.maxUnits) {
    const id = processed === 0 && retry !== null ? retry : store.claim(owner);
    if (id === null) break;
    const result = await runSourceReview(prepareSourceReview(context, id), options);
    store.complete(id, owner, result);
    processed++;
    if (result.status === 'failed') break;
  }
  return processed;
}

export async function reviewCohortCommand(args: string[]) {
  const options = assessmentArguments(args, 'review');
  if (options.discard !== undefined)
    return AssessmentStore.discard(options.store, options.discard, reviewContract.applicationId);
  const context = createReviewContext(options);
  const plan = sourceReviewPlan(context);
  const envelope = {
    command: 'graph',
    operation: 'review-cohort',
    accepted: false,
    graphHash: plan.graphHash,
    planHash: hash(JSON.stringify(plan)),
  };
  if (options.show !== undefined || options.export) {
    const rows = AssessmentStore.read(options.store, plan, reviewContract);
    validateSourceReviews(rows, context);
    if (options.export) {
      const result = { ...envelope, plan, reviews: rows, ...summarize(rows) };
      if (Buffer.byteLength(JSON.stringify(result)) + 1 > options.maxBytes)
        throw new HivexError({
          code: 'REVIEW_OUTPUT_BUDGET',
          message: 'The complete review cohort exceeds the output budget',
        });
      return result;
    }
    const row = rows.find((row) => row.id === options.show);
    if (!row)
      throw new HivexError({
        code: 'SOURCE_NOT_FOUND',
        message: 'The review source is not in this cohort',
      });
    const result = {
      ...envelope,
      source: row.id,
      state: row.state,
      result: row.result,
      previousAttempts: row.previousAttempts,
    };
    if (Buffer.byteLength(JSON.stringify(result)) + 1 > options.maxBytes)
      throw new HivexError({
        code: 'REVIEW_OUTPUT_BUDGET',
        message: 'The complete retained review exceeds the output budget',
      });
    return result;
  }
  if (options.reuse && options.from) {
    const { reuseSourceReviews } = await import('./review-reuse.ts');
    reuseSourceReviews(options, context);
  }
  using store = new AssessmentStore(options.store, plan, reviewContract);
  const previous = store.snapshot();
  validateSourceReviews(previous, context);
  const reused = previous.filter((row) => row.state === 'reviewed').length;
  const processed = await reviewPending(options, context, store);
  const rows = store.snapshot();
  validateSourceReviews(rows, context);
  return { ...envelope, processed, reused, ...summarize(rows) };
}

export function sourceReviewPlan(context: ReturnType<typeof createReviewContext>) {
  return {
    graphHash: context.input.graph.hash,
    contract: {
      nativeVersion,
      requestedPolicyHash: requestedPolicyHash(),
      schemaHash: hash(JSON.stringify(z.toJSONSchema(sourceReviewSchema))),
    },
    sources: context.input.graph.sources.map((source) => ({
      id: source.id,
      promptHash: hash(prepareSourceReview(context, source.id).prompt),
    })),
  } satisfies AssessmentPlan;
}
