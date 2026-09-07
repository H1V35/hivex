import type { inspectLock } from './lock.ts';

function supports(value: string, constraints: string | string[] | undefined) {
  const entries = typeof constraints === 'string' ? [constraints] : (constraints ?? []);
  if (entries.includes(`!${value}`)) return false;
  const positive = entries.filter((entry) => !entry.startsWith('!'));
  return positive.length === 0 || positive.includes(value) || positive.includes('any');
}

export function verifyInventory(
  lock: ReturnType<typeof inspectLock>,
  installed: ReadonlyArray<{ name: string; version: string }>,
  platform: { os: string; cpu: string } = {
    os: process.platform,
    cpu: process.arch,
  },
) {
  const expected = new Set(
    lock.registry
      .filter(
        (entry) =>
          supports(platform.os, entry.metadata.os) && supports(platform.cpu, entry.metadata.cpu),
      )
      .map((entry) => `${entry.name}@${entry.version}`),
  );
  const actual = new Set(installed.map((entry) => `${entry.name}@${entry.version}`));
  const missing = [...expected].filter((identity) => !actual.has(identity)).sort();
  const unexpected = [...actual].filter((identity) => !expected.has(identity)).sort();
  return {
    passed: missing.length === 0 && unexpected.length === 0,
    platform,
    expected: expected.size,
    installed: actual.size,
    missing,
    unexpected,
  };
}
