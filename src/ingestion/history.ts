import { z } from 'zod';
import { HivexError } from '../errors.ts';
import { hash } from '../sources/markdown.ts';
import { candidateSchema } from './claims.ts';

const digest = z.string().regex(/^[a-f0-9]{64}$/);
export const revisionSchema = z.strictObject({
  afterAttempt: z.number().int().min(1).max(9),
  candidate: candidateSchema,
  previousResultHash: digest,
  feedback: z.record(z.string(), z.unknown()),
  feedbackHash: digest,
  prompt: z.string().max(262144),
  promptHash: digest,
});
export const revisionsSchema = z.array(revisionSchema).max(3);
export type Revision = z.infer<typeof revisionSchema>;

export function validateHistory(revisions: Revision[], reports: number) {
  let previous = 0;
  for (const revision of revisions) {
    if (
      revision.afterAttempt <= previous ||
      revision.afterAttempt > previous + 3 ||
      revision.afterAttempt > reports ||
      hash(JSON.stringify(revision.feedback)) !== revision.feedbackHash ||
      Buffer.byteLength(revision.prompt) > 262144 ||
      hash(revision.prompt) !== revision.promptHash
    )
      throw new HivexError({
        code: 'INVALID_INGESTION_STORE',
        message: 'Candidate revision history is inconsistent or altered',
      });
    previous = revision.afterAttempt;
  }
  if (reports > previous + 3)
    throw new HivexError({
      code: 'INVALID_INGESTION_STORE',
      message: 'The extraction round exceeds three attempts',
    });
}
