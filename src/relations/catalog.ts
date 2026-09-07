import { HivexError } from '../errors.ts';
import { parseSource, type Source } from '../sources/markdown.ts';
import { selectionsFor } from '../workspace/config.ts';
import { blobs } from '../workspace/git.ts';
import type { Snapshot } from '../workspace/snapshot.ts';
import { cursorFor } from '../retrieval/read.ts';
import { MAX_RELATION_RECORDS, parseIndex, type IndexedReference } from './index.ts';

function regularFile(snapshot: Snapshot, path: string) {
  const file = snapshot.files.find((entry) => entry.path === path);
  if (!file || !['100644', '100755'].includes(file.mode))
    throw new HivexError({
      code: 'INVALID_RELATION_FILE',
      message: `Relation input must be a regular tracked file: ${path}`,
    });
  return file;
}

export function loadRelations(options: { root: string; snapshot: Snapshot }) {
  const { root, snapshot } = options;
  const indexFiles = snapshot.config.relationIndexes.map((index) =>
    regularFile(snapshot, index.path),
  );
  const contents = blobs(root, indexFiles);
  const indexes = indexFiles.map((file) =>
    parseIndex({ path: file.path, text: contents.get(file.path) ?? '' }),
  );
  if (indexes.reduce((count, index) => count + index.entries.length, 0) > MAX_RELATION_RECORDS)
    throw new HivexError({
      code: 'TOO_MANY_RELATION_RECORDS',
      message: 'At most 10000 indexed records may be loaded per query',
    });
  const paths = new Set(
    indexes.flatMap((index) =>
      index.entries.flatMap((entry) => [
        entry.source.path,
        ...entry.supersededBy.map((pointer) => pointer.source.path),
      ]),
    ),
  );
  if (paths.size > 2048)
    throw new HivexError({
      code: 'TOO_MANY_SOURCES',
      message: 'Relation indexes may reference at most 2048 Markdown documents',
    });
  const files = [...paths].map((path) => {
    if (
      !/\.(?:md|markdown|mdown)$/i.test(path) ||
      !selectionsFor(path, snapshot.config.collections).length
    )
      throw new HivexError({
        code: 'UNDECLARED_RELATION_SOURCE',
        message: `Relation references an undeclared Markdown document: ${path}`,
      });
    return regularFile(snapshot, path);
  });
  const markdown = blobs(root, files);
  const documents = new Map(
    files.map((file) => [
      file.path,
      parseSource({ path: file.path, content: markdown.get(file.path) ?? '', collection: null }),
    ]),
  );
  return { indexes, documents };
}

export function resolveReference(options: {
  reference: IndexedReference;
  documents: Map<string, Source>;
  commit: string;
}) {
  const { reference, documents, commit } = options;
  const document = documents.get(reference.path);
  if (!document)
    throw new HivexError({
      code: 'INVALID_RELATION_FILE',
      message: 'Referenced document was not loaded',
    });
  const heading = document.headings.find((entry) => entry.anchor === reference.anchor);
  const block = heading && document.blocks[heading.block];
  let resolution = 'missing-anchor';
  if (document.containedAnchors.includes(reference.anchor)) resolution = 'contained-heading';
  if (heading) resolution = 'resolved';
  return {
    path: reference.path,
    anchor: reference.anchor,
    indexedHeading: reference.section,
    contentHash: document.contentHash,
    resolution,
    location: block ? { lineStart: block.lineStart, lineEnd: block.lineEnd } : null,
    readCursor: cursorFor(document, commit, heading?.block ?? 0),
  };
}
