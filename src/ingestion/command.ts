import { HivexError } from '../errors.ts';
import { loadSnapshot } from '../workspace/snapshot.ts';
import { extractionArguments } from './arguments.ts';
import { extractAttempt, type ExtractionAttempt } from './attempt.ts';
import { maximumSourceBytes, prepareExtraction, processingContract } from './preparation.ts';

export async function extractCommand(args: string[]) {
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
  };
  const attempts: ExtractionAttempt[] = [];
  for (let index = 0; index < options.attempts; index++) {
    const previous = attempts.at(-1);
    const feedback = previous ? correctionFeedback(previous) : '';
    const result = await extractAttempt({ ...options, prompt: prompt + feedback, source });
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
