import { lstatSync, readFileSync } from 'node:fs';
import { isDeepStrictEqual } from 'node:util';
import { z } from 'zod';
import { HivexError } from '../errors.ts';
import { hash } from '../sources/markdown.ts';
import { readGraph, parseGraphDocument } from './verify.ts';
import { createReviewContext } from './source-review.ts';
import { prepareComparison } from './comparison.ts';
import { unresolvedInvocation } from './assessment-cohort.ts';
import { AssessmentStore, validateAssessmentBinding } from './assessment-store.ts';
import {
  comparisonBinding,
  comparisonContract,
  comparisonResultSchema,
  originalComparison,
  prepareComparisonCohort,
  validateComparisons,
  type ComparisonResult,
} from './comparison-cohort.ts';

const archiveSchema = z.object({
  command: z.literal('graph'),
  operation: z.literal('comparison-cohort'),
  accepted: z.literal(false),
  graphHash: z.string(),
  planHash: z.string(),
  selectionHash: z.string(),
  plan: z.unknown(),
  selection: z.unknown(),
  comparisons: z
    .array(
      z.union([
        z.object({
          id: z.string(),
          state: z.enum(['pending', 'running']),
          result: z.null(),
          previousAttempts: z.array(comparisonResultSchema).max(2).optional(),
        }),
        z.object({
          id: z.string(),
          state: z.enum(['reviewed', 'failed']),
          result: comparisonResultSchema,
          previousAttempts: z.array(comparisonResultSchema).max(2).optional(),
        }),
      ]),
    )
    .max(2048),
});
type Context = ReturnType<typeof prepareComparisonCohort>;

function invalid(message: string): never {
  throw new HivexError({ code: 'REVIEW_ARCHIVE_MISMATCH', message });
}

function readArchive(options: { reuse: string; from: string; root: string }) {
  const stat = lstatSync(options.reuse);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size > 128 * 1024 * 1024)
    invalid('Preserve a complete regular comparison export of at most 128 MiB');
  const bytes = readFileSync(options.reuse);
  if (bytes.length > 128 * 1024 * 1024) invalid('The complete comparison archive exceeds 128 MiB');
  const archive = archiveSchema.parse(parseGraphDocument(bytes));
  const settings = z
    .object({
      lexical: z.object({ neighbors: z.number().int().min(1).max(8) }).optional(),
    })
    .parse(archive.selection);
  const input = readGraph(options.from);
  const context = prepareComparisonCohort(
    createReviewContext({ input, root: options.root, against: input.graph.sourceSnapshot.commit }),
    settings.lexical?.neighbors ?? 0,
  );
  const { plan, selection } = context;
  if (
    !isDeepStrictEqual(archive.plan, plan) ||
    !isDeepStrictEqual(archive.selection, selection) ||
    archive.graphHash !== plan.graphHash ||
    archive.planHash !== hash(JSON.stringify(plan)) ||
    archive.selectionHash !== selection.planHash ||
    archive.comparisons.length !== plan.sources.length
  )
    invalid('The archive must preserve the previous graph, selection and complete processing plan');
  for (const [index, row] of archive.comparisons.entries()) {
    const source = plan.sources[index];
    if (!source || source.id !== row.id)
      invalid('The archive must retain every selected pair and its state exactly once');
    if (!row.result) continue;
    if (row.state !== row.result.status) invalid('The archive verdict differs from its pair state');
    validateAssessmentBinding(
      comparisonBinding(row.result),
      {
        id: row.id,
        actualId: comparisonContract.unitId(row.result),
        promptHash: source.promptHash,
      },
      plan,
    );
  }
  validateComparisons(archive.comparisons, context);
  if (
    archive.comparisons.some(
      (row) => row.state === 'running' || (row.result && unresolvedInvocation(row.result.report)),
    )
  )
    throw new HivexError({
      code: 'REVIEW_UNRESOLVED',
      message: 'Resolve uncertain starts, interruptions and cleanup before replacing this cohort',
    });
  return { plan, rows: archive.comparisons, selection };
}

function compatibleComparisons(previous: ReturnType<typeof readArchive>, context: Context) {
  const results = new Map<string, ComparisonResult>();
  for (const row of previous.rows) {
    const ids = context.pairs.get(row.id);
    if (!row.result || !ids) continue;
    const original = originalComparison(row.result);
    const prepared = prepareComparison(context.context, ids);
    if (
      hash(prepared.prompt) !== original.contract.promptHash ||
      !isDeepStrictEqual(
        original.sources,
        prepared.sources.map((source) => source.packet.source),
      )
    )
      continue;
    const originalSelectionHash =
      row.result.association?.originalSelectionHash ?? previous.selection.planHash;
    const result: ComparisonResult = {
      ...original,
      association: {
        graphHash: context.plan.graphHash,
        sourceSnapshot: context.context.input.graph.sourceSnapshot,
        selectionHash: context.selection.planHash,
        originalSelectionHash,
        originalHash: hash(
          JSON.stringify({ result: original, selectionHash: originalSelectionHash }),
        ),
      },
    };
    validateComparisons([{ id: row.id, state: result.status, result }], context);
    results.set(row.id, result);
  }
  return results;
}

export function reuseComparisons(
  options: { store: string; root: string; reuse?: string; from?: string },
  context: Context,
) {
  if (!options.reuse || !options.from) invalid('Comparison reuse requires both --reuse and --from');
  const previous = readArchive({ ...options, reuse: options.reuse, from: options.from });
  AssessmentStore.refresh(
    {
      path: options.store,
      previous,
      next: { plan: context.plan, results: compatibleComparisons(previous, context) },
    },
    comparisonContract,
  );
}
