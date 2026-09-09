import { expect, test } from 'bun:test';
import { readFileSync, writeFileSync, rmSync, symlinkSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { hash } from './sources/markdown.ts';
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

type PromptVersion = {
  path?: string;
  lines?: [number, string][];
  text?: string;
};
type PromptPacket = {
  code: { path: string; before: PromptVersion | null; after: PromptVersion | null }[];
  contextFiles?: PromptVersion[];
};

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

test('numbers complete code versions and context without changing their bytes', async () => {
  await projectWithReviews((paths, fixture) => {
    const beforeText = 'const version = "antes 🧭";\r\n\r\n';
    const afterText = 'const version = "después λ";\r\n\r\n';
    const contextText = 'const context = "雪";\r\n\r\n';
    expect(compare(paths, fixture).status).toBe(0);
    const admittedResult = admit(paths, fixture);
    expect(admittedResult.status).toBe(0);
    const admitted = retained(paths, admittedResult.stdout);
    writeFileSync(join(paths.root, 'cache.ts'), beforeText);
    commit(paths);
    const base = paths.git(['rev-parse', 'HEAD']);
    paths.git(['switch', '-qc', 'numbered-context', base]);
    writeFileSync(join(paths.root, 'prototype.ts'), contextText);
    writeFileSync(join(paths.root, 'empty.ts'), '');
    commit(paths);
    paths.git(['switch', '-q', 'main']);
    writeFileSync(join(paths.root, 'cache.ts'), afterText);
    commit(paths);

    const prepared = invoke(paths.root, [
      ...command({ admitted, base }),
      '--context-file',
      'numbered-context:prototype.ts',
      '--context-file',
      'numbered-context:empty.ts',
      '--prepare',
    ]);
    expect(prepared.status).toBe(0);
    const result = JSON.parse(prepared.stdout) as { prompt: string };
    const packet = JSON.parse(
      result.prompt.slice(result.prompt.indexOf('\n\n') + 2),
    ) as PromptPacket;
    const file = packet.code.find((entry: { path: string }) => entry.path === 'cache.ts');
    if (!file?.before?.lines || !file.after?.lines) throw new Error('Expected code versions');
    const context = packet.contextFiles?.find((entry) => entry.path === 'prototype.ts');
    const emptyContext = packet.contextFiles?.find((entry) => entry.path === 'empty.ts');
    if (!context?.lines) throw new Error('Expected contextual code');
    if (!emptyContext?.lines) throw new Error('Expected empty contextual code');
    const reconstruct = (lines: [number, string][]) => lines.map(([, text]) => text).join('\n');

    expect(file.before.text).toBeUndefined();
    expect(file.after.text).toBeUndefined();
    expect(file.before.lines).toEqual([
      [1, 'const version = "antes 🧭";\r'],
      [2, '\r'],
      [3, ''],
    ]);
    expect(file.after.lines).toEqual([
      [1, 'const version = "después λ";\r'],
      [2, '\r'],
      [3, ''],
    ]);
    expect(context.text).toBeUndefined();
    expect(context.lines).toEqual([
      [1, 'const context = "雪";\r'],
      [2, '\r'],
      [3, ''],
    ]);
    expect(reconstruct(file.before.lines)).toBe(beforeText);
    expect(reconstruct(file.after.lines)).toBe(afterText);
    expect(reconstruct(context.lines)).toBe(contextText);
    expect(emptyContext.lines).toEqual([[1, '']]);
    expect(reconstruct(emptyContext.lines)).toBe('');
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
  source?: string,
) {
  await projectWithReviews(
    async (paths, fixture) => {
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
    },
    2,
    false,
    source,
  );
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
    writeFileSync(saved, checked.stdout);
    expect(invoke(paths.root, args).status).toBe(0);
    const forged = JSON.parse(assessed.stdout);
    forged.contract.promptHash = '0'.repeat(64);
    writeFileSync(saved, JSON.stringify(forged));
    expect(invoke(paths.root, args).status).toBe(1);
    for (const field of ['usage', 'threadId', 'turnId']) {
      const changed = JSON.parse(assessed.stdout);
      if (field === 'usage') changed.report.usage.totalTokens = 0;
      else changed.report[field] = 'substituted-id';
      writeFileSync(saved, JSON.stringify(changed));
      expect(invoke(paths.root, args).status).toBe(1);
    }
    writeFileSync(saved, assessed.stdout);
    writeFileSync(join(paths.root, 'cache.ts'), 'export const cacheIsAuthority = true;\n');
    commit(paths);
    const stale = invoke(paths.root, args);
    expect(stale.status).toBe(1);
    expect(JSON.parse(stale.stderr).error.code).toBe('GROUND_RESULT_MISMATCH');
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
  });
});

test('checks current and historical prompt hashes against complete original inputs', async () => {
  await implementation((paths, fixture) => {
    writeFileSync(paths.scenario, 'invalid-json');
    const args = [...command(fixture), '--codex', paths.binary];
    const prepared = invoke(paths.root, [...command(fixture), '--prepare']);
    const run = invoke(paths.root, args);
    expect(prepared.status).toBe(0);
    expect(run.status).toBe(1);
    const rawPacket = (prompt: string) => {
      const separator = prompt.indexOf('\n\n');
      const packet = JSON.parse(prompt.slice(separator + 2)) as PromptPacket;
      for (const file of packet.code) {
        for (const revision of ['before', 'after'] as const) {
          const version = file[revision];
          if (!version) continue;
          if (!version.lines) throw new Error('Expected numbered code');
          version.text = version.lines.map(([, text]) => text).join('\n');
          delete version.lines;
        }
      }
      for (const file of packet.contextFiles ?? []) {
        if (!file.lines) throw new Error('Expected numbered context');
        file.text = file.lines.map(([, text]) => text).join('\n');
        delete file.lines;
      }
      return { packet, guidance: prompt.slice(0, separator) };
    };
    const preparedOutput = JSON.parse(prepared.stdout);
    const original = rawPacket(preparedOutput.prompt);
    const rawInput = '\n\n' + JSON.stringify(original.packet);
    const legacySuffix =
      '\nEvery precedence entry must cite each distinct endpoint source of that relation, including inapplicable relations. A local relation with both endpoints in one source needs that one source; a cross-source relation needs quotes from both.';
    const oldHashes = [
      hash(original.guidance + rawInput),
      hash(original.guidance.slice(0, -legacySuffix.length) + rawInput),
    ];
    const receipt = JSON.parse(run.stdout);
    const saved = join(dirname(paths.store), 'historical-failed.json');
    const check = ['ground', '--check', saved, '--input', fixture.admitted];
    const calls = readFileSync(paths.calls, 'utf8');
    for (const promptHash of oldHashes) {
      const historical = {
        ...receipt,
        contract: { ...receipt.contract, promptHash },
      };
      const bytes = JSON.stringify(historical);
      writeFileSync(saved, bytes);
      const checked = invoke(paths.root, check);
      expect(checked.stderr).toBe('');
      expect(checked.status).toBe(1);
      expect(JSON.parse(checked.stdout)).toMatchObject({
        checked: true,
        operation: 'check',
        status: 'failed',
        assessment: null,
      });
      expect(readFileSync(saved, 'utf8')).toBe(bytes);
    }
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);

    paths.git(['switch', '-qc', 'historical-context', 'HEAD']);
    writeFileSync(join(paths.root, 'prototype.ts'), 'const context = "雪";\r\n\r\n');
    commit(paths);
    paths.git(['switch', '-q', 'main']);
    const contextArgs = [
      ...command(fixture),
      '--context-file',
      'historical-context:prototype.ts',
      '--codex',
      paths.binary,
    ];
    const contextPrepared = invoke(paths.root, [
      ...command(fixture),
      '--context-file',
      'historical-context:prototype.ts',
      '--prepare',
    ]);
    const contextRun = invoke(paths.root, contextArgs);
    expect(contextPrepared.status).toBe(0);
    expect(contextRun.status).toBe(1);
    const contextOriginal = rawPacket(JSON.parse(contextPrepared.stdout).prompt);
    const contextReceipt = JSON.parse(contextRun.stdout);
    const contextHistorical = {
      ...contextReceipt,
      contract: {
        ...contextReceipt.contract,
        promptHash: hash(
          contextOriginal.guidance + '\n\n' + JSON.stringify(contextOriginal.packet),
        ),
      },
    };
    const contextBytes = JSON.stringify(contextHistorical);
    writeFileSync(saved, contextBytes);
    const contextCheck = invoke(paths.root, [
      'ground',
      '--check',
      saved,
      '--input',
      fixture.admitted,
    ]);
    expect(contextCheck.stderr).toBe('');
    expect(contextCheck.status).toBe(1);
    expect(JSON.parse(contextCheck.stdout)).toMatchObject({
      checked: true,
      operation: 'check',
      status: 'failed',
    });
    expect(readFileSync(saved, 'utf8')).toBe(contextBytes);

    writeFileSync(
      saved,
      JSON.stringify({
        ...contextReceipt,
        contract: { ...contextReceipt.contract, promptHash: '0'.repeat(64) },
      }),
    );
    const arbitrary = invoke(paths.root, ['ground', '--check', saved, '--input', fixture.admitted]);
    expect(arbitrary.status).toBe(1);
    expect(JSON.parse(arbitrary.stderr).error.code).toBe('GROUND_RESULT_MISMATCH');
  });
});

