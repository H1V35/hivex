import { spawnSync } from 'node:child_process';
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import nodePath from 'node:path';
import { pathToFileURL } from 'node:url';
import { expect, test } from 'bun:test';
import { captureImplementation } from './implementation.ts';
import { KnowledgeStore } from './knowledge-store.ts';

type JsonRecord = Record<string, unknown>;

const git = Bun.which('git');
if (git === null) {
  throw new Error('Git is required by the review fixtures');
}

interface ModelDecision extends JsonRecord {
  lineEnd: number;
  lineStart: number;
  text: string;
}

interface ModelRelationship extends JsonRecord {
  evidence: unknown[];
  to: string;
}

interface ModelExtraction extends JsonRecord {
  decisions: ModelDecision[];
  relationships: ModelRelationship[];
  uncertainties: string[];
}

interface ModelCheck extends JsonRecord {
  findings: JsonRecord[];
}

interface ModelAnswer extends JsonRecord {
  answer: string;
  evidence: unknown[];
  uncertainties: string[];
}
interface ModelDocumentResponse {
  decisions: ModelDecision[];
  relationships: ModelRelationship[];
}
interface ModelReview extends JsonRecord {
  findings: JsonRecord[];
  uncertainties: string[];
}

interface ModelResponses extends JsonRecord {
  ask: ModelAnswer;
  byDocument?: Record<string, ModelDocumentResponse>;
  check: ModelCheck;
  extract: ModelExtraction;
  review?: ModelReview;
}

interface ModelPacket extends JsonRecord {
  documents: JsonRecord[];
  operation: string;
  targets?: string[];
}

interface SnapshotGraph extends JsonRecord {
  decisions: JsonRecord[];
  relationships: JsonRecord[];
}

const isRecord = (value: unknown): value is JsonRecord =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const isStringArray = (value: unknown): value is string[] =>
  Array.isArray(value) && value.every((entry) => typeof entry === 'string');

const isRecordArray = (value: unknown): value is JsonRecord[] =>
  Array.isArray(value) && value.every(isRecord);

const isDecision = function isDecision(value: unknown): value is ModelDecision {
  return (
    isRecord(value) &&
    Number.isSafeInteger(value.lineEnd) &&
    Number.isSafeInteger(value.lineStart) &&
    typeof value.text === 'string'
  );
};

const isRelationship = function isRelationship(value: unknown): value is ModelRelationship {
  return isRecord(value) && Array.isArray(value.evidence) && typeof value.to === 'string';
};

const isAnswer = function isAnswer(value: unknown): value is ModelAnswer {
  return (
    isRecord(value) &&
    typeof value.answer === 'string' &&
    Array.isArray(value.evidence) &&
    isStringArray(value.uncertainties)
  );
};

const isModelResponses = function isModelResponses(value: unknown): value is ModelResponses {
  if (!isRecord(value) || !isAnswer(value.ask)) {
    return false;
  }
  if (!isRecord(value.check) || !isRecordArray(value.check.findings)) {
    return false;
  }
  if (!isRecord(value.extract)) {
    return false;
  }
  if (!isStringArray(value.extract.uncertainties)) {
    return false;
  }
  if (!Array.isArray(value.extract.decisions)) {
    return false;
  }
  if (!value.extract.decisions.every(isDecision)) {
    return false;
  }
  return (
    Array.isArray(value.extract.relationships) && value.extract.relationships.every(isRelationship)
  );
};

const isDocumentResponse = function isDocumentResponse(
  value: unknown
): value is ModelDocumentResponse {
  if (!isRecord(value) || !Array.isArray(value.decisions)) {
    return false;
  }
  if (!value.decisions.every(isDecision)) {
    return false;
  }
  return Array.isArray(value.relationships) && value.relationships.every(isRelationship);
};

const isDocumentResponseMap = (value: unknown): value is Record<string, ModelDocumentResponse> =>
  isRecord(value) && Object.values(value).every(isDocumentResponse);

const isModelReview = function isModelReview(value: unknown): value is ModelReview {
  return isRecord(value) && isStringArray(value.uncertainties) && isRecordArray(value.findings);
};

const parseJson = (text: string): unknown => JSON.parse(text);

const parseModelResponses = (text: string): ModelResponses => {
  const value = parseJson(text);
  if (!isModelResponses(value)) {
    throw new Error('Expected the model fixture response shape');
  }
  return value;
};

interface CliError extends JsonRecord {
  error: { code: string };
}

const isCliError = function isCliError(value: unknown): value is CliError {
  return isRecord(value) && isRecord(value.error) && typeof value.error.code === 'string';
};

const parseRecord = (text: string): JsonRecord => {
  const value = parseJson(text);
  if (!isRecord(value)) {
    throw new Error('Expected a JSON object');
  }
  return value;
};

const parseError = (text: string): CliError => {
  const value = parseJson(text);
  if (!isCliError(value)) {
    throw new Error('Expected a CLI error response');
  }
  return value;
};

const documentResponses = (responses: ModelResponses): Record<string, ModelDocumentResponse> => {
  if (!isDocumentResponseMap(responses.byDocument)) {
    throw new Error('Expected document-specific model responses');
  }
  return responses.byDocument;
};

const documentResponse = (responses: ModelResponses, id: string): ModelDocumentResponse => {
  const response = documentResponses(responses)[id];
  if (response === undefined) {
    throw new Error(`Expected a model response for ${id}`);
  }
  return response;
};

const reviewResponse = (responses: ModelResponses): ModelReview => {
  if (!isModelReview(responses.review)) {
    throw new Error('Expected a review model response');
  }
  return responses.review;
};

const isModelPacket = function isModelPacket(value: unknown): value is ModelPacket {
  if (!isRecord(value)) {
    return false;
  }
  return (
    (value.targets === undefined || isStringArray(value.targets)) &&
    typeof value.operation === 'string' &&
    isRecordArray(value.documents)
  );
};

const parsePacket = (text: string): ModelPacket => {
  const value = parseJson(text);
  if (!isModelPacket(value)) {
    throw new Error('Expected the model packet shape');
  }
  return value;
};

const isSnapshotGraph = function isSnapshotGraph(value: unknown): value is SnapshotGraph {
  return isRecord(value) && isRecordArray(value.decisions) && isRecordArray(value.relationships);
};

const parseSnapshotGraph = (text: string): SnapshotGraph => {
  const value = parseJson(text);
  if (!isSnapshotGraph(value)) {
    throw new Error('Expected the knowledge snapshot shape');
  }
  return value;
};

const objectField = (value: unknown, name: string): JsonRecord => {
  if (!isRecord(value) || !isRecord(value[name])) {
    throw new Error(`Expected object field ${name}`);
  }
  return value[name];
};

const arrayField = (value: unknown, name: string): unknown[] => {
  if (!isRecord(value) || !Array.isArray(value[name])) {
    throw new Error(`Expected array field ${name}`);
  }
  return value[name];
};

const stringField = (value: unknown, name: string): string => {
  if (!isRecord(value) || typeof value[name] !== 'string') {
    throw new Error(`Expected string field ${name}`);
  }
  return value[name];
};

const numberField = (value: unknown, name: string): number => {
  if (!isRecord(value) || typeof value[name] !== 'number') {
    throw new Error(`Expected numeric field ${name}`);
  }
  return value[name];
};

const at = <T>(values: readonly T[], index: number): T => {
  const value = values.at(index);
  if (value === undefined) {
    throw new Error(`Expected fixture value at index ${index}`);
  }
  return value;
};

const recordAt = (values: readonly unknown[], index: number): JsonRecord => {
  const value = at(values, index);
  if (!isRecord(value)) {
    throw new Error('Expected a fixture object');
  }
  return value;
};

const decisionId = (value: unknown): string =>
  stringField(recordAt(arrayField(value, 'decisions'), 0), 'id');

const workId = (value: unknown): string => stringField(objectField(value, 'work'), 'id');

const workCalls = (value: unknown): number => numberField(objectField(value, 'work'), 'calls');

interface LegacyWorkFixture {
  command: 'ask' | 'update';
  key: string;
  kind: 'ask' | 'update';
  maxCalls: 0;
  maxInputBytes: 131_072;
  remaining: string[];
  snapshot: string;
  source: string;
  task: string | null;
}

// Legacy work identity fixtures from pre-refactor HEAD b6d93b1.
const legacyWorkFixtures: LegacyWorkFixture[] = [
  {
    command: 'update',
    key: '7196fc877822ee2f3634c2c9014fd0b8c507e5dd796736a64011ff3515d9a7f3',
    kind: 'update',
    maxCalls: 0,
    maxInputBytes: 131_072,
    remaining: ['docs/cache.md:1-3'],
    snapshot: 'a82b745c845cc5f7cc34d71a796ffb2894f3478a3e522836456fe47eff88eea5',
    source: 'docs/cache.md',
    task: null,
  },
  {
    command: 'ask',
    key: '0d9874f4a3dfb82655c55f7c5200409d74f6cfd6c680fb091722b2d9afa51202',
    kind: 'ask',
    maxCalls: 0,
    maxInputBytes: 131_072,
    remaining: ['docs/cache.md:1-3'],
    snapshot: 'a82b745c845cc5f7cc34d71a796ffb2894f3478a3e522836456fe47eff88eea5',
    source: 'docs/cache.md',
    task: 'private cache',
  },
];

const cli = nodePath.join(import.meta.dirname, 'cli.ts');

const project = (run: (root: string) => void) => {
  const root = mkdtempSync(nodePath.join(tmpdir(), 'hivex-knowledge-'));
  try {
    writeFileSync(
      nodePath.join(root, 'cache.md'),
      '# Cache\n\nCached data expires after seven days.\n'
    );
    writeFileSync(
      nodePath.join(root, 'privacy.md'),
      '# Access\n\nRevoking access immediately removes cached private data.\n'
    );
    run(root);
  } finally {
    rmSync(root, { force: true, recursive: true });
  }
};

const runCli = function runCli(root: string, cliArguments: string[]) {
  return spawnSync(process.execPath, [cli, ...cliArguments, '--root', root], {
    encoding: 'utf-8',
    timeout: 12_000,
  });
};

const invoke = (root: string, cliArguments: string[]) => {
  const result = runCli(root, cliArguments);
  if (!result.stdout) {
    throw new Error('Expected CLI JSON output');
  }
  return { ...result, value: parseRecord(result.stdout) };
};

const invokeError = (root: string, cliArguments: string[]) => {
  const result = runCli(root, cliArguments);
  if (result.stdout) {
    throw new Error('Expected CLI error output');
  }
  return result;
};

const quote = (value: string) => `'${value.replaceAll("'", "'\\''")}'`;

const model = function model(root: string, scenario = '') {
  const responses = nodePath.join(root, 'responses.json');
  const binary = nodePath.join(root, 'codex');
  const entry = nodePath.join(root, 'codex.mjs');
  writeFileSync(
    responses,
    JSON.stringify({
      ask: {
        answer:
          'Revoking access removes private cached data immediately; seven days is the ordinary lifetime.',
        evidence: [{ document: 'privacy.md', lineEnd: 3, lineStart: 3 }],
        uncertainties: [],
      },
      check: { findings: [] },
      extract: {
        decisions: [
          {
            conditions: [],
            document: 'cache.md',
            exceptions: [],
            id: 'c1',
            kind: 'constraint',
            lineEnd: 3,
            lineStart: 3,
            reason: 'Bound ordinary cache lifetime.',
            status: 'current',
            text: 'Cached data expires after seven days.',
          },
          {
            conditions: ['access is revoked'],
            document: 'privacy.md',
            exceptions: [],
            id: 'c2',
            kind: 'constraint',
            lineEnd: 3,
            lineStart: 3,
            reason: 'A cache must not extend access.',
            status: 'current',
            text: 'Access revocation immediately purges private cache.',
          },
        ],
        relationships: [
          {
            evidence: [
              { document: 'cache.md', lineEnd: 3, lineStart: 3 },
              { document: 'privacy.md', lineEnd: 3, lineStart: 3 },
            ],
            from: 'c2',
            id: 'r1',
            reason: 'Revocation overrides ordinary retention for private data.',
            to: 'c1',
            type: 'exception-to',
          },
        ],
        uncertainties: [],
      },
    })
  );
  writeFileSync(binary, `#!/bin/sh\nexec ${quote(process.execPath)} ${quote(entry)} "$@"\n`, {
    mode: 0o700,
  });
  writeFileSync(
    entry,
    `process.env.HIVEX_TEST_SCENARIO = ${JSON.stringify(scenario)};
process.env.HIVEX_TEST_CALLS_FILE = ${JSON.stringify(nodePath.join(root, 'model-calls.log'))};
process.env.HIVEX_TEST_RESPONSES = ${JSON.stringify(responses)};
await import(${JSON.stringify(
      pathToFileURL(nodePath.join(import.meta.dirname, '../test/codex-server.mjs')).href
    )});
`
  );
  return binary;
};

