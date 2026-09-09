import { isDeepStrictEqual, parseArgs } from 'node:util';
import {
  closeSync,
  existsSync,
  fsyncSync,
  lstatSync,
  mkdirSync,
  openSync,
  readFileSync,
  realpathSync,
  renameSync,
  unlinkSync,
  writeFileSync,
} from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { z } from 'zod';
import { parseLimit } from './cli/arguments.ts';
import { HivexError } from './errors.ts';
import { admitCommand, readProjection } from './graph/admission.ts';
import { unresolvedInvocation as unresolvedAssessment } from './graph/assessment-cohort.ts';
import {
  comparisonCohortCommand,
  comparisonContract,
  prepareComparisonCohort,
  validateComparisons,
} from './graph/comparison-cohort.ts';
import {
  comparisonContextSchema,
  readComparisonContext,
  type ComparisonContext,
} from './graph/context.ts';
import { AssessmentStore } from './graph/assessment-store.ts';
import { createReviewContext } from './graph/source-review.ts';
import { graphCommand } from './graph/command.ts';
import { reviewCohortCommand } from './graph/review-cohort.ts';
import { buildGraph, inputHash } from './graph/build.ts';
import { ingestCommand } from './ingestion/run.ts';
import { IngestionStore, unresolvedInvocation as unresolvedExtraction } from './ingestion/store.ts';
import { createPlan } from './ingestion/plan.ts';
import { hash } from './sources/markdown.ts';
import { loadSnapshot } from './workspace/snapshot.ts';

const maximumExportBytes = 128 * 1024 * 1024;
const maximumCandidateBytes = 64 * 1024 * 1024;
const maximumOutputBytes = 256 * 1024 * 1024;
const digest = z.string().regex(/^[a-f0-9]{64}$/);
const commit = z.string().regex(/^[a-f0-9]{40}$/);

const fileRecordSchema = z.strictObject({
  name: z.string(),
  hash: digest,
  bytes: z.number().int().positive(),
});
const transitionSchema = z.strictObject({
  format: z.literal('hivex-update-transition'),
  version: z.literal(1),
  state: z.enum(['archiving', 'active', 'complete']),
  target: z.strictObject({
    commit,
    inputHash: digest,
    comparisonContext: comparisonContextSchema.optional(),
  }),
  previous: z.strictObject({
    admittedHash: digest.nullable(),
    candidateHash: digest.nullable(),
  }),
  expectedAcceptedHash: digest.nullable(),
  files: z.strictObject({
    admitted: fileRecordSchema.nullable(),
    candidate: fileRecordSchema.nullable(),
    ingestion: fileRecordSchema.nullable(),
    reviews: fileRecordSchema.nullable(),
    comparisons: fileRecordSchema.nullable(),
  }),
});
type Transition = z.infer<typeof transitionSchema>;

const checkpointSchema = z.strictObject({
  format: z.literal('hivex-update'),
  version: z.literal(1),
  root: z.string(),
  output: z.string(),
  candidate: z.string(),
  transition: z.string(),
  stores: z.strictObject({
    ingestion: z.string(),
    reviews: z.string(),
    comparisons: z.string(),
  }),
  target: z.strictObject({
    commit,
    collection: z.string().nullable(),
    neighbors: z.number().int().min(0).max(8),
    inputHash: digest,
    comparisonContext: comparisonContextSchema.optional(),
  }),
  phase: z.enum(['ingest', 'build', 'review', 'compare', 'admit', 'admitted']),
  candidateHash: digest.nullable(),
  admittedHash: digest.nullable(),
});
type Checkpoint = z.infer<typeof checkpointSchema>;

type UpdateOptions = {
  root: string;
  output: string;
  ref?: string;
  refExplicit: boolean;
  collection?: string;
  collectionExplicit: boolean;
  neighbors: number;
  neighborsExplicit: boolean;
  maxUnits: number;
  binary: string;
  deadlineMilliseconds: number;
  comparisonContextFile?: string;
  comparisonContext?: ComparisonContext;
};

type Paths = {
  runtime: string;
  checkpoint: string;
  candidate: string;
  transition: string;
  lock: string;
  ingestion: string;
  reviews: string;
  comparisons: string;
  output: string;
};

type Target = {
  snapshot: ReturnType<typeof loadSnapshot>;
  plan: ReturnType<typeof createPlan>;
};

type Blocker = {
  id: string | null;
  kind: 'adverse' | 'uncertain' | 'failed' | 'contract-mismatch' | 'invalid';
  message: string;
};

type PhaseResult = {
  ready: boolean;
  response?: Record<string, unknown>;
  blockers?: Blocker[];
};

function invalid(message: string, code = 'INVALID_ARGUMENT'): never {
  throw new HivexError({ code, message });
}

function parseUpdateArguments(args: string[]): UpdateOptions {
  let values;
  try {
    values = parseArgs({
      args,
      strict: true,
      options: {
        root: { type: 'string' },
        ref: { type: 'string' },
        collection: { type: 'string' },
        output: { type: 'string' },
        neighbors: { type: 'string' },
        'max-units': { type: 'string' },
        codex: { type: 'string' },
        'deadline-ms': { type: 'string' },
        'comparison-context': { type: 'string' },
      },
    }).values;
  } catch (error) {
    invalid(error instanceof Error ? error.message : 'Invalid update arguments');
  }
  if (!values.output || Object.values(values).some((value) => value === ''))
    invalid('Update requires --output and nonempty options');
  const root = resolve(values.root ?? process.cwd());
  return {
    root,
    output: resolve(root, values.output),
    ref: values.ref,
    refExplicit: values.ref !== undefined,
    collection: values.collection,
    collectionExplicit: values.collection !== undefined,
    neighbors: parseLimit(values.neighbors, { fallback: 0, minimum: 0, maximum: 8 }),
    neighborsExplicit: values.neighbors !== undefined,
    maxUnits: parseLimit(values['max-units'], { fallback: 20, minimum: 0, maximum: 2048 }),
    binary: values.codex ?? 'codex',
    comparisonContextFile:
      values['comparison-context'] === undefined
        ? undefined
        : resolve(root, values['comparison-context']),
    deadlineMilliseconds: parseLimit(values['deadline-ms'], {
      fallback: 600_000,
      minimum: 100,
      maximum: 1_800_000,
    }),
  };
}

function pathsFor(options: UpdateOptions): Paths {
  const runtime = resolve(options.root, '.hivex');
  return {
    runtime,
    checkpoint: join(runtime, 'update.json'),
    candidate: join(runtime, 'update-candidate.json'),
    transition: join(runtime, 'update-transition'),
    lock: join(runtime, 'update.lock'),
    ingestion: join(runtime, 'ingestion.sqlite'),
    reviews: join(runtime, 'reviews.sqlite'),
    comparisons: join(runtime, 'comparisons.sqlite'),
    output: options.output,
  };
}

function canonicalPath(path: string): string {
  try {
    return realpathSync(path);
  } catch (error) {
    if (!(error instanceof Error && 'code' in error && error.code === 'ENOENT')) throw error;
    return join(canonicalPath(dirname(path)), basename(path));
  }
}

