import { readFileSync, statSync } from "node:fs";
import pathModule from "node:path";
import { parseArgs } from "node:util";
import { z } from "zod";
import { loadProject } from "./documents.ts";
import { captureImplementation } from "./implementation.ts";
import {
  citationSchema,
  sourceEvidence,
  suppliedCitation,
} from "./knowledge-model.ts";
import { HivexError } from "./errors.ts";
import type { Project } from "./documents.ts";
import type { Implementation } from "./implementation.ts";
import type { SuppliedDocument } from "./knowledge-model.ts";

const codeCitation = z.object({
  lineEnd: z.number().int().positive(),
  lineStart: z.number().int().positive(),
  path: z.string().min(1),
  side: z.enum(["before", "after"]),
});
export const reviewSchema = z.object({
  findings: z
    .array(
      z.object({
        assessment: z.enum(["conflict", "exception", "uncertain"]),
        code: z.array(codeCitation).max(8),
        documents: z.array(citationSchema).max(8),
        explanation: z.string().min(1).max(4096),
      })
    )
    .max(12),
  uncertainties: z.array(z.string().min(1).max(2048)).max(24),
});
export const reviewInstructions =
  "Assist the principal reviewer with the task and implementation diff. Discover possible conflicts without requiring suspicions. Explain how documentary rules, direct/indirect dependencies, conditions and exceptions apply. Findings may identify a conflict, a valid exception, or uncertainty; do not turn missing context into approval or reject the entire change. Cite the supplied Markdown ranges and before/after code lines supporting each finding. Distinguish a rule violated by the change from behavior merely seen in context. The reviewer must verify each finding. This is knowledge assistance, not general code review, lint, tests or implementation approval.";

const codeEvidence = function codeEvidence(
  citation: z.infer<typeof codeCitation>,
  implementation: Implementation
) {
  const file = implementation.files.find(
    (entry) => entry.path === citation.path
  )?.[citation.side];
  if (!file || citation.lineEnd < citation.lineStart) {
    return null;
  }
  const lines = file.lines.filter(
    ([number]) => number >= citation.lineStart && number <= citation.lineEnd
  );
  if (lines.length !== citation.lineEnd - citation.lineStart + 1) {
    return null;
  }
  return {
    ...citation,
    text: lines.map(([, text]) => text).join("\n"),
    version: file.version,
  };
};

export const materializeReview = function materializeReview(
  project: Project,
  implementation: Implementation,
  { supplied, value }: { supplied: SuppliedDocument[]; value: unknown }
) {
  const response = reviewSchema.parse(value);
  const findings = response.findings.map((finding) => {
    const documents = finding.documents
      .map((citation) => {
        const isSupplied = suppliedCitation(citation, supplied);
        return isSupplied ? sourceEvidence(citation, project) : null;
      })
      .filter((entry) => entry !== null);
    const code = finding.code
      .map((citation) => codeEvidence(citation, implementation))
      .filter((entry) => entry !== null);
    const areReferencesVerified =
      documents.length > 0 &&
      code.length > 0 &&
      documents.length === finding.documents.length &&
      code.length === finding.code.length;
    return {
      ...finding,
      assessment: areReferencesVerified ? finding.assessment : "uncertain",
      code,
      documents,
      referencesVerified: areReferencesVerified,
    };
  });
  return {
    findings,
    invalidReferences: findings.some((finding) => !finding.referencesVerified),
    uncertainties: response.uncertainties,
  };
};

const bindingSchema = z.object({
  baseCommit: z.string().regex(/^[a-f\d]{40,64}$/u),
  documents: z.string().regex(/^[a-f\d]{64}$/u),
  implementation: z.string().regex(/^[a-f\d]{64}$/u),
});
export const reviewBinding = function reviewBinding(
  project: Project,
  implementation: Implementation
) {
  return {
    baseCommit: implementation.baseCommit,
    documents: project.snapshot,
    implementation: implementation.fingerprint,
  };
};

export const reviewFreshness = function reviewFreshness(
  root: string,
  binding: z.infer<typeof bindingSchema>
) {
  const project = loadProject(root);
  const implementation = captureImplementation(root, binding.baseCommit);
  const areDocumentsChanged = project.snapshot !== binding.documents;
  const isImplementationChanged =
    implementation.fingerprint !== binding.implementation;
  return {
    documentsChanged: areDocumentsChanged,
    implementationChanged: isImplementationChanged,
    status:
      areDocumentsChanged || isImplementationChanged ? "stale" : "current",
  };
};

export const checkReview = function checkReview(reviewArguments: string[]) {
  const parsed = parseArgs({
    allowPositionals: true,
    args: reviewArguments,
    options: { check: { type: "string" }, root: { type: "string" } },
    strict: true,
  });
  if (
    parsed.positionals.length !== 1 ||
    parsed.positionals[0] !== "review" ||
    typeof parsed.values.check !== "string" ||
    parsed.values.check.length === 0
  ) {
    throw new HivexError({
      code: "INVALID_ARGUMENT",
      message: "Use review --check <saved-report.json> [--root <project>].",
    });
  }
  const root = parsed.values.root ?? process.cwd();
  const reportPath = pathModule.resolve(root, parsed.values.check);
  if (statSync(reportPath).size > 1_048_576) {
    throw new HivexError({
      code: "INVALID_REVIEW",
      message: "Saved review exceeds 1 MiB.",
    });
  }
  const report = z
    .object({ binding: bindingSchema, command: z.literal("review") })
    .parse(JSON.parse(readFileSync(reportPath, "utf-8")));
  return {
    command: "review-check",
    ...reviewFreshness(root, report.binding),
    guidance:
      "Current means the versions still match, not that the implementation is approved.",
  };
};
