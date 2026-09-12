import { createHash } from "node:crypto";
import { z } from "zod";
import { rawMarkdownLines, sourceRange } from "./markdown.ts";
import type { Document, Project } from "./documents.ts";

export const digest = (value: string | Uint8Array) =>
  createHash("sha256").update(value).digest("hex");
// JSON property lists preserve identities created before the lint migration.
const decisionIdentityFields = [
  "version",
  "id",
  "document",
  "text",
  "kind",
  "status",
  "conditions",
  "exceptions",
  "reason",
  "lineStart",
  "lineEnd",
];
const relationshipIdentityFields = [
  "id",
  "from",
  "to",
  "type",
  "reason",
  "evidence",
  "document",
  "lineStart",
  "lineEnd",
  "version",
];
const explanation = z.string().min(1).max(2048);
export const citationSchema = z.object({
  document: z.string().min(1),
  lineEnd: z.number().int().positive(),
  lineStart: z.number().int().positive(),
});
export interface SuppliedDocument {
  id: string;
  lines: (string | number)[][];
}
export const suppliedCitation = function suppliedCitation(
  entry: z.infer<typeof citationSchema>,
  documents: SuppliedDocument[]
) {
  if (entry.lineEnd < entry.lineStart) {
    return false;
  }
  const lines = new Set(
    documents
      .filter((document) => document.id === entry.document)
      .flatMap((document) => document.lines.map(([number]) => Number(number)))
  );
  for (let line = entry.lineStart; line <= entry.lineEnd; line += 1) {
    if (!lines.has(line)) {
      return false;
    }
  }
  return true;
};
export const decisionSchema = z.object({
  conditions: z.array(explanation).max(16),
  document: z.string().min(1),
  exceptions: z.array(explanation).max(16),
  id: z.string().min(1),
  kind: z.enum(["decision", "constraint", "definition", "lesson"]),
  lineEnd: z.number().int().positive(),
  lineStart: z.number().int().positive(),
  reason: explanation,
  status: z.enum(["current", "proposed", "historical", "uncertain"]),
  text: explanation,
});
export const relationshipSchema = z.object({
  evidence: z.array(citationSchema).min(1).max(8),
  from: z.string().min(1),
  id: z.string().min(1),
  reason: explanation,
  to: z.string().min(1),
  type: z.enum([
    "requires",
    "exception-to",
    "supersedes",
    "supports",
    "contradicts",
  ]),
});
export const extractionSchema = z.object({
  decisions: z.array(decisionSchema).max(64),
  relationships: z.array(relationshipSchema).max(128),
  uncertainties: z.array(explanation).max(32),
});
export const checkSchema = z.object({
  findings: z
    .array(z.object({ reason: explanation, target: z.string().min(1) }))
    .max(64),
});
const quality = z.enum(["unchecked", "checked", "uncertain"]);
const provenance = {
  batch: z.string(),
  localId: z.string(),
  quality,
  version: z.string(),
};
const warningScopeSchema = citationSchema.extend({ version: z.string() });
export type WarningScope = z.infer<typeof warningScopeSchema>;
const warningSchema = z.union([
  z.string(),
  z.object({ message: z.string(), scope: z.array(warningScopeSchema) }),
]);

const fullWarningScope = function fullWarningScope(document: Document) {
  return {
    document: document.id,
    lineEnd: rawMarkdownLines(document.text).length,
    lineStart: 1,
  };
};

export const warningScope = function warningScope(
  documents: Document[],
  ranges?: z.infer<typeof citationSchema>[]
): WarningScope[] {
  return (ranges ?? documents.map(fullWarningScope)).flatMap((range) => {
    const source = documents.find((document) => document.id === range.document);
    if (source === undefined) {
      return [];
    }
    return [
      {
        document: range.document,
        lineEnd: range.lineEnd,
        lineStart: range.lineStart,
        version: source.hash,
      },
    ];
  });
};

