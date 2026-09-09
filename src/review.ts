import { readFileSync, statSync } from 'node:fs';
import { resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { z } from 'zod';
import { loadProject, type Project } from './documents.ts';
import { captureImplementation, type Implementation } from './implementation.ts';
import {
  citationSchema,
  sourceEvidence,
  suppliedCitation,
  type SuppliedDocument,
} from './knowledge-model.ts';
import { HivexError } from './errors.ts';

const codeCitation = z.object({
  path: z.string().min(1),
  side: z.enum(['before', 'after']),
  lineStart: z.number().int().positive(),
  lineEnd: z.number().int().positive(),
});
export const reviewSchema = z.object({
  findings: z
    .array(
      z.object({
        assessment: z.enum(['conflict', 'exception', 'uncertain']),
        explanation: z.string().min(1).max(4096),
        documents: z.array(citationSchema).max(8),
        code: z.array(codeCitation).max(8),
      }),
    )
    .max(12),
  uncertainties: z.array(z.string().min(1).max(2048)).max(24),
});
export const reviewInstructions =
  'Assist the principal reviewer with the task and implementation diff. Discover possible conflicts without requiring suspicions. Explain how documentary rules, direct/indirect dependencies, conditions and exceptions apply. Findings may identify a conflict, a valid exception, or uncertainty; do not turn missing context into approval or reject the entire change. Cite the supplied Markdown ranges and before/after code lines supporting each finding. Distinguish a rule violated by the change from behavior merely seen in context. The reviewer must verify each finding. This is knowledge assistance, not general code review, lint, tests or implementation approval.';

function codeEvidence(citation: z.infer<typeof codeCitation>, implementation: Implementation) {
  const file = implementation.files.find((entry) => entry.path === citation.path)?.[citation.side];
  if (!file || citation.lineEnd < citation.lineStart || citation.lineEnd > file.lines.length)
    return null;
  const lines = file.lines.filter(
    ([number]) => number >= citation.lineStart && number <= citation.lineEnd,
  );
  if (lines.length !== citation.lineEnd - citation.lineStart + 1) return null;
  return { ...citation, version: file.version, text: lines.map(([, text]) => text).join('\n') };
}
export function materializeReview(
  project: Project,
  implementation: Implementation,
  value: unknown,
  supplied: SuppliedDocument[],
) {
  const response = reviewSchema.parse(value);
  const findings = response.findings.map((finding) => {
    const documents = finding.documents
      .map((citation) =>
        suppliedCitation(citation, supplied) ? sourceEvidence(citation, project) : null,
      )
      .filter((entry) => entry !== null);
    const code = finding.code
      .map((citation) => codeEvidence(citation, implementation))
      .filter((entry) => entry !== null);
    const referencesVerified =
      documents.length > 0 &&
      code.length > 0 &&
      documents.length === finding.documents.length &&
      code.length === finding.code.length;
    return {
      ...finding,
      assessment: referencesVerified ? finding.assessment : 'uncertain',
      documents,
      code,
      referencesVerified,
    };
  });
  return {
    findings,
    uncertainties: response.uncertainties,
    invalidReferences: findings.some((finding) => !finding.referencesVerified),
  };
}

const bindingSchema = z.object({
  baseCommit: z.string().regex(/^[a-f0-9]{40,64}$/),
  implementation: z.string().regex(/^[a-f0-9]{64}$/),
  documents: z.string().regex(/^[a-f0-9]{64}$/),
});
export function reviewBinding(project: Project, implementation: Implementation) {
  return {
    baseCommit: implementation.baseCommit,
    implementation: implementation.fingerprint,
    documents: project.snapshot,
  };
}
export function reviewFreshness(root: string, binding: z.infer<typeof bindingSchema>) {
  const project = loadProject(root);
  const implementation = captureImplementation(root, binding.baseCommit);
  const documentsChanged = project.snapshot !== binding.documents;
  const implementationChanged = implementation.fingerprint !== binding.implementation;
  return {
    status: documentsChanged || implementationChanged ? 'stale' : 'current',
    documentsChanged,
    implementationChanged,
  };
}
export function checkReview(args: string[]) {
  const parsed = parseArgs({
    args,
    allowPositionals: true,
    strict: true,
    options: { root: { type: 'string' }, check: { type: 'string' } },
  });
  if (parsed.positionals.length !== 1 || parsed.positionals[0] !== 'review' || !parsed.values.check)
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Use review --check <saved-report.json> [--root <project>].',
    });
  const root = parsed.values.root ?? process.cwd();
  const path = resolve(root, parsed.values.check);
  if (statSync(path).size > 1048576)
    throw new HivexError({ code: 'INVALID_REVIEW', message: 'Saved review exceeds 1 MiB.' });
  const report = z
    .object({ command: z.literal('review'), binding: bindingSchema })
    .parse(JSON.parse(readFileSync(path, 'utf8')));
  return {
    command: 'review-check',
    ...reviewFreshness(root, report.binding),
    guidance: 'Current means the versions still match, not that the implementation is approved.',
  };
}
