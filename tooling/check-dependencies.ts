import { lstat, readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { inspectLock } from './dependencies/lock.ts';
import { downloadMetadata } from './dependencies/metadata.ts';
import { verifyRegistry } from './dependencies/verify.ts';
import { classifyScripts, loadPolicy } from './dependencies/config.ts';
import { verifyInstalledPeers } from './dependencies/peers.ts';
import { verifyInventory } from './dependencies/inventory.ts';

async function main() {
  const { values } = parseArgs({
    args: Bun.argv.slice(2),
    strict: true,
    options: {
      lockfile: { type: 'string', default: 'bun.lock' },
      'installed-only': { type: 'boolean' },
    },
  });
  const path = resolve(values.lockfile);
  const stat = await lstat(path);
  if (!stat.isFile() || stat.size > 16 * 1024 * 1024)
    throw new Error('Lockfile must be a regular file of at most16MiB');
  const lock = inspectLock(await readFile(path, 'utf8'));
  const root = dirname(path);
  const config = await loadPolicy(root);
  if (values['installed-only']) {
    const peers = await verifyInstalledPeers(root, config.policy.peerVersionAllowances);
    const inventory = verifyInventory(lock, peers.packages);
    const passed = peers.passed && inventory.passed;
    process.stdout.write(
      JSON.stringify({
        scope: 'installed-inventory-and-peer-bindings',
        lockHash: lock.lockHash,
        configHashes: config.hashes,
        ...peers,
        inventory,
        passed,
      }) + '\n',
    );
    if (!passed) process.exitCode = 1;
    return;
  }
  const metadata = await downloadMetadata(lock.registry.map((entry) => entry.name));
  const report = await verifyRegistry({
    lock,
    packuments: metadata.packuments,
    now: Date.now(),
    trustExclusions: config.policy.trustExclusions,
  });
  const scripts = classifyScripts(config, report);
  const passed = report.passed && scripts.passed;
  process.stdout.write(
    JSON.stringify({
      scope: 'preinstall-registry-and-script-policy',
      ...report,
      passed,
      scripts,
      configHashes: config.hashes,
      evidenceDirectory: metadata.directory,
      registryMetadataDownloads: metadata.evidence.length,
    }) + '\n',
  );
  if (!passed) process.exitCode = 1;
}

try {
  await main();
} catch (error) {
  process.stderr.write(
    JSON.stringify({
      error: error instanceof Error ? error.message : 'Dependency verification failed',
    }) + '\n',
  );
  process.exitCode = 1;
}
