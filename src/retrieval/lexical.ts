import { Database } from 'bun:sqlite';
import { z } from 'zod';

const row = z.object({ id: z.string(), score: z.number() });
export const searchTerms = (text: string) => [
  ...new Set(
    text
      .toLowerCase()
      .normalize('NFKC')
      .match(/[\p{L}\p{N}]+/gu) ?? [],
  ),
];

export function rankLexically(
  records: readonly { id: string; title: string; content: string }[],
  query: string,
  limit = 32,
) {
  using db = new Database(':memory:');
  db.run(
    'CREATE VIRTUAL TABLE sources USING fts5(id UNINDEXED, title, content, tokenize=unicode61)',
  );
  const insert = db.prepare('INSERT INTO sources VALUES (?, ?, ?)');
  db.transaction(() => {
    for (const record of records) insert.run(record.id, record.title, record.content);
  })();
  const expression = searchTerms(query)
    .map((term) => `"${term}"`)
    .join(' OR ');
  if (!expression) return [];
  return db
    .prepare(
      'SELECT id, bm25(sources) AS score FROM sources WHERE sources MATCH ? ORDER BY score, id LIMIT ?',
    )
    .all(expression, limit)
    .map((value) => row.parse(value));
}
