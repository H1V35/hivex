import { createHash } from "node:crypto";
import pathModule from "node:path";
import { fromMarkdown } from "mdast-util-from-markdown";
import { gfmFromMarkdown } from "mdast-util-gfm";
import { gfm } from "micromark-extension-gfm";
import { frontmatterFromMarkdown } from "mdast-util-frontmatter";
import { frontmatter } from "micromark-extension-frontmatter";
import { normalizeIdentifier } from "micromark-util-normalize-identifier";
import { toString as mdastToString } from "mdast-util-to-string";
import { parseDocument } from "yaml";

export const hash = (text: string) =>
  createHash("sha256").update(text).digest("hex");
export const isMarkdownPath = (path: string) =>
  /\.(?:md|markdown|mdown)$/iu.test(path);

export const parseMarkdown = function parseMarkdown(content: string) {
  return fromMarkdown(content, {
    extensions: [gfm(), frontmatter(["yaml"])],
    mdastExtensions: [gfmFromMarkdown(), frontmatterFromMarkdown(["yaml"])],
  });
};

interface MarkdownNode {
  type: string;
  children?: MarkdownNode[];
  url?: string;
  identifier?: string;
  position?: {
    start: { line: number; offset?: number };
    end: { line: number; offset?: number };
  };
}

export const descendants = function* descendants(
  tree: MarkdownNode
): Generator<MarkdownNode> {
  const pending = [tree];
  while (pending.length) {
    const node = pending.pop();
    if (!node) {
      break;
    }
    yield node;
    for (let index = (node.children?.length ?? 0) - 1; index >= 0; index -= 1) {
      const child = node.children?.[index];
      if (child) {
        pending.push(child);
      }
    }
  }
};

const emptyMetadata = { status: null, title: null };

const yamlValue = function yamlValue(content: string): unknown {
  let parsed: ReturnType<typeof parseDocument>;
  try {
    parsed = parseDocument(content, { uniqueKeys: true });
  } catch {
    return null;
  }
  if (parsed.errors.length) {
    return null;
  }
  try {
    return parsed.toJS({ maxAliasCount: 0 });
  } catch {
    return null;
  }
};

const isRecord = function isRecord(
  value: unknown
): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
};

const metadata = function metadata(tree: ReturnType<typeof parseMarkdown>) {
  const node = tree.children.find((entry) => entry.type === "yaml");
  if (node?.type !== "yaml") {
    return emptyMetadata;
  }
  const value = yamlValue(node.value);
  if (!isRecord(value)) {
    return emptyMetadata;
  }
  const { status: statusValue, title: titleValue } = value;
  return {
    status: typeof statusValue === "string" ? statusValue : null,
    title:
      typeof titleValue === "string" && titleValue.trim().length > 0
        ? titleValue
        : null,
  };
};

export const describeMarkdown = function describeMarkdown(
  path: string,
  content: string
) {
  const tree = parseMarkdown(content);
  const front = metadata(tree);
  const heading = tree.children.find((node) => node.type === "heading");
  const definitions = new Map<string, string>();
  for (const node of descendants(tree)) {
    if (node.type !== "definition") {
      continue;
    }
    if (
      typeof node.identifier === "string" &&
      node.identifier.length > 0 &&
      typeof node.url === "string" &&
      node.url.length > 0
    ) {
      const id = normalizeIdentifier(node.identifier);
      if (!definitions.has(id)) {
        definitions.set(id, node.url);
      }
    }
  }
  const links = [...descendants(tree)].flatMap((node) => {
    if (
      node.type === "link" &&
      typeof node.url === "string" &&
      node.url.length > 0
    ) {
      return [node.url];
    }
    if (
      node.type !== "linkReference" ||
      typeof node.identifier !== "string" ||
      node.identifier.length === 0
    ) {
      return [];
    }
    const url = definitions.get(normalizeIdentifier(node.identifier));
    if (url === undefined) {
      return [];
    }
    return [url];
  });
  return {
    links,
    status: front.status,
    title:
      front.title ??
      (heading ? mdastToString(heading) : pathModule.basename(path)),
  };
};

export const rawMarkdownLines = function rawMarkdownLines(
  text: string
): string[] {
  const lines: string[] = [];
  let lineStart = 0;
  let index = 0;
  while (index < text.length) {
    const character = text[index];
    if (character === "\r" || character === "\n") {
      const lineEnd =
        index + (character === "\r" && text[index + 1] === "\n" ? 2 : 1);
      lines.push(text.slice(lineStart, lineEnd));
      lineStart = lineEnd;
      index = lineEnd;
    } else {
      index += 1;
    }
  }
  if (lineStart < text.length) {
    lines.push(text.slice(lineStart));
  }
  return lines.length ? lines : [""];
};

export const lineContent = (line: string) =>
  line.replace(/(?:\r\n|\r|\n)$/u, "");

export const sourceRange = function sourceRange(
  text: string,
  from: number,
  to: number
) {
  const selected = rawMarkdownLines(text).slice(from - 1, to);
  return selected.slice(0, -1).join("") + lineContent(selected.at(-1) ?? "");
};
