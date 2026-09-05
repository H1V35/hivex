import { z } from 'zod';
import { HivexError } from '../errors.ts';
import type { Source, SourceBlock } from '../sources/markdown.ts';
import type { Snapshot } from '../workspace/snapshot.ts';

const cursorSchema = z
  .object({
    version: z.literal(1),
    commit: z.string(),
    source: z.string(),
    hash: z.string(),
    next: z.number().int().nonnegative(),
  })
  .strict();
export const encodedBytes = (value: unknown) => Buffer.byteLength(JSON.stringify(value)) + 1;

type Position = z.infer<typeof cursorSchema>;
function resume(cursor: string | undefined, source: Source, commit: string): number {
  if (!cursor) return 0;
  let position: Position;
  try {
    const bytes = Buffer.from(cursor, 'base64url');
    if (bytes.toString('base64url') !== cursor) throw new Error('Noncanonical cursor');
    const value: unknown = JSON.parse(bytes.toString('utf8'));
    position = cursorSchema.parse(value);
  } catch {
    throw new HivexError('INVALID_CURSOR', 'Continuation cursor is invalid');
  }
  if (
    position.commit !== commit ||
    position.source !== source.id ||
    position.hash !== source.contentHash ||
    position.next >= source.blocks.length
  )
    throw new HivexError(
      'CURSOR_MISMATCH',
      'Use the same source and commit as the continuation cursor',
    );
  return position.next;
}

export function cursorFor(source: Source, commit: string, next: number) {
  if (next >= source.blocks.length) return null;
  const position: Position = {
    version: 1,
    commit,
    source: source.id,
    hash: source.contentHash,
    next,
  };
  return Buffer.from(JSON.stringify(position)).toString('base64url');
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
  if (!source) throw new HivexError('SOURCE_NOT_FOUND', 'Source is not declared in this snapshot');
  const first = resume(options.cursor, source, snapshot.commit);
  const blocks: SourceBlock[] = [];
  let next = first;
  for (const block of source.blocks.slice(first)) {
    const candidate = page(snapshot, source, [...blocks, block], next + 1);
    const bytes = encodedBytes(candidate);
    if (bytes > options.maxBytes) {
      if (!blocks.length)
        throw new HivexError(
          'BLOCK_EXCEEDS_BUDGET',
          'The next complete block does not fit; increase --max-bytes',
          {
            requiredBytes: bytes,
            maximumBytes: 65_536,
            lineStart: block.lineStart,
            lineEnd: block.lineEnd,
          },
        );
      break;
    }
    blocks.push(block);
    next += 1;
  }
  const result = page(snapshot, source, blocks, next);
  if (encodedBytes(result) > options.maxBytes)
    throw new HivexError('OUTPUT_BUDGET', 'Source metadata exceeds the output budget');
  return result;
}
