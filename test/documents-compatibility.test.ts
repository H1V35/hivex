import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { expect, test } from 'bun:test';
import { Database } from 'bun:sqlite';

const parseJson = (text: string): unknown => JSON.parse(text);

const rustBinary = function rustBinary() {
  const configured = process.env.HIVEX_TEST_BINARY;
  const built = path.resolve(process.cwd(), 'target', 'debug', 'hivex');
  const binary = configured ?? (existsSync(built) ? built : undefined);
  if (binary === undefined) {
    throw new Error('HIVEX_TEST_BINARY or target/debug/hivex is required');
  }
  return binary;
};

const invoke = function invoke(argumentsList: string[]) {
  const result = spawnSync(rustBinary(), argumentsList, {
    encoding: 'utf-8',
    timeout: 10_000,
  });
  return {
    status: result.status,
    stderr: result.stderr,
    stdout: result.stdout === '' ? null : parseJson(result.stdout),
  };
};

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const arrayField = function arrayField(value: unknown, name: string): unknown[] {
  if (!isRecord(value) || !Array.isArray(value[name])) {
    throw new Error(`Expected array field ${name}`);
  }
  return value[name];
};

const objectField = function objectField(value: unknown, name: string): Record<string, unknown> {
  if (!isRecord(value) || !isRecord(value[name])) {
    throw new Error(`Expected object field ${name}`);
  }
  return value[name];
};

const stringField = function stringField(value: unknown, name: string): string {
  if (!isRecord(value) || typeof value[name] !== 'string') {
    throw new Error(`Expected string field ${name}`);
  }
  return value[name];
};

const output = function output(argumentsList: string[]): Record<string, unknown> {
  const result = invoke(argumentsList);
  if (result.status !== 0 || !isRecord(result.stdout)) {
    throw new Error(result.stderr || 'Expected Rust CLI JSON output');
  }
  return result.stdout;
};

const errorCode = function errorCode(argumentsList: string[]): string {
  const result = invoke(argumentsList);
  if (result.status === 0 || result.stderr === '') {
    throw new Error('Expected Rust CLI failure');
  }
  const diagnostic: unknown = parseJson(result.stderr);
  return stringField(objectField(diagnostic, 'error'), 'code');
};

const documentPaths = function documentPaths(value: unknown): string[] {
  return arrayField(value, 'documents').map((document) => stringField(document, 'path'));
};

const documentRecord = function documentRecord(
  value: unknown,
  pathName: string
): Record<string, unknown> {
  const document = arrayField(value, 'documents').find(
    (candidate) => isRecord(candidate) && candidate.path === pathName
  );
  if (!isRecord(document)) {
    throw new Error(`Expected document ${pathName}`);
  }
  return document;
};

const project = function project(files: Record<string, string>) {
  const root = mkdtempSync(path.join(tmpdir(), 'hivex-rust-contract-'));
  for (const [name, text] of Object.entries(files)) {
    const target = path.join(root, name);
    mkdirSync(path.dirname(target), { recursive: true });
    writeFileSync(target, text);
  }
  return {
    [Symbol.dispose]() {
      rmSync(root, { force: true, recursive: true });
    },
    root,
  };
};

