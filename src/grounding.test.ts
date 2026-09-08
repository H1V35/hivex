import { expect, test } from 'bun:test';
import { readFileSync, writeFileSync, rmSync, symlinkSync } from 'node:fs';
import { dirname, join } from 'node:path';
import {
  projectWithReviews,
  compare,
  admit,
  retained,
  invoke,
  comparisonResponse,
  type Paths,
  type Fixture,
} from '../test/reviewed-project.ts';

test('prepares a claim review against committed code and an admitted manifest without invoking a model', async () => {
  await projectWithReviews((paths, fixture) => {
    expect(compare(paths, fixture).status).toBe(0);
    const admitted = admit(paths, fixture);
    expect(admitted.status).toBe(0);
    const input = retained(paths, admitted.stdout);
    const base = paths.git(['rev-parse', 'HEAD']);
    writeFileSync(join(paths.root, 'cache.ts'), 'export const cacheIsAuthority = false;\n');
    paths.git(['add', 'cache.ts']);
    paths.git([
      '-c',
      'user.name=Test',
      '-c',
      'user.email=test@example.invalid',
      'commit',
      '-qm',
      'Add cache policy',
    ]);
    const calls = readFileSync(paths.calls, 'utf8');
    const prepared = invoke(paths.root, [
      'ground',
      'The configuration does not treat a cache as authority.',
      '--input',
      input,
      '--base',
      base,
      '--prepare',
    ]);
    expect(prepared.stderr).toBe('');
    expect(prepared.status).toBe(0);
    const result = JSON.parse(prepared.stdout);
    expect(result).toMatchObject({
      command: 'ground',
      accepted: false,
      implementationAccepted: false,
      status: 'prepared',
      graphHash: JSON.parse(admitted.stdout).hash,
      codeSnapshot: { base, head: paths.git(['rev-parse', 'HEAD']) },
    });
    expect(result.prompt).toContain('export const cacheIsAuthority = false;');
    expect(result.prompt).toContain('Never treat a cache as authority.');
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
  });
});

function commit(paths: Paths) {
  paths.git(['add', '.']);
  paths.git([
    '-c',
    'user.name=Test',
    '-c',
    'user.email=test@example.invalid',
    'commit',
    '-qm',
    'Code change',
  ]);
}
const claim = 'The configuration does not treat a cache as authority.';
const code = 'export const cacheIsAuthority = false;\n';
async function implementation(
  run: (
    paths: Paths,
    fixture: Fixture & { admitted: string; base: string },
  ) => void | Promise<void>,
  relation = 'equivalent',
) {
  await projectWithReviews(async (paths, fixture) => {
    const response = comparisonResponse();
    const edge = response.relations[0];
    if (!edge) throw new Error('Expected relation');
    edge.type = relation;
    edge.scope.extent = 'partial-claim';
    edge.scope.description = 'A cache exception with limited scope.';
    expect(compare(paths, fixture, response).status).toBe(0);
    const result = admit(paths, fixture);
    expect(result.status).toBe(0);
    const admitted = retained(paths, result.stdout);
    const base = paths.git(['rev-parse', 'HEAD']);
    writeFileSync(join(paths.root, 'cache.ts'), code);
    commit(paths);
    await run(paths, { ...fixture, admitted, base });
  });
}
function command(fixture: { admitted: string; base: string }) {
  return ['ground', claim, '--input', fixture.admitted, '--base', fixture.base];
}
function assessment() {
  return {
    verdict: 'supported',
    reason: 'The exact implementation leaves authority outside its cache.',
    coverage: {
      complete: true,
      code: [{ id: 'f1', relevance: 'relevant', reason: 'Implements the cache policy.' }],
      sources: ['s1', 's2'].map((id) => ({
        id,
        relevance: 'relevant',
        reason: 'Defines the cache policy.',
      })),
    },
    context: { verdict: 'sufficient', reason: 'All required evidence is supplied.' },
    precedence: [] as {
      id: string;
      disposition: string;
      scope: string;
      reason: string;
      documents: { source: string; quote: string; lineStart: number; lineEnd: number }[];
      code: { file: string; revision: string; quote: string; lineStart: number; lineEnd: number }[];
    }[],
    documents: [
      { source: 's1', quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 },
    ],
    code: [{ file: 'f1', revision: 'after', quote: code.trim(), lineStart: 1, lineEnd: 1 }],
  };
}
function runAssessment(
  paths: Paths,
  fixture: { admitted: string; base: string },
  response: ReturnType<typeof assessment>,
) {
  writeFileSync(paths.candidate, JSON.stringify(response));
  return invoke(paths.root, [...command(fixture), '--codex', paths.binary]);
}

