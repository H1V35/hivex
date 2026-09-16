import { z } from 'zod';
import { rawMarkdownLines } from './markdown.ts';
import {
  citationSchema,
  isWarningResolved,
  suppliedCitation,
  validCitation,
  warningId,
  withWarningResolution,
} from './knowledge-model.ts';
import type { Document } from './documents.ts';
import type { Graph, SuppliedDocument } from './knowledge-model.ts';

export const warningResolutionsSchema = z
  .array(
    z.object({
      evidence: z.array(citationSchema).min(1).max(32),
      id: z.string().min(1),
      reason: z.string().trim().min(1).max(2048),
    })
  )
  .max(32);

export const warningReviewInstruction =
  ' Review warningCandidates in this same check. Close only a descriptive observation that does not limit a meaningful decision, dependency or exception, or a previous closure still supported by the current documents. Reconsider the entire supplied documents, including later amendments; an unchanged cited paragraph alone is insufficient. Preserve real unanswered choices, contradictions, missing conditions and unavailable evidence. Return warningResolutions with candidate IDs, a specific reason and current citations, or an empty array. Do not repair knowledge through a warning closure or close a warning contradicted by a finding.';

interface ReviewContext {
  documents: Document[];
  supplied: SuppliedDocument[];
  uncertainties: string[];
}

const completeSources = function completeSources(
  context: Pick<ReviewContext, 'documents' | 'supplied'>
) {
  const isComplete = function isComplete(document: Document) {
    return suppliedCitation(
      { document: document.id, lineEnd: rawMarkdownLines(document.text).length, lineStart: 1 },
      context.supplied
    );
  };
  return new Set(context.documents.filter(isComplete).map((document) => document.id));
};

export const warningReviewCandidates = function warningReviewCandidates(
  graph: Graph,
  context: ReviewContext,
  maxBytes = Infinity
) {
  const complete = completeSources(context);
  const candidates = graph.warnings
    .flatMap((warning) => {
      if (typeof warning === 'string' || isWarningResolved(warning, context.documents)) {
        return [];
      }
      // Findings and validation failures require explicit repair, not model dismissal.
      const isLimitation =
        warning.kind === 'limitation' && context.uncertainties.includes(warning.message);
      const isClosure =
        warning.resolution !== undefined &&
        warning.target === undefined &&
        warning.kind !== 'finding' &&
        warning.kind !== 'validation';
      const evidence = [...warning.scope, ...(warning.resolution?.evidence ?? [])];
      if (
        !(isLimitation || isClosure) ||
        evidence.length === 0 ||
        evidence.some((entry) => !complete.has(entry.document))
      ) {
        return [];
      }
      return [
        {
          id: warningId(warning),
          message: warning.message,
          resolution: warning.resolution,
          scope: warning.scope,
        },
      ];
    })
    .toSorted(
      (left, right) =>
        Number(left.resolution !== undefined) - Number(right.resolution !== undefined)
    )
    .slice(0, 32);
  while (candidates.length > 0 && Buffer.byteLength(JSON.stringify(candidates)) > maxBytes) {
    candidates.pop();
  }
  return candidates;
};

export const applyWarningReview = function applyWarningReview(
  graph: Graph,
  options: {
    candidates: ReturnType<typeof warningReviewCandidates>;
    documents: Document[];
    resolutions: z.infer<typeof warningResolutionsSchema>;
    supplied: SuppliedDocument[];
  }
): Graph {
  const candidates = new Map(options.candidates.map((entry) => [entry.id, entry]));
  const complete = completeSources(options);
  const resolutions = new Map<string, z.infer<typeof warningResolutionsSchema>[number]>();
  for (const resolution of options.resolutions) {
    const candidate = candidates.get(resolution.id);
    const isValid = resolution.evidence.every((citation) => {
      const isSupplied =
        complete.has(citation.document) && suppliedCitation(citation, options.supplied);
      return isSupplied && validCitation(citation, options.documents);
    });
    if (candidate !== undefined && isValid && !resolutions.has(resolution.id)) {
      resolutions.set(resolution.id, resolution);
    }
  }
  return {
    ...graph,
    warnings: graph.warnings.map((warning) => {
      const resolution = resolutions.get(warningId(warning));
      if (resolution === undefined) {
        return warning;
      }
      const candidate = candidates.get(resolution.id);
      // Bind the closure to every reviewed source, not just the model's chosen quote.
      const reviewed = [...(candidate?.scope ?? []), ...(candidate?.resolution?.evidence ?? [])];
      const documents = new Set(
        [...reviewed, ...resolution.evidence].map((entry) => entry.document)
      );
      const evidence = options.documents
        .filter((document) => documents.has(document.id))
        .map((document) => {
          const lineEnd = rawMarkdownLines(document.text).length;
          return {
            document: document.id,
            lineEnd,
            lineStart: 1,
            version: document.hash,
          };
        });
      if (evidence.length > 32) {
        return warning;
      }
      return withWarningResolution(warning, { evidence, reason: resolution.reason });
    }),
  };
};

export const warningBaseline = function warningBaseline(graph: Graph) {
  return Object.fromEntries(
    graph.warnings.map((warning) => {
      // A closure is recorded independently of ingestion coverage. Its current
      // freshness is evaluated in warningChanges, including edits before this work.
      const isResolved = typeof warning !== 'string' && warning.resolution !== undefined;
      return [warningId(warning), isResolved ? ('resolved' as const) : ('active' as const)];
    })
  );
};

export const warningChanges = function warningChanges(
  graph: Graph,
  documents: Document[],
  baseline: Record<string, 'active' | 'resolved'>
) {
  const entries = graph.warnings.map((warning) => {
    const state = isWarningResolved(warning, documents) ? 'resolved' : 'active';
    return {
      id: warningId(warning),
      message: typeof warning === 'string' ? warning : warning.message,
      state,
    };
  });
  return {
    new: entries.filter((entry) => baseline[entry.id] === undefined),
    reopened: entries.filter(
      (entry) => baseline[entry.id] === 'resolved' && entry.state === 'active'
    ),
    resolved: entries.filter(
      (entry) => baseline[entry.id] !== 'resolved' && entry.state === 'resolved'
    ),
  };
};
