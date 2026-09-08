import { expect, test } from 'bun:test';
import { Database } from 'bun:sqlite';
import { nativeProject as fixture, nativeSource as source } from '../test/native-project.ts';
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  symlinkSync,
  truncateSync,
  writeFileSync,
} from 'node:fs';
import { spawnSync } from 'node:child_process';
import { dirname, join } from 'node:path';

const cli = join(import.meta.dirname, 'cli.ts');

async function waitForCalls(path: string, count: number) {
  const deadline = performance.now() + 3000;
  while (readFileSync(path, 'utf8') !== 'called\n'.repeat(count)) {
    if (performance.now() > deadline) throw new Error('The fake model did not receive its request');
    await Bun.sleep(10);
  }
}

function start(paths: { root: string; store: string; binary: string }) {
  return Bun.spawn(
    [
      process.execPath,
      cli,
      'ingest',
      '--root',
      paths.root,
      '--store',
      paths.store,
      '--codex',
      paths.binary,
      '--max-units',
      '1',
    ],
    { stdout: 'pipe', stderr: 'pipe' },
  );
}

function ingest(paths: { root: string; store: string; binary: string }, extra: string[] = []) {
  const execution =
    extra.includes('--show') || extra.includes('--discard')
      ? []
      : ['--codex', paths.binary, '--max-units', '1'];
  return spawnSync(
    process.execPath,
    [cli, 'ingest', '--root', paths.root, '--store', paths.store, ...execution, ...extra],
    { encoding: 'utf8', timeout: 15_000 },
  );
}

test('resumes a fixed cohort without invoking the model again for completed sources', async () => {
  await fixture((paths) => {
    const first = ingest(paths);
    expect(first.stderr).toBe('');
    expect(first.status).toBe(0);
    const firstResult = JSON.parse(first.stdout);
    expect(firstResult).toMatchObject({
      command: 'ingest',
      accepted: false,
      status: 'partial',
      completed: 1,
      pending: 1,
      processed: 1,
    });
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\n');
    const second = ingest(paths);
    expect(second.status).toBe(0);
    expect(JSON.parse(second.stdout)).toMatchObject({
      planHash: firstResult.planHash,
      status: 'candidates-ready',
      accepted: false,
      completed: 2,
      pending: 0,
      processed: 1,
    });
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
    const third = ingest(paths);
    expect(third.status).toBe(0);
    expect(JSON.parse(third.stdout)).toMatchObject({
      status: 'candidates-ready',
      accepted: false,
      processed: 0,
      reused: 2,
    });
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
  });
});

test('refuses an existing unrelated database without modifying it or invoking a model', async () => {
  await fixture((paths) => {
    const database = new Database(paths.store);
    database.run('CREATE TABLE notes (text TEXT)');
    database.run("INSERT INTO notes VALUES ('Keep this database intact')");
    database.close();
    const before = readFileSync(paths.store);
    const result = ingest(paths);
    expect(result.status).toBe(1);
    expect(JSON.parse(result.stderr)).toMatchObject({ error: { code: 'INVALID_INGESTION_STORE' } });
    expect(readFileSync(paths.store)).toEqual(before);
    expect(readFileSync(paths.calls, 'utf8')).toBe('');
  });
});

test('resumes the recorded snapshot after HEAD advances, but rejects an explicit different cohort', async () => {
  await fixture((paths) => {
    const first = ingest(paths);
    expect(first.status).toBe(0);
    const started = JSON.parse(first.stdout);
    writeFileSync(join(paths.root, 'third.md'), source);
    paths.git(['add', 'third.md']);
    paths.git([
      '-c',
      'user.name=Test',
      '-c',
      'user.email=test@example.invalid',
      'commit',
      '-qm',
      'new source',
    ]);
    const changed = ingest(paths, ['--ref', 'HEAD']);
    expect(changed.status).toBe(1);
    expect(JSON.parse(changed.stderr)).toMatchObject({
      error: { code: 'INGESTION_PLAN_MISMATCH' },
    });
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\n');
    const resumed = ingest(paths);
    expect(resumed.stderr).toBe('');
    expect(resumed.status).toBe(0);
    expect(JSON.parse(resumed.stdout)).toMatchObject({
      snapshot: started.snapshot,
      planHash: started.planHash,
      status: 'candidates-ready',
      completed: 2,
    });
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
  });
});

