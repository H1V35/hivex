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
    if (children.some((child) => ['inlineCode', 'html', 'break'].includes(child.type))) return [];
    return [
      {
        start: position.start.line - 1,
        end: position.end.line - 1,
        offset: position.start.offset,
        lines: content.slice(position.start.offset, position.end.offset).split('\n'),
        textRanges: children
          .filter((child) => child.type === 'text')
          .map((child) => child.position),
      },
    ];
  });
}

function joinedText(paragraph: ReturnType<typeof paragraphs>[number], start: number, end: number) {
  const first = start - paragraph.start;
  const offset =
    paragraph.offset +
    paragraph.lines.slice(0, first).reduce((sum, line) => sum + line.length + 1, 0);
  const text = paragraph.lines.slice(first, end - paragraph.start + 1).join('\n');
  return text.replace(/[ \t]*\r?\n[ \t]*/g, (wrap: string, index: number) => {
    const from = offset + index;
    const withinText = paragraph.textRanges.some(
      (range) =>
        range?.start.offset !== undefined &&
        range.end.offset !== undefined &&
        from >= range.start.offset &&
        from + wrap.length <= range.end.offset,
    );
    return withinText && text[index + wrap.length] !== '>' ? ' ' : wrap;
  });
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
    return paragraph && joinedText(paragraph, start, end).includes(entry.quote) ? [] : [index];
  });
}
