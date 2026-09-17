import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import nodePath from 'node:path';
import { expect, test } from 'bun:test';

type JsonRecord = Record<string, unknown>;

const isRecord = (value: unknown): value is JsonRecord =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const parseJson = (text: string): unknown => JSON.parse(text);

const arrayField = (value: unknown, name: string): unknown[] => {
  if (!isRecord(value) || !Array.isArray(value[name])) {
    throw new Error(`Expected array field ${name}`);
  }
  return value[name];
};

const objectField = (value: unknown, name: string): JsonRecord => {
  if (!isRecord(value) || !isRecord(value[name])) {
    throw new Error(`Expected object field ${name}`);
  }
  return value[name];
};

const stringField = (value: unknown, name: string): string => {
  if (!isRecord(value) || typeof value[name] !== 'string') {
    throw new Error(`Expected string field ${name}`);
  }
  return value[name];
};

const recordAt = (values: readonly unknown[], index: number): JsonRecord => {
  const value = values.at(index);
  if (!isRecord(value)) {
    throw new Error(`Expected an object at index ${index}`);
  }
  return value;
};

interface TemporaryDirectory extends Disposable {
  root: string;
}

const temporaryDirectory = function temporaryDirectory(prefix: string): TemporaryDirectory {
  const root = mkdtempSync(nodePath.join(tmpdir(), prefix));
  return {
    [Symbol.dispose]() {
      rmSync(root, { force: true, recursive: true });
    },
    root,
  };
};

const documentPaths = function documentPaths(value: unknown): string[] {
  return arrayField(value, 'documents').map((document) => stringField(document, 'path'));
};

const rustBinary = function rustBinary() {
  const configured = process.env.HIVEX_TEST_BINARY;
  const built = nodePath.resolve(process.cwd(), 'target', 'debug', 'hivex');
  const binary = configured ?? (existsSync(built) ? built : undefined);
  if (binary === undefined) {
    throw new Error('HIVEX_TEST_BINARY or target/debug/hivex is required');
  }
  return binary;
};

const documentCommand = function documentCommand(cliArguments: string[]): unknown {
  const result = spawnSync(rustBinary(), cliArguments, {
    encoding: 'utf-8',
    timeout: 10_000,
  });
  if (result.status !== 0) {
    throw new Error(result.stderr || 'Rust CLI command failed');
  }
  if (result.stdout === '') {
    throw new Error('Expected Rust CLI JSON output');
  }
  return parseJson(result.stdout);
};

test('sources accepts BOM configuration and lists current nested Markdown without returning text', () => {
  using directory = temporaryDirectory('hivex-documents-');
  const { root } = directory;
  writeFileSync(nodePath.join(root, 'hivex.json'), '\u{FEFF}{"include":["**/*.md"]}');
  mkdirSync(nodePath.join(root, 'packages', 'core'), { recursive: true });
  writeFileSync(
    nodePath.join(root, 'packages', 'core', 'decision.md'),
    '# Local choice\n\nUse the package boundary.\n',
    'utf-8'
  );

  const result = documentCommand(['sources', '--root', root]);

  expect(result).toMatchObject({
    command: 'sources',
    documents: [
      {
        links: [],
        path: 'packages/core/decision.md',
        status: null,
        title: 'Local choice',
      },
    ],
    warnings: [],
  });
  expect(recordAt(arrayField(result, 'documents'), 0)).not.toHaveProperty('text');
});

