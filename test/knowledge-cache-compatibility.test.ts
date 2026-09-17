import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { Database } from 'bun:sqlite';
import { expect, test } from 'bun:test';

const knowledgeCommand = function knowledgeCommand(input: string[]) {
  const binary = process.env.HIVEX_TEST_BINARY;
  if (binary === undefined) {
    throw new Error('HIVEX_TEST_BINARY is required');
  }
  const result = spawnSync(binary, input, {
    encoding: 'utf-8',
    timeout: 12_000,
  });
  if (!result.stdout) {
    throw new Error(result.stderr || 'Expected CLI JSON output');
  }
  const value: unknown = JSON.parse(result.stdout);
  return value;
};

const fixture = readFileSync(new URL('fixtures/knowledge-cache-v1.sql', import.meta.url), 'utf-8');

test.each([
  { cacheHits: 0, state: 'completed' },
  { cacheHits: 1, state: 'model-cache' },
])('retains a v1 $state answer with its exhausted budget', async ({ cacheHits, state }) => {
  using cleanup = new DisposableStack();
  const root = mkdtempSync(path.join(tmpdir(), 'hivex-cache-compat-'));
  cleanup.defer(() => {
    rmSync(root, { force: true, recursive: true });
  });
  writeFileSync(path.join(root, 'notes.md'), '# Policy\nUse bounded work.\nPreserve the budget.\n');
  mkdirSync(path.join(root, '.hivex'));
  using database = new Database(path.join(root, '.hivex/knowledge.sqlite'));
  database.run(fixture);
  if (state === 'model-cache') {
    database.run(
      "UPDATE work SET data=json_remove(json_set(data, '$.status', 'pending'), '$.result')"
    );
  }
  const result = await knowledgeCommand([
    'ask',
    'bounded',
    '--source',
    'notes.md',
    '--root',
    root,
    '--max-calls',
    '1',
    '--codex',
    path.join(root, 'model-must-not-start'),
  ]);
  expect(result).toMatchObject({
    answer: 'Use bounded work and preserve the budget.',
    status: 'ready',
    work: {
      cacheHits,
      calls: 1,
      id: '84171802-e68b-43d8-b327-9ff47d302375',
      inputBytes: 100,
      maxCalls: 1,
      maxInputBytes: 131_072,
      totalTokens: 10,
    },
  });
});

test('reuses v1 extraction and check caches with the original exhausted update budget', async () => {
  using cleanup = new DisposableStack();
  const root = mkdtempSync(path.join(tmpdir(), 'hivex-update-cache-compat-'));
  cleanup.defer(() => {
    rmSync(root, { force: true, recursive: true });
  });
  writeFileSync(path.join(root, 'notes.md'), '# Policy\nUse bounded work.\nPreserve the budget.\n');
  mkdirSync(path.join(root, '.hivex'));
  using database = new Database(path.join(root, '.hivex/knowledge.sqlite'));
  database.run(
    readFileSync(new URL('fixtures/knowledge-update-cache-v1.sql', import.meta.url), 'utf-8')
  );
  const result = await knowledgeCommand([
    'update',
    '--root',
    root,
    '--max-calls',
    '2',
    '--codex',
    path.join(root, 'model-must-not-start'),
  ]);
  expect(result).toMatchObject({
    decisions: 2,
    pendingUnits: [],
    relationships: 1,
    status: 'ready',
    work: {
      cacheHits: 2,
      calls: 2,
      id: 'df3ff9f3-1c23-4103-a08b-e7c3d65683e8',
      inputBytes: 4282,
      maxCalls: 2,
      maxInputBytes: 131_072,
      totalTokens: 10,
    },
  });
});

test('keeps the work budget when scoped relationship context changes a legacy packet', async () => {
  using cleanup = new DisposableStack();
  const root = mkdtempSync(path.join(tmpdir(), 'hivex-multiround-cache-compat-'));
  cleanup.defer(() => {
    rmSync(root, { force: true, recursive: true });
  });
  const markdown = [1, 2, 3]
    .map((number) => {
      const details = 'Detail '.repeat(800).trimEnd();
      return `# Section ${number}\n\nRule ${number} requires bounded work. ${details}\n`;
    })
    .join('\n');
  writeFileSync(path.join(root, 'notes.md'), markdown);
  mkdirSync(path.join(root, '.hivex'));
  using database = new Database(path.join(root, '.hivex/knowledge.sqlite'));
  database.run(
    readFileSync(new URL('fixtures/knowledge-multiround-cache-v1.sql', import.meta.url), 'utf-8')
  );
  const result = await knowledgeCommand([
    'update',
    '--root',
    root,
    '--max-calls',
    '4',
    '--codex',
    path.join(root, 'model-must-not-start'),
  ]);
  expect(result).toMatchObject({
    decisions: 2,
    pendingUnits: ['notes.md:11-11'],
    relationships: 1,
    status: 'budget-exhausted',
    work: {
      cacheHits: 2,
      calls: 4,
      id: 'cc4bb47f-3512-40b0-8c6c-9ef5a326e2db',
      inputBytes: 55_743,
      maxCalls: 4,
      maxInputBytes: 131_072,
      totalTokens: 20,
    },
  });
});