function validateOutputPath(paths: Paths) {
  const reserved = [paths.checkpoint, paths.candidate, paths.lock];
  for (const store of [paths.ingestion, paths.reviews, paths.comparisons])
    reserved.push(store, ...['-journal', '-wal', '-shm'].map((suffix) => store + suffix));
  reserved.push(...reserved.map((path) => `${path}.pending`));
  // Reserve case variants as well, including filenames not yet created on case-insensitive disks.
  const key = (path: string) => canonicalPath(path).normalize('NFC').toLowerCase();
  const names = new Set(reserved.map(key));
  const directory = key(paths.transition);
  for (const path of [paths.output, `${paths.output}.pending`]) {
    const name = key(path);
    const stat = existsSync(path) ? lstatSync(path) : null;
    const alias =
      stat &&
      reserved.some((file) => {
        if (!existsSync(file)) return false;
        const other = lstatSync(file);
        return stat.dev === other.dev && stat.ino === other.ino;
      });
    if (
      names.has(name) ||
      name === key(paths.runtime) ||
      name === directory ||
      name.startsWith(directory + '/') ||
      alias
    )
      invalid(`Output collides with a reserved update path: ${path}`, 'UPDATE_PATH_COLLISION');
  }
}

function validateContextPath(paths: Paths, path?: string) {
  if (path === undefined) return;
  validateOutputPath({ ...paths, output: path });
  const key = (value: string) => canonicalPath(value).normalize('NFC').toLowerCase();
  const stat = lstatSync(path);
  for (const output of [paths.output, `${paths.output}.pending`]) {
    const other = existsSync(output) ? lstatSync(output) : null;
    if (key(path) === key(output) || (other && stat.dev === other.dev && stat.ino === other.ino))
      invalid(
        'Comparison context must not alias the managed output or its pending file',
        'UPDATE_PATH_COLLISION',
      );
  }
}

function ensureDirectory(path: string) {
  try {
    const stat = lstatSync(path);
    if (!stat.isDirectory() || stat.isSymbolicLink())
      invalid('Update requires a regular directory', 'UPDATE_PATH_INVALID');
  } catch (error) {
    if (!(error instanceof Error && 'code' in error && error.code === 'ENOENT')) throw error;
    mkdirSync(path, { recursive: true, mode: 0o700 });
  }
}

function regularFile(path: string) {
  try {
    const stat = lstatSync(path);
    if (stat.isSymbolicLink() || !stat.isFile())
      invalid('Update requires regular files', 'UPDATE_PATH_INVALID');
    return stat;
  } catch (error) {
    if (error instanceof Error && 'code' in error && error.code === 'ENOENT') return null;
    throw error;
  }
}

function atomicWrite(path: string, value: string, mode = 0o600) {
  ensureDirectory(dirname(path));
  const pending = `${path}.pending`;
  if (regularFile(pending)) {
    if (readFileSync(pending, 'utf8') !== value)
      invalid(`An unfinished artifact needs inspection: ${pending}`, 'UPDATE_ARTIFACT_PENDING');
    renameSync(pending, path);
    return;
  }
  let fd: number | undefined;
  try {
    fd = openSync(pending, 'wx', mode);
    writeFileSync(fd, value);
    fsyncSync(fd);
    closeSync(fd);
    fd = undefined;
    renameSync(pending, path);
  } catch (error) {
    if (fd !== undefined) closeSync(fd);
    throw error;
  }
}

function pendingWrite(path: string, value: string, mode = 0o600) {
  ensureDirectory(dirname(path));
  const pending = `${path}.pending`;
  if (regularFile(pending)) {
    if (readFileSync(pending, 'utf8') !== value)
      invalid(`An unfinished artifact needs inspection: ${pending}`, 'UPDATE_ARTIFACT_PENDING');
    return pending;
  }
  const fd = openSync(pending, 'wx', mode);
  try {
    writeFileSync(fd, value);
    fsyncSync(fd);
  } finally {
    closeSync(fd);
  }
  return pending;
}

function jsonText(value: unknown) {
  return JSON.stringify(value) + '\n';
}

function readJson(path: string, maximumBytes: number) {
  const stat = regularFile(path);
  if (!stat) return null;
  if (stat.size > maximumBytes)
    invalid(`Artifact exceeds ${maximumBytes} bytes`, 'UPDATE_ARTIFACT_INVALID');
  try {
    return JSON.parse(readFileSync(path, 'utf8')) as unknown;
  } catch {
    invalid(`Artifact is not valid JSON: ${path}`, 'UPDATE_ARTIFACT_INVALID');
  }
}

function readCheckpoint(options: UpdateOptions, paths: Paths) {
  const value = readJson(paths.checkpoint, 8 * 1024 * 1024);
  const current = value === null ? null : checkpointSchema.parse(value);
  if (current) validateCheckpoint(options, paths, current);
  const pending = `${paths.checkpoint}.pending`;
  const next = readJson(pending, 8 * 1024 * 1024);
  if (next === null) return current;
  const checkpoint = checkpointSchema.parse(next);
  validateCheckpoint(options, paths, checkpoint);
  validatePendingCheckpoint(paths, checkpoint, current);
  renameSync(pending, paths.checkpoint);
  return checkpoint;
}

function validatePendingCheckpoint(
  paths: Paths,
  checkpoint: Checkpoint,
  current: Checkpoint | null,
) {
  validatePendingTarget(paths, checkpoint, current);
  const { target } = checkpoint;
  const plan = createPlan(
    loadSnapshot({
      root: checkpoint.root,
      ref: target.commit,
      selection: { collection: target.collection ?? undefined },
    }),
    target.collection,
  );
  if (inputHash(plan) !== target.inputHash)
    invalid('The pending checkpoint differs from its processing inputs', 'UPDATE_ARTIFACT_PENDING');
  if (['review', 'compare', 'admit', 'admitted'].includes(checkpoint.phase)) {
    const candidate = candidateInfo(paths, checkpoint.root);
    if (
      !candidate ||
      candidate.hash !== checkpoint.candidateHash ||
      candidate.inputHash !== target.inputHash
    )
      invalid('The pending checkpoint differs from its candidate', 'UPDATE_ARTIFACT_PENDING');
  }
  if (checkpoint.phase === 'admitted') {
    const admitted = outputProjection(paths.output, checkpoint.root, target.commit);
    if (!admitted || !admittedFresh(admitted) || admitted.check.hash !== checkpoint.admittedHash)
      invalid('The pending checkpoint lacks its verified admission', 'UPDATE_ARTIFACT_PENDING');
  }
}

function validatePendingTarget(paths: Paths, checkpoint: Checkpoint, current: Checkpoint | null) {
  const { target } = checkpoint;
  if (
    current &&
    !isDeepStrictEqual(
      [current.target.collection, current.target.neighbors],
      [target.collection, target.neighbors],
    )
  )
    invalid('The pending checkpoint changes the frozen selection', 'UPDATE_ARTIFACT_PENDING');
  if (
    current &&
    !isDeepStrictEqual(current.target, target) &&
    !(current.phase === 'admitted' && checkpoint.phase === 'ingest')
  ) {
    const { comparisonContext: _previous, ...oldTarget } = current.target;
    const { comparisonContext: _next, ...nextTarget } = target;
    if (!isDeepStrictEqual(oldTarget, nextTarget))
      invalid('The pending checkpoint changes an active target', 'UPDATE_ARTIFACT_PENDING');
    validatePreparedComparisons(paths, checkpoint, current);
  }
}

function writeCheckpoint(path: string, checkpoint: Checkpoint) {
  atomicWrite(path, jsonText(checkpoint));
}

