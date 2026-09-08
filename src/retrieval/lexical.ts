import { Database } from 'bun:sqlite';
import { z } from 'zod';
import { HivexError } from '../errors.ts';

const row = z.object({ id: z.string(), score: z.number() });
export const searchTerms = (text: string) => [
  ...new Set(
    text
      .toLowerCase()
      .normalize('NFKC')
      .match(/[\p{L}\p{N}]+/gu) ?? [],
  ),
];
type Record = { id: string; title: string; content: string };

export class LexicalIndex {
  private readonly db = new Database(':memory:');
  private readonly ids = new Map<string, number>();
  private vocabularyReady = false;

  constructor(records: readonly Record[]) {
    try {
      this.db.run('PRAGMA page_size=4096');
      this.db.run('PRAGMA max_page_count=32768');
      this.db.run(
        'CREATE VIRTUAL TABLE sources USING fts5(id UNINDEXED, title, content, tokenize=unicode61)',
      );
      const insert = this.db.prepare(
        'INSERT INTO sources(rowid, id, title, content) VALUES (?, ?, ?, ?)',
      );
      this.db.transaction(() => {
        for (const [index, record] of records.entries()) {
          insert.run(index + 1, record.id, record.title, record.content);
          this.ids.set(record.id, index + 1);
        }
      })();
    } catch (error) {
      this.db.close();
      throw error;
    }
  }

  search(query: string, limit = 32) {
    return this.rank(searchTerms(query), limit);
  }

  private rank(terms: string[], limit: number, excludedId: string | null = null) {
    const expression = terms.map((term) => `"${term.replaceAll('"', '""')}"`).join(' OR ');
    if (!expression) return [];
    return this.db
      .prepare(
        'SELECT id, bm25(sources) AS score FROM sources WHERE sources MATCH ? AND (? IS NULL OR id != ?) ORDER BY score, id LIMIT ?',
      )
      .all(expression, excludedId, excludedId, limit)
      .map((value) => row.parse(value));
  }

  private prepareVocabulary() {
    if (this.vocabularyReady) return;
    this.db.run("CREATE VIRTUAL TABLE vocabulary USING fts5vocab(sources, 'row')");
    this.db.run("CREATE VIRTUAL TABLE instances USING fts5vocab(sources, 'instance')");
    this.db.run('CREATE TABLE document_terms AS SELECT DISTINCT doc, term FROM instances');
    this.db.run('CREATE INDEX document_terms_by_doc ON document_terms(doc)');
    this.vocabularyReady = true;
  }

  neighbors(id: string, limit: number) {
    const doc = this.ids.get(id);
    if (doc === undefined)
      throw new HivexError({
        code: 'LEXICAL_SOURCE_NOT_FOUND',
        message: 'The source is not in this lexical index',
      });
    this.prepareVocabulary();
    const terms = this.db
      .query<{ term: string }, [number]>(
        'SELECT t.term FROM document_terms t JOIN vocabulary v ON v.term=t.term WHERE t.doc=? AND v.doc>=2 ORDER BY v.doc, t.term LIMIT 32',
      )
      .all(doc)
      .map((row) => row.term);
    return { terms, matches: this.rank(terms, limit, id) };
  }

  [Symbol.dispose]() {
    this.db.close();
  }
}

export function rankLexically(records: readonly Record[], query: string, limit = 32) {
  using index = new LexicalIndex(records);
  return index.search(query, limit);
}
