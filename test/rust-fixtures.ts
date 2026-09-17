import { mkdirSync } from 'node:fs';
import path from 'node:path';
import { Database } from 'bun:sqlite';

export interface DecisionRecord {
  [key: string]: unknown;
  document: string;
  id: string;
  lineEnd: number;
  lineStart: number;
  localId: string;
}

export interface RelationshipRecord {
  [key: string]: unknown;
  from: string;
  id: string;
  localId: string;
  to: string;
}

export interface WarningRecord {
  [key: string]: unknown;
  message: string;
}

interface UnitRecord {
  [key: string]: unknown;
  document: string;
  version: string;
}

export interface GraphRecord {
  [key: string]: unknown;
  decisions: DecisionRecord[];
  documents: Record<string, string>;
  relationships: RelationshipRecord[];
  units: Record<string, UnitRecord>;
  version: number;
  warnings: (WarningRecord | string)[];
}

interface WorkRecord {
  [key: string]: unknown;
  id: string;
  kind: string;
  key: string;
  maxCalls: number;
  maxInputBytes: number;
  remaining: string[];
  plannedUnits: string[];
  phase: string;
  calls: number;
  cacheHits: number;
  inputBytes: number;
  totalTokens: number;
  status: string;
  pending: unknown;
  attempts: Record<string, unknown>[];
  snapshot: string;
}

const schema = [
  'CREATE TABLE IF NOT EXISTS graph (id INTEGER PRIMARY KEY CHECK(id=1), data TEXT NOT NULL)',
  'CREATE TABLE IF NOT EXISTS work (id TEXT PRIMARY KEY, kind TEXT NOT NULL, key TEXT NOT NULL, data TEXT NOT NULL)',
  'CREATE TABLE IF NOT EXISTS model_cache (key TEXT PRIMARY KEY, value TEXT NOT NULL)',
  'CREATE INDEX IF NOT EXISTS work_key ON work(kind,key)',
].join(';');

const databasePath = function databasePath(root: string) {
  mkdirSync(path.join(root, '.hivex'), { recursive: true });
  return path.join(root, '.hivex', 'knowledge.sqlite');
};

const openDatabase = function openDatabase(root: string) {
  const database = new Database(databasePath(root));
  database.run(schema);
  return database;
};

const parseJson = function parseJson(text: string): unknown {
  return JSON.parse(text);
};

const invalidGraphShape = 'Fixture graph shape is invalid';
const invalidWorkShape = 'Fixture work shape is invalid';

const isRecord = function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
};

const stringField = function stringField(
  value: Record<string, unknown>,
  name: string
): string | undefined {
  return typeof value[name] === 'string' ? value[name] : undefined;
};

const numberField = function numberField(
  value: Record<string, unknown>,
  name: string
): number | undefined {
  return typeof value[name] === 'number' ? value[name] : undefined;
};

const stringArray = function stringArray(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) {
    return undefined;
  }
  const entries: string[] = [];
  for (const entry of value) {
    if (typeof entry !== 'string') {
      return undefined;
    }
    entries.push(entry);
  }
  return entries;
};

const requiredString = function requiredString(
  value: Record<string, unknown>,
  name: string,
  message: string
): string {
  const result = stringField(value, name);
  if (result === undefined) {
    throw new Error(message);
  }
  return result;
};

const requiredNumber = function requiredNumber(
  value: Record<string, unknown>,
  name: string,
  message: string
): number {
  const result = numberField(value, name);
  if (result === undefined) {
    throw new Error(message);
  }
  return result;
};

const requiredStringArray = function requiredStringArray(
  value: unknown,
  message: string
): string[] {
  const result = stringArray(value);
  if (result === undefined) {
    throw new Error(message);
  }
  return result;
};

const records = function records(value: unknown): Record<string, unknown>[] | undefined {
  if (!Array.isArray(value) || !value.every(isRecord)) {
    return undefined;
  }
  return value;
};

const requiredRecords = function requiredRecords(
  value: unknown,
  message: string
): Record<string, unknown>[] {
  const result = records(value);
  if (result === undefined) {
    throw new Error(message);
  }
  return result;
};

