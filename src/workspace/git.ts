import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { devNull } from 'node:os';
import { HivexError } from '../errors.ts';

const MAX_BUFFER = 32 * 1024 * 1024;

function environment() {
  const inherited = Object.fromEntries(
    Object.entries(process.env).filter(([key]) => !key.startsWith('GIT_')),
  );
  return {
    ...inherited,
    GIT_CONFIG_GLOBAL: devNull,
    GIT_CONFIG_NOSYSTEM: '1',
    GIT_NO_LAZY_FETCH: '1',
    GIT_NO_REPLACE_OBJECTS: '1',
    GIT_TERMINAL_PROMPT: '0',
    GIT_OPTIONAL_LOCKS: '0',
  };
}
export type GitFile = { path: string; oid: string; mode: string };

export function git(root: string, args: string[], input?: string): Buffer {
  try {
    return execFileSync('git', args, {
      cwd: resolve(root),
      input,
      maxBuffer: MAX_BUFFER,
      timeout: 30_000,
      env: environment(),
      stdio: ['pipe', 'pipe', 'pipe'],
    });
  } catch {
    throw new HivexError('GIT_READ_FAILED', 'Cannot read the requested local Git snapshot');
  }
}

export function resolveCommit(root: string, ref: string): string {
  const commit = git(root, ['rev-parse', '--verify', '--end-of-options', `${ref}^{commit}`])
    .toString('utf8')
    .trim();
  if (!/^(?:[a-f0-9]{40}|[a-f0-9]{64})$/.test(commit))
    throw new HivexError('INVALID_REVISION', 'Git did not resolve a complete commit ID');
  return commit;
}

export function trackedFiles(root: string, commit: string): GitFile[] {
  return git(root, ['ls-tree', '-rz', '--full-tree', commit])
    .toString('utf8')
    .split('\0')
    .filter(Boolean)
    .map((record) => {
      const tab = record.indexOf('\t');
      const [mode, type, oid] = record.slice(0, tab).split(' ');
      if (tab < 0 || !mode || !oid || (type !== 'blob' && type !== 'commit'))
        throw new HivexError('INVALID_TREE', 'Unsupported Git tree record');
      return { path: record.slice(tab + 1), mode, oid };
    });
}

export function blobs(root: string, files: GitFile[]): Map<string, string> {
  if (!files.length) return new Map();
  const response = git(
    root,
    ['cat-file', '--batch'],
    files.map((file) => file.oid).join('\n') + '\n',
  );
  const result = new Map<string, string>();
  let cursor = 0;
  for (const file of files) {
    const newline = response.indexOf(10, cursor);
    const header = response.subarray(cursor, newline).toString('utf8').split(' ');
    const length = Number(header[2]);
    if (
      newline < 0 ||
      header[0] !== file.oid ||
      header[1] !== 'blob' ||
      !Number.isSafeInteger(length) ||
      length < 0 ||
      length > 2 * 1024 * 1024
    )
      throw new HivexError('INVALID_BLOB', `Invalid or oversized Markdown blob: ${file.path}`);
    cursor = newline + 1;
    if (cursor + length >= response.length || response[cursor + length] !== 10)
      throw new HivexError('INVALID_BLOB', 'Incomplete Git batch response');
    try {
      result.set(
        file.path,
        new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(
          response.subarray(cursor, cursor + length),
        ),
      );
    } catch {
      throw new HivexError('INVALID_ENCODING', `Source is not UTF-8: ${file.path}`);
    }
    cursor += length + 1;
  }
  return result;
}