test('Rust retains public help and diagnostic contracts', () => {
  using fixture = project({ 'a.md': '# One\n' });
  expect(output([])).toMatchObject({ application: 'hivex' });
  expect(output(['--help'])).toMatchObject({ application: 'hivex' });
  for (const [argumentsList, expectedCode] of [
    [['unknown'], 'INVALID_ARGUMENT'],
    [['--unknown'], 'READ_FAILED'],
    [['-x'], 'READ_FAILED'],
    [['-abc'], 'READ_FAILED'],
    [['unknown', '--unknown'], 'READ_FAILED'],
    [['sources', '--unknown'], 'INVALID_ARGUMENT'],
    [['sources', '--root'], 'INVALID_ARGUMENT'],
    [['sources', '--root', '--unknown'], 'INVALID_ARGUMENT'],
    [['sources', 'extra'], 'INVALID_ARGUMENT'],
    [['read'], 'INVALID_ARGUMENT'],
    [['read', 'missing.md'], 'SOURCE_NOT_FOUND'],
    [['read', 'a.md', '--from', '0'], 'INVALID_ARGUMENT'],
    [['read', 'a.md', '--max-bytes', '1'], 'OUTPUT_LIMIT'],
    [['prune', '--keep-caches', '-1'], 'READ_FAILED'],
    [['recover', '--keep-caches', '1'], 'INVALID_ARGUMENT'],
    [['prune', '--acknowledge-uncertain'], 'INVALID_ARGUMENT'],
    [['init', 'extra'], 'INVALID_ARGUMENT'],
  ] as const) {
    const result = invoke([...argumentsList, '--root', fixture.root]);
    expect(result.status).toBe(1);
    expect(result.stderr).not.toBe('');
    const diagnostic: unknown = parseJson(result.stderr);
    expect(stringField(objectField(diagnostic, 'error'), 'code')).toBe(expectedCode);
  }
  const longResult = invoke(['read', '😀'.repeat(200), '--root', fixture.root]);
  expect(longResult.status).toBe(1);
  expect(longResult.stderr).not.toBe('');
  const longDiagnostic: unknown = parseJson(longResult.stderr);
  expect(objectField(longDiagnostic, 'error')).toMatchObject({
    code: 'SOURCE_NOT_FOUND',
    details: { omitted: true },
    message: `Markdown source was not selected: ${'😀'.repeat(55)}`,
    messageTruncated: true,
  });
  expect(Buffer.byteLength(stringField(objectField(longDiagnostic, 'error'), 'message'))).toBe(254);
});

test('Rust preserves source hashes, Unicode ordering, Markdown metadata and local links', () => {
  using fixture = project({
    'b.md': '# Upper\n',
    'c.md': '# A\n',
    'docs/aliases.md': '---\ntitle: &title Wrong\nstatus: *title\n---\n# Alias fallback\n',
    'docs/anchor.md': '---\ntitle: &title Anchored title\nstatus: accepted\n---\n# Fallback\n',
    'docs/archive/old.md': '# Old\n',
    'docs/block.md':
      '---\ntitle: |-\n  *Literal title\nstatus: accepted # *not-an-alias\n---\n# Fallback\n',
    'docs/duplicates.md': '---\ntitle: One\ntitle: Two\n---\n# Fallback\n',
    'docs/guide.md':
      '---\r\ntitle: "Guide: decisions"\r\nstatus: accepted\r\n---\r\n# Heading\r\n[unicode](../%F0%9F%98%80.md#part) [ref][RULE] ![image](../c.md)\r\n[RULE]: ../b.md\r\n',
    'docs/heading.md': '# *Emphasis* and `code` ![alt](img.png) &amp; **strong**\n',
    'docs/nested-heading.md': '> # Quoted heading\n\n# Top heading\n',
    'hivex.json': JSON.stringify({ history: ['docs/archive/**'] }),
    'é.md': '# Accent\n',
    '𐀀.md': '# Supplementary\n',
    '😀.md': '# Emoji\n',
    '\u{E000}.md': '# Private Unicode\n',
  });
  const listed = output(['sources', '--root', fixture.root]);
  expect(documentPaths(listed)).toEqual([
    '😀.md',
    'b.md',
    'c.md',
    'docs/aliases.md',
    'docs/anchor.md',
    'docs/archive/old.md',
    'docs/block.md',
    'docs/duplicates.md',
    'docs/guide.md',
    'docs/heading.md',
    'docs/nested-heading.md',
    'é.md',
    '𐀀.md',
    '\u{E000}.md',
  ]);
  expect(documentRecord(listed, '\u{E000}.md')).toMatchObject({
    hash: 'f137c6a83e4fcb357a31725b9d6b6bc835551d1818856a419059f88f3f301396',
    historical: false,
    id: '\u{E000}.md',
    links: [],
    path: '\u{E000}.md',
    status: null,
    title: 'Private Unicode',
  });
  expect(documentRecord(listed, 'docs/aliases.md')).toMatchObject({
    links: [],
    status: null,
    title: 'Alias fallback',
  });
  expect(documentRecord(listed, 'docs/anchor.md')).toMatchObject({
    links: [],
    status: 'accepted',
    title: 'Anchored title',
  });
  expect(documentRecord(listed, 'docs/block.md')).toMatchObject({
    links: [],
    status: 'accepted',
    title: '*Literal title',
  });
  expect(documentRecord(listed, 'docs/duplicates.md')).toMatchObject({
    links: [],
    status: null,
    title: 'Fallback',
  });
  expect(documentRecord(listed, 'docs/guide.md')).toMatchObject({
    links: ['😀.md'],
    path: 'docs/guide.md',
    status: 'accepted',
    title: 'Guide: decisions',
  });
  expect(documentRecord(listed, 'docs/heading.md')).toMatchObject({
    links: [],
    status: null,
    title: 'Emphasis and code alt & strong',
  });
  expect(documentRecord(listed, 'docs/nested-heading.md')).toMatchObject({
    links: [],
    status: null,
    title: 'Top heading',
  });
  for (const [bytes, expected] of [
    ['500', ['😀.md']],
    ['1000', ['😀.md', 'b.md', 'c.md', 'docs/aliases.md']],
    [
      '2000',
      [
        '😀.md',
        'b.md',
        'c.md',
        'docs/aliases.md',
        'docs/anchor.md',
        'docs/archive/old.md',
        'docs/block.md',
        'docs/duplicates.md',
        'docs/guide.md',
      ],
    ],
  ] as const) {
    expect(
      documentPaths(output(['sources', '--root', fixture.root, '--max-bytes', bytes]))
    ).toEqual([...expected]);
  }
});

