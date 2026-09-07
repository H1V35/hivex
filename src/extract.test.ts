import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const cli = join(import.meta.dirname, 'cli.ts');
const codex = join(import.meta.dirname, '../test/codex-server.mjs');
const text = '# Cache\n\nNever treat a cache as authority.\n';

function fixture(run: (root: string) => void) {
  const root = mkdtempSync(join(tmpdir(), 'hivex-extract-'));
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
    writeFileSync(join(root, 'policy.md'), text);
    writeFileSync(
      join(root, 'hivex.json'),
      JSON.stringify({ version: 1, collections: [{ id: 'project', include: ['policy.md'] }] }),
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
    run(root);
    expect(readFileSync(join(root, 'policy.md'), 'utf8')).toBe(text);
    expect(git(['status', '--porcelain'])).toBe('');
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

test('extracts a source-bound candidate through native Codex without accepting a graph', () => {
  fixture((root) => {
    const result = spawnSync(
      process.execPath,
      [cli, 'extract', 'policy.md', '--root', root, '--codex', codex],
      { encoding: 'utf8', timeout: 10_000 },
    );
    expect(result.stderr).toBe('');
    expect(result.status).toBe(0);
    const output: unknown = JSON.parse(result.stdout);
    expect(output).toMatchObject({
      command: 'extract',
      status: 'candidate',
      accepted: false,
      source: { id: 'policy.md', path: 'policy.md' },
      model: { name: 'gpt-5.6-luna', effort: 'max', provider: 'openai' },
      attempts: [
        {
          outcome: 'completed',
          threadId: 'thread1',
          turnId: 'turn1',
          cleanup: 'confirmed',
          usage: { inputTokens: 100, outputTokens: 50, totalTokens: 150 },
        },
      ],
      candidate: {
        claims: [
          {
            id: 'c1',
            text: 'A cache must never be treated as authority.',
            kind: 'constraint',
            conditions: [],
            exceptions: [],
            evidence: [{ quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }],
          },
        ],
        relations: [],
      },
    });
  });
});

test('rejects invented evidence while retaining usage and the failed attempt', () => {
  fixture((root) => {
    const result = spawnSync(
      process.execPath,
      [cli, 'extract', 'policy.md', '--root', root, '--codex', codex, '--attempts', '1'],
      {
        encoding: 'utf8',
        timeout: 10_000,
        env: { ...process.env, HIVEX_TEST_SCENARIO: 'invented-evidence' },
      },
    );
    expect(result.status).toBe(1);
    expect(result.stderr).toBe('');
    const output: unknown = JSON.parse(result.stdout);
    expect(output).toMatchObject({
      status: 'failed',
      accepted: false,
      candidate: null,
      attempts: [
        { outcome: 'invalid-output', code: 'INVALID_MODEL_EVIDENCE', usage: { totalTokens: 150 } },
      ],
    });
  });
});

test('interrupts a hung invocation, accounts for it and exhausts the finite retry limit', () => {
  fixture((root) => {
    const result = spawnSync(
      process.execPath,
      [
        cli,
        'extract',
        'policy.md',
        '--root',
        root,
        '--codex',
        codex,
        '--attempts',
        '2',
        '--deadline-ms',
        '100',
      ],
      {
        encoding: 'utf8',
        timeout: 10_000,
        env: { ...process.env, HIVEX_TEST_SCENARIO: 'timeout' },
      },
    );
    expect(result.status).toBe(1);
    expect(result.stderr).toBe('');
    const output = JSON.parse(result.stdout);
    expect(output).toMatchObject({ status: 'failed', accepted: false, candidate: null });
    expect(output.attempts).toHaveLength(2);
    for (const attempt of output.attempts) {
      expect(attempt).toMatchObject({
        outcome: 'timeout',
        code: 'MODEL_TIMEOUT',
        deadlineMilliseconds: 100,
        interruption: 'confirmed',
        usage: { totalTokens: 125 },
      });
    }
  });
});

test('rejects an oversized source before invoking the configured model binary', () => {
  fixture((root) => {
    const original = readFileSync(join(root, 'policy.md'), 'utf8');
    const large = '# Large\n\n' + 'A long source sentence.\n'.repeat(2000);
    writeFileSync(join(root, 'policy.md'), large);
    spawnSync('git', ['add', 'policy.md'], { cwd: root });
    spawnSync(
      'git',
      ['-c', 'user.name=Test', '-c', 'user.email=test@example.invalid', 'commit', '-qm', 'large'],
      { cwd: root },
    );
    const result = spawnSync(
      process.execPath,
      [cli, 'extract', 'policy.md', '--root', root, '--codex', '/absent/codex'],
      { encoding: 'utf8', timeout: 10_000 },
    );
    expect(result.status).toBe(1);
    expect(result.stdout).toBe('');
    expect(JSON.parse(result.stderr)).toMatchObject({
      error: { code: 'EXTRACTION_SOURCE_TOO_LARGE' },
    });
    writeFileSync(join(root, 'policy.md'), original);
    spawnSync('git', ['add', 'policy.md'], { cwd: root });
    spawnSync(
      'git',
      ['-c', 'user.name=Test', '-c', 'user.email=test@example.invalid', 'commit', '-qm', 'restore'],
      { cwd: root },
    );
  });
});

test('bounds an unconfirmed turn start and reports unknown consumption without retrying it', () => {
  fixture((root) => {
    const result = spawnSync(
      process.execPath,
      [cli, 'extract', 'policy.md', '--root', root, '--codex', codex, '--deadline-ms', '100'],
      {
        encoding: 'utf8',
        timeout: 2000,
        env: { ...process.env, HIVEX_TEST_SCENARIO: 'start-unconfirmed' },
      },
    );
    expect(result.error).toBeUndefined();
    expect(result.status).toBe(1);
    expect(result.stderr).toBe('');
    expect(JSON.parse(result.stdout)).toMatchObject({
      status: 'failed',
      attempts: [{ code: 'MODEL_START_UNCONFIRMED', usage: null, turnAccepted: 'unknown' }],
    });
  });
});

test('refuses an oversized protocol frame even if a valid completion follows', () => {
  fixture((root) => {
    const result = spawnSync(
      process.execPath,
      [cli, 'extract', 'policy.md', '--root', root, '--codex', codex, '--attempts', '1'],
      {
        encoding: 'utf8',
        timeout: 5000,
        env: { ...process.env, HIVEX_TEST_SCENARIO: 'oversized-frame' },
      },
    );
    expect(result.status).toBe(1);
    expect(result.stderr).toBe('');
    expect(JSON.parse(result.stdout)).toMatchObject({
      status: 'failed',
      accepted: false,
      candidate: null,
    });
  });
});

test('cancels an active model turn on SIGINT and preserves its failure report', () => {
  fixture((root) => {
    const result = spawnSync(
      process.execPath,
      [cli, 'extract', 'policy.md', '--root', root, '--codex', codex],
      { encoding: 'utf8', timeout: 5000, env: { ...process.env, HIVEX_TEST_SCENARIO: 'cancel' } },
    );
    expect(result.signal).toBeNull();
    expect(result.status).toBe(1);
    expect(result.stderr).toBe('');
    const output = JSON.parse(result.stdout);
    expect(output).toMatchObject({ status: 'failed', candidate: null });
    expect(output.attempts).toHaveLength(1);
    expect(output.attempts[0]).toMatchObject({
      code: 'MODEL_CANCELLED',
      interruption: 'confirmed',
      cleanup: 'confirmed',
    });
  });
});

test('cleans up an owned descendant even when the native server exits first', () => {
  fixture((root) => {
    const observation = mkdtempSync(join(tmpdir(), 'hivex-child-observation-'));
    const path = join(observation, 'pid');
    try {
      const result = spawnSync(
        process.execPath,
        [cli, 'extract', 'policy.md', '--root', root, '--codex', codex],
        {
          encoding: 'utf8',
          timeout: 8000,
          env: { ...process.env, HIVEX_TEST_SCENARIO: 'descendant', HIVEX_TEST_PID_PATH: path },
        },
      );
      expect(result.stderr).toBe('');
      expect(result.status).toBe(0);
      const pid = Number(readFileSync(path, 'utf8'));
      expect(pid).toBeGreaterThan(1);
      expect(() => process.kill(pid, 0)).toThrow();
    } finally {
      try {
        const pid = Number(readFileSync(path, 'utf8'));
        if (Number.isSafeInteger(pid) && pid > 1) process.kill(pid, 'SIGKILL');
      } catch {
        /* The tested descendant may already be gone. */
      }
      rmSync(observation, { recursive: true, force: true });
    }
  });
});

test('disables an existing MCP server locally before creating a knowledge thread', () => {
  fixture((root) => {
    const result = spawnSync(
      process.execPath,
      [cli, 'extract', 'policy.md', '--root', root, '--codex', codex],
      {
        encoding: 'utf8',
        timeout: 5000,
        env: { ...process.env, HIVEX_TEST_SCENARIO: 'configured-mcp' },
      },
    );
    expect(result.stderr).toBe('');
    expect(result.status).toBe(0);
    expect(JSON.parse(result.stdout)).toMatchObject({ status: 'candidate', accepted: false });
  });
});

test('rejects a redirected backend despite the openai logical provider name', () => {
  fixture((root) => {
    const result = spawnSync(
      process.execPath,
      [cli, 'extract', 'policy.md', '--root', root, '--codex', codex],
      {
        encoding: 'utf8',
        timeout: 5000,
        env: { ...process.env, HIVEX_TEST_SCENARIO: 'redirected-provider' },
      },
    );
    expect(result.status).toBe(1);
    expect(result.stderr).toBe('');
    expect(JSON.parse(result.stdout)).toMatchObject({
      status: 'failed',
      candidate: null,
      attempts: [{ code: 'MODEL_ADMISSION_FAILED', usage: null }],
    });
  });
});

test('retains consumption and applies the finite correction budget to invalid JSON', () => {
  fixture((root) => {
    const result = spawnSync(
      process.execPath,
      [cli, 'extract', 'policy.md', '--root', root, '--codex', codex, '--attempts', '2'],
      {
        encoding: 'utf8',
        timeout: 5000,
        env: { ...process.env, HIVEX_TEST_SCENARIO: 'invalid-json' },
      },
    );
    expect(result.stderr).toBe('');
    expect(result.status).toBe(1);
    const output = JSON.parse(result.stdout);
    expect(output.attempts).toHaveLength(2);
    expect(output.attempts[0]).toMatchObject({
      outcome: 'invalid-output',
      usage: { totalTokens: 150 },
    });
    expect(output.attempts[1]).toMatchObject({
      outcome: 'invalid-output',
      usage: { totalTokens: 150 },
    });
  });
});
