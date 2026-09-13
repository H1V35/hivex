import { lstatSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { HivexError } from './errors.ts';

interface TemplateFile {
  bytes: Buffer;
  path: string;
}

interface FileOperation {
  absolutePath: string;
  bytes: Buffer;
  path: string;
  state: 'created' | 'preserved' | 'updated';
}

interface InitReport {
  command: 'init';
  created: string[];
  modelCalls: 0;
  preserved: string[];
  updated: string[];
}

const ignoreRules = ['!/.hivex/', '/.hivex/*', '!/.hivex/graph.json'];
const assetsPath = path.join(import.meta.dirname, '../skills/hivex/assets/project');
const assetsUnavailableCode = 'INIT_ASSETS_UNAVAILABLE';
const assetsInvalidCode = 'INIT_ASSETS_INVALID';

const fail = function fail(code: string, message: string): never {
  throw new HivexError({ code, message });
};

const safeRelativePath = function safeRelativePath(relativePath: string) {
  const normalized = relativePath.replaceAll('\\', '/');
  const parts = normalized.split('/');
  const hasInvalidPath = [
    !relativePath,
    path.isAbsolute(relativePath),
    normalized.startsWith('/'),
    normalized.includes('\u{0}'),
    relativePath.includes('\\'),
  ].includes(true);
  if (hasInvalidPath) {
    return fail('INVALID_DESTINATION', 'Initialization paths must be relative project files');
  }
  if (parts.some((part) => ['', '.', '..'].includes(part))) {
    return fail('INVALID_DESTINATION', 'Initialization paths must be relative project files');
  }
  return normalized;
};

const errorMessage = function errorMessage(error: unknown) {
  return Error.isError(error) ? error.message : 'unknown error';
};

const templateEntries = function templateEntries(directory: string) {
  try {
    return readdirSync(directory, { withFileTypes: true }).toSorted((left, right) =>
      left.name.localeCompare(right.name)
    );
  } catch (error) {
    return fail(assetsUnavailableCode, `Unable to read project templates: ${errorMessage(error)}`);
  }
};

const readTemplateFile = function readTemplateFile(
  absolutePath: string,
  relativePath: string
): TemplateFile {
  try {
    return { bytes: readFileSync(absolutePath), path: relativePath };
  } catch (error) {
    return fail(
      assetsUnavailableCode,
      `Unable to read project template ${relativePath}: ${errorMessage(error)}`
    );
  }
};

const readTemplateFiles = function readTemplateFiles(
  directory: string,
  relativeDirectory = ''
): TemplateFile[] {
  const files: TemplateFile[] = [];
  const visit = function visit(current: string, currentRelativeDirectory: string) {
    for (const entry of templateEntries(current)) {
      const relativePath = safeRelativePath(
        currentRelativeDirectory ? `${currentRelativeDirectory}/${entry.name}` : entry.name
      );
      const absolutePath = path.join(current, entry.name);
      if (entry.isSymbolicLink()) {
        fail(assetsInvalidCode, `Project template must not be a symlink: ${relativePath}`);
      } else if (entry.isDirectory()) {
        visit(absolutePath, relativePath);
      } else if (entry.isFile()) {
        files.push(readTemplateFile(absolutePath, relativePath));
      } else {
        fail(assetsInvalidCode, `Project template is not a regular file: ${relativePath}`);
      }
    }
  };
  visit(directory, relativeDirectory);
  return files;
};

const projectRoot = function projectRoot(requested: string) {
  if (!requested.trim()) {
    return fail('INVALID_ROOT', 'Project root must be a non-empty path');
  }
  const root = path.resolve(requested);
  let stat;
  try {
    stat = lstatSync(root);
  } catch (error) {
    return fail('INVALID_ROOT', `Project root is not readable: ${errorMessage(error)}`);
  }
  if (stat.isSymbolicLink()) {
    return fail('INVALID_ROOT', 'Project root must not be a symlink');
  }
  if (!stat.isDirectory()) {
    return fail('INVALID_ROOT', 'Project root must be a directory');
  }
  return root;
};

const destination = function destination(root: string, relativePath: string) {
  const absolutePath = path.resolve(root, relativePath);
  const relative = path.relative(root, absolutePath);
  if (
    !relative ||
    relative === '..' ||
    relative.startsWith(`..${path.sep}`) ||
    path.isAbsolute(relative)
  ) {
    return fail(
      'INVALID_DESTINATION',
      `Initialization path escapes the project root: ${relativePath}`
    );
  }

  let current = root;
  const parts = relative.split(path.sep);
  for (const [index, part] of parts.entries()) {
    current = path.join(current, part);
    let stat;
    try {
      stat = lstatSync(current, { throwIfNoEntry: false });
    } catch (error) {
      return fail(
        'INVALID_DESTINATION',
        `Unable to inspect initialization path ${relativePath}: ${errorMessage(error)}`
      );
    }
    if (stat === undefined) {
      return { absolutePath, exists: false };
    }
    if (stat.isSymbolicLink()) {
      return fail(
        'INVALID_DESTINATION',
        `Initialization path must not use symlinks: ${relativePath}`
      );
    }
    const isFinal = index === parts.length - 1;
    if ((!isFinal && !stat.isDirectory()) || (isFinal && !stat.isFile())) {
      return fail(
        'INVALID_DESTINATION',
        `Initialization path is not a regular file: ${relativePath}`
      );
    }
  }
  return { absolutePath, exists: true };
};

const validateNestedIgnore = function validateNestedIgnore(root: string) {
  const relativePath = '.hivex/.gitignore';
  const target = destination(root, relativePath);
  if (!target.exists) {
    return;
  }
  let text: string;
  try {
    text = readFileSync(target.absolutePath, 'utf-8');
  } catch (error) {
    throw new HivexError({
      code: 'INIT_READ_FAILED',
      message: `Unable to read ${relativePath}: ${errorMessage(error)}`,
    });
  }
  if (text.split(/\r?\n/u).some((line) => line.trim() !== '' && !line.startsWith('#'))) {
    fail(
      'INIT_IGNORE_CONFLICT',
      '.hivex/.gitignore contains rules that can override snapshot visibility or local state privacy.'
    );
  }
};

const hasFinalIgnoreRules = function hasFinalIgnoreRules(text: string) {
  const lines = text.split(/\r?\n/u);
  while (lines.at(-1) === '') {
    lines.pop();
  }
  const start = lines.length - ignoreRules.length;
  return start >= 0 && ignoreRules.every((rule, index) => lines[start + index] === rule);
};

const ignoreUpdate = function ignoreUpdate(existing: Buffer | null) {
  const block = `${ignoreRules.join('\n')}\n`;
  if (existing === null) {
    return Buffer.from(block);
  }
  const text = existing.toString('utf-8');
  if (hasFinalIgnoreRules(text)) {
    return null;
  }
  const separator = text.length === 0 || text.endsWith('\n') ? '' : '\n';
  return Buffer.concat([existing, Buffer.from(`${separator}${block}`)]);
};

const templateOperations = function templateOperations(root: string, templates: TemplateFile[]) {
  return templates.map(({ bytes, path: relativePath }) => {
    const target = destination(root, relativePath);
    return {
      absolutePath: target.absolutePath,
      bytes,
      path: relativePath,
      state: target.exists ? 'preserved' : 'created',
    } satisfies FileOperation;
  });
};

const ignoreOperation = function ignoreOperation(root: string) {
  const relativePath = '.gitignore';
  const target = destination(root, relativePath);
  let existing: Buffer | null = null;
  if (target.exists) {
    try {
      existing = readFileSync(target.absolutePath);
    } catch (error) {
      return fail('INIT_READ_FAILED', `Unable to read ${relativePath}: ${errorMessage(error)}`);
    }
  }
  const bytes = ignoreUpdate(existing);
  let state: FileOperation['state'] = 'created';
  if (target.exists) {
    state = bytes === null ? 'preserved' : 'updated';
  }
  return {
    absolutePath: target.absolutePath,
    bytes: bytes ?? existing ?? Buffer.alloc(0),
    path: relativePath,
    state,
  } satisfies FileOperation;
};

const writeOperations = function writeOperations(operations: FileOperation[]) {
  for (const operation of operations) {
    if (operation.state === 'preserved') {
      continue;
    }
    mkdirSync(path.dirname(operation.absolutePath), { recursive: true });
    if (operation.state === 'created') {
      writeFileSync(operation.absolutePath, operation.bytes, { flag: 'wx', mode: 0o644 });
    } else {
      writeFileSync(operation.absolutePath, operation.bytes);
    }
  }
};

const report = function report(operations: FileOperation[]): InitReport {
  const paths = function paths(state: FileOperation['state']) {
    return operations
      .filter((operation) => operation.state === state)
      .map((operation) => operation.path)
      .toSorted((left, right) => left.localeCompare(right));
  };
  return {
    command: 'init',
    created: paths('created'),
    modelCalls: 0,
    preserved: paths('preserved'),
    updated: paths('updated'),
  };
};

const parseInitArguments = function parseInitArguments(argumentsList: string[]) {
  try {
    return parseArgs({
      allowPositionals: true,
      args: argumentsList,
      options: { root: { type: 'string' } },
      strict: true,
    });
  } catch (error) {
    return fail(
      'INVALID_ARGUMENT',
      Error.isError(error) ? error.message : 'Invalid init arguments'
    );
  }
};

export const projectInitializationCommand = function projectInitializationCommand(
  argumentsList: string[]
) {
  const parsed = parseInitArguments(argumentsList);
  if (parsed.positionals.length !== 1 || parsed.positionals[0] !== 'init') {
    return fail('INVALID_ARGUMENT', 'Use init [--root <project>]');
  }
  const root = projectRoot(parsed.values.root ?? process.cwd());
  validateNestedIgnore(root);
  const templates = readTemplateFiles(assetsPath);
  if (!templates.length) {
    return fail('INIT_ASSETS_UNAVAILABLE', 'No project templates are available');
  }
  const operations = [...templateOperations(root, templates), ignoreOperation(root)];
  writeOperations(operations);
  return report(operations);
};
