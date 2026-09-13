import { spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import nodePath from 'node:path';
import { expect, test } from 'bun:test';

const cli = nodePath.join(import.meta.dirname, 'cli.ts');
const git = Bun.which('git') ?? 'git';
const requiredFiles = [
  'AGENTS.md',
  'hivex.json',
  'docs/README.md',
  'docs/PRD.md',
  'docs/CONTEXT.md',
  'docs/adr/README.md',
  'docs/guidelines/engineering.md',
  'docs/guidelines/triage-labels.md',
  'docs/procedures/issue-tracker.md',
];

const temporaryProject = function temporaryProject(run: (root: string) => void) {
  const root = mkdtempSync(nodePath.join(tmpdir(), 'hivex-init-'));
  try {
    run(root);
  } finally {
    rmSync(root, { force: true, recursive: true });
  }
};

const invoke = function invoke(root: string, argumentsList: string[]) {
  return spawnSync(process.execPath, [cli, ...argumentsList, '--root', root], {
    encoding: 'utf-8',
    timeout: 10_000,
  });
};

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const value = function value(result: ReturnType<typeof invoke>): Record<string, unknown> {
  if (result.status !== 0 || !result.stdout) {
    throw new Error(result.stderr || 'Expected successful CLI JSON output');
  }
  const parsed: unknown = JSON.parse(result.stdout);
  if (!isRecord(parsed)) {
    throw new Error('Expected a JSON object');
  }
  return parsed;
};

test('init creates the project foundation through the public CLI', () => {
  temporaryProject((root) => {
    const result = invoke(root, ['init']);
    const response = value(result);

    expect(response).toMatchObject({ command: 'init', modelCalls: 0, updated: [] });
    expect(response.created).toEqual(expect.arrayContaining([...requiredFiles, '.gitignore']));
    for (const file of requiredFiles) {
      expect(existsSync(nodePath.join(root, file))).toBe(true);
    }
  });
});

test('init is repeatable and preserves existing project files and graph bytes', () => {
  temporaryProject((root) => {
    const configPath = nodePath.join(root, 'hivex.json');
    const contextPath = nodePath.join(root, 'docs/CONTEXT.md');
    const graphPath = nodePath.join(root, '.hivex/graph.json');
    const config = '{"include":["custom/**/*.md"]}\r\n';
    const context = '# Owner context\r\n';
    const graph = '{"graph":"owned"}\n';
    writeFileSync(configPath, config);
    mkdirSync(nodePath.dirname(contextPath), { recursive: true });
    writeFileSync(contextPath, context);
    mkdirSync(nodePath.dirname(graphPath), { recursive: true });
    writeFileSync(graphPath, graph);

    expect(value(invoke(root, ['init']))).toMatchObject({ command: 'init', modelCalls: 0 });
    const firstIgnore = readFileSync(nodePath.join(root, '.gitignore'));
    const repeated = value(invoke(root, ['init']));

    expect(repeated).toMatchObject({ command: 'init', created: [], modelCalls: 0, updated: [] });
    expect(repeated.preserved).toEqual(expect.arrayContaining([...requiredFiles, '.gitignore']));
    expect(readFileSync(configPath, 'utf-8')).toBe(config);
    expect(readFileSync(contextPath, 'utf-8')).toBe(context);
    expect(readFileSync(graphPath, 'utf-8')).toBe(graph);
    expect(readFileSync(nodePath.join(root, '.gitignore'))).toEqual(firstIgnore);
  });
});

test.each(['docs', '.hivex', '.hivex/.gitignore'])(
  'init rejects a symlinked %s before writing any template',
  (relativePath) => {
    temporaryProject((root) => {
      const outside = mkdtempSync(nodePath.join(tmpdir(), 'hivex-init-outside-'));
      try {
        const link = nodePath.join(root, relativePath);
        mkdirSync(nodePath.dirname(link), { recursive: true });
        symlinkSync(outside, link);
        const before = readdirSync(root, { recursive: true });
        const result = invoke(root, ['init']);

        expect(result.status).toBe(1);
        expect(JSON.parse(result.stderr)).toMatchObject({
          error: { code: 'INVALID_DESTINATION' },
        });
        expect(existsSync(nodePath.join(root, 'AGENTS.md'))).toBe(false);
        expect(readdirSync(root, { recursive: true })).toEqual(before);
        expect(readdirSync(outside)).toEqual([]);
      } finally {
        rmSync(outside, { force: true, recursive: true });
      }
    });
  }
);

test('init makes graph.json visible while ignoring local SQLite state', () => {
  temporaryProject((root) => {
    const ignorePath = nodePath.join(root, '.gitignore');
    const original = Buffer.from(
      '!/.hivex/\r\n/.hivex/*\r\n!/.hivex/graph.json\r\n.hivex/\r\n!/.hivex/knowledge.sqlite'
    );
    writeFileSync(ignorePath, original);
    expect(spawnSync(git, ['init', '-q'], { cwd: root }).status).toBe(0);

    const result = value(invoke(root, ['init']));
    expect(result).toMatchObject({ command: 'init', modelCalls: 0, updated: ['.gitignore'] });
    const corrected = readFileSync(ignorePath);
    expect(corrected.subarray(0, original.length)).toEqual(original);
    expect(
      spawnSync(git, ['check-ignore', '--no-index', '-q', '--', '.hivex/knowledge.sqlite'], {
        cwd: root,
      }).status
    ).toBe(0);
    expect(
      spawnSync(git, ['check-ignore', '--no-index', '-q', '--', '.hivex/graph.json'], {
        cwd: root,
      }).status
    ).toBe(1);
    expect(value(invoke(root, ['init']))).toMatchObject({ created: [], updated: [] });
    expect(readFileSync(ignorePath)).toEqual(corrected);
  });
});

test.each(['!knowledge.sqlite', '*'])(
  'init rejects nested ignore rule %s without changing any project file',
  (rule) => {
    temporaryProject((root) => {
      const files = {
        '.gitignore': Buffer.from('/.hivex/\r\n'),
        '.hivex/.gitignore': Buffer.from(`# Local rules\r\n${rule}\r\n`),
        '.hivex/graph.json': Buffer.from('{"owned":"graph"}\r\n'),
        '.hivex/knowledge.sqlite': Buffer.from([0, 255, 127, 1]),
        'AGENTS.md': Buffer.from('# Owner instructions\r\n'),
        'docs/CONTEXT.md': Buffer.from('# Existing context\n'),
        'hivex.json': Buffer.from('{"include":["**/*.md"]}\r\n'),
      };
      for (const [relativePath, bytes] of Object.entries(files)) {
        const file = nodePath.join(root, relativePath);
        mkdirSync(nodePath.dirname(file), { recursive: true });
        writeFileSync(file, bytes);
      }
      const before = readdirSync(root, { recursive: true });

      const result = invoke(root, ['init']);

      expect(result.status).toBe(1);
      expect(result.stdout).toBe('');
      expect(JSON.parse(result.stderr)).toMatchObject({
        error: { code: 'INIT_IGNORE_CONFLICT' },
      });
      expect(readdirSync(root, { recursive: true })).toEqual(before);
      for (const [relativePath, bytes] of Object.entries(files)) {
        expect(readFileSync(nodePath.join(root, relativePath))).toEqual(bytes);
      }
    });
  }
);

test.each(['', ' \t\r\n', '# Local state\r\n\r\n# No active rules\r\n'])(
  'init preserves an empty or comment-only nested ignore file (%j)',
  (text) => {
    temporaryProject((root) => {
      mkdirSync(nodePath.join(root, '.hivex'));
      const nestedIgnore = nodePath.join(root, '.hivex/.gitignore');
      writeFileSync(nestedIgnore, text);

      expect(value(invoke(root, ['init']))).toMatchObject({ command: 'init', modelCalls: 0 });
      expect(readFileSync(nestedIgnore)).toEqual(Buffer.from(text));
      expect(value(invoke(root, ['init']))).toMatchObject({ created: [], updated: [] });
    });
  }
);

test.each(['md', 'markdown', 'mdown'])(
  'init keeps .%s sources readable and classifies archived sources as history',
  (extension) => {
    temporaryProject((root) => {
      const sources = [
        { historical: false, path: `guide.${extension}` },
        { historical: true, path: `docs/archive/old.${extension}` },
        { historical: true, path: `packages/core/docs/archive/old.${extension}` },
      ];
      const text = '# Existing source\n\nKeep the original evidence.\n';
      for (const source of sources) {
        const file = nodePath.join(root, source.path);
        mkdirSync(nodePath.dirname(file), { recursive: true });
        writeFileSync(file, text);
      }
      const before = value(invoke(root, ['sources']));
      for (const source of sources) {
        expect(before.documents).toContainEqual(
          expect.objectContaining({ historical: false, path: source.path })
        );
        expect(value(invoke(root, ['read', source.path]))).toMatchObject({
          source: { historical: false, path: source.path },
          text,
        });
      }

      expect(value(invoke(root, ['init']))).toMatchObject({ command: 'init', modelCalls: 0 });

      const after = value(invoke(root, ['sources', '--limit', '100']));
      for (const source of sources) {
        expect(after.documents).toContainEqual(expect.objectContaining(source));
        expect(value(invoke(root, ['read', source.path]))).toMatchObject({ source, text });
      }
    });
  }
);
