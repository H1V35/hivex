import { descendants, parseMarkdown, type Source } from './markdown.ts';

type Citation = { quote: string; lineStart: number; lineEnd: number };

function paragraphs(content: string) {
  return [...descendants(parseMarkdown(content))].flatMap((node) => {
    const position = node.position;
    if (
      node.type !== 'paragraph' ||
      position?.start.offset === undefined ||
      position.end.offset === undefined
    )
      return [];
    const children = [...descendants(node)];
    if (children.some((child) => ['html', 'break'].includes(child.type))) return [];
    return [
      {
        start: position.start.line - 1,
        end: position.end.line - 1,
        textRanges: children
          .filter((child) => child.type === 'text')
          .map((child) => child.position),
      },
    ];
  });
}

function withinText(paragraph: ReturnType<typeof paragraphs>[number], start: number, end: number) {
  return paragraph.textRanges.some(
    (range) =>
      range?.start.offset !== undefined &&
      range.end.offset !== undefined &&
      start >= range.start.offset &&
      end <= range.end.offset,
  );
}

function joinedText(
  paragraph: ReturnType<typeof paragraphs>[number],
  text: string,
  offset: number,
) {
  const joins: { at: number; removed: number }[] = [];
  let removed = 0;
  const joined = text.replace(/[ \t]*\r?\n[ \t]*/g, (wrap: string, index: number) => {
    const from = offset + index;
    if (!withinText(paragraph, from, from + wrap.length) || text[index + wrap.length] === '>')
      return wrap;
    const at = index - removed;
    removed += wrap.length - 1;
    joins.push({ at, removed });
    return ' ';
  });
  return { text: joined, joins };
}

function matchesQuote(
  paragraph: ReturnType<typeof paragraphs>[number],
  content: string,
  offset: number,
  quote: string,
) {
  const source = joinedText(paragraph, content, offset);
  if (!quote.includes('\n')) return source.text.includes(quote);
  const lastLine = quote.split('\n').length - 1;
  const quotedParagraph = paragraphs(quote).find(
    (item) => item.start === 0 && item.end === lastLine,
  );
  if (!quotedParagraph) return false;
  const cited = joinedText(quotedParagraph, quote, 0);
  for (
    let match = source.text.indexOf(cited.text);
    match >= 0;
    match = source.text.indexOf(cited.text, match + 1)
  ) {
    if (
      cited.joins.every(({ at }) => {
        const position = match + at;
        const removed = source.joins.findLast((join) => join.at < position)?.removed ?? 0;
        const original = offset + position + removed;
        return withinText(paragraph, original, original + 1);
      })
    )
      return true;
  }
  return false;
}

export function invalidCitationIndexes(
  source: Pick<Source, 'content' | 'section'>,
  entries: Citation[],
  format: 'markdown' | 'code' = 'markdown',
) {
  const lines = source.content.split('\n');
  const firstLine = source.section?.lineStart ?? 1;
  let parsed: ReturnType<typeof paragraphs> | undefined;
  return entries.flatMap((entry, index) => {
    const start = entry.lineStart - firstLine;
    const end = entry.lineEnd - firstLine;
    const inRange = start >= 0 && end >= start && end < lines.length;
    const content = inRange ? lines.slice(start, end + 1).join('\n') : '';
    if (!inRange) return [index];
    if (content.includes(entry.quote)) return [];
    if (format === 'code') return [index];
    parsed ??= paragraphs(source.content);
    const paragraph = parsed.find((item) => start >= item.start && end <= item.end);
    if (!paragraph) return [index];
    const offset = lines.slice(0, start).reduce((sum, line) => sum + line.length + 1, 0);
    return matchesQuote(paragraph, content, offset, entry.quote) ? [] : [index];
  });
}