test('plans an initial knowledge update without spending when its work budget is zero', () => {
  project((root) => {
    const response = invoke(root, ['update', '--max-calls', '0', '--codex', model(root)]);
    expect(response.status).toBe(0);
    expect(response.value).toMatchObject({
      command: 'update',
      pendingDocuments: ['cache.md', 'privacy.md'],
      status: 'budget-exhausted',
      work: { calls: 0, maxCalls: 0 },
    });
  });
});

test('keeps declared history out of an ordinary update plan', () => {
  const root = mkdtempSync(nodePath.join(tmpdir(), 'hivex-selective-history-'));
  try {
    mkdirSync(nodePath.join(root, 'archive'), { recursive: true });
    writeFileSync(nodePath.join(root, 'active.md'), '# Active\n\nCurrent rule.\n');
    writeFileSync(nodePath.join(root, 'archive', 'replaced.md'), '# Replaced\n\nOld rule.\n');
    writeFileSync(
      nodePath.join(root, 'hivex.json'),
      JSON.stringify({ history: ['archive/**/*.md'], include: ['**/*.md'] })
    );

    const response = invoke(root, ['update', '--max-calls', '0', '--codex', '/nonexistent-codex']);

    expect(response.value).toMatchObject({
      pendingDocuments: ['active.md'],
      status: 'budget-exhausted',
    });
    expect(arrayField(response.value, 'pendingDocuments')).not.toContain('archive/replaced.md');
  } finally {
    rmSync(root, { force: true, recursive: true });
  }
});

for (const fixture of legacyWorkFixtures) {
  test(`reuses persisted ${fixture.command} work identity after refactoring`, () => {
    project((root) => {
      rmSync(nodePath.join(root, 'cache.md'));
      rmSync(nodePath.join(root, 'privacy.md'));
      mkdirSync(nodePath.join(root, 'docs'));
      writeFileSync(
        nodePath.join(root, fixture.source),
        '# Private cache\n\nRemove cached private data when access is revoked.\n',
        'utf-8'
      );
      let oldWorkId: string;
      {
        using store = new KnowledgeStore(root);
        oldWorkId = store.begin({
          key: fixture.key,
          kind: fixture.kind,
          maxCalls: fixture.maxCalls,
          maxInputBytes: fixture.maxInputBytes,
          remaining: fixture.remaining,
          snapshot: fixture.snapshot,
        }).id;
      }

      const commandArguments =
        fixture.command === 'ask'
          ? ['ask', fixture.task ?? '', '--source', fixture.source, '--max-calls', '0']
          : ['update', '--max-calls', '0'];
      const response = invoke(root, commandArguments);

      expect(response.value).toMatchObject({
        status: 'budget-exhausted',
        work: { calls: 0, id: oldWorkId, maxCalls: 0 },
      });
    });
  });
}

test('resumes a checked knowledge batch without repeating its extraction or resetting its budget', () => {
  project((root) => {
    const binary = model(root);
    const first = invoke(root, ['update', '--max-calls', '1', '--codex', binary]);
    expect(first.value).toMatchObject({
      status: 'budget-exhausted',
      work: { calls: 1, maxCalls: 1 },
    });
    const held = invoke(root, ['update', '--max-calls', '1', '--codex', binary]);
    expect(workCalls(held.value)).toBe(1);
    expect(
      readFileSync(nodePath.join(root, 'model-calls.log'), 'utf-8').trim().split('\n')
    ).toHaveLength(1);
    const resumed = invoke(root, ['update', '--max-calls', '2', '--codex', binary]);
    expect(resumed.status).toBe(0);
    expect(resumed.value).toMatchObject({
      status: 'ready',
      work: { calls: 2, maxCalls: 2, totalTokens: 300 },
    });
    const found = invoke(root, ['search', 'seven days']);
    expect(arrayField(found.value, 'decisions')).toContainEqual(
      expect.objectContaining({
        quality: 'checked',
        text: 'Cached data expires after seven days.',
      })
    );
    const context = invoke(root, ['neighbors', decisionId(found.value)]);
    expect(arrayField(context.value, 'decisions')).toContainEqual(
      expect.objectContaining({
        text: 'Access revocation immediately purges private cache.',
      })
    );
    const repeated = invoke(root, ['update', '--max-calls', '2', '--codex', binary]);
    expect(workCalls(repeated.value)).toBe(2);
  });
});

test('answers a task with original Markdown evidence rather than asking the owner again', () => {
  project((root) => {
    const binary = model(root);
    expect(invoke(root, ['update', '--codex', binary]).status).toBe(0);
    const answer = invoke(root, [
      'ask',
      'private cached data after access revocation',
      '--codex',
      binary,
    ]);
    expect(answer.status).toBe(0);
    expect(stringField(answer.value, 'answer')).toContain(
      'removes private cached data immediately'
    );
    expect(arrayField(answer.value, 'evidence')).toContainEqual(
      expect.objectContaining({
        document: 'privacy.md',
        lineEnd: 3,
        lineStart: 3,
        text: 'Revoking access immediately removes cached private data.',
      })
    );
    expect(workCalls(answer.value)).toBe(1);
    const repeated = invoke(root, [
      'ask',
      'private cached data after access revocation',
      '--codex',
      binary,
    ]);
    expect(repeated.value).toEqual(answer.value);
  });
});

test('discovers a cross-batch exception and follows its indirect dependents within the query limit', () => {
  project((root) => {
    rmSync(nodePath.join(root, 'cache.md'));
    rmSync(nodePath.join(root, 'privacy.md'));
    writeFileSync(
      nodePath.join(root, '01-cache.md'),
      '# Cache\n\nCached data expires after seven days.\n'
    );
    for (const name of ['02-note.md', '03-note.md', '04-note.md']) {
      writeFileSync(nodePath.join(root, name), '# Notes\n');
    }
    writeFileSync(
      nodePath.join(root, '05-access.md'),
      '# Permission\n\nWithdrawing authorisation destroys retained personal records immediately.\n'
    );
    writeFileSync(
      nodePath.join(root, '06-media.md'),
      '# Previews\n\nDerivative previews inherit withdrawal handling.\n'
    );
    const binary = model(root);
    const responsePath = nodePath.join(root, 'responses.json');
    const responses = parseModelResponses(
      readFileSync(responsePath, 'utf-8')
        .replaceAll('cache.md', '01-cache.md')
        .replaceAll('privacy.md', '05-access.md')
    );
    responses.byDocument = {
      '01-cache.md': {
        decisions: [at(responses.extract.decisions, 0)],
        relationships: [],
      },
      '05-access.md': {
        decisions: [at(responses.extract.decisions, 1)],
        relationships: [
          {
            ...at(responses.extract.relationships, 0),
            to: '@existing:01-cache.md',
          },
        ],
      },
      '06-media.md': {
        decisions: [
          {
            ...at(responses.extract.decisions, 1),
            document: '06-media.md',
            id: 'c3',
            text: 'Thumbnail caches honor access revocation.',
          },
        ],
        relationships: [
          {
            evidence: [
              { document: '05-access.md', lineEnd: 3, lineStart: 3 },
              { document: '06-media.md', lineEnd: 3, lineStart: 3 },
            ],
            from: 'c3',
            id: 'r2',
            reason: 'Thumbnail cleanup depends on revocation.',
            to: 'c2',
            type: 'requires',
          },
        ],
      },
    };
    writeFileSync(responsePath, JSON.stringify(responses));
    const updated = invoke(root, ['update', '--max-calls', '4', '--codex', binary]);
    expect(updated.value.status).toBe('ready');
    const found = invoke(root, ['search', 'seven days']);
    const expanded = invoke(root, ['neighbors', decisionId(found.value)]);
    expect(arrayField(expanded.value, 'decisions')).toContainEqual(
      expect.objectContaining({
        text: 'Thumbnail caches honor access revocation.',
      })
    );
    const bounded = invoke(root, ['neighbors', decisionId(found.value), '--limit', '2']);
    expect(arrayField(bounded.value, 'decisions')).toHaveLength(2);
    expect(arrayField(bounded.value, 'unexpandedDecisions')).toHaveLength(1);
  });
});

for (const scenario of ['invalid-json', 'changed-effort']) {
  test(`retains ${scenario} failure and does not retry it just because the budget is increased`, () => {
    project((root) => {
      const binary = model(root, scenario);
      const failed = invoke(root, ['update', '--codex', binary]);
      expect(failed.status).toBe(1);
      expect(failed.value).toMatchObject({ work: { calls: 1 } });
      const outcome = stringField(
        objectField(objectField(failed.value, 'work'), 'lastAttempt'),
        'outcome'
      );
      expect(typeof outcome).toBe('string');
      const unchanged = invoke(root, ['update', '--codex', binary, '--max-calls', '4']);
      expect(workCalls(unchanged.value)).toBe(1);
      model(root);
      const retried = invoke(root, [
        'update',
        '--codex',
        binary,
        '--max-calls',
        '4',
        '--retry-failed',
      ]);
      expect(retried.value).toMatchObject({
        status: 'ready',
        work: { calls: 3, maxCalls: 4 },
      });
    });
  });
}

test('reports a removed source and leaves its affected dependency unresolved', () => {
  project((root) => {
    const binary = model(root);
    expect(invoke(root, ['update', '--codex', binary]).status).toBe(0);
    const found = invoke(root, ['search', 'seven days']);
    rmSync(nodePath.join(root, 'privacy.md'));
    const status = invoke(root, ['status']);
    expect(status.value).toMatchObject({
      availableDecisions: 1,
      pendingDocuments: ['privacy.md'],
    });
    const context = invoke(root, ['neighbors', decisionId(found.value)]);
    expect(arrayField(context.value, 'unexpandedDecisions')).toHaveLength(1);
  });
});

test('keeps a decision with a bad line reference uncertain but can consult its original document', () => {
  project((root) => {
    const binary = model(root);
    const file = nodePath.join(root, 'responses.json');
    const responses = parseModelResponses(readFileSync(file, 'utf-8'));
    at(responses.extract.decisions, 1).lineStart = 999;
    at(responses.extract.decisions, 1).lineEnd = 999;
    writeFileSync(file, JSON.stringify(responses));
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('partial');
    const found = invoke(root, ['search', 'revocation']);
    expect(arrayField(found.value, 'decisions')).toContainEqual(
      expect.objectContaining({
        document: 'privacy.md',
        evidence: null,
        quality: 'uncertain',
      })
    );
    const answer = invoke(root, ['ask', 'access revocation', '--codex', binary]);
    expect(stringField(answer.value, 'answer')).toContain(
      'removes private cached data immediately'
    );
    expect(arrayField(answer.value, 'evidence')).toContainEqual(
      expect.objectContaining({ document: 'privacy.md', lineStart: 3 })
    );
    expect(answer.value.status).toBe('partial');
  });
});

test('finds source terminology even when the extracted decision does not repeat the word', () => {
  project((root) => {
    writeFileSync(
      nodePath.join(root, 'cache.md'),
      '# Cache\n\nCached data expires after seven days.\n\nMarmalade is the project name for the private cache.\n'
    );
    const binary = model(root);
    expect(invoke(root, ['update', '--codex', binary]).status).toBe(0);
    const found = invoke(root, ['search', 'Marmalade']);
    expect(arrayField(found.value, 'documents')).toContainEqual(
      expect.objectContaining({ id: 'cache.md' })
    );
    expect(arrayField(found.value, 'decisions')).toContainEqual(
      expect.objectContaining({ document: 'cache.md' })
    );
  });
});

