import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { writeBundleNotices } from './bundle-notices.ts';

const hash = (value: Uint8Array) => createHash('sha256').update(value).digest('hex');
const entrypoint = 'tooling/check-dependencies.ts';
const result = await Bun.build({
  entrypoints: [entrypoint],
  target: 'bun',
  minify: true,
  // Use msgpackr's supported JavaScript path; a bootstrap must not load a host addon.
  define: { 'process.env.MSGPACKR_NATIVE_ACCELERATION_DISABLED': '"true"' },
  external: ['msgpackr-extract'],
  outdir: 'tooling/generated',
  naming: 'check-dependencies.mjs',
  metafile: true,
});
if (!result.success) throw new AggregateError(result.logs, 'Dependency checker bundle failed');
if (!result.metafile) throw new Error('Dependency checker bundle did not produce a metafile');
const bundlePath = 'tooling/generated/check-dependencies.mjs';
const bundleBytes = await readFile(bundlePath);
if (bundleBytes.includes(Buffer.from(process.cwd())))
  throw new Error('The bootstrap bundle contains its construction directory');
if (Object.keys(result.metafile.inputs).some((file) => file.includes('/msgpackr-extract/')))
  throw new Error('The bootstrap must use the supported JavaScript serialization path');
const provenance = await writeBundleNotices({
  root: process.cwd(),
  outputDirectory: 'tooling/generated',
  metafile: result.metafile,
  lockText: await readFile('bun.lock', 'utf8'),
  bundleSha256: hash(bundleBytes),
});
const sources: Record<string, string> = {};
const sourceFiles = Object.keys(result.metafile.inputs)
  .filter((file) => file.startsWith('tooling/') && file.endsWith('.ts'))
  .sort();
for (const file of sourceFiles) sources[file] = hash(await readFile(file));
const builderSources: Record<string, string> = {};
for (const file of ['tooling/build-checker.ts', 'tooling/bundle-notices.ts'])
  builderSources[file] = hash(await readFile(file));
const manifest = {
  version: 1,
  bunVersion: Bun.version,
  bunRevision: Bun.revision,
  bundleSha256: hash(bundleBytes),
  buildLockSha256: hash(await readFile('bun.lock')),
  metafileSha256: provenance.metafileSha256,
  licenseInventorySha256: provenance.licenseInventorySha256,
  licenseNoticesSha256: provenance.licenseNoticesSha256,
  bundleInputCount: provenance.inputCount,
  bundlePackageCount: provenance.packageCount,
  metadataOnlyPackages: provenance.metadataOnlyPackages,
  missingDeclarationPackages: provenance.missingDeclarationPackages,
  sources,
  builderSources,
};
await writeFile('tooling/generated/checker-build.json', JSON.stringify(manifest, null, 2) + '\n');
console.log(JSON.stringify(manifest));
