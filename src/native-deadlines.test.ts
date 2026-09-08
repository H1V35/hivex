import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { nativeProject } from '../test/native-project.ts';
import { invoke } from '../test/reviewed-project.ts';
import { assessmentArguments } from './graph/assessment-cohort.ts';
import { extractionArguments } from './ingestion/arguments.ts';

test('accepts the 30-minute native deadline and keeps the current default', () => {
  expect(extractionArguments(['source.md']).deadlineMilliseconds).toBe(600_000);
  expect(extractionArguments(['source.md', '--deadline-ms', '1800000']).deadlineMilliseconds).toBe(
    1_800_000,
  );
  expect(() => extractionArguments(['source.md', '--deadline-ms', '1800001'])).toThrow();

  expect(
    assessmentArguments(['--all', '--input', 'graph.json', '--deadline-ms', '1800000'], 'review')
      .deadlineMilliseconds,
  ).toBe(1_800_000);
});

test('retains a 30-minute ingestion receipt across progress and inspection without another call', async () => {
  await nativeProject((paths) => {
    const args = ['ingest', '--store', paths.store];
    const ingested = invoke(paths.root, [
      ...args,
      '--codex',
      paths.binary,
      '--max-units',
      '1',
      '--deadline-ms',
      '1800000',
    ]);
    expect(ingested.stderr).toBe('');
    expect(ingested.status).toBe(0);
    expect(JSON.parse(ingested.stdout)).toMatchObject({ completed: 1, processed: 1 });
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\n');

    const progress = invoke(paths.root, [...args, '--codex', paths.binary, '--max-units', '0']);
    expect(progress.stderr).toBe('');
    expect(progress.status).toBe(0);
    expect(JSON.parse(progress.stdout)).toMatchObject({ completed: 1, processed: 0 });
    const inspected = invoke(paths.root, [...args, '--show', 'first.md']);
    expect(inspected.stderr).toBe('');
    expect(inspected.status).toBe(0);
    expect(JSON.parse(inspected.stdout)).toMatchObject({
      state: 'candidate',
      result: {
        status: 'candidate',
        attempts: [{ outcome: 'completed', deadlineMilliseconds: 1_800_000 }],
      },
    });
    expect(readFileSync(paths.calls, 'utf8')).toBe('called\n');
  });
});
