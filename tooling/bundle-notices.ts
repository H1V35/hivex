import { createHash } from 'node:crypto';
import { lstat, readFile, readdir, writeFile } from 'node:fs/promises';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { z } from 'zod';

type UnknownRecord = Record<string, unknown>;
const packageManifestSchema = z.looseObject({
  name: z.string().min(1),
  version: z.string().min(1),
  license: z.unknown().optional(),
  licenses: z.unknown().optional(),
});
const packageManifestCandidateSchema = z.looseObject({
  name: z.string().min(1).optional(),
  version: z.string().min(1).optional(),
  license: z.unknown().optional(),
  licenses: z.unknown().optional(),
});
const lockSchema = z.looseObject({ packages: z.record(z.string(), z.unknown()) });
const lockTupleSchema = z.tuple([z.string(), z.string(), z.unknown(), z.string()]);
type PackageManifest = z.infer<typeof packageManifestSchema>;

type MetafileInput = {
  bytes: number;
  imports?: Array<{ path: string; kind?: string; original?: string; external?: boolean }>;
  format?: string;
};

type MetafileOutput = {
  bytes: number;
  inputs: Record<string, { bytesInOutput: number }>;
  imports?: Array<{ path: string; kind?: string }>;
  exports?: string[];
  entryPoint?: string;
  cssBundle?: string;
};

type BundleMetafile = {
  inputs: Record<string, MetafileInput>;
  outputs: Record<string, MetafileOutput>;
};

type LockPackage = {
  locator: string;
  name: string;
  version: string;
  integrity: string;
};

type LicenseFile = {
  logicalPath: string;
  bytes: number;
  sha256: string;
  text: string | null;
  base64: string | null;
};

type PackageNotice = {
  name: string;
  version: string;
  locator: string | null;
  integrity: string | null;
  manifestSha256: string;
  declaration: { field: 'license' | 'licenses'; value: unknown } | null;
  disposition: 'present' | 'metadata-only' | 'missing-declaration';
  licenseFiles: LicenseFile[];
};

type PackageRoot = { path: string; manifest: PackageManifest; manifestPath: string };

export type BundleNoticeResult = {
  metafileSha256: string;
  licenseInventorySha256: string;
  licenseNoticesSha256: string;
  inputCount: number;
  packageCount: number;
  metadataOnlyPackages: string[];
  missingDeclarationPackages: string[];
};

const hash = (value: Uint8Array | string) => createHash('sha256').update(value).digest('hex');

function string(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.length === 0) throw new Error(`${label} must be a string`);
  return value;
}

function within(root: string, path: string) {
  const value = relative(root, path);
  return value === '' || (value !== '..' && !value.startsWith(`..${sep}`));
}

function stablePath(root: string, value: string): string {
  const normalized = value.replaceAll('\\', '/');
  const absolute = isAbsolute(normalized) ? normalized : resolve(root, normalized);
  if (within(root, absolute)) return relative(root, absolute).split(sep).join('/') || '.';
  throw new Error(`Bundle provenance path escapes the project: ${value}`);
}

function normalizeMetafile(root: string, source: BundleMetafile): BundleMetafile {
  const inputs = Object.fromEntries(
    Object.entries(source.inputs).map(([path, input]) => [
      stablePath(root, path),
      {
        ...input,
        imports: input.imports?.map((item) => ({ ...item, path: stablePath(root, item.path) })),
      },
    ]),
  );
  const outputs = Object.fromEntries(
    Object.entries(source.outputs).map(([path, output]) => [
      stablePath(root, path),
      {
        ...output,
        inputs: Object.fromEntries(
          Object.entries(output.inputs).map(([input, info]) => [stablePath(root, input), info]),
        ),
        imports: output.imports?.map((item) => ({ ...item, path: stablePath(root, item.path) })),
        entryPoint: output.entryPoint ? stablePath(root, output.entryPoint) : undefined,
        cssBundle: output.cssBundle ? stablePath(root, output.cssBundle) : undefined,
      },
    ]),
  );
  return { inputs, outputs };
}

function lockPackages(text: string): Map<string, LockPackage> {
  const value = lockSchema.parse(Bun.JSONC.parse(text));
  const packages = value.packages;
  const result = new Map<string, LockPackage>();
  for (const [locator, raw] of Object.entries(packages)) {
    const tuple = lockTupleSchema.parse(raw);
    const specifier = tuple[0];
    const separator = specifier.lastIndexOf('@');
    if (separator <= 0) throw new Error(`Unsupported lock package identity: ${locator}`);
    const name = specifier.slice(0, separator);
    const version = specifier.slice(separator + 1);
    result.set(`${name}@${version}`, { locator, name, version, integrity: tuple[3] });
  }
  return result;
}

function hasPackageIdentity(manifest: UnknownRecord) {
  return typeof manifest.name === 'string' && typeof manifest.version === 'string';
}

async function readPackageManifest(path: string): Promise<PackageManifest | null> {
  try {
    const info = await lstat(path);
    if (!info.isFile()) return null;
    const candidate = packageManifestCandidateSchema.parse(
      JSON.parse(await readFile(path, 'utf8')),
    );
    return hasPackageIdentity(candidate) ? packageManifestSchema.parse(candidate) : null;
  } catch (error) {
    if (typeof error === 'object' && error !== null && 'code' in error && error.code === 'ENOENT')
      return null;
    throw error;
  }
}