test('two CLI processes can finish different sources without duplicating model requests', async () => {
  await fixture(async (paths) => {
    writeFileSync(paths.hold, 'hold native responses');
    const first = start(paths);
    const second = start(paths);
    try {
      await waitForCalls(paths.calls, 2);
      const pending = ingest(paths, ['--max-units', '0']);
      expect(pending.status).toBe(0);
      expect(JSON.parse(pending.stdout)).toMatchObject({ pending: 0, completed: 0, unresolved: 2 });
      rmSync(paths.hold);
      expect(await first.exited).toBe(0);
      expect(await second.exited).toBe(0);
      const complete = ingest(paths);
      expect(complete.status).toBe(0);
      expect(JSON.parse(complete.stdout)).toMatchObject({
        status: 'candidates-ready',
        completed: 2,
        unresolved: 0,
        processed: 0,
      });
      expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
    } finally {
      first.kill();
      second.kill();
      await Promise.all([first.exited, second.exited]);
    }
  });
});

test('an abrupt CLI crash leaves its invocation unresolved and never automatically repeats it', async () => {
  await fixture(async (paths) => {
    writeFileSync(paths.hold, 'hold native responses');
    const interrupted = start(paths);
    try {
      await waitForCalls(paths.calls, 1);
      interrupted.kill('SIGKILL');
      await interrupted.exited;
      rmSync(paths.hold);
      const continued = ingest(paths);
      expect(continued.status).toBe(0);
      expect(JSON.parse(continued.stdout)).toMatchObject({
        status: 'partial',
        completed: 1,
        unresolved: 1,
        pending: 0,
        processed: 1,
      });
      const repeated = ingest(paths);
      expect(repeated.status).toBe(0);
      expect(JSON.parse(repeated.stdout)).toMatchObject({
        status: 'partial',
        completed: 1,
        unresolved: 1,
        processed: 0,
      });
      expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
    } finally {
      interrupted.kill('SIGKILL');
      await interrupted.exited;
    }
  });
});

test('retains consumption from a completed attempt when the next attempt is interrupted', async () => {
  await fixture(async (paths) => {
    writeFileSync(paths.hold, 'after-first');
    writeFileSync(paths.scenario, 'invalid-json');
    const interrupted = start(paths);
    try {
      await waitForCalls(paths.calls, 2);
      interrupted.kill('SIGKILL');
      await interrupted.exited;
      const progress = ingest(paths, ['--max-units', '0']);
      expect(progress.status).toBe(0);
      expect(JSON.parse(progress.stdout)).toMatchObject({
        processed: 0,
        unresolved: 1,
        attempts: { recorded: 1, unresolved: 1, unknownUsage: 1, knownTotalTokens: 150 },
      });
      expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
    } finally {
      interrupted.kill('SIGKILL');
      await interrupted.exited;
    }
  });
});

test('opens a complete retained result through the CLI without mutating the store or spending tokens', async () => {
  await fixture((paths) => {
    const first = ingest(paths);
    expect(first.status).toBe(0);
    const snapshot = JSON.parse(first.stdout).snapshot;
    const before = readFileSync(paths.store);
    const opened = ingest(paths, ['--show', 'first.md']);
    expect(opened.status).toBe(0);
    expect(opened.stderr).toBe('');
    expect(JSON.parse(opened.stdout)).toMatchObject({
      command: 'ingest',
      mode: 'result',
      accepted: false,
      source: 'first.md',
      state: 'candidate',
      result: {
        snapshot,
        source: { id: 'first.md' },
        accepted: false,
        status: 'candidate',
        attempts: [{ usage: { totalTokens: 150 } }],
      },
    });
    const pending = ingest(paths, ['--show', 'second.md']);
    expect(pending.status).toBe(0);
    expect(JSON.parse(pending.stdout)).toMatchObject({
      source: 'second.md',
      state: 'pending',
      result: null,
    });
    const limited = ingest(paths, ['--show', 'first.md', '--max-bytes', '1024']);
    expect(limited.status).toBe(1);
    expect(JSON.parse(limited.stderr)).toMatchObject({
      error: { code: 'INGESTION_RESULT_EXCEEDS_BUDGET' },
    });
    expect(readFileSync(paths.store)).toEqual(before);
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\n');
  });
});