function transitionFilePath(paths: Paths, name: keyof Transition['files']) {
  return join(paths.transition, `${name}.json`);
}

function readTransition(paths: Paths) {
  try {
    const directory = lstatSync(paths.transition);
    if (!directory.isDirectory() || directory.isSymbolicLink())
      invalid('The transition path is not a regular directory', 'UPDATE_RETENTION_REQUIRED');
  } catch (error) {
    if (error instanceof Error && 'code' in error && error.code === 'ENOENT') return null;
    throw error;
  }
  const manifest = join(paths.transition, 'manifest.json');
  const pending = `${manifest}.pending`;
  const value = readJson(manifest, 8 * 1024 * 1024);
  const next = readJson(pending, 8 * 1024 * 1024);
  const current = value === null ? null : transitionSchema.parse(value);
  const transition = next === null ? current : transitionSchema.parse(next);
  if (!transition) invalid('The transition directory has no manifest', 'UPDATE_RETENTION_REQUIRED');
  if (next !== null && current) validatePendingTransition(current, transition);
  validateTransitionFiles(paths, transition);
  if (next !== null) {
    if (transition.state === 'complete') validateTransitionAdmission(paths, transition);
    renameSync(pending, manifest);
  }
  return transition;
}

function validateTransitionAdmission(paths: Paths, transition: Transition) {
  const admitted = outputProjection(paths.output, dirname(paths.runtime), transition.target.commit);
  if (
    !admitted ||
    !admittedFresh(admitted) ||
    admitted.check.hash !== transition.expectedAcceptedHash ||
    admitted.input.graph.inputHash !== transition.target.inputHash
  )
    invalid('The pending transition lacks its verified admission', 'UPDATE_RETENTION_REQUIRED');
}

function validatePendingTransition(current: Transition, next: Transition) {
  const phases = ['archiving', 'active', 'complete'];
  if (
    !sameTarget(current.target, next.target) ||
    !isDeepStrictEqual(current.previous, next.previous) ||
    phases.indexOf(next.state) < phases.indexOf(current.state) ||
    (current.expectedAcceptedHash !== null &&
      next.expectedAcceptedHash !== current.expectedAcceptedHash)
  )
    invalid(
      'The pending transition changes its retained identity or completed work',
      'UPDATE_RETENTION_REQUIRED',
    );
  for (const name of ['admitted', 'candidate', 'ingestion', 'reviews', 'comparisons'] as const) {
    if (
      (current.files[name] !== null || current.state !== 'archiving') &&
      !isDeepStrictEqual(current.files[name], next.files[name])
    )
      invalid(
        'The pending transition replaces retained archive evidence',
        'UPDATE_RETENTION_REQUIRED',
      );
  }
}

function validateTransitionFiles(paths: Paths, transition: Transition) {
  for (const name of ['admitted', 'candidate', 'ingestion', 'reviews', 'comparisons'] as const) {
    const record = transition.files[name];
    if (!record) continue;
    const file = transitionFilePath(paths, name);
    const stat = regularFile(file);
    if (!stat || stat.size !== record.bytes || hash(readFileSync(file, 'utf8')) !== record.hash)
      invalid(`Altered transition archive: ${file}`, 'UPDATE_RETENTION_REQUIRED');
  }
}

function writeTransition(paths: Paths, transition: Transition) {
  atomicWrite(join(paths.transition, 'manifest.json'), jsonText(transition));
}

function archiveText(paths: Paths, name: keyof Transition['files'], text: string) {
  const path = transitionFilePath(paths, name);
  if (regularFile(path)) {
    const existing = readFileSync(path, 'utf8');
    if (existing !== text)
      invalid(`Transition archive differs: ${path}`, 'UPDATE_RETENTION_REQUIRED');
  } else atomicWrite(path, text);
  return { name, hash: hash(text), bytes: Buffer.byteLength(text) };
}

function archiveJson(
  paths: Paths,
  name: keyof Transition['files'],
  value: unknown,
  maximumBytes: number,
) {
  const text = jsonText(value);
  if (Buffer.byteLength(text) > maximumBytes)
    invalid(`Transition archive exceeds ${maximumBytes} bytes`, 'UPDATE_RETENTION_REQUIRED');
  return archiveText(paths, name, text);
}

function isHivexError(error: unknown): error is HivexError {
  return error instanceof HivexError;
}

function isContractError(error: unknown) {
  if (!isHivexError(error)) return false;
  return [
    'INVALID_INGESTION_STORE',
    'INGESTION_PLAN_MISMATCH',
    'INGESTION_ARCHIVE_MISMATCH',
    'INVALID_REVIEW_STORE',
    'INVALID_COMPARISON_STORE',
    'REVIEW_PLAN_MISMATCH',
    'REVIEW_ARCHIVE_MISMATCH',
    'REVIEW_STORE_RETIRED',
  ].includes(error.code);
}

function errorBlocker(error: unknown, phase: string): Blocker {
  return {
    id: null,
    kind: isContractError(error) ? 'contract-mismatch' : 'invalid',
    message: `${phase}: ${isHivexError(error) ? `${error.code}: ` : ''}${error instanceof Error ? error.message : 'operation failed'}`,
  };
}

function blocked(phase: string, blockers: Blocker[], extra: Record<string, unknown> = {}) {
  return {
    command: 'update',
    accepted: false,
    status: blockers.some((blocker) => blocker.kind === 'contract-mismatch')
      ? 'contract-mismatch'
      : 'blocked',
    phase,
    blockers,
    actions: [
      'Inspect the cited store, receipt or archive with the existing --show/--export commands.',
      'Resolve adverse or uncertain evidence explicitly; update never runs --retry-failed or --revise.',
    ],
    ...extra,
  } as Record<string, unknown>;
}

function retention(paths: Paths, message: string) {
  return {
    command: 'update',
    accepted: false,
    status: 'retention-required',
    phase: 'transition',
    message,
    transition: paths.transition,
    actions: [
      `Inspect ${join(paths.transition, 'manifest.json')} and preserve its complete files.`,
      `Move ${paths.transition} to a caller-owned archive after verifying the admitted output and stores.`,
      'Rerun update with the same semantic options only after the old transition directory is out of the active path.',
    ],
  } as Record<string, unknown>;
}

function pidAlive(pid: number) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return !(error instanceof Error && 'code' in error && error.code === 'ESRCH');
  }
}

function acquireLock(path: string) {
  ensureDirectory(dirname(path));
  try {
    const fd = openSync(path, 'wx', 0o600);
    writeFileSync(fd, jsonText({ pid: process.pid }));
    fsyncSync(fd);
    return {
      release: () => {
        closeSync(fd);
        unlinkSync(path);
      },
    };
  } catch (error) {
    if (!(error instanceof Error && 'code' in error && error.code === 'EEXIST')) throw error;
    let value: unknown;
    try {
      value = readJson(path, 4096);
    } catch {
      return { response: lockBlocked(path, 'lock-invalid') };
    }
    const pid = z.object({ pid: z.number().int().positive() }).safeParse(value).data?.pid;
    return {
      response: lockBlocked(
        path,
        pid !== undefined && pidAlive(pid) ? 'lock-active' : 'lock-obsolete',
        pid,
      ),
    };
  }
}

