import { expect, test } from 'bun:test';
import { Database } from 'bun:sqlite';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { nativeProject } from '../test/native-project.ts';
import { invoke } from '../test/reviewed-project.ts';

function calls(path: string) {
  return readFileSync(path, 'utf8').trim()
    ? readFileSync(path, 'utf8').trim().split('\n').length
    : 0;
}

function update(
  paths: Parameters<Parameters<typeof nativeProject>[0]>[0],
  output: string,
  extra: string[] = [],
) {
  return invoke(paths.root, ['update', '--output', output, '--codex', paths.binary, ...extra]);
}

function ignoreRuntime(paths: Parameters<Parameters<typeof nativeProject>[0]>[0]) {
  writeFileSync(join(paths.root, '.gitignore'), '.hivex/\n');
  paths.git(['add', '.gitignore']);
  paths.git([
    '-c',
    'user.name=Test',
    '-c',
    'user.email=test@example.invalid',
    'commit',
    '-qm',
    'ignore runtime state',
  ]);
}

const linkedSource = '# Cache\n\nNever treat a cache as authority.\n\n[Other](second.md)\n';

test('runs the complete update cycle and returns unchanged with zero calls', async () => {
  await nativeProject(
    async (paths) => {
      ignoreRuntime(paths);
      writeFileSync(paths.scenario, 'update');
      const output = join(dirname(paths.store), 'admitted.json');
      const first = update(paths, output);
      expect(first.status).toBe(0);
      expect(JSON.parse(first.stdout)).toMatchObject({ accepted: true, status: 'admitted' });
      expect(existsSync(output)).toBe(true);
      expect(calls(paths.calls)).toBe(5);

      const unchanged = update(paths, output, ['--max-units', '0']);
      expect(unchanged.status).toBe(0);
      expect(JSON.parse(unchanged.stdout)).toMatchObject({ status: 'unchanged', calls: 0 });
      expect(calls(paths.calls)).toBe(5);
    },
    { source: '# Cache\n\nNever treat a cache as authority.\n\n[Other](second.md)\n' },
  );
});

test('resumes a no-model checkpoint without repeating completed work', async () => {
  await nativeProject(
    async (paths) => {
      ignoreRuntime(paths);
      writeFileSync(paths.scenario, 'update');
      const output = join(dirname(paths.store), 'admitted.json');
      const paused = update(paths, output, ['--max-units', '0']);
      expect(paused.status).toBe(0);
      expect(JSON.parse(paused.stdout)).toMatchObject({ status: 'partial', phase: 'ingest' });
      expect(calls(paths.calls)).toBe(0);
      const resumed = update(paths, output);
      expect(resumed.status).toBe(0);
      expect(JSON.parse(resumed.stdout)).toMatchObject({ status: 'admitted' });
      expect(calls(paths.calls)).toBe(5);
    },
    { source: linkedSource },
  );
});

test('reuses the unchanged source and preserves the previous admitted graph', async () => {
  await nativeProject(
    async (paths) => {
      ignoreRuntime(paths);
      writeFileSync(paths.scenario, 'update');
      const output = join(dirname(paths.store), 'admitted.json');
      expect(update(paths, output).status).toBe(0);
      const previous = readFileSync(output, 'utf8');
      const before = calls(paths.calls);
      writeFileSync(join(paths.root, 'first.md'), linkedSource + '\nChanged first source.\n');
      paths.git(['add', 'first.md']);
      paths.git([
        '-c',
        'user.name=Test',
        '-c',
        'user.email=test@example.invalid',
        'commit',
        '-qm',
        'change first source',
      ]);
      const next = update(paths, output);
      expect(next.status).toBe(0);
      expect(JSON.parse(next.stdout)).toMatchObject({ status: 'admitted' });
      expect(calls(paths.calls)).toBe(before + 3);
      expect(
        readFileSync(join(paths.root, '.hivex', 'update-transition', 'admitted.json'), 'utf8'),
      ).toBe(previous);
    },
    { source: linkedSource, assertSourceUnchanged: false },
  );
});