test('selects nested documents, preserves exact text, and resolves declared local links', () => {
  using directory = temporaryDirectory('hivex-documents-');
  const { root } = directory;
  mkdirSync(nodePath.join(root, 'packages', 'core'), { recursive: true });
  mkdirSync(nodePath.join(root, 'packages', 'vendor'), { recursive: true });
  mkdirSync(nodePath.join(root, '.private'), { recursive: true });
  const exactText =
    '---\ntitle: Package choice\nstatus: draft\n---\n\nSee [module](../module.md).\r\n';
  writeFileSync(nodePath.join(root, 'packages', 'core', 'decision.md'), exactText, 'utf-8');
  writeFileSync(nodePath.join(root, 'packages', 'module.md'), '# Module\n', 'utf-8');
  writeFileSync(nodePath.join(root, 'packages', 'vendor', 'ignored.md'), '# Vendor\n', 'utf-8');
  writeFileSync(nodePath.join(root, '.private', 'ignored.md'), '# Private\n', 'utf-8');

  const listed = documentCommand(['sources', '--root', root]);
  expect(stringField(listed, 'origin')).toBe('current-worktree');
  expect(documentPaths(listed)).toEqual(['packages/core/decision.md', 'packages/module.md']);
  expect(recordAt(arrayField(listed, 'documents'), 0)).toMatchObject({
    hash: 'c2e968f15088b7999576c97d666cd94d938b6160777a67a14036763aa03b9e30',
    links: ['packages/module.md'],
    status: 'draft',
    title: 'Package choice',
  });

  const opened = documentCommand(['read', 'packages/core/decision.md', '--root', root]);
  expect(objectField(opened, 'source')).not.toHaveProperty('text');
  expect(stringField(opened, 'text')).toBe(exactText);
  expect(isRecord(opened) ? opened.continuation : undefined).toBeNull();

  writeFileSync(nodePath.join(root, 'packages', 'module.md'), '# Module changed\n', 'utf-8');
  const changed = documentCommand(['sources', '--root', root]);
  expect(stringField(changed, 'snapshot')).not.toBe(stringField(listed, 'snapshot'));
});

test('accepts a custom explicit layout and rejects legacy collections without migration', () => {
  const root = mkdtempSync(nodePath.join(tmpdir(), 'hivex-documents-'));
  try {
    mkdirSync(nodePath.join(root, 'knowledge'), { recursive: true });
    mkdirSync(nodePath.join(root, 'docs'), { recursive: true });
    writeFileSync(nodePath.join(root, 'knowledge', 'guide.md'), '# Guide\n', 'utf-8');
    writeFileSync(nodePath.join(root, 'docs', 'not-selected.md'), '# Other\n', 'utf-8');
    writeFileSync(
      nodePath.join(root, 'hivex.json'),
      JSON.stringify({
        exclude: ['knowledge/archive/**'],
        include: ['knowledge/**/*.md'],
      }),
      'utf-8'
    );
    const selected = documentCommand(['sources', '--root', root]);
    expect(documentPaths(selected)).toEqual(['knowledge/guide.md']);

    writeFileSync(
      nodePath.join(root, 'hivex.json'),
      JSON.stringify({
        collections: [{ id: 'legacy', include: ['docs/**/*.md'] }],
        version: 1,
      }),
      'utf-8'
    );
    expect(() => documentCommand(['sources', '--root', root])).toThrow(/legacy collections/u);
  } finally {
    rmSync(root, { force: true, recursive: true });
  }
});

test('declares historical globs for focused reads without treating them as current sources', () => {
  const root = mkdtempSync(nodePath.join(tmpdir(), 'hivex-history-'));
  try {
    mkdirSync(nodePath.join(root, 'docs', 'archive'), { recursive: true });
    writeFileSync(
      nodePath.join(root, 'docs', 'current.md'),
      '# Current\n\nCurrent rule.\n',
      'utf-8'
    );
    writeFileSync(
      nodePath.join(root, 'docs', 'archive', 'replaced.md'),
      '# Replaced\n\nHistorical rule.\n',
      'utf-8'
    );
    writeFileSync(
      nodePath.join(root, 'hivex.json'),
      JSON.stringify({
        history: ['docs/archive/**/*.md'],
        include: ['docs/**/*.md'],
      }),
      'utf-8'
    );

    const listed = documentCommand(['sources', '--root', root]);
    expect(arrayField(listed, 'documents')).toEqual([
      expect.objectContaining({
        historical: true,
        path: 'docs/archive/replaced.md',
      }),
      expect.objectContaining({ historical: false, path: 'docs/current.md' }),
    ]);

    const opened = documentCommand(['read', 'docs/archive/replaced.md', '--root', root]);
    expect(objectField(opened, 'source')).toMatchObject({
      historical: true,
      path: 'docs/archive/replaced.md',
    });
    expect(stringField(opened, 'text')).toBe('# Replaced\n\nHistorical rule.\n');
  } finally {
    rmSync(root, { force: true, recursive: true });
  }
});

