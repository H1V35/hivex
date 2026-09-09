import { createHash } from 'node:crypto';
import { basename } from 'node:path';
import { fromMarkdown } from 'mdast-util-from-markdown';
import { toString } from 'mdast-util-to-string';
import { gfmFromMarkdown } from 'mdast-util-gfm';
import { gfm } from 'micromark-extension-gfm';
import { frontmatterFromMarkdown } from 'mdast-util-frontmatter';
import { frontmatter } from 'micromark-extension-frontmatter';
import { normalizeIdentifier } from 'micromark-util-normalize-identifier';
import { parseDocument } from 'yaml';

export const hash = (text: string) => createHash('sha256').update(text).digest('hex');
export const isMarkdownPath = (path: string) => /\.(?:md|markdown|mdown)$/i.test(path);

export function parseMarkdown(content: string) {
  return fromMarkdown(content, {
    extensions: [gfm(), frontmatter(['yaml'])],
    mdastExtensions: [gfmFromMarkdown(), frontmatterFromMarkdown(['yaml'])],
  });
}

type MarkdownNode = {
  type: string;
  children?: MarkdownNode[];
  url?: string;
  identifier?: string;
  position?: { start: { line: number; offset?: number }; end: { line: number; offset?: number } };
};

export function* descendants(tree: MarkdownNode): Generator<MarkdownNode> {
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

function metadata(tree: ReturnType<typeof parseMarkdown>) {
  const node = tree.children.find((entry) => entry.type === 'yaml');
  if (node?.type !== 'yaml') return { title: null, status: null };
  try {
    const parsed = parseDocument(node.value, { uniqueKeys: true });
    if (parsed.errors.length) return { title: null, status: null };
    const value: unknown = parsed.toJS({ maxAliasCount: 0 });
    if (!value || typeof value !== 'object' || Array.isArray(value))
      return { title: null, status: null };
    return {
      title:
        'title' in value && typeof value.title === 'string' && value.title.trim()
          ? value.title
          : null,
      status: 'status' in value && typeof value.status === 'string' ? value.status : null,
    };
  } catch {
    return { title: null, status: null };
  }
}

export function describeMarkdown(path: string, content: string) {
  const tree = parseMarkdown(content);
  const front = metadata(tree);
  const heading = tree.children.find((node) => node.type === 'heading');
  const definitions = new Map<string, string>();
  for (const node of descendants(tree)) {
    if (node.type === 'definition' && node.identifier && node.url) {
      const id = normalizeIdentifier(node.identifier);
      if (!definitions.has(id)) definitions.set(id, node.url);
    }
  }
  const links = [...descendants(tree)].flatMap((node) => {
    if (node.type === 'link' && node.url) return [node.url];
    if (node.type !== 'linkReference' || !node.identifier) return [];
    const url = definitions.get(normalizeIdentifier(node.identifier));
    return url ? [url] : [];
  });
  return {
    title: front.title ?? (heading ? toString(heading) : basename(path)),
    status: front.status,
    links,
  };
}

export function rawMarkdownLines(text: string): string[] {
  const lines = (text.match(/[^\r\n]*(?:\r\n|\r|\n|$)/g) ?? []).filter((line) => line.length > 0);
  return lines.length ? lines : [''];
}

export const lineContent = (line: string) => line.replace(/(?:\r\n|\r|\n)$/, '');

export function sourceRange(text: string, from: number, to: number) {
  const selected = rawMarkdownLines(text).slice(from - 1, to);
  return selected.slice(0, -1).join('') + lineContent(selected.at(-1) ?? '');
}
