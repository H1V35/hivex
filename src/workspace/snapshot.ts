import { HivexError } from '../errors.ts';
import { hash, parseSource, type Source } from '../sources/markdown.ts';
import { collectionFor, parseConfig } from './config.ts';
import { blobs, resolveCommit, trackedFiles } from './git.ts';

export type Snapshot = ReturnType<typeof loadSnapshot>;
export function loadSnapshot(options: {
  root: string;
  ref: string;
  selection?: { sourceId?: string; collection?: string };
}) {
  const { root, ref, selection = {} } = options;
  const commit = resolveCommit(root, ref);
  const files = trackedFiles(root, commit);
  const configFile = files.find((file) => file.path === 'hivex.json');
  if (!configFile || (configFile.mode !== '100644' && configFile.mode !== '100755'))
    throw new HivexError({
      code: 'CONFIG_NOT_FOUND',
      message: 'The selected commit needs a regular tracked hivex.json',
    });
  const configText = blobs(root, [configFile]).get('hivex.json');
  if (!configText) throw new HivexError({ code: 'INVALID_CONFIG', message: 'hivex.json is empty' });
  const config = parseConfig(configText);
  if (selection.collection && !config.collections.some((item) => item.id === selection.collection))
    throw new HivexError({
      code: 'UNKNOWN_COLLECTION',
      message: 'Collection is not declared in hivex.json',
    });
  const declared = files.flatMap((file) => {
    if (!/\.(?:md|markdown|mdown)$/i.test(file.path)) return [];
    const collection = collectionFor(file.path, config.collections);
    if (!collection) return [];
    return [{ ...file, collection }];
  });
  const active = new Set(
    config.collections
      .filter((item) =>
        selection.collection
          ? item.id === selection.collection
          : item.default && item.kind !== 'legacy' && item.kind !== 'evidence',
      )
      .map((item) => item.id),
  );
  const selected = declared.filter((file) =>
    selection.sourceId ? file.path === selection.sourceId : active.has(file.collection.id),
  );
  for (const file of selected) {
    if (file.mode !== '100644' && file.mode !== '100755')
      throw new HivexError({
        code: 'UNSUPPORTED_SOURCE',
        message: `Sources must be regular tracked files: ${file.path}`,
      });
  }
  if (selected.length > 2048)
    throw new HivexError({
      code: 'TOO_MANY_SOURCES',
      message: 'A snapshot may contain at most 2048 Markdown sources',
    });
  const contents = blobs(root, selected);
  const sources = selected.map((file) => {
    const content = contents.get(file.path);
    if (content === undefined)
      throw new HivexError({ code: 'MISSING_SOURCE', message: `Missing source: ${file.path}` });
    return parseSource({ path: file.path, content: content, collection: file.collection });
  });
  validateReplacements(sources, declared);
  return { commit, configHash: hash(configText), config, sources };
}

function validateReplacements(sources: Source[], declared: { path: string; mode: string }[]) {
  const declaredPaths = new Set(
    declared
      .filter((file) => file.mode === '100644' || file.mode === '100755')
      .map((file) => file.path),
  );
  for (const source of sources) {
    for (const replacement of source.authority.supersededBy) {
      if (replacement === source.id || !declaredPaths.has(replacement))
        throw new HivexError({
          code: 'INVALID_REPLACEMENT',
          message: 'Declared replacement must be another regular source in the same snapshot',
          details: { source: source.id, replacement },
        });
    }
  }
}