function lockBlocked(
  path: string,
  reason: 'lock-active' | 'lock-obsolete' | 'lock-invalid',
  pid?: number,
) {
  const owner = pid === undefined ? '' : ` (PID ${pid})`;
  return {
    command: 'update',
    accepted: false,
    status: 'blocked',
    phase: 'lock',
    reason,
    lock: path,
    message: `The update lock${owner} needs operator inspection`,
    actions: [
      `Inspect ${path} and confirm whether an update coordinator is still running.`,
      `Remove ${path} manually only after confirming that no coordinator owns it.`,
    ],
  } as Record<string, unknown>;
}

function targetFor(options: UpdateOptions, checkpoint: Checkpoint | null): Target {
  const collection = checkpoint?.target.collection ?? options.collection ?? null;
  const ref =
    checkpoint && checkpoint.phase !== 'admitted' && !options.refExplicit
      ? checkpoint.target.commit
      : (options.ref ?? 'HEAD');
  const snapshot = loadSnapshot({
    root: options.root,
    ref,
    selection: { collection: collection ?? undefined },
  });
  if (checkpoint && checkpoint.phase !== 'admitted' && snapshot.commit !== checkpoint.target.commit)
    invalid(
      'An active update target changed; resume its recorded commit',
      'UPDATE_OPTIONS_MISMATCH',
    );
  return { snapshot, plan: createPlan(snapshot, collection) };
}

function checkpointFor(
  options: UpdateOptions,
  paths: Paths,
  target: Target,
  state: Pick<Checkpoint, 'phase' | 'candidateHash' | 'admittedHash'>,
) {
  return checkpointSchema.parse({
    format: 'hivex-update',
    version: 1,
    root: options.root,
    output: paths.output,
    candidate: paths.candidate,
    transition: paths.transition,
    stores: {
      ingestion: paths.ingestion,
      reviews: paths.reviews,
      comparisons: paths.comparisons,
    },
    target: {
      commit: target.snapshot.commit,
      collection: target.plan.selection.collection,
      neighbors: options.neighbors,
      inputHash: inputHash(target.plan),
      ...(options.comparisonContext ? { comparisonContext: options.comparisonContext } : {}),
    },
    ...state,
  });
}

function validateCheckpoint(options: UpdateOptions, paths: Paths, checkpoint: Checkpoint) {
  if (
    checkpoint.root !== options.root ||
    checkpoint.output !== paths.output ||
    checkpoint.candidate !== paths.candidate ||
    checkpoint.transition !== paths.transition ||
    checkpoint.stores.ingestion !== paths.ingestion ||
    checkpoint.stores.reviews !== paths.reviews ||
    checkpoint.stores.comparisons !== paths.comparisons
  )
    invalid('Update paths differ from the recorded checkpoint', 'UPDATE_OPTIONS_MISMATCH');
  if (options.collectionExplicit && options.collection !== checkpoint.target.collection)
    invalid('Collection differs from the recorded checkpoint', 'UPDATE_OPTIONS_MISMATCH');
  if (options.neighborsExplicit && options.neighbors !== checkpoint.target.neighbors)
    invalid('Neighbors differ from the recorded checkpoint', 'UPDATE_OPTIONS_MISMATCH');
}

function outputProjection(path: string, root: string, against?: string) {
  if (!regularFile(path)) return null;
  const projection = readProjection(path, root, against);
  if (!('admission' in projection.check))
    invalid('Update output must be an admitted graph', 'UPDATE_OUTPUT_INVALID');
  return projection;
}

function admittedFresh(projection: ReturnType<typeof readProjection>) {
  return (
    'admission' in projection.check &&
    projection.check.status === 'admitted' &&
    projection.check.freshness.status === 'fresh'
  );
}

type OldAccepted = {
  hash: string;
  sourceCommit: string;
  graphInputHash: string;
  text: string;
  comparisonContext?: ComparisonContext;
};
type CandidateInfo = { hash: string; inputHash: string; sourceCommit: string };

function readOldAccepted(paths: Paths, root: string, targetCommit: string): OldAccepted | null {
  if (!regularFile(paths.output)) return null;
  const current = outputProjection(paths.output, root, targetCommit);
  if (!current) return null;
  const original = readProjection(paths.output, root, current.input.graph.sourceSnapshot.commit);
  if (!admittedFresh(original))
    invalid('The previous admitted graph cannot be verified', 'UPDATE_CONTRACT_MISMATCH');
  return {
    hash: current.check.hash,
    sourceCommit: current.input.graph.sourceSnapshot.commit,
    graphInputHash: current.input.graph.inputHash,
    text: readFileSync(paths.output, 'utf8'),
    comparisonContext: 'comparisonContext' in current ? current.comparisonContext : undefined,
  };
}

function candidateInfo(paths: Paths, root: string): CandidateInfo | null {
  if (!regularFile(paths.candidate)) return null;
  const projection = readProjection(paths.candidate, root);
  if (projection.input.graph.format !== 'hivex-graph-candidate')
    invalid('The update candidate must remain unaccepted', 'UPDATE_CANDIDATE_INVALID');
  return {
    hash: projection.input.graph.hash,
    inputHash: projection.input.graph.inputHash,
    sourceCommit: projection.input.graph.sourceSnapshot.commit,
  };
}

function manifestWithFile(
  transition: Transition,
  name: keyof Transition['files'],
  file: z.infer<typeof fileRecordSchema> | null,
) {
  return transitionSchema.parse({ ...transition, files: { ...transition.files, [name]: file } });
}

async function prepareTransition(
  options: UpdateOptions,
  paths: Paths,
  target: Target,
  inputs: { oldAccepted: OldAccepted | null; oldCandidate: CandidateInfo | null },
) {
  const targetIdentity = {
    commit: target.snapshot.commit,
    inputHash: inputHash(target.plan),
    ...(options.comparisonContext ? { comparisonContext: options.comparisonContext } : {}),
  };
  let transition = readTransition(paths);
  if (transition && !sameTarget(transition.target, targetIdentity))
    return {
      response: retention(paths, 'A previous transition still owns the single retention directory'),
    };
  if (!transition) {
    const started = startTransition(paths, targetIdentity, inputs);
    if ('response' in started) return started;
    transition = started.transition;
  }
  if (transition.state === 'complete')
    return {
      response: retention(paths, 'A previous transition still owns the single retention directory'),
    };
  if (transition.state === 'active') return { transition };
  transition = archivePreviousFiles(paths, transition, inputs);
  writeTransition(paths, transition);
  return archiveStores(options, paths, transition, inputs.oldCandidate);
}

function sameTarget(left: Transition['target'], right: Transition['target']) {
  return (
    left.commit === right.commit &&
    left.inputHash === right.inputHash &&
    isDeepStrictEqual(left.comparisonContext, right.comparisonContext)
  );
}

function startTransition(
  paths: Paths,
  target: Transition['target'],
  inputs: { oldAccepted: OldAccepted | null; oldCandidate: CandidateInfo | null },
) {
  if (existsSync(paths.transition))
    return {
      response: retention(paths, 'The transition directory exists without a valid manifest'),
    };
  mkdirSync(paths.transition, { recursive: false, mode: 0o700 });
  const transition = transitionSchema.parse({
    format: 'hivex-update-transition',
    version: 1,
    state: 'archiving',
    target,
    previous: {
      admittedHash: inputs.oldAccepted?.hash ?? null,
      candidateHash: inputs.oldCandidate?.hash ?? null,
    },
    expectedAcceptedHash: null,
    files: { admitted: null, candidate: null, ingestion: null, reviews: null, comparisons: null },
  });
  writeTransition(paths, transition);
  return { transition };
}

