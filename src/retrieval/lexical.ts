import { Database } from 'bun:sqlite';
import { z } from 'zod';
import { HivexError } from '../errors.ts';

const row = z.object({ id: z.string(), score: z.number() });
export const searchTerms = (text: string) => {
  const terms = text
    .toLowerCase()
    .normalize('NFKC')
    .match(/[\p{L}\p{N}]+/gu);
  if (terms === null) {
    return [];
  }
  return [...new Set(terms)];
};
interface Record {
  id: string;
  title: string;
  content: string;
}
interface RankOptions {
  database: Database;
  excludedId?: string | null;
  limit: number;
  terms: string[];
}

const rank = function rank({ database, excludedId = null, limit, terms }: RankOptions) {
  const expression = terms.map((term) => `"${term.replaceAll('"', '""')}"`).join(' OR ');
  if (!expression) {
    return [];
  }
  return database
    .prepare(
      'SELECT id, bm25(sources) AS score FROM sources WHERE sources MATCH ? AND (? IS NULL OR id != ?) ORDER BY score, id LIMIT ?'
    )
    .all(expression, excludedId, excludedId, limit)
    .map((value) => row.parse(value));
};

const prepareVocabulary = function prepareVocabulary(database: Database) {
  database.run("CREATE VIRTUAL TABLE vocabulary USING fts5vocab(sources, 'row')");
  database.run("CREATE VIRTUAL TABLE instances USING fts5vocab(sources, 'instance')");
  database.run('CREATE TABLE document_terms AS SELECT DISTINCT doc, term FROM instances');
  database.run('CREATE INDEX document_terms_by_doc ON document_terms(doc)');
};

export class LexicalIndex {
  private readonly db = new Database(':memory:');
  private readonly ids = new Map<string, number>();
  private vocabularyReady = false;

  constructor(records: readonly Record[]) {
    try {
      this.db.run('PRAGMA page_size=4096');
      this.db.run('PRAGMA max_page_count=32768');
      this.db.run(
        'CREATE VIRTUAL TABLE sources USING fts5(id UNINDEXED, title, content, tokenize=unicode61)'
      );
      const insert = this.db.prepare(
        'INSERT INTO sources(rowid, id, title, content) VALUES (?, ?, ?, ?)'
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
    return rank({ database: this.db, limit, terms: searchTerms(query) });
  }

  neighbors(id: string, limit: number) {
    const documentId = this.ids.get(id);
    if (documentId === undefined) {
      throw new HivexError({
        code: 'LEXICAL_SOURCE_NOT_FOUND',
        message: 'The source is not in this lexical index',
      });
    }
    if (!this.vocabularyReady) {
      prepareVocabulary(this.db);
      this.vocabularyReady = true;
    }
    const terms = this.db
      .query<{ term: string }, [number]>(
        'SELECT t.term FROM document_terms t JOIN vocabulary v ON v.term=t.term WHERE t.doc=? AND v.doc>=2 ORDER BY v.doc, t.term LIMIT 32'
      )
      .all(documentId)
      .map((entry) => entry.term);
    return {
      matches: rank({ database: this.db, excludedId: id, limit, terms }),
      terms,
    };
  }

  [Symbol.dispose]() {
    this.db.close();
  }
}

export const rankLexically = function rankLexically(
  records: readonly Record[],
  query: string,
  limit = 32
) {
  using index = new LexicalIndex(records);
  return index.search(query, limit);
};
