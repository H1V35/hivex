import { expect, test } from 'bun:test';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';

function documentCommand(args: string[]): unknown {
  const result = spawnSync(process.execPath, [join(import.meta.dirname, 'cli.ts'), ...args], {
    encoding: 'utf8',
    timeout: 10000,
  });
  if (result.status !== 0) throw new Error(result.stderr);
  return JSON.parse(result.stdout);
}

test('sources accepts BOM configuration and lists current nested Markdown without returning text', () => {
  const root = mkdtempSync(join(tmpdir(), 'hivex-documents-'));
  try {
    writeFileSync(join(root, 'hivex.json'), '\uFEFF{"include":["**/*.md"]}');
    mkdirSync(join(root, 'packages', 'core'), { recursive: true });
    writeFileSync(
      join(root, 'packages', 'core', 'decision.md'),
      '# Local choice\n\nUse the package boundary.\n',
      'utf8',
    );

    const result = documentCommand(['sources', '--root', root]);

    expect(result).toMatchObject({
      command: 'sources',
      documents: [
        {
          path: 'packages/core/decision.md',
          title: 'Local choice',
          status: null,
          links: [],
        },
      ],
      warnings: [],
    });
    const response = result as { documents: Array<Record<string, unknown>> };
    expect(response.documents[0]).not.toHaveProperty('text');
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('selects nested documents, preserves exact text, and resolves declared local links', () => {
  const root = mkdtempSync(join(tmpdir(), 'hivex-documents-'));
  try {
    mkdirSync(join(root, 'packages', 'core'), { recursive: true });
    mkdirSync(join(root, 'packages', 'vendor'), { recursive: true });
    mkdirSync(join(root, '.private'), { recursive: true });
    const exactText =
      '---\ntitle: Package choice\nstatus: draft\n---\n\nSee [module](../module.md).\r\n';
    writeFileSync(join(root, 'packages', 'core', 'decision.md'), exactText, 'utf8');
    writeFileSync(join(root, 'packages', 'module.md'), '# Module\n', 'utf8');
    writeFileSync(join(root, 'packages', 'vendor', 'ignored.md'), '# Vendor\n', 'utf8');
    writeFileSync(join(root, '.private', 'ignored.md'), '# Private\n', 'utf8');

    const listed = documentCommand(['sources', '--root', root]) as {
      origin: string;
      snapshot: string;
      documents: Array<Record<string, unknown>>;
    };
    expect(listed.origin).toBe('current-worktree');
    expect(listed.documents.map((document) => document.path)).toEqual([
      'packages/core/decision.md',
      'packages/module.md',
    ]);
    expect(listed.documents[0]).toMatchObject({
      title: 'Package choice',
      status: 'draft',
      links: ['packages/module.md'],
      hash: createHash('sha256').update(exactText).digest('hex'),
    });

    const opened = documentCommand(['read', 'packages/core/decision.md', '--root', root]) as {
      source: Record<string, unknown>;
      text: string;
      continuation: unknown;
    };
    expect(opened.source).not.toHaveProperty('text');
    expect(opened.text).toBe(exactText);
    expect(opened.continuation).toBeNull();

    writeFileSync(join(root, 'packages', 'module.md'), '# Module changed\n', 'utf8');
    const changed = documentCommand(['sources', '--root', root]) as { snapshot: string };
    expect(changed.snapshot).not.toBe(listed.snapshot);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('accepts a custom explicit layout and rejects legacy collections without migration', () => {
  const root = mkdtempSync(join(tmpdir(), 'hivex-documents-'));
  try {
    mkdirSync(join(root, 'knowledge'), { recursive: true });
    mkdirSync(join(root, 'docs'), { recursive: true });
    writeFileSync(join(root, 'knowledge', 'guide.md'), '# Guide\n', 'utf8');
    writeFileSync(join(root, 'docs', 'not-selected.md'), '# Other\n', 'utf8');
    writeFileSync(
      join(root, 'hivex.json'),
      JSON.stringify({ include: ['knowledge/**/*.md'], exclude: ['knowledge/archive/**'] }),
      'utf8',
    );
    const selected = documentCommand(['sources', '--root', root]) as {
      documents: Array<Record<string, unknown>>;
    };
    expect(selected.documents.map((document) => document.path)).toEqual(['knowledge/guide.md']);

    writeFileSync(
      join(root, 'hivex.json'),
      JSON.stringify({ version: 1, collections: [{ id: 'legacy', include: ['docs/**/*.md'] }] }),
      'utf8',
    );
    expect(() => documentCommand(['sources', '--root', root])).toThrow(/legacy collections/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('declares historical globs for focused reads without treating them as current sources', () => {
  const root = mkdtempSync(join(tmpdir(), 'hivex-history-'));
  try {
    mkdirSync(join(root, 'docs', 'archive'), { recursive: true });
    writeFileSync(join(root, 'docs', 'current.md'), '# Current\n\nCurrent rule.\n', 'utf8');
    writeFileSync(
      join(root, 'docs', 'archive', 'replaced.md'),
      '# Replaced\n\nHistorical rule.\n',
      'utf8',
    );
    writeFileSync(
      join(root, 'hivex.json'),
      JSON.stringify({ include: ['docs/**/*.md'], history: ['docs/archive/**/*.md'] }),
      'utf8',
    );

    const listed = documentCommand(['sources', '--root', root]) as {
      documents: Array<Record<string, unknown>>;
    };
    expect(listed.documents).toEqual([
      expect.objectContaining({ path: 'docs/archive/replaced.md', historical: true }),
      expect.objectContaining({ path: 'docs/current.md', historical: false }),
    ]);

    const opened = documentCommand(['read', 'docs/archive/replaced.md', '--root', root]) as {
      source: Record<string, unknown>;
      text: string;
    };
    expect(opened.source).toMatchObject({ historical: true, path: 'docs/archive/replaced.md' });
    expect(opened.text).toBe('# Replaced\n\nHistorical rule.\n');
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('a large historical archive does not displace current sources at the document limit', () => {
  const root = mkdtempSync(join(tmpdir(), 'hivex-history-limit-'));
  try {
    mkdirSync(join(root, 'archive'));
    for (let index = 0; index < 2048; index += 1)
      writeFileSync(join(root, 'archive', `${index}.md`), '# Old rule\n');
    writeFileSync(join(root, 'current.md'), '# Current\n\nCurrent rule.\n');
    writeFileSync(join(root, 'hivex.json'), JSON.stringify({ history: ['archive/**/*.md'] }));
    expect(documentCommand(['read', 'current.md', '--root', root])).toMatchObject({
      source: { historical: false, path: 'current.md' },
      text: '# Current\n\nCurrent rule.\n',
    });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('does not make an excluded historical source readable', () => {
  const root = mkdtempSync(join(tmpdir(), 'hivex-history-excluded-'));
  try {
    mkdirSync(join(root, 'docs', 'archive'), { recursive: true });
    writeFileSync(join(root, 'docs', 'archive', 'replaced.md'), '# Replaced\n', 'utf8');
    writeFileSync(
      join(root, 'hivex.json'),
      JSON.stringify({
        include: ['docs/**/*.md'],
        history: ['docs/archive/**/*.md'],
        exclude: ['docs/archive/**'],
      }),
      'utf8',
    );

    expect(
      (documentCommand(['sources', '--root', root]) as { documents: unknown[] }).documents,
    ).toEqual([]);
    expect(() => documentCommand(['read', 'docs/archive/replaced.md', '--root', root])).toThrow(
      /was not selected/,
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('prunes excluded directories before inspecting their files and symlinks', () => {
  const root = mkdtempSync(join(tmpdir(), 'hivex-excluded-directory-'));
  try {
    mkdirSync(join(root, 'app', 'ios'), { recursive: true });
    writeFileSync(join(root, 'app', 'docs.md'), '# Legitimate\n', 'utf8');
    writeFileSync(join(root, 'app', 'ios', 'README.md'), '# Generated\n', 'utf8');
    symlinkSync(join(root, 'missing.md'), join(root, 'app', 'ios', 'broken.md'));
    writeFileSync(
      join(root, 'hivex.json'),
      JSON.stringify({ include: ['app/**/*.md'], exclude: ['app/ios/**'] }),
      'utf8',
    );

    const result = documentCommand(['sources', '--root', root]) as {
      documents: Array<Record<string, unknown>>;
      warnings: Array<{ path: string; message: string }>;
    };
    expect(result.documents.map((document) => document.path)).toEqual(['app/docs.md']);
    expect(result.warnings).toEqual([]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('reports invalid UTF-8 and skips symlinked sources without reading outside the root', () => {
  const root = mkdtempSync(join(tmpdir(), 'hivex-documents-'));
  const outside = mkdtempSync(join(tmpdir(), 'hivex-documents-outside-'));
  try {
    mkdirSync(join(root, 'docs'), { recursive: true });
    const outsideFile = join(outside, 'secret.md');
    writeFileSync(outsideFile, '# Secret\n', 'utf8');
    symlinkSync(outsideFile, join(root, 'docs', 'outside.md'));
    writeFileSync(join(root, 'docs', 'invalid.md'), Buffer.from([0xc3, 0x28]));

    const result = documentCommand(['sources', '--root', root]) as {
      documents: Array<Record<string, unknown>>;
      warnings: Array<{ path: string; message: string }>;
    };
    expect(result.documents).toEqual([]);
    expect(result.warnings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ path: 'docs/outside.md', message: 'Skipped symbolic link' }),
        expect.objectContaining({
          path: 'docs/invalid.md',
          message: expect.stringContaining('UTF-8'),
        }),
      ]),
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
    rmSync(outside, { recursive: true, force: true });
  }
});

test('returns explicit continuation for a byte-limited read and rejects invalid ranges', () => {
  const root = mkdtempSync(join(tmpdir(), 'hivex-documents-'));
  try {
    writeFileSync(join(root, 'notes.md'), '# One\nline 2\nline 3\nline 4\n', 'utf8');

    const limited = documentCommand(['read', 'notes.md', '--root', root, '--max-bytes', '12']) as {
      text: string;
      lineStart: number;
      lineEnd: number;
      truncated: boolean;
      continuation: { from: number; reason: string } | null;
    };
    expect(limited).toMatchObject({
      text: '# One\nline 2',
      lineStart: 1,
      lineEnd: 2,
      truncated: true,
      continuation: { from: 3, reason: 'max-bytes' },
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
    ]) as { text: string; lineStart: number; lineEnd: number; continuation: unknown };
    expect(ranged).toMatchObject({
      text: 'line 2\nline 3',
      lineStart: 2,
      lineEnd: 3,
      continuation: { from: 4, reason: 'range' },
    });
    expect(() =>
      documentCommand(['read', 'notes.md', '--root', root, '--from', '4', '--to', '2']),
    ).toThrow(/outside the source/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('keeps readable Markdown with custom metadata instead of requiring a particular schema', () => {
  const root = mkdtempSync(join(tmpdir(), 'hivex-custom-doc-'));
  try {
    writeFileSync(
      join(root, 'policy.md'),
      '---\nstatus: [reviewed, adopted]\n---\n# Policy\n\nRetain requests for seven days.\n',
    );
    expect(documentCommand(['sources', '--root', root])).toMatchObject({
      documents: [expect.objectContaining({ id: 'policy.md', title: 'Policy', status: null })],
    });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('pages source discovery and rejects a continuation after the selected documents change', () => {
  const root = mkdtempSync(join(tmpdir(), 'hivex-pages-'));
  try {
    for (const name of ['a.md', 'b.md', 'c.md']) writeFileSync(join(root, name), '# Decision\n');
    const first = documentCommand(['sources', '--root', root, '--limit', '1']) as {
      documents: { id: string }[];
      continuation: string;
    };
    expect(first.documents.map((document) => document.id)).toEqual(['a.md']);
    const second = documentCommand([
      'sources',
      '--root',
      root,
      '--limit',
      '1',
      '--cursor',
      first.continuation,
    ]) as { documents: { id: string }[] };
    expect(second.documents.map((document) => document.id)).toEqual(['b.md']);
    writeFileSync(join(root, 'c.md'), '# Changed decision\n');
    expect(() =>
      documentCommand(['sources', '--root', root, '--cursor', first.continuation]),
    ).toThrow(/snapshot/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