export const graphSchema = z.object({
  decisions: z.array(decisionSchema.extend(provenance)),
  documents: z.record(z.string(), z.string()),
  lastExtraction: z.string().optional(),
  relationships: z.array(
    relationshipSchema.extend({
      batch: z.string(),
      evidence: z.array(
        citationSchema.extend({ version: z.string().optional() })
      ),
      localId: z.string(),
      quality,
    })
  ),
  units: z
    .record(
      z.string(),
      z.object({
        document: z.string(),
        version: z.string(),
        workKey: z.string().optional(),
      })
    )
    .default({}),
  version: z.literal(1),
  warnings: z.array(warningSchema),
});
export type Graph = z.infer<typeof graphSchema>;
export type Extraction = z.infer<typeof extractionSchema>;
export type KnowledgeCheck = z.infer<typeof checkSchema>;
export const emptyGraph = function emptyGraph(): Graph {
  return {
    decisions: [],
    documents: {},
    relationships: [],
    units: {},
    version: 1,
    warnings: [],
  };
};

export const validCitation = function validCitation(
  entry: z.infer<typeof citationSchema>,
  documents: Document[]
) {
  const document = documents.find((item) => item.id === entry.document);
  return (
    document !== undefined &&
    entry.lineStart <= entry.lineEnd &&
    entry.lineEnd <= rawMarkdownLines(document.text).length &&
    sourceRange(document.text, entry.lineStart, entry.lineEnd).trim().length > 0
  );
};

export const sourceEvidence = function sourceEvidence(
  entry: z.infer<typeof citationSchema>,
  project: Project
) {
  const document = project.documents.find((item) => item.id === entry.document);
  if (document === undefined || !validCitation(entry, project.documents)) {
    return null;
  }
  return {
    document: entry.document,
    historical: document.historical,
    lineEnd: entry.lineEnd,
    lineStart: entry.lineStart,
    text: sourceRange(document.text, entry.lineStart, entry.lineEnd),
    version: document.hash,
  };
};

const inRanges = function inRanges(
  entry: z.infer<typeof citationSchema>,
  ranges?: z.infer<typeof citationSchema>[]
) {
  if (ranges === undefined) {
    return true;
  }
  for (let line = entry.lineStart; line <= entry.lineEnd; line += 1) {
    const isCovered = ranges.some((range) => {
      if (range.document !== entry.document) {
        return false;
      }
      return range.lineStart <= line && range.lineEnd >= line;
    });
    if (!isCovered) {
      return false;
    }
  }
  return true;
};

interface ExtractionOptions {
  batch: string;
  contextDocuments?: Document[];
  contextRanges?: z.infer<typeof citationSchema>[];
  documents: Document[];
  existingIds?: string[];
  extraction: Extraction;
  graph: Graph;
  targetRanges?: z.infer<typeof citationSchema>[];
}

const retainedWarnings = function retainedWarnings(
  graph: Graph,
  scope: WarningScope[]
) {
  return graph.warnings.filter((warning) => {
    if (typeof warning === "string") {
      return true;
    }
    return warning.scope.every((old) => {
      const hasOverlap = scope.some((current) => {
        if (current.document !== old.document) {
          return false;
        }
        return (
          current.version !== old.version ||
          (current.lineStart <= old.lineEnd && current.lineEnd >= old.lineStart)
        );
      });
      return !hasOverlap;
    });
  });
};

