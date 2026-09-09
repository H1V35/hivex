import { z } from 'zod';
import { HivexError } from '../errors.ts';
import { hash, type Source } from '../sources/markdown.ts';
import { invokeModel } from '../model/invoke.ts';
import { candidateSchema, extractionSchema } from './claims.ts';
import { validateCandidateEvidence } from './evidence.ts';
import type { Revision } from './history.ts';

const maximumRejectedOutputBytes = 32 * 1024;
export const rejectedExtractionOutputSchema = z
  .strictObject({
    text: z.string().max(maximumRejectedOutputBytes).nullable(),
    hash: z.string().regex(/^[a-f0-9]{64}$/),
    bytes: z.number().int().nonnegative(),
    omittedReason: z.literal('retention-limit').nullable(),
  })
  .refine((output) => {
    if (output.text === null)
      return (
        output.bytes > maximumRejectedOutputBytes && output.omittedReason === 'retention-limit'
      );
    return (
      output.omittedReason === null &&
      output.bytes <= maximumRejectedOutputBytes &&
      Buffer.byteLength(output.text) === output.bytes &&
      hash(output.text) === output.hash
    );
  }, 'Rejected extraction output is altered or exceeds its retention budget');

function retainRejectedOutput(value: unknown) {
  if (typeof value !== 'string') return undefined;
  const bytes = Buffer.byteLength(value);
  const fits = bytes <= maximumRejectedOutputBytes;
  return {
    text: fits ? value : null,
    hash: hash(value),
    bytes,
    omittedReason: fits ? null : ('retention-limit' as const),
  };
}

type ModelReport = Awaited<ReturnType<typeof invokeModel>>['report'];
export type ExtractionAttempt = {
  outcome: string;
  code?: string;
  issues?: unknown;
  usage: ModelReport['usage'];
  deadlineMilliseconds: number;
  promptHash: string;
  rejectedOutput?: z.infer<typeof rejectedExtractionOutputSchema>;
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
      rejectedOutput: retainRejectedOutput(response.value),
      code: error instanceof HivexError ? error.code : 'INVALID_MODEL_OUTPUT',
      issues:
        error instanceof HivexError
          ? error.details?.issues
          : 'Output does not match the extraction schema',
    };
    return { candidate: null, report, retry: report.code !== 'UNCHANGED_REVISION' };
  }
}