test('Rust preserves CR, CRLF and UTF-8 bounded source ranges', () => {
  using fixture = project({
    'empty.md': '',
    'mixed.md': '\u{FEFF}# Title\rLine 😀\r\nThird é\nLast\r\n',
    'trailing.md': 'a\n\n',
  });
  const mixed = output(['read', 'mixed.md', '--root', fixture.root]);
  expect(mixed).toMatchObject({
    lineEnd: 4,
    lineStart: 1,
    source: { path: 'mixed.md', title: 'Title' },
    text: '﻿# Title\rLine 😀\r\nThird é\nLast\r\n',
    truncated: false,
  });
  expect(
    output(['read', 'mixed.md', '--root', fixture.root, '--from', '1', '--to', '1'])
  ).toMatchObject({
    continuation: { from: 2, reason: 'range', to: 4 },
    lineEnd: 1,
    lineStart: 1,
    text: '﻿# Title',
  });
  expect(
    output(['read', 'mixed.md', '--root', fixture.root, '--from', '2', '--to', '3'])
  ).toMatchObject({
    continuation: { from: 4, reason: 'range', to: 4 },
    lineEnd: 3,
    lineStart: 2,
    text: 'Line 😀\r\nThird é',
  });
  expect(output(['read', 'empty.md', '--root', fixture.root])).toMatchObject({
    lineEnd: 1,
    lineStart: 1,
    source: { path: 'empty.md', title: 'empty.md' },
    text: '',
    truncated: false,
  });
  expect(output(['read', 'trailing.md', '--root', fixture.root])).toMatchObject({
    lineEnd: 2,
    lineStart: 1,
    source: { path: 'trailing.md', title: 'trailing.md' },
    text: 'a\n\n',
    truncated: false,
  });
  expect(errorCode(['read', 'mixed.md', '--root', fixture.root, '--max-bytes', '1'])).toBe(
    'OUTPUT_LIMIT'
  );
  for (const [bytes, expected] of [
    ['12', { from: 2, lineEnd: 1, text: '﻿# Title' }],
    ['24', { from: 3, lineEnd: 2, text: '﻿# Title\rLine 😀' }],
    ['36', { from: 4, lineEnd: 3, text: '﻿# Title\rLine 😀\r\nThird é' }],
  ] as const) {
    expect(
      output(['read', 'mixed.md', '--root', fixture.root, '--max-bytes', bytes])
    ).toMatchObject({
      continuation: { from: expected.from, reason: 'max-bytes', to: 4 },
      lineEnd: expected.lineEnd,
      text: expected.text,
      truncated: true,
    });
  }
});

test('Rust bounds deeply nested YAML metadata while retaining selected fields', () => {
  const metadata = ['---', 'title: Deep metadata', 'status: accepted', 'nested:'];
  for (let level = 1; level <= 24; level += 1) {
    metadata.push(`${'  '.repeat(level)}nested:`);
  }
  metadata.push(`${'  '.repeat(25)}unknown: value`, '---', '# Fallback heading', '');
  using fixture = project({ 'deep-metadata.md': metadata.join('\n') });
  const result = output(['sources', '--root', fixture.root]);
  expect(documentRecord(result, 'deep-metadata.md')).toMatchObject({
    status: 'accepted',
    title: 'Deep metadata',
  });
});