test('grounds supported and contradicted review claims without approving the entire implementation', async () => {
  await implementation((paths, fixture) => {
    for (const verdict of ['supported', 'contradicted']) {
      const response = assessment();
      response.verdict = verdict;
      const run = runAssessment(paths, fixture, response);
      expect(run.stderr).toBe('');
      expect(run.status).toBe(0);
      expect(JSON.parse(run.stdout)).toMatchObject({
        status: 'reviewed',
        accepted: false,
        implementationAccepted: false,
        scope: 'supplied-review-claim',
        assessment: { verdict },
        currentAtCompletion: true,
        report: { outcome: 'completed', usage: { totalTokens: 150 } },
      });
    }
  });
});

test('retains unresolved evidence and invocation usage without claiming success', async () => {
  await implementation((paths, fixture) => {
    const response = assessment();
    response.verdict = 'unresolved';
    response.context.verdict = 'insufficient';
    const run = runAssessment(paths, fixture, response);
    expect(run.status).toBe(1);
    expect(JSON.parse(run.stdout)).toMatchObject({
      status: 'failed',
      implementationAccepted: false,
      assessment: { verdict: 'unresolved' },
      report: { outcome: 'completed', usage: { totalTokens: 150 } },
    });
  });
});

test('rejects invented evidence, duplicate or incomplete coverage, and definitive verdicts with unresolved context', async () => {
  await implementation((paths, fixture) => {
    const mutations = [
      (value: ReturnType<typeof assessment>) => {
        value.documents[0]!.quote = 'Invented doctrine';
      },
      (value: ReturnType<typeof assessment>) => {
        value.documents[0]!.lineStart = 4;
      },
      (value: ReturnType<typeof assessment>) => {
        value.code[0]!.quote = 'export const cacheIsAuthority = true;';
      },
      (value: ReturnType<typeof assessment>) => {
        value.code[0]!.revision = 'before';
      },
      (value: ReturnType<typeof assessment>) => {
        value.coverage.sources.pop();
      },
      (value: ReturnType<typeof assessment>) => {
        value.coverage.sources[1]!.id = 's1';
      },
      (value: ReturnType<typeof assessment>) => {
        value.context.verdict = 'insufficient';
      },
      (value: ReturnType<typeof assessment>) => {
        value.coverage.complete = false;
      },
      (value: ReturnType<typeof assessment>) => {
        value.coverage.code[0]!.relevance = 'unresolved';
      },
      (value: ReturnType<typeof assessment>) => {
        value.coverage.code[0]!.relevance = 'irrelevant';
      },
    ];
    for (const mutate of mutations) {
      const response = assessment();
      mutate(response);
      const run = runAssessment(paths, fixture, response);
      expect(run.status).toBe(1);
      expect(JSON.parse(run.stdout)).toMatchObject({
        status: 'failed',
        assessment: null,
        report: {
          outcome: 'invalid-output',
          code: 'INVALID_GROUNDING_OUTPUT',
          usage: { totalTokens: 150 },
        },
      });
    }
  });
});

test('requires explicit evidence for every amendment and preserves its partial scope', async () => {
  await implementation((paths, fixture) => {
    const source = fixture.graph.sources[0]?.id;
    if (!source) throw new Error('Expected source');
    const prepared = invoke(paths.root, [
      'ground',
      'UnmatchedVocabularyForRetrieval',
      '--input',
      fixture.admitted,
      '--base',
      fixture.base,
      '--source',
      source,
      '--prepare',
    ]);
    expect(prepared.status).toBe(0);
    const context = JSON.parse(prepared.stdout);
    expect(context.selection.seeds).toEqual([]);
    expect(Object.keys(context.sourceBindings)).toHaveLength(2);
    expect(context.prompt).toContain('exception-to');
    const missing = runAssessment(paths, fixture, assessment());
    expect(missing.status).toBe(1);
    expect(JSON.parse(missing.stdout).issue).toContain('omitted');
    const response = assessment();
    response.precedence.push({
      id: 'r1',
      disposition: 'applies',
      scope: 'partial-claim',
      reason: 'The bounded exception applies here.',
      documents: ['s1', 's2'].map((source) => ({ ...response.documents[0]!, source })),
      code: response.code,
    });
    const reviewed = runAssessment(paths, fixture, response);
    expect(reviewed.status).toBe(0);
    expect(JSON.parse(reviewed.stdout).assessment.precedence[0].scope).toBe('partial-claim');
    response.precedence[0]!.scope = 'whole-claim';
    expect(runAssessment(paths, fixture, response).status).toBe(1);
    response.precedence[0]!.scope = 'partial-claim';
    response.precedence[0]!.documents.pop();
    expect(runAssessment(paths, fixture, response).status).toBe(1);
  }, 'exception-to');
});

