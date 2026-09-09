import { createHash } from 'node:crypto';
import { z } from 'zod';
import { rawMarkdownLines, sourceRange } from './markdown.ts';
import type { Document, Project } from './documents.ts';

export const digest = (value: string) => createHash('sha256').update(value).digest('hex');
const explanation = z.string().min(1).max(2048);
export const citationSchema = z.object({
  document: z.string().min(1),
  lineStart: z.number().int().positive(),
  lineEnd: z.number().int().positive(),
});
export const decisionSchema = z.object({
  id: z.string().min(1),
  document: z.string().min(1),
  text: explanation,
  kind: z.enum(['decision', 'constraint', 'definition', 'lesson']),
  status: z.enum(['current', 'proposed', 'historical', 'uncertain']),
  conditions: z.array(explanation).max(16),
  exceptions: z.array(explanation).max(16),
  reason: explanation,
  lineStart: z.number().int().positive(),
  lineEnd: z.number().int().positive(),
});
export const relationshipSchema = z.object({
  id: z.string().min(1),
  from: z.string().min(1),
  to: z.string().min(1),
  type: z.enum(['requires', 'exception-to', 'supersedes', 'supports', 'contradicts']),
  reason: explanation,
  evidence: z.array(citationSchema).min(1).max(8),
});
export const extractionSchema = z.object({
  decisions: z.array(decisionSchema).max(64),
  relationships: z.array(relationshipSchema).max(128),
  uncertainties: z.array(explanation).max(32),
});
export const checkSchema = z.object({
  findings: z.array(z.object({ target: z.string().min(1), reason: explanation })).max(64),
});
const quality = z.enum(['unchecked', 'checked', 'uncertain']);
const provenance = { version: z.string(), batch: z.string(), localId: z.string(), quality };
export const graphSchema = z.object({
  version: z.literal(1),
  lastExtraction: z.string().optional(),
  documents: z.record(z.string(), z.string()),
  units: z.record(z.string(), z.object({ document: z.string(), version: z.string() })).default({}),
  decisions: z.array(decisionSchema.extend(provenance)),
  relationships: z.array(
    relationshipSchema.extend({
      batch: z.string(),
      localId: z.string(),
      quality,
      evidence: z.array(citationSchema.extend({ version: z.string().optional() })),
    }),
  ),
  warnings: z.array(z.string()),
});
export type Graph = z.infer<typeof graphSchema>;
export type Extraction = z.infer<typeof extractionSchema>;
export type KnowledgeCheck = z.infer<typeof checkSchema>;
export const emptyGraph = (): Graph => ({
  version: 1,
  documents: {},
  units: {},
  decisions: [],
  relationships: [],
  warnings: [],
});

export function validCitation(entry: z.infer<typeof citationSchema>, documents: Document[]) {
  const document = documents.find((item) => item.id === entry.document);
  return (
    document !== undefined &&
    entry.lineStart <= entry.lineEnd &&
    entry.lineEnd <= rawMarkdownLines(document.text).length &&
    sourceRange(document.text, entry.lineStart, entry.lineEnd).trim().length > 0
  );
}

export function sourceEvidence(entry: z.infer<typeof citationSchema>, project: Project) {
  const document = project.documents.find((item) => item.id === entry.document);
  if (!document || !validCitation(entry, project.documents)) return null;
  return {
    ...entry,
    version: document.hash,
    text: sourceRange(document.text, entry.lineStart, entry.lineEnd),
  };
}

function inRanges(
  entry: z.infer<typeof citationSchema>,
  ranges?: z.infer<typeof citationSchema>[],
) {
  if (!ranges) return true;
  for (let line = entry.lineStart; line <= entry.lineEnd; line += 1) {
    if (
      !ranges.some(
        (range) =>
          range.document === entry.document && range.lineStart <= line && range.lineEnd >= line,
      )
    )
      return false;
  }
  return true;
}

type ExtractionOptions = {
  graph: Graph;
  extraction: Extraction;
  documents: Document[];
  batch: string;
  contextDocuments?: Document[];
  existingIds?: string[];
  targetRanges?: z.infer<typeof citationSchema>[];
  contextRanges?: z.infer<typeof citationSchema>[];
};

