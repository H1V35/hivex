import { createHash } from 'node:crypto';
import { basename } from 'node:path';
import GithubSlugger from 'github-slugger';
import { fromMarkdown } from 'mdast-util-from-markdown';
import { toString } from 'mdast-util-to-string';
import { gfmFromMarkdown } from 'mdast-util-gfm';
import { gfm } from 'micromark-extension-gfm';
import { frontmatterFromMarkdown } from 'mdast-util-frontmatter';
import { frontmatter } from 'micromark-extension-frontmatter';
import { parseDocument } from 'yaml';
import { z } from 'zod';
import { HivexError } from '../errors.ts';
import type { Collection } from '../workspace/config.ts';

const metadata = z.looseObject({
  title: z.string().optional(),
  status: z.string().optional(),
  superseded_by: z.array(z.string()).max(32).optional(),
});
export type SourceBlock = {
  kind: string;
  text: string;
  lineStart: number;
  lineEnd: number;
  anchor: string | null;
};
export type Source = ReturnType<typeof parseSource>;
export const hash = (text: string) => createHash('sha256').update(text).digest('hex');

function readMetadata(tree: ReturnType<typeof fromMarkdown>) {
  const node = tree.children.find((child) => child.type === 'yaml');
  if (node?.type !== 'yaml') return metadata.parse({});
  const document = parseDocument(node.value, { uniqueKeys: true });
  if (document.errors.length || document.warnings.length)
    throw new HivexError('INVALID_FRONTMATTER', 'Source frontmatter must be valid plain YAML');
  const value: unknown = document.toJS({ maxAliasCount: 0 });
  const parsed = metadata.safeParse(value);
  if (!parsed.success)
    throw new HivexError(
      'INVALID_FRONTMATTER',
      'Source frontmatter metadata has invalid field types',
    );
  return parsed.data;
}

function declaredStatus(status: string | undefined) {
  const match = status
    ?.trim()
    .toLowerCase()
    .match(/^(accepted|proposed|superseded|historical)(?:\s|$)/)?.[1];
  return match ?? 'unknown';
}

export function parseSource(path: string, content: string, collection: Collection) {
  const tree = fromMarkdown(content, {
    extensions: [gfm(), frontmatter(['yaml'])],
    mdastExtensions: [gfmFromMarkdown(), frontmatterFromMarkdown(['yaml'])],
  });
  const front = readMetadata(tree);
  const slugger = new GithubSlugger();
  const blocks: SourceBlock[] = tree.children.map((node) => {
    const start = node.position?.start.offset;
    const end = node.position?.end.offset;
    if (start === undefined || end === undefined || !node.position)
      throw new HivexError('INVALID_POSITION', 'Markdown parser did not provide source positions');
    return {
      kind: node.type,
      text: content.slice(start, end),
      lineStart: node.position.start.line,
      lineEnd: node.position.end.line,
      anchor: node.type === 'heading' ? slugger.slug(toString(node)) : null,
    };
  });
  const heading = tree.children.find((node) => node.type === 'heading');
  return {
    id: path,
    path,
    title: front.title ?? (heading ? toString(heading) : basename(path)),
    collection: collection.id,
    collectionKind: collection.kind,
    contentHash: hash(content),
    content,
    blocks,
    authority: {
      declaredStatus: declaredStatus(front.status),
      currentness: 'not-established',
      basis: front.status ? 'frontmatter' : 'unspecified',
      supersededBy: front.superseded_by ?? [],
    },
  };
}
