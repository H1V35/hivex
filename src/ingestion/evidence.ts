import type { z } from 'zod';
import type { Source } from '../sources/markdown.ts';
import { HivexError } from '../errors.ts';
import type { candidateSchema } from './claims.ts';
import { invalidCitationIndexes } from '../sources/citation.ts';

type Candidate = z.infer<typeof candidateSchema>;

export function validateCandidateEvidence(options: { candidate: Candidate; source: Source }) {
  const { candidate, source } = options;
  const ids = new Set<string>();
  const issues: { path: string; rule: string }[] = [];
  const check = (entries: Candidate['claims'][number]['evidence'], path: string) => {
    for (const index of invalidCitationIndexes(source, entries)) {
      issues.push({
        path: `${path}.evidence[${index}]`,
        rule: 'Quote must occur within the stated original source lines',
      });
    }
  };
  for (const [index, claim] of candidate.claims.entries()) {
    if (ids.has(claim.id))
      issues.push({ path: `claims[${index}].id`, rule: 'Claim IDs must be unique' });
    ids.add(claim.id);
    check(claim.evidence, `claims[${index}]`);
  }
  for (const [index, relation] of candidate.relations.entries()) {
    if (!ids.has(relation.from) || !ids.has(relation.to) || relation.from === relation.to)
      issues.push({
        path: `relations[${index}]`,
        rule: 'A relation must connect two distinct claims in this candidate',
      });
    check(relation.evidence, `relations[${index}]`);
  }
  if (issues.length)
    throw new HivexError({
      code: 'INVALID_MODEL_EVIDENCE',
      message: 'Candidate evidence does not match its source',
      details: { issues },
    });
}
