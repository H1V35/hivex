import { describe, expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
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