test('can use an explicitly selected document when a task uses different vocabulary', () => {
  project((root) => {
    const binary = model(root);
    expect(invoke(root, ['update', '--codex', binary]).status).toBe(0);
    const answer = invoke(root, [
      'ask',
      '¿Cuándo desaparece mi información?',
      '--source',
      'privacy.md',
      '--codex',
      binary,
    ]);
    expect(stringField(answer.value, 'answer')).toContain(
      'removes private cached data immediately'
    );
    expect(workCalls(answer.value)).toBe(1);
  });
});

test('asks a declared historical source with bounded evidence and historical provenance', () => {
  project((root) => {
    mkdirSync(nodePath.join(root, 'archive'), { recursive: true });
    writeFileSync(
      nodePath.join(root, 'archive', 'replaced.md'),
      '# Replaced\n\nThe old cache rule allowed seven days.\n'
    );
    writeFileSync(
      nodePath.join(root, 'hivex.json'),
      JSON.stringify({ history: ['archive/**/*.md'], include: ['*.md'] })
    );
    const binary = model(root);
    const file = nodePath.join(root, 'responses.json');
    const responses = parseModelResponses(readFileSync(file, 'utf-8'));
    responses.capturePackets = true;
    responses.byDocument = {
      'archive/replaced.md': {
        decisions: [
          {
            ...at(responses.extract.decisions, 0),
            document: 'archive/replaced.md',
            id: 'history-rule',
            lineEnd: 3,
            lineStart: 3,
            status: 'current',
            text: 'The old cache rule allowed seven days.',
          },
        ],
        relationships: [],
      },
    };
    responses.ask = {
      answer: 'The old cache rule allowed seven days.',
      evidence: [{ document: 'archive/replaced.md', lineEnd: 3, lineStart: 3 }],
      uncertainties: [],
    };
    writeFileSync(file, JSON.stringify(responses));

    const first = invoke(root, [
      'ask',
      'old cache rule',
      '--source',
      'archive/replaced.md',
      '--max-calls',
      '2',
      '--codex',
      binary,
    ]);
    expect(first.value).toMatchObject({
      status: 'budget-exhausted',
      work: { calls: 2, maxCalls: 2 },
    });

    const answer = invoke(root, [
      'ask',
      'old cache rule',
      '--source',
      'archive/replaced.md',
      '--max-calls',
      '3',
      '--codex',
      binary,
    ]);

    expect(stringField(answer.value, 'answer')).toBe('The old cache rule allowed seven days.');
    expect(workId(answer.value)).toBe(workId(first.value));
    expect(workCalls(answer.value)).toBe(3);
    expect(arrayField(answer.value, 'evidence')).toContainEqual(
      expect.objectContaining({
        document: 'archive/replaced.md',
        historical: true,
        text: 'The old cache rule allowed seven days.',
      })
    );
    const found = invoke(root, ['search', 'old cache rule', '--source', 'archive/replaced.md']);
    expect(arrayField(found.value, 'decisions')).toContainEqual(
      expect.objectContaining({
        document: 'archive/replaced.md',
        historical: true,
        status: 'historical',
      })
    );
    const repaired = invoke(root, [
      'update',
      '--repair',
      'archive/replaced.md',
      '--reason',
      'Check the historical lifetime.',
      '--codex',
      binary,
    ]);
    expect(repaired.value).toMatchObject({
      status: 'ready',
      work: { calls: 2 },
    });
    writeFileSync(
      nodePath.join(root, 'cache.md'),
      '# Cache\n\nCurrent cache expires after eight days.\n'
    );
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('ready');
    const packets = readFileSync(`${file}.packets`, 'utf-8').trim().split('\n').map(parsePacket);
    const ordinary = packets.findLast((packet) => packet.operation === 'extract');
    if (!ordinary) {
      throw new Error('Expected extraction of the changed current document');
    }
    expect(Array.isArray(ordinary.targets)).toBe(true);
    expect(ordinary.targets).not.toContain('archive/replaced.md');
    expect(ordinary.documents).not.toContainEqual(
      expect.objectContaining({ id: 'archive/replaced.md' })
    );
  });
});

test('does not ingest a historical match during an ordinary consultation', () => {
  project((root) => {
    mkdirSync(nodePath.join(root, 'archive'), { recursive: true });
    writeFileSync(
      nodePath.join(root, 'archive', 'replaced.md'),
      '# Replaced\n\nThe old cache rule allowed seven days.\n'
    );
    writeFileSync(
      nodePath.join(root, 'hivex.json'),
      JSON.stringify({ history: ['archive/**/*.md'], include: ['*.md'] })
    );

    const answer = invoke(root, [
      'ask',
      'old cache rule',
      '--max-calls',
      '0',
      '--codex',
      '/nonexistent-codex',
    ]);

    expect(answer.value).toMatchObject({
      answer: null,
      command: 'ask',
      status: 'budget-exhausted',
    });
    expect(arrayField(answer.value, 'documents')).not.toContainEqual(
      expect.objectContaining({ id: 'archive/replaced.md' })
    );
    expect(arrayField(answer.value, 'pendingDocuments')).not.toContain('archive/replaced.md');
    writeFileSync(
      nodePath.join(root, 'archive/replaced.md'),
      '# Replaced\n\nUnrelated historical note.\n'
    );
    const resumed = invoke(root, ['ask', 'old cache rule', '--codex', '/nonexistent-codex']);
    expect(resumed.value.work).toMatchObject({
      calls: 0,
      id: workId(answer.value),
      maxCalls: 0,
    });
  });
});

test('names an unconsulted historical reference instead of silently approving incomplete context', () => {
  project((root) => {
    mkdirSync(nodePath.join(root, 'archive'));
    writeFileSync(
      nodePath.join(root, 'archive/exception.md'),
      '# Exception\n\nThe old exemption.\n'
    );
    writeFileSync(
      nodePath.join(root, 'cache.md'),
      '# Cache\n\nCached data expires after seven days.\n\nSee the [exception](archive/exception.md).\n'
    );
    writeFileSync(
      nodePath.join(root, 'hivex.json'),
      JSON.stringify({ history: ['archive/**/*.md'] })
    );
    const binary = model(root);
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('ready');
    const answer = invoke(root, ['ask', 'cache retention', '--codex', binary]);
    expect(answer.value.status).toBe('partial');
    expect(JSON.stringify(arrayField(answer.value, 'warnings'))).toContain('archive/exception.md');
    expect(workCalls(answer.value)).toBe(1);
  });
});

test('retrieves a consulted historical dependency when a current query reaches it', () => {
  project((root) => {
    mkdirSync(nodePath.join(root, 'archive'), { recursive: true });
    writeFileSync(
      nodePath.join(root, 'archive', 'replaced.md'),
      '# Replaced\n\nThe old cache rule allowed seven days. See [current](../cache.md).\n'
    );
    writeFileSync(
      nodePath.join(root, 'hivex.json'),
      JSON.stringify({ history: ['archive/**/*.md'], include: ['*.md'] })
    );
    const binary = model(root);
    const file = nodePath.join(root, 'responses.json');
    const responses = parseModelResponses(readFileSync(file, 'utf-8'));
    responses.capturePackets = true;
    responses.byDocument = {
      'archive/replaced.md': {
        decisions: [
          {
            ...at(responses.extract.decisions, 0),
            document: 'archive/replaced.md',
            id: 'history-rule',
            lineEnd: 3,
            lineStart: 3,
            text: 'The old cache rule allowed seven days.',
          },
        ],
        relationships: [
          {
            evidence: [
              { document: 'archive/replaced.md', lineEnd: 3, lineStart: 3 },
              { document: 'cache.md', lineEnd: 3, lineStart: 3 },
            ],
            from: 'history-rule',
            id: 'history-to-cache',
            reason: 'The current rule replaced the historical retention period.',
            to: '@existing:cache.md',
            type: 'supersedes',
          },
        ],
      },
      'cache.md': {
        decisions: [at(responses.extract.decisions, 0)],
        relationships: [],
      },
      'privacy.md': {
        decisions: [at(responses.extract.decisions, 1)],
        relationships: [],
      },
    };
    responses.ask = {
      answer: 'The current cache rule superseded the historical seven-day rule.',
      evidence: [{ document: 'archive/replaced.md', lineEnd: 3, lineStart: 3 }],
      uncertainties: [],
    };
    writeFileSync(file, JSON.stringify(responses));

    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('ready');
    expect(
      invoke(root, ['ask', 'old cache rule', '--source', 'archive/replaced.md', '--codex', binary])
        .value.status
    ).toBe('ready');

    const answer = invoke(root, ['ask', 'cache retention', '--codex', binary]);

    expect(arrayField(answer.value, 'evidence')).toContainEqual(
      expect.objectContaining({
        document: 'archive/replaced.md',
        historical: true,
      })
    );
    expect(arrayField(answer.value, 'pendingDocuments')).not.toContain('archive/replaced.md');
    writeFileSync(
      nodePath.join(root, 'archive', 'unread.md'),
      '# Unrelated history\n\nOld rule.\n'
    );
    expect(invoke(root, ['snapshot', 'export']).value).toMatchObject({
      modelCalls: 0,
      pendingUnits: [],
      status: 'ready',
    });
    project((clone) => {
      mkdirSync(nodePath.join(clone, 'archive'));
      mkdirSync(nodePath.join(clone, '.hivex'));
      for (const sourceFile of [
        'hivex.json',
        'archive/replaced.md',
        'archive/unread.md',
        '.hivex/graph.json',
      ]) {
        copyFileSync(nodePath.join(root, sourceFile), nodePath.join(clone, sourceFile));
      }
      const reused = invoke(clone, ['search', 'cache retention']);
      const neighbors = invoke(clone, ['neighbors', decisionId(reused.value)]);
      expect(arrayField(neighbors.value, 'decisions')).toContainEqual(
        expect.objectContaining({
          document: 'archive/replaced.md',
          status: 'historical',
        })
      );
      expect(arrayField(reused.value, 'pendingDocuments')).not.toContain('archive/unread.md');
      expect(existsSync(nodePath.join(clone, '.hivex/knowledge.sqlite'))).toBe(false);
      expect(invoke(clone, ['snapshot', 'import']).value.status).toBe('ready');
      const limited = invoke(clone, [
        'ask',
        'retention',
        '--source',
        'cache.md',
        '--limit',
        '1',
        '--max-calls',
        '0',
        '--codex',
        '/no-model',
      ]);
      const expanded = invoke(clone, [
        'ask',
        'retention',
        '--source',
        'cache.md',
        '--limit',
        '2',
        '--codex',
        '/no-model',
      ]);
      expect(expanded.value.work).toMatchObject({
        calls: 0,
        id: workId(limited.value),
        maxCalls: 0,
      });
    });
    writeFileSync(
      nodePath.join(root, 'cache.md'),
      '# Cache\n\nCached data expires after seven days.\n\nNew current context.\n'
    );
    const pending = invoke(root, ['update', '--max-calls', '1', '--codex', binary]);
    expect(workCalls(pending.value)).toBe(1);
    writeFileSync(
      nodePath.join(root, 'archive/replaced.md'),
      '# Replaced\n\nUpdated historical explanation.\n'
    );
    const resumed = invoke(root, ['update', '--max-calls', '2', '--codex', binary]);
    expect(resumed.value.work).toMatchObject({
      calls: 2,
      id: workId(pending.value),
    });
    const packetLines = readFileSync(`${file}.packets`, 'utf-8').trim().split('\n');
    const lastLine = packetLines.at(-1);
    if (lastLine === undefined) {
      throw new Error('Expected a recorded model packet');
    }
    const lastPacket = parsePacket(lastLine);
    expect(lastPacket.operation).toBe('extract');
    expect(JSON.stringify(lastPacket.documents)).toContain('Updated historical explanation.');
  });
});