function archivePreviousFiles(
  paths: Paths,
  transition: Transition,
  inputs: { oldAccepted: OldAccepted | null; oldCandidate: CandidateInfo | null },
) {
  let result = transition;
  if (inputs.oldAccepted && !result.files.admitted)
    result = manifestWithFile(
      result,
      'admitted',
      archiveText(paths, 'admitted', inputs.oldAccepted.text),
    );
  if (inputs.oldCandidate && !result.files.candidate)
    result = manifestWithFile(
      result,
      'candidate',
      archiveText(paths, 'candidate', readFileSync(paths.candidate, 'utf8')),
    );
  return result;
}

type StoreArchive = 'ingestion' | 'reviews' | 'comparisons';

async function exportStore(
  options: UpdateOptions,
  paths: Paths,
  name: StoreArchive,
  sourceCommit: string,
) {
  const store = paths[name];
  if (name === 'ingestion') return IngestionStore.export(store, maximumExportBytes);
  const input = transitionFilePath(paths, 'candidate');
  const common = [
    '--export',
    '--input',
    input,
    '--root',
    options.root,
    '--against',
    sourceCommit,
    '--store',
    store,
    '--max-bytes',
    String(maximumExportBytes),
  ];
  if (name === 'reviews') return reviewCohortCommand(common);
  return comparisonCohortCommand(
    [...common, '--neighbors', String(options.neighbors)],
    options.comparisonContext,
  );
}

async function archiveStores(
  options: UpdateOptions,
  paths: Paths,
  transition: Transition,
  oldCandidate: CandidateInfo | null,
) {
  let result = transition;
  for (const name of ['ingestion', 'reviews', 'comparisons'] as const) {
    if (!regularFile(paths[name]) || result.files[name]) continue;
    if ((name === 'reviews' || name === 'comparisons') && !oldCandidate)
      return {
        response: retention(paths, `Cannot archive ${name} without the old candidate graph`),
      };
    try {
      const exported = await exportStore(
        options,
        paths,
        name,
        oldCandidate?.sourceCommit ?? 'HEAD',
      );
      result = manifestWithFile(
        result,
        name,
        archiveJson(paths, name, exported, maximumExportBytes),
      );
      writeTransition(paths, result);
    } catch (error) {
      return { response: blocked('transition', [errorBlocker(error, name)]) };
    }
  }
  result = transitionSchema.parse({ ...result, state: 'active' });
  writeTransition(paths, result);
  return { transition: result };
}

function archivePath(paths: Paths, transition: Transition, name: keyof Transition['files']) {
  if (!transition.files[name])
    invalid(`Transition has no ${name} archive`, 'UPDATE_RETENTION_REQUIRED');
  return transitionFilePath(paths, name);
}

function selectionMatches(selection: ReturnType<typeof IngestionStore.selection>, target: Target) {
  return (
    selection?.ref === target.snapshot.commit &&
    selection.collection === target.plan.selection.collection
  );
}

function ingestionBlockers(path: string): Blocker[] {
  if (!regularFile(path)) return [];
  try {
    const exported = IngestionStore.export(path, maximumExportBytes);
    return exported.units.flatMap((unit): Blocker[] => {
      const reports = unit.result?.attempts ?? unit.checkpoint.reports;
      if (unit.state === 'running' || unit.checkpoint.active || reports.some(unresolvedExtraction))
        return [
          {
            id: unit.id,
            kind: 'uncertain' as const,
            message: 'The extraction invocation is unresolved',
          },
        ];
      if (unit.state === 'failed')
        return [
          {
            id: unit.id,
            kind: 'failed' as const,
            message: 'The extraction failed and was retained',
          },
        ];
      return [];
    });
  } catch (error) {
    return [errorBlocker(error, 'ingestion')];
  }
}

function assessmentBlockers(
  rows: {
    id: string;
    state: string;
    result: {
      report: { outcome: string; [key: string]: unknown };
      review?: unknown;
      comparison?: unknown;
    } | null;
  }[],
): Blocker[] {
  return rows.flatMap(assessmentBlocker);
}

function assessmentBlocker(row: Parameters<typeof assessmentBlockers>[0][number]): Blocker[] {
  if (row.state === 'running' || (row.result && unresolvedAssessment(row.result.report)))
    return [{ id: row.id, kind: 'uncertain', message: 'The assessment invocation is unresolved' }];
  if (row.state !== 'failed') return [];
  const adverse =
    (row.result?.review !== null && row.result?.review !== undefined) ||
    (row.result?.comparison !== null && row.result?.comparison !== undefined);
  return [
    {
      id: row.id,
      kind: adverse ? 'adverse' : 'failed',
      message: adverse
        ? 'The retained assessment is adverse'
        : 'The assessment failed and was retained',
    },
  ];
}

async function runIngestion(
  options: UpdateOptions,
  paths: Paths,
  target: Target,
  transition: Transition | null,
): Promise<PhaseResult> {
  try {
    const selection = IngestionStore.selection(paths.ingestion);
    if (selection && !selectionMatches(selection, target)) {
      if (!transition)
        return { ready: false, response: retention(paths, 'Ingestion needs a transition archive') };
      const archive = archivePath(paths, transition, 'ingestion');
      await ingestCommand([
        '--reuse',
        archive,
        '--ref',
        target.snapshot.commit,
        '--max-units',
        '0',
        '--root',
        options.root,
        '--store',
        paths.ingestion,
      ]);
    }
    const retainedBlockers = ingestionBlockers(paths.ingestion);
    if (retainedBlockers.length)
      return { ready: false, response: blocked('ingest', retainedBlockers) };
    if (
      regularFile(paths.ingestion)?.size &&
      IngestionStore.export(paths.ingestion, maximumExportBytes).pending === 0
    )
      return { ready: true };
    const args = [
      '--ref',
      target.snapshot.commit,
      '--max-units',
      String(options.maxUnits),
      '--root',
      options.root,
      '--store',
      paths.ingestion,
      '--codex',
      options.binary,
      '--deadline-ms',
      String(options.deadlineMilliseconds),
    ];
    if (target.plan.selection.collection)
      args.push('--collection', target.plan.selection.collection);
    await ingestCommand(args);
    const blockers = ingestionBlockers(paths.ingestion);
    if (blockers.length) return { ready: false, response: blocked('ingest', blockers) };
    const progress = IngestionStore.export(paths.ingestion, maximumExportBytes) as {
      pending: number;
      unresolved: number;
      failed: number;
    };
    if (progress.pending || progress.unresolved || progress.failed)
      return {
        ready: false,
        response: {
          command: 'update',
          accepted: false,
          status: 'partial',
          phase: 'ingest',
          pending: progress.pending,
        },
      };
    return { ready: true };
  } catch (error) {
    return { ready: false, response: blocked('ingest', [errorBlocker(error, 'ingestion')]) };
  }
}

type AssessmentRows = Parameters<typeof assessmentBlockers>[0];
type AssessmentPhase = {
  name: 'review' | 'compare';
  prepare: () => Promise<void>;
  inspect: () => Promise<{ status: string; rows: AssessmentRows }>;
  execute: () => Promise<void>;
};