test('Rust preserves YAML key types when checking metadata duplicates', () => {
  using fixture = project({
    'numeric-duplicate.md': '---\n1: a\n1.0: b\ntitle: Main\n---\n# Fallback\n',
    'typed-string-key.md': '---\n1: a\n!!str 1: b\ntitle: Main\n---\n# Fallback\n',
  });
  const result = output(['sources', '--root', fixture.root]);
  expect(documentRecord(result, 'numeric-duplicate.md')).toMatchObject({
    status: null,
    title: 'Fallback',
  });
  expect(documentRecord(result, 'typed-string-key.md')).toMatchObject({
    status: null,
    title: 'Main',
  });
});

test('Rust preserves metadata around tagged scalars and object keys', () => {
  using fixture = project({
    'binary.md': '---\ntitle: !!binary SGVsbG8=\nstatus: accepted\n---\n# Fallback\n',
    'blank-title.md': '---\ntitle: "  "\nstatus: ""\n---\n# Fallback\n',
    'bom-title.md': '---\ntitle: "\u{FEFF}"\n---\n# Fallback\n',
    'empty-title.md': '---\ntitle: ""\n---\n# Fallback\n',
    'infinity.md': '---\ntitle: Main\nstatus: .inf\n---\n# Fallback\n',
    'invalid-timestamp.md': '---\ntitle: Main\nstatus: !!timestamp invalid\n---\n# Fallback\n',
    'large-hex.md': '---\ntitle: 0x10000000000000000\nstatus: accepted\n---\n# Fallback\n',
    'large-integer-keys.md':
      '---\n0x2000000000000101: one\n2305843009213694464: two\ntitle: Main\n---\n# Fallback\n',
    'object-keys.md': '---\n? {a: 1}\n? {a: 1}\ntitle: Main\n---\n# Fallback\n',
    'overflow.md': '---\ntitle: Main\nstatus: 1e999\n---\n# Fallback\n',
    'timestamp.md': '---\ntitle: Main\nstatus: !!timestamp 2024-01-01\n---\n# Fallback\n',
  });
  const result = output(['sources', '--root', fixture.root]);
  expect(documentRecord(result, 'binary.md')).toMatchObject({
    status: 'accepted',
    title: 'Fallback',
  });
  expect(documentRecord(result, 'blank-title.md')).toMatchObject({
    status: '',
    title: 'Fallback',
  });
  expect(documentRecord(result, 'bom-title.md')).toMatchObject({
    status: null,
    title: 'Fallback',
  });
  expect(documentRecord(result, 'empty-title.md')).toMatchObject({
    status: null,
    title: 'Fallback',
  });
  expect(documentRecord(result, 'infinity.md')).toMatchObject({
    status: null,
    title: 'Main',
  });
  expect(documentRecord(result, 'invalid-timestamp.md')).toMatchObject({
    status: null,
    title: 'Fallback',
  });
  expect(documentRecord(result, 'large-hex.md')).toMatchObject({
    status: 'accepted',
    title: 'Fallback',
  });
  expect(documentRecord(result, 'large-integer-keys.md')).toMatchObject({
    status: null,
    title: 'Fallback',
  });
  expect(documentRecord(result, 'object-keys.md')).toMatchObject({
    status: null,
    title: 'Main',
  });
  expect(documentRecord(result, 'overflow.md')).toMatchObject({
    status: null,
    title: 'Main',
  });
  expect(documentRecord(result, 'timestamp.md')).toMatchObject({
    status: null,
    title: 'Main',
  });
});

test('Rust follows the YAML core scalar patterns for ambiguous metadata', () => {
  using fixture = project({
    'ambiguous-scalars.md':
      '---\ntitle: 1_000\nstatus: tRuE\nbinary: 0b101\nhex: +0x10\nnan: +.nan\ninfinity: .iNF\nnullish: nUlL\n---\n# Fallback\n',
    'invalid-core-tags.md': '---\ntitle: !!float 2\nstatus: !!bool tRuE\n---\n# Fallback\n',
  });
  const result = output(['sources', '--root', fixture.root]);
  expect(documentRecord(result, 'ambiguous-scalars.md')).toMatchObject({
    status: 'tRuE',
    title: '1_000',
  });
  expect(documentRecord(result, 'invalid-core-tags.md')).toMatchObject({
    status: 'tRuE',
    title: '2',
  });
});

