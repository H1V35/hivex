import { expect, test } from 'bun:test';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { verifyInstalledPeers } from './peers.ts';

async function fixture(run: (root: string) => Promise<void>) {
  const root = await mkdtemp(join(tmpdir(), 'hivex-peer-'));
  try {
    await mkdir(join(root, 'node_modules', '.bun'), { recursive: true });
    await run(root);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

async function packageAt(directory: string, value: object) {
  await mkdir(directory, { recursive: true });
  await writeFile(join(directory, 'package.json'), JSON.stringify(value));
}

test('applies an existing allowance to its exact parent and still rejects another parent', async () => {
  await fixture(async (root) => {
    for (const name of ['eslint-plugin-sonarjs', 'another-consumer'])
      await packageAt(join(root, 'node_modules', '.bun', `${name}@1.0.0`, 'node_modules', name), {
        name,
        version: '1.0.0',
        peerDependencies: { eslint: '^9.0.0' },
      });
    await packageAt(join(root, 'node_modules', 'eslint'), {
      name: 'eslint',
      version: '10.7.0',
    });
    const result = await verifyInstalledPeers(root, {
      'eslint-plugin-sonarjs>eslint': '10',
    });
    expect(result.passed).toBe(false);
    expect(
      result.bindings.find((binding) => binding.consumer === 'eslint-plugin-sonarjs')?.passed,
    ).toBe(true);
    expect(result.issues.map((issue) => issue.consumer)).toEqual(['another-consumer']);
  });
});

test('permits an absent optional peer but rejects an installed incompatible optional peer', async () => {
  await fixture(async (root) => {
    await packageAt(
      join(root, 'node_modules', '.bun', 'consumer@1.0.0', 'node_modules', 'consumer'),
      {
        name: 'consumer',
        version: '1.0.0',
        peerDependencies: { optional: '^1.0.0' },
        peerDependenciesMeta: { optional: { optional: true } },
      },
    );
    expect((await verifyInstalledPeers(root, {})).passed).toBe(true);
    await packageAt(join(root, 'node_modules', 'optional'), {
      name: 'optional',
      version: '2.0.0',
    });
    expect((await verifyInstalledPeers(root, {})).passed).toBe(false);
  });
});
