import { expect, test } from 'bun:test';
import { Database } from 'bun:sqlite';
import { spawn } from 'node:child_process';
import {
  existsSync,
  linkSync,
  readFileSync,
  renameSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import { once } from 'node:events';
import { join, dirname } from 'node:path';
import { nativeProject } from '../test/native-project.ts';
import { invoke } from '../test/reviewed-project.ts';
import { IngestionStore, validateResultForPlan } from './ingestion/store.ts';
import { createPlan } from './ingestion/plan.ts';
import { loadSnapshot } from './workspace/snapshot.ts';
import { hash } from './sources/markdown.ts';

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

async function waitForCall(path: string) {
  const deadline = Date.now() + 5000;
  while (calls(path) < 1 && Date.now() < deadline)
    await new Promise((resolve) => setTimeout(resolve, 10));
  expect(calls(path)).toBeGreaterThanOrEqual(1);
}

const linkedSource = '# Cache\n\nNever treat a cache as authority.\n\n[Other](second.md)\n';

function commitFixture(paths: Parameters<Parameters<typeof nativeProject>[0]>[0], file: string) {
  paths.git(['add', file]);
  paths.git([
    '-c',
    'user.name=Test',
    '-c',
    'user.email=test@example.invalid',
    'commit',
    '-qm',
    'fixture change',
  ]);
}

function reviseFixtureSource(
  paths: Parameters<Parameters<typeof nativeProject>[0]>[0],
  input: string,
) {
  const feedback = join(dirname(paths.root), 'revision-feedback.json');
  writeFileSync(paths.scenario, 'update-adverse');
  const review = invoke(paths.root, [
    'graph',
    'review',
    'first.md',
    '--input',
    input,
    '--codex',
    paths.binary,
  ]);
  expect(JSON.parse(review.stdout)).toMatchObject({
    status: 'failed',
    report: { outcome: 'completed' },
  });
  writeFileSync(feedback, review.stdout);
  writeFileSync(paths.scenario, 'update');
  writeFileSync(
    paths.candidate,
    JSON.stringify({
      claims: [
        {
          id: 'c1',
          text: 'Never use the cache as documentary authority.',
          kind: 'constraint',
          conditions: [],
          exceptions: [],
          evidence: [{ quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }],
        },
      ],
      relations: [],
    }),
  );
  expect(
    invoke(paths.root, [
      'ingest',
      '--revise',
      'first.md',
      '--input',
      input,
      '--feedback',
      feedback,
      '--codex',
      paths.binary,
      '--attempts',
      '1',
    ]).status,
  ).toBe(0);
}

test.each(['archiving', 'active'])(
  'resumes an incoming %s bundle after a crash before checkpoint B is saved',
  async (state) => {
    await nativeProject(
      (paths) => {
        ignoreRuntime(paths);
        writeFileSync(paths.scenario, 'update');
        const output = join(dirname(paths.root), 'admitted.json');
        expect(update(paths, output, ['--neighbors', '1']).status).toBe(0);
        const checkpoint = join(paths.root, '.hivex/update.json');
        const ingestion = join(paths.root, '.hivex/ingestion.sqlite');
        const checkpointA = readFileSync(checkpoint);
        const ingestionA = readFileSync(ingestion);
        const admittedA = readFileSync(output);

        writeFileSync(join(paths.root, 'second.md'), linkedSource + '\nSource B.\n');
        commitFixture(paths, 'second.md');
        const commitB = paths.git(['rev-parse', 'HEAD']);
        expect(JSON.parse(update(paths, output, ['--max-units', '0']).stdout)).toMatchObject({
          status: 'partial',
          phase: 'ingest',
        });
        const targetB = JSON.parse(readFileSync(checkpoint, 'utf8')).target;
        const directory = join(paths.root, '.hivex/update-transition');
        const manifest = join(directory, 'manifest.json');
        const bundle = JSON.parse(readFileSync(manifest, 'utf8'));
        const archives = ['admitted', 'candidate', 'ingestion', 'reviews', 'comparisons'].map(
          (name) => join(directory, `${name}.json`),
        );
        const evidence = archives.map((path) => readFileSync(path));
        // Reproduce the durable boundary: bundle B exists, but checkpoint and stores still belong to A.
        writeFileSync(ingestion, ingestionA);
        writeFileSync(checkpoint, checkpointA);
        writeFileSync(manifest, JSON.stringify({ ...bundle, state }));
        const retainedManifest = readFileSync(manifest);
        writeFileSync(join(paths.root, 'code.ts'), 'export const laterCommit = true;\n');
        commitFixture(paths, 'code.ts');
        const commitC = paths.git(['rev-parse', 'HEAD']);
        const count = calls(paths.calls);

        const different = update(paths, output, ['--ref', commitC, '--max-units', '0']);
        expect(different.status).toBe(1);
        expect(readFileSync(checkpoint)).toEqual(checkpointA);
        expect(readFileSync(ingestion)).toEqual(ingestionA);
        expect(readFileSync(manifest)).toEqual(retainedManifest);

        const resumed = update(paths, output, ['--max-units', '0']);
        expect(JSON.parse(resumed.stdout)).toMatchObject({ status: 'partial', phase: 'ingest' });
        expect(JSON.parse(readFileSync(checkpoint, 'utf8'))).toMatchObject({
          phase: 'ingest',
          target: { ...targetB, commit: commitB, neighbors: 1 },
        });
        expect(JSON.parse(readFileSync(manifest, 'utf8'))).toMatchObject({
          state: 'active',
          target: bundle.target,
          previous: bundle.previous,
        });
        expect(archives.map((path) => readFileSync(path))).toEqual(evidence);
        expect(readFileSync(output)).toEqual(admittedA);
        expect(calls(paths.calls)).toBe(count);
        expect(JSON.parse(update(paths, output).stdout)).toMatchObject({ status: 'admitted' });
        expect(JSON.parse(readFileSync(output, 'utf8')).graph.sourceSnapshot.commit).toBe(commitB);
        expect(archives.map((path) => readFileSync(path))).toEqual(evidence);
        expect(calls(paths.calls)).toBe(count + 3);
      },
      { source: linkedSource },
    );
  },
  15000,
);

test('rejects a different collection before reusing a complete unmanaged ingestion cohort', async () => {
  await nativeProject((paths) => {
    ignoreRuntime(paths);
    writeFileSync(paths.scenario, 'update');
    writeFileSync(
      join(paths.root, 'hivex.json'),
      JSON.stringify({
        version: 1,
        collections: [
          { id: 'a', include: ['first.md'] },
          { id: 'b', include: ['second.md'] },
        ],
      }),
    );
    commitFixture(paths, 'hivex.json');
    expect(
      invoke(paths.root, ['ingest', '--collection', 'a', '--codex', paths.binary]).status,
    ).toBe(0);
    const store = join(paths.root, '.hivex/ingestion.sqlite');
    const before = readFileSync(store);
    const count = calls(paths.calls);
    const output = join(dirname(paths.root), 'admitted.json');
    const result = update(paths, output, ['--collection', 'b']);
    expect(result.status).toBe(1);
    expect(JSON.parse(result.stdout)).toMatchObject({ accepted: false, status: 'blocked' });
    expect(readFileSync(store)).toEqual(before);
    for (const path of [
      output,
      join(paths.root, '.hivex/update.json'),
      join(paths.root, '.hivex/update-transition'),
    ])
      expect(existsSync(path)).toBe(false);
    expect(calls(paths.calls)).toBe(count);
  });
});

test.each([false, true])(
  'never reports historical unchanged as current admission (checkpoint: %s)',
  async (checkpoint) => {
    await nativeProject(
      (paths) => {
        ignoreRuntime(paths);
        writeFileSync(paths.scenario, 'update');
        const output = join(dirname(paths.root), 'admitted.json');
        expect(update(paths, output).status).toBe(0);
        const commitA = paths.git(['rev-parse', 'HEAD']);
        const admittedA = readFileSync(output);
        if (!checkpoint)
          renameSync(
            join(paths.root, '.hivex/update.json'),
            join(dirname(paths.root), 'checkpoint-A.json'),
          );
        writeFileSync(join(paths.root, 'second.md'), linkedSource + '\nSource B.\n');
        commitFixture(paths, 'second.md');
        const count = calls(paths.calls);
        const result = update(paths, output, ['--ref', commitA, '--max-units', '0']);
        expect(result.status).toBe(1);
        expect(JSON.parse(result.stdout)).toMatchObject({
          accepted: false,
          status: 'blocked',
          phase: 'admit',
        });
        expect(
          JSON.parse(
            invoke(paths.root, ['graph', 'check', '--input', output, '--against', commitA]).stdout,
          ),
        ).toMatchObject({ accepted: true });
        expect(
          JSON.parse(invoke(paths.root, ['graph', 'check', '--input', output]).stdout),
        ).toMatchObject({ accepted: false, freshness: { status: 'stale' } });
        expect(readFileSync(output)).toEqual(admittedA);
        expect(calls(paths.calls)).toBe(count);
      },
      { source: linkedSource },
    );
  },
);

test('reconciles compatible pending transition manifests and preserves invalid pending evidence', async () => {
  await nativeProject(
    (paths) => {
      ignoreRuntime(paths);
      writeFileSync(paths.scenario, 'update');
      const output = join(dirname(paths.root), 'admitted.json');
      expect(update(paths, output).status).toBe(0);
      const admittedA = readFileSync(output);
      writeFileSync(join(paths.root, 'second.md'), linkedSource + '\nSource B.\n');
      commitFixture(paths, 'second.md');
      expect(update(paths, output, ['--max-units', '0']).status).toBe(0);
      const manifest = join(paths.root, '.hivex/update-transition/manifest.json');
      const active = JSON.parse(readFileSync(manifest, 'utf8'));
      const archiving = JSON.stringify({ ...active, state: 'archiving' }) + '\n';
      const count = calls(paths.calls);
      for (const pending of [
        '{',
        JSON.stringify({ ...active, target: { ...active.target, inputHash: '0'.repeat(64) } }),
        JSON.stringify({
          ...active,
          previous: { ...active.previous, admittedHash: '0'.repeat(64) },
        }),
        JSON.stringify({ ...active, files: { ...active.files, ingestion: null } }),
      ]) {
        writeFileSync(manifest, archiving);
        writeFileSync(`${manifest}.pending`, pending);
        expect(update(paths, output, ['--max-units', '0']).status).toBe(1);
        expect(readFileSync(manifest, 'utf8')).toBe(archiving);
        expect(readFileSync(`${manifest}.pending`, 'utf8')).toBe(pending);
      }
      writeFileSync(`${manifest}.pending`, JSON.stringify(active) + '\n');
      expect(JSON.parse(update(paths, output, ['--max-units', '0']).stdout)).toMatchObject({
        phase: 'ingest',
        status: 'partial',
      });
      expect(JSON.parse(readFileSync(manifest, 'utf8'))).toEqual(active);
      expect(existsSync(`${manifest}.pending`)).toBe(false);
      expect(readFileSync(output)).toEqual(admittedA);
      expect(calls(paths.calls)).toBe(count);
      expect(JSON.parse(update(paths, output).stdout)).toMatchObject({ status: 'admitted' });
      expect(calls(paths.calls)).toBe(count + 3);
      const complete = JSON.parse(readFileSync(manifest, 'utf8'));
      writeFileSync(manifest, JSON.stringify({ ...complete, state: 'active' }) + '\n');
      writeFileSync(`${manifest}.pending`, JSON.stringify(complete) + '\n');
      expect(JSON.parse(update(paths, output, ['--max-units', '0']).stdout)).toMatchObject({
        status: 'unchanged',
      });
      expect(JSON.parse(readFileSync(manifest, 'utf8'))).toEqual(complete);
      expect(existsSync(`${manifest}.pending`)).toBe(false);
      expect(calls(paths.calls)).toBe(count + 3);
    },
    { source: linkedSource },
  );
}, 15000);

test('rejects an older ingestion processing contract before any snapshot transition', async () => {
  await nativeProject((paths) => {
    ignoreRuntime(paths);
    const plan = createPlan(loadSnapshot({ root: paths.root, ref: 'HEAD' }), null);
    plan.processing.nativeVersion = 'codex-cli 0.0.1';
    const {
      command: _command,
      accepted: _accepted,
      planHash: _planHash,
      summary: _summary,
      ...body
    } = plan;
    plan.planHash = hash(JSON.stringify(body));
    const store = join(paths.root, '.hivex/ingestion.sqlite');
    {
      using _store = new IngestionStore(store, plan);
    }
    const before = readFileSync(store);
    writeFileSync(join(paths.root, 'code.ts'), 'export const version = 2;\n');
    commitFixture(paths, 'code.ts');
    const output = join(dirname(paths.root), 'admitted.json');
    const result = update(paths, output);
    expect(JSON.parse(result.stdout)).toMatchObject({ status: 'contract-mismatch' });
    expect(readFileSync(store)).toEqual(before);
    expect(existsSync(join(paths.root, '.hivex/update-transition'))).toBe(false);
    expect(existsSync(join(paths.root, '.hivex/update-candidate.json'))).toBe(false);
    expect(existsSync(output)).toBe(false);
    expect(calls(paths.calls)).toBe(0);
  });
});

test('keeps an intermediate candidate intact when its predecessor owns the active bundle', async () => {
  await nativeProject(
    (paths) => {
      ignoreRuntime(paths);
      writeFileSync(paths.scenario, 'update');
      const output = join(dirname(paths.root), 'admitted.json');
      expect(update(paths, output).status).toBe(0);
      const previous = readFileSync(output);
      writeFileSync(join(paths.root, 'first.md'), linkedSource + '\nChanged source.\n');
      commitFixture(paths, 'first.md');
      writeFileSync(paths.scenario, 'update-adverse');
      expect(JSON.parse(update(paths, output).stdout)).toMatchObject({
        phase: 'review',
        status: 'blocked',
      });
      const input = join(paths.root, '.hivex/update-candidate.json');
      reviseFixtureSource(paths, input);
      const protectedPaths = [
        input,
        join(paths.root, '.hivex/reviews.sqlite'),
        join(paths.root, '.hivex/ingestion.sqlite'),
        join(paths.root, '.hivex/update-transition/candidate.json'),
        join(paths.root, '.hivex/update-transition/manifest.json'),
      ];
      const before = protectedPaths.map((path) => readFileSync(path));
      const count = calls(paths.calls);
      expect(JSON.parse(update(paths, output, ['--max-units', '0']).stdout)).toMatchObject({
        status: 'retention-required',
      });
      expect(protectedPaths.map((path) => readFileSync(path))).toEqual(before);
      expect(readFileSync(output)).toEqual(previous);
      expect(calls(paths.calls)).toBe(count);
    },
    { source: linkedSource, assertSourceUnchanged: false },
  );
});

test('does not return unchanged for another requested scope without a checkpoint', async () => {
  await nativeProject(
    (paths) => {
      ignoreRuntime(paths);
      writeFileSync(paths.scenario, 'update');
      writeFileSync(join(paths.root, 'third.md'), '# Cache\n\nNever treat a cache as authority.\n');
      writeFileSync(
        join(paths.root, 'hivex.json'),
        JSON.stringify({
          version: 1,
          collections: [
            { id: 'project', include: ['first.md', 'second.md'] },
            { id: 'other', include: ['third.md'], default: false },
          ],
        }),
      );
      commitFixture(paths, '.');
      const output = join(dirname(paths.root), 'admitted.json');
      expect(update(paths, output, ['--neighbors', '1']).status).toBe(0);
      const before = readFileSync(output);
      const count = calls(paths.calls);
      const checkpoint = join(paths.root, '.hivex/update.json');
      for (const [index, selection] of [
        ['--neighbors', '2'],
        ['--collection', 'other'],
      ].entries()) {
        if (existsSync(checkpoint))
          renameSync(checkpoint, join(dirname(paths.root), `checkpoint-${index}.json`));
        const result = update(paths, output, [...selection, '--max-units', '0']);
        expect(result.status).toBe(1);
        expect(JSON.parse(result.stdout).status).not.toBe('unchanged');
        expect(readFileSync(output)).toEqual(before);
        expect(calls(paths.calls)).toBe(count);
      }
    },
    { source: linkedSource },
  );
});

test.each([false, true])(
  'propagates the frozen ref to assessments and keeps HEAD admission explicit (checkpoint: %s)',
  async (checkpoint) => {
    await nativeProject(
      (paths) => {
        ignoreRuntime(paths);
        writeFileSync(paths.scenario, 'update');
        const output = join(dirname(paths.root), 'admitted.json');
        const original = paths.git(['rev-parse', 'HEAD']);
        if (checkpoint) expect(update(paths, output, ['--max-units', '0']).status).toBe(0);
        writeFileSync(join(paths.root, 'second.md'), linkedSource + '\nLater documentary input.\n');
        commitFixture(paths, 'second.md');
        const args = checkpoint ? [] : ['--ref', original];
        const reviewed = update(paths, output, args);
        expect(JSON.parse(reviewed.stdout)).toMatchObject({
          status: 'blocked',
          phase: 'admit',
          target: original,
        });
        expect(calls(paths.calls)).toBe(5);
        expect(existsSync(output)).toBe(false);
        expect(JSON.parse(update(paths, output, ['--max-units', '0']).stdout)).toMatchObject({
          phase: 'admit',
        });
        expect(calls(paths.calls)).toBe(5);
        writeFileSync(join(paths.root, 'second.md'), linkedSource);
        commitFixture(paths, 'second.md');
        expect(JSON.parse(update(paths, output, ['--max-units', '0']).stdout)).toMatchObject({
          status: 'admitted',
        });
        expect(calls(paths.calls)).toBe(5);
      },
      { source: linkedSource },
    );
  },
);

test('does not archive a newly created comparison store into its single-source predecessor', async () => {
  await nativeProject((paths) => {
    ignoreRuntime(paths);
    paths.git(['rm', 'second.md']);
    commitFixture(paths, '.');
    writeFileSync(paths.scenario, 'update');
    const output = join(dirname(paths.root), 'admitted.json');
    expect(update(paths, output, ['--neighbors', '1']).status).toBe(0);
    const comparisons = join(paths.root, '.hivex/comparisons.sqlite');
    expect(existsSync(comparisons)).toBe(false);
    writeFileSync(join(paths.root, 'second.md'), '# Cache\n\nNever treat a cache as authority.\n');
    commitFixture(paths, 'second.md');
    expect(JSON.parse(update(paths, output, ['--max-units', '0']).stdout)).toMatchObject({
      phase: 'ingest',
    });
    expect(invoke(paths.root, ['ingest', '--codex', paths.binary]).status).toBe(0);
    expect(JSON.parse(update(paths, output, ['--max-units', '0']).stdout)).toMatchObject({
      phase: 'review',
    });
    const input = join(paths.root, '.hivex/update-candidate.json');
    expect(
      invoke(paths.root, ['graph', 'review', '--all', '--input', input, '--codex', paths.binary])
        .status,
    ).toBe(0);
    expect(JSON.parse(update(paths, output, ['--max-units', '0']).stdout)).toMatchObject({
      phase: 'compare',
      status: 'partial',
    });
    const manifest = join(paths.root, '.hivex/update-transition/manifest.json');
    const saved = readFileSync(manifest, 'utf8');
    expect(JSON.parse(saved)).toMatchObject({ state: 'active', files: { comparisons: null } });
    expect(existsSync(comparisons)).toBe(true);
    const count = calls(paths.calls);
    expect(JSON.parse(update(paths, output, ['--max-units', '0']).stdout)).toMatchObject({
      phase: 'compare',
      status: 'partial',
    });
    expect(readFileSync(manifest, 'utf8')).toBe(saved);
    expect(calls(paths.calls)).toBe(count);
    expect(JSON.parse(update(paths, output).stdout)).toMatchObject({ status: 'admitted' });
    expect(calls(paths.calls)).toBe(count + 1);
  });
}, 15000);

test.each(['invalid-json', 'start-unconfirmed'])(
  'prefers full oversized extraction receipts over summarized checkpoints (%s)',
  async (scenario) => {
    await nativeProject((paths) => {
      ignoreRuntime(paths);
      writeFileSync(paths.scenario, scenario);
      const extracted = invoke(paths.root, [
        'extract',
        'first.md',
        '--codex',
        paths.binary,
        '--attempts',
        '1',
        '--deadline-ms',
        '1000',
      ]);
      const result: Parameters<IngestionStore['complete']>[2] = JSON.parse(extracted.stdout);
      const plan = createPlan(loadSnapshot({ root: paths.root, ref: 'HEAD' }), null);
      const first = validateResultForPlan(result, 'first.md', plan).attempts[0];
      if (!first) throw new Error('Expected the full fixture receipt');
      const report = { ...first, diagnostic: { detail: 'x'.repeat(1024 * 1024) } };
      const path = join(paths.root, '.hivex/ingestion.sqlite');
      {
        using store = new IngestionStore(path, plan);
        const owner = 'oversized-fixture';
        expect(store.claim(owner)).toBe('first.md');
        store.checkpoint('first.md', owner, {
          state: 'started',
          attempt: 1,
          promptHash: report.promptHash,
          deadlineMilliseconds: report.deadlineMilliseconds,
        });
        store.checkpoint('first.md', owner, { state: 'recorded', attempt: 1, report });
        store.complete('first.md', owner, { ...result, attempts: [report] });
      }
      const before = IngestionStore.export(path, 4 * 1024 * 1024);
      expect(before.units[0]?.checkpoint.reports[0]).toHaveProperty('omittedDetails');
      const count = calls(paths.calls);
      const updated = update(paths, join(dirname(paths.root), 'admitted.json'), [
        '--max-units',
        '0',
      ]);
      if (scenario === 'invalid-json')
        expect(JSON.parse(updated.stdout)).toMatchObject({
          status: 'blocked',
          phase: 'ingest',
          blockers: [{ id: 'first.md', kind: 'failed' }],
        });
      else
        expect(JSON.parse(updated.stdout)).toMatchObject({
          status: 'blocked',
          phase: 'ingest',
          blockers: [{ id: 'first.md', kind: 'uncertain' }],
        });
      expect(calls(paths.calls)).toBe(count);
      expect(IngestionStore.export(path, 4 * 1024 * 1024)).toEqual(before);
    });
  },
);

test('rejects output and pending collisions with reserved paths before creating stores', async () => {
  await nativeProject((paths) => {
    ignoreRuntime(paths);
    const alias = join(dirname(paths.root), 'project-alias');
    symlinkSync(paths.root, alias);
    const reserved = [
      '.hivex/ingestion.sqlite',
      '.hivex/reviews.sqlite',
      '.hivex/comparisons.sqlite',
      '.hivex/update.json',
      '.hivex/update-candidate.json.pending',
      '.hivex/update.lock',
      '.hivex/reviews.sqlite-journal',
      '.hivex/comparisons.sqlite-wal',
      '.hivex/update-transition/manifest.json',
      '.hivex/../.hivex/REVIEWS.SQLITE',
      join(alias, '.hivex/reviews.sqlite'),
    ];
    for (const path of reserved) {
      const result = update(paths, path, ['--max-units', '0']);
      expect(result.status).toBe(1);
      expect(JSON.parse(result.stderr)).toMatchObject({ error: { code: 'UPDATE_PATH_COLLISION' } });
      expect(existsSync(join(paths.root, '.hivex'))).toBe(false);
    }
    const output = join(dirname(paths.root), 'admitted.json');
    expect(update(paths, output, ['--max-units', '0']).status).toBe(0);
    const checkpoint = join(paths.root, '.hivex/update.json');
    const before = readFileSync(checkpoint);
    linkSync(checkpoint, `${output}.pending`);
    const collided = update(paths, output, ['--max-units', '0']);
    expect(JSON.parse(collided.stderr)).toMatchObject({ error: { code: 'UPDATE_PATH_COLLISION' } });
    expect(readFileSync(checkpoint)).toEqual(before);
    expect(readFileSync(`${output}.pending`)).toEqual(before);
    expect(calls(paths.calls)).toBe(0);
  });
});

test('reconciles a published admission before unchanged and permits a later ref', async () => {
  await nativeProject(
    (paths) => {
      ignoreRuntime(paths);
      writeFileSync(paths.scenario, 'update');
      const output = join(dirname(paths.root), 'admitted.json');
      expect(update(paths, output).status).toBe(0);
      const oldHash = JSON.parse(readFileSync(output, 'utf8')).hash;
      writeFileSync(join(paths.root, 'second.md'), linkedSource + '\nUpdated source.\n');
      commitFixture(paths, 'second.md');
      expect(update(paths, output).status).toBe(0);
      const published = readFileSync(output, 'utf8');
      const artifact = JSON.parse(published);
      const checkpoint = join(paths.root, '.hivex/update.json');
      const state = JSON.parse(readFileSync(checkpoint, 'utf8'));
      const directory = join(paths.root, '.hivex/update-transition');
      const manifest = join(directory, 'manifest.json');
      const transition = JSON.parse(readFileSync(manifest, 'utf8'));
      writeFileSync(
        checkpoint,
        JSON.stringify({ ...state, phase: 'admit', admittedHash: oldHash }),
      );
      writeFileSync(manifest, JSON.stringify({ ...transition, state: 'active' }));
      const before = calls(paths.calls);
      const resumed = update(paths, output, ['--max-units', '0']);
      expect(JSON.parse(resumed.stdout)).toMatchObject({ status: 'unchanged' });
      expect(JSON.parse(readFileSync(checkpoint, 'utf8'))).toMatchObject({
        phase: 'admitted',
        admittedHash: artifact.hash,
        candidateHash: artifact.graph.hash,
      });
      expect(JSON.parse(readFileSync(manifest, 'utf8'))).toMatchObject({ state: 'complete' });
      expect(readFileSync(output, 'utf8')).toBe(published);
      renameSync(directory, join(dirname(paths.root), 'retained-transition'));
      writeFileSync(join(paths.root, 'code.ts'), 'export const value = 1;\n');
      commitFixture(paths, 'code.ts');
      // Recovery must also precede validation of an explicit later revision.
      writeFileSync(
        checkpoint,
        JSON.stringify({ ...state, phase: 'admit', admittedHash: oldHash }),
      );
      const next = update(paths, output, [
        '--ref',
        paths.git(['rev-parse', 'HEAD']),
        '--max-units',
        '0',
      ]);
      expect(JSON.parse(next.stdout)).toMatchObject({ status: 'unchanged' });
      expect(calls(paths.calls)).toBe(before);
    },
    { source: linkedSource },
  );
});

test('reconciles a pending review checkpoint over build and preserves incompatible pending bytes', async () => {
  await nativeProject(
    (paths) => {
      ignoreRuntime(paths);
      writeFileSync(paths.scenario, 'update');
      const output = join(dirname(paths.root), 'admitted.json');
      expect(update(paths, output, ['--max-units', '0']).status).toBe(0);
      expect(invoke(paths.root, ['ingest', '--codex', paths.binary]).status).toBe(0);
      const built = invoke(paths.root, ['graph', 'build', '--export']);
      expect(built.status).toBe(0);
      writeFileSync(join(paths.root, '.hivex/update-candidate.json'), built.stdout);
      const checkpoint = join(paths.root, '.hivex/update.json');
      const original = JSON.parse(readFileSync(checkpoint, 'utf8'));
      const building = JSON.stringify({ ...original, phase: 'build' }) + '\n';
      const reviewing = {
        ...original,
        phase: 'review',
        candidateHash: JSON.parse(built.stdout).hash,
      };
      const before = calls(paths.calls);
      for (const value of [
        '{',
        JSON.stringify({ ...reviewing, output: output + '.other' }),
        JSON.stringify({
          ...reviewing,
          target: { ...reviewing.target, inputHash: '0'.repeat(64) },
        }),
        JSON.stringify({ ...reviewing, candidateHash: '0'.repeat(64) }),
      ]) {
        writeFileSync(checkpoint, building);
        writeFileSync(`${checkpoint}.pending`, value);
        expect(update(paths, output, ['--max-units', '0']).status).toBe(1);
        expect(readFileSync(checkpoint, 'utf8')).toBe(building);
        expect(readFileSync(`${checkpoint}.pending`, 'utf8')).toBe(value);
      }
      writeFileSync(`${checkpoint}.pending`, JSON.stringify(reviewing) + '\n');
      const resumed = update(paths, output, ['--max-units', '0']);
      expect(JSON.parse(resumed.stdout)).toMatchObject({ status: 'partial', phase: 'review' });
      expect(existsSync(`${checkpoint}.pending`)).toBe(false);
      expect(JSON.parse(readFileSync(checkpoint, 'utf8'))).toMatchObject(reviewing);
      expect(calls(paths.calls)).toBe(before);
      expect(JSON.parse(update(paths, output).stdout)).toMatchObject({ status: 'admitted' });
      expect(calls(paths.calls)).toBe(before + 3);
    },
    { source: linkedSource },
  );
});

test('updates a fresh admitted output after an explicit same-commit extraction revision', async () => {
  await nativeProject(
    (paths) => {
      ignoreRuntime(paths);
      writeFileSync(paths.scenario, 'update');
      const output = join(dirname(paths.root), 'admitted.json');
      expect(update(paths, output).status).toBe(0);
      const previous = readFileSync(output, 'utf8');
      const old = JSON.parse(previous);
      const input = join(paths.root, '.hivex/update-candidate.json');
      const candidate = readFileSync(input, 'utf8');
      const feedback = join(dirname(paths.root), 'adverse.json');
      writeFileSync(paths.scenario, 'update-adverse');
      const review = invoke(paths.root, [
        'graph',
        'review',
        'first.md',
        '--input',
        input,
        '--codex',
        paths.binary,
      ]);
      expect(JSON.parse(review.stdout)).toMatchObject({
        status: 'failed',
        report: { outcome: 'completed' },
      });
      writeFileSync(feedback, review.stdout);
      writeFileSync(paths.scenario, 'update');
      writeFileSync(
        paths.candidate,
        JSON.stringify({
          claims: [
            {
              id: 'c1',
              text: 'Never use the cache as documentary authority.',
              kind: 'constraint',
              conditions: [],
              exceptions: [],
              evidence: [{ quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }],
            },
          ],
          relations: [],
        }),
      );
      const revised = invoke(paths.root, [
        'ingest',
        '--revise',
        'first.md',
        '--input',
        input,
        '--feedback',
        feedback,
        '--codex',
        paths.binary,
        '--attempts',
        '1',
      ]);
      expect(revised.status).toBe(0);
      const before = calls(paths.calls);
      const paused = update(paths, output, ['--max-units', '0']);
      expect(JSON.parse(paused.stdout)).toMatchObject({ status: 'partial', phase: 'review' });
      expect(calls(paths.calls)).toBe(before);
      expect(readFileSync(output, 'utf8')).toBe(previous);
      const directory = join(paths.root, '.hivex/update-transition');
      expect(readFileSync(join(directory, 'candidate.json'), 'utf8')).toBe(candidate);
      expect(readFileSync(join(directory, 'admitted.json'), 'utf8')).toBe(previous);
      expect(JSON.parse(update(paths, output).stdout)).toMatchObject({ status: 'admitted' });
      const current = JSON.parse(readFileSync(output, 'utf8'));
      expect(current.graph.inputHash).toBe(old.graph.inputHash);
      expect(current.graph.sources[0].extraction.receiptHash).not.toBe(
        old.graph.sources[0].extraction.receiptHash,
      );
      expect(current.sourceReviews[1].report).toEqual(old.sourceReviews[1].report);
      expect(calls(paths.calls)).toBe(before + 2);
      expect(JSON.parse(update(paths, output).stdout)).toMatchObject({ status: 'unchanged' });
      expect(calls(paths.calls)).toBe(before + 2);
    },
    { source: linkedSource },
  );
}, 15000);

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

test('archives the previous graph before resuming after an explicit candidate revision', async () => {
  await nativeProject(
    async (paths) => {
      ignoreRuntime(paths);
      writeFileSync(paths.scenario, 'update-adverse');
      const output = join(dirname(paths.store), 'admitted.json');
      const failed = update(paths, output);
      expect(failed.status).toBe(1);
      const candidateGraph = join(paths.root, '.hivex', 'update-candidate.json');
      const previousGraph = readFileSync(candidateGraph, 'utf8');
      const feedback = join(dirname(paths.store), 'review.json');
      const exported = invoke(paths.root, [
        'graph',
        'review',
        '--export',
        '--input',
        candidateGraph,
        '--store',
        join(paths.root, '.hivex', 'reviews.sqlite'),
        '--max-bytes',
        '134217728',
      ]);
      expect(exported.stdout).toContain('"operation":"review-cohort"');
      writeFileSync(feedback, exported.stdout);
      writeFileSync(paths.scenario, 'update');
      writeFileSync(
        paths.candidate,
        JSON.stringify({
          claims: [
            {
              id: 'c1',
              text: 'A revised cache rule must never be treated as authority.',
              kind: 'constraint',
              conditions: [],
              exceptions: [],
              evidence: [{ quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }],
            },
          ],
          relations: [],
        }),
      );
      const revised = invoke(paths.root, [
        'ingest',
        '--revise',
        'first.md',
        '--input',
        candidateGraph,
        '--feedback',
        feedback,
        '--store',
        join(paths.root, '.hivex', 'ingestion.sqlite'),
        '--codex',
        paths.binary,
        '--attempts',
        '1',
      ]);
      expect(revised.status).toBe(0);
      const beforeUpdate = calls(paths.calls);
      const admitted = update(paths, output);
      expect(admitted.status).toBe(0);
      expect(JSON.parse(admitted.stdout)).toMatchObject({ status: 'admitted' });
      expect(calls(paths.calls)).toBe(beforeUpdate + 3);
      expect(
        readFileSync(join(paths.root, '.hivex', 'update-transition', 'candidate.json'), 'utf8'),
      ).toBe(previousGraph);
    },
    { source: linkedSource },
  );
});

test('blocks active and obsolete coordinator locks without reaping either lock', async () => {
  await nativeProject(
    async (paths) => {
      ignoreRuntime(paths);
      writeFileSync(paths.scenario, 'update');
      writeFileSync(paths.hold, 'hold');
      const output = join(dirname(paths.store), 'admitted.json');
      const cli = join(import.meta.dirname, 'cli.ts');
      const first = spawn(process.execPath, [
        cli,
        'update',
        '--output',
        output,
        '--codex',
        paths.binary,
        '--max-units',
        '1',
        '--root',
        paths.root,
      ]);
      await waitForCall(paths.calls);
      const lock = join(paths.root, '.hivex', 'update.lock');
      expect(existsSync(lock)).toBe(true);
      const active = update(paths, output, ['--max-units', '0']);
      expect(active.status).toBe(1);
      expect(JSON.parse(active.stdout)).toMatchObject({
        status: 'blocked',
        reason: 'lock-active',
        lock,
      });
      expect(existsSync(lock)).toBe(true);
      rmSync(paths.hold);
      await once(first, 'close');

      const stale = JSON.stringify({ pid: 2_147_483_647 });
      writeFileSync(lock, stale);
      const obsolete = update(paths, output, ['--max-units', '0']);
      expect(obsolete.status).toBe(1);
      expect(JSON.parse(obsolete.stdout)).toMatchObject({
        status: 'blocked',
        reason: 'lock-obsolete',
        lock,
      });
      expect(readFileSync(lock, 'utf8')).toBe(stale);
    },
    { source: linkedSource },
  );
});

test('finalizes an exact pending artifact and blocks a partial pending artifact', async () => {
  await nativeProject(
    async (paths) => {
      ignoreRuntime(paths);
      writeFileSync(paths.scenario, 'update');
      const output = join(dirname(paths.store), 'admitted.json');
      expect(update(paths, output).status).toBe(0);
      const before = calls(paths.calls);
      renameSync(output, `${output}.pending`);
      const resumed = update(paths, output, ['--max-units', '0']);
      expect(resumed.status).toBe(0);
      expect(existsSync(output)).toBe(true);
      expect(existsSync(`${output}.pending`)).toBe(false);
      expect(calls(paths.calls)).toBe(before);
      const checkpoint = join(paths.root, '.hivex', 'update.json');
      renameSync(checkpoint, `${checkpoint}.pending`);
      const checkpointed = update(paths, output, ['--max-units', '0']);
      expect(checkpointed.status).toBe(0);
      expect(existsSync(checkpoint)).toBe(true);
      expect(existsSync(`${checkpoint}.pending`)).toBe(false);

      renameSync(output, `${output}.pending`);
      writeFileSync(`${output}.pending`, '{');
      const blockedPending = update(paths, output, ['--max-units', '0']);
      expect(blockedPending.status).toBe(1);
      expect(JSON.parse(blockedPending.stdout)).toMatchObject({ status: 'blocked' });
      expect(existsSync(`${output}.pending`)).toBe(true);
    },
    { source: linkedSource },
  );
});
