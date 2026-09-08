import { HivexError } from '../errors.ts';
import { hash, type Source } from '../sources/markdown.ts';
import { invokeModel } from '../model/invoke.ts';
import { candidateSchema, extractionSchema } from './claims.ts';
import { validateCandidateEvidence } from './evidence.ts';
import type { Revision } from './history.ts';

type ModelReport = Awaited<ReturnType<typeof invokeModel>>['report'];
export type ExtractionAttempt = {
  outcome: string;
  code?: string;
  issues?: unknown;
  usage: ModelReport['usage'];
  deadlineMilliseconds: number;
  promptHash: string;
};

export async function extractAttempt(options: {
  binary: string;
  prompt: string;
  deadlineMilliseconds: number;
  source: Source;
  previousCandidate?: Revision['candidate'];
}) {
  const base = {
    promptHash: hash(options.prompt),
    deadlineMilliseconds: options.deadlineMilliseconds,
  };
  const response = await invokeModel({ ...options, schema: extractionSchema });
  if (response.report.outcome !== 'completed')
    return { candidate: null, report: { ...response.report, ...base }, retry: response.retry };
  try {
    const value: unknown = typeof response.value === 'string' ? JSON.parse(response.value) : null;
    const parsed = candidateSchema.safeParse(value);
    if (!parsed.success)
      throw new HivexError({
        code: 'INVALID_MODEL_OUTPUT',
        message: 'Output does not match the extraction schema',
        details: {
          issues: parsed.error.issues.map((issue) => ({
            path: issue.path.join('.'),
            rule: issue.message,
          })),
        },
      });
    const candidate = parsed.data;
    validateCandidateEvidence({ candidate, source: options.source });
    if (
      options.previousCandidate &&
      JSON.stringify(candidate) === JSON.stringify(options.previousCandidate)
    )
      throw new HivexError({
        code: 'UNCHANGED_REVISION',
        message: 'The replacement did not change the candidate with adverse fidelity findings',
      });
    return { candidate, report: { ...response.report, ...base }, retry: false };
  } catch (error) {
    const report: ExtractionAttempt = {
      ...response.report,
      ...base,
      outcome: 'invalid-output',
      code: error instanceof HivexError ? error.code : 'INVALID_MODEL_OUTPUT',
      issues:
        error instanceof HivexError
          ? error.details?.issues
          : 'Output does not match the extraction schema',
    };
    return { candidate: null, report, retry: report.code !== 'UNCHANGED_REVISION' };
  }
}
