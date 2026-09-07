import { expect, test } from 'bun:test';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { bootstrapFixture, executeInstaller } from './bootstrap-fixture.ts';

test('refuses an altered bootstrap before executing its code', async () => {
  await bootstrapFixture({}, async (root) => {
    await writeFile(
      join(root, 'tooling/generated/check-dependencies.mjs'),
      'await Bun.write("executed", "yes");',
    );
    expect(executeInstaller(root).status).toBe(1);
    expect(await Bun.file(join(root, 'executed')).exists()).toBe(false);
    expect(await Bun.file(join(root, 'phases')).exists()).toBe(false);
  });
});

test.each(['fail', 'mutate-input'] as const)(
  'refuses preflight %s before installation',
  async (preflight) => {
    await bootstrapFixture({ preflight }, async (root) => {
      expect(executeInstaller(root).status).toBe(1);
      expect(await readFile(join(root, 'phases'), 'utf8')).toBe('preflight');
      expect(await Bun.file(join(root, 'bun.lock')).text()).toContain('lockfileVersion');
      expect(existsSync(join(root, 'node_modules'))).toBe(false);
    });
  },
);

test('reports a frozen installation failure without running the installed checker', async () => {
  await bootstrapFixture({}, async (root) => {
    await writeFile(join(root, 'bun.lock'), '{ invalid lock');
    const result = executeInstaller(root);
    expect(result.status).toBe(1);
    expect(result.stderr).toContain('install failed');
    expect(await readFile(join(root, 'phases'), 'utf8')).toBe('preflight');
  });
});

test('refuses an adverse installed check while preserving the unchanged inputs', async () => {
  await bootstrapFixture({ installed: 'fail' }, async (root) => {
    const before = await readFile(join(root, 'bun.lock'), 'utf8');
    expect(executeInstaller(root).status).toBe(1);
    expect(await readFile(join(root, 'phases'), 'utf8')).toBe('installed');
    expect(await readFile(join(root, 'bun.lock'), 'utf8')).toBe(before);
  });
});

test('installs the owned fixture after the checker permits it and reports all completed phases', async () => {
  await bootstrapFixture({}, async (root) => {
    const result = executeInstaller(root);
    expect(result.stderr).toBe('');
    expect(result.status).toBe(0);
    const report: unknown = JSON.parse(result.stdout);
    expect(report).toMatchObject({
      passed: true,
      scope: 'scripts-disabled-fresh-install',
      bunVersion: '1.4.2',
    });
  });
});

test('the actual bootstrap does not load an available external optional native addon', async () => {
  await bootstrapFixture({ actualChecker: true }, async (root) => {
    await mkdir(join(root, 'node_modules/.bun'), { recursive: true });
    await mkdir(join(root, 'node_modules/msgpackr-extract'), { recursive: true });
    await writeFile(
      join(root, 'node_modules/msgpackr-extract/package.json'),
      '{"name":"msgpackr-extract","version":"3.0.4","main":"index.js"}',
    );
    await writeFile(
      join(root, 'node_modules/msgpackr-extract/index.js'),
      'require("node:fs").writeFileSync("native-addon-executed", "yes"); throw new Error("Owned sentinel");',
    );
    const result = executeInstaller(root, { checkerOnly: true });
    expect(result.stderr).toBe('');
    expect(result.status).toBe(0);
    expect(await Bun.file(join(root, 'native-addon-executed')).exists()).toBe(false);
  });
});