const decisions = function decisions(value: unknown): DecisionRecord[] | undefined {
  const entries = records(value);
  if (entries === undefined) {
    return undefined;
  }
  const result: DecisionRecord[] = [];
  for (const entry of entries) {
    const document = stringField(entry, 'document');
    if (document === undefined) {
      return undefined;
    }
    const id = stringField(entry, 'id');
    if (id === undefined) {
      return undefined;
    }
    const lineEnd = numberField(entry, 'lineEnd');
    if (lineEnd === undefined) {
      return undefined;
    }
    const lineStart = numberField(entry, 'lineStart');
    if (lineStart === undefined) {
      return undefined;
    }
    const localId = stringField(entry, 'localId');
    if (localId === undefined) {
      return undefined;
    }
    result.push({ ...entry, document, id, lineEnd, lineStart, localId });
  }
  return result;
};

const relationships = function relationships(value: unknown): RelationshipRecord[] | undefined {
  const entries = records(value);
  if (entries === undefined) {
    return undefined;
  }
  const result: RelationshipRecord[] = [];
  for (const entry of entries) {
    const from = stringField(entry, 'from');
    const id = stringField(entry, 'id');
    const localId = stringField(entry, 'localId');
    const to = stringField(entry, 'to');
    if (from === undefined || id === undefined || localId === undefined || to === undefined) {
      return undefined;
    }
    result.push({ ...entry, from, id, localId, to });
  }
  return result;
};

const stringMap = function stringMap(value: unknown): Record<string, string> | undefined {
  if (!isRecord(value)) {
    return undefined;
  }
  const entries: [string, string][] = [];
  for (const [key, entry] of Object.entries(value)) {
    if (typeof entry !== 'string') {
      return undefined;
    }
    entries.push([key, entry]);
  }
  return Object.fromEntries(entries);
};

const unitMap = function unitMap(value: unknown): Record<string, UnitRecord> | undefined {
  if (!isRecord(value)) {
    return undefined;
  }
  const entries: [string, UnitRecord][] = [];
  for (const [key, entry] of Object.entries(value)) {
    if (
      !isRecord(entry) ||
      typeof entry.document !== 'string' ||
      typeof entry.version !== 'string'
    ) {
      return undefined;
    }
    entries.push([key, { ...entry, document: entry.document, version: entry.version }]);
  }
  return Object.fromEntries(entries);
};

const warningEntries = function warningEntries(
  value: unknown
): (WarningRecord | string)[] | undefined {
  if (!Array.isArray(value)) {
    return undefined;
  }
  const entries: (WarningRecord | string)[] = [];
  for (const entry of value) {
    if (typeof entry === 'string') {
      entries.push(entry);
    } else if (isRecord(entry) && typeof entry.message === 'string') {
      entries.push({ ...entry, message: entry.message });
    } else {
      return undefined;
    }
  }
  return entries;
};

const graphRecord = function graphRecord(value: unknown): GraphRecord {
  if (!isRecord(value)) {
    throw new Error('Fixture graph is not an object');
  }
  const parsedDecisions = decisions(value.decisions);
  if (parsedDecisions === undefined) {
    throw new Error(invalidGraphShape);
  }
  const parsedDocuments = stringMap(value.documents);
  if (parsedDocuments === undefined) {
    throw new Error(invalidGraphShape);
  }
  const parsedRelationships = relationships(value.relationships);
  if (parsedRelationships === undefined) {
    throw new Error(invalidGraphShape);
  }
  const parsedUnits = unitMap(value.units);
  if (parsedUnits === undefined) {
    throw new Error(invalidGraphShape);
  }
  const version = requiredNumber(value, 'version', invalidGraphShape);
  const parsedWarnings = warningEntries(value.warnings);
  if (parsedWarnings === undefined) {
    throw new Error(invalidGraphShape);
  }
  return {
    ...value,
    decisions: parsedDecisions,
    documents: parsedDocuments,
    relationships: parsedRelationships,
    units: parsedUnits,
    version,
    warnings: parsedWarnings,
  };
};

export const parseGraph = function parseGraph(value: unknown): GraphRecord {
  return graphRecord(value);
};

