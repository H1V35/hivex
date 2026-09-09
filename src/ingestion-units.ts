import type { Document } from './documents.ts';
import { rawMarkdownLines } from './markdown.ts';
import { digest } from './knowledge-model.ts';

const MAX_BYTES = 8192;

export type IngestionUnit = {
  id: string;
  document: string;
  hash: string;
  lineStart: number;
  lineEnd: number;
  text: string;
};

type SourceLine = {
  number: number;
  text: string;
  bytes: number;
  blank: boolean;
  heading: boolean;
  fence: { marker: string; length: number; closing: boolean } | null;
};
type Warning = { path: string; message: string };

const contentOf = (text: string) => text.replace(/(?:\r\n|\r|\n)$/, '');
function fenceOf(content: string) {
  const match = /^ {0,3}(`{3,}|~{3,})(.*)$/.exec(content);
  return match?.[1]
    ? { marker: match[1].charAt(0), length: match[1].length, closing: !match[2]?.trim() }
    : null;
}
const headingOf = (content: string) => /^\s{0,3}#{1,6}(?:\s|$)/.test(content);

function sourceLines(text: string) {
  return rawMarkdownLines(text)
    .filter((line) => line !== '')
    .map((line, index) => {
      const content = contentOf(line);
      return {
        number: index + 1,
        text: line,
        bytes: Buffer.byteLength(line, 'utf8'),
        blank: content.trim() === '',
        heading: headingOf(content),
        fence: fenceOf(content),
      };
    });
}

function blocksFor(document: Document, warnings: Warning[]) {
  const blocks: SourceLine[][] = [];
  let block: SourceLine[] = [];
  let activeFence: SourceLine['fence'] = null;
  const flush = () => {
    if (block.length) blocks.push(block);
    block = [];
  };

  for (const line of sourceLines(document.text)) {
    const inFence = activeFence !== null;
    const closingFence =
      activeFence &&
      line.fence?.closing &&
      line.fence.marker === activeFence.marker &&
      line.fence.length >= activeFence.length;
    if (closingFence) activeFence = null;
    else if (!activeFence) activeFence = line.fence;
    if (line.bytes > MAX_BYTES) {
      flush();
      warnings.push({
        path: document.path,
        message: `Line ${line.number} is ${line.bytes} UTF-8 bytes, exceeding the ${MAX_BYTES}-byte limit; omitted as unread.`,
      });
      continue;
    }
    if (!inFence && line.heading) flush();
    block.push(line);
    if (!activeFence && (line.blank || closingFence)) flush();
  }
  flush();
  return blocks;
}

function splitBlock(block: SourceLine[]) {
  const pieces: SourceLine[][] = [];
  let piece: SourceLine[] = [];
  let bytes = 0;
  for (const line of block) {
    if (piece.length && bytes + line.bytes > MAX_BYTES) {
      pieces.push(piece);
      piece = [];
      bytes = 0;
    }
    piece.push(line);
    bytes += line.bytes;
  }
  if (piece.length) pieces.push(piece);
  return pieces;
}

function packedBlocks(blocks: SourceLine[][]) {
  const packed: SourceLine[][] = [];
  let current: SourceLine[] = [];
  let bytes = 0;
  const flush = () => {
    if (current.length) packed.push(current);
    current = [];
    bytes = 0;
  };

  for (const block of blocks.flatMap((item) => splitBlock(item))) {
    const first = block.at(0);
    if (!first) continue;
    const blockBytes = block.reduce((total, line) => total + line.bytes, 0);
    const last = current.at(-1);
    if (
      current.length &&
      (bytes + blockBytes > MAX_BYTES || !last || last.number + 1 !== first.number)
    )
      flush();
    current.push(...block);
    bytes += blockBytes;
  }
  flush();
  return packed;
}

function makeUnit(document: Document, lines: SourceLine[]): IngestionUnit {
  const first = lines.at(0);
  const last = lines.at(-1);
  if (!first || !last) throw new Error('Cannot create an empty ingestion unit');
  const text = lines.map((line) => line.text).join('');
  return {
    id: `${document.path}:${first.number}-${last.number}`,
    document: document.id,
    hash: digest(text),
    lineStart: first.number,
    lineEnd: last.number,
    text,
  };
}

export function ingestionUnits(documents: Document[]): {
  units: IngestionUnit[];
  warnings: Warning[];
} {
  const units: IngestionUnit[] = [];
  const warnings: Warning[] = [];
  for (const document of documents)
    units.push(
      ...packedBlocks(blocksFor(document, warnings)).map((lines) => makeUnit(document, lines)),
    );
  return { units, warnings };
}
