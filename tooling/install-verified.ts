import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { lstat, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const root = process.cwd();
const checker = join(import.meta.dirname, 'generated', 'check-dependencies.mjs');
const inputs = ['bun.lock', 'package.json', 'bunfig.toml', 'dependency-policy.json'];
const hash = (value: Uint8Array) => createHash('sha256').update(value).digest('hex');

async function snapshot() {
  const result: Record<string, string> = {};
  for (const name of inputs) {
    const path = join(root, name);
    const info = await lstat(path);
    if (!info.isFile() || info.size > 16 * 1024 * 1024)
      throw new Error(`Expected a regular bounded installation input: ${name}`);
    result[name] = hash(await readFile(path));
  }
  return result;
}

function isPassed(value: unknown, scope: string) {
  return (
    typeof value === 'object' &&
    value !== null &&
    'passed' in value &&
    value.passed === true &&
    'scope' in value &&
    value.scope === scope
  );
}

async function run(args: string[], evidence: { directory: string; phase: string; cache?: string }) {
  const result = spawnSync(process.execPath, args, {
    cwd: root,
    encoding: 'utf8',
    timeout: 1_200_000,
    maxBuffer: 16 * 1024 * 1024,
    env: {
      ...process.env,
      ...(evidence.cache ? { BUN_INSTALL_CACHE_DIR: evidence.cache } : {}),
    },
  });
  await writeFile(join(evidence.directory, `${evidence.phase}.stdout`), result.stdout ?? '', {
    mode: 0o600,
  });
  await writeFile(join(evidence.directory, `${evidence.phase}.stderr`), result.stderr ?? '', {
    mode: 0o600,
  });
  if (result.status !== 0 || result.error)
    throw new Error(`${evidence.phase} failed; evidence: ${evidence.directory}`, {
      cause: result.error,
    });
  return result.stdout;
}

async function verifyBootstrap() {
  const build: unknown = JSON.parse(
    await readFile(join(import.meta.dirname, 'generated', 'checker-build.json'), 'utf8'),
  );
  if (typeof build !== 'object' || build === null) throw new Error('Missing checker build record');
  const files = {
    bundleSha256: 'check-dependencies.mjs',
    metafileSha256: 'checker-metafile.json',
    licenseInventorySha256: 'checker-licenses.json',
    licenseNoticesSha256: 'checker-notices.txt',
  };
  for (const [field, file] of Object.entries(files)) {
    const expected: unknown = Reflect.get(build, field);
    const actual = hash(await readFile(join(import.meta.dirname, 'generated', file)));
    if (expected !== actual)
      throw new Error(`The trusted checker artifact differs from its build record: ${file}`);
  }
}

async function main() {
  if (Bun.version !== '1.4.2') throw new Error('Verified installation requires Bun 1.4.2');
  await verifyBootstrap();
  const before = await snapshot();
  const directory = await mkdtemp(join(tmpdir(), 'hivex-install-'));
  const preflight: unknown = JSON.parse(await run([checker], { directory, phase: 'preflight' }));
  if (!isPassed(preflight, 'preinstall-registry-and-script-policy'))
    throw new Error(`Preflight did not pass: ${directory}`);
  if (JSON.stringify(before) !== JSON.stringify(await snapshot()))
    throw new Error('Installation inputs changed during preflight; installation refused');
  const cache = await mkdtemp(join(tmpdir(), 'hivex-install-cache-'));
  await run(
    [
      'install',
      '--frozen-lockfile',
      '--force',
      '--no-cache',
      '--ignore-scripts',
      `--config=${join(root, 'bunfig.toml')}`,
    ],
    { directory, phase: 'install', cache },
  );
  if (JSON.stringify(before) !== JSON.stringify(await snapshot()))
    throw new Error('Installation inputs changed; admission refused');
  const peers: unknown = JSON.parse(
    await run([checker, '--installed-only'], { directory, phase: 'peers' }),
  );
  if (!isPassed(peers, 'installed-inventory-and-peer-bindings'))
    throw new Error(`Installed dependency check did not pass: ${directory}`);
  const report = {
    passed: true,
    scope: 'scripts-disabled-fresh-install',
    bunVersion: Bun.version,
    bunRevision: Bun.revision,
    inputHashes: before,
    evidenceDirectory: directory,
    cacheDirectory: cache,
  };
  await writeFile(join(directory, 'result.json'), JSON.stringify(report, null, 2) + '\n', {
    mode: 0o600,
  });
  console.log(JSON.stringify(report));
}

try {
  await main();
} catch (error) {
  console.error(error instanceof Error ? error.message : 'Verified install failed');
  process.exitCode = 1;
}