test('adds exact historical code context without changing the reviewed implementation', async () => {
  await implementation((paths, fixture) => {
    const head = paths.git(['rev-parse', 'HEAD']);
    paths.git(['switch', '-qc', 'approved-prototype', fixture.base]);
    writeFileSync(
      join(paths.root, 'prototype.ts'),
      'export const approvedCacheLifetimeMinutes = 1440;\n',
    );
    commit(paths);
    const prototype = paths.git(['rev-parse', 'HEAD']);
    paths.git(['switch', '-q', 'main']);
    const before = readFileSync(paths.calls, 'utf8');
    const prepared = invoke(paths.root, [
      ...command(fixture),
      '--context-file',
      'approved-prototype:prototype.ts',
      '--prepare',
    ]);
    expect(prepared.stderr).toBe('');
    expect(prepared.status).toBe(0);
    const result = JSON.parse(prepared.stdout);
    expect(result.codeSnapshot.head).toBe(head);
    expect(result.contextFiles).toEqual([
      expect.objectContaining({ id: 'e1', path: 'prototype.ts', commit: prototype }),
    ]);
    expect(result.prompt).toContain('export const approvedCacheLifetimeMinutes = 1440;');
    expect(result.files).toHaveLength(1);
    const duplicate = invoke(paths.root, [
      ...command(fixture),
      '--context-file',
      'approved-prototype:prototype.ts',
      '--context-file',
      'approved-prototype:prototype.ts',
      '--prepare',
    ]);
    expect(duplicate.status).toBe(0);
    expect(JSON.parse(duplicate.stdout).contextFiles).toHaveLength(1);
    const alias = invoke(paths.root, [
      ...command(fixture),
      '--context-file',
      'approved-prototype:prototype.ts',
      '--context-file',
      `${prototype}:prototype.ts`,
      '--prepare',
    ]);
    expect(alias.status).toBe(1);
    expect(JSON.parse(alias.stderr).error.code).toBe('GROUND_CONTEXT_CODE_DUPLICATE');
    expect(readFileSync(paths.calls, 'utf8')).toBe(before);
    expect(paths.git(['rev-parse', 'HEAD'])).toBe(head);
  });
});

