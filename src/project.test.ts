import { expect, test } from 'bun:test';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { join } from 'node:path';
import { devNull } from 'node:os';
import { z } from 'zod';

const root = join(import.meta.dirname, '..');
const cli = join(import.meta.dirname, 'cli.ts');

function git(args: string[]) {
  const result = spawnSync('git', ['--no-lazy-fetch', '--no-replace-objects', ...args], {
    cwd: root,
    encoding: 'utf8',
    timeout: 10_000,
    maxBuffer: 1_048_576,
    env: {
      ...Object.fromEntries(
        Object.entries(process.env).filter(([name]) => !name.startsWith('GIT_')),
      ),
      GIT_CONFIG_GLOBAL: devNull,
      GIT_CONFIG_NOSYSTEM: '1',
    },
  });
  if (result.status !== 0) throw new Error(result.stderr);
  return result.stdout;
}

const commit = git(['rev-parse', '--verify', 'HEAD']).trim();
const config = git(['show', `${commit}:hivex.json`]);
const configHash = createHash('sha256').update(config).digest('hex');
const project = z
  .object({
    collections: z
      .array(
        z.object({
          id: z.string(),
          include: z.array(z.union([z.string(), z.object({ path: z.string() })])),
          exclude: z.array(z.string()).default([]),
        }),
      )
      .min(1),
    relationIndexes: z.array(z.object({ path: z.string() })).default([]),
  })
  .parse(JSON.parse(config));

test.each(project.collections)(
  'reads every declared source in the committed project collection: $id',
  ({ id }) => {
    const result = spawnSync(
      process.execPath,
      [
        cli,
        'search',
        'hivexconfigurationvalidation',
        '--root',
        root,
        '--ref',
        commit,
        '--collection',
        id,
        '--limit',
        '1',
        '--max-bytes',
        '65536',
      ],
      { encoding: 'utf8', timeout: 15_000, maxBuffer: 1_048_576 },
    );
    expect(result.stderr).toBe('');
    expect(result.status).toBe(0);
    const response: unknown = JSON.parse(result.stdout);
    expect(response).toMatchObject({ snapshot: { commit, configHash } });
  },
  20_000,
);

test.each(project.relationIndexes)(
  'validates all referenced documents in the committed index through the CLI: $path',
  ({ path }) => {
    const contents = git(['show', `${commit}:${path}`]);
    const source = fixtureSource();
    const result = spawnSync(
      process.execPath,
      [cli, 'relations', source, '--root', root, '--ref', commit, '--max-bytes', '65536'],
      { encoding: 'utf8', timeout: 15_000, maxBuffer: 1_048_576 },
    );
    expect(result.stderr).toBe('');
    expect(result.status).toBe(0);
    const response: unknown = JSON.parse(result.stdout);
    expect(response).toMatchObject({
      snapshot: { commit, configHash },
      indexes: expect.arrayContaining([
        expect.objectContaining({
          path,
          hash: createHash('sha256').update(contents).digest('hex'),
        }),
      ]),
      currentness: 'not-established',
      freshness: 'not-established',
    });
  },
  20_000,
);

function fixtureSource() {
  const paths = git(['ls-tree', '-rz', '--name-only', commit]).split('\0');
  const source = paths.find(
    (path) =>
      /\.(?:md|markdown|mdown)$/i.test(path) &&
      project.collections.some(
        (collection) =>
          collection.include.some((include) =>
            typeof include === 'string' ? new Bun.Glob(include).match(path) : include.path === path,
          ) && !collection.exclude.some((exclude) => new Bun.Glob(exclude).match(path)),
      ),
  );
  if (!source) throw new Error('The project smoke fixture needs a declared Markdown source');
  return source;
}
