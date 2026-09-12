import {
  lstatSync,
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { randomUUID } from "node:crypto";
import path from "node:path";
import { compareSerializedStrings } from "./ordering.ts";
import { HivexError } from "./errors.ts";
import { isMarkdownPath } from "./markdown.ts";
import { emptyGraph, graphSchema } from "./knowledge-model.ts";
import type { Graph } from "./knowledge-model.ts";

const maxBytes = 64 * 1024 * 1024;

const isPortableDocument = function isPortableDocument(id: string) {
  const isProjectRelative =
    !id.startsWith("/") && !id.includes("\\") && !id.includes("\0");
  const hasSafeParts = id
    .split("/")
    .every(
      (part) =>
        !["..", ".", ".git", ".hivex", "node_modules", ""].includes(part)
    );
  return isMarkdownPath(id) && isProjectRelative && hasSafeParts;
};

const validateGraph = function validateGraph(graph: Graph) {
  const decisions = new Set(graph.decisions.map((entry) => entry.id));
  const relationships = new Set(graph.relationships.map((entry) => entry.id));
  const hasInvalidDecision = graph.decisions.some((entry) => {
    if (entry.quality !== "checked") {
      return false;
    }
    return entry.lineStart > entry.lineEnd;
  });
  const hasInvalidRelationship = graph.relationships.some((edge) => {
    if (!decisions.has(edge.from) || !decisions.has(edge.to)) {
      return true;
    }
    if (edge.evidence.length === 0) {
      return true;
    }
    return edge.evidence.some((entry) => entry.lineStart > entry.lineEnd);
  });
  const isInvalid =
    decisions.size !== graph.decisions.length ||
    relationships.size !== graph.relationships.length ||
    hasInvalidDecision ||
    hasInvalidRelationship;
  const references = [
    ...Object.keys(graph.documents),
    ...Object.values(graph.units).map((unit) => unit.document),
    ...graph.decisions.map((entry) => entry.document),
    ...graph.relationships.flatMap((edge) =>
      edge.evidence.map((entry) => entry.document)
    ),
    ...graph.warnings.flatMap((warning) => {
      if (typeof warning === "string") {
        return [];
      }
      return warning.scope.map((entry) => entry.document);
    }),
  ];
  if (isInvalid || references.some((id) => !isPortableDocument(id))) {
    throw new HivexError({
      code: "INVALID_SNAPSHOT",
      message:
        "Knowledge snapshot has invalid identities, relationships or project-relative sources.",
    });
  }
  return graph;
};

const snapshotPath = function snapshotPath(root: string) {
  const directory = path.join(root, ".hivex");
  const snapshotFile = path.join(directory, "graph.json");
  for (const candidate of [directory, snapshotFile]) {
    const stat = lstatSync(candidate, { throwIfNoEntry: false });
    if (
      stat?.isSymbolicLink() === true ||
      (stat && !(candidate === directory ? stat.isDirectory() : stat.isFile()))
    ) {
      throw new HivexError({
        code: "INVALID_SNAPSHOT",
        message: "Knowledge snapshot must use regular project-local files.",
      });
    }
  }
  return snapshotFile;
};

const parseSnapshot = function parseSnapshot(bytes: Uint8Array) {
  if (bytes.length > maxBytes) {
    throw new Error("Snapshot exceeds size limit");
  }
  const decoder = new TextDecoder("utf-8", { fatal: true });
  const data: unknown = JSON.parse(decoder.decode(bytes));
  return validateGraph(graphSchema.strict().parse(data));
};

export const readKnowledgeSnapshot = function readKnowledgeSnapshot(
  root: string
): Graph | null {
  const snapshotFile = snapshotPath(root);
  const stat = lstatSync(snapshotFile, { throwIfNoEntry: false });
  if (stat === undefined) {
    return null;
  }
  if (stat.size > maxBytes) {
    throw new HivexError({
      code: "INVALID_SNAPSHOT",
      message: "Knowledge snapshot exceeds 64 MiB.",
    });
  }
  try {
    return parseSnapshot(readFileSync(snapshotFile));
  } catch {
    throw new HivexError({
      code: "INVALID_SNAPSHOT",
      message: "Knowledge snapshot is not a supported graph JSON document.",
    });
  }
};

const ordered = function ordered(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(ordered);
  }
  if (value !== null && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value)
        .toSorted(([a], [b]) => compareSerializedStrings(a, b))
        .map(([key, entry]) => [key, ordered(entry)])
    );
  }
  return value;
};

const byId = function byId(a: { id: string }, b: { id: string }) {
  return compareSerializedStrings(a.id, b.id);
};

export const writeKnowledgeSnapshot = function writeKnowledgeSnapshot(
  root: string,
  value: Graph
) {
  const snapshotFile = snapshotPath(root);
  const graph = validateGraph(graphSchema.parse(value));
  const text = `${JSON.stringify(
    ordered({
      ...graph,
      decisions: graph.decisions.toSorted(byId),
      relationships: graph.relationships.toSorted(byId),
    }),
    null,
    2
  )}\n`;
  if (Buffer.byteLength(text) > maxBytes) {
    throw new HivexError({
      code: "INVALID_SNAPSHOT",
      message: "Knowledge snapshot exceeds 64 MiB.",
    });
  }
  mkdirSync(path.join(root, ".hivex"), { mode: 0o700, recursive: true });
  const temporary = path.join(root, ".hivex", `graph-${randomUUID()}.tmp`);
  try {
    writeFileSync(temporary, text, { flag: "wx", mode: 0o600 });
    renameSync(temporary, snapshotFile);
  } finally {
    rmSync(temporary, { force: true });
  }
  return snapshotFile;
};

export const sharedKnowledge = function sharedKnowledge(root: string) {
  return readKnowledgeSnapshot(root) ?? emptyGraph();
};
