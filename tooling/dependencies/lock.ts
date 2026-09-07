import { createHash } from 'node:crypto';
import { valid } from 'semver';
import { z } from 'zod';

const dependencies = z.record(z.string(), z.string());
const workspace = z.looseObject({ name: z.string().min(1) });
const metadata = z.strictObject({
  dependencies: dependencies.optional(),
  optionalDependencies: dependencies.optional(),
  peerDependencies: dependencies.optional(),
  optionalPeers: z.array(z.string()).optional(),
  os: z.union([z.string(), z.array(z.string())]).optional(),
  cpu: z.union([z.string(), z.array(z.string())]).optional(),
  bin: z.union([z.string(), dependencies]).optional(),
});
const schema = z.strictObject({
  lockfileVersion: z.literal(2),
  configVersion: z.literal(1),
  workspaces: z.record(z.string(), workspace),
  packages: z.record(z.string(), z.unknown()),
  trustedDependencies: z.array(z.string()).optional(),
});

export const digest = (value: string) => createHash('sha256').update(value).digest('hex');
export const isRegistryName = (name: string) =>
  /^(?:@[a-z0-9][a-z0-9._-]*\/)?[a-z0-9][a-z0-9._-]*$/i.test(name);

function registryIdentity(locator: string, value: unknown) {
  const tuple = z.tuple([z.string(), z.string(), metadata, z.string()]).parse(value);
  const [specifier, registry, details, integrity] = tuple;
  const separator = specifier.lastIndexOf('@');
  const name = specifier.slice(0, separator);
  const version = specifier.slice(separator + 1);
  if (!isRegistryName(name) || valid(version) !== version)
    throw new Error(`Unsupported registry identity: ${locator}`);
  if (registry !== '' && registry !== 'https://registry.npmjs.org/')
    throw new Error(`Unadmitted registry: ${locator}`);
  const match = /^sha512-([A-Za-z0-9+/]+={0,2})$/.exec(integrity);
  const bytes = match ? Buffer.from(match[1] ?? '', 'base64') : null;
  if (!bytes || bytes.length !== 64 || `sha512-${bytes.toString('base64')}` !== integrity)
    throw new Error(`A canonical SHA-512 integrity is required: ${locator}`);
  return {
    locator,
    name,
    version,
    integrity,
    registry: 'https://registry.npmjs.org/',
    metadata: details,
  };
}

export function inspectLock(text: string) {
  const lock = schema.parse(Bun.JSONC.parse(text));
  return {
    lockHash: digest(text),
    registry: Object.entries(lock.packages).map(([locator, entry]) =>
      registryIdentity(locator, entry),
    ),
    workspaces: Object.entries(lock.workspaces).map(([path, entry]) => ({
      path,
      name: entry.name,
    })),
    trustedDependencies: lock.trustedDependencies ?? [],
  };
}
