import { HivexError } from '../errors.ts';
import { hash } from '../sources/markdown.ts';
import { git, resolveCommit, trackedFiles, blobs, type GitFile } from '../workspace/git.ts';

export type CodeFile = {
  id: string;
  path: string;
  before: { oid: string; mode: string; text: string } | null;
  after: { oid: string; mode: string; text: string } | null;
};
export type CodeContext = {
  id: string;
  request: string;
  ref: string;
  commit: string;
  path: string;
  oid: string;
  mode: string;
  text: string;
};

export function readCodeContext(root: string, requests: string[]): CodeContext[] {
  const unique = [...new Set(requests)].sort();
  if (unique.length > 16)
    throw new HivexError({
      code: 'GROUND_CONTEXT_CODE_LIMIT',
      message: 'Select at most 16 complete contextual code files',
    });
  const trees = new Map<string, Map<string, GitFile>>();
  const files = unique.map((request) => {
    const separator = request.indexOf(':');
    if (separator <= 0 || separator === request.length - 1)
      throw new HivexError({
        code: 'INVALID_ARGUMENT',
        message: 'Context files use <revision>:<repository-path>',
      });
    const ref = request.slice(0, separator);
    const commit = resolveCommit(root, ref);
    const path = request.slice(separator + 1);
    let tree = trees.get(commit);
    if (!tree) {
      tree = new Map(trackedFiles(root, commit).map((file) => [file.path, file]));
      trees.set(commit, tree);
    }
    const file = tree.get(path);
    if (!file || !['100644', '100755'].includes(file.mode))
      throw new HivexError({
        code: 'GROUND_CONTEXT_CODE_INVALID',
        message: 'Every contextual file must be a regular tracked file at its requested revision',
      });
    return { request, ref, commit, ...file };
  });
  const identities = files.map((file) => JSON.stringify([file.commit, file.path]));
  if (new Set(identities).size !== files.length)
    throw new HivexError({
      code: 'GROUND_CONTEXT_CODE_DUPLICATE',
      message: 'Different references selected the same contextual file revision',
    });
  const text = blobs(
    root,
    files.map((file) => ({ ...file, path: file.request })),
  );
  return files.map((file, index) => {
    const content = source({ ...file, path: file.request }, text);
    if (!content) throw new Error('Missing contextual file');
    return { ...file, id: `e${index + 1}`, text: content.text };
  });
}

function clean(root: string) {
  const status = git(root, [
    '-c',
    'core.fsmonitor=false',
    '-c',
    'core.untrackedCache=false',
    'status',
    '--porcelain=v1',
    '-z',
    '--untracked-files=normal',
    '--ignore-submodules=all',
  ]);
  if (status.length)
    throw new HivexError({
      code: 'GROUND_DIRTY_TREE',
      message: 'Grounding requires a clean checkout of the committed implementation being reviewed',
    });
}
export function isCurrent(root: string, head: string, context: CodeContext[] = []) {
  try {
    clean(root);
    return (
      resolveCommit(root, 'HEAD') === head &&
      context.every((file) => resolveCommit(root, file.ref) === file.commit)
    );
  } catch {
    return false;
  }
}
function source(file: GitFile | undefined, content: Map<string, string>) {
  if (!file) return null;
  const text = content.get(file.path);
  if (text === undefined || text.includes('\0'))
    throw new HivexError({
      code: 'GROUND_UNSUPPORTED_CODE',
      message: 'Grounding requires complete UTF-8 text for every changed file',
    });
  return { oid: file.oid, mode: file.mode, text };
}
export function codeSnapshot(root: string, reference: string) {
  clean(root);
  const head = resolveCommit(root, 'HEAD');
  const requestedBase = resolveCommit(root, reference);
  const base = git(root, ['merge-base', requestedBase, head]).toString('utf8').trim();
  const before = new Map(trackedFiles(root, base).map((file) => [file.path, file]));
  const after = new Map(trackedFiles(root, head).map((file) => [file.path, file]));
  const paths = [...new Set([...before.keys(), ...after.keys()])]
    .filter(
      (path) =>
        before.get(path)?.oid !== after.get(path)?.oid ||
        before.get(path)?.mode !== after.get(path)?.mode,
    )
    .sort();
  if (!paths.length)
    throw new HivexError({
      code: 'GROUND_EMPTY_CHANGESET',
      message: 'An empty changeset cannot establish implementation grounding',
    });
  if (paths.length > 32)
    throw new HivexError({
      code: 'GROUND_CHANGESET_TOO_LARGE',
      message: 'Grounding supports at most 32 complete changed text files per request',
    });
  const originals = paths.flatMap((path) => {
    const file = before.get(path);
    return file ? [file] : [];
  });
  const current = paths.flatMap((path) => {
    const file = after.get(path);
    return file ? [file] : [];
  });
  if ([...originals, ...current].some((file) => !['100644', '100755'].includes(file.mode)))
    throw new HivexError({
      code: 'GROUND_UNSUPPORTED_CODE',
      message: 'Symlink and submodule changes require a separate review',
    });
  const previousText = blobs(root, originals);
  const currentText = blobs(root, current);
  const files: CodeFile[] = paths.map((path, index) => ({
    id: `f${index + 1}`,
    path,
    before: source(before.get(path), previousText),
    after: source(after.get(path), currentText),
  }));
  const manifest = files.map(({ id, path, before, after }) => ({
    id,
    path,
    before: before && { oid: before.oid, mode: before.mode },
    after: after && { oid: after.oid, mode: after.mode },
  }));
  return {
    head,
    base,
    requestedBase,
    diffHash: hash(JSON.stringify({ base, head, files: manifest })),
    files,
  };
}
