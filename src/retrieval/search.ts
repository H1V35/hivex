import { Database } from 'bun:sqlite';
import { basename } from 'node:path';
import { z } from 'zod';
import { HivexError } from '../errors.ts';
import type { Source } from '../sources/markdown.ts';
import type { Snapshot } from '../workspace/snapshot.ts';
import { cursorFor, encodedBytes } from './read.ts';

const row = z.object({ id: z.string(), score: z.number() });
const terms = (text: string) => [
  ...new Set(
    text
      .toLowerCase()
      .normalize('NFKC')
      .match(/[\p{L}\p{N}]+/gu) ?? [],
  ),
];
type Ranked = { id: string; kind: 'identifier' | 'text'; score: number | null };

function lexical(snapshot: Snapshot, query: string): Ranked[] {
  const db = new Database(':memory:');
  try {
    db.run(
      'CREATE VIRTUAL TABLE sources USING fts5(id UNINDEXED, title, content, tokenize=unicode61)',
    );
    const insert = db.prepare('INSERT INTO sources VALUES (?, ?, ?)');
    db.transaction(() => {
      for (const source of snapshot.sources) insert.run(source.id, source.title, source.content);
    })();
    const expression = terms(query)
      .map((term) => `"${term}"`)
      .join(' OR ');
    if (!expression) return [];
    return db
      .prepare(
        'SELECT id, bm25(sources) AS score FROM sources WHERE sources MATCH ? ORDER BY score, id LIMIT 32',
      )
      .all(expression)
      .map((value) => ({ ...row.parse(value), kind: 'text' }));
  } finally {
    db.close();
  }
}

function identifiers(snapshot: Snapshot, query: string): Ranked[] {
  const prefixes = new Map(snapshot.config.collections.map((item) => [item.id, item.aliasPrefix]));
  return snapshot.sources
    .filter((source) => {
      if (query === source.id) return true;
      const prefix = prefixes.get(source.collection ?? '');
      const number = basename(source.path).match(/^(\d{1,6})[-_]/)?.[1];
      if (!prefix || !number) return false;
      return [...query.matchAll(new RegExp(`\\b${prefix}[\\s:_-]*(\\d{1,6})\\b`, 'gi'))].some(
        (match) => Number(match[1]) === Number(number),
      );
    })
    .map((source) => source.id)
    .sort()
    .map((id) => ({ id, kind: 'identifier', score: null }));
}

function matchedBlock(source: Source, query: string) {
  const tokens = terms(query);
  const ranked = source.blocks
    .map((block, index) => {
      const words = new Set(terms(block.text));
      return { block, index, score: tokens.filter((token) => words.has(token)).length };
    })
    .filter((entry) => entry.block.kind !== 'yaml')
    .sort((a, b) => b.score - a.score || a.index - b.index);
  const best = ranked[0];
  if (!best) return { preview: null, start: 0, location: null };
  const heading = source.blocks
    .slice(0, best.index + 1)
    .findLastIndex((block) => block.anchor !== null);
  const anchor = source.blocks[heading]?.anchor ?? null;
  return {
    preview: Buffer.byteLength(best.block.text) <= 384 ? best.block.text : null,
    start: Math.max(0, heading),
    location: { anchor, lineStart: best.block.lineStart, lineEnd: best.block.lineEnd },
  };
}

function card(snapshot: Snapshot, source: Source, match: Ranked, query: string) {
  const block = matchedBlock(source, query);
  return {
    id: source.id,
    title: source.title,
    path: source.path,
    collection: source.collection,
    collectionKind: source.collectionKind,
    contentHash: source.contentHash,
    section: source.section,
    authority: source.authority,
    match: { kind: match.kind, bm25: match.score, location: block.location },
    preview: block.preview,
    evidenceCompleteness: 'not-assessed',
    readCursor: cursorFor(source, snapshot.commit, match.kind === 'identifier' ? 0 : block.start),
  };
}

export function search(
  snapshot: Snapshot,
  query: string,
  options: { limit: number; maxBytes: number },
) {
  const unique = new Map<string, Ranked>();
  for (const match of [...identifiers(snapshot, query), ...lexical(snapshot, query)])
    if (!unique.has(match.id)) unique.set(match.id, match);
  const ranked = [...unique.values()];
  const byId = new Map(snapshot.sources.map((source) => [source.id, source]));
  const results: ReturnType<typeof card>[] = [];
  const response = () => ({
    snapshot: { commit: snapshot.commit, configHash: snapshot.configHash },
    results,
    truncated: results.length < ranked.length,
  });
  for (const match of ranked) {
    const source = byId.get(match.id);
    if (!source)
      throw new HivexError({
        code: 'INVALID_INDEX',
        message: 'Derived index returned an unknown source',
      });
    const item = card(snapshot, source, match, query);
    results.push(item);
    if (encodedBytes(response()) > options.maxBytes) item.preview = null;
    if (encodedBytes(response()) > options.maxBytes) {
      results.pop();
      break;
    }
    if (results.length === options.limit) break;
  }
  if (ranked.length > 0 && results.length === 0)
    throw new HivexError({
      code: 'OUTPUT_BUDGET',
      message: 'The highest-ranked source metadata does not fit; increase --max-bytes',
    });
  if (encodedBytes(response()) > options.maxBytes)
    throw new HivexError({
      code: 'OUTPUT_BUDGET',
      message: 'Response metadata exceeds the output budget',
    });
  return response();
}