export function applyExtraction(options: ExtractionOptions) {
  const { graph, extraction, documents, batch } = options;
  const decisions = graph.decisions.filter((entry) => {
    const source = documents.find((document) => document.id === entry.document);
    return !source || source.hash === entry.version;
  });
  const ids = new Map(
    decisions
      .filter((entry) => options.existingIds?.includes(entry.id))
      .map((entry) => [entry.id, entry.id]),
  );
  const warnings = [...extraction.uncertainties];
  for (const entry of extraction.decisions) {
    const source = documents.find((document) => document.id === entry.document);
    if (!source || ids.has(entry.id)) {
      warnings.push(`Decision ${entry.id} has an unknown, duplicate or invalid source reference.`);
      continue;
    }
    const located = validCitation(entry, documents) && inRanges(entry, options.targetRanges);
    if (!located)
      warnings.push(
        `Decision ${entry.id} has an unverified line range; its document remains available.`,
      );
    const id = digest(JSON.stringify({ version: source.hash, ...entry }));
    ids.set(entry.id, id);
    if (decisions.some((decision) => decision.id === id)) continue;
    decisions.push({
      ...entry,
      id,
      localId: entry.id,
      version: source.hash,
      batch,
      quality: located ? 'unchecked' : 'uncertain',
    });
  }
  const relationships = extractedRelationships({ options, decisions, ids, warnings });
  return {
    version: 1 as const,
    lastExtraction: batch,
    documents: Object.fromEntries(
      Object.entries(graph.documents).filter(
        ([id, version]) =>
          !documents.some((document) => document.id === id && document.hash !== version),
      ),
    ),
    units: Object.fromEntries(
      Object.entries(graph.units).filter(
        ([, unit]) =>
          !documents.some(
            (document) => document.id === unit.document && document.hash !== unit.version,
          ),
      ),
    ),
    decisions,
    relationships,
    warnings: [...graph.warnings, ...warnings],
  };
}

function extractedRelationships(input: {
  options: ExtractionOptions;
  decisions: Graph['decisions'];
  ids: Map<string, string>;
  warnings: string[];
}) {
  const { options, decisions, ids, warnings } = input;
  const { graph, extraction, documents, batch } = options;
  const available = new Set(decisions.map((entry) => entry.id));
  const relationships = graph.relationships.filter(
    (entry) =>
      available.has(entry.from) &&
      available.has(entry.to) &&
      !entry.evidence.some((citation) =>
        (options.contextDocuments ?? documents).some(
          (document) => document.id === citation.document && document.hash !== citation.version,
        ),
      ),
  );
  const seen = new Set<string>();
  for (const entry of extraction.relationships) {
    const from = ids.get(entry.from);
    const to = ids.get(entry.to);
    if (
      !from ||
      !to ||
      seen.has(entry.id) ||
      entry.evidence.some(
        (item) =>
          !validCitation(item, options.contextDocuments ?? documents) ||
          !inRanges(item, options.contextRanges),
      )
    ) {
      warnings.push(
        `Relationship ${entry.id} has an unknown endpoint, duplicate ID or invalid reference.`,
      );
      continue;
    }
    seen.add(entry.id);
    const evidence = entry.evidence.map((citation) => ({
      ...citation,
      version: (options.contextDocuments ?? documents).find(
        (document) => document.id === citation.document,
      )?.hash,
    }));
    const id = digest(JSON.stringify({ ...entry, evidence, from, to }));
    if (relationships.some((relationship) => relationship.id === id)) continue;
    relationships.push({
      ...entry,
      evidence,
      id,
      from,
      to,
      localId: entry.id,
      batch,
      quality: 'unchecked',
    });
  }
  return relationships;
}

export function applyCheck(graph: Graph, check: KnowledgeCheck, batch: string): Graph {
  const targets = new Set(check.findings.map((finding) => finding.target));
  const known = new Set([
    'batch',
    ...graph.decisions
      .filter((entry) => entry.batch === batch)
      .flatMap((entry) => [entry.localId, entry.document]),
    ...graph.relationships.filter((entry) => entry.batch === batch).map((entry) => entry.localId),
  ]);
  const uncertainBatch = targets.has('batch') || [...targets].some((target) => !known.has(target));
  const decisions = graph.decisions.map((entry) => {
    if (entry.batch !== batch) return entry;
    const uncertain =
      entry.quality === 'uncertain' ||
      uncertainBatch ||
      targets.has(entry.localId) ||
      targets.has(entry.document);
    return { ...entry, quality: quality.parse(uncertain ? 'uncertain' : 'checked') };
  });
  const relationships = graph.relationships.map((entry) => {
    if (entry.batch !== batch) return entry;
    const uncertain =
      uncertainBatch ||
      targets.has(entry.localId) ||
      decisions.some(
        (node) => (node.id === entry.from || node.id === entry.to) && node.quality === 'uncertain',
      );
    return { ...entry, quality: quality.parse(uncertain ? 'uncertain' : 'checked') };
  });
  return {
    ...graph,
    decisions,
    relationships,
    warnings: [...graph.warnings, ...check.findings.map((finding) => finding.reason)],
  };
}
