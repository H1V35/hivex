import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { expect, test } from 'bun:test';
import { Database } from 'bun:sqlite';

const rustBinary = process.env.HIVEX_TEST_BINARY;
const compatibility = test.skipIf(rustBinary === undefined);

const invoke = function invoke(argumentsList: string[], isRust = false) {
  const executable = isRust ? rustBinary : process.execPath;
  if (executable === undefined) {
    throw new Error('HIVEX_TEST_BINARY is required');
  }
  const prefix = isRust ? [] : [path.join(import.meta.dirname, 'cli.ts')];
  const result = spawnSync(executable, [...prefix, ...argumentsList], {
    encoding: 'utf-8',
    timeout: 10_000,
  });
  return {
    status: result.status,
    stderr: result.stderr ? (JSON.parse(result.stderr) as unknown) : null,
    stdout: result.stdout ? (JSON.parse(result.stdout) as unknown) : null,
  };
};

const compare = function compare(argumentsList: string[]) {
  expect(invoke(argumentsList, true)).toEqual(invoke(argumentsList));
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

compatibility('Rust retains public help and diagnostic contracts', () => {
  using fixture = project({ 'a.md': '# One\n' });
  compare([]);
  for (const argumentsList of [
    [],
    ['--help'],
    ['unknown'],
    ['--unknown'],
    ['-x'],
    ['-abc'],
    ['unknown', '--unknown'],
    ['sources', '--unknown'],
    ['sources', '--root'],
    ['sources', '--root', '--unknown'],
    ['sources', 'extra'],
    ['read'],
    ['read', 'missing.md'],
    ['read', '😀'.repeat(200)],
    ['read', 'a.md', '--from', '0'],
    ['read', 'a.md', '--max-bytes', '1'],
    ['prune', '--keep-caches', '-1'],
    ['recover', '--keep-caches', '1'],
    ['prune', '--acknowledge-uncertain'],
    ['init', 'extra'],
  ]) {
    compare([...argumentsList, '--root', fixture.root]);
  }
});

compatibility(
  'Rust preserves source hashes, Unicode ordering, Markdown metadata and local links',
  () => {
    using fixture = project({
      'A.md': '# Upper\n',
      'a.md': '# A\n',
      'docs/aliases.md': '---\ntitle: &title Wrong\nstatus: *title\n---\n# Alias fallback\n',
      'docs/anchor.md': '---\ntitle: &title Anchored title\nstatus: accepted\n---\n# Fallback\n',
      'docs/archive/old.md': '# Old\n',
      'docs/block.md':
        '---\ntitle: |-\n  *Literal title\nstatus: accepted # *not-an-alias\n---\n# Fallback\n',
      'docs/duplicates.md': '---\ntitle: One\ntitle: Two\n---\n# Fallback\n',
      'docs/guide.md':
        '---\r\ntitle: "Guide: decisions"\r\nstatus: accepted\r\n---\r\n# Heading\r\n[unicode](../%F0%9F%98%80.md#part) [ref][RULE] ![image](../a.md)\r\n[RULE]: ../A.md\r\n',
      'docs/heading.md': '# *Emphasis* and `code` ![alt](img.png) &amp; **strong**\n',
      'docs/nested-heading.md': '> # Quoted heading\n\n# Top heading\n',
      'hivex.json': JSON.stringify({ history: ['docs/archive/**'] }),
      'é.md': '# Accent\n',
      '𐀀.md': '# Supplementary\n',
      '😀.md': '# Emoji\n',
      '\u{E000}.md': '# Private Unicode\n',
    });
    compare(['sources', '--root', fixture.root]);
    for (const bytes of ['500', '1000', '2000']) {
      compare(['sources', '--root', fixture.root, '--max-bytes', bytes]);
    }
  }
);

compatibility('Rust preserves CR, CRLF and UTF-8 bounded source ranges', () => {
  using fixture = project({
    'empty.md': '',
    'mixed.md': '\u{FEFF}# Title\rLine 😀\r\nThird é\nLast\r\n',
    'trailing.md': 'a\n\n',
  });
  for (const name of ['mixed.md', 'empty.md', 'trailing.md']) {
    compare(['read', name, '--root', fixture.root]);
    compare(['read', name, '--root', fixture.root, '--from', '1', '--to', '1']);
  }
  for (const bytes of ['1', '12', '24', '36']) {
    compare(['read', 'mixed.md', '--root', fixture.root, '--max-bytes', bytes]);
  }
  compare(['read', 'mixed.md', '--root', fixture.root, '--from', '2', '--to', '3']);
});

compatibility('Rust bounds deeply nested YAML metadata while retaining selected fields', () => {
  const metadata = ['---', 'title: Deep metadata', 'status: accepted', 'nested:'];
  for (let level = 1; level <= 24; level += 1) {
    metadata.push(`${'  '.repeat(level)}nested:`);
  }
  metadata.push(`${'  '.repeat(25)}unknown: value`, '---', '# Fallback heading', '');
  using fixture = project({ 'deep-metadata.md': metadata.join('\n') });
  compare(['sources', '--root', fixture.root]);
  expect(invoke(['sources', '--root', fixture.root], true)).toMatchObject({
    status: 0,
    stdout: { documents: [{ status: 'accepted', title: 'Deep metadata' }] },
  });
});

compatibility('Rust preserves YAML key types when checking metadata duplicates', () => {
  using fixture = project({
    'numeric-duplicate.md': '---\n1: a\n1.0: b\ntitle: Main\n---\n# Fallback\n',
    'typed-string-key.md': '---\n1: a\n!!str 1: b\ntitle: Main\n---\n# Fallback\n',
  });
  compare(['sources', '--root', fixture.root]);
});

compatibility('Rust accepts one hundred YAML nesting levels around metadata', () => {
  const metadata = ['---', 'title: Deep metadata', 'status: accepted', 'nested:'];
  for (let level = 1; level <= 100; level += 1) {
    metadata.push(`${'  '.repeat(level)}nested:`);
  }
  metadata.push(`${'  '.repeat(101)}unknown: value`, '---', '# Fallback heading', '');
  using fixture = project({ 'deep-metadata-100.md': metadata.join('\n') });
  compare(['sources', '--root', fixture.root]);
});

compatibility('Rust preserves metadata through deeply nested flow collections', () => {
  const flow = `${'['.repeat(1000)}value${']'.repeat(1000)}`;
  using fixture = project({
    'flow.md': `---\ntitle: Flow metadata\nstatus: accepted\nnested: ${flow}\n---\n# Fallback\n`,
  });
  compare(['sources', '--root', fixture.root]);
  expect(invoke(['sources', '--root', fixture.root], true)).toMatchObject({
    status: 0,
    stdout: { documents: [{ status: 'accepted', title: 'Flow metadata' }] },
  });
});

compatibility('Rust keeps explicit directory, brace, negation and exclusion glob behavior', () => {
  using fixture = project({
    '.decisions/e.md': '# E\n',
    '.git/secret.md': '# Never\n',
    'a.md': '# A\n',
    'docs/b.md': '# B\n',
    'docs/c.markdown': '# C\n',
    'docs/deep/d.md': '# D\n',
    'vendor/f.md': '# F\n',
  });
  for (const config of [
    { include: ['**/*.{md,markdown}'] },
    { include: ['docs/?.md', '.decisions/**', 'vendor/**'] },
    { include: ['docs/[bc].*'] },
    { include: ['!!a.md'] },
    { include: ['!!!a.md'] },
    { exclude: ['!!docs/**'] },
    { exclude: ['!docs/deep/**'] },
    { exclude: ['docs/*'] },
    { exclude: ['docs/**'], history: ['docs/deep/**'] },
    { include: ['docs\\**\\*.md'] },
    { include: ['.git/**'] },
  ]) {
    writeFileSync(path.join(fixture.root, 'hivex.json'), JSON.stringify(config));
    compare(['sources', '--root', fixture.root]);
  }
});

compatibility('Rust maintenance accepts SQL v1 fixtures without rewriting retained rows', () => {
  for (const name of [
    'knowledge-cache-v1.sql',
    'knowledge-update-cache-v1.sql',
    'knowledge-multiround-cache-v1.sql',
  ]) {
    using fixture = project({});
    mkdirSync(path.join(fixture.root, '.hivex'));
    using database = new Database(path.join(fixture.root, '.hivex/knowledge.sqlite'));
    database.run(readFileSync(path.join(import.meta.dirname, '../test/fixtures', name), 'utf-8'));
    const before = database.query('SELECT * FROM work').all();
    const graph = database.query('SELECT * FROM graph').all();
    const cache = database.query('SELECT * FROM model_cache').all();
    expect(invoke(['recover', '--root', fixture.root], true).status).toBe(0);
    expect(
      invoke(
        ['prune', '--keep-completed', '4096', '--keep-caches', '4096', '--root', fixture.root],
        true
      ).status
    ).toBe(0);
    expect(database.query('SELECT * FROM work').all()).toEqual(before);
    expect(database.query('SELECT * FROM graph').all()).toEqual(graph);
    expect(database.query('SELECT * FROM model_cache').all()).toEqual(cache);
  }
});

compatibility('Rust keeps CJK source ordering and page boundaries', () => {
  using fixture = project({
    '中.md': '# First common ideograph\n',
    '文.md': '# Second common ideograph\n',
    '𠀀.md': '# Supplementary ideograph\n',
  });
  compare(['sources', '--root', fixture.root]);
  compare(['sources', '--root', fixture.root, '--limit', '1']);
});