const extractedRelationships = function extractedRelationships(input: {
  options: ExtractionOptions;
  decisions: Graph["decisions"];
  ids: Map<string, string>;
  warnings: string[];
}) {
  const { options, decisions, ids, warnings } = input;
  const { graph, extraction, documents, batch } = options;
  const available = new Set(decisions.map((entry) => entry.id));
  const relationships = graph.relationships.filter((entry) => {
    const hasChangedEvidence = entry.evidence.some((citation) => {
      const isTargetRange =
        options.targetRanges?.some((range) => {
          if (range.document !== citation.document) {
            return false;
          }
          return (
            range.lineStart <= citation.lineEnd &&
            range.lineEnd >= citation.lineStart
          );
        }) === true;
      const hasChangedDocument = (options.contextDocuments ?? documents).some(
        (document) => {
          if (document.id !== citation.document) {
            return false;
          }
          return document.hash !== citation.version;
        }
      );
      return isTargetRange || hasChangedDocument;
    });
    return (
      available.has(entry.from) &&
      available.has(entry.to) &&
      !hasChangedEvidence
    );
  });
  const seen = new Set<string>();
  for (const entry of extraction.relationships) {
    const from = ids.get(entry.from);
    const to = ids.get(entry.to);
    const hasInvalidEvidence = entry.evidence.some((item) => {
      if (!validCitation(item, options.contextDocuments ?? documents)) {
        return true;
      }
      return !inRanges(item, options.contextRanges);
    });
    const hasMissingEndpoint = from === undefined || to === undefined;
    const isDuplicate = seen.has(entry.id);
    if (hasMissingEndpoint || isDuplicate || hasInvalidEvidence) {
      warnings.push(
        `Relationship ${entry.id} has an unknown endpoint, duplicate ID or invalid reference.`
      );
      continue;
    }
    seen.add(entry.id);
    const evidence = entry.evidence.map((citation) => {
      const source = (options.contextDocuments ?? documents).find(
        (document) => document.id === citation.document
      );
      return { ...citation, version: source?.hash };
    });
    const id = digest(
      JSON.stringify(
        { ...entry, evidence, from, to },
        relationshipIdentityFields
      )
    );
    const previous = relationships.findIndex(
      (relationship) => relationship.id === id
    );
    if (previous !== -1) {
      relationships.splice(previous, 1);
    }
    relationships.push({
      ...entry,
      batch,
      evidence,
      from,
      id,
      localId: entry.id,
      quality: "unchecked",
      to,
    });
  }
  return relationships;
};

export const applyExtraction = function applyExtraction(
  options: ExtractionOptions
) {
  const { graph, extraction, documents, batch } = options;
  const decisions = graph.decisions.filter((entry) => {
    const source = documents.find((document) => document.id === entry.document);
    const isTargetRange =
      options.targetRanges?.some((range) => {
        if (range.document !== entry.document) {
          return false;
        }
        return (
          range.lineStart <= entry.lineEnd && range.lineEnd >= entry.lineStart
        );
      }) === true;
    return (
      source === undefined ||
      (source.hash === entry.version &&
        validCitation(entry, [source]) &&
        !isTargetRange)
    );
  });
  const ids = new Map(
    decisions
      .filter((entry) => options.existingIds?.includes(entry.id) === true)
      .map((entry) => [entry.id, entry.id])
  );
  const warnings = [...extraction.uncertainties];
  for (const entry of extraction.decisions) {
    const source = documents.find((document) => document.id === entry.document);
    if (source === undefined || ids.has(entry.id)) {
      warnings.push(
        `Decision ${entry.id} has an unknown, duplicate or invalid source reference.`
      );
      continue;
    }
    const isLocated =
      validCitation(entry, documents) && inRanges(entry, options.targetRanges);
    if (!isLocated) {
      warnings.push(
        `Decision ${entry.id} has an unverified line range; its document remains available.`
      );
    }
    const id = digest(
      JSON.stringify({ version: source.hash, ...entry }, decisionIdentityFields)
    );
    ids.set(entry.id, id);
    const previous = decisions.findIndex((decision) => decision.id === id);
    if (previous !== -1) {
      decisions.splice(previous, 1);
    }
    decisions.push({
      ...entry,
      batch,
      id,
      localId: entry.id,
      quality: isLocated ? "unchecked" : "uncertain",
      version: source.hash,
    });
  }
  const relationships = extractedRelationships({
    decisions,
    ids,
    options,
    warnings,
  });
  return {
    decisions,
    documents: Object.fromEntries(
      Object.entries(graph.documents).filter(([id, version]) => {
        const hasChanged = documents.some((document) => {
          if (document.id !== id) {
            return false;
          }
          return document.hash !== version;
        });
        return !hasChanged;
      })
    ),
    lastExtraction: batch,
    relationships,
    units: Object.fromEntries(
      Object.entries(graph.units).filter(([, unit]) => {
        const hasChanged = documents.some((document) => {
          if (document.id !== unit.document) {
            return false;
          }
          return document.hash !== unit.version;
        });
        return !hasChanged;
      })
    ),
    version: 1 as const,
    warnings: [
      ...retainedWarnings(graph, warningScope(documents, options.targetRanges)),
      ...warnings.map((message) => {
        const scope = warningScope(documents, options.targetRanges);
        return { message, scope };
      }),
    ],
  };
};

