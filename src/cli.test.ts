import { describe, expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { z } from 'zod';

const cli = join(import.meta.dirname, 'cli.ts');

function withRepository(run: (root: string) => void) {
  const root = mkdtempSync(join(tmpdir(), 'hivex-cli-'));
  const git = (args: string[]) => {
    const result = spawnSync('git', args, {
      cwd: root,
      encoding: 'utf8',
      env: { ...process.env, GIT_CONFIG_GLOBAL: '/dev/null', GIT_CONFIG_NOSYSTEM: '1' },
    });
    if (result.status !== 0) throw new Error(result.stderr);
  };
  try {
    git(['init', '-q', '--initial-branch=main']);
    mkdirSync(join(root, 'docs'));
    writeFileSync(
      join(root, 'hivex.json'),
      JSON.stringify({
        version: 1,
        collections: [{ id: 'product', include: ['docs/**/*.md'], default: true }],
      }),
    );
    writeFileSync(
      join(root, 'docs', 'cache.md'),
      [
        '---',
        'title: Cache policy',
        'status: accepted',
        '---',
        '',
        '# Cache policy',
        '',
        'A cached extraction is reusable only while its source hash is unchanged.',
        '',
        'Never treat the cache as authority. Read the versioned source to verify a claim.',
        '',
      ].join('\n'),
    );
    git(['add', '.']);
    git([
      '-c',
      'user.name=Hivex test',
      '-c',
      'user.email=hivex@example.invalid',
      'commit',
      '-qm',
      'fixture',
    ]);
    run(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

function invoke(root: string, args: string[]) {
  return spawnSync(process.execPath, [cli, ...args, '--root', root], {
    encoding: 'utf8',
    timeout: 10_000,
  });
}

function output(text: string): unknown {
  return JSON.parse(text);
}

function commitChanges(root: string) {
  expect(spawnSync('git', ['add', '.'], { cwd: root }).status).toBe(0);
  expect(
    spawnSync(
      'git',
      [
        '-c',
        'user.name=Hivex test',
        '-c',
        'user.email=hivex@example.invalid',
        'commit',
        '-qm',
        'update fixture',
      ],
      { cwd: root },
    ).status,
  ).toBe(0);
}

const page = z.object({
  snapshot: z.object({ commit: z.string() }),
  blocks: z.array(z.object({ text: z.string() })),
  continuation: z.string().nullable(),
});

describe('hivex CLI', () => {
  test('keeps a quoted heading and the following condition inside the enclosing section', () => {
    withRepository((root) => {
      writeFileSync(
        join(root, 'docs', 'mixed.md'),
        '# Context\n\n## Product\n\n> # Quoted\n> Prior evidence.\n\nOnly apply this rule to the stated case.\n\n## Outside\n\nAnother domain.\n',
      );
      writeFileSync(
        join(root, 'hivex.json'),
        JSON.stringify({
          version: 1,
          collections: [{ id: 'product', include: [{ path: 'docs/mixed.md', anchor: 'product' }] }],
        }),
      );
      commitChanges(root);
      const result = invoke(root, ['read', 'docs/mixed.md#product']);
      expect(result.status).toBe(0);
      expect(page.parse(output(result.stdout)).blocks.map((block) => block.text)).toEqual([
        '## Product',
        '> # Quoted\n> Prior evidence.',
        'Only apply this rule to the stated case.',
      ]);
    });
  });
  test('reserves anchors in quoted headings and refuses to cut a containing block', () => {
    withRepository((root) => {
      writeFileSync(
        join(root, 'docs', 'mixed.md'),
        '# Context\n\n> ## Policy\n> Quoted rule.\n\n## Policy\n\nCurrent rule.\n',
      );
      const configure = (anchor: string) => {
        writeFileSync(
          join(root, 'hivex.json'),
          JSON.stringify({
            version: 1,
            collections: [{ id: 'product', include: [{ path: 'docs/mixed.md', anchor }] }],
          }),
        );
        commitChanges(root);
      };
      configure('policy-1');
      const result = invoke(root, ['read', 'docs/mixed.md#policy-1']);
      expect(result.status).toBe(0);
      expect(page.parse(output(result.stdout)).blocks.map((block) => block.text)).toEqual([
        '## Policy',
        'Current rule.',
      ]);
      const full = invoke(root, ['read', 'docs/mixed.md']);
      expect(full.status).toBe(0);
      expect(output(full.stdout)).toMatchObject({
        blocks: expect.arrayContaining([
          expect.objectContaining({ text: '> ## Policy\n> Quoted rule.', kind: 'blockquote' }),
        ]),
      });
      configure('policy');
      const unsupported = invoke(root, ['read', 'docs/mixed.md#policy']);
      expect(unsupported.status).toBe(1);
      expect(output(unsupported.stderr)).toMatchObject({ error: { code: 'UNSUPPORTED_SECTION' } });
    });
  });
  test('paginates a duplicate heading section with cursors bound to its original scope and revision', () => {
    withRepository((root) => {
      const paragraphs = [
        'First condition: ' + 'á'.repeat(180),
        'Second condition: ' + 'é'.repeat(180),
        'Third condition: ' + 'í'.repeat(180),
      ];
      writeFileSync(
        join(root, 'docs', 'mixed.md'),
        [
          '# Context',
          '## Policy',
          'Earlier rule.',
          '## Policy',
          ...paragraphs,
          '## Outside',
          'Outside evidence.',
        ].join('\n\n') + '\n',
      );
      writeFileSync(
        join(root, 'hivex.json'),
        JSON.stringify({
          version: 1,
          collections: [
            {
              id: 'product',
              include: [
                { path: 'docs/mixed.md', anchor: 'policy' },
                { path: 'docs/mixed.md', anchor: 'policy-1' },
              ],
            },
          ],
        }),
      );
      commitChanges(root);
      const first = invoke(root, ['read', 'docs/mixed.md#policy-1', '--max-bytes', '1500']);
      expect(first.status).toBe(0);
      const parsed = page.parse(output(first.stdout));
      if (!parsed.continuation) throw new Error('Expected another complete page');
      for (const id of ['docs/mixed.md#policy', 'docs/mixed.md']) {
        const wrongScope = invoke(root, ['read', id, '--cursor', parsed.continuation]);
        expect(wrongScope.status).toBe(1);
        expect(output(wrongScope.stderr)).toMatchObject({ error: { code: 'CURSOR_MISMATCH' } });
      }
      writeFileSync(
        join(root, 'docs', 'mixed.md'),
        '# Changed context\n\n## Policy\n\nNew rule.\n\n## Policy\n\nNew exception.\n',
      );
      commitChanges(root);
      const wrongRevision = invoke(root, [
        'read',
        'docs/mixed.md#policy-1',
        '--cursor',
        parsed.continuation,
      ]);
      expect(wrongRevision.status).toBe(1);
      expect(output(wrongRevision.stderr)).toMatchObject({ error: { code: 'CURSOR_MISMATCH' } });
      const texts = parsed.blocks.map((block) => block.text);
      let cursor: string | null = parsed.continuation;
      for (let pages = 0; cursor !== null && pages < 10; pages += 1) {
        const next = invoke(root, [
          'read',
          'docs/mixed.md#policy-1',
          '--ref',
          parsed.snapshot.commit,
          '--cursor',
          cursor,
          '--max-bytes',
          '1500',
        ]);
        expect(next.status).toBe(0);
        expect(Buffer.byteLength(next.stdout)).toBeLessThanOrEqual(1500);
        const current = page.parse(output(next.stdout));
        texts.push(...current.blocks.map((block) => block.text));
        cursor = current.continuation;
      }
      expect(cursor).toBeNull();
      expect(texts).toEqual(['## Policy', ...paragraphs]);
    });
  });
  test.each([
    ['docs/cache.md', { path: 'docs/cache.md', anchor: 'cache-policy' }],
    [
      { path: 'docs/cache.md', anchor: 'cache-policy' },
      { path: 'docs/cache.md', anchor: 'cache-policy' },
    ],
  ])('rejects duplicate or full-document/section ownership', (...include) => {
    withRepository((root) => {
      writeFileSync(
        join(root, 'hivex.json'),
        JSON.stringify({
          version: 1,
          collections: [{ id: 'history', default: false, include }],
        }),
      );
      commitChanges(root);
      const result = invoke(root, ['search', 'cache']);
      expect(result.status).toBe(1);
      expect(output(result.stderr)).toMatchObject({ error: { code: 'AMBIGUOUS_COLLECTION' } });
    });
  });
  test.each([
    { path: 'docs/missing.md', anchor: 'policy', code: 'SECTION_NOT_FOUND' },
    { path: 'docs/cache.md', anchor: 'missing', code: 'SECTION_NOT_FOUND' },
    { path: '../docs/cache.md', anchor: 'policy', code: 'INVALID_CONFIG' },
    { path: 'docs/*.md', anchor: 'policy', code: 'INVALID_CONFIG' },
    { path: 'docs/cache.md', anchor: '   ', code: 'INVALID_CONFIG' },
  ])('rejects a missing or unsafe section selector: $path#$anchor', (selector) => {
    withRepository((root) => {
      writeFileSync(
        join(root, 'hivex.json'),
        JSON.stringify({
          version: 1,
          collections: [
            {
              id: 'history',
              default: false,
              include: [{ path: selector.path, anchor: selector.anchor }],
            },
          ],
        }),
      );
      commitChanges(root);
      const result = invoke(root, ['search', 'cache']);
      expect(result.status).toBe(1);
      expect(output(result.stderr)).toMatchObject({ error: { code: selector.code } });
    });
  });
  test('rejects overlapping sections even when both collections are opt-in', () => {
    withRepository((root) => {
      writeFileSync(
        join(root, 'docs', 'mixed.md'),
        '# Context\n\n## Domain\n\nIdentity.\n\n### Exceptions\n\nCompany identity.\n',
      );
      writeFileSync(
        join(root, 'hivex.json'),
        JSON.stringify({
          version: 1,
          collections: [
            { id: 'product', include: ['docs/cache.md'] },
            { id: 'one', include: [{ path: 'docs/mixed.md', anchor: 'domain' }], default: false },
            {
              id: 'two',
              include: [{ path: 'docs/mixed.md', anchor: 'exceptions' }],
              default: false,
            },
          ],
        }),
      );
      commitChanges(root);
      const result = invoke(root, ['search', 'cache']);
      expect(result.status).toBe(1);
      expect(output(result.stderr)).toMatchObject({ error: { code: 'AMBIGUOUS_COLLECTION' } });
    });
  });
  test('separates a mixed document into scoped sources while keeping the full document readable', () => {
    withRepository((root) => {
      writeFileSync(
        join(root, 'docs', 'context.md'),
        [
          '---',
          'status: accepted',
          '---',
          '',
          '# Context',
          '',
          '## Product',
          '',
          'A person owns an identity.',
          '',
          '### Exception',
          '',
          'A company owns its own identity.',
          '',
          '```md',
          '## Fake heading',
          '```',
          '',
          '## Machinery',
          '',
          'An arbiter manages a contested round.',
          '',
        ].join('\n'),
      );
      writeFileSync(
        join(root, 'hivex.json'),
        JSON.stringify({
          version: 1,
          collections: [
            { id: 'product', include: [{ path: 'docs/context.md', anchor: 'product' }] },
            {
              id: 'legacy',
              include: [{ path: 'docs/context.md', anchor: 'machinery' }],
              kind: 'legacy',
              default: false,
            },
          ],
        }),
      );
      commitChanges(root);
      const found = invoke(root, ['search', 'identity arbiter']);
      expect(found.status).toBe(0);
      expect(output(found.stdout)).toMatchObject({
        results: [
          {
            id: 'docs/context.md#product',
            path: 'docs/context.md',
            collection: 'product',
            section: { anchor: 'product', lineStart: 7, lineEnd: 17 },
            authority: {
              declaredStatus: 'accepted',
              currentness: 'not-established',
              scope: 'document',
            },
          },
        ],
      });
      const opened = invoke(root, ['read', 'docs/context.md#product']);
      expect(opened.status).toBe(0);
      expect(page.parse(output(opened.stdout)).blocks.map((block) => block.text)).toEqual([
        '## Product',
        'A person owns an identity.',
        '### Exception',
        'A company owns its own identity.',
        '```md\n## Fake heading\n```',
      ]);
      expect(output(opened.stdout)).toMatchObject({ continuation: null });
      const full = invoke(root, ['read', 'docs/context.md']);
      expect(full.status).toBe(0);
      expect(output(full.stdout)).toMatchObject({
        source: { id: 'docs/context.md', collection: null, section: null },
        blocks: expect.arrayContaining([
          expect.objectContaining({ text: 'An arbiter manages a contested round.' }),
        ]),
      });
      const legacy = invoke(root, ['search', 'arbiter', '--collection', 'legacy']);
      expect(legacy.status).toBe(0);
      expect(output(legacy.stdout)).toMatchObject({
        results: [{ id: 'docs/context.md#machinery' }],
      });
    });
  });
  test('opens the whole decision for an identifier even when its text only matches an amendment', () => {
    withRepository((root) => {
      writeFileSync(
        join(root, 'docs', '0006-policy.md'),
        '# Render policy\n\n## Decision\n\nDo not add manual memoization.\n\n## Amendment\n\nADR 0006 permits memoization when the compiler does not cache the value.\n',
      );
      writeFileSync(
        join(root, 'hivex.json'),
        JSON.stringify({
          version: 1,
          collections: [{ id: 'product', include: ['docs/**'], aliasPrefix: 'ADR' }],
        }),
      );
      commitChanges(root);
      const found = invoke(root, ['search', 'ADR 0006', '--limit', '1']);
      expect(found.status).toBe(0);
      const search = z
        .object({
          snapshot: z.object({ commit: z.string() }),
          results: z.array(z.object({ id: z.string(), readCursor: z.string() })).min(1),
        })
        .parse(output(found.stdout));
      const hit = search.results[0];
      if (!hit) throw new Error('Expected the declared decision');
      const opened = invoke(root, [
        'read',
        hit.id,
        '--ref',
        search.snapshot.commit,
        '--cursor',
        hit.readCursor,
      ]);
      expect(opened.status).toBe(0);
      expect(output(opened.stdout)).toMatchObject({
        blocks: expect.arrayContaining([
          expect.objectContaining({ text: 'Do not add manual memoization.' }),
          expect.objectContaining({
            text: 'ADR 0006 permits memoization when the compiler does not cache the value.',
          }),
        ]),
        continuation: null,
      });
    });
  });
  test.each([
    ['read', 'docs/cache.md', '--cursor'],
    ['search', 'cache', '--collection'],
  ])('rejects an explicitly empty option for %s', (command, value, option) => {
    withRepository((root) => {
      const result = invoke(root, [command, value, option, '']);
      expect(result.status).toBe(1);
    });
  });
  test('shows its interface without requiring a configured repository', () => {
    const result = spawnSync(process.execPath, [cli, '--help'], {
      cwd: tmpdir(),
      encoding: 'utf8',
      timeout: 10_000,
    });
    expect(result.status).toBe(0);
    expect(output(result.stdout)).toMatchObject({
      application: 'hivex',
      commands: expect.arrayContaining([
        expect.objectContaining({ name: 'search' }),
        expect.objectContaining({ name: 'read' }),
      ]),
    });
  });
  test('finds a declared source and opens its complete condition and prohibition', () => {
    withRepository((root) => {
      const found = invoke(root, ['search', 'cached extraction']);
      expect(found.status).toBe(0);
      expect(output(found.stdout)).toMatchObject({
        results: [
          {
            id: 'docs/cache.md',
            collection: 'product',
            authority: { declaredStatus: 'accepted', currentness: 'not-established' },
          },
        ],
      });
      const opened = invoke(root, ['read', 'docs/cache.md']);
      expect(opened.status).toBe(0);
      expect(output(opened.stdout)).toMatchObject({
        source: { id: 'docs/cache.md' },
        blocks: expect.arrayContaining([
          expect.objectContaining({
            text: 'A cached extraction is reusable only while its source hash is unchanged.',
          }),
          expect.objectContaining({
            text: 'Never treat the cache as authority. Read the versioned source to verify a claim.',
          }),
        ]),
        continuation: null,
      });
    });
  });

  test('reads the original snapshot after HEAD and working-tree contents change', () => {
    withRepository((root) => {
      const original = spawnSync('git', ['rev-parse', 'HEAD'], {
        cwd: root,
        encoding: 'utf8',
      }).stdout.trim();
      writeFileSync(
        join(root, 'docs', 'cache.md'),
        '# Changed policy\n\nThis is a later decision.\n',
      );
      spawnSync('git', ['add', '.'], { cwd: root });
      const committed = spawnSync(
        'git',
        [
          '-c',
          'user.name=Hivex test',
          '-c',
          'user.email=hivex@example.invalid',
          'commit',
          '-qm',
          'change policy',
        ],
        { cwd: root },
      );
      expect(committed.status).toBe(0);
      writeFileSync(
        join(root, 'docs', 'cache.md'),
        'Uncommitted text must not replace the snapshot.',
      );
      const opened = invoke(root, ['read', 'docs/cache.md', '--ref', original]);
      expect(opened.status).toBe(0);
      expect(output(opened.stdout)).toMatchObject({
        snapshot: { commit: original },
        blocks: expect.arrayContaining([
          expect.objectContaining({
            text: 'A cached extraction is reusable only while its source hash is unchanged.',
          }),
        ]),
      });
    });
  });

  test('paginates complete Unicode blocks inside the output budget without losing text', () => {
    withRepository((root) => {
      const paragraphs = Array.from({ length: 8 }, (_, index) =>
        `Ámbito ${index + 1} 🐝: no reutilizar si cambia la fuente. `.repeat(6).trim(),
      );
      writeFileSync(
        join(root, 'docs', 'cache.md'),
        '# Política\n\n' + paragraphs.join('\n\n') + '\n',
      );
      spawnSync('git', ['add', '.'], { cwd: root });
      expect(
        spawnSync(
          'git',
          [
            '-c',
            'user.name=Hivex test',
            '-c',
            'user.email=hivex@example.invalid',
            'commit',
            '-qm',
            'unicode blocks',
          ],
          { cwd: root },
        ).status,
      ).toBe(0);
      const seen: string[] = [];
      let cursor: string | null = null;
      let pages = 0;
      do {
        const args = ['read', 'docs/cache.md', '--max-bytes', '1536'];
        if (cursor) args.push('--cursor', cursor);
        const result = invoke(root, args);
        expect(result.status).toBe(0);
        expect(Buffer.byteLength(result.stdout)).toBeLessThanOrEqual(1536);
        const decoded = page.parse(output(result.stdout));
        seen.push(...decoded.blocks.map((block) => block.text));
        cursor = decoded.continuation;
        pages += 1;
        expect(pages).toBeLessThan(20);
      } while (cursor);
      expect(pages).toBeGreaterThan(1);
      expect(seen).toEqual(['# Política', ...paragraphs]);
    });
  });

  test('keeps legacy documents out of default search but makes them explicitly discoverable', () => {
    withRepository((root) => {
      mkdirSync(join(root, 'docs', 'legacy'));
      writeFileSync(
        join(root, 'docs', 'legacy', 'cache.md'),
        '# Cached extraction\n\nOld fleet cache instructions.\n',
      );
      writeFileSync(
        join(root, 'hivex.json'),
        JSON.stringify({
          version: 1,
          collections: [
            { id: 'product', include: ['docs/*.md'], default: true },
            { id: 'legacy', include: ['docs/legacy/*.md'], default: false, kind: 'legacy' },
          ],
        }),
      );
      commitChanges(root);
      const normal = invoke(root, ['search', 'cached extraction']);
      expect(normal.status).toBe(0);
      expect(output(normal.stdout)).toMatchObject({ results: [{ id: 'docs/cache.md' }] });
      const historical = invoke(root, ['search', 'cached extraction', '--collection', 'legacy']);
      expect(historical.status).toBe(0);
      expect(output(historical.stdout)).toMatchObject({
        results: [
          {
            id: 'docs/legacy/cache.md',
            collectionKind: 'legacy',
            authority: { declaredStatus: 'unknown' },
          },
        ],
      });
    });
  });

  test('prioritizes a configured document reference while respecting result and byte limits', () => {
    withRepository((root) => {
      writeFileSync(
        join(root, 'docs', '0006-target.md'),
        '# The requested decision\n\nThis governs rendering.\n',
      );
      writeFileSync(
        join(root, 'docs', '0001-other.md'),
        '# ADR 0006 cache\n\nADR 0006 cache appears in an unrelated source.\n',
      );
      writeFileSync(
        join(root, 'hivex.json'),
        JSON.stringify({
          version: 1,
          collections: [{ id: 'product', include: ['docs/**/*.md'], aliasPrefix: 'ADR' }],
        }),
      );
      commitChanges(root);
      const result = invoke(root, [
        'search',
        'ADR 0006 cache',
        '--limit',
        '1',
        '--max-bytes',
        '1024',
      ]);
      expect(result.status).toBe(0);
      expect(Buffer.byteLength(result.stdout)).toBeLessThanOrEqual(1024);
      expect(output(result.stdout)).toMatchObject({
        results: [{ id: 'docs/0006-target.md', match: { kind: 'identifier' } }],
        truncated: true,
      });
    });
  });

  test('preserves GFM tables, fenced code and duplicate heading anchors in Markdown sources', () => {
    withRepository((root) => {
      const table = '| Rule | State |\n| --- | --- |\n| Do not copy private memory | accepted |';
      const code = '```sh\n# Not a heading\nprintf do-not-execute\n```';
      writeFileSync(
        join(root, 'docs', 'rules.markdown'),
        `---\nstatus: proposed\n---\n\n# Rules\n\n## Save now\n\n${table}\n\n${code}\n\n## Save now\n\nReview first.\n`,
      );
      writeFileSync(
        join(root, 'hivex.json'),
        JSON.stringify({ version: 1, collections: [{ id: 'product', include: ['docs/**'] }] }),
      );
      commitChanges(root);
      const result = invoke(root, ['read', 'docs/rules.markdown']);
      expect(result.status).toBe(0);
      expect(output(result.stdout)).toMatchObject({
        source: { authority: { declaredStatus: 'proposed' } },
        blocks: expect.arrayContaining([
          expect.objectContaining({ kind: 'table', text: table }),
          expect.objectContaining({ kind: 'code', text: code }),
          expect.objectContaining({ kind: 'heading', anchor: 'save-now' }),
          expect.objectContaining({ kind: 'heading', anchor: 'save-now-1' }),
        ]),
      });
    });
  });

  test('validates declared replacements against the same snapshot instead of inventing authority', () => {
    withRepository((root) => {
      writeFileSync(
        join(root, 'docs', 'cache.md'),
        '---\nstatus: superseded\nsuperseded_by: [docs/replacement.md]\n---\n\n# Old policy\n',
      );
      commitChanges(root);
      const missing = invoke(root, ['read', 'docs/cache.md']);
      expect(missing.status).toBe(1);
      expect(output(missing.stderr)).toMatchObject({ error: { code: 'INVALID_REPLACEMENT' } });
      writeFileSync(
        join(root, 'docs', 'replacement.md'),
        '---\nstatus: accepted\n---\n\n# New policy\n',
      );
      commitChanges(root);
      const valid = invoke(root, ['read', 'docs/cache.md']);
      expect(valid.status).toBe(0);
      expect(output(valid.stdout)).toMatchObject({
        source: {
          authority: {
            declaredStatus: 'superseded',
            supersededBy: ['docs/replacement.md'],
          },
        },
      });
    });
  });

  test('refuses symlink sources without reading outside the declared repository', () => {
    withRepository((root) => {
      symlinkSync('/not-a-project/private.md', join(root, 'docs', 'outside.md'));
      commitChanges(root);
      const result = invoke(root, ['read', 'docs/outside.md']);
      expect(result.status).toBe(1);
      expect(result.stdout).toBe('');
      expect(output(result.stderr)).toMatchObject({ error: { code: 'UNSUPPORTED_SOURCE' } });
    });
  });

  test('reports an oversized block instead of silently cutting its condition', () => {
    withRepository((root) => {
      writeFileSync(
        join(root, 'docs', 'cache.md'),
        'Never reuse a stale cache; the source always wins. '.repeat(70),
      );
      commitChanges(root);
      const result = invoke(root, ['read', 'docs/cache.md', '--max-bytes', '1024']);
      expect(result.status).toBe(1);
      expect(result.stdout).toBe('');
      expect(output(result.stderr)).toMatchObject({
        error: { code: 'BLOCK_EXCEEDS_BUDGET', details: { requiredBytes: expect.any(Number) } },
      });
    });
  });

  test('rejects a cursor from another source in the same commit', () => {
    withRepository((root) => {
      writeFileSync(
        join(root, 'docs', 'cache.md'),
        '# Rules\n\n' +
          Array.from({ length: 8 }, () =>
            'This condition must remain complete. '.repeat(8).trim(),
          ).join('\n\n'),
      );
      writeFileSync(join(root, 'docs', 'other.md'), '# Other source\n');
      commitChanges(root);
      const first = invoke(root, ['read', 'docs/cache.md', '--max-bytes', '1536']);
      expect(first.status).toBe(0);
      const cursor = page
        .extend({ continuation: z.string() })
        .parse(output(first.stdout)).continuation;
      const result = invoke(root, ['read', 'docs/other.md', '--cursor', cursor]);
      expect(result.status).toBe(1);
      expect(output(result.stderr)).toMatchObject({ error: { code: 'CURSOR_MISMATCH' } });
    });
  });

  test.each(['0', '-1', '1.5', 'NaN', '65537'])('rejects invalid output limit %s', (limit) => {
    const result = invoke('/not-a-repository', ['read', 'docs/cache.md', '--max-bytes', limit]);
    expect(result.status).toBe(1);
    expect(output(result.stderr)).toMatchObject({ error: { code: 'INVALID_ARGUMENT' } });
  });

  test('rejects source patterns escaping the repository', () => {
    withRepository((root) => {
      writeFileSync(
        join(root, 'hivex.json'),
        JSON.stringify({ version: 1, collections: [{ id: 'outside', include: ['../**/*.md'] }] }),
      );
      commitChanges(root);
      const result = invoke(root, ['search', 'cache']);
      expect(result.status).toBe(1);
      expect(output(result.stderr)).toMatchObject({ error: { code: 'INVALID_CONFIG' } });
    });
  });

  test('uses the requested root even when the caller has an unrelated Git environment', () => {
    withRepository((root) => {
      const result = spawnSync(process.execPath, [cli, 'read', 'docs/cache.md', '--root', root], {
        encoding: 'utf8',
        timeout: 10_000,
        env: { ...process.env, GIT_DIR: join(root, 'unrelated-git-directory') },
      });
      expect(result.status).toBe(0);
      expect(output(result.stdout)).toMatchObject({ source: { id: 'docs/cache.md' } });
    });
  });

  test('bounds diagnostic output even when invalid configuration contains a huge field name', () => {
    withRepository((root) => {
      writeFileSync(
        join(root, 'hivex.json'),
        JSON.stringify({
          version: 1,
          collections: [{ id: 'product', include: ['docs/**'] }],
          ['invalid-'.repeat(600)]: true,
        }),
      );
      commitChanges(root);
      const result = invoke(root, ['search', 'cache', '--max-bytes', '1024']);
      expect(result.status).toBe(1);
      expect(Buffer.byteLength(result.stderr)).toBeLessThanOrEqual(1024);
      expect(output(result.stderr)).toMatchObject({ error: { code: 'INVALID_CONFIG' } });
    });
  });
});

const relationRule = {
  id: '0001:decision:1',
  type: 'rule',
  adr: '0001',
  kind: 'decision',
  text: 'Cache reuse has a narrower condition.',
  status: 'partially-amended',
  source: { path: 'docs/cache.md', section: 'Cache policy', anchor: 'cache-policy' },
  supersededBy: [
    {
      adr: '0002',
      source: {
        path: 'docs/revised.md',
        section: 'Conditional reuse',
        anchor: 'conditional-reuse',
      },
    },
  ],
};

function writeRelationIndex(root: string, records: unknown[]) {
  const header = {
    type: 'schema',
    schemaVersion: 1,
    format: 'compi-adr-supersession-index',
    statuses: ['live', 'superseded', 'partially-amended', 'unresolved'],
  };
  writeFileSync(
    join(root, 'docs', 'relations.jsonl'),
    [header, ...records].map((row) => JSON.stringify(row)).join('\n') + '\n',
  );
}

function withRelations(run: (root: string) => void) {
  withRepository((root) => {
    writeFileSync(
      join(root, 'hivex.json'),
      JSON.stringify({
        version: 1,
        collections: [{ id: 'product', include: ['docs/**/*.md'] }],
        relationIndexes: [{ path: 'docs/relations.jsonl', format: 'compi-adr-supersession-index' }],
      }),
    );
    writeFileSync(
      join(root, 'docs', 'revised.md'),
      '# Replacement\n\n## Conditional reuse\n\nReuse only when the source hash is unchanged and the schema remains compatible.\n',
    );
    writeRelationIndex(root, [
      relationRule,
      { ...relationRule, id: '0001:summary', status: 'live', supersededBy: [] },
    ]);
    commitChanges(root);
    run(root);
  });
}

describe('hivex relations', () => {
  test('the project CI entrypoint accepts an empty index and still rejects an invalid header', () => {
    withRelations((root) => {
      mkdirSync(join(root, 'hivex', 'src'), { recursive: true });
      writeFileSync(join(root, '.gitignore'), 'node_modules\n');
      symlinkSync(join(import.meta.dirname, '../node_modules'), join(root, 'node_modules'));
      writeFileSync(
        join(root, 'hivex', 'src', 'project.test.ts'),
        readFileSync(join(import.meta.dirname, 'project.test.ts')),
      );
      writeFileSync(join(root, 'hivex', 'src', 'cli.ts'), 'import ' + JSON.stringify(cli) + ';\n');
      writeRelationIndex(root, []);
      commitChanges(root);
      const run = () =>
        spawnSync(process.execPath, ['test', 'hivex/src/project.test.ts'], {
          cwd: root,
          encoding: 'utf8',
          timeout: 10_000,
          maxBuffer: 1_048_576,
        });
      const valid = run();
      expect(valid.status).toBe(0);
      writeFileSync(
        join(root, 'docs', 'relations.jsonl'),
        JSON.stringify({
          type: 'schema',
          schemaVersion: 2,
          format: 'compi-adr-supersession-index',
          statuses: ['live', 'superseded', 'partially-amended', 'unresolved'],
        }) + '\n',
      );
      commitChanges(root);
      const invalid = run();
      expect(invalid.status).toBe(1);
      expect(invalid.stderr).toContain('INVALID_RELATION_INDEX');
    });
  }, 30_000);
  test('retains the required byte budget when an oversized record has a long identifier', () => {
    withRelations((root) => {
      writeRelationIndex(root, [{ ...relationRule, id: 'x'.repeat(2000) }]);
      commitChanges(root);
      const result = invoke(root, ['relations', 'docs/cache.md', '--max-bytes', '1024']);
      expect(result.status).toBe(1);
      const error = z
        .object({
          error: z.object({
            code: z.literal('RELATION_EXCEEDS_BUDGET'),
            details: z.object({
              requiredBytes: z.number(),
              maximumBytes: z.number(),
              line: z.number(),
            }),
          }),
        })
        .parse(output(result.stderr));
      expect(error.error.details.requiredBytes).toBeGreaterThan(1024);
      expect(error.error.details.line).toBe(2);
      expect(Buffer.byteLength(result.stderr)).toBeLessThanOrEqual(1024);
    });
  });
  test('rejects a reference whose document read would fail and accepts declared replacements outside the index', () => {
    withRelations((root) => {
      writeFileSync(
        join(root, 'docs', 'revised.md'),
        '---\nsuperseded_by: [docs/final.md]\n---\n\n# Replacement\n\n## Conditional reuse\n\nA later condition applies.\n',
      );
      commitChanges(root);
      const invalid = invoke(root, ['relations', 'docs/cache.md']);
      expect(invalid.status).toBe(1);
      expect(output(invalid.stderr)).toMatchObject({ error: { code: 'INVALID_REPLACEMENT' } });
      writeFileSync(
        join(root, 'docs', 'final.md'),
        '# Final policy\n\nThe declared replacement is outside the index.\n',
      );
      commitChanges(root);
      const valid = invoke(root, ['relations', 'docs/cache.md']);
      expect(valid.status).toBe(0);
      const schema = z.object({
        snapshot: z.object({ commit: z.string() }),
        records: z.array(
          z.object({
            supersededBy: z.array(z.object({ path: z.string(), readCursor: z.string() })),
          }),
        ),
      });
      const parsed = schema.parse(output(valid.stdout));
      const target = parsed.records[0]?.supersededBy[0];
      if (!target) throw new Error('Expected a readable replacement reference');
      const read = invoke(root, [
        'read',
        target.path,
        '--ref',
        parsed.snapshot.commit,
        '--cursor',
        target.readCursor,
      ]);
      expect(read.status).toBe(0);
      expect(output(read.stdout)).toMatchObject({
        source: { authority: { supersededBy: ['docs/final.md'] } },
      });
    });
  });
  test('keeps equal IDs from distinct indexes separate by their provenance', () => {
    withRelations((root) => {
      writeFileSync(
        join(root, 'docs', 'second.jsonl'),
        [
          JSON.stringify({
            type: 'schema',
            schemaVersion: 1,
            format: 'compi-adr-supersession-index',
            statuses: ['live', 'superseded', 'partially-amended', 'unresolved'],
          }),
          JSON.stringify({ ...relationRule, text: 'A different indexed interpretation.' }),
        ].join('\n') + '\n',
      );
      writeFileSync(
        join(root, 'hivex.json'),
        JSON.stringify({
          version: 1,
          collections: [{ id: 'product', include: ['docs/**/*.md'] }],
          relationIndexes: [
            { path: 'docs/relations.jsonl', format: 'compi-adr-supersession-index' },
            { path: 'docs/second.jsonl', format: 'compi-adr-supersession-index' },
          ],
        }),
      );
      commitChanges(root);
      const result = invoke(root, ['relations', 'docs/cache.md']);
      expect(result.status).toBe(0);
      expect(output(result.stdout)).toMatchObject({
        records: [
          { id: '0001:decision:1', indexEntry: { path: 'docs/relations.jsonl' } },
          {
            id: '0001:decision:1',
            indexEntry: { path: 'docs/second.jsonl' },
            indexedText: 'A different indexed interpretation.',
          },
        ],
      });
    });
  });

  test('rejects a newer index schema and a symlink replacement document', () => {
    withRelations((root) => {
      writeFileSync(
        join(root, 'docs', 'relations.jsonl'),
        JSON.stringify({
          type: 'schema',
          schemaVersion: 2,
          format: 'compi-adr-supersession-index',
          statuses: ['live', 'superseded', 'partially-amended', 'unresolved'],
        }) + '\n',
      );
      commitChanges(root);
      const schema = invoke(root, ['relations', 'docs/cache.md']);
      expect(schema.status).toBe(1);
      expect(output(schema.stderr)).toMatchObject({
        error: { code: 'INVALID_RELATION_INDEX', details: { line: 1 } },
      });
      writeRelationIndex(root, [relationRule]);
      rmSync(join(root, 'docs', 'revised.md'));
      symlinkSync('/etc/passwd', join(root, 'docs', 'revised.md'));
      commitChanges(root);
      const linked = invoke(root, ['relations', 'docs/cache.md']);
      expect(linked.status).toBe(1);
      expect(output(linked.stderr)).toMatchObject({ error: { code: 'INVALID_RELATION_FILE' } });
    });
  });
  test('does not hide broken references behind an indexed live status', () => {
    withRelations((root) => {
      writeRelationIndex(root, [
        {
          ...relationRule,
          status: 'live',
          source: { ...relationRule.source, anchor: 'obsolete-heading' },
          supersededBy: [],
        },
      ]);
      commitChanges(root);
      const result = invoke(root, ['relations', 'docs/cache.md']);
      expect(result.status).toBe(0);
      expect(output(result.stdout)).toMatchObject({
        records: [
          {
            indexedStatus: 'live',
            subject: { anchor: 'obsolete-heading', resolution: 'missing-anchor' },
            supersededBy: [],
          },
        ],
        currentness: 'not-established',
      });
    });
  });
  test('bounds index record loading before resolving its references', () => {
    withRelations((root) => {
      writeRelationIndex(
        root,
        Array.from({ length: 10_001 }, (_, index) => ({
          id: String(index),
          type: 'rule',
          adr: '0001',
          kind: 'scope',
          text: '',
          status: 'live',
          source: { path: 'docs/cache.md', anchor: 'cache-policy', section: '' },
          supersededBy: [],
        })),
      );
      commitChanges(root);
      const result = invoke(root, ['relations', 'docs/cache.md']);
      expect(result.status).toBe(1);
      expect(output(result.stderr)).toMatchObject({ error: { code: 'TOO_MANY_RELATION_RECORDS' } });
    });
  });
  test('distinguishes an unconfigured index from an empty relation result', () => {
    withRepository((root) => {
      const result = invoke(root, ['relations', 'docs/cache.md']);
      expect(result.status).toBe(0);
      expect(output(result.stdout)).toMatchObject({
        coverage: 'not-configured',
        indexes: [],
        records: [],
        continuation: null,
        currentness: 'not-established',
      });
    });
    withRelations((root) => {
      const result = invoke(root, ['relations', 'docs/revised.md']);
      expect(result.status).toBe(0);
      expect(output(result.stdout)).toMatchObject({
        coverage: 'configured-indexes',
        records: [],
        currentness: 'not-established',
      });
    });
  });

  test('keeps relation scope at the complete document when a selected section is requested', () => {
    withRelations((root) => {
      writeFileSync(
        join(root, 'docs', 'cache.md'),
        '# Cache policy\n\n## Reuse\n\nThe scope selected for reading.\n\n## Other rule\n\nA related rule outside that section.\n',
      );
      writeFileSync(
        join(root, 'hivex.json'),
        JSON.stringify({
          version: 1,
          collections: [
            {
              id: 'product',
              include: [{ path: 'docs/cache.md', anchor: 'reuse' }, 'docs/revised.md'],
            },
          ],
          relationIndexes: [
            { path: 'docs/relations.jsonl', format: 'compi-adr-supersession-index' },
          ],
        }),
      );
      writeRelationIndex(root, [
        relationRule,
        {
          ...relationRule,
          id: '0001:other',
          source: { path: 'docs/cache.md', section: 'Other rule', anchor: 'other-rule' },
        },
      ]);
      commitChanges(root);
      const result = invoke(root, ['relations', 'docs/cache.md#reuse']);
      expect(result.status).toBe(0);
      expect(output(result.stdout)).toMatchObject({
        scope: 'document',
        source: { id: 'docs/cache.md#reuse', section: { anchor: 'reuse' } },
        records: [{ id: '0001:decision:1' }, { id: '0001:other' }],
      });
    });
  });

  test.each([
    {
      source: { path: '../outside.md', anchor: 'outside', section: 'Outside' },
      code: 'INVALID_RELATION_INDEX',
    },
    {
      source: { path: 'private.md', anchor: 'outside', section: 'Outside' },
      code: 'UNDECLARED_RELATION_SOURCE',
    },
    {
      source: { path: 'docs/missing.md', anchor: 'missing', section: 'Missing' },
      code: 'INVALID_RELATION_FILE',
    },
  ])(
    'rejects an unsafe, undeclared or missing referenced document: $source.path',
    ({ source, code }) => {
      withRelations((root) => {
        writeFileSync(join(root, 'private.md'), '# Outside\n');
        writeRelationIndex(root, [{ ...relationRule, supersededBy: [{ adr: '0002', source }] }]);
        commitChanges(root);
        const result = invoke(root, ['relations', 'docs/cache.md']);
        expect(result.status).toBe(1);
        expect(output(result.stderr)).toMatchObject({ error: { code } });
      });
    },
  );

  test('refuses a symlink index and reports malformed JSONL without affecting source reads', () => {
    withRelations((root) => {
      writeFileSync(join(root, 'docs', 'relations.jsonl'), '{not-json}\n');
      commitChanges(root);
      const malformed = invoke(root, ['relations', 'docs/cache.md']);
      expect(malformed.status).toBe(1);
      expect(output(malformed.stderr)).toMatchObject({
        error: { code: 'INVALID_RELATION_INDEX', details: { line: 1 } },
      });
      expect(invoke(root, ['read', 'docs/cache.md']).status).toBe(0);
      rmSync(join(root, 'docs', 'relations.jsonl'));
      symlinkSync('/etc/passwd', join(root, 'docs', 'relations.jsonl'));
      commitChanges(root);
      const linked = invoke(root, ['relations', 'docs/cache.md']);
      expect(linked.status).toBe(1);
      expect(output(linked.stderr)).toMatchObject({ error: { code: 'INVALID_RELATION_FILE' } });
    });
  });

  test('fails clearly when the next complete relation cannot fit the output budget', () => {
    withRelations((root) => {
      writeRelationIndex(root, [{ ...relationRule, text: 'é'.repeat(5000) }]);
      commitChanges(root);
      const result = invoke(root, ['relations', 'docs/cache.md', '--max-bytes', '1024']);
      expect(result.status).toBe(1);
      expect(Buffer.byteLength(result.stderr)).toBeLessThanOrEqual(1024);
      expect(output(result.stderr)).toMatchObject({
        error: { code: 'RELATION_EXCEEDS_BUDGET', details: { line: 2 } },
      });
    });
  });
  test('rejects declaring the same relation index twice', () => {
    withRelations((root) => {
      const index = { path: 'docs/relations.jsonl', format: 'compi-adr-supersession-index' };
      writeFileSync(
        join(root, 'hivex.json'),
        JSON.stringify({
          version: 1,
          collections: [{ id: 'product', include: ['docs/**/*.md'] }],
          relationIndexes: [index, index],
        }),
      );
      commitChanges(root);
      const result = invoke(root, ['relations', 'docs/cache.md']);
      expect(result.status).toBe(1);
      expect(output(result.stderr)).toMatchObject({ error: { code: 'INVALID_CONFIG' } });
    });
  });
  test('rejects duplicate record identities within one index while keeping source reading available', () => {
    withRelations((root) => {
      writeRelationIndex(root, [relationRule, { ...relationRule, status: 'superseded' }]);
      commitChanges(root);
      const result = invoke(root, ['relations', 'docs/cache.md']);
      expect(result.status).toBe(1);
      expect(output(result.stderr)).toMatchObject({ error: { code: 'DUPLICATE_RELATION_ID' } });
      expect(invoke(root, ['read', 'docs/cache.md']).status).toBe(0);
      expect(invoke(root, ['search', 'cache']).status).toBe(0);
    });
  });
  test('paginates complete indexed records in index order with scope-bound continuation', () => {
    withRelations((root) => {
      const ids = ['0001:z', '0001:b', '0001:a'];
      writeRelationIndex(
        root,
        ids.map((id) => ({
          ...relationRule,
          id,
          text: 'An indexed condition: ' + 'á'.repeat(700),
        })),
      );
      commitChanges(root);
      const schema = z.object({
        snapshot: z.object({ commit: z.string() }),
        records: z.array(z.object({ id: z.string(), indexedText: z.string() })),
        continuation: z.string().nullable(),
      });
      const first = invoke(root, [
        'relations',
        'docs/cache.md',
        '--limit',
        '1',
        '--max-bytes',
        '4096',
      ]);
      expect(first.status).toBe(0);
      const parsed = schema.parse(output(first.stdout));
      expect(parsed.records.map((row) => row.id)).toEqual(['0001:z']);
      if (!parsed.continuation) throw new Error('Expected continuation for the remaining records');
      const wrongSource = invoke(root, [
        'relations',
        'docs/revised.md',
        '--cursor',
        parsed.continuation,
      ]);
      expect(wrongSource.status).toBe(1);
      expect(output(wrongSource.stderr)).toMatchObject({ error: { code: 'CURSOR_MISMATCH' } });
      writeRelationIndex(root, [{ ...relationRule, id: '0001:new' }]);
      commitChanges(root);
      const wrongRevision = invoke(root, [
        'relations',
        'docs/cache.md',
        '--cursor',
        parsed.continuation,
      ]);
      expect(wrongRevision.status).toBe(1);
      expect(output(wrongRevision.stderr)).toMatchObject({ error: { code: 'CURSOR_MISMATCH' } });
      const found = parsed.records.map((row) => row.id);
      let cursor: string | null = parsed.continuation;
      for (let pages = 0; cursor !== null && pages < 10; pages += 1) {
        const next = invoke(root, [
          'relations',
          'docs/cache.md',
          '--ref',
          parsed.snapshot.commit,
          '--cursor',
          cursor,
          '--limit',
          '1',
          '--max-bytes',
          '4096',
        ]);
        expect(next.status).toBe(0);
        expect(Buffer.byteLength(next.stdout)).toBeLessThanOrEqual(4096);
        const page = schema.parse(output(next.stdout));
        found.push(...page.records.map((row) => row.id));
        expect(page.records[0]?.indexedText).toBe('An indexed condition: ' + 'á'.repeat(700));
        cursor = page.continuation;
      }
      expect(cursor).toBeNull();
      expect(found).toEqual(ids);
    });
  });
  test('reports missing and contained anchors without repairing them or hiding unresolved records', () => {
    withRelations((root) => {
      writeFileSync(
        join(root, 'docs', 'revised.md'),
        '# Replacement\n\n> ## Quoted replacement\n> Scoped evidence.\n',
      );
      writeRelationIndex(root, [
        {
          ...relationRule,
          id: '0001:unresolved',
          type: 'unresolved',
          status: 'unresolved',
          reason: 'No unique replacement was identified.',
          supersededBy: [],
        },
        {
          ...relationRule,
          supersededBy: [
            {
              adr: '0002',
              source: { path: 'docs/revised.md', anchor: 'retired-anchor', section: 'Old title' },
            },
            {
              adr: '0002',
              source: {
                path: 'docs/revised.md',
                anchor: 'quoted-replacement',
                section: 'Quoted replacement',
              },
            },
          ],
        },
      ]);
      commitChanges(root);
      const result = invoke(root, ['relations', 'docs/cache.md']);
      expect(result.status).toBe(0);
      expect(output(result.stdout)).toMatchObject({
        records: [
          { id: '0001:unresolved', indexedStatus: 'unresolved', supersededBy: [] },
          {
            id: '0001:decision:1',
            supersededBy: [
              { anchor: 'retired-anchor', resolution: 'missing-anchor', location: null },
              { anchor: 'quoted-replacement', resolution: 'contained-heading', location: null },
            ],
          },
        ],
      });
      const parsed = z
        .object({
          records: z.array(
            z.object({
              supersededBy: z.array(z.object({ path: z.string(), readCursor: z.string() })),
            }),
          ),
        })
        .parse(output(result.stdout));
      const target = parsed.records[1]?.supersededBy[0];
      if (!target) throw new Error('Expected a whole-document fallback');
      const full = invoke(root, ['read', target.path, '--cursor', target.readCursor]);
      expect(full.status).toBe(0);
      expect(page.parse(output(full.stdout)).blocks[0]?.text).toBe('# Replacement');
    });
  });
  test('traces a partial replacement to its indexed entry and opens the versioned evidence', () => {
    withRelations((root) => {
      const result = invoke(root, ['relations', 'docs/cache.md']);
      expect(result.status).toBe(0);
      expect(output(result.stdout)).toMatchObject({
        scope: 'document',
        basis: 'derived-index',
        currentness: 'not-established',
        freshness: 'not-established',
        order: 'index-not-precedence',
        source: { id: 'docs/cache.md', authority: { declaredStatus: 'accepted' } },
        records: [
          {
            id: '0001:decision:1',
            indexedStatus: 'partially-amended',
            indexedText: 'Cache reuse has a narrower condition.',
            indexEntry: { path: 'docs/relations.jsonl', line: 2 },
            subject: { path: 'docs/cache.md', anchor: 'cache-policy', resolution: 'resolved' },
            supersededBy: [
              {
                path: 'docs/revised.md',
                anchor: 'conditional-reuse',
                resolution: 'resolved',
                location: { lineStart: 3 },
              },
            ],
          },
        ],
        continuation: null,
      });
      const response = z
        .object({
          snapshot: z.object({ commit: z.string() }),
          records: z.array(
            z.object({
              supersededBy: z.array(z.object({ path: z.string(), readCursor: z.string() })),
            }),
          ),
        })
        .parse(output(result.stdout));
      const replacement = response.records[0]?.supersededBy[0];
      if (!replacement) throw new Error('Expected the indexed replacement');
      const opened = invoke(root, [
        'read',
        replacement.path,
        '--ref',
        response.snapshot.commit,
        '--cursor',
        replacement.readCursor,
      ]);
      expect(opened.status).toBe(0);
      expect(output(opened.stdout)).toMatchObject({
        blocks: expect.arrayContaining([
          expect.objectContaining({
            text: 'Reuse only when the source hash is unchanged and the schema remains compatible.',
          }),
        ]),
      });
    });
  });
});
