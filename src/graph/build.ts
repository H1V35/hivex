import { isDeepStrictEqual } from 'node:util';
import { z } from 'zod';
import { HivexError } from '../errors.ts';
import { createPlan } from '../ingestion/plan.ts';
import { IngestionStore } from '../ingestion/store.ts';
import { validateCandidateEvidence } from '../ingestion/evidence.ts';
import { hash } from '../sources/markdown.ts';
import { loadSnapshot, type Snapshot } from '../workspace/snapshot.ts';
import {
  digest,
  sectionSchema,
  sealGraph,
  statementSchema,
  reportedProfileSchema,
  inputUnitSchema,
  sourceInputHash,
  type GraphContent,
} from './snapshot.ts';

type Plan = ReturnType<typeof createPlan>;
const provenanceSchema = z.object({
  snapshot: z.object({ commit: z.string(), configHash: digest }),
  source: z.object({
    id: z.string(),
    path: z.string(),
    contentHash: digest,
    section: sectionSchema,
  }),
  model: z.object({ name: z.string(), effort: z.string(), provider: z.string() }),
  contract: z.object({
    nativeVersion: z.string(),
    requestedPolicyHash: digest,
    basePromptHash: digest,
    schemaHash: digest,
  }),
  association: z
    .strictObject({
      planHash: digest,
      snapshot: z.strictObject({ commit: z.string(), configHash: digest }),
      originalHash: digest,
    })
    .optional(),
});
const completedReceiptSchema = z.object({
  outcome: z.literal('completed'),
  cleanup: z.literal('confirmed'),
  turnAccepted: z.literal('confirmed'),
  admission: reportedProfileSchema.strip(),
});

export function inputHash(plan: Plan) {
  return sourceInputHash({
    selection: plan.selection,
    configHash: plan.snapshot.configHash,
    processing: plan.processing,
    units: plan.units,
  });
}

function validateProvenance(record: unknown, source: Snapshot['sources'][number], plan: Plan) {
  const raw = z.record(z.string(), z.unknown()).parse(record);
  const provenance = provenanceSchema.parse(raw);
  const unit = plan.units.find((item) => item.id === source.id);
  const { association } = provenance;
  const { association: _association, ...original } = raw;
  if (
    (association &&
      (association.planHash !== plan.planHash ||
        !isDeepStrictEqual(association.snapshot, plan.snapshot) ||
        association.originalHash !== hash(JSON.stringify(original)))) ||
    (!association && !isDeepStrictEqual(provenance.snapshot, plan.snapshot)) ||
    !isDeepStrictEqual(provenance.model, plan.processing.model) ||
    !isDeepStrictEqual(provenance.source, {
      id: source.id,
      path: source.path,
      contentHash: source.contentHash,
      section: source.section,
    }) ||
    !isDeepStrictEqual(provenance.contract, {
      nativeVersion: plan.processing.nativeVersion,
      requestedPolicyHash: plan.processing.requestedPolicyHash,
      basePromptHash: unit?.basePromptHash,
      schemaHash: plan.processing.schemaHash,
    })
  )
    throw new HivexError({
      code: 'GRAPH_PROVENANCE_MISMATCH',
      message: 'A retained candidate does not match its declared inputs',
    });
}

type CandidateRecord = ReturnType<typeof IngestionStore.candidates>[number];
type Candidate = NonNullable<CandidateRecord['candidate']>;
type GraphSource = GraphContent['sources'][number];