test('ingests a large document in resumable rounds without losing earlier decisions', () => {
  project((root) => {
    rmSync(nodePath.join(root, 'privacy.md'));
    const paragraphs = Array.from({ length: 140 }, (...callbackArguments: [undefined, number]) => {
      const [, index] = callbackArguments;
      return `## Rule ${index}\n\nRule ${index} requires cache expiry. ${'Detailed rationale. '.repeat(16)}\n`;
    });
    writeFileSync(nodePath.join(root, 'cache.md'), paragraphs.join('\n'));
    const binary = model(root);
    const file = nodePath.join(root, 'responses.json');
    const responses = parseModelResponses(readFileSync(file, 'utf-8'));
    responses.fromVisibleRules = true;
    writeFileSync(file, JSON.stringify(responses));
    const first = invoke(root, ['update', '--max-calls', '2', '--codex', binary]);
    expect(first.value).toMatchObject({
      status: 'budget-exhausted',
      work: { calls: 2 },
    });
    expect(arrayField(first.value, 'pendingUnits').length).toBeGreaterThan(0);
    expect(arrayField(first.value, 'pendingDocuments')).toEqual(['cache.md']);
    const early = invoke(root, ['search', 'Rule 0']);
    expect(arrayField(early.value, 'decisions')).toContainEqual(
      expect.objectContaining({ text: 'Rule 0 requires cache expiry.' })
    );
    const resumed = invoke(root, [
      'update',
      '--max-calls',
      '32',
      '--max-input-bytes',
      '1048576',
      '--codex',
      binary,
    ]);
    expect(resumed.value.status).toBe('ready');
    expect(arrayField(resumed.value, 'pendingUnits')).toEqual([]);
    const found = invoke(root, ['search', 'Rule 139']);
    const expectedText = 'Rule 139 requires cache expiry.';
    const decision = arrayField(found.value, 'decisions').find(
      (entry) => isRecord(entry) && entry.text === expectedText
    );
    expect(decision).toMatchObject({
      evidence: { lineStart: 559 },
      text: expectedText,
    });
    expect(invoke(root, ['status']).value.availableDecisions).toBe(140);
    responses.ask.evidence = [{ document: 'cache.md', lineEnd: 559, lineStart: 559 }];
    responses.ask.answer = 'Rule 139 requires cache expiry.';
    writeFileSync(file, JSON.stringify(responses));
    const held = invoke(root, ['ask', 'Rule 139', '--max-calls', '0', '--codex', binary]);
    expect(held.value.status).toBe('budget-exhausted');
    expect(held.value.omittedUnits).toBeGreaterThan(0);
    const answer = invoke(root, ['ask', 'Rule 139', '--max-calls', '1', '--codex', binary]);
    expect(stringField(answer.value, 'answer')).toBe('Rule 139 requires cache expiry.');
    expect(arrayField(answer.value, 'evidence')).toContainEqual(
      expect.objectContaining({ lineStart: 559 })
    );
    expect(answer.value.omittedUnits).toBeGreaterThan(0);
    expect(
      readFileSync(nodePath.join(root, 'model-calls.log'), 'utf-8').trim().split('\n')
    ).toHaveLength(workCalls(resumed.value) + 1);
  });
});

test('reuses retained model results when previously ingested Markdown is restored', () => {
  project((root) => {
    rmSync(nodePath.join(root, 'privacy.md'));
    const original = readFileSync(nodePath.join(root, 'cache.md'), 'utf-8');
    const binary = model(root);
    const responsePath = nodePath.join(root, 'responses.json');
    const responses = parseModelResponses(readFileSync(responsePath, 'utf-8'));
    responses.extract.decisions = [at(responses.extract.decisions, 0)];
    responses.extract.relationships = [];
    writeFileSync(responsePath, JSON.stringify(responses));
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('ready');
    writeFileSync(nodePath.join(root, 'cache.md'), `${original}\nA new explanation.\n`);
    expect(workCalls(invoke(root, ['update', '--codex', binary]).value)).toBe(2);
    writeFileSync(nodePath.join(root, 'cache.md'), original);
    const restored = invoke(root, ['update', '--max-calls', '0', '--codex', binary]);
    expect(restored.value).toMatchObject({
      status: 'ready',
      work: { cacheHits: 2, calls: 0 },
    });
    expect(
      readFileSync(nodePath.join(root, 'model-calls.log'), 'utf-8').trim().split('\n')
    ).toHaveLength(4);
  });
});

test('accepts instruction-source metadata when native project instructions are disabled', () => {
  project((root) => {
    const binary = model(root, 'instruction-source-metadata');
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('ready');
  });
});

test('invalidates a relationship when its independent supporting document changes', () => {
  project((root) => {
    writeFileSync(
      nodePath.join(root, 'scope.md'),
      '# Scope\n\nRevocation overrides cache retention for private records.\n'
    );
    const binary = model(root);
    const file = nodePath.join(root, 'responses.json');
    const responses = parseModelResponses(readFileSync(file, 'utf-8'));
    at(responses.extract.relationships, 0).evidence = [
      { document: 'scope.md', lineEnd: 3, lineStart: 3 },
    ];
    writeFileSync(file, JSON.stringify(responses));
    expect(invoke(root, ['update', '--codex', binary]).value.relationships).toBe(1);
    const found = invoke(root, ['search', 'seven days']);
    writeFileSync(
      nodePath.join(root, 'scope.md'),
      '# Scope\n\nThis relationship is awaiting a decision.\n'
    );
    const neighbors = invoke(root, ['neighbors', decisionId(found.value)]);
    expect(arrayField(neighbors.value, 'relationships')).toEqual([]);
    expect(arrayField(neighbors.value, 'unexpandedDecisions')).toHaveLength(1);
  });
});

for (const initialBudget of [1, 2, 16]) {
  test(`restores earlier rounds after an intervening document version (initial budget ${initialBudget})`, () => {
    project((root) => {
      rmSync(nodePath.join(root, 'privacy.md'));
      const original = Array.from({ length: 90 }, (...callbackArguments: [undefined, number]) => {
        const [, index] = callbackArguments;
        return `## Rule ${index}\n\nRule ${index} requires cache expiry. ${'Reason. '.repeat(40)}\n`;
      }).join('\n');
      writeFileSync(nodePath.join(root, 'cache.md'), original);
      const binary = model(root);
      const file = nodePath.join(root, 'responses.json');
      const responses = parseModelResponses(readFileSync(file, 'utf-8'));
      responses.fromVisibleRules = true;
      writeFileSync(file, JSON.stringify(responses));
      const first = invoke(root, [
        'update',
        '--max-calls',
        String(initialBudget),
        '--max-input-bytes',
        '1048576',
        '--codex',
        binary,
      ]);
      expect(first.value.status).toBe(initialBudget < 16 ? 'budget-exhausted' : 'ready');
      writeFileSync(nodePath.join(root, 'cache.md'), original.replace('Rule 0', 'Rule zero'));
      expect(
        invoke(root, [
          'update',
          '--max-calls',
          initialBudget === 1 ? '16' : '2',
          '--max-input-bytes',
          '1048576',
          '--codex',
          binary,
        ]).value.status
      ).toBe(initialBudget === 1 ? 'ready' : 'budget-exhausted');
      writeFileSync(nodePath.join(root, 'cache.md'), original);
      const resumed = invoke(root, [
        'update',
        '--max-calls',
        '16',
        '--max-input-bytes',
        '1048576',
        '--codex',
        binary,
      ]);
      expect(resumed.value.status).toBe('ready');
      if (initialBudget < 16) {
        expect(workId(resumed.value)).toBe(workId(first.value));
      } else {
        expect(workCalls(resumed.value)).toBe(0);
      }
      expect(invoke(root, ['status']).value).toMatchObject({
        availableDecisions: 90,
        pendingDocuments: [],
      });
    });
  });
}

for (const scenario of ['unconfirmed-interrupt', 'start-unconfirmed']) {
  test(`requires acknowledgement before retrying ${scenario}`, () => {
    project((root) => {
      const binary = model(root, scenario);
      const first = invoke(root, [
        'update',
        '--max-calls',
        '1',
        '--deadline-ms',
        '100',
        '--codex',
        binary,
      ]);
      expect(first.value).toMatchObject({
        work: {
          lastAttempt: {
            turnAccepted: scenario === 'start-unconfirmed' ? 'unknown' : 'confirmed',
          },
        },
      });
      model(root);
      const retry = invokeError(root, [
        'update',
        '--max-calls',
        '2',
        '--retry-failed',
        '--codex',
        binary,
      ]);
      expect(retry.status).toBe(1);
      expect(retry.stderr).toContain('WORK_UNCERTAIN');
      expect(
        readFileSync(nodePath.join(root, 'model-calls.log'), 'utf-8').trim().split('\n')
      ).toHaveLength(1);
      const recovered = invoke(root, ['recover', '--acknowledge-uncertain']);
      expect(recovered.status).toBe(0);
      expect(recovered.value.acknowledgedWorks).toBe(1);
      const resumed = invoke(root, [
        'update',
        '--max-calls',
        '3',
        '--retry-failed',
        '--codex',
        binary,
      ]);
      expect(resumed.value).toMatchObject({
        status: 'ready',
        work: { calls: 3, unmeasuredAttempts: 1 },
      });
    });
  });
}

test('keeps original CR line numbers and separators in reads and evidence', () => {
  project((root) => {
    writeFileSync(
      nodePath.join(root, 'cache.md'),
      '# Cache\r\rCached data expires after seven days.\r'
    );
    const read = invoke(root, ['read', 'cache.md', '--from', '2', '--to', '3']);
    expect(read.value.text).toBe('\rCached data expires after seven days.\r');
    const binary = model(root);
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('ready');
    const found = invoke(root, ['search', 'seven days']);
    const expectedText = 'Cached data expires after seven days.';
    const decision = arrayField(found.value, 'decisions').find(
      (entry) => isRecord(entry) && entry.text === expectedText
    );
    expect(decision).toMatchObject({
      evidence: { lineStart: 3, text: expectedText },
    });
  });
});

test('plans rounds for Markdown larger than the previous two MiB source limit', () => {
  project((root) => {
    writeFileSync(
      nodePath.join(root, 'large.md'),
      `A durable rule. ${'Detail '.repeat(140)}\n\n`.repeat(2400)
    );
    const response = invoke(root, ['update', '--max-calls', '0', '--codex', '/nonexistent-codex']);
    expect(arrayField(response.value, 'pendingDocuments')).toContain('large.md');
    expect(arrayField(response.value, 'pendingUnits').length).toBeGreaterThan(100);
    expect(workCalls(response.value)).toBe(0);
    expect(arrayField(response.value, 'warnings')).toEqual([]);
  });
});

test('runs maintenance through the CLI without calling the model or discarding graph knowledge', () => {
  project((root) => {
    const binary = model(root);
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('ready');
    const before = invoke(root, ['search', 'seven days']);
    const recovered = invoke(root, ['recover']);
    expect(recovered.status).toBe(0);
    expect(recovered.value.modelCalls).toBe(0);
    const pruned = invoke(root, ['prune', '--keep-completed', '0', '--keep-caches', '0']);
    expect(pruned.status).toBe(0);
    expect(pruned.value.modelCalls).toBe(0);
    const after = invoke(root, ['search', 'seven days']);
    expect(arrayField(after.value, 'decisions')).toEqual(arrayField(before.value, 'decisions'));
    expect(
      readFileSync(nodePath.join(root, 'model-calls.log'), 'utf-8').trim().split('\n')
    ).toHaveLength(2);
  });
});

test('keeps an answer partial when its selected relationship was questioned', () => {
  project((root) => {
    const binary = model(root);
    const file = nodePath.join(root, 'responses.json');
    const responses = parseModelResponses(readFileSync(file, 'utf-8'));
    responses.check.findings = [
      { reason: 'The exception scope needs clarification.', target: 'r1' },
    ];
    writeFileSync(file, JSON.stringify(responses));
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('partial');
    const answer = invoke(root, ['ask', 'cache', '--codex', binary]);
    expect(answer.value.status).toBe('partial');
  });
});

