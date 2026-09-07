import { createRequire } from 'node:module';
import { readFile, readdir, realpath, stat } from 'node:fs/promises';
import { join, relative } from 'node:path';
import { satisfies, valid } from 'semver';
import { z } from 'zod';

const manifest = z.looseObject({
  name: z.string(),
  version: z.string(),
  peerDependencies: z.record(z.string(), z.string()).optional(),
  peerDependenciesMeta: z
    .record(z.string(), z.looseObject({ optional: z.boolean().optional() }))
    .optional(),
});
type Manifest = z.infer<typeof manifest>;
type Context = {
  root: string;
  file: string;
  parent: Manifest;
  overrides: Readonly<Record<string, string>>;
};

function within(root: string, path: string) {
  const value = relative(root, path);
  return value !== '..' && !value.startsWith('../');
}

async function readManifest(path: string) {
  if ((await stat(path)).size > 1024 * 1024) throw new Error('Package manifest exceeds 1 MiB');
  return manifest.parse(JSON.parse(await readFile(path, 'utf8')));
}

async function packageManifests(directory: string): Promise<string[]> {
  const files: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (!entry.isDirectory() || entry.name.startsWith('.')) continue;
    const child = join(directory, entry.name);
    if (entry.name.startsWith('@')) files.push(...(await packageManifests(child)));
    else files.push(join(child, 'package.json'));
  }
  return files;
}

async function installedManifests(root: string) {
  const store = join(root, 'node_modules', '.bun');
  const files: string[] = [];
  for (const entry of await readdir(store, { withFileTypes: true })) {
    if (entry.isDirectory() && entry.name !== 'node_modules')
      files.push(...(await packageManifests(join(store, entry.name, 'node_modules'))));
  }
  return files;
}

async function resolvedPeer(context: Context, peer: string) {
  const paths = createRequire(context.file).resolve.paths(peer) ?? [];
  for (const directory of paths) {
    if (!within(context.root, directory)) continue;
    const candidate = join(directory, peer, 'package.json');
    if (!(await Bun.file(candidate).exists())) continue;
    const path = await realpath(candidate);
    if (!within(context.root, path)) throw new Error('Peer manifest resolves outside the project');
    return {
      path: relative(context.root, path),
      ...(await readManifest(path)),
    };
  }
  return null;
}

async function verifyPeer(context: Context, peer: string, range: string) {
  const { parent, file, root } = context;
  const optional = parent.peerDependenciesMeta?.[peer]?.optional === true;
  const base = {
    consumer: parent.name,
    consumerVersion: parent.version,
    consumerPath: relative(root, file),
    peer,
    range,
    optional,
  };
  const target = await resolvedPeer(context, peer);
  if (!target) return { ...base, passed: optional, resolution: 'absent', exception: null };
  const allowed = context.overrides[`${parent.name}>${peer}`];
  const ordinary = valid(target.version) !== null && satisfies(target.version, range);
  const exception = !ordinary && allowed !== undefined && satisfies(target.version, allowed);
  return {
    ...base,
    passed: ordinary || exception,
    resolution: 'installed',
    targetName: target.name,
    targetVersion: target.version,
    targetPath: target.path,
    exception: exception ? allowed : null,
  };
}

export async function verifyInstalledPeers(
  root: string,
  overrides: Readonly<Record<string, string>>,
) {
  root = await realpath(root);
  const files = await installedManifests(root);
  const bindings = [];
  const packages = [];
  for (const file of files) {
    const parent = await readManifest(file);
    packages.push({
      name: parent.name,
      version: parent.version,
      path: relative(root, file),
    });
    for (const [peer, range] of Object.entries(parent.peerDependencies ?? {}))
      bindings.push(await verifyPeer({ root, file, parent, overrides }, peer, range));
  }
  const issues = bindings.filter((binding) => !binding.passed);
  return {
    passed: issues.length === 0,
    packagesInspected: files.length,
    packages,
    bindings,
    issues,
  };
}
