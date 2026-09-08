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
import { normalizeIdentifier } from 'micromark-util-normalize-identifier';
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
export type Source = {
  id: string;
  path: string;
  title: string;
  collection: string | null;
  collectionKind: Collection['kind'] | null;
  contentHash: string;
  content: string;
  blocks: SourceBlock[];
  headings: { anchor: string; title: string; depth: number; block: number; offset: number }[];
  section: { anchor: string; lineStart: number; lineEnd: number } | null;
  containedAnchors: string[];
  references: {
    url: string;
    evidence: { quote: string; lineStart: number; lineEnd: number };
    definition?: { quote: string; lineStart: number; lineEnd: number };
  }[];
  authority: {
    declaredStatus: string;
    currentness: string;
    basis: string;
    scope: 'document';
    supersededBy: string[];
  };
};
export const hash = (text: string) => createHash('sha256').update(text).digest('hex');
export const isMarkdownPath = (path: string) => /\.(?:md|markdown|mdown)$/i.test(path);

function readMetadata(tree: ReturnType<typeof fromMarkdown>) {
  const node = tree.children.find((child) => child.type === 'yaml');
  if (node?.type !== 'yaml') return metadata.parse({});
  const document = parseDocument(node.value, { uniqueKeys: true });
  if (document.errors.length || document.warnings.length)
    throw new HivexError({
      code: 'INVALID_FRONTMATTER',
      message: 'Source frontmatter must be valid plain YAML',
    });
  const value: unknown = document.toJS({ maxAliasCount: 0 });
  const parsed = metadata.safeParse(value);
  if (!parsed.success)
    throw new HivexError({
      code: 'INVALID_FRONTMATTER',
      message: 'Source frontmatter metadata has invalid field types',
    });
  return parsed.data;
}

function declaredStatus(status: string | undefined) {
  const match = status
    ?.trim()
    .toLowerCase()
    .match(/^(accepted|proposed|superseded|historical)(?:\s|$)/)?.[1];
  return match ?? 'unknown';
}

export function parseSource(options: {
  path: string;
  content: string;
  collection: Collection | null;
}): Source {
  const { path, content, collection } = options;
  const tree = fromMarkdown(content, {
    extensions: [gfm(), frontmatter(['yaml'])],
    mdastExtensions: [gfmFromMarkdown(), frontmatterFromMarkdown(['yaml'])],
  });
  const front = readMetadata(tree);
  const anchors = headingAnchors(tree);
  const headings: Source['headings'] = [];
  const blocks: SourceBlock[] = tree.children.map((node, index) => {
    const start = node.position?.start.offset;
    const end = node.position?.end.offset;
    if (start === undefined || end === undefined || !node.position)
      throw new HivexError({
        code: 'INVALID_POSITION',
        message: 'Markdown parser did not provide source positions',
      });
    const anchor = anchors.get(node) ?? null;
    if (node.type === 'heading' && anchor !== null)
      headings.push({
        anchor,
        title: toString(node),
        depth: node.depth,
        block: index,
        offset: start,
      });
    return {
      kind: node.type,
      text: content.slice(start, end),
      lineStart: node.position.start.line,
      lineEnd: node.position.end.line,
      anchor,
    };
  });
  const heading = tree.children.find((node) => node.type === 'heading');
  const selectedAnchors = new Set(headings.map((entry) => entry.anchor));
  return {
    id: path,
    path,
    title: front.title ?? (heading ? toString(heading) : basename(path)),
    collection: collection?.id ?? null,
    collectionKind: collection?.kind ?? null,
    contentHash: hash(content),
    content,
    blocks,
    headings,
    section: null,
    containedAnchors: [...anchors.values()].filter((anchor) => !selectedAnchors.has(anchor)),
    references: documentReferences(tree, content),
    authority: {
      declaredStatus: declaredStatus(front.status),
      currentness: 'not-established',
      scope: 'document',
      basis: front.status ? 'frontmatter' : 'unspecified',
      supersededBy: front.superseded_by ?? [],
    },
  };
}

type MarkdownNode = {
  type: string;
  children?: MarkdownNode[];
  url?: string;
  identifier?: string;
  position?: { start: { line: number; offset?: number }; end: { line: number; offset?: number } };
};

function* descendants(tree: MarkdownNode): Generator<MarkdownNode> {
  const pending = [tree];
  while (pending.length) {
    const node = pending.pop();
    if (!node) break;
    yield node;
    for (let index = (node.children?.length ?? 0) - 1; index >= 0; index--) {
      const child = node.children?.[index];
      if (child) pending.push(child);
    }
  }
}

function quotation(node: MarkdownNode, content: string) {
  const start = node.position?.start.offset;
  const end = node.position?.end.offset;
  if (start === undefined || end === undefined || !node.position)
    throw new HivexError({
      code: 'INVALID_POSITION',
      message: 'A Markdown reference lacks original source positions',
    });
  return {
    quote: content.slice(start, end),
    lineStart: node.position.start.line,
    lineEnd: node.position.end.line,
  };
}

function linkDefinitions(tree: MarkdownNode) {
  const definitions = new Map<string, MarkdownNode>();
  let count = 0;
  for (const node of descendants(tree)) {
    if (node.type !== 'definition' || node.identifier === undefined) continue;
    count++;
    const key = normalizeIdentifier(node.identifier);
    if (!definitions.has(key)) definitions.set(key, node);
    if (count > 10000)
      throw new HivexError({
        code: 'SOURCE_REFERENCE_LIMIT',
        message: 'A document may contain at most 10000 link definitions',
      });
  }
  return definitions;
}

function documentReferences(tree: MarkdownNode, content: string): Source['references'] {
  const definitions = linkDefinitions(tree);
  const references: Source['references'] = [];
  let links = 0;
  for (const node of descendants(tree)) {
    if (node.type !== 'link' && node.type !== 'linkReference') continue;
    links++;
    if (links > 10000)
      throw new HivexError({
        code: 'SOURCE_REFERENCE_LIMIT',
        message: 'A document may contain at most 10000 Markdown links',
      });
    if (node.type === 'link' && node.url !== undefined) {
      references.push({ url: node.url, evidence: quotation(node, content) });
      continue;
    }
    if (node.type !== 'linkReference' || node.identifier === undefined) continue;
    const definition = definitions.get(normalizeIdentifier(node.identifier));
    if (definition?.url === undefined) continue;
    references.push({
      url: definition.url,
      evidence: quotation(node, content),
      definition: quotation(definition, content),
    });
  }
  return references;
}

function headingAnchors(tree: MarkdownNode) {
  const slugger = new GithubSlugger();
  const anchors = new Map<MarkdownNode, string>();
  for (const node of descendants(tree)) {
    if (node.type === 'heading') anchors.set(node, slugger.slug(toString(node)));
  }
  return anchors;
}
