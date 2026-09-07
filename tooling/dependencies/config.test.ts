import { expect, test } from 'bun:test';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { loadPolicy } from './config.ts';

test('requires explicit trusted dependencies and actual Bun install policy fields', async () => {
  const root = await mkdtemp(join(tmpdir(), 'hivex-install-policy-'));
  try {
    await writeFile(
      join(root, 'dependency-policy.json'),
      JSON.stringify({
        version: 1,
        minimumReleaseAgeSeconds: 604800,
        trustPolicy: 'no-downgrade',
        trustExclusions: [],
        allowedInstallScripts: {},
        deniedInstallScripts: {},
        peerVersionAllowances: {},
      }),
    );
    await writeFile(join(root, 'package.json'), JSON.stringify({ name: 'fixture' }));
    await writeFile(
      join(root, 'bunfig.toml'),
      '[install]\nlinker="isolated"\nminimumReleaseAge=604800\n',
    );
    expect(
      await loadPolicy(root).then(
        () => null,
        (error: unknown) => error,
      ),
    ).toBeInstanceOf(Error);
    await writeFile(
      join(root, 'package.json'),
      JSON.stringify({ name: 'fixture', trustedDependencies: [] }),
    );
    expect((await loadPolicy(root)).policy.minimumReleaseAgeSeconds).toBe(604800);
    const policyText = await readFile(join(root, 'dependency-policy.json'), 'utf8');
    await writeFile(
      join(root, 'dependency-policy.json'),
      policyText.replace('"trustExclusions":[]', '"trustExclusions":["example@*"]'),
    );
    expect(
      await loadPolicy(root).then(
        () => null,
        (error: unknown) => error,
      ),
    ).toBeInstanceOf(Error);
    await writeFile(join(root, 'dependency-policy.json'), policyText);
    await writeFile(join(root, 'bunfig.toml'), 'linker="isolated"\nminimumReleaseAge=604800\n');
    expect(
      await loadPolicy(root).then(
        () => null,
        (error: unknown) => error,
      ),
    ).toBeInstanceOf(Error);
    await writeFile(
      join(root, 'bunfig.toml'),
      '[install]\nlinker="isolated"\nminimumReleaseAge=604800\n',
    );
    await writeFile(
      join(root, 'package.json'),
      JSON.stringify({ name: 'fixture', trustedDependencies: ['unapproved'] }),
    );
    expect(
      await loadPolicy(root).then(
        () => null,
        (error: unknown) => error,
      ),
    ).toBeInstanceOf(Error);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