const workRecord = function workRecord(value: unknown): WorkRecord {
  if (!isRecord(value)) {
    throw new Error('Fixture work is not an object');
  }
  const attempts = requiredRecords(value.attempts, invalidWorkShape);
  const remaining = requiredStringArray(value.remaining, invalidWorkShape);
  const plannedUnits = requiredStringArray(value.plannedUnits, invalidWorkShape);
  const id = requiredString(value, 'id', invalidWorkShape);
  const kind = requiredString(value, 'kind', invalidWorkShape);
  const key = requiredString(value, 'key', invalidWorkShape);
  const phase = requiredString(value, 'phase', invalidWorkShape);
  const snapshot = requiredString(value, 'snapshot', invalidWorkShape);
  const status = requiredString(value, 'status', invalidWorkShape);
  const maxCalls = requiredNumber(value, 'maxCalls', invalidWorkShape);
  const maxInputBytes = requiredNumber(value, 'maxInputBytes', invalidWorkShape);
  const calls = requiredNumber(value, 'calls', invalidWorkShape);
  const cacheHits = requiredNumber(value, 'cacheHits', invalidWorkShape);
  const inputBytes = requiredNumber(value, 'inputBytes', invalidWorkShape);
  const totalTokens = requiredNumber(value, 'totalTokens', invalidWorkShape);
  return {
    ...value,
    attempts,
    cacheHits,
    calls,
    id,
    inputBytes,
    key,
    kind,
    materializedChecks: value.materializedChecks,
    maxCalls,
    maxInputBytes,
    pending: value.pending,
    phase,
    plannedUnits,
    remaining,
    snapshot,
    status,
    totalTokens,
  };
};

export const emptyGraph = function emptyGraph(): GraphRecord {
  return {
    decisions: [],
    documents: {},
    relationships: [],
    units: {},
    version: 1,
    warnings: [],
  };
};

export const readGraph = function readGraph(root: string): GraphRecord {
  using database = new Database(databasePath(root), { readonly: true });
  const row = database.query<{ data: string }, []>('SELECT data FROM graph WHERE id=1').get();
  return row === null ? emptyGraph() : graphRecord(parseJson(row.data));
};

export const writeGraph = function writeGraph(root: string, graph: GraphRecord) {
  using database = openDatabase(root);
  database.run('INSERT INTO graph VALUES(1,?) ON CONFLICT(id) DO UPDATE SET data=excluded.data', [
    JSON.stringify(graph),
  ]);
};

export const readWork = function readWork(root: string, id: string): WorkRecord {
  using database = new Database(databasePath(root), { readonly: true });
  const row = database
    .query<{ data: string }, [string]>('SELECT data FROM work WHERE id=?')
    .get(id);
  if (row === null) {
    throw new Error(`Missing fixture work ${id}`);
  }
  return workRecord(parseJson(row.data));
};

export const readLatestWork = function readLatestWork(root: string): WorkRecord {
  using database = new Database(databasePath(root), { readonly: true });
  const row = database
    .query<{ data: string }, []>('SELECT data FROM work ORDER BY rowid DESC LIMIT 1')
    .get();
  if (row === null) {
    throw new Error('Missing fixture work');
  }
  return workRecord(parseJson(row.data));
};

export const writeWork = function writeWork(
  root: string,
  id: string,
  work: Record<string, unknown>
) {
  const kind = stringField(work, 'kind');
  const key = stringField(work, 'key');
  if (kind === undefined || key === undefined) {
    throw new Error('Fixture work identity is invalid');
  }
  using database = openDatabase(root);
  database.run(
    'INSERT INTO work VALUES(?,?,?,?) ON CONFLICT(id) DO UPDATE SET data=excluded.data',
    [id, kind, key, JSON.stringify(work)]
  );
};

export const writeCache = function writeCache(root: string, key: string, value: unknown) {
  using database = openDatabase(root);
  database.run(
    'INSERT INTO model_cache VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value',
    [key, JSON.stringify(value)]
  );
};

export interface SeedWorkOptions {
  id?: string;
  key: string;
  kind: 'ask' | 'review' | 'update';
  maxCalls?: number;
  maxInputBytes?: number;
  remaining?: string[];
  resultKey?: string;
  snapshot: string;
}

export const seedWork = function seedWork(root: string, options: SeedWorkOptions) {
  const id = options.id ?? `fixture-${options.kind}-${options.key}`;
  const remaining = options.remaining ?? [];
  const work: WorkRecord = {
    attempts: [],
    cacheHits: 0,
    calls: 0,
    id,
    inputBytes: 0,
    key: options.key,
    kind: options.kind,
    materializedChecks: true,
    maxCalls: options.maxCalls ?? (options.kind === 'update' ? 2 : 3),
    maxInputBytes: options.maxInputBytes ?? 131_072,
    pending: null,
    phase: options.kind,
    plannedUnits: [...remaining],
    remaining: [...remaining],
    snapshot: options.snapshot,
    status: 'pending',
    totalTokens: 0,
  };
  if (options.resultKey !== undefined) {
    work.resultKey = options.resultKey;
  }
  writeWork(root, id, work);
  return id;
};

export const updateWork = function updateWork(
  root: string,
  id: string,
  mutate: (work: WorkRecord) => void
) {
  const work = readWork(root, id);
  mutate(work);
  writeWork(root, id, work);
  return work;
};
