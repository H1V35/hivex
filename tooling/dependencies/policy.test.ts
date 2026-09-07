import { expect, test } from 'bun:test';
import { inspectLock } from './lock.ts';
import { verifyRegistry } from './verify.ts';

const integrity = `sha512-${Buffer.alloc(64, 7).toString('base64')}`;

test('retains alias locators while validating canonical registry identities and lock versions', () => {
  const lock = `{
    // Bun lockfile v2 preserves aliases in the package locator.
    "lockfileVersion": 2, "configVersion": 1,
    "workspaces": { "": { "name": "hivex", "devDependencies": { "compiler": "npm:typescript@7.0.2" } } },
    "packages": { "compiler": ["typescript@7.0.2", "", {}, "${integrity}"], },
  }`;
  expect(inspectLock(lock)).toMatchObject({
    registry: [{ locator: 'compiler', name: 'typescript', version: '7.0.2', integrity }],
    workspaces: [{ path: '', name: 'hivex' }],
  });
  expect(() => inspectLock(lock.replace('"lockfileVersion": 2', '"lockfileVersion": 3'))).toThrow();
});

test('retains the platform constraint shapes emitted by the admitted Bun lock writer', () => {
  const value = inspectLock(
    JSON.stringify({
      lockfileVersion: 2,
      configVersion: 1,
      workspaces: { '': { name: 'hivex' } },
      packages: {
        compiler: ['typescript@7.0.2', '', { os: 'darwin', cpu: ['arm64', 'x64'] }, integrity],
      },
    }),
  );
  expect(value.registry[0]?.metadata).toEqual({
    os: 'darwin',
    cpu: ['arm64', 'x64'],
  });
});

test('rejects mismatched integrity and missing version time even when package metadata is old', async () => {
  const lock = JSON.stringify({
    lockfileVersion: 2,
    configVersion: 1,
    workspaces: { '': { name: 'hivex' } },
    packages: { example: ['example@1.0.0', '', {}, integrity] },
  });
  const packument = {
    name: 'example',
    modified: '2000-01-01T00:00:00.000Z',
    'dist-tags': { latest: '1.0.0' },
    versions: {
      '1.0.0': {
        name: 'example',
        version: '1.0.0',
        dist: {
          tarball: 'https://registry.npmjs.org/example/-/example-1.0.0.tgz',
          integrity: `sha512-${Buffer.alloc(64, 8).toString('base64')}`,
        },
      },
    },
  };
  const result = await verifyRegistry({
    lock: inspectLock(lock),
    packuments: new Map([['example', packument]]),
    now: Date.parse('2026-09-07T00:00:00Z'),
  });
  expect(result.passed).toBe(false);
  expect(result.issues).toEqual(
    expect.arrayContaining([
      expect.objectContaining({ code: 'SRI_MISMATCH' }),
      expect.objectContaining({ code: 'MISSING_PUBLICATION_TIME' }),
    ]),
  );
});

test('uses the public trust verifier and keeps exclusions exact rather than allowing a neighboring downgrade', async () => {
  const manifest = (version: string, trusted: boolean) => ({
    name: 'example',
    version,
    dist: {
      tarball: `https://registry.npmjs.org/example/-/example-${version}.tgz`,
      integrity,
      ...(trusted ? { attestations: { provenance: {} } } : {}),
    },
    ...(trusted ? { _npmUser: { trustedPublisher: { id: 'fixture-publisher' } } } : {}),
  });
  const packument = {
    name: 'example',
    'dist-tags': { latest: '1.2.0' },
    modified: '2022-01-01T00:00:00Z',
    versions: {
      '1.0.0': manifest('1.0.0', true),
      '1.1.0': manifest('1.1.0', false),
      '1.2.0': manifest('1.2.0', false),
    },
    time: {
      '1.0.0': '2020-01-01T00:00:00Z',
      '1.1.0': '2021-01-01T00:00:00Z',
      '1.2.0': '2022-01-01T00:00:00Z',
    },
  };
  const check = async (version: string, trustExclusions: string[] = []) =>
    verifyRegistry({
      lock: inspectLock(
        JSON.stringify({
          lockfileVersion: 2,
          configVersion: 1,
          workspaces: { '': { name: 'hivex' } },
          packages: { example: [`example@${version}`, '', {}, integrity] },
        }),
      ),
      packuments: new Map([['example', packument]]),
      now: Date.parse('2026-09-07T00:00:00Z'),
      trustExclusions,
    });
  expect((await check('1.0.0')).passed).toBe(true);
  expect((await check('1.1.0')).issues).toEqual(
    expect.arrayContaining([expect.objectContaining({ code: 'TRUST_DOWNGRADE' })]),
  );
  expect((await check('1.1.0', ['example@1.1.0'])).passed).toBe(true);
  expect((await check('1.2.0', ['example@1.1.0'])).passed).toBe(false);
});