const findingScope = function findingScope(
  graph: Graph,
  target: string,
  { batch, fallback }: { batch: string; fallback: WarningScope[] }
): WarningScope[] {
  const decisions = graph.decisions.filter((entry) => {
    if (entry.id === target) {
      return true;
    }
    return (
      entry.batch === batch &&
      (entry.localId === target || entry.document === target)
    );
  });
  const relationships = graph.relationships.filter((entry) => {
    if (entry.id === target) {
      return true;
    }
    return entry.batch === batch && entry.localId === target;
  });
  const evidenceScopeFor = function evidenceScopeFor(
    entry: Graph["relationships"][number]
  ) {
    return entry.evidence.flatMap((citation) => {
      if (citation.version === undefined) {
        return [];
      }
      return [{ ...citation, version: citation.version }];
    });
  };
  const relationshipScope = relationships.flatMap(evidenceScopeFor);
  const scope = [
    ...decisions.map((entry) => {
      const { document, lineEnd, lineStart, version } = entry;
      return { document, lineEnd, lineStart, version };
    }),
    ...relationshipScope,
  ];
  if (scope.length > 0) {
    return scope;
  }
  const documentScope = fallback.filter((entry) => entry.document === target);
  return documentScope.length > 0 ? documentScope : fallback;
};

export const applyCheck = function applyCheck(
  graph: Graph,
  check: KnowledgeCheck,
  { batch, scope = [] }: { batch: string; scope?: WarningScope[] }
): Graph {
  const targets = new Set(check.findings.map((finding) => finding.target));
  const known = new Set([
    "batch",
    ...scope.map((entry) => entry.document),
    ...graph.decisions.map((entry) => entry.id),
    ...graph.relationships.map((entry) => entry.id),
    ...graph.decisions.flatMap((entry) => {
      if (entry.batch !== batch) {
        return [];
      }
      return [entry.localId, entry.document];
    }),
    ...graph.relationships
      .filter((entry) => entry.batch === batch)
      .map((entry) => entry.localId),
  ]);
  const isUncertainBatch =
    targets.has("batch") || [...targets].some((target) => !known.has(target));
  const decisions = graph.decisions.map((entry) => {
    if (entry.batch !== batch && !targets.has(entry.id)) {
      return entry;
    }
    const isTargeted =
      targets.has(entry.id) ||
      targets.has(entry.localId) ||
      targets.has(entry.document);
    const isUncertain =
      isTargeted || entry.quality === "uncertain" || isUncertainBatch;
    return {
      ...entry,
      quality: quality.parse(isUncertain ? "uncertain" : "checked"),
    };
  });
  const relationships = graph.relationships.map((entry) => {
    if (entry.batch !== batch && !targets.has(entry.id)) {
      return entry;
    }
    const isUncertain =
      targets.has(entry.id) ||
      isUncertainBatch ||
      targets.has(entry.localId) ||
      decisions.some((node) => {
        if (node.id !== entry.from && node.id !== entry.to) {
          return false;
        }
        return node.quality === "uncertain";
      });
    return {
      ...entry,
      quality: quality.parse(isUncertain ? "uncertain" : "checked"),
    };
  });
  return {
    ...graph,
    decisions,
    relationships,
    warnings: [
      ...graph.warnings,
      ...check.findings.map((finding) => {
        const findingWarningScope = findingScope(graph, finding.target, {
          batch,
          fallback: scope,
        });
        return { message: finding.reason, scope: findingWarningScope };
      }),
    ],
  };
};