test('does not follow a redirected default store directory', async () => {
  await fixture((paths) => {
    const external = join(dirname(paths.store), 'external');
    const redirected = join(paths.root, '.hivex');
    mkdirSync(external);
    symlinkSync(external, redirected, 'dir');
    try {
      const result = spawnSync(
        process.execPath,
        [cli, 'ingest', '--root', paths.root, '--codex', paths.binary, '--max-units', '1'],
        { encoding: 'utf8', timeout: 15_000 },
      );
      expect(result.status).toBe(1);
      expect(JSON.parse(result.stderr)).toMatchObject({
        error: { code: 'INVALID_INGESTION_STORE' },
      });
      expect(readdirSync(external)).toEqual([]);
      expect(readFileSync(paths.calls, 'utf8')).toBe('');
    } finally {
      rmSync(redirected);
    }
  });
});

test('reuses the same store for another cohort only after an explicit matching discard', async () => {
  await fixture((paths) => {
    const completed = ingest(paths, ['--max-units', '2']);
    expect(completed.status).toBe(0);
    const planHash = JSON.parse(completed.stdout).planHash;
    const before = readFileSync(paths.store);
    const wrong = ingest(paths, ['--discard', '0'.repeat(64)]);
    expect(wrong.status).toBe(1);
    expect(JSON.parse(wrong.stderr)).toMatchObject({ error: { code: 'INGESTION_PLAN_MISMATCH' } });
    expect(readFileSync(paths.store)).toEqual(before);
    const discarded = ingest(paths, ['--discard', planHash]);
    expect(discarded.status).toBe(0);
    expect(JSON.parse(discarded.stdout)).toMatchObject({
      command: 'ingest',
      mode: 'discard',
      discarded: true,
      planHash,
    });
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
    const restarted = ingest(paths, ['--max-units', '0']);
    expect(restarted.status).toBe(0);
    expect(JSON.parse(restarted.stdout)).toMatchObject({ completed: 0, pending: 2, processed: 0 });
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
  });
});

test('refuses to discard a cohort while it contains an unresolved invocation', async () => {
  await fixture(async (paths) => {
    const initialized = ingest(paths, ['--max-units', '0']);
    expect(initialized.status).toBe(0);
    const planHash = JSON.parse(initialized.stdout).planHash;
    writeFileSync(paths.hold, 'hold native responses');
    const active = start(paths);
    try {
      await waitForCalls(paths.calls, 1);
      const before = readFileSync(paths.store);
      const discarded = ingest(paths, ['--discard', planHash]);
      expect(discarded.status).toBe(1);
      expect(JSON.parse(discarded.stderr)).toMatchObject({
        error: { code: 'INGESTION_UNRESOLVED' },
      });
      expect(readFileSync(paths.store)).toEqual(before);
      expect(readFileSync(paths.calls, 'utf8')).toBe('called\n');
    } finally {
      active.kill('SIGKILL');
      await active.exited;
    }
  });
});

test('rejects combining result inspection and discard before changing retained evidence', async () => {
  await fixture((paths) => {
    const completed = ingest(paths, ['--max-units', '2']);
    expect(completed.status).toBe(0);
    const planHash = JSON.parse(completed.stdout).planHash;
    const before = readFileSync(paths.store);
    const mixed = ingest(paths, ['--show', 'first.md', '--discard', planHash]);
    expect(mixed.status).toBe(1);
    expect(JSON.parse(mixed.stderr)).toMatchObject({ error: { code: 'INVALID_ARGUMENT' } });
    expect(readFileSync(paths.store)).toEqual(before);
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
  });
});