test('supplies an independent source cited by a selected relationship', () => {
  project((root) => {
    writeFileSync(
      nodePath.join(root, 'scope.md'),
      '# Boundaries\n\nAmber overrides ordinary retention on authority withdrawal.\n'
    );
    const binary = model(root);
    const file = nodePath.join(root, 'responses.json');
    const responses = parseModelResponses(readFileSync(file, 'utf-8'));
    at(responses.extract.relationships, 0).evidence = [
      { document: 'scope.md', lineEnd: 3, lineStart: 3 },
    ];
    responses.ask.evidence = at(responses.extract.relationships, 0).evidence;
    writeFileSync(file, JSON.stringify(responses));
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('ready');
    const answer = invoke(root, ['ask', 'cache', '--codex', binary]);
    expect(arrayField(answer.value, 'evidence')).toContainEqual(
      expect.objectContaining({
        document: 'scope.md',
        text: 'Amber overrides ordinary retention on authority withdrawal.',
      })
    );
  });
});

test('checks a retained decision when another snapshot re-extracts the same unit', () => {
  project((root) => {
    rmSync(nodePath.join(root, 'privacy.md'));
    const binary = model(root);
    const file = nodePath.join(root, 'responses.json');
    const responses = parseModelResponses(readFileSync(file, 'utf-8'));
    responses.extract.decisions = [at(responses.extract.decisions, 0)];
    responses.extract.relationships = [];
    writeFileSync(file, JSON.stringify(responses));
    expect(invoke(root, ['update', '--max-calls', '1', '--codex', binary]).value.status).toBe(
      'budget-exhausted'
    );
    writeFileSync(nodePath.join(root, 'note.md'), '# Note\n\nAdditional project background.\n');
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('ready');
    rmSync(nodePath.join(root, 'note.md'));
    const restored = invoke(root, ['update', '--max-calls', '2', '--codex', binary]);
    expect(restored.value.status).toBe('ready');
    expect(invoke(root, ['status']).value.uncheckedDecisions).toEqual([]);
  });
});

test('ask updates knowledge and resumes its answer under one total work budget', () => {
  project((root) => {
    const binary = model(root);
    const first = invoke(root, ['ask', 'cache', '--max-calls', '2', '--codex', binary]);
    expect(first.value).toMatchObject({
      status: 'budget-exhausted',
      work: { calls: 2 },
    });
    expect(invoke(root, ['status']).value.availableDecisions).toBe(2);
    const resumed = invoke(root, ['ask', 'cache', '--max-calls', '3', '--codex', binary]);
    expect(resumed.value).toMatchObject({
      status: 'ready',
      work: { calls: 3, id: workId(first.value) },
    });
    const repeated = invoke(root, ['ask', 'cache', '--max-calls', '0', '--codex', binary]);
    expect(repeated.value).toEqual(resumed.value);
    expect(
      readFileSync(nodePath.join(root, 'model-calls.log'), 'utf-8').trim().split('\n')
    ).toHaveLength(3);
  });
});

test('refreshes a changed decision and its known incoming dependency beyond lexical or recent matches', () => {
  project((root) => {
    rmSync(nodePath.join(root, 'cache.md'));
    rmSync(nodePath.join(root, 'privacy.md'));
    writeFileSync(
      nodePath.join(root, '01-cache.md'),
      '# Cache\n\nCached data expires after seven days.\n'
    );
    writeFileSync(
      nodePath.join(root, '02-privacy.md'),
      '# Access\n\nRevoking access immediately removes cached private data.\n'
    );
    const binary = model(root);
    const file = nodePath.join(root, 'responses.json');
    const responses = parseModelResponses(
      readFileSync(file, 'utf-8')
        .replaceAll('cache.md', '01-cache.md')
        .replaceAll('privacy.md', '02-privacy.md')
    );
    responses.byDocument = {
      '01-cache.md': {
        decisions: [at(responses.extract.decisions, 0)],
        relationships: [],
      },
      '02-privacy.md': {
        decisions: [at(responses.extract.decisions, 1)],
        relationships: responses.extract.relationships,
      },
    };
    for (let index = 3; index <= 8; index += 1) {
      const document = `0${index}-note.md`;
      const text = `Amber ${index} controls decorative glyphs.`;
      writeFileSync(nodePath.join(root, document), `# Decoration ${index}\n\n${text}\n`);
      documentResponses(responses)[document] = {
        decisions: [
          {
            ...at(responses.extract.decisions, 0),
            document,
            id: `c${index}`,
            text,
          },
        ],
        relationships: [],
      };
    }
    writeFileSync(file, JSON.stringify(responses));
    expect(invoke(root, ['update', '--max-calls', '4', '--codex', binary]).value.status).toBe(
      'ready'
    );
    const before = recordAt(
      arrayField(invoke(root, ['search', 'revocation']).value, 'decisions'),
      0
    );
    writeFileSync(
      nodePath.join(root, '01-cache.md'),
      '# Timer\n\nFreshness renews every second sunrise.\n'
    );
    at(documentResponse(responses, '01-cache.md').decisions, 0).text =
      'Freshness renews every second sunrise.';
    documentResponse(responses, '01-cache.md').relationships = [
      {
        ...at(responses.extract.relationships, 0),
        from: '@existing:02-privacy.md',
        to: 'c1',
      },
    ];
    responses.ask.answer = 'Freshness renews every second sunrise; revocation takes priority.';
    writeFileSync(file, JSON.stringify(responses));
    const updated = invoke(root, ['ask', 'Freshness', '--codex', binary]);
    expect(updated.value).toMatchObject({
      status: 'ready',
      work: { calls: 3 },
    });
    const neighbors = invoke(root, ['neighbors', stringField(before, 'id')]);
    expect(arrayField(neighbors.value, 'relationships')).toHaveLength(1);
    expect(arrayField(neighbors.value, 'decisions')).toContainEqual(
      expect.objectContaining({
        document: '01-cache.md',
        text: 'Freshness renews every second sunrise.',
      })
    );
    expect(arrayField(neighbors.value, 'decisions')).toContainEqual(
      expect.objectContaining({ id: stringField(before, 'id') })
    );
  });
});

test('acknowledges document deletion without model extraction and names the unavailable dependency', () => {
  project((root) => {
    const binary = model(root);
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('ready');
    rmSync(nodePath.join(root, 'cache.md'));
    const answer = invoke(root, ['ask', 'revocation', '--codex', binary]);
    expect(answer.value).toMatchObject({
      pendingDocuments: [],
      status: 'partial',
      work: { calls: 1 },
    });
    expect(answer.value.unavailableDocuments).toContain('cache.md');
  });
});

test('repairs a wrong interpretation without changing Markdown or repeating an identical repair', () => {
  project((root) => {
    const original = readFileSync(nodePath.join(root, 'cache.md'), 'utf-8');
    const binary = model(root);
    const file = nodePath.join(root, 'responses.json');
    const responses = parseModelResponses(readFileSync(file, 'utf-8'));
    at(responses.extract.decisions, 0).text = 'Cached data never expires.';
    responses.check.findings = [
      { reason: 'The source says seven days, not forever.', target: 'c1' },
    ];
    writeFileSync(file, JSON.stringify(responses));
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('partial');
    responses.byDocument = {
      'cache.md': {
        decisions: [
          {
            ...at(responses.extract.decisions, 0),
            text: 'Cached data expires after seven days.',
          },
        ],
        relationships: [
          {
            ...at(responses.extract.relationships, 0),
            from: '@existing:privacy.md',
            to: 'c1',
          },
        ],
      },
    };
    responses.check.findings = [];
    writeFileSync(file, JSON.stringify(responses));
    const commandArguments = [
      'update',
      '--repair',
      'cache.md',
      '--reason',
      'Correct the lifetime against the seven-day rule.',
      '--codex',
      binary,
    ];
    const repaired = invoke(root, commandArguments);
    expect(repaired.value).toMatchObject({
      status: 'ready',
      work: { calls: 2 },
    });
    const found = invoke(root, ['search', 'expires']);
    expect(arrayField(found.value, 'decisions')).not.toContainEqual(
      expect.objectContaining({ text: 'Cached data never expires.' })
    );
    expect(arrayField(found.value, 'decisions')).toContainEqual(
      expect.objectContaining({
        quality: 'checked',
        text: 'Cached data expires after seven days.',
      })
    );
    expect(readFileSync(nodePath.join(root, 'cache.md'), 'utf-8')).toBe(original);
    expect(workId(invoke(root, commandArguments).value)).toBe(workId(repaired.value));
    expect(
      readFileSync(nodePath.join(root, 'model-calls.log'), 'utf-8').trim().split('\n')
    ).toHaveLength(4);
  });
});

test('keeps a source-local check finding out of an unrelated consultation', () => {
  project((root) => {
    writeFileSync(
      nodePath.join(root, 'decoration.md'),
      '# Decoration\n\nAmber controls glyph colour.\n'
    );
    const binary = model(root);
    const file = nodePath.join(root, 'responses.json');
    const responses = parseModelResponses(readFileSync(file, 'utf-8'));
    responses.extract.decisions = [
      ...responses.extract.decisions,
      {
        ...at(responses.extract.decisions, 0),
        document: 'decoration.md',
        id: 'c3',
        reason: 'Consistent decorative glyphs.',
        text: 'Amber controls glyph colour.',
      },
    ];
    responses.check.findings = [{ reason: 'The decorative exception is unclear.', target: 'c3' }];
    writeFileSync(file, JSON.stringify(responses));
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('partial');
    const answer = invoke(root, ['ask', 'cache', '--codex', binary]);
    expect(answer.value.status).toBe('ready');
    expect(JSON.stringify(arrayField(answer.value, 'warnings'))).not.toContain(
      'decorative exception'
    );
    const decoration = invoke(root, ['search', 'Amber']);
    expect(JSON.stringify(arrayField(decoration.value, 'warnings'))).toContain(
      'decorative exception'
    );
  });
});

test('warning scopes carry source references without repeating Markdown bodies', () => {
  project((root) => {
    const binary = model(root);
    const file = nodePath.join(root, 'responses.json');
    const responses = parseModelResponses(readFileSync(file, 'utf-8'));
    responses.extract.uncertainties = ['Source applicability is unclear.'];
    writeFileSync(file, JSON.stringify(responses));
    const updated = invoke(root, ['update', '--codex', binary]);
    expect(JSON.stringify(arrayField(updated.value, 'warnings'))).toContain(
      'Source applicability is unclear.'
    );
    expect(JSON.stringify(arrayField(updated.value, 'warnings'))).not.toContain(
      'Cached data expires after seven days.'
    );
  });
});

test('automatic maintenance prioritizes the matching fragment in a large document', () => {
  project((root) => {
    rmSync(nodePath.join(root, 'privacy.md'));
    writeFileSync(
      nodePath.join(root, 'cache.md'),
      Array.from({ length: 100 }, (...callbackArguments: [undefined, number]) => {
        const [, index] = callbackArguments;
        return `## Rule ${index}\n\nRule ${index} requires cache expiry. ${'Background detail. '.repeat(24)}${index === 99 ? ' Quasar.' : ''}\n`;
      }).join('\n')
    );
    const binary = model(root);
    const file = nodePath.join(root, 'responses.json');
    const responses = parseModelResponses(readFileSync(file, 'utf-8'));
    responses.fromVisibleRules = true;
    writeFileSync(file, JSON.stringify(responses));
    expect(
      invoke(root, ['ask', 'Quasar', '--max-calls', '2', '--codex', binary]).value.status
    ).toBe('budget-exhausted');
    expect(invoke(root, ['search', 'Rule 99']).value.decisions).toContainEqual(
      expect.objectContaining({ text: 'Rule 99 requires cache expiry.' })
    );
  });
});

test('evidence contains source coordinates and text without duplicating decision metadata', () => {
  project((root) => {
    const binary = model(root);
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('ready');
    const evidence = objectField(
      recordAt(arrayField(invoke(root, ['search', 'seven days']).value, 'decisions'), 0),
      'evidence'
    );
    expect(Object.keys(evidence).toSorted((left, right) => left.localeCompare(right))).toEqual([
      'document',
      'historical',
      'lineEnd',
      'lineStart',
      'text',
      'version',
    ]);
  });
});

