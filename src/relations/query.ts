import { HivexError } from '../errors.ts';
import { hash, type Source } from '../sources/markdown.ts';
import type { Snapshot } from '../workspace/snapshot.ts';
import { encodedBytes } from '../retrieval/read.ts';
import { loadRelations, resolveReference } from './catalog.ts';
import type { IndexedReference } from './index.ts';

type Catalog = ReturnType<typeof loadRelations>;

function recordsFor(options: { catalog: Catalog; source: Source; commit: string }) {
  const { catalog, source, commit } = options;
  const reference = (value: IndexedReference) =>
    resolveReference({ reference: value, documents: catalog.documents, commit });
  return catalog.indexes.flatMap((index) =>
    index.entries
      .filter((entry) => entry.source.path === source.path)
      .map((entry) => ({
        id: entry.id,
        kind: entry.kind,
        type: entry.type,
        indexedStatus: entry.status,
        indexedText: entry.text,
        reason: entry.reason,
        indexEntry: { path: index.path, hash: index.hash, line: entry.line },
        subject: { ...reference(entry.source), indexedAdr: entry.adr },
        supersededBy: entry.supersededBy.map((pointer) => ({
          ...reference(pointer.source),
          indexedAdr: pointer.adr,
        })),
      }))
      .filter(
        (record) =>
          record.type === 'unresolved' ||
          record.indexedStatus !== 'live' ||
          record.supersededBy.length > 0 ||
          record.subject.resolution !== 'resolved',
      ),
  );
}

function positionFor(options: { cursor?: string; binding: string; total: number }) {
  if (options.cursor === undefined) return 0;
  const match = /^r1\.([a-f0-9]{64})\.([0-9]+)$/.exec(options.cursor);
  if (!match)
    throw new HivexError({
      code: 'INVALID_CURSOR',
      message: 'Relation continuation cursor is invalid',
    });
  const next = Number(match[2]);
  if (match[1] !== options.binding || !Number.isSafeInteger(next) || next >= options.total)
    throw new HivexError({
      code: 'CURSOR_MISMATCH',
      message: 'Use the same source, commit and indexes as the continuation cursor',
    });
  return next;
}

export function relations(options: {
  root: string;
  snapshot: Snapshot;
  id: string;
  maxBytes: number;
  limit: number;
  cursor?: string;
}) {
  const { root, snapshot, id } = options;
  const source = snapshot.sources.find((entry) => entry.id === id);
  if (!source)
    throw new HivexError({
      code: 'SOURCE_NOT_FOUND',
      message: 'Source is not declared in this snapshot',
    });
  const catalog = loadRelations({ root, snapshot });
  const candidates = recordsFor({ catalog, source, commit: snapshot.commit });
  const metadata = {
    snapshot: { commit: snapshot.commit, configHash: snapshot.configHash },
    source: {
      id: source.id,
      path: source.path,
      section: source.section,
      contentHash: source.contentHash,
      authority: source.authority,
    },
    scope: 'document',
    basis: 'derived-index',
    currentness: 'not-established',
    freshness: 'not-established',
    order: 'index-not-precedence',
    indexes: catalog.indexes.map((index) => ({ path: index.path, hash: index.hash })),
    coverage: catalog.indexes.length ? 'configured-indexes' : 'not-configured',
  };
  const binding = hash(
    JSON.stringify({
      commit: snapshot.commit,
      id,
      configHash: snapshot.configHash,
      indexes: metadata.indexes,
    }),
  );
  let next = positionFor({ cursor: options.cursor, binding, total: candidates.length });
  const records: typeof candidates = [];
  const response = (position: number) => ({
    ...metadata,
    records,
    continuation: position < candidates.length ? `r1.${binding}.${position}` : null,
  });
  for (const candidate of candidates.slice(next)) {
    records.push(candidate);
    const requiredBytes = encodedBytes(response(next + 1));
    if (requiredBytes > options.maxBytes) {
      records.pop();
      if (!records.length)
        throw new HivexError({
          code: 'RELATION_EXCEEDS_BUDGET',
          message: 'The next complete indexed record does not fit; increase --max-bytes',
          details: { requiredBytes, maximumBytes: 65_536, id: candidate.id },
        });
      break;
    }
    next += 1;
    if (records.length === options.limit) break;
  }
  const result = response(next);
  if (encodedBytes(result) > options.maxBytes)
    throw new HivexError({
      code: 'OUTPUT_BUDGET',
      message: 'Relation response metadata exceeds the output budget',
    });
  return result;
}
