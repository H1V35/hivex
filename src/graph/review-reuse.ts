import { lstatSync, readFileSync } from 'node:fs';
import { isDeepStrictEqual } from 'node:util';
import { z } from 'zod';
import { HivexError } from '../errors.ts';
import { hash } from '../sources/markdown.ts';
import { readGraph, parseGraphDocument } from './verify.ts';
import { createReviewContext, prepareSourceReview, sourceReviewPrompt } from './source-review.ts';
import { AssessmentStore, validateAssessmentBinding } from './assessment-store.ts';
import { unresolvedInvocation } from './assessment-cohort.ts';
import {
  originalReview,
  reviewBinding,
  reviewContract,
  reviewResultSchema,
  sourceReviewPlan,
  validateSourceReviews,
  type ReviewResult,
} from './review-cohort.ts';

const archiveSchema = z.object({
  command: z.literal('graph'),
  operation: z.literal('review-cohort'),
  accepted: z.literal(false),
  graphHash: z.string(),
  planHash: z.string(),
  plan: z.unknown(),
  reviews: z
    .array(
      z.union([
        z.object({ id: z.string(), state: z.enum(['pending', 'running']), result: z.null() }),
        z.object({
          id: z.string(),
          state: z.enum(['reviewed', 'failed']),
          result: reviewResultSchema,
        }),
      ]),
    )
    .max(2048),
});
type Context = ReturnType<typeof createReviewContext>;

function invalid(message: string): never {
  throw new HivexError({ code: 'REVIEW_ARCHIVE_MISMATCH', message });
}

function readArchive(path: string, context: Context) {
  const stat = lstatSync(path);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size > 128 * 1024 * 1024)
    invalid('Preserve a complete regular review export of at most 128 MiB');
  const bytes = readFileSync(path);
  if (bytes.length > 128 * 1024 * 1024) invalid('The complete review archive exceeds 128 MiB');
  const archive = archiveSchema.parse(parseGraphDocument(bytes));
  const plan = sourceReviewPlan(context);
  if (
    !isDeepStrictEqual(archive.plan, plan) ||
    archive.graphHash !== plan.graphHash ||
    archive.planHash !== hash(JSON.stringify(plan)) ||
    archive.reviews.length !== plan.sources.length
  )
    invalid('The archive must preserve the previous graph and its complete processing plan');
  for (const [index, row] of archive.reviews.entries()) {
    const source = plan.sources[index];
    if (
      !source ||
      source.id !== row.id ||
      (row.state === 'reviewed' || row.state === 'failed') !== (row.result !== null)
    )
      invalid('The archive must retain each source and its state exactly once');
    if (!row.result) continue;
    if (row.state !== row.result.status)
      invalid('The archive verdict differs from its source state');
    validateAssessmentBinding(
      reviewBinding(row.result),
      { id: row.id, actualId: row.result.source.id, promptHash: source.promptHash },
      plan,
    );
  }
  validateSourceReviews(archive.reviews, context);
  if (
    archive.reviews.some(
      (row) => row.state === 'running' || (row.result && unresolvedInvocation(row.result.report)),
    )
  )
    throw new HivexError({
      code: 'REVIEW_UNRESOLVED',
      message: 'Resolve uncertain starts, interruptions and cleanup before replacing this cohort',
    });
  return { plan, rows: archive.reviews };
}

function compatibleReviews(previous: ReturnType<typeof readArchive>, context: Context) {
  const results = new Map<string, ReviewResult>();
  for (const row of previous.rows) {
    if (!row.result || !context.sources.has(row.id) || !context.input.sources.has(row.id)) continue;
    const original = originalReview(row.result);
    const prepared = prepareSourceReview(context, row.id);
    const originalPrompt = sourceReviewPrompt({
      ...prepared.packet,
      graphHash: original.graphHash,
    });
    if (hash(originalPrompt) !== original.contract.promptHash) continue;
    const result: ReviewResult = {
      ...original,
      association: {
        graphHash: context.input.graph.hash,
        promptHash: hash(prepared.prompt),
        sourceSnapshot: context.input.graph.sourceSnapshot,
        originalHash: hash(JSON.stringify(original)),
      },
    };
    validateSourceReviews([{ id: row.id, state: result.status, result }], context);
    results.set(row.id, result);
  }
  return results;
}

export function reuseSourceReviews(
  options: { store: string; root: string; reuse?: string; from?: string },
  context: Context,
) {
  if (!options.reuse || !options.from) invalid('Review reuse requires both --reuse and --from');
  const input = readGraph(options.from);
  const previous = readArchive(
    options.reuse,
    createReviewContext({ input, root: options.root, against: input.graph.sourceSnapshot.commit }),
  );
  AssessmentStore.refresh(
    {
      path: options.store,
      previous,
      next: { plan: sourceReviewPlan(context), results: compatibleReviews(previous, context) },
    },
    reviewContract,
  );
}
