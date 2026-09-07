import { expect, test } from 'bun:test';
import { inspectLock } from './lock.ts';
import { verifyInventory } from './inventory.ts';

test('rejects missing or extra installed identities while respecting canonical aliases and platform constraints', () => {
  const integrity = `sha512-${Buffer.alloc(64, 7).toString('base64')}`;
  const lock = inspectLock(
    JSON.stringify({
      lockfileVersion: 2,
      configVersion: 1,
      workspaces: { '': { name: 'hivex' } },
      packages: {
        compiler: ['typescript@7.0.2', '', {}, integrity],
        native: ['native@1.0.0', '', { os: ['darwin', '!linux'], cpu: 'arm64' }, integrity],
      },
    }),
  );
  const platform = { os: 'linux', cpu: 'x64' };
  expect(verifyInventory(lock, [{ name: 'typescript', version: '7.0.2' }], platform).passed).toBe(
    true,
  );
  expect(verifyInventory(lock, [{ name: 'typescript', version: '7.0.1' }], platform)).toMatchObject(
    {
      passed: false,
      missing: ['typescript@7.0.2'],
      unexpected: ['typescript@7.0.1'],
    },
  );
  expect(verifyInventory(lock, [], platform).passed).toBe(false);
  expect(
    verifyInventory(lock, [{ name: 'typescript', version: '7.0.2' }], {
      os: 'darwin',
      cpu: 'arm64',
    }).missing,
  ).toEqual(['native@1.0.0']);
});