test('repair removes an unsupported relationship even when its endpoints are in unchanged documents', () => {
  project((root) => {
    writeFileSync(
      nodePath.join(root, 'privacy.md'),
      '# Access\n\nPrivate reports require explicit authorisation.\n'
    );
    writeFileSync(
      nodePath.join(root, 'scope.md'),
      '# Scope\n\nThe examples do not approve an exception to retention.\n'
    );
    const binary = model(root);
    const file = nodePath.join(root, 'responses.json');
    const responses = parseModelResponses(readFileSync(file, 'utf-8'));
    responses.extract.decisions[1] = {
      ...at(responses.extract.decisions, 1),
      conditions: [],
      reason: 'Protect private reports.',
      text: 'Private reports require explicit authorisation.',
    };
    at(responses.extract.relationships, 0).evidence = [
      { document: 'scope.md', lineEnd: 3, lineStart: 3 },
    ];
    responses.check.findings = [{ reason: 'The claimed exception is unsupported.', target: 'r1' }];
    writeFileSync(file, JSON.stringify(responses));
    expect(invoke(root, ['update', '--codex', binary]).value.relationships).toBe(1);
    responses.byDocument = { 'scope.md': { decisions: [], relationships: [] } };
    responses.check.findings = [];
    writeFileSync(file, JSON.stringify(responses));
    const repaired = invoke(root, [
      'update',
      '--repair',
      'scope.md',
      '--reason',
      'Remove the unsupported exception.',
      '--codex',
      binary,
    ]);
    expect(repaired.value).toMatchObject({
      decisions: 2,
      relationships: 0,
      status: 'ready',
    });
  });
});

test('raising a context limit resumes the same work without resetting maintenance cost', () => {
  project((root) => {
    const binary = model(root);
    const first = invoke(root, [
      'ask',
      'cache',
      '--max-calls',
      '2',
      '--max-context-bytes',
      '2048',
      '--codex',
      binary,
    ]);
    expect(workCalls(first.value)).toBe(2);
    const resumed = invoke(root, [
      'ask',
      'cache',
      '--max-calls',
      '3',
      '--max-context-bytes',
      '65536',
      '--codex',
      binary,
    ]);
    expect(resumed.value).toMatchObject({
      status: 'ready',
      work: { calls: 3, id: workId(first.value) },
    });
  });
});

test('an omitted call limit never raises the budget of an unfinished consultation', () => {
  project((root) => {
    const binary = model(root);
    const first = invoke(root, ['ask', 'cache', '--max-calls', '1', '--codex', binary]);
    expect(first.value.work).toMatchObject({ calls: 1, maxCalls: 1 });
    const repeated = invoke(root, ['ask', 'cache', '--codex', binary]);
    expect(repeated.value).toMatchObject({
      status: 'budget-exhausted',
      work: { calls: 1, id: workId(first.value), maxCalls: 1 },
    });
    expect(
      readFileSync(nodePath.join(root, 'model-calls.log'), 'utf-8').trim().split('\n')
    ).toHaveLength(1);
  });
});

test('an endpoint update retains independent evidence and pauses before calls if that context cannot fit', () => {
  project((root) => {
    const supporting = 'The blue pulse condition governs this exception. '.repeat(3);
    writeFileSync(
      nodePath.join(root, 'scope.md'),
      `# Scope\n\n${Array.from({ length: 70 }, () => supporting).join('\n')}\n`
    );
    const binary = model(root);
    const file = nodePath.join(root, 'responses.json');
    const responses = parseModelResponses(readFileSync(file, 'utf-8'));
    at(responses.extract.relationships, 0).evidence = [
      { document: 'scope.md', lineEnd: 72, lineStart: 3 },
    ];
    writeFileSync(file, JSON.stringify(responses));
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('ready');
    writeFileSync(
      nodePath.join(root, 'cache.md'),
      '# Cache\n\nCached data expires after two days.\n'
    );
    responses.byDocument = {
      'cache.md': {
        decisions: [
          {
            ...at(responses.extract.decisions, 0),
            text: 'Cached data expires after two days.',
          },
        ],
        relationships: [
          {
            ...at(responses.extract.relationships, 0),
            from: '@existing:privacy.md',
            requiresEvidenceDocument: 'scope.md',
            to: 'c1',
          },
        ],
      },
    };
    writeFileSync(file, JSON.stringify(responses));
    const first = invoke(root, [
      'ask',
      'cache',
      '--max-calls',
      '3',
      '--max-context-bytes',
      '4096',
      '--codex',
      binary,
    ]);
    expect(first.value).toMatchObject({
      status: 'context-limit',
      work: { calls: 0 },
    });
    expect(
      arrayField(objectField(objectField(first.value, 'work'), 'contextLimit'), 'documents')
    ).toContain('scope.md');
    const resumed = invoke(root, [
      'ask',
      'cache',
      '--max-calls',
      '3',
      '--max-context-bytes',
      '65536',
      '--codex',
      binary,
    ]);
    expect(resumed.value).toMatchObject({
      status: 'ready',
      work: { calls: 3, id: workId(first.value) },
    });
    expect(invoke(root, ['search', 'cache']).value.relationships).toHaveLength(1);
  });
});

test('a document-level omission stays local even when no decision was extracted for that document', () => {
  project((root) => {
    writeFileSync(
      nodePath.join(root, 'decoration.md'),
      '# Decoration\n\nAmber controls glyph colour.\n'
    );
    const binary = model(root);
    const file = nodePath.join(root, 'responses.json');
    const responses = parseModelResponses(readFileSync(file, 'utf-8'));
    responses.check.findings = [
      { reason: 'The glyph decision was omitted.', target: 'decoration.md' },
    ];
    writeFileSync(file, JSON.stringify(responses));
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('partial');
    const answer = invoke(root, ['ask', 'cache', '--codex', binary]);
    expect(answer.value.status).toBe('ready');
    expect(JSON.stringify(arrayField(answer.value, 'warnings'))).not.toContain('glyph decision');
    expect(JSON.stringify(invoke(root, ['search', 'Amber']).value.warnings)).toContain(
      'glyph decision'
    );
  });
});

test('a changed known supporting document takes priority over unrelated pending documents', () => {
  project((root) => {
    writeFileSync(
      nodePath.join(root, 'z-scope.md'),
      '# Scope\n\nThe blue pulse activates this exception.\n'
    );
    const binary = model(root);
    const file = nodePath.join(root, 'responses.json');
    const responses = parseModelResponses(readFileSync(file, 'utf-8'));
    at(responses.extract.relationships, 0).evidence = [
      { document: 'z-scope.md', lineEnd: 3, lineStart: 3 },
    ];
    writeFileSync(file, JSON.stringify(responses));
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('ready');
    for (const name of ['b', 'c', 'd', 'e']) {
      writeFileSync(nodePath.join(root, `${name}.md`), '# Decoration\n\nAmber controls glyphs.\n');
    }
    writeFileSync(
      nodePath.join(root, 'z-scope.md'),
      '# Scope\n\nThe blue pulse no longer activates this exception.\n'
    );
    responses.byDocument = {
      'z-scope.md': { decisions: [], relationships: [] },
    };
    writeFileSync(file, JSON.stringify(responses));
    const answer = invoke(root, ['ask', 'cache', '--codex', binary]);
    expect(answer.value.work).toMatchObject({ calls: 3 });
    expect(arrayField(answer.value, 'pendingDocuments')).not.toContain('z-scope.md');
    expect(arrayField(answer.value, 'pendingDocuments').length).toBeGreaterThan(0);
    expect(answer.value.unavailableDocuments).not.toContain('z-scope.md');
  });
});

const reviewProject = function reviewProject(root: string, response?: ModelReview) {
  writeFileSync(
    nodePath.join(root, '.gitignore'),
    '.hivex/\nresponses.json\ncodex\ncodex.mjs\nmodel-calls.log\nreport.json\n'
  );
  writeFileSync(nodePath.join(root, 'cache.ts'), 'export const purgeOnRevocation = true;\n');
  for (const cliArguments of [
    ['init', '-q'],
    ['add', '.'],
    [
      '-c',
      'user.name=Fixture',
      '-c',
      'user.email=fixture@example.invalid',
      'commit',
      '-qm',
      'Initial',
    ],
  ]) {
    expect(spawnSync(git, cliArguments, { cwd: root }).status).toBe(0);
  }
  writeFileSync(nodePath.join(root, 'cache.ts'), 'export const purgeOnRevocation = false;\n');
  const binary = model(root);
  const file = nodePath.join(root, 'responses.json');
  const responses = parseModelResponses(readFileSync(file, 'utf-8'));
  responses.review = response ?? {
    findings: [
      {
        assessment: 'conflict',
        code: [{ lineEnd: 1, lineStart: 1, path: 'cache.ts', side: 'after' }],
        documents: [{ document: 'privacy.md', lineEnd: 3, lineStart: 3 }],
        explanation: 'Private cache survives access revocation, contradicting the purge rule.',
      },
    ],
    uncertainties: [],
  };
  writeFileSync(file, JSON.stringify(responses));
  return binary;
};

test('review shares maintenance budget, cites current code and documents, and detects later changes', () => {
  project((root) => {
    const binary = reviewProject(root);
    const first = invoke(root, [
      'review',
      'change cache behavior',
      '--base',
      'HEAD',
      '--max-calls',
      '2',
      '--codex',
      binary,
    ]);
    expect(first.stderr).toBe('');
    expect(first.value).toMatchObject({
      status: 'budget-exhausted',
      work: { calls: 2, maxCalls: 2 },
    });
    const final = invoke(root, [
      'review',
      'change cache behavior',
      '--base',
      'HEAD',
      '--max-calls',
      '3',
      '--codex',
      binary,
    ]);
    expect(final.value).toMatchObject({
      command: 'review',
      status: 'ready',
      work: { calls: 3, id: workId(first.value) },
    });
    expect(recordAt(arrayField(final.value, 'findings'), 0)).toMatchObject({
      assessment: 'conflict',
      code: [{ path: 'cache.ts', text: 'export const purgeOnRevocation = false;' }],
    });
    expect(
      recordAt(arrayField(recordAt(arrayField(final.value, 'findings'), 0), 'documents'), 0)
    ).toMatchObject({
      document: 'privacy.md',
      text: 'Revoking access immediately removes cached private data.',
    });
    writeFileSync(nodePath.join(root, 'report.json'), JSON.stringify(final.value));
    expect(invoke(root, ['review', '--check', 'report.json']).value.status).toBe('current');
    const repeat = invoke(root, [
      'review',
      'change cache behavior',
      '--base',
      'HEAD',
      '--max-calls',
      '0',
      '--codex',
      '/nonexistent-codex',
    ]);
    expect(repeat.value.work).toMatchObject({
      calls: 3,
      id: workId(final.value),
    });
    writeFileSync(nodePath.join(root, 'cache.ts'), 'export const purgeOnRevocation = true;\n');
    expect(invoke(root, ['review', '--check', 'report.json']).value).toMatchObject({
      documentsChanged: false,
      implementationChanged: true,
      status: 'stale',
    });
    writeFileSync(
      nodePath.join(root, 'privacy.md'),
      '\u{FEFF}# Access\n\nRevoking access immediately removes cached private data.\n'
    );
    expect(invoke(root, ['review', '--check', 'report.json']).value.documentsChanged).toBe(true);
  });
});