test('a large historical archive does not displace current sources at the document limit', () => {
  using directory = temporaryDirectory('hivex-history-limit-');
  const { root } = directory;
  mkdirSync(nodePath.join(root, 'archive'));
  for (let index = 0; index < 2048; index += 1) {
    writeFileSync(nodePath.join(root, 'archive', `${index}.md`), '# Old rule\n');
  }
  writeFileSync(nodePath.join(root, 'current.md'), '# Current\n\nCurrent rule.\n');
  writeFileSync(
    nodePath.join(root, 'hivex.json'),
    JSON.stringify({ history: ['archive/**/*.md'] })
  );
  expect(documentCommand(['read', 'current.md', '--root', root])).toMatchObject({
    source: { historical: false, path: 'current.md' },
    text: '# Current\n\nCurrent rule.\n',
  });
});

test('does not make an excluded historical source readable', () => {
  const root = mkdtempSync(nodePath.join(tmpdir(), 'hivex-history-excluded-'));
  try {
    mkdirSync(nodePath.join(root, 'docs', 'archive'), { recursive: true });
    writeFileSync(nodePath.join(root, 'docs', 'archive', 'replaced.md'), '# Replaced\n', 'utf-8');
    writeFileSync(
      nodePath.join(root, 'hivex.json'),
      JSON.stringify({
        exclude: ['docs/archive/**'],
        history: ['docs/archive/**/*.md'],
        include: ['docs/**/*.md'],
      }),
      'utf-8'
    );

    expect(arrayField(documentCommand(['sources', '--root', root]), 'documents')).toEqual([]);
    expect(() => documentCommand(['read', 'docs/archive/replaced.md', '--root', root])).toThrow(
      /was not selected/u
    );
  } finally {
    rmSync(root, { force: true, recursive: true });
  }
});

test('prunes excluded directories before inspecting their files and symlinks', () => {
  const root = mkdtempSync(nodePath.join(tmpdir(), 'hivex-excluded-directory-'));
  try {
    mkdirSync(nodePath.join(root, 'app', 'ios'), { recursive: true });
    writeFileSync(nodePath.join(root, 'app', 'docs.md'), '# Legitimate\n', 'utf-8');
    writeFileSync(nodePath.join(root, 'app', 'ios', 'README.md'), '# Generated\n', 'utf-8');
    symlinkSync(nodePath.join(root, 'missing.md'), nodePath.join(root, 'app', 'ios', 'broken.md'));
    writeFileSync(
      nodePath.join(root, 'hivex.json'),
      JSON.stringify({ exclude: ['app/ios/**'], include: ['app/**/*.md'] }),
      'utf-8'
    );

    const result = documentCommand(['sources', '--root', root]);
    expect(documentPaths(result)).toEqual(['app/docs.md']);
    expect(arrayField(result, 'warnings')).toEqual([]);
  } finally {
    rmSync(root, { force: true, recursive: true });
  }
});

test.each([
  {
    exclude: 'app/ios/*',
    expected: ['app/docs.md', 'app/ios/nested/guide.md'],
  },
  { exclude: '!app/ios/nested/**', expected: ['app/ios/nested/guide.md'] },
])('keeps descendants not excluded by $exclude', ({ exclude, expected }) => {
  const root = mkdtempSync(nodePath.join(tmpdir(), 'hivex-file-exclusion-'));
  try {
    mkdirSync(nodePath.join(root, 'app', 'ios', 'nested'), { recursive: true });
    writeFileSync(nodePath.join(root, 'app', 'docs.md'), '# Ordinary\n', 'utf-8');
    writeFileSync(nodePath.join(root, 'app', 'ios', 'top.md'), '# Top\n', 'utf-8');
    writeFileSync(nodePath.join(root, 'app', 'ios', 'nested', 'guide.md'), '# Nested\n', 'utf-8');
    symlinkSync(
      nodePath.join(root, 'missing.md'),
      nodePath.join(root, 'app', 'ios', 'nested', 'broken.md')
    );
    writeFileSync(
      nodePath.join(root, 'hivex.json'),
      JSON.stringify({ exclude: [exclude], include: ['app/**/*.md'] }),
      'utf-8'
    );

    const result = documentCommand(['sources', '--root', root]);
    expect(documentPaths(result)).toEqual([...expected]);
    expect(arrayField(result, 'warnings')).toEqual([
      { message: 'Skipped symbolic link', path: 'app/ios/nested/broken.md' },
    ]);
  } finally {
    rmSync(root, { force: true, recursive: true });
  }
});

