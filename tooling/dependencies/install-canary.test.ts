import { expect, test } from 'bun:test';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import {
  canaryName,
  install,
  verifiedFlags,
  withInstallCanary,
  writeProject,
} from './install-canary-fixture.ts';

test('the admitted Bun install rejects altered lock integrity after a warm install and repairs altered files', async () => {
  await withInstallCanary(async ({ root, packageRoot }) => {
    expect((await install({ root, fresh: false })).code).toBe(0);
    const lockPath = join(root, 'bun.lock');
    const lock = await readFile(lockPath, 'utf8');
    const corrupted = lock.replace(
      /sha512-[A-Za-z0-9+/=]+/,
      `sha512-${Buffer.alloc(64, 9).toString('base64')}`,
    );
    expect(corrupted).not.toBe(lock);
    await writeFile(lockPath, corrupted);
    expect((await install({ root, fresh: true, flags: verifiedFlags })).code).not.toBe(0);
    expect(await readFile(lockPath, 'utf8')).toBe(corrupted);
    await writeFile(lockPath, lock);
    await writeFile(join(packageRoot, 'content.txt'), 'altered');
    const result = await install({ root, fresh: true, flags: verifiedFlags });
    expect(result).toMatchObject({ code: 0 });
    expect(await readFile(join(packageRoot, 'content.txt'), 'utf8')).toBe('original');
    expect(await readFile(lockPath, 'utf8')).toBe(lock);
    expect(await Bun.file(join(packageRoot, 'executed.txt')).exists()).toBe(false);
  });
}, 90000);

test('Bun reads the configured age floor and only runs an explicitly trusted owned script', async () => {
  await withInstallCanary(async ({ root, packageRoot }) => {
    expect((await install({ root, fresh: false })).code).toBe(0);
    const manifest: unknown = JSON.parse(await readFile(join(packageRoot, 'package.json'), 'utf8'));
    expect(manifest).toMatchObject({ version: '1.0.0' });
    expect(await Bun.file(join(packageRoot, 'executed.txt')).exists()).toBe(false);
    await writeProject({ root, trusted: [canaryName] });
    expect((await install({ root, fresh: true, flags: ['--force', '--no-cache'] })).code).toBe(0);
    expect(await readFile(join(packageRoot, 'executed.txt'), 'utf8')).toBe('executed');
  });
}, 90000);