test('review can use an explicitly selected historical source with visible provenance', () => {
  project((root) => {
    mkdirSync(nodePath.join(root, 'archive'), { recursive: true });
    writeFileSync(
      nodePath.join(root, 'archive', 'replaced.md'),
      '# Replaced\n\nThe old rule required immediate purge.\n'
    );
    writeFileSync(
      nodePath.join(root, 'hivex.json'),
      JSON.stringify({ history: ['archive/**/*.md'], include: ['*.md'] })
    );
    const binary = reviewProject(root);
    const file = nodePath.join(root, 'responses.json');
    const responses = parseModelResponses(readFileSync(file, 'utf-8'));
    responses.byDocument = {
      'archive/replaced.md': {
        decisions: [
          {
            ...at(responses.extract.decisions, 0),
            document: 'archive/replaced.md',
            id: 'historical-purge',
            lineEnd: 3,
            lineStart: 3,
            status: 'current',
            text: 'The old rule required immediate purge.',
          },
        ],
        relationships: [],
      },
    };
    responses.review = {
      findings: [
        {
          assessment: 'conflict',
          code: [{ lineEnd: 1, lineStart: 1, path: 'cache.ts', side: 'after' }],
          documents: [{ document: 'archive/replaced.md', lineEnd: 3, lineStart: 3 }],
          explanation: 'The change drops the historical purge requirement.',
        },
      ],
      uncertainties: [],
    };
    writeFileSync(file, JSON.stringify(responses));

    const result = invoke(root, [
      'review',
      'historical cache behavior',
      '--base',
      'HEAD',
      '--source',
      'archive/replaced.md',
      '--codex',
      binary,
    ]);

    expect(result.value).toMatchObject({ command: 'review', status: 'ready' });
    expect(
      arrayField(recordAt(arrayField(result.value, 'findings'), 0), 'documents')
    ).toContainEqual(
      expect.objectContaining({
        document: 'archive/replaced.md',
        historical: true,
        text: 'The old rule required immediate purge.',
      })
    );
  });
});

test('review keeps an unverifiable finding local while retaining a supported exception', () => {
  project((root) => {
    const documents = [{ document: 'privacy.md', lineEnd: 3, lineStart: 3 }];
    const code = [{ lineEnd: 1, lineStart: 1, path: 'cache.ts', side: 'after' }];
    const binary = reviewProject(root, {
      findings: [
        {
          assessment: 'conflict',
          code: [{ ...code[0], lineEnd: 999 }],
          documents,
          explanation: 'A location that was not supplied.',
        },
        {
          assessment: 'exception',
          code,
          documents,
          explanation: 'Revocation overrides the normal expiry.',
        },
      ],
      uncertainties: ['The deployment size is not documented.'],
    });
    const result = invoke(root, ['review', 'cache', '--base', 'HEAD', '--codex', binary]);
    expect(result.value.status).toBe('partial');
    expect(recordAt(arrayField(result.value, 'findings'), 0)).toMatchObject({
      assessment: 'uncertain',
      code: [],
      referencesVerified: false,
    });
    expect(recordAt(arrayField(result.value, 'findings'), 1)).toMatchObject({
      assessment: 'exception',
      referencesVerified: true,
    });
    expect(result.value.uncertainties).toContain('The deployment size is not documented.');
  });
});

test('review binds deleted and untracked code and preserves an omitted resumed call limit', () => {
  project((root) => {
    const binary = reviewProject(root, {
      findings: [
        {
          assessment: 'conflict',
          code: [
            { lineEnd: 1, lineStart: 1, path: 'cache.ts', side: 'before' },
            { lineEnd: 1, lineStart: 1, path: 'new cache.ts', side: 'after' },
          ],
          documents: [{ document: 'privacy.md', lineEnd: 3, lineStart: 3 }],
          explanation: 'The replacement drops immediate purge.',
        },
      ],
      uncertainties: [],
    });
    rmSync(nodePath.join(root, 'cache.ts'));
    writeFileSync(nodePath.join(root, 'new cache.ts'), 'export const purgeOnRevocation = false;\n');
    const first = invoke(root, [
      'review',
      'cache',
      '--base',
      'HEAD',
      '--max-calls',
      '1',
      '--codex',
      binary,
    ]);
    expect(first.value.work).toMatchObject({ calls: 1, maxCalls: 1 });
    const held = invoke(root, ['review', 'cache', '--base', 'HEAD', '--codex', binary]);
    expect(held.value.work).toMatchObject({
      calls: 1,
      id: workId(first.value),
      maxCalls: 1,
    });
    const final = invoke(root, [
      'review',
      'cache',
      '--base',
      'HEAD',
      '--max-calls',
      '3',
      '--codex',
      binary,
    ]);
    expect(final.value.status).toBe('ready');
    expect(arrayField(recordAt(arrayField(final.value, 'findings'), 0), 'code')).toEqual([
      expect.objectContaining({
        path: 'cache.ts',
        side: 'before',
        text: 'export const purgeOnRevocation = true;',
      }),
      expect.objectContaining({
        path: 'new cache.ts',
        side: 'after',
        text: 'export const purgeOnRevocation = false;',
      }),
    ]);
    writeFileSync(nodePath.join(root, 'report.json'), JSON.stringify(final.value));
    writeFileSync(
      nodePath.join(root, 'new cache.ts'),
      '\u{FEFF}export const purgeOnRevocation = false;\n'
    );
    expect(invoke(root, ['review', '--check', 'report.json']).value.implementationChanged).toBe(
      true
    );
  });
});

test('review rejects oversized implementation before spending and exposes unsupported binary scope', () => {
  project((root) => {
    const binary = reviewProject(root);
    writeFileSync(nodePath.join(root, 'cache.ts'), 'x'.repeat(262_145));
    const large = invokeError(root, ['review', 'cache', '--base', 'HEAD', '--codex', binary]);
    expect(parseError(large.stderr).error.code).toBe('IMPLEMENTATION_TOO_LARGE');
    expect(invoke(root, ['status']).value.availableDecisions).toBe(0);
    writeFileSync(nodePath.join(root, 'cache.ts'), 'export const purgeOnRevocation = false;\n');
    writeFileSync(nodePath.join(root, 'asset.bin'), Buffer.from([0, 1, 2]));
    const partial = invoke(root, ['review', 'cache', '--base', 'HEAD', '--codex', binary]);
    expect(partial.value).toMatchObject({
      status: 'partial',
      work: { calls: 3 },
    });
    expect(JSON.stringify(arrayField(partial.value, 'warnings'))).toContain('Unsupported binary');
  });
});

test('review keeps 200 KiB binary versions bounded with digest-only warnings', () => {
  project((root) => {
    const assetPath = nodePath.join(root, 'asset.bin');
    const before = Buffer.alloc(200 * 1024);
    const after = Buffer.alloc(200 * 1024, 255);
    writeFileSync(assetPath, before);
    const binary = reviewProject(root);
    writeFileSync(assetPath, after);

    const implementation = captureImplementation(root, 'HEAD');
    const warnings = [
      'Unsupported binary or invalid UTF-8 content: before asset.bin (13f85ed26dc953b0410f9b1ab4ada10cc9f1719924804a2662cd46f8977e76e0)',
      'Unsupported binary or invalid UTF-8 content: after asset.bin (1b49c45eb2cce0c9af787939a85d848590b8383da07333bf8ecc56d57b5dfd75)',
    ];
    expect(implementation.files).toContainEqual({
      after: null,
      before: null,
      path: 'asset.bin',
    });
    expect(implementation.warnings).toEqual(warnings);
    expect(Buffer.byteLength(JSON.stringify(implementation))).toBeLessThan(256 * 1024);

    const result = invoke(root, ['review', 'cache', '--base', 'HEAD', '--codex', binary]);
    expect(result.status).toBe(0);
    expect(result.stderr).toBe('');
    expect(result.value).toMatchObject({
      status: 'partial',
      work: { calls: 3 },
    });
    const output = JSON.stringify(result.value);
    for (const warning of warnings) {
      expect(output).toContain(warning);
    }
    expect(output).not.toContain(before.toBase64());
    expect(output).not.toContain(after.toBase64());
    expect(Buffer.byteLength(output)).toBeLessThan(256 * 1024);
  });
});

test('expanding a partial review keeps the original work and its consumed budget', () => {
  project((root) => {
    writeFileSync(
      nodePath.join(root, 'cache.md'),
      `# Cache\n\nCached data expires after seven days.\n\n${'Supporting rationale. '.repeat(270)}\n`
    );
    const binary = reviewProject(root);
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('ready');
    const first = invoke(root, [
      'review',
      'cache',
      '--base',
      'HEAD',
      '--max-calls',
      '1',
      '--max-context-bytes',
      '5000',
      '--codex',
      binary,
    ]);
    expect(first.value).toMatchObject({
      omittedUnits: 1,
      status: 'partial',
      work: { calls: 1, maxCalls: 1 },
    });
    const expanded = invoke(root, [
      'review',
      'cache',
      '--base',
      'HEAD',
      '--max-calls',
      '1',
      '--max-context-bytes',
      '30000',
      '--codex',
      binary,
    ]);
    expect(expanded.value).toMatchObject({
      status: 'budget-exhausted',
      work: { calls: 1, id: workId(first.value), maxCalls: 1 },
    });
    const continued = invoke(root, [
      'review',
      'cache',
      '--base',
      'HEAD',
      '--max-calls',
      '2',
      '--max-context-bytes',
      '30000',
      '--codex',
      binary,
    ]);
    expect(continued.value).toMatchObject({
      omittedUnits: 0,
      status: 'ready',
      work: { calls: 2, id: workId(first.value), maxCalls: 2 },
    });
  });
});

test('review retrieves decisions from new file content without hints in the task or filename', () => {
  project((root) => {
    const binary = reviewProject(root);
    writeFileSync(nodePath.join(root, 'cache.ts'), 'export const purgeOnRevocation = true;\n');
    writeFileSync(
      nodePath.join(root, 'worker.ts'),
      'export const cache = { expiresAfterDays: 90 };\n'
    );
    const result = invoke(root, [
      'review',
      'Implement worker',
      '--base',
      'HEAD',
      '--max-calls',
      '0',
      '--codex',
      binary,
    ]);
    expect(result.value.status).toBe('budget-exhausted');
    expect(arrayField(result.value, 'documents')).toContainEqual(
      expect.objectContaining({ id: 'cache.md' })
    );
  });
});

test('source quotes carried by the graph remain citable when full document units are omitted', () => {
  project((root) => {
    writeFileSync(
      nodePath.join(root, 'cache.md'),
      `# Cache\n\nCached data expires after seven days.\n\n${'Supporting rationale. '.repeat(270)}\n`
    );
    const binary = reviewProject(root);
    const file = nodePath.join(root, 'responses.json');
    const responses = parseModelResponses(readFileSync(file, 'utf-8'));
    at(responses.extract.decisions, 0).lineEnd = 4;
    const evidence = [{ document: 'cache.md', lineEnd: 4, lineStart: 3 }];
    responses.ask.evidence = evidence;
    at(reviewResponse(responses).findings, 0).documents = evidence;
    const review = reviewResponse(responses);
    review.findings = [
      at(review.findings, 0),
      {
        ...at(review.findings, 0),
        documents: [{ document: 'cache.md', lineEnd: 5, lineStart: 5 }],
      },
    ];
    writeFileSync(file, JSON.stringify(responses));
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('ready');
    const asked = invoke(root, ['ask', 'cache', '--max-context-bytes', '5000', '--codex', binary]);
    expect(asked.value.omittedUnits).toBe(1);
    expect(arrayField(asked.value, 'evidence')).toContainEqual(
      expect.objectContaining({
        document: 'cache.md',
        lineStart: 3,
        text: 'Cached data expires after seven days.\n',
      })
    );
    const reviewed = invoke(root, [
      'review',
      'cache',
      '--base',
      'HEAD',
      '--max-context-bytes',
      '5000',
      '--codex',
      binary,
    ]);
    expect(reviewed.value.omittedUnits).toBe(1);
    expect(recordAt(arrayField(reviewed.value, 'findings'), 0)).toMatchObject({
      documents: [{ document: 'cache.md', lineStart: 3 }],
      referencesVerified: true,
    });
    expect(recordAt(arrayField(reviewed.value, 'findings'), 1)).toMatchObject({
      assessment: 'uncertain',
      documents: [],
      referencesVerified: false,
    });
  });
});