async function runAssessmentPhase(
  options: UpdateOptions,
  phase: AssessmentPhase,
): Promise<PhaseResult> {
  try {
    await phase.prepare();
    const initial = await phase.inspect();
    const blockers = assessmentBlockers(initial.rows);
    if (blockers.length) return { ready: false, response: blocked(phase.name, blockers) };
    if (options.maxUnits > 0) await phase.execute();
    const final = await phase.inspect();
    const finalBlockers = assessmentBlockers(final.rows);
    if (finalBlockers.length) return { ready: false, response: blocked(phase.name, finalBlockers) };
    if (final.status !== 'reviewed')
      return {
        ready: false,
        response: { command: 'update', accepted: false, status: 'partial', phase: phase.name },
      };
    return { ready: true };
  } catch (error) {
    return { ready: false, response: blocked(phase.name, [errorBlocker(error, phase.name)]) };
  }
}

async function runReview(
  options: UpdateOptions,
  paths: Paths,
  transition: Transition | null,
): Promise<PhaseResult> {
  const common = [
    '--input',
    paths.candidate,
    '--root',
    options.root,
    '--against',
    options.ref ?? 'HEAD',
    '--store',
    paths.reviews,
  ];
  const inspect = async () => {
    const result = (await reviewCohortCommand([
      '--export',
      ...common,
      '--max-bytes',
      String(maximumExportBytes),
    ])) as { status: string; reviews: AssessmentRows };
    return { status: result.status, rows: result.reviews };
  };
  return runAssessmentPhase(options, {
    name: 'review',
    prepare: async () => {
      if (!regularFile(paths.reviews)?.size) {
        await reviewCohortCommand(['--all', ...common, '--max-units', '0']);
        return;
      }
      try {
        await inspect();
      } catch (error) {
        if (!transition || !isContractError(error)) throw error;
        await reviewCohortCommand([
          '--all',
          ...common,
          '--from',
          archivePath(paths, transition, 'candidate'),
          '--reuse',
          archivePath(paths, transition, 'reviews'),
          '--max-units',
          '0',
        ]);
      }
    },
    inspect,
    execute: () =>
      reviewCohortCommand([
        '--all',
        ...common,
        '--codex',
        options.binary,
        '--deadline-ms',
        String(options.deadlineMilliseconds),
        '--max-units',
        String(options.maxUnits),
      ]).then(() => undefined),
  });
}

async function runComparison(
  options: UpdateOptions,
  paths: Paths,
  transition: Transition | null,
  target: Target,
): Promise<PhaseResult> {
  const graph = readProjection(paths.candidate, options.root, target.snapshot.commit).input;
  if (new Set([...graph.nodes.values()].map((node) => node.source)).size <= 1)
    return { ready: true };
  const common = [
    '--input',
    paths.candidate,
    '--root',
    options.root,
    '--against',
    target.snapshot.commit,
    '--store',
    paths.comparisons,
    '--neighbors',
    String(options.neighbors),
  ];
  const inspect = async () => {
    const result = (await comparisonCohortCommand(
      ['--export', ...common, '--max-bytes', String(maximumExportBytes)],
      options.comparisonContext,
    )) as { status: string; comparisons: AssessmentRows };
    return { status: result.status, rows: result.comparisons };
  };
  return runAssessmentPhase(options, {
    name: 'compare',
    prepare: async () => {
      if (!regularFile(paths.comparisons)?.size) {
        await comparisonCohortCommand(
          ['--all', ...common, '--max-units', '0'],
          options.comparisonContext,
        );
        return;
      }
      try {
        await inspect();
      } catch (error) {
        if (!transition || !isContractError(error)) throw error;
        await comparisonCohortCommand(
          [
            '--all',
            ...common,
            '--from',
            archivePath(paths, transition, 'candidate'),
            '--reuse',
            archivePath(paths, transition, 'comparisons'),
            '--max-units',
            '0',
          ],
          options.comparisonContext,
        );
      }
    },
    inspect,
    execute: () =>
      comparisonCohortCommand(
        [
          '--all',
          ...common,
          '--codex',
          options.binary,
          '--deadline-ms',
          String(options.deadlineMilliseconds),
          '--max-units',
          String(options.maxUnits),
        ],
        options.comparisonContext,
      ).then(() => undefined),
  });
}

type CycleContext = {
  options: UpdateOptions;
  paths: Paths;
  target: Target;
  checkpoint: Checkpoint;
  transition: Transition | null;
  old: OldAccepted | null;
};
type PrepareResult = { response: Record<string, unknown> } | { context: CycleContext };
type TransitionResult = { response: Record<string, unknown> } | { transition: Transition | null };

function failureResponse(paths: Paths, phase: string, error: unknown) {
  if (isHivexError(error) && error.code === 'UPDATE_RETENTION_REQUIRED')
    return retention(paths, error.message);
  return blocked(phase, [errorBlocker(error, phase)]);
}

function readCycleCheckpoint(options: UpdateOptions, paths: Paths) {
  let checkpoint = readCheckpoint(options, paths);
  if (!checkpoint) return null;
  validateCheckpoint(options, paths, checkpoint);
  if (!options.neighborsExplicit) options.neighbors = checkpoint.target.neighbors;
  if (!options.collectionExplicit) options.collection = checkpoint.target.collection ?? undefined;
  if (options.comparisonContextFile === undefined)
    options.comparisonContext = checkpoint.target.comparisonContext;
  if (!isDeepStrictEqual(options.comparisonContext, checkpoint.target.comparisonContext)) {
    const next = checkpointSchema.parse({
      ...checkpoint,
      phase: 'compare',
      target: { ...checkpoint.target, comparisonContext: options.comparisonContext },
    });
    validatePreparedComparisons(paths, next, checkpoint);
    writeCheckpoint(paths.checkpoint, next);
    checkpoint = next;
  }
  return reconcileAdmission(options, paths, resumeIncomingTransition(options, paths, checkpoint));
}

function validatePreparedComparisons(paths: Paths, checkpoint: Checkpoint, previous: Checkpoint) {
  const context = createReviewContext({
    input: paths.candidate,
    root: checkpoint.root,
    against: checkpoint.target.commit,
  });
  const prepared = prepareComparisonCohort(
    context,
    checkpoint.target.neighbors,
    checkpoint.target.comparisonContext,
  );
  const prior = prepareComparisonCohort(
    context,
    previous.target.neighbors,
    previous.target.comparisonContext,
  );
  const rows = AssessmentStore.readRefreshed(
    paths.comparisons,
    prepared.plan,
    prior.plan,
    comparisonContract,
  );
  validateComparisons(rows, prepared);
  if (rows.some((row) => row.state === 'running'))
    invalid('A comparison invocation is unresolved; preserve its context', 'REVIEW_UNRESOLVED');
}

function resumeIncomingTransition(options: UpdateOptions, paths: Paths, checkpoint: Checkpoint) {
  if (checkpoint.phase !== 'admitted') return checkpoint;
  const transition = readTransition(paths);
  if (
    !transition ||
    transition.state === 'complete' ||
    sameTarget(transition.target, checkpoint.target)
  )
    return checkpoint;
  const admitted = outputProjection(paths.output, options.root, checkpoint.target.commit);
  if (
    transition.previous.admittedHash !== checkpoint.admittedHash ||
    transition.previous.candidateHash !== checkpoint.candidateHash ||
    !admitted ||
    admitted.check.hash !== checkpoint.admittedHash ||
    !admissionMatchesTarget(admitted, checkpoint.target)
  )
    invalid(
      'The incoming transition does not belong to the retained admission',
      'UPDATE_RETENTION_REQUIRED',
    );
  const incoming: Checkpoint = {
    ...checkpoint,
    phase: 'ingest',
    target: { ...checkpoint.target, ...transition.target },
  };
  const target = targetFor(options, incoming);
  if (inputHash(target.plan) !== incoming.target.inputHash)
    invalid(
      'The incoming transition differs from its frozen processing inputs',
      'INGESTION_PLAN_MISMATCH',
    );
  writeCheckpoint(paths.checkpoint, incoming);
  return incoming;
}

