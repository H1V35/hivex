import { parseArgs } from 'node:util';
import { resolve } from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { z } from 'zod';
import { parseLimit } from '../cli/arguments.ts';
import { HivexError } from '../errors.ts';
import { hash } from '../sources/markdown.ts';
import { knowledgeModel, nativeVersion, requestedPolicyHash } from '../model/profile.ts';
import { reportedProfileSchema } from './snapshot.ts';
import {
  createReviewContext,
  prepareSourceReview,
  runSourceReview,
  sourceReviewSchema,
  satisfactory,
  validateReview,
} from './source-review.ts';
import { ReviewStore, type ReviewPlan, type ReviewResult } from './review-store.ts';

function input(args: string[]) {
  try {
    return parseArgs({
      args,
      strict: true,
      options: {
        all: { type: 'boolean' },
        input: { type: 'string' },
        root: { type: 'string' },
        store: { type: 'string' },
        against: { type: 'string' },
        codex: { type: 'string' },
        'max-units': { type: 'string' },
        'deadline-ms': { type: 'string' },
        show: { type: 'string' },
        export: { type: 'boolean' },
        discard: { type: 'string' },
        'max-bytes': { type: 'string' },
      },
    }).values;
  } catch (error) {
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: error instanceof Error ? error.message : 'Invalid cohort review arguments',
    });
  }
}

function validateInspectionFlags(values: ReturnType<typeof input>) {
  if (
    !values.all &&
    [values.codex, values['max-units'], values['deadline-ms']].some((value) => value !== undefined)
  )
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Inspection and retirement cannot be mixed with execution options',
    });
  if ((values.all || values.discard !== undefined) && values['max-bytes'] !== undefined)
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: '--max-bytes belongs to inspection or export',
    });
  if (values.discard !== undefined && (values.input !== undefined || values.against !== undefined))
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Retirement uses the retained plan hash, not a replacement graph',
    });
}

function argumentsFor(args: string[]) {
  const values = input(args);
  if (
    (!values.input && !values.discard) ||
    [values.all, values.show !== undefined, values.export, values.discard !== undefined].filter(
      Boolean,
    ).length !== 1 ||
    Object.values(values).some((value) => value === '')
  )
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message:
        'Choose --all, --show, --export or --discard; non-discard operations require --input',
    });
  validateInspectionFlags(values);
  const root = values.root ?? process.cwd();
  return {
    root,
    input: values.input ?? '',
    against: values.against,
    store: values.store ?? resolve(root, '.hivex/reviews.sqlite'),
    binary: values.codex ?? 'codex',
    show: values.show,
    export: values.export ?? false,
    discard: values.discard,
    maxUnits: parseLimit(values['max-units'], { fallback: 20, minimum: 0, maximum: 2048 }),
    maxBytes: parseLimit(values['max-bytes'], {
      fallback: 16384,
      minimum: 1024,
      maximum: 128 * 1024 * 1024,
    }),
    deadlineMilliseconds: parseLimit(values['deadline-ms'], {
      fallback: 600000,
      minimum: 100,
      maximum: 900000,
    }),
  };
}

function summarize(rows: ReturnType<ReviewStore['snapshot']>) {
  const count = (state: string) => rows.filter((row) => row.state === state).length;
  const completed = count('reviewed');
  const failed = count('failed');
  const unresolved = count('running');
  let status = 'pending';
  if (completed === rows.length) status = 'reviewed';
  if (failed || unresolved) status = 'failed';
  return {
    status,
    completed,
    failed,
    unresolved,
    pending: count('pending'),
    reportedTokens: rows.reduce(
      (sum, row) => sum + (row.result?.report.usage?.totalTokens ?? 0),
      0,
    ),
    unmeasuredResults: rows.filter((row) => row.result && row.result.report.usage === null).length,
  };
}

function validateResults(
  rows: ReturnType<ReviewStore['snapshot']>,
  context: ReturnType<typeof createReviewContext>,
) {
  for (const row of rows) {
    const result = row.result;
    if (!result) continue;
    const prepared = prepareSourceReview(context, row.id);
    if (
      !isDeepStrictEqual(result.source, prepared.packet.source) ||
      !isDeepStrictEqual(result.sourceSnapshot, context.input.graph.sourceSnapshot) ||
      !isDeepStrictEqual(result.model, knowledgeModel)
    )
      throw new HivexError({
        code: 'INVALID_REVIEW_STORE',
        message: 'Review provenance differs from its source or admitted model',
      });
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

function validateCompletedInvocation(report: ReviewResult['report']) {
  if (report.outcome !== 'completed') return;
  const profile = z.looseObject(reportedProfileSchema.shape).safeParse(report.admission);
  if (
    !profile.success ||
    profile.data.model !== knowledgeModel.name ||
    profile.data.effort !== knowledgeModel.effort ||
    profile.data.modelProvider !== knowledgeModel.provider ||
    profile.data.authType !== 'chatgpt' ||
    profile.data.configuredEndpointOrigin !== 'https://chatgpt.com' ||
    report.cleanup !== 'confirmed' ||
    report.turnAccepted !== 'confirmed' ||
    typeof report.threadId !== 'string' ||
    typeof report.turnId !== 'string'
  )
    throw new HivexError({
      code: 'INVALID_REVIEW_STORE',
      message: 'A completed review lacks its admitted native invocation evidence',
    });
}

async function reviewPending(
  options: ReturnType<typeof argumentsFor>,
  context: ReturnType<typeof createReviewContext>,
  store: ReviewStore,
) {
  const owner = crypto.randomUUID();
  let processed = 0;
  while (processed < options.maxUnits) {
    const id = store.claim(owner);
    if (id === null) break;
    const result = await runSourceReview(prepareSourceReview(context, id), options);
    store.complete(id, owner, result);
    processed++;
    if (result.status === 'failed') break;
  }
  return processed;
}

export async function reviewCohortCommand(args: string[]) {
  const options = argumentsFor(args);
  if (options.discard !== undefined) return ReviewStore.discard(options.store, options.discard);
  const context = createReviewContext(options);
  const plan: ReviewPlan = {
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
  };
  const envelope = {
    command: 'graph',
    operation: 'review-cohort',
    accepted: false,
    graphHash: plan.graphHash,
    planHash: hash(JSON.stringify(plan)),
  };
  if (options.show !== undefined || options.export) {
    const rows = ReviewStore.read(options.store, plan);
    validateResults(rows, context);
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
    const result = { ...envelope, source: row.id, state: row.state, result: row.result };
    if (Buffer.byteLength(JSON.stringify(result)) + 1 > options.maxBytes)
      throw new HivexError({
        code: 'REVIEW_OUTPUT_BUDGET',
        message: 'The complete retained review exceeds the output budget',
      });
    return result;
  }
  using store = new ReviewStore(options.store, plan);
  const previous = store.snapshot();
  validateResults(previous, context);
  const reused = previous.filter((row) => row.state === 'reviewed').length;
  const processed = await reviewPending(options, context, store);
  const rows = store.snapshot();
  validateResults(rows, context);
  return { ...envelope, processed, reused, ...summarize(rows) };
}