test('reports retention-required before creating a second transition directory', async () => {
  await nativeProject(
    async (paths) => {
      ignoreRuntime(paths);
      writeFileSync(paths.scenario, 'update');
      const output = join(dirname(paths.store), 'admitted.json');
      expect(update(paths, output).status).toBe(0);
      writeFileSync(join(paths.root, 'first.md'), linkedSource + '\nFirst change.\n');
      paths.git(['add', 'first.md']);
      paths.git([
        '-c',
        'user.name=Test',
        '-c',
        'user.email=test@example.invalid',
        'commit',
        '-qm',
        'first change',
      ]);
      expect(update(paths, output).status).toBe(0);
      const before = calls(paths.calls);
      writeFileSync(join(paths.root, 'second.md'), linkedSource + '\nSecond change.\n');
      paths.git(['add', 'second.md']);
      paths.git([
        '-c',
        'user.name=Test',
        '-c',
        'user.email=test@example.invalid',
        'commit',
        '-qm',
        'second change',
      ]);
      const retained = update(paths, output);
      expect(retained.status).toBe(1);
      expect(JSON.parse(retained.stdout)).toMatchObject({ status: 'retention-required' });
      expect(calls(paths.calls)).toBe(before);
      expect(readFileSync(output, 'utf8')).toContain('accepted');
    },
    { source: linkedSource, assertSourceUnchanged: false },
  );
});

test('blocks an adverse review without retrying it', async () => {
  await nativeProject(
    async (paths) => {
      ignoreRuntime(paths);
      writeFileSync(paths.scenario, 'update-adverse');
      const output = join(dirname(paths.store), 'admitted.json');
      const failed = update(paths, output);
      expect(failed.status).toBe(1);
      expect(JSON.parse(failed.stdout)).toMatchObject({
        status: 'blocked',
        phase: 'review',
        blockers: [{ kind: 'adverse' }],
      });
      const before = calls(paths.calls);
      const repeated = update(paths, output);
      expect(repeated.status).toBe(1);
      expect(calls(paths.calls)).toBe(before);
    },
    { source: linkedSource },
  );
});

test('blocks an uncertain extraction and never resumes it implicitly', async () => {
  await nativeProject(
    async (paths) => {
      ignoreRuntime(paths);
      writeFileSync(paths.scenario, 'start-unconfirmed');
      const output = join(dirname(paths.store), 'admitted.json');
      const uncertain = update(paths, output, ['--deadline-ms', '100']);
      expect(uncertain.status).toBe(1);
      expect(JSON.parse(uncertain.stdout)).toMatchObject({
        status: 'blocked',
        phase: 'ingest',
        blockers: [{ kind: 'uncertain' }],
      });
      const before = calls(paths.calls);
      writeFileSync(paths.scenario, 'update');
      const repeated = update(paths, output);
      expect(repeated.status).toBe(1);
      expect(calls(paths.calls)).toBe(before);
    },
    { source: linkedSource },
  );
});

test('reports an incompatible review store without discarding it', async () => {
  await nativeProject(
    async (paths) => {
      ignoreRuntime(paths);
      writeFileSync(paths.scenario, 'update');
      const output = join(dirname(paths.store), 'admitted.json');
      expect(update(paths, output).status).toBe(0);
      const reviews = join(paths.root, '.hivex', 'reviews.sqlite');
      using db = new Database(reviews);
      db.run("UPDATE cohort SET value='{}'");
      const incompatible = readFileSync(reviews);
      writeFileSync(join(paths.root, 'first.md'), linkedSource + '\nContract change.\n');
      paths.git(['add', 'first.md']);
      paths.git([
        '-c',
        'user.name=Test',
        '-c',
        'user.email=test@example.invalid',
        'commit',
        '-qm',
        'contract change',
      ]);
      const result = update(paths, output);
      expect(result.status).toBe(1);
      expect(JSON.parse(result.stdout)).toMatchObject({
        status: 'contract-mismatch',
        phase: 'transition',
      });
      expect(readFileSync(reviews)).toEqual(incompatible);
      expect(readFileSync(output, 'utf8')).toContain('accepted');
    },
    { source: linkedSource, assertSourceUnchanged: false },
  );
});
