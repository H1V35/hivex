import { expect, test } from 'bun:test';
import { applyExtraction, emptyGraph, extractionSchema } from './knowledge-model.ts';
import { compareSerializedStrings } from './ordering.ts';
import type { Document } from './documents.ts';

// Captured from release source b6d93b1 before the lint migration.
const source: Document = {
  hash: 'c6faa807ee055873a73b0e8b823511be81bfaa4b69bc3e141cafc636cfc31c2a',
  historical: false,
  id: 'docs/cache.md',
  links: [],
  path: 'docs/cache.md',
  status: null,
  text: '# Private cache\n\nRemove cached private data when access is revoked.\n',
  title: 'Private cache',
};

const extraction = extractionSchema.parse({
  decisions: [
    {
      conditions: [],
      document: 'docs/cache.md',
      exceptions: [],
      id: 'c1',
      kind: 'constraint',
      lineEnd: 3,
      lineStart: 3,
      reason: 'Revoked access must not retain data.',
      status: 'current',
      text: 'Remove cached private data when access is revoked.',
    },
    {
      conditions: [],
      document: 'docs/cache.md',
      exceptions: [],
      id: 'c2',
      kind: 'decision',
      lineEnd: 3,
      lineStart: 3,
      reason: 'Keep cache behavior aligned with access.',
      status: 'current',
      text: 'Remove cached private data when access is revoked.',
    },
  ],
  relationships: [
    {
      evidence: [
        {
          document: 'docs/cache.md',
          lineEnd: 3,
          lineStart: 3,
        },
      ],
      from: 'c1',
      id: 'r1',
      reason: 'Both refer to revoking cached data.',
      to: 'c2',
      type: 'supports',
    },
  ],
  uncertainties: [],
});

test('retains existing decision and relationship identities for unchanged extraction', () => {
  const graph = applyExtraction({
    batch: 'fixture',
    documents: [source],
    extraction,
    graph: emptyGraph(),
  });
  expect(graph.decisions.map((entry) => entry.id)).toEqual([
    'fda25af550b379877147825c3e375f5aa6ad5398fc4664dc8d062bc115a18afc',
    'dc94835ead5426394740d8d63fe98c5ce70e6522fe44e6adaf8e0a94fa2c3c31',
  ]);
  expect(graph.relationships.map((entry) => entry.id)).toEqual([
    '8df360ba09823f9f5ef282ab21b4634d3d15f7e9b1212b0f986eeb6a80fe2c74',
  ]);
});

test('retains serialized UTF-16 order for supplementary Unicode document names', () => {
  const documents = ['docs/\u{E000}.md', 'docs/\u{10000}.md'];
  expect(documents.toSorted(compareSerializedStrings)).toEqual([
    'docs/\u{10000}.md',
    'docs/\u{E000}.md',
  ]);
});
