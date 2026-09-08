import { parseArgs } from 'node:util';
import { resolve } from 'node:path';
import { HivexError } from '../errors.ts';
import { parseLimit } from '../cli/arguments.ts';
import { loadSnapshot } from '../workspace/snapshot.ts';
import { createPlan } from './plan.ts';
import { extractCommand } from './command.ts';
import { IngestionStore } from './store.ts';

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
        'max-bytes': { type: 'string' },
      },
    }).values;
  } catch (error) {
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: error instanceof Error ? error.message : 'Invalid ingestion arguments',
    });
  }
}

function argumentsFor(args: string[]) {
  const values = input(args);
  if (Object.values(values).some((value) => value === ''))
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Ingestion options cannot be empty',
    });
  if (values.show !== undefined && values.discard !== undefined)
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Inspection and discard are separate commands',
    });
  if (
    (values.show !== undefined || values.discard !== undefined) &&
    [
      values.ref,
      values.collection,
      values.codex,
      values['max-units'],
      values.attempts,
      values['deadline-ms'],
    ].some((value) => value !== undefined)
  )
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Do not combine inspection or discard with execution options',
    });
  if (values.show === undefined && values['max-bytes'] !== undefined)
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: '--max-bytes belongs to result inspection',
    });
  const root = values.root ?? process.cwd();
  return {
    root,
    ref: values.ref,
    collection: values.collection,
    store: values.store ?? resolve(root, '.hivex/ingestion.sqlite'),
    binary: values.codex ?? 'codex',
    maxUnits: parseLimit(values['max-units'], { fallback: 20, minimum: 0, maximum: 2048 }),
    attempts: parseLimit(values.attempts, { fallback: 3, minimum: 1, maximum: 3 }),
    deadlineMilliseconds: parseLimit(values['deadline-ms'], {
      fallback: 600_000,
      minimum: 100,
      maximum: 900_000,
    }),
    show: values.show,
    discard: values.discard,
    maxBytes: parseLimit(values['max-bytes'], {
      fallback: 16_384,
      minimum: 1024,
      maximum: 8 * 1024 * 1024,
    }),
  };
}

export async function ingestCommand(args: string[]) {
  const options = argumentsFor(args);
  if (options.discard !== undefined) return IngestionStore.discard(options.store, options.discard);
  if (options.show !== undefined)
    return IngestionStore.result(options.store, options.show, options.maxBytes);
  const previous = IngestionStore.selection(options.store);
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
  const reused = store.counts().completed;
  const owner = crypto.randomUUID();
  let processed = 0;
  while (processed < options.maxUnits) {
    const id = store.claim(owner);
    if (id === null) break;
    const result = await extractCommand(
      [
        id,
        '--root',
        options.root,
        '--ref',
        snapshot.commit,
        '--codex',
        options.binary,
        '--attempts',
        String(options.attempts),
        '--deadline-ms',
        String(options.deadlineMilliseconds),
      ],
      (event) => store.checkpoint(id, owner, event),
    );
    store.complete(id, owner, result);
    processed += 1;
    if (result.status === 'failed') break;
  }
  const counts = store.counts();
  let status = 'partial';
  if (counts.failed) status = 'failed';
  else if (!counts.pending && !counts.unresolved) status = 'candidates-ready';
  return {
    command: 'ingest',
    accepted: false,
    status,
    planHash: plan.planHash,
    snapshot: plan.snapshot,
    processed,
    reused,
    ...counts,
    attempts: store.attempts(),
  };
}