test('checks contextual code citations and invalidates a moved reference without a model call', async () => {
  await implementation((paths, fixture) => {
    paths.git(['switch', '-qc', 'approved-prototype', fixture.base]);
    const text = 'export const approvedCacheLifetimeMinutes = 1440;';
    writeFileSync(join(paths.root, 'prototype.ts'), text + '\n');
    commit(paths);
    paths.git(['switch', '-q', 'main']);
    const response = assessment();
    response.coverage.code.push({
      id: 'e1',
      relevance: 'relevant',
      reason: 'Provides the approved reference.',
    });
    response.code.push({ file: 'e1', revision: 'context', quote: text, lineStart: 1, lineEnd: 1 });
    writeFileSync(paths.candidate, JSON.stringify(response));
    const args = [
      ...command(fixture),
      '--context-file',
      'approved-prototype:prototype.ts',
      '--codex',
      paths.binary,
    ];
    const result = invoke(paths.root, args);
    expect(result.stderr).toBe('');
    expect(result.status).toBe(0);
    const saved = join(dirname(paths.store), 'context-grounding.json');
    writeFileSync(saved, result.stdout);
    const check = ['ground', '--check', saved, '--input', fixture.admitted];
    expect(invoke(paths.root, check).status).toBe(0);
    response.code[1]!.revision = 'after';
    writeFileSync(paths.candidate, JSON.stringify(response));
    expect(invoke(paths.root, args).status).toBe(1);
    const calls = readFileSync(paths.calls, 'utf8');
    paths.git(['branch', '-f', 'approved-prototype', 'HEAD']);
    expect(invoke(paths.root, check).status).toBe(1);
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
  });
});

