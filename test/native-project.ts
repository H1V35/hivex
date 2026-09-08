import { expect } from 'bun:test';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const server = pathToFileURL(join(import.meta.dirname, 'codex-server.mjs')).href;
export const nativeSource = '# Cache\n\nNever treat a cache as authority.\n';

export async function nativeProject(
  run: (paths: {
    root: string;
    store: string;
    binary: string;
    calls: string;
    hold: string;
    scenario: string;
    candidate: string;
    git: (args: string[]) => string;
  }) => void | Promise<void>,
  options: { source?: string } = {},
) {
  const source = options.source ?? nativeSource;
  const directory = mkdtempSync(join(tmpdir(), 'hivex-native-project-'));
  const root = join(directory, 'project');
  const store = join(directory, 'ingestion.sqlite');
  const binary = join(directory, 'codex');
  const calls = join(directory, 'calls');
  const hold = join(directory, 'hold');
  const pids = join(directory, 'pids');
  const scenario = join(directory, 'scenario');
  const candidate = join(directory, 'candidate.json');
  const entry = join(directory, 'server.ts');
  mkdirSync(root);
  const git = (args: string[]) => {
    const result = spawnSync('git', args, {
      cwd: root,
      encoding: 'utf8',
      env: { ...process.env, GIT_CONFIG_GLOBAL: '/dev/null', GIT_CONFIG_NOSYSTEM: '1' },
    });
    if (result.status !== 0) throw new Error(result.stderr);
    return result.stdout.trim();
  };
  try {
    git(['init', '-q', '--initial-branch=main']);
    writeFileSync(join(root, 'first.md'), source);
    writeFileSync(join(root, 'second.md'), source);
    writeFileSync(
      join(root, 'hivex.json'),
      JSON.stringify({
        version: 1,
        collections: [{ id: 'project', include: ['*.md'] }],
      }),
    );
    git(['add', '.']);
    git([
      '-c',
      'user.name=Test',
      '-c',
      'user.email=test@example.invalid',
      'commit',
      '-qm',
      'fixture',
    ]);
    const quote = (text: string) => `'${text.replaceAll("'", "'\\''")}'`;
    writeFileSync(binary, `#!/bin/sh\nexec ${quote(process.execPath)} ${quote(entry)} "$@"\n`, {
      mode: 0o700,
    });
    writeFileSync(
      entry,
      [
        "import { appendFileSync, existsSync, readFileSync } from 'node:fs';",
        'if (!process.argv.includes("--version")) {',
        `appendFileSync(${JSON.stringify(pids)}, String(process.pid) + '\\n');`,
        '}',
        `process.env.HIVEX_TEST_CALLS_PATH = ${JSON.stringify(calls)};`,
        `process.env.HIVEX_TEST_HOLD_PATH = ${JSON.stringify(hold)};`,
        `process.env.HIVEX_TEST_CANDIDATE_PATH = ${JSON.stringify(candidate)};`,
        `if (existsSync(${JSON.stringify(scenario)})) process.env.HIVEX_TEST_SCENARIO = readFileSync(${JSON.stringify(scenario)}, 'utf8');`,
        `await import(${JSON.stringify(server)});`,
      ].join('\n'),
    );
    writeFileSync(calls, '');
    await run({ root, store, binary, calls, hold, scenario, candidate, git });
    expect(git(['status', '--porcelain'])).toBe('');
    expect(readFileSync(join(root, 'first.md'), 'utf8')).toBe(source);
  } finally {
    cleanObservedServers(pids);
    rmSync(directory, { recursive: true, force: true });
  }
}

function cleanObservedServers(path: string) {
  let pids: string[];
  try {
    pids = readFileSync(path, 'utf8').trim().split('\n');
  } catch {
    return;
  }
  for (const value of pids) {
    const pid = Number(value);
    try {
      if (Number.isSafeInteger(pid) && pid > 1) process.kill(-pid, 'SIGKILL');
    } catch {
      /* Completed fake servers have already been reaped. */
    }
  }
}
