import type { Source } from './markdown.ts';

type Citation = { quote: string; lineStart: number; lineEnd: number };

export function invalidCitationIndexes(source: Source, entries: Citation[]) {
  const lines = source.content.split('\n');
  const firstLine = source.section?.lineStart ?? 1;
  return entries.flatMap((entry, index) => {
    const start = entry.lineStart - firstLine;
    const end = entry.lineEnd - firstLine;
    const inRange = start >= 0 && end >= start && end < lines.length;
    const content = inRange ? lines.slice(start, end + 1).join('\n') : '';
    return inRange && content.includes(entry.quote) ? [] : [index];
  });
}