test('rejects missing, untracked and excessive contextual selections before a model call', async () => {
  await implementation((paths, fixture) => {
    const calls = readFileSync(paths.calls, 'utf8');
    for (const request of [
      'HEAD:',
      'HEAD:missing.ts',
      'HEAD:../outside.ts',
      'missing-ref:cache.ts',
    ]) {
      expect(invoke(paths.root, [...command(fixture), '--context-file', request]).status).toBe(1);
    }
    const excessive = Array.from({ length: 17 }, (_, index) => [
      '--context-file',
      `HEAD:file-${index}.ts`,
    ]).flat();
    expect(invoke(paths.root, [...command(fixture), ...excessive]).status).toBe(1);
    expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
  });
});

test('preserves malformed model output for diagnosis and checks its integrity', async () => {
  await implementation((paths, fixture) => {
    writeFileSync(paths.scenario, 'invalid-json');
    const result = invoke(paths.root, [...command(fixture), '--codex', paths.binary]);
    expect(result.status).toBe(1);
    const value = JSON.parse(result.stdout);
    expect(value.rejectedOutput.text).toBe('{broken');
    expect(value.assessment).toBeNull();
    const saved = join(dirname(paths.store), 'rejected-grounding.json');
    writeFileSync(saved, result.stdout);
    const checked = invoke(paths.root, ['ground', '--check', saved, '--input', fixture.admitted]);
    expect(checked.stderr).toBe('');
    expect(JSON.parse(checked.stdout)).toMatchObject({
      status: 'failed',
      checked: true,
      rejectedOutput: { text: '{broken' },
    });
    value.rejectedOutput.text = 'replaced';
    writeFileSync(saved, JSON.stringify(value));
    expect(
      invoke(paths.root, ['ground', '--check', saved, '--input', fixture.admitted]).status,
    ).toBe(1);
  });
});

test('rejects code citations outside exact line ranges without repairing them', async () => {
  await implementation((paths, fixture) => {
    for (const [lineStart, lineEnd] of [
      [2, 2],
      [1, 3],
    ] as [number, number][]) {
      const response = assessment();
      response.code[0]!.lineStart = lineStart;
      response.code[0]!.lineEnd = lineEnd;
      const result = runAssessment(paths, fixture, response);
      expect(result.status).toBe(1);
      expect(JSON.parse(result.stdout)).toMatchObject({
        status: 'failed',
        assessment: null,
        issue: 'Code citations must match the supplied file version and range',
        rejectedOutput: { text: JSON.stringify(response) },
      });
    }
  });
});

