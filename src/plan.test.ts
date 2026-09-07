import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { z } from 'zod';

const cli = join(import.meta.dirname, 'cli.ts');
const planPage = z.object({
  planHash: z.string(),
  snapshot: z.object({ commit: z.string() }),
  summary: z.object({ sourceCount: z.number() }),
  units: z.array(z.object({ id: z.string() })),
  continuation: z.string().nullable(),
});

function invokePlan(root: string, args: string[] = []) {
  return spawnSync(process.execPath, [cli, 'plan', '--root', root, ...args], {
    encoding: 'utf8',
    timeout: 10_000,
    env: { ...process.env, PATH: '/usr/bin:/bin' },
  });
}

function commit(root: string) {
  git(root, ['add', '.']);
  git(root, [
    '-c',
    'user.name=Test',
    '-c',
    'user.email=test@example.invalid',
    'commit',
    '-qm',
    'update',
  ]);
}

function git(root: string, args: string[]) {
  const result = spawnSync('git', args, {
    cwd: root,
    encoding: 'utf8',
    env: { ...process.env, GIT_CONFIG_GLOBAL: '/dev/null', GIT_CONFIG_NOSYSTEM: '1' },
  });
  if (result.status !== 0) throw new Error(result.stderr);
  return result.stdout.trim();
}