test('reports invalid UTF-8 and skips symlinked sources without reading outside the root', () => {
  using rootDirectory = temporaryDirectory('hivex-documents-');
  using outsideDirectory = temporaryDirectory('hivex-documents-outside-');
  const { root } = rootDirectory;
  const { root: outside } = outsideDirectory;
  mkdirSync(nodePath.join(root, 'docs'), { recursive: true });
  const outsideFile = nodePath.join(outside, 'secret.md');
  writeFileSync(outsideFile, '# Secret\n', 'utf-8');
  symlinkSync(outsideFile, nodePath.join(root, 'docs', 'outside.md'));
  writeFileSync(nodePath.join(root, 'docs', 'invalid.md'), Buffer.from([195, 40]));

  const result = documentCommand(['sources', '--root', root]);
  expect(arrayField(result, 'documents')).toEqual([]);
  const warnings = arrayField(result, 'warnings');
  expect(warnings).toContainEqual({
    message: 'Skipped symbolic link',
    path: 'docs/outside.md',
  });
  const invalidWarning = warnings.find(
    (warning) => isRecord(warning) && warning.path === 'docs/invalid.md'
  );
  if (!isRecord(invalidWarning)) {
    throw new Error('Expected an invalid UTF-8 warning');
  }
  expect(stringField(invalidWarning, 'message')).toContain(String.raw`UTF-8`);
});

test('returns explicit continuation for a byte-limited read and rejects invalid ranges', () => {
  const root = mkdtempSync(nodePath.join(tmpdir(), 'hivex-documents-'));
  try {
    writeFileSync(nodePath.join(root, 'notes.md'), '# One\nline 2\nline 3\nline 4\n', 'utf-8');

    const limited = documentCommand(['read', 'notes.md', '--root', root, '--max-bytes', '12']);
    expect(limited).toMatchObject({
      continuation: { from: 3, reason: 'max-bytes' },
      lineEnd: 2,
      lineStart: 1,
      text: '# One\nline 2',
      truncated: true,
    });

    const ranged = documentCommand([
      'read',
      'notes.md',
      '--root',
      root,
      '--from',
      '2',
      '--to',
      '3',
    ]);
    expect(ranged).toMatchObject({
      continuation: { from: 4, reason: 'range' },
      lineEnd: 3,
      lineStart: 2,
      text: 'line 2\nline 3',
    });
    expect(
      documentCommand.bind(null, ['read', 'notes.md', '--root', root, '--from', '4', '--to', '2'])
    ).toThrow(/outside the source/u);
  } finally {
    rmSync(root, { force: true, recursive: true });
  }
});

test('keeps readable Markdown with custom metadata instead of requiring a particular schema', () => {
  const root = mkdtempSync(nodePath.join(tmpdir(), 'hivex-custom-doc-'));
  try {
    writeFileSync(
      nodePath.join(root, 'policy.md'),
      '---\nstatus: [reviewed, adopted]\n---\n# Policy\n\nRetain requests for seven days.\n'
    );
    expect(documentCommand(['sources', '--root', root])).toMatchObject({
      documents: [
        expect.objectContaining({
          id: 'policy.md',
          status: null,
          title: 'Policy',
        }),
      ],
    });
  } finally {
    rmSync(root, { force: true, recursive: true });
  }
});

test('pages source discovery and rejects a continuation after the selected documents change', () => {
  using directory = temporaryDirectory('hivex-pages-');
  const { root } = directory;
  for (const name of ['a.md', 'b.md', 'c.md']) {
    writeFileSync(nodePath.join(root, name), '# Decision\n');
  }
  const first = documentCommand(['sources', '--root', root, '--limit', '1']);
  expect(arrayField(first, 'documents').map((document) => stringField(document, 'id'))).toEqual([
    'a.md',
  ]);
  const second = documentCommand([
    'sources',
    '--root',
    root,
    '--limit',
    '1',
    '--cursor',
    stringField(first, 'continuation'),
  ]);
  expect(arrayField(second, 'documents').map((document) => stringField(document, 'id'))).toEqual([
    'b.md',
  ]);
  writeFileSync(nodePath.join(root, 'c.md'), '# Changed decision\n');
  expect(
    documentCommand.bind(null, [
      'sources',
      '--root',
      root,
      '--cursor',
      stringField(first, 'continuation'),
    ])
  ).toThrow(/snapshot/u);
});
