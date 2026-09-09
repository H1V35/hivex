import { spawnSync } from 'node:child_process';
import { lstatSync, readFileSync, readlinkSync, realpathSync } from 'node:fs';
import { resolve } from 'node:path';
import { HivexError } from './errors.ts';
import { digest } from './knowledge-model.ts';
import { rawMarkdownLines, lineContent } from './markdown.ts';

type Version = { version: string; lines: Array<[number, string]> };
export type Implementation = {
  baseCommit: string;
  fingerprint: string;
  diff: string;
  files: Array<{ path: string; before: Version | null; after: Version | null }>;
  warnings: string[];
};
const maxBytes = 256 * 1024;
const protectedDirectories = new Set(['.git', '.hivex', 'node_modules', '.codex']);
const decoder = new TextDecoder('utf-8', { fatal: true });

function git(root: string, args: string[]) {
  const result = spawnSync('git', ['--literal-pathspecs', ...args], {
    cwd: root,
    maxBuffer: maxBytes + 1,
    timeout: 30000,
    env: { ...process.env, LC_ALL: 'C', GIT_OPTIONAL_LOCKS: '0' },
  });
  if (result.error || result.status !== 0) {
    const tooLarge = result.error && 'code' in result.error && result.error.code === 'ENOBUFS';
    throw new HivexError({
      code: tooLarge ? 'IMPLEMENTATION_TOO_LARGE' : 'GIT_COMMAND_FAILED',
      message: result.error?.message ?? result.stderr.toString('utf8').trim().slice(0, 1024),
    });
  }
  return result.stdout;
}
function text(root: string, args: string[]) {
  return decoder.decode(git(root, args));
}
function checkSize(bytes: number) {
  if (bytes > maxBytes)
    throw new HivexError({
      code: 'IMPLEMENTATION_TOO_LARGE',
      message: 'Implementation exceeds 256 KiB; split the change into coherent reviews.',
    });
}
function version(bytes: Buffer, label: string, warnings: string[]): Version | null {
  checkSize(bytes.byteLength);
  let content: string;
  try {
    content = decoder.decode(bytes);
    if (content.includes('\0')) throw new Error('binary');
  } catch {
    warnings.push(
      `Unsupported binary or invalid UTF-8 content: ${label} (${digest(bytes.toString('base64'))})`,
    );
    return null;
  }
  return {
    version: digest(content),
    lines: rawMarkdownLines(content).map((line, index) => [index + 1, lineContent(line)]),
  };
}
function beforeVersion(root: string, base: string, path: string, warnings: string[]) {
  const entry = text(root, ['ls-tree', '-z', base, '--', path])
    .split('\0')
    .find((row) => row.slice(row.indexOf('\t') + 1) === path);
  if (!entry) return null;
  const [mode, kind, object] = entry.split('\t')[0]!.split(' ');
  if (kind !== 'blob' || !mode?.startsWith('100')) {
    warnings.push(`Unsupported base file: ${path} (${mode} ${object})`);
    return null;
  }
  return version(git(root, ['cat-file', 'blob', object!]), `before ${path}`, warnings);
}
function afterVersion(root: string, path: string, warnings: string[]) {
  const absolute = resolve(root, path);
  let stat;
  try {
    stat = lstatSync(absolute);
  } catch (error) {
    if (error && typeof error === 'object' && 'code' in error && error.code === 'ENOENT')
      return null;
    throw error;
  }
  if (stat.isSymbolicLink()) {
    warnings.push(`Unsupported working symlink: ${path} (${digest(readlinkSync(absolute))})`);
    return null;
  }
  if (!stat.isFile() || !realpathSync(absolute).startsWith(root + '/')) {
    warnings.push(`Unsupported working file: ${path}`);
    return null;
  }
  checkSize(stat.size);
  return version(readFileSync(absolute), `after ${path}`, warnings);
}
export function captureImplementation(root: string, base: string): Implementation {
  const actualRoot = realpathSync(resolve(root));
  if (realpathSync(text(actualRoot, ['rev-parse', '--show-toplevel']).trim()) !== actualRoot)
    throw new HivexError({ code: 'INVALID_ROOT', message: 'Review from the Git project root.' });
  const baseCommit = text(actualRoot, [
    'rev-parse',
    '--verify',
    '--end-of-options',
    base + '^{commit}',
  ]).trim();
  const tracked = text(actualRoot, [
    'diff',
    '--no-ext-diff',
    '--no-textconv',
    '--no-renames',
    '--name-only',
    '-z',
    baseCommit,
    '--',
  ]).split('\0');
  const untracked = text(actualRoot, ['ls-files', '--others', '--exclude-standard', '-z']).split(
    '\0',
  );
  const paths = [...new Set([...tracked, ...untracked])]
    .filter((path) => path && !path.split('/').some((part) => protectedDirectories.has(part)))
    .sort();
  if (paths.length > 64)
    throw new HivexError({
      code: 'IMPLEMENTATION_TOO_LARGE',
      message: 'Implementation exceeds 64 files; split the change into coherent reviews.',
    });
  const warnings: string[] = [];
  const files = paths.map((path) => ({
    path,
    before: beforeVersion(actualRoot, baseCommit, path, warnings),
    after: afterVersion(actualRoot, path, warnings),
  }));
  let diff = '';
  if (paths.length)
    diff = text(actualRoot, [
      'diff',
      '--no-ext-diff',
      '--no-textconv',
      '--no-renames',
      '--no-color',
      '--src-prefix=a/',
      '--dst-prefix=b/',
      '--unified=3',
      baseCommit,
      '--',
      ...paths,
    ]);
  diff += paths
    .filter((path) => untracked.includes(path))
    .map((path) => `\nNew untracked file: ${JSON.stringify(path)}\n`)
    .join('');
  const packet = { baseCommit, diff, files, warnings };
  checkSize(Buffer.byteLength(JSON.stringify(packet)));
  return { ...packet, fingerprint: digest(JSON.stringify(packet)) };
}
