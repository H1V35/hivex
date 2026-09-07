import { createNpmResolutionVerifier } from '@pnpm/resolving.npm-resolver';
import { z } from 'zod';
import { digest, type inspectLock } from './lock.ts';

type Lock = ReturnType<typeof inspectLock>;
type Entry = Lock['registry'][number];
type Issue = { locator: string; code: string; message: string };
const registry = 'https://registry.npmjs.org/';
const minimumAge = 604_800_000;
const packumentSchema = z.looseObject({
  name: z.string(),
  versions: z.record(z.string(), z.unknown()),
  time: z.record(z.string(), z.string()).optional(),
});
const manifestSchema = z.looseObject({
  name: z.string(),
  version: z.string(),
  dist: z.looseObject({
    integrity: z.string().optional(),
    tarball: z.string(),
  }),
  scripts: z.record(z.string(), z.string()).optional(),
});

function inspectMetadata(entry: Entry, raw: unknown, now: number) {
  const issues: Issue[] = [];
  const fail = (code: string, message: string) =>
    issues.push({ locator: entry.locator, code, message });
  const parsed = packumentSchema.safeParse(raw);
  if (!parsed.success) {
    fail('INVALID_METADATA', 'A complete package metadata object is required');
    return { issues, artifact: null };
  }
  const packument = parsed.data;
  const selected = manifestSchema.safeParse(packument.versions[entry.version]);
  if (!selected.success) {
    fail('MISSING_VERSION', 'The locked version needs a registry manifest');
    return { issues, artifact: null };
  }
  const manifest = selected.data;
  if (
    packument.name !== entry.name ||
    manifest.name !== entry.name ||
    manifest.version !== entry.version
  )
    fail('IDENTITY_MISMATCH', 'Registry name and version must match the canonical locked identity');
  if (manifest.dist.integrity !== entry.integrity)
    fail('SRI_MISMATCH', 'Locked SHA-512 integrity must equal the registry version integrity');
  const publishedAt = packument.time?.[entry.version];
  const timestamp = publishedAt ? Date.parse(publishedAt) : NaN;
  if (!Number.isFinite(timestamp))
    fail('MISSING_PUBLICATION_TIME', 'A valid per-version publication timestamp is required');
  else if (now - timestamp < minimumAge)
    fail('MINIMUM_RELEASE_AGE', 'The locked version has not cleared seven days');
  if (!manifest.dist.tarball.startsWith(registry))
    fail('UNADMITTED_TARBALL', 'Only artifacts from the declared npm registry are admitted');
  return {
    issues,
    artifact: {
      manifest,
      publishedAt: publishedAt ?? null,
      metadataHash: digest(JSON.stringify(raw)),
    },
  };
}

export async function verifyRegistry(options: {
  lock: Lock;
  packuments: ReadonlyMap<string, unknown>;
  now: number;
  trustExclusions?: string[];
}) {
  if (!Number.isFinite(options.now)) throw new Error('A finite evaluation time is required');
  const fetches: string[] = [];
  const verifier = createNpmResolutionVerifier({
    registries: { default: registry },
    minimumReleaseAge: 10_080,
    trustPolicy: 'no-downgrade',
    trustPolicyExclude: options.trustExclusions ?? [],
    ignoreMissingTimeField: false,
    now: options.now,
    getAuthHeaderValueByURI: () => undefined,
    fetchOpts: {
      retry: { retries: 0 },
      timeout: 1000,
      fetchWarnTimeoutMs: 1000,
      fetch: async (input) => {
        const url = new URL(input);
        if (url.origin !== 'https://registry.npmjs.org')
          throw new Error('Unplanned metadata origin');
        const name = decodeURIComponent(url.pathname.slice(1));
        if (!options.packuments.has(name)) throw new Error('Unplanned metadata request');
        fetches.push(name);
        return new Response(JSON.stringify(options.packuments.get(name)), {
          headers: { 'content-type': 'application/json' },
        });
      },
    },
  });
  const issues: Issue[] = [];
  const entries = [];
  for (const entry of options.lock.registry) {
    const inspected = inspectMetadata(entry, options.packuments.get(entry.name), options.now);
    issues.push(...inspected.issues);
    if (!inspected.artifact) continue;
    const { manifest, publishedAt, metadataHash } = inspected.artifact;
    const trust = await verifier.verify(
      { integrity: entry.integrity, tarball: manifest.dist.tarball },
      { name: entry.name, version: entry.version },
    );
    if (!trust.ok)
      issues.push({
        locator: entry.locator,
        code: trust.code,
        message: trust.reason,
      });
    const scripts = Object.entries(manifest.scripts ?? {}).filter(([name]) =>
      ['preinstall', 'install', 'postinstall'].includes(name),
    );
    entries.push({
      locator: entry.locator,
      name: entry.name,
      version: entry.version,
      publishedAt,
      metadataHash,
      trust,
      installScripts: Object.fromEntries(scripts),
    });
  }
  return {
    passed: issues.length === 0,
    lockHash: options.lock.lockHash,
    evaluatedAt: new Date(options.now).toISOString(),
    issues,
    entries,
    verifierMetadataLookups: fetches.length,
  };
}