async function packageRoot(root: string, input: string): Promise<PackageRoot | null> {
  const absolute = isAbsolute(input) ? input : resolve(root, input);
  let cursor = dirname(absolute);
  while (within(root, cursor)) {
    const path = join(cursor, 'package.json');
    const manifest = await readPackageManifest(path);
    if (manifest) return { path: cursor, manifest, manifestPath: path };
    const parent = dirname(cursor);
    if (parent === cursor) break;
    cursor = parent;
  }
  return null;
}

function declaration(manifest: UnknownRecord): PackageNotice['declaration'] {
  if ('license' in manifest) return { field: 'license', value: manifest.license };
  if ('licenses' in manifest) return { field: 'licenses', value: manifest.licenses };
  return null;
}

async function licenseFiles(packageName: string, packagePath: string) {
  const files = (await readdir(packagePath, { withFileTypes: true }))
    .filter(
      (entry) =>
        entry.isFile() && /^(?:license|licence|copying|notice)(?:[._-].*)?$/iu.test(entry.name),
    )
    .map((entry) => entry.name)
    .sort((a, b) => a.localeCompare(b, 'en'));
  const result: LicenseFile[] = [];
  for (const name of files) {
    const path = join(packagePath, name);
    const bytes = await readFile(path);
    let text: string | null = null;
    let base64: string | null = null;
    try {
      text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    } catch {
      base64 = Buffer.from(bytes).toString('base64');
    }
    result.push({
      logicalPath: `node_modules/${packageName}/${name}`,
      bytes: bytes.byteLength,
      sha256: hash(bytes),
      text,
      base64,
    });
  }
  return result;
}

function disposition(
  files: LicenseFile[],
  declared: PackageNotice['declaration'],
): PackageNotice['disposition'] {
  if (files.length > 0) return 'present';
  if (declared) return 'metadata-only';
  return 'missing-declaration';
}

async function bundledPackages(root: string, inputs: string[]) {
  const packages = new Map<string, PackageRoot>();
  for (const input of inputs) {
    if (!input.includes('/node_modules/')) continue;
    const found = await packageRoot(root, input);
    if (!found) throw new Error(`Cannot resolve bundled package manifest for ${input}`);
    const name = string(found.manifest.name, `${input}.name`);
    const version = string(found.manifest.version, `${input}.version`);
    packages.set(`${name}@${version}`, found);
  }
  return packages;
}

async function packageNotices(
  packages: Map<string, PackageRoot>,
  lock: Map<string, LockPackage>,
): Promise<PackageNotice[]> {
  const notices: PackageNotice[] = [];
  for (const key of [...packages.keys()].sort((a, b) => a.localeCompare(b, 'en'))) {
    const found = packages.get(key);
    if (!found) throw new Error(`Missing package root for ${key}`);
    const name = string(found.manifest.name, `${key}.name`);
    const version = string(found.manifest.version, `${key}.version`);
    const lockEntry = lock.get(key);
    if (!lockEntry) throw new Error(`Bundled package is absent from bun.lock: ${key}`);
    const files = await licenseFiles(name, found.path);
    const declared = declaration(found.manifest);
    notices.push({
      name,
      version,
      locator: lockEntry?.locator ?? null,
      integrity: lockEntry?.integrity ?? null,
      manifestSha256: hash(await readFile(found.manifestPath)),
      declaration: declared,
      disposition: disposition(files, declared),
      licenseFiles: files,
    });
  }
  return notices;
}

export async function writeBundleNotices(options: {
  root: string;
  outputDirectory: string;
  metafile: BundleMetafile;
  lockText: string;
  bundleSha256: string;
}): Promise<BundleNoticeResult> {
  const normalized = normalizeMetafile(options.root, options.metafile);
  const metafileText = `${JSON.stringify(normalized, null, 2)}\n`;
  const lock = lockPackages(options.lockText);
  const packages = await bundledPackages(options.root, Object.keys(normalized.inputs));
  const notices = await packageNotices(packages, lock);

  const metadataOnlyPackages = notices
    .filter((entry) => entry.disposition === 'metadata-only')
    .map((entry) => `${entry.name}@${entry.version}`);
  const missingDeclarationPackages = notices
    .filter((entry) => entry.disposition === 'missing-declaration')
    .map((entry) => `${entry.name}@${entry.version}`);
  const inventory = {
    version: 1,
    bundleSha256: options.bundleSha256,
    inputCount: Object.keys(normalized.inputs).length,
    packageCount: notices.length,
    metadataOnlyPackages,
    missingDeclarationPackages,
    packages: notices,
  };
  const inventoryText = `${JSON.stringify(inventory, null, 2)}\n`;
  const noticeText = notices
    .flatMap((entry) =>
      entry.licenseFiles.map((file) => {
        const content = file.text ?? `[binary license; base64=${file.base64}]`;
        return `===== ${entry.name}@${entry.version} :: ${file.logicalPath} =====\n${content}${content.endsWith('\n') ? '' : '\n'}`;
      }),
    )
    .join('\n');
  const noticesText = noticeText.length > 0 ? `${noticeText}\n` : '';
  await writeFile(join(options.outputDirectory, 'checker-metafile.json'), metafileText, {
    mode: 0o644,
  });
  await writeFile(join(options.outputDirectory, 'checker-licenses.json'), inventoryText, {
    mode: 0o644,
  });
  await writeFile(join(options.outputDirectory, 'checker-notices.txt'), noticesText, {
    mode: 0o644,
  });
  return {
    metafileSha256: hash(metafileText),
    licenseInventorySha256: hash(inventoryText),
    licenseNoticesSha256: hash(noticesText),
    inputCount: Object.keys(normalized.inputs).length,
    packageCount: notices.length,
    metadataOnlyPackages,
    missingDeclarationPackages,
  };
}
