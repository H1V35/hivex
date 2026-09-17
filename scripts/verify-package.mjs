import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  accessSync,
  constants,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { Database } from 'bun:sqlite';
import manifest from '../package.json' with { type: 'json' };

const argumentIndex = 2;
const indentation = 2;
const successfulExitCode = 0;
const retainedCount = 1;
const archive = path.resolve(
  process.argv[argumentIndex] ?? `dist/h1v35-hivex-${manifest.version}.tgz`
);
const report = JSON.parse(readFileSync(`${archive}.json`, 'utf-8'));
const digest = (file) => createHash('sha256').update(readFileSync(file)).digest('hex');
assert.equal(digest(archive), report.sha256);
assert.equal(report.version, manifest.version);

const temporary = mkdtempSync(path.join(tmpdir(), 'hivex-package-test-'));
const execute = function execute(command, argumentsList, options = {}) {
  const result = spawnSync(command, argumentsList, { encoding: 'utf-8', ...options });
  assert.equal(result.status, successfulExitCode, result.error?.message ?? result.stderr);
  return result.stdout;
};
const run = (binary, argumentsList) =>
  JSON.parse(execute(binary, argumentsList, { env: { ...process.env, PATH: '/usr/bin:/bin' } }));

using cleanup = new DisposableStack();
cleanup.defer(() => rmSync(temporary, { force: true, recursive: true }));
const entries = execute('tar', ['-tzf', archive])
  .trim()
  .split('\n')
  .filter((entry) => !entry.endsWith('/'));
assert.ok(
  entries.every((entry) => entry.startsWith('package/') && !entry.split('/').includes('..'))
);
const files = entries
  .map((entry) => entry.slice('package/'.length))
  .toSorted((left, right) => left.localeCompare(right, 'en'));
assert.deepEqual(
  files,
  [...report.files].toSorted((left, right) => left.localeCompare(right, 'en'))
);
assert.ok(
  files.every((file) => !/^(?:src|rust|test|scripts|target|node_modules|\.hivex)\//u.test(file))
);
assert.ok(files.every((file) => !/(?:^|\/)evidence\/|\.sqlite(?:-|$)/u.test(file)));
for (const skill of [
  'hivex',
  'hivex-design',
  'hivex-document',
  'hivex-implement',
  'hivex-review',
  'hivex-git',
]) {
  assert.ok(files.includes(`skills/${skill}/SKILL.md`));
}
assert.ok(files.includes('THIRD-PARTY-NOTICES.txt'));
assert.ok(files.includes('templates/README.md'));
const installed = path.join(temporary, 'installed');
execute('npm', [
  'install',
  '--offline',
  '--ignore-scripts',
  '--no-audit',
  '--no-fund',
  '--cache',
  path.join(temporary, 'npm-cache'),
  '--prefix',
  installed,
  archive,
]);
const packageRoot = path.join(installed, 'node_modules', '@h1v35', 'hivex');
const packageMetadata = JSON.parse(readFileSync(path.join(packageRoot, 'package.json'), 'utf-8'));
assert.equal(packageMetadata.version, manifest.version);
assert.deepEqual(packageMetadata.os, ['darwin']);
assert.deepEqual(packageMetadata.cpu, ['arm64']);
assert.deepEqual(Object.keys(packageMetadata.dependencies ?? {}), []);
assert.deepEqual(packageMetadata.bin, { hivex: 'bin/hivex' });
const binary = path.join(installed, 'node_modules', '.bin', 'hivex');
accessSync(binary, constants.X_OK);
assert.equal(digest(binary), report.nativeSha256);
assert.equal(run(binary, ['--help']).application, 'hivex');
assert.equal(
  JSON.parse(execute(process.execPath, ['hivex', '--help'], { cwd: installed })).application,
  'hivex'
);
const root = path.join(temporary, 'project');
mkdirSync(path.join(root, '.hivex'), { recursive: true });
writeFileSync(path.join(root, 'notes.md'), '# Policy\nUse bounded work.\nPreserve the budget.\n');
using database = new Database(path.join(root, '.hivex', 'knowledge.sqlite'));
database.run(
  readFileSync(new URL('../test/fixtures/knowledge-cache-v1.sql', import.meta.url), 'utf-8')
);
const before = database.query('SELECT data FROM graph').get();
const cacheBefore = database.query('SELECT value FROM model_cache ORDER BY key').all();
const answer = run(binary, [
  'ask',
  'bounded',
  '--source',
  'notes.md',
  '--root',
  root,
  '--max-calls',
  '1',
  '--codex',
  path.join(temporary, 'no-model'),
]);
assert.equal(answer.answer, 'Use bounded work and preserve the budget.');
assert.equal(answer.status, 'ready');
assert.equal(answer.work.calls, retainedCount);
assert.equal(answer.work.id, '84171802-e68b-43d8-b327-9ff47d302375');
assert.deepEqual(database.query('SELECT data FROM graph').get(), before);
assert.deepEqual(database.query('SELECT value FROM model_cache ORDER BY key').all(), cacheBefore);
assert.equal(
  run(binary, ['read', 'notes.md', '--root', root]).text,
  '# Policy\nUse bounded work.\nPreserve the budget.\n'
);
assert.equal(run(binary, ['sources', '--root', root]).totalDocuments, retainedCount);
assert.equal(run(binary, ['snapshot', 'export', '--root', root]).command, 'snapshot');
assert.equal(run(binary, ['warnings', '--root', root]).command, 'warnings');
const verification = {
  archive,
  checks: [
    'public-file-allowlist-and-six-skills',
    'installed-native-bin',
    'no-runtime-dependencies',
    'no-bun-or-node-in-path',
    'retained-v1-answer-and-budget',
    'unchanged-graph-and-cache',
    'source-read-and-snapshot',
  ],
  nativeSha256: report.nativeSha256,
  sha256: report.sha256,
  target: report.target,
};
writeFileSync(
  `${archive}.verification.json`,
  `${JSON.stringify(verification, null, indentation)}\n`
);
process.stdout.write(`${JSON.stringify(verification, null, indentation)}\n`);
