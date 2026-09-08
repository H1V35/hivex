import { HivexError } from '../errors.ts';
import { hash, type Source } from '../sources/markdown.ts';
import { loadSnapshot, type Snapshot } from '../workspace/snapshot.ts';
import { extractionArguments } from './arguments.ts';
import { extractAttempt, type ExtractionAttempt } from './attempt.ts';
import { maximumSourceBytes, prepareExtraction, processingContract } from './preparation.ts';
import type { Revision } from './history.ts';

export type ExtractionCheckpoint =
  | { state: 'started'; attempt: number; promptHash: string; deadlineMilliseconds: number }
  | { state: 'recorded'; attempt: number; report: ExtractionAttempt };

export async function extractCommand(
  args: string[],
  checkpoint?: (event: ExtractionCheckpoint) => void,
) {
  const options = extractionArguments(args);
  const { id } = options;
  const snapshot = loadSnapshot({
    root: options.root,
    ref: options.ref,
    selection: { sourceId: id },
  });
  const source = snapshot.sources.find((item) => item.id === id);
  if (!source)
    throw new HivexError({
      code: 'SOURCE_NOT_FOUND',
      message: 'Extraction requires a declared source',
    });
  return extractSource({ ...options, source, snapshot }, checkpoint);
}

export async function extractSource(
  options: {
    source: Source;
    snapshot: Pick<Snapshot, 'commit' | 'configHash'>;
    binary: string;
    attempts: number;
    deadlineMilliseconds: number;
    previousAttempts?: ExtractionAttempt[];
    revisions?: Revision[];
  },
  checkpoint?: (event: ExtractionCheckpoint) => void,
) {
  const { source, snapshot } = options;
  const sourceBytes = Buffer.byteLength(source.content);
  if (sourceBytes > maximumSourceBytes)
    throw new HivexError({
      code: 'EXTRACTION_SOURCE_TOO_LARGE',
      message: 'Extract a declared section containing at most 32768 UTF-8 bytes',
      details: { sourceBytes, maximumBytes: maximumSourceBytes },
    });
  const { prompt, basePromptHash } = prepareExtraction(source);
  const processing = processingContract();
  const envelope = {
    command: 'extract',
    accepted: false,
    snapshot: { commit: snapshot.commit, configHash: snapshot.configHash },
    source: {
      id: source.id,
      path: source.path,
      contentHash: source.contentHash,
      section: source.section,
    },
    model: processing.model,
    contract: {
      nativeVersion: processing.nativeVersion,
      requestedPolicyHash: processing.requestedPolicyHash,
      basePromptHash,
      schemaHash: processing.schemaHash,
    },
    ...(options.revisions?.length ? { revisions: options.revisions } : {}),
  };
  const attempts: ExtractionAttempt[] = [...(options.previousAttempts ?? [])];
  const revision = options.revisions?.at(-1);
  const offset = revision?.afterAttempt ?? 0;
  for (let index = attempts.length; index < offset + options.attempts; index++) {
    const previous = attempts.at(-1);
    const feedback =
      index > offset && previous?.outcome === 'invalid-output' ? correctionFeedback(previous) : '';
    const requestedPrompt = (revision?.prompt ?? prompt) + feedback;
    checkpoint?.({
      state: 'started',
      attempt: index + 1,
      promptHash: hash(requestedPrompt),
      deadlineMilliseconds: options.deadlineMilliseconds,
    });
    const result = await extractAttempt({
      binary: options.binary,
      deadlineMilliseconds: options.deadlineMilliseconds,
      prompt: requestedPrompt,
      source,
      previousCandidate: revision?.candidate,
    });
    checkpoint?.({ state: 'recorded', attempt: index + 1, report: result.report });
    attempts.push(result.report);
    if (result.candidate)
      return {
        ...envelope,
        status: 'candidate',
        attempts,
        candidateAttempt: attempts.length,
        candidate: result.candidate,
      };
    if (!result.retry) break;
  }
  return { ...envelope, status: 'failed', attempts, candidateAttempt: null, candidate: null };
}

function correctionFeedback(attempt: ExtractionAttempt) {
  const issues = Array.isArray(attempt.issues) ? attempt.issues : [];
  return `\nCorrect the previous invalid extraction without inventing evidence. ${JSON.stringify({
    code: attempt.code,
    issues: issues.slice(0, 16),
    omittedIssues: Math.max(0, issues.length - 16),
  })}`;
}
