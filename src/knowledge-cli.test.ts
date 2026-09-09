import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const cli = join(import.meta.dirname, 'cli.ts');

function project(run: (root: string) => void) {
  const root = mkdtempSync(join(tmpdir(), 'hivex-knowledge-'));
  try {
    writeFileSync(join(root, 'cache.md'), '# Cache\n\nCached data expires after seven days.\n');
    writeFileSync(
      join(root, 'privacy.md'),
      '# Access\n\nRevoking access immediately removes cached private data.\n',
    );
    run(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

function invoke(root: string, args: string[]) {
  const result = spawnSync(process.execPath, [cli, ...args, '--root', root], {
    encoding: 'utf8',
    timeout: 12000,
  });
  return { ...result, value: result.stdout ? JSON.parse(result.stdout) : null };
}

test('plans an initial knowledge update without spending when its work budget is zero', () => {
  project((root) => {
    const response = invoke(root, ['update', '--max-calls', '0', '--codex', model(root)]);
    expect(response.status).toBe(0);
    expect(response.value).toMatchObject({
      command: 'update',
      status: 'budget-exhausted',
      work: { calls: 0, maxCalls: 0 },
      pendingDocuments: ['cache.md', 'privacy.md'],
    });
  });
});

function model(root: string, scenario = '') {
  const responses = join(root, 'responses.json');
  const binary = join(root, 'codex');
  const entry = join(root, 'codex.mjs');
  writeFileSync(
    responses,
    JSON.stringify({
      extract: {
        decisions: [
          {
            id: 'c1',
            document: 'cache.md',
            text: 'Cached data expires after seven days.',
            kind: 'constraint',
            status: 'current',
            conditions: [],
            exceptions: [],
            reason: 'Bound ordinary cache lifetime.',
            lineStart: 3,
            lineEnd: 3,
          },
          {
            id: 'c2',
            document: 'privacy.md',
            text: 'Access revocation immediately purges private cache.',
            kind: 'constraint',
            status: 'current',
            conditions: ['access is revoked'],
            exceptions: [],
            reason: 'A cache must not extend access.',
            lineStart: 3,
            lineEnd: 3,
          },
        ],
        relationships: [
          {
            id: 'r1',
            from: 'c2',
            to: 'c1',
            type: 'exception-to',
            reason: 'Revocation overrides ordinary retention for private data.',
            evidence: [
              { document: 'cache.md', lineStart: 3, lineEnd: 3 },
              { document: 'privacy.md', lineStart: 3, lineEnd: 3 },
            ],
          },
        ],
        uncertainties: [],
      },
      check: { findings: [] },
      ask: {
        answer:
          'Revoking access removes private cached data immediately; seven days is the ordinary lifetime.',
        evidence: [{ document: 'privacy.md', lineStart: 3, lineEnd: 3 }],
        uncertainties: [],
      },
    }),
  );
  const quote = (value: string) => "'" + value.replaceAll("'", "'\\''") + "'";
  writeFileSync(
    binary,
    '#!/bin/sh\nexec ' + quote(process.execPath) + ' ' + quote(entry) + ' "$@"\n',
    { mode: 0o700 },
  );
  writeFileSync(
    entry,
    'process.env.HIVEX_TEST_SCENARIO = ' +
      JSON.stringify(scenario) +
      ';\nprocess.env.HIVEX_TEST_CALLS_FILE = ' +
      JSON.stringify(join(root, 'model-calls.log')) +
      ';\nprocess.env.HIVEX_TEST_RESPONSES = ' +
      JSON.stringify(responses) +
      ';\nawait import(' +
      JSON.stringify(pathToFileURL(join(import.meta.dirname, '../test/codex-server.mjs')).href) +
      ');\n',
  );
  return binary;
}

test('resumes a checked knowledge batch without repeating its extraction or resetting its budget', () => {
  project((root) => {
    const binary = model(root);
    const first = invoke(root, ['update', '--max-calls', '1', '--codex', binary]);
    expect(first.value).toMatchObject({
      status: 'budget-exhausted',
      work: { calls: 1, maxCalls: 1 },
    });
    const held = invoke(root, ['update', '--max-calls', '1', '--codex', binary]);
    expect(held.value.work.calls).toBe(1);
    expect(readFileSync(join(root, 'model-calls.log'), 'utf8').trim().split('\n')).toHaveLength(1);
    const resumed = invoke(root, ['update', '--max-calls', '2', '--codex', binary]);
    expect(resumed.status).toBe(0);
    expect(resumed.value).toMatchObject({
      status: 'ready',
      work: { calls: 2, maxCalls: 2, totalTokens: 300 },
    });
    const found = invoke(root, ['search', 'seven days']);
    expect(found.value.decisions).toContainEqual(
      expect.objectContaining({
        text: 'Cached data expires after seven days.',
        quality: 'checked',
      }),
    );
    const context = invoke(root, ['neighbors', found.value.decisions[0].id]);
    expect(context.value.decisions).toContainEqual(
      expect.objectContaining({ text: 'Access revocation immediately purges private cache.' }),
    );
    const repeated = invoke(root, ['update', '--max-calls', '2', '--codex', binary]);
    expect(repeated.value.work.calls).toBe(2);
  });
});

test('answers a task with original Markdown evidence rather than asking the owner again', () => {
  project((root) => {
    const binary = model(root);
    expect(invoke(root, ['update', '--codex', binary]).status).toBe(0);
    const answer = invoke(root, [
      'ask',
      'private cached data after access revocation',
      '--codex',
      binary,
    ]);
    expect(answer.status).toBe(0);
    expect(answer.value.answer).toContain('removes private cached data immediately');
    expect(answer.value.evidence).toContainEqual(
      expect.objectContaining({
        document: 'privacy.md',
        lineStart: 3,
        lineEnd: 3,
        text: 'Revoking access immediately removes cached private data.',
      }),
    );
    expect(answer.value.work.calls).toBe(1);
    const repeated = invoke(root, [
      'ask',
      'private cached data after access revocation',
      '--codex',
      binary,
    ]);
    expect(repeated.value).toEqual(answer.value);
  });
});

test('discovers a cross-batch exception and follows its indirect dependents within the query limit', () => {
  project((root) => {
    rmSync(join(root, 'cache.md'));
    rmSync(join(root, 'privacy.md'));
    writeFileSync(join(root, '01-cache.md'), '# Cache\n\nCached data expires after seven days.\n');
    for (const name of ['02-note.md', '03-note.md', '04-note.md'])
      writeFileSync(join(root, name), '# Notes\n');
    writeFileSync(
      join(root, '05-access.md'),
      '# Permission\n\nWithdrawing authorisation destroys retained personal records immediately.\n',
    );
    writeFileSync(
      join(root, '06-media.md'),
      '# Previews\n\nDerivative previews inherit withdrawal handling.\n',
    );
    const binary = model(root);
    const responsePath = join(root, 'responses.json');
    const responses = JSON.parse(
      readFileSync(responsePath, 'utf8')
        .replaceAll('cache.md', '01-cache.md')
        .replaceAll('privacy.md', '05-access.md'),
    );
    responses.byDocument = {
      '01-cache.md': { decisions: [responses.extract.decisions[0]], relationships: [] },
      '05-access.md': {
        decisions: [responses.extract.decisions[1]],
        relationships: [{ ...responses.extract.relationships[0], to: '@existing:01-cache.md' }],
      },
      '06-media.md': {
        decisions: [
          {
            ...responses.extract.decisions[1],
            id: 'c3',
            document: '06-media.md',
            text: 'Thumbnail caches honor access revocation.',
          },
        ],
        relationships: [
          {
            id: 'r2',
            from: 'c3',
            to: 'c2',
            type: 'requires',
            reason: 'Thumbnail cleanup depends on revocation.',
            evidence: [
              { document: '05-access.md', lineStart: 3, lineEnd: 3 },
              { document: '06-media.md', lineStart: 3, lineEnd: 3 },
            ],
          },
        ],
      },
    };
    writeFileSync(responsePath, JSON.stringify(responses));
    const updated = invoke(root, ['update', '--max-calls', '4', '--codex', binary]);
    expect(updated.value.status).toBe('ready');
    const found = invoke(root, ['search', 'seven days']);
    const expanded = invoke(root, ['neighbors', found.value.decisions[0].id]);
    expect(expanded.value.decisions).toContainEqual(
      expect.objectContaining({ text: 'Thumbnail caches honor access revocation.' }),
    );
    const bounded = invoke(root, ['neighbors', found.value.decisions[0].id, '--limit', '2']);
    expect(bounded.value.decisions).toHaveLength(2);
    expect(bounded.value.unexpandedDecisions).toHaveLength(1);
  });
});

for (const scenario of ['invalid-json', 'changed-effort']) {
  test(`retains ${scenario} failure and does not retry it just because the budget is increased`, () => {
    project((root) => {
      const binary = model(root, scenario);
      const failed = invoke(root, ['update', '--codex', binary]);
      expect(failed.status).toBe(1);
      expect(failed.value.work).toMatchObject({
        calls: 1,
        lastAttempt: expect.objectContaining({ outcome: expect.any(String) }),
      });
      const unchanged = invoke(root, ['update', '--codex', binary, '--max-calls', '4']);
      expect(unchanged.value.work.calls).toBe(1);
      model(root);
      const retried = invoke(root, [
        'update',
        '--codex',
        binary,
        '--max-calls',
        '4',
        '--retry-failed',
      ]);
      expect(retried.value).toMatchObject({ status: 'ready', work: { calls: 3, maxCalls: 4 } });
    });
  });
}

test('reports a removed source and leaves its affected dependency unresolved', () => {
  project((root) => {
    const binary = model(root);
    expect(invoke(root, ['update', '--codex', binary]).status).toBe(0);
    const found = invoke(root, ['search', 'seven days']);
    rmSync(join(root, 'privacy.md'));
    const status = invoke(root, ['status']);
    expect(status.value).toMatchObject({ availableDecisions: 1, pendingDocuments: ['privacy.md'] });
    const context = invoke(root, ['neighbors', found.value.decisions[0].id]);
    expect(context.value.unexpandedDecisions).toHaveLength(1);
  });
});

test('keeps a decision with a bad line reference uncertain but can consult its original document', () => {
  project((root) => {
    const binary = model(root);
    const file = join(root, 'responses.json');
    const responses = JSON.parse(readFileSync(file, 'utf8'));
    responses.extract.decisions[1].lineStart = 999;
    responses.extract.decisions[1].lineEnd = 999;
    writeFileSync(file, JSON.stringify(responses));
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('partial');
    const found = invoke(root, ['search', 'revocation']);
    expect(found.value.decisions).toContainEqual(
      expect.objectContaining({ document: 'privacy.md', quality: 'uncertain', evidence: null }),
    );
    const answer = invoke(root, ['ask', 'access revocation', '--codex', binary]);
    expect(answer.value.answer).toContain('removes private cached data immediately');
    expect(answer.value.evidence).toContainEqual(
      expect.objectContaining({ document: 'privacy.md', lineStart: 3 }),
    );
    expect(answer.value.status).toBe('partial');
  });
});

test('finds source terminology even when the extracted decision does not repeat the word', () => {
  project((root) => {
    writeFileSync(
      join(root, 'cache.md'),
      '# Cache\n\nCached data expires after seven days.\n\nMarmalade is the project name for the private cache.\n',
    );
    const binary = model(root);
    expect(invoke(root, ['update', '--codex', binary]).status).toBe(0);
    const found = invoke(root, ['search', 'Marmalade']);
    expect(found.value.documents).toContainEqual(expect.objectContaining({ id: 'cache.md' }));
    expect(found.value.decisions).toContainEqual(expect.objectContaining({ document: 'cache.md' }));
  });
});

test('can use an explicitly selected document when a task uses different vocabulary', () => {
  project((root) => {
    const binary = model(root);
    expect(invoke(root, ['update', '--codex', binary]).status).toBe(0);
    const answer = invoke(root, [
      'ask',
      '¿Cuándo desaparece mi información?',
      '--source',
      'privacy.md',
      '--codex',
      binary,
    ]);
    expect(answer.value.answer).toContain('removes private cached data immediately');
    expect(answer.value.work.calls).toBe(1);
  });
});

test('ingests a large document in resumable rounds without losing earlier decisions', () => {
  project((root) => {
    rmSync(join(root, 'privacy.md'));
    const paragraphs = Array.from(
      { length: 140 },
      (_, index) =>
        `## Rule ${index}\n\nRule ${index} requires cache expiry. ${'Detailed rationale. '.repeat(16)}\n`,
    );
    writeFileSync(join(root, 'cache.md'), paragraphs.join('\n'));
    const binary = model(root);
    const file = join(root, 'responses.json');
    const responses = JSON.parse(readFileSync(file, 'utf8'));
    responses.fromVisibleRules = true;
    writeFileSync(file, JSON.stringify(responses));
    const first = invoke(root, ['update', '--max-calls', '2', '--codex', binary]);
    expect(first.value).toMatchObject({ status: 'budget-exhausted', work: { calls: 2 } });
    expect(first.value.pendingUnits.length).toBeGreaterThan(0);
    expect(first.value.pendingDocuments).toEqual(['cache.md']);
    const early = invoke(root, ['search', 'Rule 0']);
    expect(early.value.decisions).toContainEqual(
      expect.objectContaining({ text: 'Rule 0 requires cache expiry.' }),
    );
    const resumed = invoke(root, [
      'update',
      '--max-calls',
      '32',
      '--max-input-bytes',
      '1048576',
      '--codex',
      binary,
    ]);
    expect(resumed.value.status).toBe('ready');
    expect(resumed.value.pendingUnits).toEqual([]);
    const found = invoke(root, ['search', 'Rule 139']);
    expect(found.value.decisions).toContainEqual(
      expect.objectContaining({
        text: 'Rule 139 requires cache expiry.',
        evidence: expect.objectContaining({ lineStart: 559 }),
      }),
    );
    expect(invoke(root, ['status']).value.availableDecisions).toBe(140);
    responses.ask.evidence = [{ document: 'cache.md', lineStart: 559, lineEnd: 559 }];
    responses.ask.answer = 'Rule 139 requires cache expiry.';
    writeFileSync(file, JSON.stringify(responses));
    const held = invoke(root, ['ask', 'Rule 139', '--max-calls', '0', '--codex', binary]);
    expect(held.value.status).toBe('budget-exhausted');
    expect(held.value.omittedUnits).toBeGreaterThan(0);
    const answer = invoke(root, ['ask', 'Rule 139', '--max-calls', '1', '--codex', binary]);
    expect(answer.value.answer).toBe('Rule 139 requires cache expiry.');
    expect(answer.value.evidence).toContainEqual(expect.objectContaining({ lineStart: 559 }));
    expect(answer.value.omittedUnits).toBeGreaterThan(0);
    expect(readFileSync(join(root, 'model-calls.log'), 'utf8').trim().split('\n')).toHaveLength(
      resumed.value.work.calls + 1,
    );
  });
});

test('reuses retained model results when previously ingested Markdown is restored', () => {
  project((root) => {
    rmSync(join(root, 'privacy.md'));
    const original = readFileSync(join(root, 'cache.md'), 'utf8');
    const binary = model(root);
    const responsePath = join(root, 'responses.json');
    const responses = JSON.parse(readFileSync(responsePath, 'utf8'));
    responses.extract.decisions = [responses.extract.decisions[0]];
    responses.extract.relationships = [];
    writeFileSync(responsePath, JSON.stringify(responses));
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('ready');
    writeFileSync(join(root, 'cache.md'), original + '\nA new explanation.\n');
    expect(invoke(root, ['update', '--codex', binary]).value.work.calls).toBe(2);
    writeFileSync(join(root, 'cache.md'), original);
    const restored = invoke(root, ['update', '--max-calls', '0', '--codex', binary]);
    expect(restored.value).toMatchObject({ status: 'ready', work: { calls: 0, cacheHits: 2 } });
    expect(readFileSync(join(root, 'model-calls.log'), 'utf8').trim().split('\n')).toHaveLength(4);
  });
});

test('accepts instruction-source metadata when native project instructions are disabled', () => {
  project((root) => {
    const binary = model(root, 'instruction-source-metadata');
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('ready');
  });
});

test('invalidates a relationship when its independent supporting document changes', () => {
  project((root) => {
    writeFileSync(
      join(root, 'scope.md'),
      '# Scope\n\nRevocation overrides cache retention for private records.\n',
    );
    const binary = model(root);
    const file = join(root, 'responses.json');
    const responses = JSON.parse(readFileSync(file, 'utf8'));
    responses.extract.relationships[0].evidence = [
      { document: 'scope.md', lineStart: 3, lineEnd: 3 },
    ];
    writeFileSync(file, JSON.stringify(responses));
    expect(invoke(root, ['update', '--codex', binary]).value.relationships).toBe(1);
    const found = invoke(root, ['search', 'seven days']);
    writeFileSync(join(root, 'scope.md'), '# Scope\n\nThis relationship is awaiting a decision.\n');
    const neighbors = invoke(root, ['neighbors', found.value.decisions[0].id]);
    expect(neighbors.value.relationships).toEqual([]);
    expect(neighbors.value.unexpandedDecisions).toHaveLength(1);
  });
});

for (const initialBudget of [1, 2, 16])
  test(`restores earlier rounds after an intervening document version (initial budget ${initialBudget})`, () => {
    project((root) => {
      rmSync(join(root, 'privacy.md'));
      const original = Array.from(
        { length: 90 },
        (_, i) => `## Rule ${i}\n\nRule ${i} requires cache expiry. ${'Reason. '.repeat(40)}\n`,
      ).join('\n');
      writeFileSync(join(root, 'cache.md'), original);
      const binary = model(root);
      const file = join(root, 'responses.json');
      const responses = JSON.parse(readFileSync(file, 'utf8'));
      responses.fromVisibleRules = true;
      writeFileSync(file, JSON.stringify(responses));
      const first = invoke(root, [
        'update',
        '--max-calls',
        String(initialBudget),
        '--max-input-bytes',
        '1048576',
        '--codex',
        binary,
      ]);
      expect(first.value.status).toBe(initialBudget < 16 ? 'budget-exhausted' : 'ready');
      writeFileSync(join(root, 'cache.md'), original.replace('Rule 0', 'Rule zero'));
      expect(
        invoke(root, [
          'update',
          '--max-calls',
          initialBudget === 1 ? '16' : '2',
          '--max-input-bytes',
          '1048576',
          '--codex',
          binary,
        ]).value.status,
      ).toBe(initialBudget === 1 ? 'ready' : 'budget-exhausted');
      writeFileSync(join(root, 'cache.md'), original);
      const resumed = invoke(root, [
        'update',
        '--max-calls',
        '16',
        '--max-input-bytes',
        '1048576',
        '--codex',
        binary,
      ]);
      expect(resumed.value.status).toBe('ready');
      if (initialBudget < 16) expect(resumed.value.work.id).toBe(first.value.work.id);
      else expect(resumed.value.work.calls).toBe(0);
      expect(invoke(root, ['status']).value).toMatchObject({
        availableDecisions: 90,
        pendingDocuments: [],
      });
    });
  });

for (const scenario of ['unconfirmed-interrupt', 'start-unconfirmed'])
  test(`requires acknowledgement before retrying ${scenario}`, () => {
    project((root) => {
      const binary = model(root, scenario);
      const first = invoke(root, [
        'update',
        '--max-calls',
        '1',
        '--deadline-ms',
        '100',
        '--codex',
        binary,
      ]);
      expect(first.value.work.lastAttempt.turnAccepted).toBe(
        scenario === 'start-unconfirmed' ? 'unknown' : 'confirmed',
      );
      model(root);
      const retry = invoke(root, [
        'update',
        '--max-calls',
        '2',
        '--retry-failed',
        '--codex',
        binary,
      ]);
      expect(retry.status).toBe(1);
      expect(retry.stderr).toContain('WORK_UNCERTAIN');
      expect(readFileSync(join(root, 'model-calls.log'), 'utf8').trim().split('\n')).toHaveLength(
        1,
      );
      const recovered = invoke(root, ['recover', '--acknowledge-uncertain']);
      expect(recovered.status).toBe(0);
      expect(recovered.value.acknowledgedWorks).toBe(1);
      const resumed = invoke(root, [
        'update',
        '--max-calls',
        '3',
        '--retry-failed',
        '--codex',
        binary,
      ]);
      expect(resumed.value).toMatchObject({
        status: 'ready',
        work: { calls: 3, unmeasuredAttempts: 1 },
      });
    });
  });

test('keeps original CR line numbers and separators in reads and evidence', () => {
  project((root) => {
    writeFileSync(join(root, 'cache.md'), '# Cache\r\rCached data expires after seven days.\r');
    const read = invoke(root, ['read', 'cache.md', '--from', '2', '--to', '3']);
    expect(read.value.text).toBe('\rCached data expires after seven days.\r');
    const binary = model(root);
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('ready');
    const found = invoke(root, ['search', 'seven days']);
    expect(found.value.decisions).toContainEqual(
      expect.objectContaining({
        evidence: expect.objectContaining({
          lineStart: 3,
          text: 'Cached data expires after seven days.',
        }),
      }),
    );
  });
});

test('plans rounds for Markdown larger than the previous two MiB source limit', () => {
  project((root) => {
    writeFileSync(
      join(root, 'large.md'),
      ('A durable rule. ' + 'Detail '.repeat(140) + '\n\n').repeat(2400),
    );
    const response = invoke(root, ['update', '--max-calls', '0', '--codex', '/nonexistent-codex']);
    expect(response.value.pendingDocuments).toContain('large.md');
    expect(response.value.pendingUnits.length).toBeGreaterThan(100);
    expect(response.value.work.calls).toBe(0);
    expect(response.value.warnings).toEqual([]);
  });
});

test('runs maintenance through the CLI without calling the model or discarding graph knowledge', () => {
  project((root) => {
    const binary = model(root);
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('ready');
    const before = invoke(root, ['search', 'seven days']);
    const recovered = invoke(root, ['recover']);
    expect(recovered.status).toBe(0);
    expect(recovered.value.modelCalls).toBe(0);
    const pruned = invoke(root, ['prune', '--keep-completed', '0', '--keep-caches', '0']);
    expect(pruned.status).toBe(0);
    expect(pruned.value.modelCalls).toBe(0);
    const after = invoke(root, ['search', 'seven days']);
    expect(after.value.decisions).toEqual(before.value.decisions);
    expect(readFileSync(join(root, 'model-calls.log'), 'utf8').trim().split('\n')).toHaveLength(2);
  });
});

test('keeps an answer partial when its selected relationship was questioned', () => {
  project((root) => {
    const binary = model(root);
    const file = join(root, 'responses.json');
    const responses = JSON.parse(readFileSync(file, 'utf8'));
    responses.check.findings = [
      { target: 'r1', reason: 'The exception scope needs clarification.' },
    ];
    writeFileSync(file, JSON.stringify(responses));
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('partial');
    const answer = invoke(root, ['ask', 'cache', '--codex', binary]);
    expect(answer.value.status).toBe('partial');
  });
});

test('supplies an independent source cited by a selected relationship', () => {
  project((root) => {
    writeFileSync(
      join(root, 'scope.md'),
      '# Boundaries\n\nAmber overrides ordinary retention on authority withdrawal.\n',
    );
    const binary = model(root);
    const file = join(root, 'responses.json');
    const responses = JSON.parse(readFileSync(file, 'utf8'));
    responses.extract.relationships[0].evidence = [
      { document: 'scope.md', lineStart: 3, lineEnd: 3 },
    ];
    responses.ask.evidence = responses.extract.relationships[0].evidence;
    writeFileSync(file, JSON.stringify(responses));
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('ready');
    const answer = invoke(root, ['ask', 'cache', '--codex', binary]);
    expect(answer.value.evidence).toContainEqual(
      expect.objectContaining({
        document: 'scope.md',
        text: 'Amber overrides ordinary retention on authority withdrawal.',
      }),
    );
  });
});

test('checks a retained decision when another snapshot re-extracts the same unit', () => {
  project((root) => {
    rmSync(join(root, 'privacy.md'));
    const binary = model(root);
    const file = join(root, 'responses.json');
    const responses = JSON.parse(readFileSync(file, 'utf8'));
    responses.extract.decisions = [responses.extract.decisions[0]];
    responses.extract.relationships = [];
    writeFileSync(file, JSON.stringify(responses));
    expect(invoke(root, ['update', '--max-calls', '1', '--codex', binary]).value.status).toBe(
      'budget-exhausted',
    );
    writeFileSync(join(root, 'note.md'), '# Note\n\nAdditional project background.\n');
    expect(invoke(root, ['update', '--codex', binary]).value.status).toBe('ready');
    rmSync(join(root, 'note.md'));
    const restored = invoke(root, ['update', '--max-calls', '2', '--codex', binary]);
    expect(restored.value.status).toBe('ready');
    expect(invoke(root, ['status']).value.uncheckedDecisions).toEqual([]);
  });
});
