import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { mkdir, mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

type Scenario = {
  preflight?: 'fail' | 'mutate-input';
  installed?: 'fail';
  actualChecker?: boolean;
};
const generated = join(import.meta.dirname, 'generated');

async function checker(root: string, scenario: Scenario) {
  const destination = join(root, 'tooling/generated');
  await mkdir(destination, { recursive: true });
  if (scenario.actualChecker) {
    for (const file of await readdir(generated))
      await writeFile(join(destination, file), await readFile(join(generated, file)));
    return;
  }
  const body = `
    const scenario = ${JSON.stringify(scenario)};
    const installed = process.argv.includes('--installed-only');
    await Bun.write('phases', installed ? 'installed' : 'preflight');
    if (!installed && scenario.preflight === 'mutate-input')
      await Bun.write('package.json', JSON.stringify({ name: 'changed-during-preflight' }));
    console.log(JSON.stringify({
      passed: installed ? scenario.installed !== 'fail' : scenario.preflight !== 'fail',
      scope: installed ? 'installed-inventory-and-peer-bindings' : 'preinstall-registry-and-script-policy'
    }));
  `;
  const files = {
    bundleSha256: 'check-dependencies.mjs',
    metafileSha256: 'checker-metafile.json',
    licenseInventorySha256: 'checker-licenses.json',
    licenseNoticesSha256: 'checker-notices.txt',
  };
  const record: Record<string, string> = {};
  for (const [key, file] of Object.entries(files)) {
    const content = key === 'bundleSha256' ? body : '{}';
    await writeFile(join(destination, file), content);
    record[key] = createHash('sha256').update(content).digest('hex');
  }
  await writeFile(join(destination, 'checker-build.json'), JSON.stringify(record));
}

async function prepareOwnedDependency(root: string) {
  const directory = join(root, 'fixture-package');
  await mkdir(directory);
  await writeFile(join(directory, 'package.json'), '{"name":"owned-fixture","version":"1.0.0"}');
  await writeFile(
    join(root, 'package.json'),
    JSON.stringify({
      name: 'owned-bootstrap-fixture',
      private: true,
      trustedDependencies: [],
      dependencies: { 'owned-fixture': 'file:./fixture-package' },
    }),
  );
  const result = spawnSync(
    process.execPath,
    ['install', '--lockfile-only', '--ignore-scripts', `--config=${join(root, 'bunfig.toml')}`],
    {
      cwd: root,
      encoding: 'utf8',
      timeout: 10000,
      env: { ...process.env, BUN_INSTALL_CACHE_DIR: join(root, 'temp/prepare-cache') },
    },
  );
  if (result.status !== 0)
    throw new Error(`Owned fixture lock generation failed: ${result.stderr}`);
}

export async function bootstrapFixture(scenario: Scenario, run: (root: string) => Promise<void>) {
  const root = await mkdtemp(join(tmpdir(), 'hivex-bootstrap-test-'));
  try {
    await mkdir(join(root, 'temp'));
    await checker(root, scenario);
    await writeFile(
      join(root, 'tooling/install-verified.ts'),
      await readFile(join(import.meta.dirname, 'install-verified.ts')),
    );
    await writeFile(
      join(root, 'package.json'),
      JSON.stringify({
        name: 'owned-bootstrap-fixture',
        private: true,
        dependencies: {},
        trustedDependencies: [],
      }),
    );
    await writeFile(
      join(root, 'bun.lock'),
      JSON.stringify({
        lockfileVersion: 2,
        configVersion: 1,
        workspaces: { '': { name: 'owned-bootstrap-fixture', dependencies: {} } },
        packages: {},
      }),
    );
    await writeFile(
      join(root, 'bunfig.toml'),
      '[install]\nlinker="isolated"\nminimumReleaseAge=604800\n',
    );
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
    if (!scenario.actualChecker) await prepareOwnedDependency(root);
    await run(root);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

export function executeInstaller(root: string, options: { checkerOnly?: boolean } = {}) {
  const args = [join(root, 'tooling/install-verified.ts')];
  if (options.checkerOnly)
    args.splice(0, 1, join(root, 'tooling/generated/check-dependencies.mjs'), '--installed-only');
  return spawnSync(process.execPath, args, {
    cwd: root,
    encoding: 'utf8',
    timeout: 20000,
    maxBuffer: 1024 * 1024,
    env: {
      ...process.env,
      TMPDIR: join(root, 'temp'),
      MSGPACKR_NATIVE_ACCELERATION_DISABLED: 'false',
    },
  });
}