test('Rust accepts one hundred YAML nesting levels around metadata', () => {
  const metadata = ['---', 'title: Deep metadata', 'status: accepted', 'nested:'];
  for (let level = 1; level <= 100; level += 1) {
    metadata.push(`${'  '.repeat(level)}nested:`);
  }
  metadata.push(`${'  '.repeat(101)}unknown: value`, '---', '# Fallback heading', '');
  using fixture = project({ 'deep-metadata-100.md': metadata.join('\n') });
  expect(
    documentRecord(output(['sources', '--root', fixture.root]), 'deep-metadata-100.md')
  ).toMatchObject({
    status: 'accepted',
    title: 'Deep metadata',
  });
});

test('Rust preserves metadata through deeply nested flow collections', () => {
  const flow = `${'['.repeat(1000)}value${']'.repeat(1000)}`;
  using fixture = project({
    'flow.md': `---\ntitle: Flow metadata\nstatus: accepted\nnested: ${flow}\n---\n# Fallback\n`,
  });
  expect(documentRecord(output(['sources', '--root', fixture.root]), 'flow.md')).toMatchObject({
    status: 'accepted',
    title: 'Flow metadata',
  });
});

test('Rust keeps explicit directory, brace, negation and exclusion glob behavior', () => {
  using fixture = project({
    '.decisions/e.md': '# E\n',
    '.git/secret.md': '# Never\n',
    'a.md': '# A\n',
    'docs/b.md': '# B\n',
    'docs/c.markdown': '# C\n',
    'docs/deep/d.md': '# D\n',
    'vendor/f.md': '# F\n',
  });
  for (const [config, expected] of [
    [
      { include: ['**/*.{md,markdown}'] },
      ['a.md', 'docs/b.md', 'docs/c.markdown', 'docs/deep/d.md'],
    ],
    [
      { include: ['docs/?.md', '.decisions/**', 'vendor/**'] },
      ['.decisions/e.md', 'docs/b.md', 'vendor/f.md'],
    ],
    [{ include: ['docs/[bc].*'] }, ['docs/b.md', 'docs/c.markdown']],
    [{ include: ['!!a.md'] }, ['a.md']],
    [{ include: ['!!!a.md'] }, ['docs/b.md', 'docs/c.markdown', 'docs/deep/d.md']],
    [{ exclude: ['!!docs/**'] }, ['a.md']],
    [{ exclude: ['!docs/deep/**'] }, ['docs/deep/d.md']],
    [{ exclude: ['docs/*'] }, ['a.md', 'docs/deep/d.md']],
    [{ exclude: ['docs/**'], history: ['docs/deep/**'] }, ['a.md']],
    [{ include: ['docs\\**\\*.md'] }, ['docs/b.md', 'docs/deep/d.md']],
    [{ include: ['.git/**'] }, []],
  ] as const) {
    writeFileSync(path.join(fixture.root, 'hivex.json'), JSON.stringify(config));
    expect(documentPaths(output(['sources', '--root', fixture.root]))).toEqual([...expected]);
  }
});

test('Rust maintenance accepts SQL v1 fixtures without rewriting retained rows', () => {
  for (const name of [
    'knowledge-cache-v1.sql',
    'knowledge-update-cache-v1.sql',
    'knowledge-multiround-cache-v1.sql',
  ]) {
    using fixture = project({});
    mkdirSync(path.join(fixture.root, '.hivex'));
    using database = new Database(path.join(fixture.root, '.hivex/knowledge.sqlite'));
    database.run(readFileSync(path.join(process.cwd(), 'test', 'fixtures', name), 'utf-8'));
    const before = database.query('SELECT * FROM work').all();
    const graph = database.query('SELECT * FROM graph').all();
    const cache = database.query('SELECT * FROM model_cache').all();
    expect(invoke(['recover', '--root', fixture.root]).status).toBe(0);
    expect(
      invoke(['prune', '--keep-completed', '4096', '--keep-caches', '4096', '--root', fixture.root])
        .status
    ).toBe(0);
    expect(database.query('SELECT * FROM work').all()).toEqual(before);
    expect(database.query('SELECT * FROM graph').all()).toEqual(graph);
    expect(database.query('SELECT * FROM model_cache').all()).toEqual(cache);
  }
});