test('review supplies changed ranges of a large file and keeps exact line evidence', () => {
  project((root) => {
    const binary = reviewProject(root, {
      findings: [
        {
          assessment: 'conflict',
          code: [
            {
              lineEnd: 14_001,
              lineStart: 14_001,
              path: 'large.ts',
              side: 'after',
            },
          ],
          documents: [{ document: 'privacy.md', lineEnd: 3, lineStart: 3 }],
          explanation: 'The changed flag disables required purge.',
        },
        {
          assessment: 'conflict',
          code: [{ lineEnd: 1, lineStart: 1, path: 'large.ts', side: 'after' }],
          documents: [{ document: 'privacy.md', lineEnd: 3, lineStart: 3 }],
          explanation: 'This unrelated line is not supplied.',
        },
      ],
      uncertainties: [],
    });
    const unchanged = '// Unchanged generated implementation context.\n'.repeat(14_000);
    writeFileSync(
      nodePath.join(root, 'large.ts'),
      `${unchanged}export const purgeOnRevocation = true;\n`
    );
    expect(spawnSync(git, ['add', 'large.ts'], { cwd: root }).status).toBe(0);
    expect(
      spawnSync(
        git,
        [
          '-c',
          'user.name=Fixture',
          '-c',
          'user.email=fixture@example.invalid',
          'commit',
          '-qm',
          'Large baseline',
        ],
        { cwd: root }
      ).status
    ).toBe(0);
    writeFileSync(
      nodePath.join(root, 'large.ts'),
      `${unchanged}export const purgeOnRevocation = false;\n`
    );
    const result = invoke(root, ['review', 'cache purge', '--base', 'HEAD', '--codex', binary]);
    expect(result.stderr).toBe('');
    expect(result.value).toMatchObject({
      status: 'partial',
      work: { calls: 3 },
    });
    expect(recordAt(arrayField(result.value, 'findings'), 0)).toMatchObject({
      code: [
        {
          lineStart: 14_001,
          path: 'large.ts',
          text: 'export const purgeOnRevocation = false;',
        },
      ],
      referencesVerified: true,
    });
    expect(recordAt(arrayField(result.value, 'findings'), 1)).toMatchObject({
      assessment: 'uncertain',
      code: [],
      referencesVerified: false,
    });
    expect(JSON.stringify(arrayField(result.value, 'warnings'))).toContain(
      'unchanged code is omitted'
    );
  });
});

test('shares checked knowledge with a fresh clone without re-extraction or query artifacts', () => {
  project((root) => {
    const binary = model(root);
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('ready');
    const exported = invoke(root, ['snapshot', 'export']);
    expect(exported.status).toBe(0);
    expect(exported.value).toMatchObject({
      command: 'snapshot',
      modelCalls: 0,
      operation: 'export',
    });
    const path = nodePath.join(root, '.hivex/graph.json');
    const snapshot = readFileSync(path, 'utf-8');
    expect(invoke(root, ['snapshot', 'export']).status).toBe(0);
    expect(readFileSync(path, 'utf-8')).toBe(snapshot);
    expect(
      Object.keys(parseRecord(snapshot)).toSorted((left, right) => left.localeCompare(right))
    ).toEqual([
      'decisions',
      'documents',
      'lastExtraction',
      'relationships',
      'units',
      'version',
      'warnings',
    ]);
    project((clone) => {
      mkdirSync(nodePath.join(clone, '.hivex'));
      copyFileSync(path, nodePath.join(clone, '.hivex/graph.json'));
      const found = invoke(clone, ['search', 'seven days']);
      expect(found.status).toBe(0);
      expect(arrayField(found.value, 'decisions')).toContainEqual(
        expect.objectContaining({
          quality: 'checked',
          text: 'Cached data expires after seven days.',
        })
      );
      expect(existsSync(nodePath.join(clone, '.hivex/knowledge.sqlite'))).toBe(false);
      const neighbors = invoke(clone, ['neighbors', decisionId(found.value)]);
      expect(arrayField(neighbors.value, 'decisions')).toContainEqual(
        expect.objectContaining({
          text: 'Access revocation immediately purges private cache.',
        })
      );
      const asked = invoke(clone, [
        'ask',
        'seven days',
        '--max-calls',
        '0',
        '--codex',
        '/no-model',
      ]);
      expect(workCalls(asked.value)).toBe(0);
      expect(invoke(clone, ['neighbors', decisionId(found.value)]).value.decisions).toEqual(
        arrayField(neighbors.value, 'decisions')
      );
      const updated = invoke(clone, ['update', '--max-calls', '0', '--codex', '/no-model']);
      expect(updated.value).toMatchObject({
        pendingUnits: [],
        status: 'ready',
        work: { calls: 0 },
      });
      expect(readFileSync(nodePath.join(clone, '.hivex/graph.json'), 'utf-8')).toBe(snapshot);
    });
  });
});

test('snapshot import preserves unfinished work and only restores graph knowledge after completion', () => {
  project((root) => {
    expect(invoke(root, ['update', '--codex', model(root)]).value.status).toBe('ready');
    expect(invoke(root, ['snapshot', 'export']).status).toBe(0);
    project((clone) => {
      const binary = model(clone);
      const pending = invoke(clone, ['update', '--max-calls', '1', '--codex', binary]);
      expect(workCalls(pending.value)).toBe(1);
      copyFileSync(
        nodePath.join(root, '.hivex/graph.json'),
        nodePath.join(clone, '.hivex/graph.json')
      );
      const refused = invokeError(clone, ['snapshot', 'import']);
      expect(refused.status).toBe(1);
      expect(parseError(refused.stderr).error.code).toBe('UNFINISHED_WORK');
      const resumed = invoke(clone, ['update', '--max-calls', '2', '--codex', binary]);
      expect(resumed.value).toMatchObject({
        status: 'ready',
        work: { calls: 2, id: workId(pending.value) },
      });
      const imported = invoke(clone, ['snapshot', 'import']);
      expect(imported.status).toBe(0);
      expect(imported.value).toMatchObject({
        command: 'snapshot',
        modelCalls: 0,
        operation: 'import',
        status: 'ready',
      });
      expect(workCalls(invoke(clone, ['update', '--codex', binary]).value)).toBe(2);
      expect(
        readFileSync(nodePath.join(clone, 'model-calls.log'), 'utf-8').trim().split('\n')
      ).toHaveLength(2);
    });
  });
});

test('shared snapshots expose changed sources and retain reusable neighboring knowledge', () => {
  project((root) => {
    expect(invoke(root, ['update', '--codex', model(root)]).value.status).toBe('ready');
    expect(invoke(root, ['snapshot', 'export']).status).toBe(0);
    project((clone) => {
      mkdirSync(nodePath.join(clone, '.hivex'));
      copyFileSync(
        nodePath.join(root, '.hivex/graph.json'),
        nodePath.join(clone, '.hivex/graph.json')
      );
      writeFileSync(
        nodePath.join(clone, 'cache.md'),
        '# Cache\n\nCached data expires after thirty days.\n'
      );
      const imported = invoke(clone, ['snapshot', 'import']);
      expect(imported.value).toMatchObject({
        sources: { stale: ['cache.md'], unavailable: [] },
        status: 'partial',
      });
      expect(invoke(clone, ['search', 'seven days']).value.decisions).toEqual([]);
      expect(invoke(clone, ['search', 'revocation']).value.decisions).toContainEqual(
        expect.objectContaining({
          text: 'Access revocation immediately purges private cache.',
        })
      );
      const pending = invoke(clone, ['update', '--max-calls', '0', '--codex', '/no-model']);
      expect(pending.value).toMatchObject({
        pendingDocuments: ['cache.md'],
        work: { calls: 0 },
      });
    });
  });
});

test('rejects invalid snapshot relationships without corrupting local knowledge', () => {
  project((root) => {
    expect(invoke(root, ['update', '--codex', model(root)]).value.status).toBe('ready');
    invoke(root, ['snapshot', 'export']);
    const path = nodePath.join(root, '.hivex/graph.json');
    const graph = parseSnapshotGraph(readFileSync(path, 'utf-8'));
    at(graph.relationships, 0).to = 'missing-decision';
    writeFileSync(path, JSON.stringify(graph));
    const invalid = invokeError(root, ['snapshot', 'import']);
    expect(invalid.status).toBe(1);
    expect(parseError(invalid.stderr).error.code).toBe('INVALID_SNAPSHOT');
    expect(invoke(root, ['search', 'seven days']).value.decisions).toHaveLength(1);
    expect(invoke(root, ['snapshot', 'export']).status).toBe(0);
    expect(readFileSync(path, 'utf-8')).not.toContain('missing-decision');
  });
});

test('rejects broken evidence before replacing locally checked knowledge', () => {
  project((root) => {
    expect(invoke(root, ['update', '--codex', model(root)]).value.status).toBe('ready');
    expect(invoke(root, ['snapshot', 'export']).status).toBe(0);
    const path = nodePath.join(root, '.hivex/graph.json');
    const original = readFileSync(path, 'utf-8');
    const empty = parseSnapshotGraph(original);
    at(empty.relationships, 0).evidence = [];
    const reversed = parseSnapshotGraph(original);
    recordAt(arrayField(at(reversed.relationships, 0), 'evidence'), 0).lineEnd = 1;
    const decision = parseSnapshotGraph(original);
    at(decision.decisions, 0).lineEnd = 1;
    for (const invalid of [empty, reversed, decision]) {
      writeFileSync(path, JSON.stringify(invalid));
      const imported = invokeError(root, ['snapshot', 'import']);
      expect(imported.status).toBe(1);
      expect(parseError(imported.stderr).error.code).toBe('INVALID_SNAPSHOT');
      expect(invoke(root, ['snapshot', 'export']).status).toBe(0);
      expect(readFileSync(path, 'utf-8')).toBe(original);
    }
  });
});

test('does not follow a shared snapshot symlink or overwrite its target', () => {
  project((root) => {
    mkdirSync(nodePath.join(root, '.hivex'));
    const target = nodePath.join(root, 'untouched.json');
    writeFileSync(target, '{"private":"untouched"}\n');
    symlinkSync(target, nodePath.join(root, '.hivex/graph.json'));
    for (const cliArguments of [
      ['search', 'cache'],
      ['snapshot', 'import'],
      ['snapshot', 'export'],
    ]) {
      const response = invokeError(root, cliArguments);
      expect(response.status).toBe(1);
      expect(parseError(response.stderr).error.code).toBe('INVALID_SNAPSHOT');
    }
    expect(readFileSync(target, 'utf-8')).toBe('{"private":"untouched"}\n');
  });
});

test('does not seed over unfinished local work when its first graph is still absent', () => {
  project((root) => {
    invoke(root, ['update', '--codex', model(root)]);
    invoke(root, ['snapshot', 'export']);
    project((clone) => {
      const pending = invoke(clone, ['update', '--max-calls', '0', '--codex', '/no-model']);
      copyFileSync(
        nodePath.join(root, '.hivex/graph.json'),
        nodePath.join(clone, '.hivex/graph.json')
      );
      const unchanged = invoke(clone, ['update', '--max-calls', '0', '--codex', '/no-model']);
      expect(unchanged.value).toMatchObject({
        pendingDocuments: ['cache.md', 'privacy.md'],
        status: 'budget-exhausted',
        work: { calls: 0, id: workId(pending.value) },
      });
      expect(invoke(clone, ['search', 'seven days']).value.decisions).toEqual([]);
    });
  });
});

test('reports unavailable snapshot sources and rejects malformed JSON without losing local data', () => {
  project((root) => {
    invoke(root, ['update', '--codex', model(root)]);
    invoke(root, ['snapshot', 'export']);
    rmSync(nodePath.join(root, 'privacy.md'));
    const partial = invoke(root, ['snapshot', 'import']);
    expect(partial.value).toMatchObject({
      sources: { unavailable: ['privacy.md'] },
      status: 'partial',
    });
    writeFileSync(nodePath.join(root, '.hivex/graph.json'), '{');
    const malformed = invokeError(root, ['snapshot', 'import']);
    expect(malformed.status).toBe(1);
    expect(parseError(malformed.stderr).error.code).toBe('INVALID_SNAPSHOT');
    expect(invoke(root, ['search', 'seven days']).value.decisions).toHaveLength(1);
  });
});