test('accepts Unicode source IDs without confusing database order with the plan', async () => {
  await fixture((paths) => {
    writeFileSync(join(paths.root, '\ue000.md'), source);
    writeFileSync(join(paths.root, '😀.md'), source);
    paths.git(['add', '.']);
    paths.git([
      '-c',
      'user.name=Test',
      '-c',
      'user.email=test@example.invalid',
      'commit',
      '-qm',
      'unicode sources',
    ]);
    const initialized = ingest(paths, ['--max-units', '0']);
    expect(initialized.status).toBe(0);
    expect(JSON.parse(initialized.stdout)).toMatchObject({
      pending: 4,
      completed: 0,
      processed: 0,
    });
    expect(readFileSync(paths.calls, 'utf8')).toBe('');
  });
});

test('rejects a claimed source whose state was altered to pending instead of repeating its invocation', async () => {
  await fixture(async (paths) => {
    writeFileSync(paths.hold, 'hold native responses');
    const interrupted = start(paths);
    try {
      await waitForCalls(paths.calls, 1);
      interrupted.kill('SIGKILL');
      await interrupted.exited;
      rmSync(paths.hold);
      // Fault injection changes persisted state without touching the checkpoint's evidence.
      const damaged = new Database(paths.store);
      damaged.run("UPDATE units SET state='pending' WHERE state='running'");
      damaged.close();
      const before = readFileSync(paths.store);
      const resumed = ingest(paths);
      expect(resumed.status).toBe(1);
      expect(JSON.parse(resumed.stderr)).toMatchObject({
        error: { code: 'INVALID_INGESTION_STORE' },
      });
      expect(readFileSync(paths.store)).toEqual(before);
      expect(readFileSync(paths.calls, 'utf8')).toBe('called\n');
    } finally {
      interrupted.kill('SIGKILL');
      await interrupted.exited;
    }
  });
});

test('preflights every source before spending tokens or creating the store', async () => {
  await fixture((paths) => {
    writeFileSync(join(paths.root, 'second.md'), '# Large\n\n' + 'é'.repeat(16_384));
    paths.git(['add', 'second.md']);
    paths.git([
      '-c',
      'user.name=Test',
      '-c',
      'user.email=test@example.invalid',
      'commit',
      '-qm',
      'oversized source',
    ]);
    const result = ingest(paths);
    expect(result.status).toBe(1);
    expect(JSON.parse(result.stderr)).toMatchObject({
      error: { code: 'INGESTION_REQUIRES_SECTIONS' },
    });
    expect(existsSync(paths.store)).toBe(false);
    expect(readFileSync(paths.calls, 'utf8')).toBe('');
  });
});

test('refuses an oversized store before starting another invocation', async () => {
  await fixture((paths) => {
    expect(ingest(paths, ['--max-units', '0']).status).toBe(0);
    truncateSync(paths.store, 128 * 1024 * 1024 + 1);
    const result = ingest(paths);
    expect(result.status).toBe(1);
    expect(JSON.parse(result.stderr)).toMatchObject({ error: { code: 'INVALID_INGESTION_STORE' } });
    expect(readFileSync(paths.calls, 'utf8')).toBe('');
  });
});

test('never presents retained failures as candidates after an outcome is altered', async () => {
  await fixture((paths) => {
    writeFileSync(paths.scenario, 'invalid-json');
    expect(ingest(paths).status).toBe(1);
    expect(ingest(paths).status).toBe(1);
    const beforeCalls = readFileSync(paths.calls, 'utf8');
    expect(beforeCalls).toBe('called\n'.repeat(6));
    const damaged = new Database(paths.store);
    damaged.run("UPDATE units SET state='candidate' WHERE state='failed'");
    damaged.close();
    const progress = ingest(paths, ['--max-units', '0']);
    expect(progress.status).toBe(1);
    expect(JSON.parse(progress.stderr)).toMatchObject({
      error: { code: 'INVALID_INGESTION_STORE' },
    });
    const opened = ingest(paths, ['--show', 'first.md']);
    expect(opened.status).toBe(1);
    expect(JSON.parse(opened.stderr)).toMatchObject({ error: { code: 'INVALID_INGESTION_STORE' } });
    expect(readFileSync(paths.calls, 'utf8')).toBe(beforeCalls);
  });
});