test('Rust keeps CJK source ordering and page boundaries', () => {
  using fixture = project({
    '中.md': '# First common ideograph\n',
    '文.md': '# Second common ideograph\n',
    '𠀀.md': '# Supplementary ideograph\n',
  });
  const listed = output(['sources', '--root', fixture.root]);
  expect(documentPaths(listed)).toEqual(['𠀀.md', '中.md', '文.md']);
  expect(documentPaths(output(['sources', '--root', fixture.root, '--limit', '1']))).toEqual([
    '𠀀.md',
  ]);
});

test('Rust queries retain current and historical graph evidence', () => {
  using fixture = project({
    'current.md': '# Safety\r\nKeep the bounded budget.\r\n[old](old.md)\r\n',
    'hivex.json': JSON.stringify({ history: ['old.md'] }),
    'old.md': '# Safety before migration\nA historical alternative.\n',
    'other.md': '# Persistence\nPreserve existing data.\n',
  });
  const versions = new Map([
    ['current.md', 'dc8edc855777a957a8ef347875ea0c9bb6b25d6261cca7904a2a71505f7b329e'],
    ['old.md', '024cf5318a11e42fd107e3ec7506c026205065677f840eff63903bb7ad559183'],
    ['other.md', '526f04aecb719eaef8fc6950e4bc83f14ded44d1bc74ed8c396f075553a0da4e'],
  ]);
  const retainedDecision = function retainedDecision(document: string, index: number) {
    return {
      batch: 'fixture',
      conditions: [],
      document,
      exceptions: [],
      id: `decision-${index}`,
      kind: 'decision',
      lineEnd: 2,
      lineStart: 2,
      localId: `local-${index}`,
      quality: index === 1 ? 'unchecked' : 'checked',
      reason: 'Preserve intent',
      status: 'current',
      text: index === 1 ? 'Preserve existing data.' : 'Keep the bounded budget.',
      version: versions.get(document),
    };
  };
  const decisions = ['current.md', 'other.md', 'old.md'].map(retainedDecision);
  const graph = {
    decisions,
    documents: Object.fromEntries(versions),
    relationships: [
      {
        batch: 'fixture',
        evidence: [
          {
            document: 'current.md',
            lineEnd: 2,
            lineStart: 2,
            version: versions.get('current.md'),
          },
        ],
        from: 'decision-0',
        id: 'relationship-0',
        localId: 'edge',
        quality: 'checked',
        reason: 'Budget protects persistence',
        to: 'decision-1',
        type: 'requires',
      },
    ],
    units: {},
    version: 1,
    warnings: ['Retained uncertainty'],
  };
  mkdirSync(path.join(fixture.root, '.hivex'));
  using database = new Database(path.join(fixture.root, '.hivex', 'knowledge.sqlite'));
  database.run('CREATE TABLE graph(id INTEGER PRIMARY KEY, data TEXT NOT NULL)');
  database.run('CREATE TABLE work(id TEXT PRIMARY KEY, kind TEXT, key TEXT, data TEXT)');
  database.run('CREATE TABLE model_cache(key TEXT PRIMARY KEY, value TEXT)');
  database.run('INSERT INTO graph VALUES(1,?)', [JSON.stringify(graph)]);
  for (const argumentsList of [
    ['status'],
    ['search', 'budget'],
    ['search', 'ＢＵＤＧＥＴ'],
    ['search', 'safety', '--source', 'old.md'],
    ['neighbors', 'decision-0'],
    ['neighbors', 'decision-0', '--limit', '1'],
    ['search', 'missing'],
    ['search', 'budget', '--source', 'other.md', '--source', 'old.md'],
  ]) {
    expect(output([...argumentsList, '--root', fixture.root])).toMatchObject({
      command: argumentsList[0],
    });
  }
  writeFileSync(path.join(fixture.root, 'other.md'), '# Changed\nDifferent current content.\n');
  expect(output(['status', '--root', fixture.root])).toMatchObject({
    command: 'status',
    selectedDocuments: 3,
  });
  expect(output(['neighbors', 'decision-0', '--root', fixture.root])).toMatchObject({
    command: 'neighbors',
  });
});