function describeSource(record: CandidateRecord, source: Snapshot['sources'][number], plan: Plan) {
  if (record.candidate === null || record.candidateAttempt === null)
    throw new HivexError({
      code: 'GRAPH_SOURCE_MISSING',
      message: 'The source has no complete candidate',
    });
  validateProvenance(record, source, plan);
  validateCandidateEvidence({ candidate: record.candidate, source });
  const receipt = completedReceiptSchema.parse(record.attempts[record.candidateAttempt - 1]);
  if (
    receipt.admission.model !== plan.processing.model.name ||
    receipt.admission.modelProvider !== plan.processing.model.provider ||
    receipt.admission.effort !== plan.processing.model.effort
  )
    throw new HivexError({
      code: 'GRAPH_PROVENANCE_MISMATCH',
      message: 'The reported model differs from the ingestion profile',
    });
  const descriptor: GraphSource = {
    ...inputUnitSchema.parse(plan.units.find((unit) => unit.id === source.id)),
    readiness: 'extractable',
    extraction: {
      candidateHash: hash(JSON.stringify(record.candidate)),
      receiptHash: hash(JSON.stringify(record)),
      candidateAttempt: record.candidateAttempt,
      reportedProfile: receipt.admission,
      claimOrder: record.candidate.claims.map((claim) => claim.id),
      relationOrder: [],
      attempts: record.attempts.map((attempt) => ({
        outcome: attempt.outcome,
        code: attempt.code,
        usage: attempt.usage,
        promptHash: attempt.promptHash,
        deadlineMilliseconds: attempt.deadlineMilliseconds,
      })),
    },
  };
  return { source: descriptor, candidate: record.candidate };
}

function indexClaims(
  source: string,
  claims: Candidate['claims'],
  nodes: Map<string, GraphContent['nodes'][number]>,
) {
  const localIds = new Map<string, string>();
  for (const claim of claims) {
    const { id: localId, ...body } = claim;
    const statement = statementSchema.parse(body);
    const id = hash(JSON.stringify({ source, statement }));
    const existing = nodes.get(id);
    if (existing) existing.localIds.push(localId);
    else nodes.set(id, { id, source, localIds: [localId], statement });
    localIds.set(localId, id);
  }
  return localIds;
}

function indexRelations(
  source: string,
  relations: Candidate['relations'],
  localIds: Map<string, string>,
  edges: Map<string, GraphContent['edges'][number]>,
) {
  const order: GraphSource['extraction']['relationOrder'] = [];
  for (const relation of relations) {
    const from = localIds.get(relation.from);
    const to = localIds.get(relation.to);
    if (!from || !to)
      throw new HivexError({
        code: 'GRAPH_RELATION_INVALID',
        message: 'A relation refers to a missing claim',
      });
    const edge = { source, from, to, type: relation.type, evidence: relation.evidence };
    const id = hash(JSON.stringify(edge));
    edges.set(id, { id, ...edge });
    order.push({ edgeId: id, fromLocalId: relation.from, toLocalId: relation.to });
  }
  return order;
}

function compile(snapshot: Snapshot, plan: Plan, records: CandidateRecord[]) {
  const sources: GraphContent['sources'] = [];
  const nodes = new Map<string, GraphContent['nodes'][number]>();
  const edges = new Map<string, GraphContent['edges'][number]>();
  const byId = new Map(snapshot.sources.map((source) => [source.id, source]));
  for (const record of records) {
    const source = byId.get(record.source.id);
    if (!source)
      throw new HivexError({
        code: 'GRAPH_SOURCE_MISSING',
        message: 'The candidate source is missing from its snapshot',
      });
    const prepared = describeSource(record, source, plan);
    const localIds = indexClaims(source.id, prepared.candidate.claims, nodes);
    prepared.source.extraction.relationOrder = indexRelations(
      source.id,
      prepared.candidate.relations,
      localIds,
      edges,
    );
    sources.push(prepared.source);
  }
  return sealGraph({
    format: 'hivex-graph-candidate',
    version: 1,
    accepted: false,
    sourceSnapshot: plan.snapshot,
    selection: plan.selection,
    processing: plan.processing,
    inputHash: inputHash(plan),
    sources,
    nodes: [...nodes.values()],
    edges: [...edges.values()],
  });
}

export function buildGraph(root: string, store: string) {
  const selection = IngestionStore.selection(store);
  if (!selection)
    throw new HivexError({
      code: 'INGESTION_NOT_READY',
      message: 'A complete ingestion cohort is required',
    });
  const snapshot = loadSnapshot({
    root,
    ref: selection.ref,
    selection: { collection: selection.collection ?? undefined },
  });
  const plan = createPlan(snapshot, selection.collection);
  return compile(snapshot, plan, IngestionStore.candidates(store, plan));
}