function withProject(run: (root: string) => void) {
  const root = mkdtempSync(join(tmpdir(), 'hivex-plan-'));
  try {
    git(root, ['init', '-q', '--initial-branch=main']);
    writeFileSync(join(root, 'policy.md'), '# Cache\n\nNever treat a cache as authority.\n');
    writeFileSync(
      join(root, 'hivex.json'),
      JSON.stringify({ version: 1, collections: [{ id: 'project', include: ['policy.md'] }] }),
    );
    git(root, ['add', '.']);
    git(root, [
      '-c',
      'user.name=Test',
      '-c',
      'user.email=test@example.invalid',
      'commit',
      '-qm',
      'fixture',
    ]);
    run(root);
    expect(git(root, ['status', '--porcelain'])).toBe('');
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

test('plans source-bound extraction without a model or workspace mutation', () => {
  withProject((root) => {
    const result = spawnSync(process.execPath, [cli, 'plan', '--root', root], {
      encoding: 'utf8',
      timeout: 10_000,
      env: { ...process.env, PATH: '/usr/bin:/bin' },
    });
    expect(result.stderr).toBe('');
    expect(result.status).toBe(0);
    const output: unknown = JSON.parse(result.stdout);
    expect(output).toMatchObject({
      command: 'plan',
      accepted: false,
      planHash: expect.stringMatching(/^[a-f0-9]{64}$/),
      snapshot: { commit: git(root, ['rev-parse', 'HEAD']) },
      summary: { sourceCount: 1, sourceBytes: 43, oversizedSources: 0, modelCalls: 0 },
      processing: {
        model: { name: 'gpt-5.6-luna', effort: 'max', provider: 'openai' },
        maximumSourceBytes: 32_768,
      },
      units: [
        {
          id: 'policy.md',
          path: 'policy.md',
          collection: 'project',
          sourceBytes: 43,
          contentHash: '05cb71b7bfdd3070b902ab081d4954749696cae83f431375f0f548db96020441',
          basePromptHash: '7899dc5aba3c066dc90194ba44155ef909c6e49abf61e264235abbda0aefedd9',
          readiness: 'extractable',
        },
      ],
      continuation: null,
    });
  });
});

test('paginates the complete cohort within a byte budget without changing its identity', () => {
  withProject((root) => {
    const ids = ['a.md', 'b.md', 'c.md', 'd.md', 'e.md', 'f.md', 'policy.md'];
    for (const id of ids.slice(0, -1)) writeFileSync(join(root, id), '# Scope\n\nKeep evidence.\n');
    writeFileSync(
      join(root, 'hivex.json'),
      JSON.stringify({ version: 1, collections: [{ id: 'project', include: ['*.md'] }] }),
    );
    git(root, ['add', '.']);
    git(root, [
      '-c',
      'user.name=Test',
      '-c',
      'user.email=test@example.invalid',
      'commit',
      '-qm',
      'cohort',
    ]);
    const seen: string[] = [];
    const hashes = new Set<string>();
    let cursor: string | null = null;
    for (let attempt = 0; attempt < 10; attempt++) {
      const args = [cli, 'plan', '--root', root, '--max-bytes', '2048', '--limit', '2'];
      if (cursor) args.push('--cursor', cursor);
      const result = spawnSync(process.execPath, args, { encoding: 'utf8', timeout: 10_000 });
      expect(result.stderr).toBe('');
      expect(result.status).toBe(0);
      expect(Buffer.byteLength(result.stdout)).toBeLessThanOrEqual(2048);
      const page = planPage.parse(JSON.parse(result.stdout));
      expect(page.summary.sourceCount).toBe(ids.length);
      expect(page.units.length).toBeLessThanOrEqual(2);
      hashes.add(page.planHash);
      seen.push(...page.units.map((unit) => unit.id));
      cursor = page.continuation;
      if (!cursor) break;
    }
    expect(cursor).toBeNull();
    expect(seen).toEqual(ids);
    expect(hashes.size).toBe(1);
  });
});

test('requires explicit legacy selection and reports oversized UTF-8 sources without hiding them', () => {
  withProject((root) => {
    const source = '# History\n\n' + 'ñ'.repeat(17_000) + '\n';
    writeFileSync(join(root, 'legacy.md'), source);
    writeFileSync(
      join(root, 'hivex.json'),
      JSON.stringify({
        version: 1,
        collections: [
          { id: 'project', include: ['policy.md'] },
          { id: 'legacy', include: ['legacy.md'], kind: 'legacy', default: false },
        ],
      }),
    );
    commit(root);
    const normal = planPage.parse(JSON.parse(invokePlan(root).stdout));
    expect(normal.units.map((unit) => unit.id)).toEqual(['policy.md']);
    const explicit = invokePlan(root, ['--collection', 'legacy']);
    expect(explicit.status).toBe(0);
    const result: unknown = JSON.parse(explicit.stdout);
    expect(result).toMatchObject({
      accepted: false,
      summary: { sourceCount: 1, oversizedSources: 1, modelCalls: 0 },
      units: [
        { id: 'legacy.md', sourceBytes: Buffer.byteLength(source), readiness: 'requires-section' },
      ],
    });
    const unknown = invokePlan(root, ['--collection', 'missing']);
    expect(unknown.status).toBe(1);
    expect(unknown.stderr).toContain('UNKNOWN_COLLECTION');
  });
});

test('binds continuation to its exact snapshot and selection while old revisions remain resumable', () => {
  withProject((root) => {
    writeFileSync(join(root, 'a.md'), '# Earlier\n\nKeep both sources.\n');
    writeFileSync(
      join(root, 'hivex.json'),
      JSON.stringify({ version: 1, collections: [{ id: 'project', include: ['*.md'] }] }),
    );
    commit(root);
    const first = planPage.parse(JSON.parse(invokePlan(root, ['--limit', '1']).stdout));
    const cursor = z.string().parse(first.continuation);
    writeFileSync(join(root, 'a.md'), '# Changed\n\nUse the new decision.\n');
    commit(root);
    const changed = invokePlan(root, ['--cursor', cursor]);
    expect(changed.status).toBe(1);
    expect(changed.stderr).toContain('CURSOR_MISMATCH');
    const original = invokePlan(root, ['--ref', first.snapshot.commit, '--cursor', cursor]);
    expect(original.status).toBe(0);
    const resumed = planPage.parse(JSON.parse(original.stdout));
    expect(resumed.planHash).toBe(first.planHash);
    expect(resumed.units.map((unit) => unit.id)).toEqual(['policy.md']);
    expect(resumed.continuation).toBeNull();
    const otherScope = invokePlan(root, [
      '--ref',
      first.snapshot.commit,
      '--collection',
      'project',
      '--cursor',
      cursor,
    ]);
    expect(otherScope.status).toBe(1);
    expect(otherScope.stderr).toContain('CURSOR_MISMATCH');
    const otherCommand = invokePlan(root, ['--cursor', cursor.replace('p1.', 'r1.')]);
    expect(otherCommand.status).toBe(1);
    expect(otherCommand.stderr).toContain('INVALID_CURSOR');
  });
});

test('reports the required budget for an indivisible unit and preserves every replacement reference', () => {
  withProject((root) => {
    const replacements = Array.from({ length: 8 }, (_, index) => `z${'é'.repeat(100)}${index}.md`);
    for (const path of replacements) writeFileSync(join(root, path), '# Replacement\n');
    writeFileSync(
      join(root, 'policy.md'),
      `---\nsuperseded_by: ${JSON.stringify(replacements)}\n---\n\n# Policy\n\nRead every exception.\n`,
    );
    writeFileSync(
      join(root, 'hivex.json'),
      JSON.stringify({ version: 1, collections: [{ id: 'project', include: ['*.md'] }] }),
    );
    commit(root);
    const small = invokePlan(root, ['--limit', '1', '--max-bytes', '1024']);
    expect(small.status).toBe(1);
    const error = z
      .object({
        error: z.object({
          code: z.literal('PLAN_UNIT_EXCEEDS_BUDGET'),
          details: z.object({ requiredBytes: z.number() }),
        }),
      })
      .parse(JSON.parse(small.stderr));
    const required = error.error.details.requiredBytes;
    const expanded = invokePlan(root, ['--limit', '1', '--max-bytes', String(required)]);
    expect(expanded.status).toBe(0);
    expect(Buffer.byteLength(expanded.stdout)).toBe(required);
    const result: unknown = JSON.parse(expanded.stdout);
    expect(result).toMatchObject({
      units: [{ id: 'policy.md', authority: { supersededBy: replacements } }],
    });
  });
});

test('plans each declared section with its original lines and distinct extraction input', () => {
  withProject((root) => {
    writeFileSync(
      join(root, 'policy.md'),
      '# Guide\n\n## Alpha\n\nKeep evidence.\n\n## Beta\n\nNever invent authority.\n',
    );
    writeFileSync(
      join(root, 'hivex.json'),
      JSON.stringify({
        version: 1,
        collections: [
          {
            id: 'project',
            include: [
              { path: 'policy.md', anchor: 'alpha' },
              { path: 'policy.md', anchor: 'beta' },
            ],
          },
        ],
      }),
    );
    commit(root);
    const result = invokePlan(root);
    expect(result.status).toBe(0);
    const output: unknown = JSON.parse(result.stdout);
    expect(output).toMatchObject({
      summary: { sourceCount: 2, oversizedSources: 0 },
      units: [
        {
          id: 'policy.md#alpha',
          section: { anchor: 'alpha', lineStart: 3, lineEnd: 5 },
          sourceBytes: Buffer.byteLength('## Alpha\n\nKeep evidence.\n\n'),
        },
        {
          id: 'policy.md#beta',
          section: { anchor: 'beta', lineStart: 7, lineEnd: 9 },
          sourceBytes: Buffer.byteLength('## Beta\n\nNever invent authority.\n'),
        },
      ],
    });
    const parsed = z
      .object({ units: z.array(z.object({ contentHash: z.string(), basePromptHash: z.string() })) })
      .parse(output);
    expect(new Set(parsed.units.map((unit) => unit.contentHash)).size).toBe(1);
    expect(new Set(parsed.units.map((unit) => unit.basePromptHash)).size).toBe(2);
  });
});

test.each([
  ['--limit', '0'],
  ['--limit', '21'],
  ['--limit', '1.5'],
  ['--max-bytes', '1023'],
  ['--max-bytes', '65537'],
  ['--root', ''],
  ['--ref', ''],
  ['--cursor', ''],
  ['--collection', 'bad/collection'],
  ['--codex', 'unexpected'],
])('rejects invalid plan option %s %s before processing', (flag, value) => {
  withProject((root) => {
    const result = invokePlan(root, [flag, value]);
    expect(result.status).toBe(1);
    expect(result.stdout).toBe('');
    expect(result.stderr).toContain('INVALID_ARGUMENT');
  });
});
