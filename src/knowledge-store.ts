import { Database } from 'bun:sqlite';
import {
  closeSync,
  lstatSync,
  mkdirSync,
  openSync,
  readFileSync,
  unlinkSync,
  writeFileSync,
} from 'node:fs';
import { join } from 'node:path';
import { randomUUID } from 'node:crypto';
import { z } from 'zod';
import { HivexError } from './errors.ts';
import { emptyGraph, extractionSchema, graphSchema, type Graph } from './knowledge-model.ts';

const attemptSchema = z.object({
  stage: z.string(),
  inputHash: z.string(),
  inputBytes: z.number(),
  report: z.unknown().optional(),
  outputHash: z.string().optional(),
  diagnostic: z.string().optional(),
  error: z.string().optional(),
  result: z.unknown().optional(),
});
const workSchema = z.object({
  id: z.string(),
  kind: z.enum(['update', 'ask']),
  key: z.string(),
  snapshot: z.string(),
  calls: z.number().int().nonnegative(),
  maxCalls: z.number().int().nonnegative(),
  inputBytes: z.number().int().nonnegative(),
  maxInputBytes: z.number().int().positive(),
  totalTokens: z.number().int().nonnegative(),
  status: z.enum(['pending', 'running', 'budget-exhausted', 'failed', 'done']),
  remaining: z.array(z.string()),
  pending: z
    .object({
      batch: z.string(),
      documents: z.array(z.string()),
      context: z.array(z.string()).default([]),
      existing: z.array(z.string()).default([]),
      extraction: extractionSchema,
    })
    .nullable(),
  attempts: z.array(attemptSchema).max(64),
  result: z.unknown().optional(),
});
export type Work = z.infer<typeof workSchema>;

export class KnowledgeStore implements Disposable {
  private readonly db: Database;
  private readonly directory: string;

  constructor(root: string, options: { readonly?: boolean } = {}) {
    const directory = join(root, '.hivex');
    this.directory = directory;
    const path = join(directory, 'knowledge.sqlite');
    for (const candidate of [directory, path]) {
      if (lstatSync(candidate, { throwIfNoEntry: false })?.isSymbolicLink())
        throw new HivexError({
          code: 'INVALID_STORE',
          message: 'Knowledge storage cannot be a symlink',
        });
    }
    if (!options.readonly) mkdirSync(directory, { recursive: true, mode: 0o700 });
    this.db = options.readonly ? new Database(path, { readonly: true }) : new Database(path);
    if (options.readonly) return;
    this.db.run('PRAGMA busy_timeout=1000');
    this.db.run('PRAGMA max_page_count=16384');
    this.db.run(
      'CREATE TABLE IF NOT EXISTS graph (id INTEGER PRIMARY KEY CHECK(id=1), data TEXT NOT NULL)',
    );
    this.db.run(
      'CREATE TABLE IF NOT EXISTS work (id TEXT PRIMARY KEY, kind TEXT NOT NULL, key TEXT NOT NULL, data TEXT NOT NULL)',
    );
    this.db.run('CREATE INDEX IF NOT EXISTS work_key ON work(kind,key)');
  }

  updateLease(): Disposable {
    const path = join(this.directory, 'knowledge.lock');
    const token = JSON.stringify({ pid: process.pid, id: randomUUID() });
    let fd: number;
    try {
      fd = openSync(path, 'wx', 0o600);
    } catch {
      throw new HivexError({
        code: 'KNOWLEDGE_LOCKED',
        message:
          'Cannot acquire the update lock; inspect any active or interrupted update before continuing',
      });
    }
    writeFileSync(fd, token);
    return {
      [Symbol.dispose]() {
        closeSync(fd);
        try {
          if (readFileSync(path, 'utf8') === token) unlinkSync(path);
        } catch (error) {
          if (!(error instanceof Error && 'code' in error && error.code === 'ENOENT')) throw error;
        }
      },
    };
  }

  graph(): Graph {
    const row = this.db.query<{ data: string }, []>('SELECT data FROM graph WHERE id=1').get();
    return row ? graphSchema.parse(JSON.parse(row.data)) : emptyGraph();
  }

  saveGraph(graph: Graph) {
    this.db.run('INSERT INTO graph VALUES(1,?) ON CONFLICT(id) DO UPDATE SET data=excluded.data', [
      JSON.stringify(graph),
    ]);
  }

  begin(options: {
    kind: Work['kind'];
    key: string;
    snapshot: string;
    maxCalls?: number;
    maxInputBytes?: number;
    remaining: string[];
  }): Work {
    return this.db
      .transaction(() => {
        const row = this.db
          .query<
            { data: string },
            [string, string]
          >('SELECT data FROM work WHERE kind=? AND key=? ORDER BY rowid DESC LIMIT 1')
          .get(options.kind, options.key);
        const previous = row ? workSchema.parse(JSON.parse(row.data)) : null;
        if (previous && (previous.status !== 'done' || options.remaining.length === 0)) {
          const work = previous;
          if (work.status === 'done') return work;
          if (work.status === 'running')
            throw new HivexError({
              code: 'WORK_RUNNING',
              message: `Work ${work.id} has an unfinished invocation; inspect it before retrying`,
            });
          if (options.maxCalls !== undefined) work.maxCalls = options.maxCalls;
          if (options.maxInputBytes !== undefined) work.maxInputBytes = options.maxInputBytes;
          this.save(work);
          return work;
        }
        const work: Work = {
          id: randomUUID(),
          ...options,
          maxCalls: options.maxCalls ?? 2,
          maxInputBytes: options.maxInputBytes ?? 131072,
          calls: 0,
          inputBytes: 0,
          totalTokens: 0,
          status: 'pending',
          pending: null,
          attempts: [],
        };
        this.save(work);
        return work;
      })
      .immediate();
  }

  save(work: Work) {
    this.db.run(
      'INSERT INTO work VALUES(?,?,?,?) ON CONFLICT(id) DO UPDATE SET data=excluded.data',
      [work.id, work.kind, work.key, JSON.stringify(work)],
    );
  }

  commit(work: Work, graph: Graph) {
    this.db.transaction(() => {
      this.saveGraph(graph);
      this.save(work);
    })();
  }

  reserve(work: Work, stage: string, inputHash: string, inputBytes: number) {
    this.db.transaction(() => {
      const stored = this.db
        .query<{ data: string }, [string]>('SELECT data FROM work WHERE id=?')
        .get(work.id);
      const current = stored && workSchema.parse(JSON.parse(stored.data));
      if (!current || current.calls !== work.calls || current.status === 'running')
        throw new HivexError({
          code: 'WORK_CONFLICT',
          message: 'Work was claimed or changed by another operation',
        });
      work.calls += 1;
      work.inputBytes += inputBytes;
      work.status = 'running';
      work.attempts.push({ stage, inputHash, inputBytes });
      this.save(work);
    })();
  }

  [Symbol.dispose]() {
    this.db.close();
  }
}
