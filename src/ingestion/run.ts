import { parseArgs } from 'node:util';
import { resolve } from 'node:path';
import { HivexError } from '../errors.ts';
import { parseLimit } from '../cli/arguments.ts';
import { loadSnapshot, type Snapshot } from '../workspace/snapshot.ts';
import { createPlan } from './plan.ts';
import { extractSource } from './command.ts';
import { IngestionStore } from './store.ts';
import { reviseCommand } from './revision.ts';
import { reuseIngestion } from './reuse.ts';

function input(args: string[]) {
  try {
    return parseArgs({
      args,
      strict: true,
      options: {
        root: { type: 'string' },
        ref: { type: 'string' },
        collection: { type: 'string' },
        store: { type: 'string' },
        codex: { type: 'string' },
        'max-units': { type: 'string' },
        attempts: { type: 'string' },
        'deadline-ms': { type: 'string' },
        show: { type: 'string' },
        discard: { type: 'string' },
        'retry-failed': { type: 'string' },
        'max-bytes': { type: 'string' },
        export: { type: 'boolean' },
        reuse: { type: 'string' },
      },
    }).values;
  } catch (error) {
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: error instanceof Error ? error.message : 'Invalid ingestion arguments',
    });
  }
}

type IngestionValues = ReturnType<typeof input>;

function assertMode(values: IngestionValues) {
  const modes = [
    values.show !== undefined,
    values.discard !== undefined,
    values.export === true,
    values.reuse !== undefined,
  ].filter(Boolean).length;
  if (modes > 1)
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Choose one ingestion operation at a time',
    });
}

function assertReadOnlyOptions(values: IngestionValues) {
  if (
    (values.show !== undefined || values.discard !== undefined || values.export === true) &&
    [
      values.ref,
      values.collection,
      values.codex,
      values['max-units'],
      values.attempts,
      values['deadline-ms'],
      values['retry-failed'],
      values.reuse,
    ].some((value) => value !== undefined)
  )
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Do not combine inspection or discard with execution options',
    });
}

function assertReuseOptions(values: IngestionValues, maxUnits: number) {
  if (values.reuse === undefined) return;
  if (values.ref === undefined)
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Reuse requires an explicit --ref destination snapshot',
    });
  if (
    [
      values.collection,
      values.codex,
      values.attempts,
      values['deadline-ms'],
      values['retry-failed'],
    ].some((value) => value !== undefined)
  )
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Reuse accepts only --root, --store, --reuse, --ref and --max-units 0',
    });
  if (maxUnits !== 0)
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Reuse is a no-model-call transition and requires --max-units 0',
    });
}

function argumentsFor(args: string[]) {
  const values = input(args);
  if (Object.values(values).some((value) => value === ''))
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Ingestion options cannot be empty',
    });
  assertMode(values);
  assertReadOnlyOptions(values);
  if (values.show === undefined && values.export !== true && values['max-bytes'] !== undefined)
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: '--max-bytes belongs to result inspection or export',
    });
  const root = values.root ?? process.cwd();
  const maxUnits = parseLimit(values['max-units'], { fallback: 20, minimum: 0, maximum: 2048 });
  if (values['retry-failed'] !== undefined && maxUnits === 0)
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'A retry requires at least one processing slot',
    });
  assertReuseOptions(values, maxUnits);
  return {
    root,
    ref: values.ref,
    collection: values.collection,
    store: values.store ?? resolve(root, '.hivex/ingestion.sqlite'),
    binary: values.codex ?? 'codex',
    maxUnits,
    retry: values['retry-failed'],
    attempts: parseLimit(values.attempts, { fallback: 3, minimum: 1, maximum: 3 }),
    deadlineMilliseconds: parseLimit(values['deadline-ms'], {
      fallback: 600_000,
      minimum: 100,
      maximum: 1_800_000,
    }),
    show: values.show,
    discard: values.discard,
    export: values.export ?? false,
    archive: values.reuse,
    maxBytes: parseLimit(values['max-bytes'], {
      fallback: 16_384,
      minimum: 1024,
      maximum: values.export === true ? 128 * 1024 * 1024 : 8 * 1024 * 1024,
    }),
  };
}

export async function ingestCommand(args: string[]) {
  if (args.some((argument) => argument === '--revise' || argument.startsWith('--revise=')))
    return reviseCommand(args);
  const options = argumentsFor(args);
  if (options.discard !== undefined) return IngestionStore.discard(options.store, options.discard);
  if (options.export) return IngestionStore.export(options.store, options.maxBytes);
  if (options.show !== undefined)
    return IngestionStore.result(options.store, options.show, options.maxBytes);
  if (options.archive !== undefined)
    return reuseIngestion({
      root: options.root,
      store: options.store,
      archive: options.archive,
      ref: options.ref ?? 'HEAD',
    });
  const previous = IngestionStore.selection(options.store);
  if (options.retry && !previous)
    throw new HivexError({
      code: 'INGESTION_RETRY_NOT_ALLOWED',
      message: 'Retry requires an existing retained cohort',
    });
  const collection = options.collection ?? previous?.collection ?? undefined;
  const snapshot = loadSnapshot({
    root: options.root,
    ref: options.ref ?? previous?.ref ?? 'HEAD',
    selection: { collection },
  });
  const plan = createPlan(snapshot, collection ?? null);
  if (plan.summary.oversizedSources)
    throw new HivexError({
      code: 'INGESTION_REQUIRES_SECTIONS',
      message: 'Declare bounded complete sections before starting this cohort',
    });
  using store = new IngestionStore(options.store, plan);
  const reused = store.progress().completed;
  const processed = await processSources(options, snapshot, store);
  return {
    command: 'ingest',
    accepted: false,
    planHash: plan.planHash,
    snapshot: plan.snapshot,
    processed,
    reused,
    ...store.progress(),
  };
}

async function processSources(
  options: ReturnType<typeof argumentsFor>,
  snapshot: Snapshot,
  store: IngestionStore,
) {
  const sources = new Map(snapshot.sources.map((source) => [source.id, source]));
  const owner = crypto.randomUUID();
  const recovery = options.retry
    ? store.retryFailed(options.retry, owner, options.attempts)
    : undefined;
  let processed = 0;
  while (processed < options.maxUnits) {
    const retrying = processed === 0 && options.retry !== undefined;
    const id = retrying ? (options.retry ?? null) : store.claim(owner);
    if (id === null) break;
    const source = sources.get(id);
    if (!source)
      throw new HivexError({
        code: 'SOURCE_NOT_FOUND',
        message: 'The claim is not in the frozen snapshot',
      });
    const result = await extractSource(
      {
        source,
        snapshot,
        binary: options.binary,
        attempts: options.attempts,
        deadlineMilliseconds: options.deadlineMilliseconds,
        ...(retrying ? recovery : {}),
      },
      (event) => store.checkpoint(id, owner, event),
    );
    store.complete(id, owner, result);
    processed += 1;
    if (result.status === 'failed') break;
  }
  return processed;
}
