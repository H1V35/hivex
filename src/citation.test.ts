import { expect, test } from 'bun:test';
import { invalidCitationIndexes } from './sources/citation.ts';
import { parseSource } from './sources/markdown.ts';
import { selectSection } from './sources/sections.ts';

test.each([
  ['Keep records\nfor 30 days.', 'Keep records for 30 days.'],
  ['Keep records \r\n  for 30 days.', 'Keep records for 30 days.'],
  ['- Keep records\n  for 30 days.', 'Keep records for 30 days.'],
  ['- Keep records\n  for 30 days.', '- Keep records for 30 days.'],
  ['Keep **approved** records\nfor 30 days.', 'Keep **approved** records for 30 days.'],
  ['Policy uses `daily` and\n  a fixed window.', 'Policy uses `daily` and a fixed window.'],
  ['- Policy uses `daily` and\n  a fixed window.', '- Policy uses `daily` and a fixed window.'],
  ['Keep records\nfor 30 days\nunless exempt.', 'Keep records for 30 days\nunless exempt.'],
  ['- Keep records\n  for 30 days.', 'Keep records\nfor 30 days.'],
  ['Keep records for 30 days.', 'Keep records\nfor 30 days.'],
  ['Keep records \r\n  for 30 days.', 'Keep records\r\nfor 30 days.'],
  ['Opening\n  text. The token is `one two`; plain one two follows.', 'one\ntwo'],
])('matches a joined soft wrap in one paragraph: %j', (content, quote) => {
  const source = { content, section: null };
  const entries = [{ quote, lineStart: 1, lineEnd: content.split('\n').length }];
  expect(invalidCitationIndexes(source, entries)).toEqual([]);
  expect(source.content).toBe(content);
  expect(entries[0]?.quote).toBe(quote);
});

test.each([
  ['Keep records, unless exempt,\nfor 30 days.', 'Keep records for 30 days.'],
  ['Do not delete records\nfor 30 days.', 'Do delete records for 30 days.'],
  ['Keep records\nfor 30 days.', 'Keep records for 3 days.'],
  ['Keep  records\nfor 30 days.', 'Keep records for 30 days.'],
  ['Keep **approved** records\nfor 30 days.', 'Keep approved records for 30 days.'],
  ['Keep records\n\nfor 30 days.', 'Keep records for 30 days.'],
  ['- Keep records\n- for 30 days.', 'Keep records for 30 days.'],
  ['    Keep records\n    for 30 days.', 'Keep records for 30 days.'],
  ['```text\nKeep records\nfor 30 days.\n```', 'Keep records for 30 days.'],
  ['Keep `records\nfor 30 days`.', 'Keep `records for 30 days`.'],
  [
    'Policy uses `daily\n  window` and\n  fixed limits.',
    'Policy uses `daily window` and fixed limits.',
  ],
  ['- Policy uses `daily`\n- A fixed window.', '- Policy uses `daily` - A fixed window.'],
  ['Keep records  \nfor 30 days.', 'Keep records for 30 days.'],
  ['Keep records\\\nfor 30 days.', 'Keep records for 30 days.'],
  ['<div>Keep records\nfor 30 days.</div>', '<div>Keep records for 30 days.</div>'],
  ['Keep <span>records\nfor 30 days.</span>', 'Keep <span>records for 30 days.</span>'],
  ['| Rule |\n| --- |\n| Keep records |\n| for 30 days. |', '| Keep records | | for 30 days. |'],
  ['---\nrule: Keep records\n  for 30 days.\n---', 'rule: Keep records for 30 days.'],
  ['[records](https://example.invalid\n "policy")', '[records](https://example.invalid "policy")'],
  ['> Keep records\n> for 30 days.', 'Keep records > for 30 days.'],
  ['The token is `one two`.', 'one\ntwo'],
  ['Opening\n  text. The token is `one two`.', 'one\ntwo'],
  ['Keep records  for 30 days.', 'Keep records\n\nfor 30 days.'],
  ['Keep records for 30 days.', 'Keep records  \nfor 30 days.'],
  ['Keep records\\ for 30 days.', 'Keep records\\\nfor 30 days.'],
  ['Use - policy - exception.', 'Use\n- policy\n- exception.'],
  ['- Keep records - unless exempt.', '- Keep records\n- unless exempt.'],
  ['--- rule: Keep records ---', '---\nrule: Keep records\n---'],
  ['| Rule | | --- | | Keep records |', '| Rule |\n| --- |\n| Keep records |'],
])('does not normalize protected syntax or changed evidence: %j', (content, quote) => {
  const source = { content, section: null };
  const lineEnd = content.split('\n').length;
  expect(invalidCitationIndexes(source, [{ quote, lineStart: 1, lineEnd }])).toEqual([0]);
  expect(invalidCitationIndexes(source, [{ quote: content, lineStart: 1, lineEnd }])).toEqual([]);
});

test('keeps original section ranges and rejects ranges outside a single paragraph', () => {
  const source = selectSection(
    parseSource({
      path: 'policy.md',
      collection: null,
      content:
        '# Policy\n\nIntroduction.\n\n## Retention\n\nKeep records\nfor 30 days.\n\nAnother paragraph.\n',
    }),
    {
      anchor: 'retention',
      collection: {
        id: 'policy',
        kind: 'documentation',
        include: ['*.md'],
        exclude: [],
        default: true,
      },
    },
  );
  const quote = 'Keep records for 30 days.';
  expect(
    invalidCitationIndexes(source, [
      { quote, lineStart: 7, lineEnd: 8 },
      { quote, lineStart: 7, lineEnd: 7 },
      { quote, lineStart: 6, lineEnd: 8 },
      { quote, lineStart: 7, lineEnd: 10 },
      { quote, lineStart: 3, lineEnd: 4 },
      { quote, lineStart: 8, lineEnd: 7 },
      { quote, lineStart: 7, lineEnd: 99 },
    ]),
  ).toEqual([1, 2, 3, 4, 5, 6]);
});

test('keeps code citations literal even when the code resembles a Markdown paragraph', () => {
  const source = { content: 'const ttl =\n  30;', section: null };
  expect(
    invalidCitationIndexes(
      source,
      [
        { quote: 'const ttl = 30;', lineStart: 1, lineEnd: 2 },
        { quote: source.content, lineStart: 1, lineEnd: 2 },
      ],
      'code',
    ),
  ).toEqual([0]);
});
