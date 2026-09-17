import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  cpSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import manifest from '../package.json' with { type: 'json' };

if (process.platform !== 'darwin' || process.arch !== 'arm64') {
  throw new Error('The current package is validated only on macOS ARM64.');
}
if (manifest.os?.join(',') !== 'darwin' || manifest.cpu?.join(',') !== 'arm64') {
  throw new Error('Package platform metadata must match the validated native target.');
}
const root = path.resolve(fileURLToPath(new URL('../', import.meta.url)));
const target = 'aarch64-apple-darwin';
const distribution = path.join(root, 'dist');
const maximumOutputBytes = 16_777_216;
const successfulExitCode = 0;

const execute = function execute(command, argumentsList, cwd = root, environment = process.env) {
  const result = spawnSync(command, argumentsList, {
    cwd,
    encoding: 'utf-8',
    env: environment,
    maxBuffer: maximumOutputBytes,
  });
  if (result.status !== successfulExitCode) {
    throw result.error ?? new Error(result.stderr || result.stdout || `${command} failed`);
  }
  return result.stdout;
};

const digest = (file) => createHash('sha256').update(readFileSync(file)).digest('hex');

const packageNotice = function packageNotice(dependency) {
  const directory = path.dirname(dependency.manifest_path);
  const files = readdirSync(directory, { withFileTypes: true }).filter(
    (entry) => entry.isFile() && /^(?:LICENSE|COPYING|NOTICE)/iu.test(entry.name)
  );
  if (!files.length) {
    throw new Error(`No license text is available for ${dependency.name}.`);
  }
  const texts = files.map((entry) => readFileSync(path.join(directory, entry.name), 'utf-8'));
  return `## ${dependency.name} ${dependency.version} (${dependency.license})\n\n${texts.join('\n\n')}`;
};

const licenseNotices = function licenseNotices(metadata, packageId) {
  const nodes = new Map(metadata.resolve.nodes.map((node) => [node.id, node]));
  const reachable = new Set();
  const remaining = [packageId];
  while (remaining.length) {
    const id = remaining.pop();
    if (reachable.has(id)) {
      continue;
    }
    reachable.add(id);
    remaining.push(...nodes.get(id).deps.map((dependency) => dependency.pkg));
  }
  const dependencies = metadata.packages.filter(
    (dependency) => reachable.has(dependency.id) && dependency.source !== null
  );
  return `# Third-party licenses\n\n${dependencies.map(packageNotice).join('\n\n')}\n`;
};

const metadata = JSON.parse(
  execute('cargo', ['metadata', '--locked', '--format-version', '1', '--filter-platform', target])
);
const rustPackage = metadata.packages.find((entry) => entry.name === 'hivex');
if (rustPackage?.version !== manifest.version) {
  throw new Error('Cargo and npm package versions must match.');
}
const indentation = 2;
const separator = '\u{1F}';
const existingFlags =
  process.env.CARGO_ENCODED_RUSTFLAGS?.split(separator) ??
  (process.env.RUSTFLAGS ?? '').split(/\s+/u).filter(Boolean);
const registryRoots = new Set(
  metadata.packages
    .filter((entry) => entry.source !== null)
    .map((entry) => path.dirname(path.dirname(entry.manifest_path)))
);
const remappedFlags = [
  ...existingFlags,
  `--remap-path-prefix=${root}=hivex`,
  ...[...registryRoots].map((directory) => `--remap-path-prefix=${directory}=crates`),
];
const flagsKey = 'CARGO_ENCODED_RUSTFLAGS';
execute(
  'cargo',
  ['build', '--release', '--locked', '--target', target, '--target-dir', path.join(root, 'target')],
  root,
  { ...process.env, [flagsKey]: remappedFlags.join(separator) }
);
const binary = path.join(root, 'target', target, 'release', 'hivex');
const staging = mkdtempSync(path.join(tmpdir(), 'hivex-package-'));
using cleanup = new DisposableStack();
cleanup.defer(() => rmSync(staging, { force: true, recursive: true }));
const packageRoot = path.join(staging, 'package');
mkdirSync(path.join(packageRoot, 'bin'), { recursive: true });
cpSync(binary, path.join(packageRoot, 'bin', 'hivex'));
writeFileSync(
  path.join(packageRoot, 'THIRD-PARTY-NOTICES.txt'),
  licenseNotices(metadata, rustPackage.id)
);
for (const file of manifest.files) {
  if (file === 'bin/hivex' || file === 'THIRD-PARTY-NOTICES.txt') {
    continue;
  }
  mkdirSync(path.dirname(path.join(packageRoot, file)), { recursive: true });
  cpSync(path.join(root, file), path.join(packageRoot, file), { recursive: true });
}
writeFileSync(
  path.join(packageRoot, 'package.json'),
  `${JSON.stringify(manifest, null, indentation)}\n`
);
mkdirSync(distribution, { recursive: true });
const packed = JSON.parse(
  execute(
    'npm',
    [
      'pack',
      '--offline',
      '--ignore-scripts',
      '--json',
      '--cache',
      path.join(staging, 'npm-cache'),
      '--pack-destination',
      distribution,
    ],
    packageRoot
  )
);
const [result] = packed;
const archive = path.join(distribution, result.filename);
const report = {
  archive,
  files: result.files.map((file) => file.path),
  nativeSha256: digest(binary),
  sha256: digest(archive),
  target,
  version: manifest.version,
};
writeFileSync(`${archive}.json`, `${JSON.stringify(report, null, indentation)}\n`);
process.stdout.write(`${JSON.stringify(report, null, indentation)}\n`);
