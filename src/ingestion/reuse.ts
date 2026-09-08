import { isDeepStrictEqual } from 'node:util';
import { lstatSync, readFileSync } from 'node:fs';
import { z } from 'zod';
import { HivexError } from '../errors.ts';
import { hash } from '../sources/markdown.ts';
import { loadSnapshot } from '../workspace/snapshot.ts';
import { createPlan } from './plan.ts';
import {
  IngestionStore,
  originalExtractionResult,
  parseIngestionPlan,
  summarizeRows,
  assertTransferSafe,
  validateResultForPlan,
  validateRows,
  type IngestionCohort,
  type IngestionResult,
  type IngestionRow,
} from './store.ts';

const maximumArchiveBytes = 128 * 1024 * 1024;
const digest = z.string().regex(/^[a-f0-9]{64}$/);
const archiveSchema = z.strictObject({
  command: z.literal('ingest'),
  mode: z.literal('export'),
  accepted: z.literal(false),
  planHash: digest,
  plan: z.unknown(),
  units: z.array(z.unknown()).max(2048),
  status: z.enum(['partial', 'failed', 'candidates-ready']),
  completed: z.number().int().nonnegative(),
  failed: z.number().int().nonnegative(),
  pending: z.number().int().nonnegative(),
  unresolved: z.number().int().nonnegative(),
  attempts: z.strictObject({
    recorded: z.number().int().nonnegative(),
    unresolved: z.number().int().nonnegative(),
    unknownUsage: z.number().int().nonnegative(),
    knownTotalTokens: z.number().int().nonnegative(),
  }),
});

function invalid(message: string): never {
  throw new HivexError({ code: 'INGESTION_ARCHIVE_MISMATCH', message });
}

function readJson(path: string): unknown {
  let bytes: Uint8Array;
  try {
    const stat = lstatSync(path);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.size > maximumArchiveBytes)
      invalid('Preserve a complete regular ingestion export of at most 128 MiB');
    bytes = readFileSync(path);
  } catch (error) {
    if (error instanceof HivexError) throw error;
    invalid('The ingestion export cannot be read as a regular file');
  }
  if (bytes.length > maximumArchiveBytes) invalid('The complete ingestion export exceeds 128 MiB');
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes)) as unknown;
  } catch {
    invalid('The ingestion export is not valid UTF-8 JSON');
  }
}

function readArchive(path: string) {
  const value = readJson(path);
  let archive: z.infer<typeof archiveSchema>;
  try {
    archive = archiveSchema.parse(value);
  } catch {
    invalid('Use a complete ingestion export with its plan and every retained unit');
  }
  let plan: ReturnType<typeof createPlan>;
  try {
    plan = parseIngestionPlan(archive.plan);
  } catch {
    invalid('The archived ingestion plan is altered or unsupported');
  }
  if (archive.planHash !== plan.planHash || JSON.stringify(archive.plan) !== JSON.stringify(plan))
    invalid('The archived ingestion plan hash does not match its complete plan');
  let rows: IngestionRow[];
  try {
    rows = validateRows(plan, archive.units);
  } catch {
    invalid('The archived ingestion rows do not match their plan or evidence');
  }
  const summary = summarizeRows(rows);
  if (
    summary.status !== archive.status ||
    summary.completed !== archive.completed ||
    summary.failed !== archive.failed ||
    summary.pending !== archive.pending ||
    summary.unresolved !== archive.unresolved ||
    !isDeepStrictEqual(summary.attempts, archive.attempts)
  )
    invalid('The archived ingestion summary differs from its complete rows');
  assertTransferSafe(rows);
  return { plan, rows };
}

function compatibleResults(
  previous: { plan: ReturnType<typeof createPlan>; rows: IngestionRow[] },
  next: ReturnType<typeof createPlan>,
) {
  const results = new Map<string, IngestionResult>();
  if (!isDeepStrictEqual(previous.plan.selection, next.selection)) return results;
  if (!isDeepStrictEqual(previous.plan.processing, next.processing)) return results;
  const currentUnits = new Map(next.units.map((unit) => [unit.id, unit]));
  for (const [index, row] of previous.rows.entries()) {
    const oldUnit = previous.plan.units[index];
    const currentUnit = currentUnits.get(row.id);
    if (!row.result || !oldUnit || !currentUnit || !isDeepStrictEqual(oldUnit, currentUnit))
      continue;
    const original = originalExtractionResult(row.result);
    const result: IngestionResult = {
      ...original,
      association: {
        planHash: next.planHash,
        snapshot: next.snapshot,
        originalHash: hash(JSON.stringify(original)),
      },
    };
    try {
      validateResultForPlan(result, row.id, next);
    } catch {
      continue;
    }
    results.set(row.id, result);
  }
  return results;
}

export function reuseIngestion(options: {
  root: string;
  store: string;
  archive: string;
  ref: string;
}) {
  const previous = readArchive(options.archive);
  const collection = previous.plan.selection.collection ?? undefined;
  const snapshot = loadSnapshot({
    root: options.root,
    ref: options.ref,
    selection: { collection },
  });
  const plan = createPlan(snapshot, collection ?? null);
  if (plan.summary.oversizedSources)
    throw new HivexError({
      code: 'INGESTION_REQUIRES_SECTIONS',
      message: 'Declare bounded complete sections before starting this cohort',
    });
  const results = compatibleResults(previous, plan);
  const cohort: IngestionCohort = IngestionStore.refresh({
    path: options.store,
    previous,
    next: { plan, results },
  });
  return {
    command: 'ingest',
    operation: 'reuse',
    accepted: false,
    planHash: plan.planHash,
    snapshot: plan.snapshot,
    processed: 0,
    reused: results.size,
    ...summarizeRows(cohort.rows),
  };
}