test('checks joined documentary evidence without promoting an earlier failed receipt', async () => {
  const source =
    '# Cache\n\nNever treat a cache as authority.\nKeep records, unless exempt,\n  for 30 days.\n';
  await implementation(
    (paths, fixture) => {
      const before = readFileSync(paths.calls, 'utf8');
      const prepared = invoke(paths.root, [...command(fixture), '--prepare']);
      expect(prepared.status).toBe(0);
      const { prompt }: { prompt: string } = JSON.parse(prepared.stdout);
      expect(hash(prompt.split('\n\n')[0]!)).toBe(
        '72b6a7feee37062b5077a92b87478486b83c604c4c0353b0cfcf5f36ba4093ca',
      );
      expect(readFileSync(paths.calls, 'utf8')).toBe(before);
      const response = assessment();
      response.documents = [
        {
          source: 's1',
          quote: 'Keep records, unless exempt,\nfor 30 days.',
          lineStart: 4,
          lineEnd: 5,
        },
      ];
      const reviewed = runAssessment(paths, fixture, response);
      expect(reviewed.status).toBe(0);
      const result = JSON.parse(reviewed.stdout);
      expect(result.assessment.documents).toEqual(response.documents);
      const saved = join(dirname(paths.store), 'joined-grounding.json');
      const check = ['ground', '--check', saved, '--input', fixture.admitted];
      writeFileSync(saved, reviewed.stdout);
      const calls = readFileSync(paths.calls, 'utf8');
      expect(invoke(paths.root, check).status).toBe(0);
      // Synthetic receipt from the earlier literal-only rejection of this same raw output.
      const historical = {
        ...result,
        status: 'failed',
        assessment: null,
        report: { ...result.report, outcome: 'invalid-output', code: 'INVALID_GROUNDING_OUTPUT' },
        rejectedOutput: { text: JSON.stringify(response), hash: hash(JSON.stringify(response)) },
      };
      delete historical.assessmentHash;
      delete historical.currentAtCompletion;
      historical.invocationHash = hash(JSON.stringify(historical.report));
      const original = JSON.stringify(historical);
      writeFileSync(saved, original);
      const checked = invoke(paths.root, check);
      expect(checked.stderr).toBe('');
      expect(checked.status).toBe(1);
      expect(JSON.parse(checked.stdout)).toMatchObject({ ...historical, checked: true });
      expect(readFileSync(saved, 'utf8')).toBe(original);
      expect(readFileSync(paths.calls, 'utf8')).toBe(calls);
      response.documents[0]!.quote = 'Keep records for 30 days.';
      expect(runAssessment(paths, fixture, response).status).toBe(1);
      writeFileSync(join(paths.root, 'cache.ts'), code + 'const ttl =\n  30;\n');
      commit(paths);
      const changed = assessment();
      changed.code = [
        { file: 'f1', revision: 'after', quote: 'const ttl = 30;', lineStart: 2, lineEnd: 3 },
      ];
      expect(runAssessment(paths, fixture, changed).status).toBe(1);
    },
    'equivalent',
    source,
  );
});

test('preserves literal Markdown and option-like review claims through execution and revalidation', async () => {
  await implementation((paths, fixture) => {
    writeFileSync(paths.candidate, JSON.stringify(assessment()));
    for (const text of [
      '- **Cache policy:** the implementation respects documentary authority.',
      '--check',
    ]) {
      const result = invoke(paths.root, [
        'ground',
        '--input',
        fixture.admitted,
        '--base',
        fixture.base,
        ...fixture.graph.sources.flatMap((source) => ['--source', source.id]),
        '--codex',
        paths.binary,
        '--',
        text,
      ]);
      expect(result.stderr).toBe('');
      expect(result.status).toBe(0);
      expect(JSON.parse(result.stdout).claim).toBe(text);
      const saved = join(dirname(paths.store), 'literal-grounding.json');
      writeFileSync(saved, result.stdout);
      const checked = invoke(paths.root, ['ground', '--check', saved, '--input', fixture.admitted]);
      expect(checked.stderr).toBe('');
      expect(checked.status).toBe(0);
      expect(JSON.parse(checked.stdout).claim).toBe(text);
    }
  });
});