function retainedCandidateMatches(paths: Paths, root: string, graphHash: string) {
  const candidate = candidateInfo(paths, root);
  if (candidate && candidate.hash !== graphHash) return false;
  if (!regularFile(paths.ingestion)) return true;
  const cohort = IngestionStore.read(paths.ingestion);
  if (cohort.rows.some((row) => row.state !== 'candidate')) return false;
  return buildGraph(root, paths.ingestion).hash === graphHash;
}

function admissionMatchesTarget(
  projection: ReturnType<typeof readProjection>,
  target: Checkpoint['target'],
) {
  return (
    admittedFresh(projection) &&
    projection.input.graph.inputHash === target.inputHash &&
    projection.input.graph.selection.collection === target.collection &&
    isDeepStrictEqual(
      'comparisonContext' in projection ? projection.comparisonContext : undefined,
      target.comparisonContext,
    ) &&
    'admission' in projection.check &&
    projection.check.admission.coverage.neighbors === target.neighbors
  );
}

function reconcileAdmission(options: UpdateOptions, paths: Paths, checkpoint: Checkpoint) {
  const admitted = outputProjection(paths.output, options.root, checkpoint.target.commit);
  if (
    !admitted ||
    !admissionMatchesTarget(admitted, checkpoint.target) ||
    !retainedCandidateMatches(paths, options.root, admitted.input.graph.hash)
  )
    return checkpoint;
  const transition = readTransition(paths);
  if (transition && transition.state !== 'complete') {
    if (
      !sameTarget(transition.target, checkpoint.target) ||
      transition.expectedAcceptedHash !== admitted.check.hash
    )
      invalid(
        'The published admission does not match the active transition',
        'UPDATE_RETENTION_REQUIRED',
      );
    writeTransition(paths, { ...transition, state: 'complete' });
  }
  const reconciled: Checkpoint = {
    ...checkpoint,
    phase: 'admitted',
    candidateHash: admitted.input.graph.hash,
    admittedHash: admitted.check.hash,
  };
  if (!isDeepStrictEqual(checkpoint, reconciled)) writeCheckpoint(paths.checkpoint, reconciled);
  return reconciled;
}

function continueCompletedCheckpoint(
  paths: Paths,
  checkpoint: Checkpoint | null,
  target: Target,
): { response: Record<string, unknown> } | { checkpoint: Checkpoint | null } {
  if (
    !checkpoint ||
    checkpoint.phase !== 'admitted' ||
    checkpoint.target.commit === target.snapshot.commit
  )
    return { checkpoint };
  if (readTransition(paths))
    return { response: retention(paths, 'A completed transition bundle is still retained') };
  return { checkpoint: null };
}

function unchangedResponse(
  options: UpdateOptions,
  paths: Paths,
  checkpoint: Checkpoint | null,
  target: Target,
) {
  if (!regularFile(paths.output)) return null;
  const current = outputProjection(paths.output, options.root, target.snapshot.commit);
  if (
    current &&
    admissionMatchesTarget(current, {
      commit: target.snapshot.commit,
      inputHash: inputHash(target.plan),
      collection: target.plan.selection.collection,
      neighbors: options.neighbors,
      comparisonContext: options.comparisonContext,
    }) &&
    retainedCandidateMatches(paths, options.root, current.input.graph.hash) &&
    admittedFresh(readProjection(paths.output, options.root)) &&
    (!checkpoint || checkpoint.target.commit === target.snapshot.commit)
  )
    return {
      command: 'update',
      accepted: true,
      status: 'unchanged',
      phase: 'admitted',
      calls: 0,
      hash: current.check.hash,
    } as Record<string, unknown>;
  return null;
}

async function transitionForCycle(
  options: UpdateOptions,
  paths: Paths,
  target: Target,
  state: { old: OldAccepted | null; candidate: CandidateInfo | null },
): Promise<TransitionResult> {
  const selection = IngestionStore.selection(paths.ingestion);
  const changed =
    Boolean(
      state.old &&
      (state.old.graphInputHash !== inputHash(target.plan) ||
        !isDeepStrictEqual(state.old.comparisonContext, options.comparisonContext)),
    ) || Boolean(selection && !selectionMatches(selection, target));
  let transition = readTransition(paths);
  if (!changed && !transition) return { transition: null };
  if (
    transition?.state === 'active' &&
    sameTarget(transition.target, {
      commit: target.snapshot.commit,
      inputHash: inputHash(target.plan),
      comparisonContext: options.comparisonContext,
    })
  )
    return { transition };
  const prepared = await prepareTransition(options, paths, target, {
    oldAccepted: state.old,
    oldCandidate: state.candidate,
  });
  if ('response' in prepared) return prepared;
  transition = prepared.transition ?? transition;
  return { transition };
}

async function prepareCycle(options: UpdateOptions, paths: Paths): Promise<PrepareResult> {
  let checkpoint = readCycleCheckpoint(options, paths);
  const target = targetFor(options, checkpoint);
  options.ref = target.snapshot.commit;
  validateIngestionTarget(paths, target);
  if (options.comparisonContext) validateContextCandidate(options, paths, target);
  const continued = continueCompletedCheckpoint(paths, checkpoint, target);
  if ('response' in continued) return continued;
  checkpoint = continued.checkpoint;
  const unchanged = unchangedResponse(options, paths, checkpoint, target);
  if (unchanged) return { response: unchanged };
  const currentOutput = regularFile(paths.output)
    ? outputProjection(paths.output, options.root, target.snapshot.commit)
    : null;
  const old = currentOutput
    ? readOldAccepted(paths, options.root, currentOutput.input.graph.sourceSnapshot.commit)
    : null;
  const candidate = candidateInfo(paths, options.root);
  const prepared = await transitionForCycle(options, paths, target, { old, candidate });
  if ('response' in prepared) return prepared;
  const transition = prepared.transition;
  if (!checkpoint || checkpoint.target.commit !== target.snapshot.commit) {
    checkpoint = checkpointFor(options, paths, target, {
      phase: 'ingest',
      candidateHash: candidate?.hash ?? null,
      admittedHash: old?.hash ?? null,
    });
    writeCheckpoint(paths.checkpoint, checkpoint);
  }
  return { context: { options, paths, target, checkpoint, transition, old } };
}

function validateIngestionTarget(paths: Paths, target: Target) {
  if (!regularFile(paths.ingestion)?.size) return;
  const { plan } = IngestionStore.read(paths.ingestion);
  if (!isDeepStrictEqual(plan.selection, target.plan.selection))
    invalid(
      'The retained ingestion collection differs from the requested update scope',
      'UPDATE_OPTIONS_MISMATCH',
    );
  if (!isDeepStrictEqual(plan.processing, target.plan.processing))
    invalid(
      'The retained ingestion processing contract is incompatible; preserve the store',
      'INGESTION_PLAN_MISMATCH',
    );
}

