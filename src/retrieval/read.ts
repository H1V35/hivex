import { HivexError } from '../errors.ts';
import { hash, type Source, type SourceBlock } from '../sources/markdown.ts';
import type { Snapshot } from '../workspace/snapshot.ts';

export const encodedBytes = (value: unknown) => Buffer.byteLength(JSON.stringify(value)) + 1;

function binding(source: Source, commit: string): string {
  return hash(`${commit}\0${source.id}\0${source.contentHash}`);
}

function resume(cursor: string | undefined, source: Source, commit: string): number {
  if (cursor === undefined) return 0;
  const match = /^1\.([a-f0-9]{64})\.([0-9]+)$/.exec(cursor);
  if (!match)
    throw new HivexError({ code: 'INVALID_CURSOR', message: 'Continuation cursor is invalid' });
  const next = Number(match[2]);
  if (
    match[1] !== binding(source, commit) ||
    !Number.isSafeInteger(next) ||
    next >= source.blocks.length
  )
    throw new HivexError({
      code: 'CURSOR_MISMATCH',
      message: 'Use the same source and commit as the continuation cursor',
    });
  return next;
}

export function cursorFor(source: Source, commit: string, next: number) {
  if (next >= source.blocks.length) return null;
  return `1.${binding(source, commit)}.${next}`;
}

function page(snapshot: Snapshot, source: Source, blocks: SourceBlock[], next: number) {
  return {
    snapshot: { commit: snapshot.commit, configHash: snapshot.configHash },
    source: {
      id: source.id,
      path: source.path,
      title: source.title,
      collection: source.collection,
      contentHash: source.contentHash,
      section: source.section,
      authority: source.authority,
    },
    blocks,
    continuation: cursorFor(source, snapshot.commit, next),
  };
}

export function read(
  snapshot: Snapshot,
  id: string,
  options: { maxBytes: number; cursor?: string },
) {
  const source = snapshot.sources.find((entry) => entry.id === id);
  if (!source)
    throw new HivexError({
      code: 'SOURCE_NOT_FOUND',
      message: 'Source is not declared in this snapshot',
    });
  const first = resume(options.cursor, source, snapshot.commit);
  const blocks: SourceBlock[] = [];
  let next = first;
  for (const block of source.blocks.slice(first)) {
    const candidate = page(snapshot, source, [...blocks, block], next + 1);
    const bytes = encodedBytes(candidate);
    if (bytes > options.maxBytes) {
      if (!blocks.length)
        throw new HivexError({
          code: 'BLOCK_EXCEEDS_BUDGET',
          message: 'The next complete block does not fit; increase --max-bytes',
          details: {
            requiredBytes: bytes,
            maximumBytes: 65_536,
            lineStart: block.lineStart,
            lineEnd: block.lineEnd,
          },
        });
      break;
    }
    blocks.push(block);
    next += 1;
  }
  const result = page(snapshot, source, blocks, next);
  if (encodedBytes(result) > options.maxBytes)
    throw new HivexError({
      code: 'OUTPUT_BUDGET',
      message: 'Source metadata exceeds the output budget',
    });
  return result;
}
