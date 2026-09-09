import { describe, expect, test } from 'bun:test';
import type { Document } from './documents.ts';
import { digest } from './knowledge-model.ts';
import { ingestionUnits } from './ingestion-units.ts';

function documentOf(text: string, path = 'notes.md'): Document {
  return { id: path, path, title: path, text, hash: digest(text), status: null, links: [] };
}

describe('ingestionUnits', () => {
  test('preserves a small document as exact raw text', () => {
    const text = 'First line\nSecond line\n';
    const [unit] = ingestionUnits([documentOf(text)]).units;

    expect(unit).toEqual({
      id: 'notes.md:1-2',
      document: 'notes.md',
      hash: digest(text),
      lineStart: 1,
      lineEnd: 2,
      text,
    });
  });

  test('retains a large Markdown source in deterministic bounded units', () => {
    const paragraphs = Array.from({ length: 40 }, (_, index) =>
      [`## Section ${index}`, '', `Paragraph ${index}: ${'word '.repeat(240)}`, ''].join('\n'),
    );
    const text = `${paragraphs.join('\n')}Final line`;
    const document = documentOf(text, 'large.md');
    const first = ingestionUnits([document]);
    const second = ingestionUnits([document]);

    expect(first.warnings).toEqual([]);
    expect(first.units.length).toBeGreaterThan(1);
    expect(first.units).toEqual(second.units);
    expect(first.units.map((unit) => unit.text).join('')).toBe(text);
    expect(first.units.every((unit) => Buffer.byteLength(unit.text, 'utf8') <= 8192)).toBe(true);
    expect(first.units.every((unit) => unit.hash === digest(unit.text))).toBe(true);
    expect(first.units.map(({ id }) => id)).toEqual(
      first.units.map((unit) => `${unit.document}:${unit.lineStart}-${unit.lineEnd}`),
    );
  });

  test('distinguishes duplicate raw content by absolute range', () => {
    const line = 'duplicate '.repeat(500);
    const result = ingestionUnits([documentOf(`${line}\n${line}\n`, 'duplicates.md')]);
    const [first, second] = result.units;

    expect(result.units).toHaveLength(2);
    if (!first || !second) throw new Error('Expected two duplicate-content units');
    expect(first.text).toBe(second.text);
    expect(first.hash).toBe(second.hash);
    expect(first.id).not.toBe(second.id);
  });

  test('enforces the limit using raw UTF-8 bytes', () => {
    const line = '😀'.repeat(2000);
    const text = `${line}\n${line}\n`;
    const result = ingestionUnits([documentOf(text, 'unicode.md')]);

    expect(result.warnings).toEqual([]);
    expect(result.units).toHaveLength(2);
    expect(result.units.map((unit) => unit.text).join('')).toBe(text);
    expect(result.units.every((unit) => Buffer.byteLength(unit.text, 'utf8') <= 8192)).toBe(true);
  });

  test('warns and omits an oversized line while retaining later lines', () => {
    const text = `before\n${'x'.repeat(8193)}\nafter\n`;
    const result = ingestionUnits([documentOf(text, 'oversized.md')]);
    const warning = result.warnings[0];

    expect(result.units.map((unit) => unit.text).join('')).toBe('before\nafter\n');
    expect(result.units.map(({ lineStart, lineEnd }) => [lineStart, lineEnd])).toEqual([
      [1, 1],
      [3, 3],
    ]);
    expect(result.warnings).toHaveLength(1);
    expect(warning?.path).toBe('oversized.md');
    expect(warning?.message).toContain('Line 2');
    expect(warning?.message).toContain('8192');
    expect(warning?.message).toContain('omitted');
  });

  test('keeps paragraphs and fences intact when a boundary can fit', () => {
    const paragraph = (letter: string) =>
      Array.from({ length: 18 }, (_, index) => `${letter}${index}${letter.repeat(228)}`).join('\n');
    const paragraphA = paragraph('A');
    const paragraphB = paragraph('B');
    const fence = [
      '```ts',
      ...Array.from({ length: 120 }, (_, index) =>
        index === 50
          ? '~~~\n\n# Still inside the backtick fence'
          : `const value${index} = ${'x'.repeat(25)};`,
      ),
      '```',
    ].join('\n');
    const text = `# First\n\n${paragraphA}\n\n${fence}\n\n# Second\n\n${paragraphB}\n`;
    const result = ingestionUnits([documentOf(text, 'boundaries.md')]);

    expect(result.warnings).toEqual([]);
    expect(result.units.map((unit) => unit.text).join('')).toBe(text);
    expect(result.units.some((unit) => unit.text.includes(paragraphA))).toBe(true);
    expect(result.units.some((unit) => unit.text.includes(paragraphB))).toBe(true);
    expect(result.units.some((unit) => unit.text.includes(fence))).toBe(true);
  });
});