function validateContextCandidate(options: UpdateOptions, paths: Paths, target: Target) {
  if (!regularFile(paths.candidate) || !regularFile(paths.ingestion))
    invalid(
      'Comparison context requires an existing verified managed candidate and its ingestion store',
      'COMPARISON_CONTEXT_REQUIRES_GRAPH',
    );
  const context = createReviewContext({
    input: paths.candidate,
    root: options.root,
    against: target.snapshot.commit,
  });
  if (buildGraph(options.root, paths.ingestion).hash !== context.input.graph.hash)
    invalid(
      'Comparison context requires the current retained candidate',
      'COMPARISON_CONTEXT_REQUIRES_GRAPH',
    );
  prepareComparisonCohort(context, options.neighbors, options.comparisonContext);
}

function preflightContextFile(options: UpdateOptions, paths: Paths) {
  if (options.comparisonContextFile === undefined) return;
  const value =
    readJson(paths.checkpoint, 8 * 1024 * 1024) ??
    readJson(`${paths.checkpoint}.pending`, 8 * 1024 * 1024);
  const checkpoint = value === null ? null : checkpointSchema.parse(value);
  const scoped = {
    ...options,
    neighbors: options.neighborsExplicit
      ? options.neighbors
      : (checkpoint?.target.neighbors ?? options.neighbors),
  };
  if (checkpoint) validateCheckpoint(scoped, paths, checkpoint);
  validateContextCandidate(scoped, paths, targetFor(scoped, checkpoint));
}

type BuildResult =
  | { response: Record<string, unknown> }
  | { candidate: CandidateInfo | null; transition: Transition | null };

async function buildCandidate(context: CycleContext): Promise<BuildResult> {
  const { options, paths, checkpoint } = context;
  writeCheckpoint(paths.checkpoint, { ...checkpoint, phase: 'build' });
  const built = graphCommand([
    'build',
    '--root',
    options.root,
    '--store',
    paths.ingestion,
    '--export',
  ]) as { hash: string; inputHash: string };
  if (Buffer.byteLength(jsonText(built)) > maximumCandidateBytes)
    invalid('The candidate graph exceeds its size limit', 'UPDATE_CANDIDATE_INVALID');
  const currentCandidate = candidateInfo(paths, options.root);
  let transition = context.transition;
  if (currentCandidate && currentCandidate.hash !== built.hash) {
    if (transition && transition.previous.candidateHash !== currentCandidate.hash)
      return {
        response: retention(
          paths,
          'The occupied bundle does not preserve the intermediate candidate and its assessments',
        ),
      };
    const prepared = await prepareTransition(options, paths, context.target, {
      oldAccepted: context.old,
      oldCandidate: currentCandidate,
    });
    if ('response' in prepared && prepared.response) return { response: prepared.response };
    transition = prepared.transition ?? transition;
  }
  if (!currentCandidate || currentCandidate.hash !== built.hash)
    atomicWrite(paths.candidate, jsonText(built));
  return { candidate: candidateInfo(paths, options.root), transition };
}

function publishAdmission(
  context: CycleContext,
  candidateHash: string | null,
  snapshot: { hash: string },
) {
  const { options, paths, checkpoint, transition, old, target } = context;
  const text = jsonText(snapshot);
  if (Buffer.byteLength(text) > maximumOutputBytes)
    invalid('The admitted snapshot exceeds its size limit', 'GRAPH_ADMISSION_BUDGET');
  const pending = pendingWrite(paths.output, text, 0o644);
  const validated = readProjection(pending, options.root, target.snapshot.commit);
  if (!admittedFresh(validated))
    invalid('The new admitted snapshot is not fresh and valid', 'GRAPH_ADMISSION_INVALID');
  if (transition) {
    const latest = readTransition(paths);
    if (!latest) invalid('The transition manifest disappeared', 'UPDATE_RETENTION_REQUIRED');
    writeTransition(
      paths,
      transitionSchema.parse({ ...latest, expectedAcceptedHash: snapshot.hash }),
    );
  }
  if (
    regularFile(paths.output) &&
    old &&
    outputProjection(paths.output, options.root, old.sourceCommit)?.check.hash !== old.hash
  )
    return retention(paths, 'The previous admitted output changed during update');
  renameSync(pending, paths.output);
  if (transition) {
    const latest = readTransition(paths);
    if (latest) writeTransition(paths, transitionSchema.parse({ ...latest, state: 'complete' }));
  }
  writeCheckpoint(paths.checkpoint, {
    ...checkpoint,
    phase: 'admitted',
    candidateHash,
    admittedHash: snapshot.hash,
  });
  return {
    command: 'update',
    accepted: true,
    status: 'admitted',
    phase: 'admit',
    hash: snapshot.hash,
    output: paths.output,
  };
}

async function runCycle(context: CycleContext) {
  const { options, paths, target, checkpoint } = context;
  const ingestion = await runIngestion(options, paths, target, context.transition);
  if (!ingestion.ready) return ingestion.response;
  const built = await buildCandidate(context);
  if ('response' in built) return built.response;
  const candidate = built.candidate;
  const transition = built.transition;
  const active = { ...context, transition };
  writeCheckpoint(paths.checkpoint, {
    ...checkpoint,
    phase: 'review',
    candidateHash: candidate?.hash ?? null,
  });
  const review = await runReview(options, paths, transition);
  if (!review.ready) return review.response;
  writeCheckpoint(paths.checkpoint, {
    ...checkpoint,
    phase: 'compare',
    candidateHash: candidate?.hash ?? null,
  });
  const comparison = await runComparison(options, paths, transition, target);
  if (!comparison.ready) return comparison.response;
  writeCheckpoint(paths.checkpoint, {
    ...checkpoint,
    phase: 'admit',
    candidateHash: candidate?.hash ?? null,
  });
  if (readProjection(paths.candidate, options.root).check.freshness.status !== 'fresh')
    return blocked(
      'admit',
      [
        {
          id: null,
          kind: 'invalid',
          message: 'Admission requires current HEAD documentary inputs (ADR 8)',
        },
      ],
      {
        target: target.snapshot.commit,
        actions: [
          `Inspect the retained graph: bun hivex graph check --root ${options.root} --input ${paths.candidate} --against ${target.snapshot.commit}`,
          'Restore the frozen documentary inputs at HEAD, or preserve this cycle before explicitly transitioning to current inputs.',
        ],
      },
    );
  const snapshot = admitCommand(
    [
      '--input',
      paths.candidate,
      '--root',
      options.root,
      '--reviews',
      paths.reviews,
      '--comparisons',
      paths.comparisons,
      '--neighbors',
      String(options.neighbors),
      '--export',
    ],
    options.comparisonContext,
  ) as { hash: string };
  return publishAdmission(active, candidate?.hash ?? null, snapshot);
}

export async function updateCommand(args: string[]) {
  const options = parseUpdateArguments(args);
  const paths = pathsFor(options);
  validateOutputPath(paths);
  validateContextPath(paths, options.comparisonContextFile);
  options.comparisonContext = readComparisonContext(options.comparisonContextFile);
  preflightContextFile(options, paths);
  ensureDirectory(paths.runtime);
  const lock = acquireLock(paths.lock);
  if ('response' in lock) return lock.response;
  try {
    try {
      const prepared = await prepareCycle(options, paths);
      if ('response' in prepared) return prepared.response;
      return await runCycle(prepared.context);
    } catch (error) {
      return failureResponse(paths, 'update', error);
    }
  } finally {
    lock.release();
  }
}
