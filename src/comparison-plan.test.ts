import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { nativeProject, nativeSource } from '../test/native-project.ts';

const cli = join(import.meta.dirname, 'cli.ts');
function invoke(root: string, args: string[]) {
  return spawnSync(process.execPath, [cli, ...args, '--root', root], {
    encoding: 'utf8',
    timeout: 15000,
  });
}

test('plans a documented comparison without interpreting the link as semantic precedence', async () => {
  await nativeProject(
    (paths) => {
      expect(
        invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
      ).toBe(0);
      const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
      expect(built.status).toBe(0);
      const input = join(dirname(paths.store), 'graph.json');
      writeFileSync(input, built.stdout);
      const result = invoke(paths.root, ['graph', 'compare-plan', '--input', input]);
      expect(result.stderr).toBe('');
      expect(result.status).toBe(0);
      expect(JSON.parse(result.stdout)).toMatchObject({
        accepted: false,
        status: 'planned',
        pairs: [
          {
            sources: ['first.md', 'second.md'],
            reasons: [
              {
                kind: 'markdown-link',
                source: 'first.md',
                target: 'second.md',
                evidence: { quote: '[Other rule](second.md)', lineStart: 5, lineEnd: 5 },
              },
            ],
          },
        ],
        semanticRelationships: 'not-established',
      });
      expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
      expect(readFileSync(input, 'utf8')).toBe(built.stdout);
    },
    { source: nativeSource + '\n[Other rule](second.md)\n' },
  );
});

test('counts duplicate definition nodes toward the document limit before resolving identifiers', async () => {
  const body = nativeSource + '\n' + '[same]: second.md\n'.repeat(10000);
  await nativeProject(
    (paths) => {
      expect(invoke(paths.root, ['search', 'cache']).status).toBe(0);
      writeFileSync(join(paths.root, 'second.md'), body + '[same]: missing.md\n');
      paths.git(['add', 'second.md']);
      paths.git([
        '-c',
        'user.name=Test',
        '-c',
        'user.email=test@example.invalid',
        'commit',
        '-qm',
        'excessive definitions',
      ]);
      const invalid = invoke(paths.root, ['search', 'cache']);
      expect(invalid.status).toBe(1);
      expect(JSON.parse(invalid.stderr)).toMatchObject({
        error: { code: 'SOURCE_REFERENCE_LIMIT' },
      });
      expect(readFileSync(paths.calls, 'utf8')).toBe('');
    },
    { source: body },
  );
});

test('accepts the documented Markdown link limit and rejects the next link without partial output', async () => {
  const body = nativeSource + '\n' + '[x]() '.repeat(10000) + '\n\nTrailing text.\n';
  await nativeProject(
    (paths) => {
      const valid = invoke(paths.root, ['search', 'cache']);
      expect(valid.status).toBe(0);
      writeFileSync(join(paths.root, 'second.md'), body + '\n[x]()\n');
      paths.git(['add', 'second.md']);
      paths.git([
        '-c',
        'user.name=Test',
        '-c',
        'user.email=test@example.invalid',
        'commit',
        '-qm',
        'excessive references',
      ]);
      const invalid = invoke(paths.root, ['search', 'cache']);
      expect(invalid.status).toBe(1);
      expect(invalid.stdout).toBe('');
      expect(JSON.parse(invalid.stderr)).toMatchObject({
        error: { code: 'SOURCE_REFERENCE_LIMIT' },
      });
      expect(readFileSync(paths.calls, 'utf8')).toBe('');
    },
    { source: body },
  );
});

test('fails instead of exporting an incomplete comparison plan when evidence exceeds the output budget', async () => {
  const label = 'A complete authored reference '.repeat(60).trim();
  await nativeProject(
    (paths) => {
      expect(
        invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
      ).toBe(0);
      const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
      const input = join(dirname(paths.store), 'graph.json');
      writeFileSync(input, built.stdout);
      const limited = invoke(paths.root, [
        'graph',
        'compare-plan',
        '--input',
        input,
        '--max-bytes',
        '1024',
      ]);
      expect(limited.status).toBe(1);
      expect(limited.stdout).toBe('');
      expect(JSON.parse(limited.stderr)).toMatchObject({
        error: { code: 'COMPARISON_PLAN_BUDGET' },
      });
      const complete = invoke(paths.root, [
        'graph',
        'compare-plan',
        '--input',
        input,
        '--max-bytes',
        '65536',
      ]);
      expect(complete.status).toBe(0);
      expect(JSON.parse(complete.stdout)).toMatchObject({
        pairs: [{ reasons: [{ evidence: { quote: `[${label}](second.md)` } }] }],
      });
      expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
    },
    { source: nativeSource + `\n[${label}](second.md)\n` },
  );
});

