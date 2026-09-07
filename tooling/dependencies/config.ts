import { lstat, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { z } from 'zod';
import { valid } from 'semver';
import { digest, isRegistryName } from './lock.ts';
import type { verifyRegistry } from './verify.ts';

const exactExclusion = z.string().refine((value) => {
  const separator = value.lastIndexOf('@');
  const name = value.slice(0, separator);
  const version = value.slice(separator + 1);
  return isRegistryName(name) && valid(version) === version;
}, 'Trust exclusions must name one exact registry package version');

const policySchema = z.strictObject({
  version: z.literal(1),
  minimumReleaseAgeSeconds: z.literal(604800),
  trustPolicy: z.literal('no-downgrade'),
  trustExclusions: z.array(exactExclusion),
  allowedInstallScripts: z.record(z.string(), z.string()),
  deniedInstallScripts: z.record(z.string(), z.string()),
  peerVersionAllowances: z.record(z.string(), z.string()),
});
const packageSchema = z.looseObject({
  name: z.string(),
  trustedDependencies: z.array(z.string()),
});
const configSchema = z.looseObject({
  install: z.looseObject({
    linker: z.literal('isolated'),
    minimumReleaseAge: z.literal(604800),
  }),
});

async function source(root: string, name: string) {
  const path = join(root, name);
  const info = await lstat(path);
  if (!info.isFile() || info.size > 1024 * 1024)
    throw new Error(`Expected a regular bounded config file: ${name}`);
  return readFile(path, 'utf8');
}

export async function loadPolicy(root: string) {
  const [policyText, packageText, configText] = await Promise.all([
    source(root, 'dependency-policy.json'),
    source(root, 'package.json'),
    source(root, 'bunfig.toml'),
  ]);
  const policy = policySchema.parse(JSON.parse(policyText));
  const project = packageSchema.parse(JSON.parse(packageText));
  configSchema.parse(Bun.TOML.parse(configText));
  const trusted = [...new Set(project.trustedDependencies)].sort();
  const allowed = Object.keys(policy.allowedInstallScripts).sort();
  if (JSON.stringify(trusted) !== JSON.stringify(allowed))
    throw new Error('trustedDependencies must explicitly match the approved script names');
  if (allowed.some((name) => name in policy.deniedInstallScripts))
    throw new Error('An install script cannot be both allowed and denied');
  return {
    policy,
    hashes: {
      policy: digest(policyText),
      package: digest(packageText),
      bunfig: digest(configText),
    },
  };
}

export function classifyScripts(
  config: Awaited<ReturnType<typeof loadPolicy>>,
  report: Awaited<ReturnType<typeof verifyRegistry>>,
) {
  const scripts = report.entries
    .filter((entry) => Object.keys(entry.installScripts).length > 0)
    .map((entry) => {
      let disposition = 'unclassified';
      if (config.policy.allowedInstallScripts[entry.name] === entry.version)
        disposition = 'allowed';
      if (config.policy.deniedInstallScripts[entry.name] === entry.version) disposition = 'denied';
      return {
        name: entry.name,
        version: entry.version,
        scripts: entry.installScripts,
        disposition,
      };
    });
  return {
    passed: scripts.every((entry) => entry.disposition !== 'unclassified'),
    scripts,
  };
}
