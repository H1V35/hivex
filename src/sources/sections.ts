import { HivexError } from '../errors.ts';
import type { Collection } from '../workspace/config.ts';
import type { Source } from './markdown.ts';

export function selectSection(
  document: Source,
  selection: { anchor: string; collection: Collection },
): Source {
  const heading = document.headings.find((entry) => entry.anchor === selection.anchor);
  if (!heading)
    throw new HivexError({
      code: 'SECTION_NOT_FOUND',
      message: `No declared heading in ${document.path}: ${selection.anchor}`,
    });
  const next = document.headings.find(
    (entry) => entry.block > heading.block && entry.depth <= heading.depth,
  );
  const blocks = document.blocks.slice(heading.block, next?.block);
  return {
    ...document,
    id: `${document.path}#${selection.anchor}`,
    title: heading.title,
    collection: selection.collection.id,
    collectionKind: selection.collection.kind,
    content: document.content.slice(heading.offset, next?.offset),
    blocks,
    section: {
      anchor: selection.anchor,
      lineStart: blocks[0]?.lineStart ?? 1,
      lineEnd: blocks.at(-1)?.lineEnd ?? 1,
    },
  };
}

export function selectedSources(
  document: Source,
  selections: { anchor: string | null; collection: Collection }[],
): Source[] {
  const sources = selections.map((selection) =>
    selection.anchor === null
      ? {
          ...document,
          collection: selection.collection.id,
          collectionKind: selection.collection.kind,
        }
      : selectSection(document, { anchor: selection.anchor, collection: selection.collection }),
  );
  const ranges = sources
    .map((source) => ({
      start: source.section?.lineStart ?? 1,
      end: source.section?.lineEnd ?? Number.MAX_SAFE_INTEGER,
    }))
    .sort((a, b) => a.start - b.start);
  for (const [index, range] of ranges.entries()) {
    const previous = ranges[index - 1];
    if (previous && range.start <= previous.end)
      throw new HivexError({
        code: 'AMBIGUOUS_COLLECTION',
        message: `Overlapping selections include ${document.path}`,
      });
  }
  return sources;
}