test('applies a finite invocation deadline and retry budget while retaining failed usage', async () => {
  await fixture((paths) => {
    writeFileSync(paths.scenario, 'timeout');
    const result = ingest(paths, ['--deadline-ms', '100', '--attempts', '2']);
    expect(result.status).toBe(1);
    expect(result.stderr).toBe('');
    expect(JSON.parse(result.stdout)).toMatchObject({
      status: 'failed',
      failed: 1,
      pending: 1,
      processed: 1,
      attempts: { recorded: 2, unresolved: 0, unknownUsage: 0, knownTotalTokens: 250 },
    });
    const opened = ingest(paths, ['--show', 'first.md']);
    expect(opened.status).toBe(0);
    expect(JSON.parse(opened.stdout)).toMatchObject({
      state: 'failed',
      result: {
        attempts: [
          { code: 'MODEL_TIMEOUT', deadlineMilliseconds: 100 },
          { code: 'MODEL_TIMEOUT', deadlineMilliseconds: 100 },
        ],
      },
    });
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
  });
});

test('does not commit an in-flight result after the stored cohort changes', async () => {
  await fixture(async (paths) => {
    writeFileSync(paths.hold, 'hold native responses');
    const active = start(paths);
    try {
      await waitForCalls(paths.calls, 1);
      const changed = new Database(paths.store);
      changed.run("UPDATE cohort SET value=json_set(value, '$.planHash', ?)", ['0'.repeat(64)]);
      changed.close();
      rmSync(paths.hold);
      expect(await active.exited).toBe(1);
      expect(JSON.parse(await new Response(active.stderr).text())).toMatchObject({
        error: { code: 'INGESTION_PLAN_MISMATCH' },
      });
      expect(readFileSync(paths.calls, 'utf8')).toBe('called\n');
    } finally {
      active.kill('SIGKILL');
      await active.exited;
    }
  });
});

test('rejects the earlier experimental store format without altering its evidence', async () => {
  await fixture((paths) => {
    const previous = new Database(paths.store);
    previous.run('PRAGMA application_id=1213618249');
    previous.run('PRAGMA user_version=1');
    previous.run('CREATE TABLE cohort (id INTEGER PRIMARY KEY, value TEXT NOT NULL)');
    previous.run('INSERT INTO cohort VALUES (1, ?)', ['{"legacy":"retained evidence"}']);
    previous.close();
    const before = readFileSync(paths.store);
    for (const args of [[], ['--show', 'first.md'], ['--discard', '0'.repeat(64)]]) {
      const result = ingest(paths, args);
      expect(result.status).toBe(1);
      expect(JSON.parse(result.stderr)).toMatchObject({
        error: { code: 'INVALID_INGESTION_STORE' },
      });
    }
    expect(readFileSync(paths.store)).toEqual(before);
    expect(readFileSync(paths.calls, 'utf8')).toBe('');
  });
});

test.each([
  ['$.planHash', '0'.repeat(64)],
  ['$.snapshot.commit', '0'.repeat(40)],
])('inspection and discard reject altered cohort metadata: %s', async (field, value) => {
  await fixture((paths) => {
    const completed = ingest(paths, ['--max-units', '2']);
    expect(completed.status).toBe(0);
    const planHash = JSON.parse(completed.stdout).planHash;
    const damaged = new Database(paths.store);
    damaged.run('UPDATE cohort SET value=json_set(value, ?, ?)', [field, value]);
    damaged.close();
    const before = readFileSync(paths.store);
    const opened = ingest(paths, ['--show', 'first.md']);
    expect(opened.status).toBe(1);
    expect(JSON.parse(opened.stderr)).toMatchObject({ error: { code: 'INVALID_INGESTION_STORE' } });
    const requestedHash = field === '$.planHash' ? value : planHash;
    const discarded = ingest(paths, ['--discard', requestedHash]);
    expect(discarded.status).toBe(1);
    expect(JSON.parse(discarded.stderr)).toMatchObject({
      error: { code: 'INVALID_INGESTION_STORE' },
    });
    expect(readFileSync(paths.store)).toEqual(before);
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\ncalled\n');
  });
});
