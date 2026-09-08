import { z } from 'zod';
import { candidateSchema } from '../ingestion/claims.ts';
import { usageSchema } from '../model/transcript.ts';
import { hash } from '../sources/markdown.ts';

export const digest = z.string().regex(/^[a-f0-9]{64}$/);
export const statementSchema = candidateSchema.shape.claims.element.omit({ id: true });
export const processingSchema = z.strictObject({
  model: z.strictObject({ name: z.string(), effort: z.string(), provider: z.string() }),
  nativeVersion: z.string(),
  requestedPolicyHash: digest,
  schemaHash: digest,
  maximumSourceBytes: z.number().int().positive(),
});
export const sectionSchema = z
  .strictObject({
    anchor: z.string(),
    lineStart: z.number().int().positive(),
    lineEnd: z.number().int().positive(),
  })
  .nullable();
const authoritySchema = z.strictObject({
  declaredStatus: z.string(),
  currentness: z.string(),
  basis: z.string(),
  scope: z.literal('document'),
  supersededBy: z.array(z.string()).max(32),
});
export const reportedProfileSchema = z.strictObject({
  authType: z.string(),
  configuredEndpointOrigin: z.string(),
  model: z.string(),
  modelProvider: z.string(),
  effort: z.string(),
  launchPolicyHash: digest,
});
export const inputUnitSchema = z.strictObject({
  id: z.string(),
  path: z.string(),
  collection: z.string().nullable(),
  contentHash: digest,
  section: sectionSchema,
  authority: authoritySchema,
  sourceBytes: z.number().int().nonnegative(),
  promptBytes: z.number().int().nonnegative(),
  basePromptHash: digest,
  readiness: z.enum(['extractable', 'requires-section']),
});
const sourceSchema = inputUnitSchema.extend({
  readiness: z.literal('extractable'),
  extraction: z.strictObject({
    candidateHash: digest,
    receiptHash: digest,
    candidateAttempt: z.number().int().min(1).max(3),
    claimOrder: z.array(z.string().regex(/^c[1-9][0-9]{0,2}$/)).max(64),
    relationOrder: z
      .array(z.strictObject({ edgeId: digest, fromLocalId: z.string(), toLocalId: z.string() }))
      .max(128),
    reportedProfile: reportedProfileSchema,
    attempts: z
      .array(
        z.strictObject({
          outcome: z.string(),
          code: z.string().optional(),
          usage: usageSchema.nullable(),
          promptHash: digest,
          deadlineMilliseconds: z.number().int().positive(),
        }),
      )
      .min(1)
      .max(3),
  }),
});
export const nodeSchema = z.strictObject({
  id: digest,
  source: z.string(),
  localIds: z.array(z.string()).min(1).max(64),
  statement: statementSchema,
});
const relationSchema = candidateSchema.shape.relations.element;
export const edgeSchema = z.strictObject({
  id: digest,
  source: z.string(),
  from: digest,
  to: digest,
  type: relationSchema.shape.type,
  evidence: relationSchema.shape.evidence,
});
export const contentSchema = z.strictObject({
  format: z.literal('hivex-graph-candidate'),
  version: z.literal(1),
  accepted: z.literal(false),
  sourceSnapshot: z.strictObject({
    commit: z.string().regex(/^[a-f0-9]{40}$/),
    configHash: digest,
  }),
  selection: z.strictObject({ collection: z.string().nullable() }),
  processing: processingSchema,
  inputHash: digest,
  sources: z.array(sourceSchema).max(2048),
  nodes: z.array(nodeSchema).max(131_072),
  edges: z.array(edgeSchema).max(262_144),
});
export type GraphContent = z.infer<typeof contentSchema>;
export type GraphSnapshot = GraphContent & { hash: string };

export function sourceInputHash(input: {
  selection: GraphContent['selection'];
  configHash: string;
  processing: GraphContent['processing'];
  units: unknown[];
}) {
  return hash(
    JSON.stringify({
      selection: input.selection,
      configHash: input.configHash,
      processing: processingSchema.parse(input.processing),
      units: input.units.map((unit) => inputUnitSchema.parse(unit)),
    }),
  );
}

export function compareIds(left: { id: string }, right: { id: string }) {
  if (left.id === right.id) return 0;
  return left.id < right.id ? -1 : 1;
}

export function sealGraph(content: GraphContent): GraphSnapshot {
  const value = contentSchema.parse(content);
  value.sources.sort(compareIds);
  value.nodes.sort(compareIds);
  value.edges.sort(compareIds);
  return { ...value, hash: hash(JSON.stringify(value)) };
}