test('resolves reference-style links from their original definitions and limits links to the selected section', async () => {
  const first =
    nativeSource +
    '\n## Selected\n\nNever treat a cache as authority.\n[Other rule][target]\n\n## Unselected\n\n[Ignore](missing.md)\n\n[TaRgEt]: second.md#cache\n';
  await nativeProject(
    (paths) => {
      writeFileSync(join(paths.root, 'second.md'), nativeSource);
      writeFileSync(
        join(paths.root, 'hivex.json'),
        JSON.stringify({
          version: 1,
          collections: [
            { id: 'project', include: [{ path: 'first.md', anchor: 'selected' }, 'second.md'] },
          ],
        }),
      );
      paths.git(['add', 'second.md', 'hivex.json']);
      paths.git([
        '-c',
        'user.name=Test',
        '-c',
        'user.email=test@example.invalid',
        'commit',
        '-qm',
        'selected source',
      ]);
      for (const line of [7, 3]) {
        writeFileSync(
          paths.candidate,
          JSON.stringify({
            claims: [
              {
                id: 'c1',
                text: 'Caches are not authority.',
                kind: 'constraint',
                conditions: [],
                exceptions: [],
                evidence: [
                  { quote: 'Never treat a cache as authority.', lineStart: line, lineEnd: line },
                ],
              },
            ],
            relations: [],
          }),
        );
        expect(
          invoke(paths.root, [
            'ingest',
            '--store',
            paths.store,
            '--codex',
            paths.binary,
            '--max-units',
            '1',
          ]).status,
        ).toBe(0);
      }
      const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
      expect(built.status).toBe(0);
      const input = join(dirname(paths.store), 'graph.json');
      writeFileSync(input, built.stdout);
      const result = invoke(paths.root, ['graph', 'compare-plan', '--input', input]);
      expect(result.stderr).toBe('');
      expect(result.status).toBe(0);
      expect(JSON.parse(result.stdout)).toMatchObject({
        pairs: [
          {
            sources: ['first.md#selected', 'second.md'],
            reasons: [
              {
                document: 'first.md',
                evidence: { quote: '[Other rule][target]', lineStart: 8, lineEnd: 8 },
                definition: { quote: '[TaRgEt]: second.md#cache', lineStart: 14, lineEnd: 14 },
              },
            ],
          },
        ],
        unresolved: [],
        coverage: { links: 1 },
      });
      expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
    },
    { source: first },
  );
});

test('reports unresolved Markdown targets while ignoring links inside code and never following external links', async () => {
  const body =
    nativeSource +
    '\n[Broken](missing.md)\n[Escape](../outside.md)\n[External](https://example.invalid/remote.md)\n[Asset](image.png)\n[Markdown](missing.markdown)\n[MDOWN](missing.MDOWN)\n\n```md\n[Code example](also-missing.md)\n```\n';
  await nativeProject(
    (paths) => {
      expect(
        invoke(paths.root, ['ingest', '--store', paths.store, '--codex', paths.binary]).status,
      ).toBe(0);
      const built = invoke(paths.root, ['graph', 'build', '--store', paths.store, '--export']);
      const input = join(dirname(paths.store), 'graph.json');
      writeFileSync(input, built.stdout);
      const result = invoke(paths.root, ['graph', 'compare-plan', '--input', input]);
      expect(result.status).toBe(1);
      const plan: { unresolved: { url: string; reason: string }[] } = JSON.parse(result.stdout);
      expect(plan.unresolved.map((item) => item.url)).toEqual([
        'missing.md',
        '../outside.md',
        'missing.markdown',
        'missing.MDOWN',
        'missing.md',
        '../outside.md',
        'missing.markdown',
        'missing.MDOWN',
      ]);
      expect(JSON.parse(result.stdout)).toMatchObject({
        accepted: false,
        pairs: [],
        coverage: { links: 12, externalLinks: 2, nonMarkdownLinks: 2 },
      });
      expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
    },
    { source: body },
  );
});
