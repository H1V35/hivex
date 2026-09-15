import { digest, validCitation } from './knowledge-model.ts';
import { HivexError } from './errors.ts';
import { rawMarkdownLines } from './markdown.ts';
import type { Graph } from './knowledge-model.ts';
import type { Document } from './documents.ts';

const maxBytes = 8192;

export interface IngestionUnit {
  document: string;
  hash: string;
  id: string;
  lineEnd: number;
  lineStart: number;
  text: string;
}

interface SourceLine {
  blank: boolean;
  bytes: number;
  fence: { marker: string; length: number; closing: boolean } | null;
  heading: boolean;
  number: number;
  text: string;
}
interface Warning {
  message: string;
  path: string;
}

const contentOf = function contentOf(text: string) {
  return text.replace(/(?:\r\n|\r|\n)$/u, '');
};
const fenceOf = function fenceOf(content: string) {
  const indentationMatch = /^ */u.exec(content);
  const indentation = indentationMatch?.[0].length ?? 0;
  if (indentation > 3) {
    return null;
  }
  const source = content.slice(indentation);
  const marker = source.at(0);
  if (marker !== '`' && marker !== '~') {
    return null;
  }
  const markerMatch = /^(?:`+|~+)/u.exec(source);
  const markerRun = markerMatch?.[0] ?? '';
  if (markerRun.length < 3) {
    return null;
  }
  return {
    closing: source.slice(markerRun.length).trim() === '',
    length: markerRun.length,
    marker,
  };
};
const isHeading = (content: string) => /^\s{0,3}#{1,6}(?:\s|$)/u.test(content);

const sourceLines = function sourceLines(text: string) {
  return rawMarkdownLines(text)
    .filter((line) => line !== '')
    .map((line, index) => {
      const content = contentOf(line);
      return {
        blank: content.trim() === '',
        bytes: Buffer.byteLength(line, 'utf-8'),
        fence: fenceOf(content),
        heading: isHeading(content),
        number: index + 1,
        text: line,
      };
    });
};

const blocksFor = function blocksFor(document: Document, warnings: Warning[]) {
  const blocks: SourceLine[][] = [];
  let block: SourceLine[] = [];
  let activeFence: SourceLine['fence'] = null;
  const flush = () => {
    if (block.length > 0) {
      blocks.push(block);
    }
    block = [];
  };

  for (const line of sourceLines(document.text)) {
    const isInFence = activeFence !== null;
    const isSameFence =
      activeFence !== null &&
      line.fence !== null &&
      line.fence.marker === activeFence.marker &&
      line.fence.length >= activeFence.length;
    const isClosingFence = line.fence?.closing === true && isSameFence;
    if (isClosingFence) {
      activeFence = null;
    } else {
      activeFence ??= line.fence;
    }
    if (line.bytes > maxBytes) {
      flush();
      warnings.push({
        message: `Line ${line.number} is ${line.bytes} UTF-8 bytes, exceeding the ${maxBytes}-byte limit; omitted as unread.`,
        path: document.path,
      });
      continue;
    }
    if (!isInFence && line.heading) {
      flush();
    }
    block.push(line);
    const shouldFlush = activeFence === null && (line.blank || isClosingFence);
    if (shouldFlush) {
      flush();
    }
  }
  flush();
  return blocks;
};

const splitBlock = function splitBlock(block: SourceLine[]) {
  const pieces: SourceLine[][] = [];
  let piece: SourceLine[] = [];
  let bytes = 0;
  for (const line of block) {
    if (piece.length > 0 && bytes + line.bytes > maxBytes) {
      pieces.push(piece);
      piece = [];
      bytes = 0;
    }
    piece.push(line);
    bytes += line.bytes;
  }
  if (piece.length > 0) {
    pieces.push(piece);
  }
  return pieces;
};

const packedBlocks = function packedBlocks(blocks: SourceLine[][]) {
  const packed: SourceLine[][] = [];
  let current: SourceLine[] = [];
  let bytes = 0;
  const flush = () => {
    if (current.length > 0) {
      packed.push(current);
    }
    current = [];
    bytes = 0;
  };

  const splitBlocks = blocks.flatMap((item) => splitBlock(item));
  for (const block of splitBlocks) {
    const first = block.at(0);
    if (first === undefined) {
      continue;
    }
    const blockBytes = block.reduce((total, line) => total + line.bytes, 0);
    const last = current.at(-1);
    const shouldFlush =
      last === undefined || last.number + 1 !== first.number || bytes + blockBytes > maxBytes;
    if (shouldFlush && current.length > 0) {
      flush();
    }
    current.push(...block);
    bytes += blockBytes;
  }
  flush();
  return packed;
};

const makeUnit = function makeUnit(document: Document, lines: SourceLine[]): IngestionUnit {
  const first = lines.at(0);
  const last = lines.at(-1);
  if (first === undefined || last === undefined) {
    throw new Error('Cannot create an empty ingestion unit');
  }
  const text = lines.map((line) => line.text).join('');
  return {
    document: document.id,
    hash: digest(text),
    id: `${document.path}:${first.number}-${last.number}`,
    lineEnd: last.number,
    lineStart: first.number,
    text,
  };
};

export const ingestionUnits = function ingestionUnits(documents: Document[]): {
  units: IngestionUnit[];
  warnings: Warning[];
} {
  const units: IngestionUnit[] = [];
  const warnings: Warning[] = [];
  for (const document of documents) {
    const documentUnits = packedBlocks(blocksFor(document, warnings)).map((lines) =>
      makeUnit(document, lines)
    );
    units.push(...documentUnits);
  }
  return { units, warnings };
};

export type RepairRange = Pick<IngestionUnit, 'document' | 'lineStart' | 'lineEnd'>;

export const unitFromRange = function unitFromRange(document: Document, range: RepairRange) {
  const lines = sourceLines(document.text).slice(range.lineStart - 1, range.lineEnd);
  if (lines.length === 0) {
    throw new HivexError({
      code: 'INVALID_REPAIR_RANGE',
      message: 'The repair range must contain source text.',
    });
  }
  const unit = makeUnit(document, lines);
  if (unit.lineStart !== range.lineStart || unit.lineEnd !== range.lineEnd) {
    throw new HivexError({
      code: 'INVALID_REPAIR_RANGE',
      message: 'The repair range must contain complete source lines.',
    });
  }
  return unit;
};

const expandedRanges = function expandedRanges(
  document: Document,
  decisions: Graph['decisions'],
  requested: RepairRange[]
) {
  const ranges = requested.map((range) => ({ ...range }));
  const current = decisions.filter((entry) => {
    const isCurrent = entry.document === document.id && entry.version === document.hash;
    return isCurrent && validCitation(entry, [document]);
  });
  let hasChanges = true;
  while (hasChanges) {
    hasChanges = false;
    for (const range of ranges) {
      const overlaps = current.filter(
        (entry) => entry.lineStart <= range.lineEnd && entry.lineEnd >= range.lineStart
      );
      const start = Math.min(range.lineStart, ...overlaps.map((entry) => entry.lineStart));
      const end = Math.max(range.lineEnd, ...overlaps.map((entry) => entry.lineEnd));
      hasChanges ||= start !== range.lineStart || end !== range.lineEnd;
      range.lineStart = start;
      range.lineEnd = end;
    }
  }
  const merged: RepairRange[] = [];
  const ordered = ranges.toSorted((a, b) => a.lineStart - b.lineStart);
  for (const range of ordered) {
    const previous = merged.at(-1);
    if (previous && previous.lineEnd >= range.lineStart) {
      previous.lineEnd = Math.max(previous.lineEnd, range.lineEnd);
    } else {
      merged.push(range);
    }
  }
  return merged;
};

export const repairUnits = function repairUnits(
  documents: Document[],
  decisions: Graph['decisions'],
  ranges: RepairRange[]
) {
  return documents.flatMap((document) => {
    const selected = ranges.filter((range) => range.document === document.id);
    return expandedRanges(document, decisions, selected).map((range) =>
      unitFromRange(document, range)
    );
  });
};

export const validateRepairUnitSize = function validateRepairUnitSize(units: IngestionUnit[]) {
  if (units.some((unit) => Buffer.byteLength(unit.text) > 2 * maxBytes)) {
    throw new HivexError({
      code: 'REPAIR_RANGE_TOO_LARGE',
      message:
        'A complete decision range exceeds the 16 KiB round limit. Inspect its source scope before repairing it.',
    });
  }
};