test('rejects dirty, empty, unsupported and oversized changes before making a model call', async () => {
  await implementation((paths, fixture) => {
    const calls = readFileSync(paths.calls, 'utf8');
    writeFileSync(join(paths.root, 'cache.ts'), 'uncommitted');
    expect(JSON.parse(invoke(paths.root, command(fixture)).stderr).error.code).toBe(
      'GROUND_DIRTY_TREE',
    );
    writeFileSync(join(paths.root, 'cache.ts'), code);
    const empty = { ...fixture, base: paths.git(['rev-parse', 'HEAD']) };
    expect(JSON.parse(invoke(paths.root, command(empty)).stderr).error.code).toBe(
      'GROUND_EMPTY_CHANGESET',
    );
    symlinkSync('cache.ts', join(paths.root, 'alias.ts'));
    commit(paths);
    expect(JSON.parse(invoke(paths.root, command(fixture)).stderr).error.code).toBe(
      'GROUND_UNSUPPORTED_CODE',
    );
    rmSync(join(paths.root, 'alias.ts'));
    writeFileSync(join(paths.root, 'cache.ts'), 'x'.repeat(262144));
    commit(paths);
    expect(JSON.parse(invoke(paths.root, command(fixture)).stderr).error.code).toBe(
      'GROUND_INPUT_TOO_LARGE',
    );
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
  });
});

test('rejects candidate-only and stale documentary graphs before a model call', async () => {
  await implementation((paths, fixture) => {
    const calls = readFileSync(paths.calls, 'utf8');
    const candidateOnly = invoke(paths.root, command({ ...fixture, admitted: fixture.input }));
    expect(JSON.parse(candidateOnly.stderr).error.code).toBe('GROUND_REQUIRES_ADMISSION');
    writeFileSync(join(paths.root, 'second.md'), '# Replacement\nDifferent doctrine.\n');
    commit(paths);
    const stale = invoke(paths.root, command(fixture));
    expect(JSON.parse(stale.stderr).error.code).toBe('GROUND_REQUIRES_ADMISSION');
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
  });
});

test('does not reuse a successful assessment as current when the checkout changes during the invocation', async () => {
  await implementation(async (paths, fixture) => {
    writeFileSync(paths.candidate, JSON.stringify(assessment()));
    writeFileSync(paths.hold, 'hold');
    const previous = readFileSync(paths.calls, 'utf8');
    const child = Bun.spawn(
      [
        process.execPath,
        join(import.meta.dirname, 'cli.ts'),
        ...command(fixture),
        '--root',
        paths.root,
        '--codex',
        paths.binary,
      ],
      { stdout: 'pipe', stderr: 'pipe' },
    );
    try {
      const deadline = Date.now() + 5000;
      while (readFileSync(paths.calls, 'utf8') === previous && Date.now() < deadline)
        await Bun.sleep(10);
      expect(readFileSync(paths.calls, 'utf8')).not.toBe(previous);
      writeFileSync(join(paths.root, 'cache.ts'), 'changed during review\n');
      rmSync(paths.hold);
      const result = JSON.parse(await new Response(child.stdout).text());
      expect(await child.exited).toBe(1);
      expect(result).toMatchObject({
        status: 'failed',
        currentAtCompletion: false,
        assessment: { verdict: 'supported' },
        report: { outcome: 'completed' },
      });
    } finally {
      child.kill();
      writeFileSync(join(paths.root, 'cache.ts'), code);
    }
  });
});

test('rechecks retained grounding without a model call and invalidates changed code or tampered provenance', async () => {
  await implementation((paths, fixture) => {
    const assessed = runAssessment(paths, fixture, assessment());
    expect(assessed.status).toBe(0);
    const saved = join(dirname(paths.store), 'grounding.json');
    writeFileSync(saved, assessed.stdout);
    const calls = readFileSync(paths.calls, 'utf8');
    const args = ['ground', '--check', saved, '--input', fixture.admitted];
    const checked = invoke(paths.root, args);
    expect(checked.stderr).toBe('');
    expect(checked.status).toBe(0);
    expect(JSON.parse(checked.stdout)).toMatchObject({
      operation: 'check',
      status: 'reviewed',
      implementationAccepted: false,
    });
    const forged = JSON.parse(assessed.stdout);
    forged.contract.promptHash = '0'.repeat(64);
    writeFileSync(saved, JSON.stringify(forged));
    expect(invoke(paths.root, args).status).toBe(1);
    writeFileSync(saved, assessed.stdout);
    writeFileSync(join(paths.root, 'cache.ts'), 'export const cacheIsAuthority = true;\n');
    commit(paths);
    const stale = invoke(paths.root, args);
    expect(stale.status).toBe(1);
    expect(JSON.parse(stale.stderr).error.code).toBe('GROUND_RESULT_MISMATCH');
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
  });
});
