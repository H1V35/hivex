import { createHash } from 'node:crypto';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';

// The owned loopback fixture uses a default-trusted name to prove the explicit empty list wins.
export const canaryName = 'esbuild';
const hash = (bytes: Uint8Array) => `sha512-${createHash('sha512').update(bytes).digest('base64')}`;

async function artifact(root: string, version: string) {
  const manifest = { name: canaryName, version, scripts: { postinstall: 'bun marker.ts' } };
  const path = join(root, `${version}.tgz`);
  await Bun.Archive.write(
    path,
    {
      'package/package.json': JSON.stringify(manifest),
      'package/content.txt': 'original',
      'package/marker.ts': 'await Bun.write("executed.txt", "executed");\n',
    },
    { compress: 'gzip' },
  );
  const bytes = await readFile(path);
  return { manifest, bytes, integrity: hash(bytes) };
}

export async function withInstallCanary(
  run: (fixture: Awaited<ReturnType<typeof prepare>>) => Promise<void>,
) {
  const root = await mkdtemp(join(tmpdir(), 'hivex-bun-canary-'));
  try {
    const fixture = await prepare(root);
    try {
      await run(fixture);
    } finally {
      await fixture.server.stop(true);
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

async function prepare(root: string) {
  const artifacts = new Map([
    ['1.0.0', await artifact(root, '1.0.0')],
    ['1.1.0', await artifact(root, '1.1.0')],
  ]);
  const server = Bun.serve({
    hostname: '127.0.0.1',
    port: 0,
    fetch(request) {
      const url = new URL(request.url);
      if (url.pathname === `/${canaryName}`)
        return Response.json({
          name: canaryName,
          'dist-tags': { latest: '1.1.0' },
          time: {
            '1.0.0': new Date(Date.now() - 14 * 86400000).toISOString(),
            '1.1.0': new Date().toISOString(),
          },
          versions: Object.fromEntries(
            [...artifacts].map(([version, value]) => [
              version,
              {
                ...value.manifest,
                dist: { integrity: value.integrity, tarball: `${url.origin}/${version}.tgz` },
              },
            ]),
          ),
        });
      const value = artifacts.get(url.pathname.slice(1).replace(/\.tgz$/, ''));
      if (value)
        return new Response(value.bytes, {
          headers: { 'content-type': 'application/octet-stream' },
        });
      return new Response('Unknown owned fixture', { status: 404 });
    },
  });
  try {
    await writeFile(
      join(root, 'bunfig.toml'),
      `[install]\nregistry="http://127.0.0.1:${server.port}/"\nlinker="isolated"\nminimumReleaseAge=604800\n`,
    );
    await writeProject({ root, trusted: [] });
  } catch (error) {
    await server.stop(true);
    throw error;
  }
  return { root, server, packageRoot: join(root, 'node_modules', canaryName) };
}

export async function writeProject(options: { root: string; trusted: string[] }) {
  await writeFile(
    join(options.root, 'package.json'),
    JSON.stringify({
      name: 'owned-canary-project',
      private: true,
      trustedDependencies: options.trusted,
      dependencies: { [canaryName]: '*' },
    }),
  );
}

export async function install(options: { root: string; fresh: boolean; flags?: string[] }) {
  const cache = options.fresh
    ? await mkdtemp(join(options.root, 'cache-'))
    : join(options.root, 'warm-cache');
  const child = Bun.spawn(
    [
      process.execPath,
      'install',
      ...(options.flags ?? []),
      `--config=${join(options.root, 'bunfig.toml')}`,
    ],
    {
      cwd: options.root,
      timeout: 30000,
      killSignal: 'SIGKILL',
      stdout: 'pipe',
      stderr: 'pipe',
      env: {
        ...process.env,
        PATH: `${dirname(process.execPath)}:${process.env.PATH ?? ''}`,
        BUN_INSTALL_CACHE_DIR: cache,
      },
    },
  );
  const [code, stdout, stderr] = await Promise.all([
    child.exited,
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
  ]);
  return { code, stdout, stderr };
}

export const verifiedFlags = ['--frozen-lockfile', '--force', '--no-cache', '--ignore-scripts'];
