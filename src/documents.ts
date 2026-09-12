import { lstatSync, readFileSync, readdirSync } from "node:fs";
import pathModule from "node:path";
import { parseArgs } from "node:util";
import { compareSerializedStrings } from "./ordering.ts";
import { HivexError } from "./errors.ts";
import {
  describeMarkdown,
  hash,
  isMarkdownPath,
  lineContent,
  rawMarkdownLines,
} from "./markdown.ts";

export interface Document {
  id: string;
  path: string;
  title: string;
  text: string;
  hash: string;
  status: string | null;
  links: string[];
  historical: boolean;
}

export interface Project {
  root: string;
  snapshot: string;
  currentSnapshot: string;
  documents: Document[];
  currentDocuments: Document[];
  historicalDocuments: Document[];
  warnings: { path: string; message: string }[];
}

interface Config {
  include: string[];
  exclude: string[];
  history: string[];
}
interface Candidate {
  absolutePath: string;
  path: string;
}
interface ParsedDocument {
  document: Document;
  rawLinks: string[];
}
interface CommandOptions {
  root: string;
  maxBytes: number;
  from?: number;
  to?: number;
  limit: number;
  cursor?: string;
}
interface ParsedValues {
  [key: string]: string | undefined;
  root?: string;
  cursor?: string;
  from?: string;
  limit?: string;
  to?: string;
}
interface CollectionContext {
  config: Config;
  root: string;
  warnings: Project["warnings"];
}
interface ContinuationOptions {
  lineEnd: number;
  maxBytes: number;
  requestedEnd: number;
  totalLines: number;
}

const defaultInclude = ["**/*.md", "**/*.markdown", "**/*.mdown"];
const defaultMaxBytes = 16_384;
const maxOutputBytes = 65_536;
const maxSourceBytes = 32 * 1024 * 1024;
const maxCorpusBytes = 64 * 1024 * 1024;
const maxDocuments = 2048;
const maxPatterns = 64;
const origin = "current-worktree";
const protectedDirectories = new Set([".git", ".hivex", "node_modules"]);
const excludedDirectories = new Set(["vendor", "dist", "build"]);
const decoder = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });

const fail = function fail(
  code: string,
  message: string,
  details?: Record<string, unknown>
): never {
  throw new HivexError({ code, details, message });
};

const pathFor = function pathFor(root: string, absolutePath: string) {
  return pathModule.relative(root, absolutePath).replaceAll("\\", "/");
};

const decodeUtf8 = function decodeUtf8(
  bytes: Buffer,
  sourcePath: string,
  limit: number
) {
  if (bytes.byteLength > limit) {
    fail("DOCUMENT_TOO_LARGE", `Markdown source exceeds ${limit} bytes`, {
      actualBytes: bytes.byteLength,
      maxBytes: limit,
      path: sourcePath,
    });
  }
  try {
    return decoder.decode(bytes);
  } catch {
    return fail("INVALID_UTF8", "Markdown source is not valid UTF-8", {
      path: sourcePath,
    });
  }
};

const readUtf8 = function readUtf8(
  absolutePath: string,
  sourcePath: string,
  limit: number
) {
  try {
    return decodeUtf8(readFileSync(absolutePath), sourcePath, limit);
  } catch (error) {
    if (error instanceof HivexError) {
      throw error;
    }
    return fail("SOURCE_READ_FAILED", "Unable to read Markdown source", {
      path: sourcePath,
      reason: Error.isError(error) ? error.message : "unknown read failure",
    });
  }
};

const validatePattern = function validatePattern(
  pattern: unknown,
  field: string,
  index: number
) {
  if (typeof pattern !== "string" || !pattern.trim()) {
    return fail(
      "INVALID_CONFIG",
      `${field}[${index}] must be a non-empty relative glob`
    );
  }
  const normalized = pattern.replaceAll("\\", "/");
  const segments = normalized.split("/");
  if (
    pathModule.isAbsolute(normalized) ||
    normalized.startsWith("/") ||
    normalized.includes("\u{0}") ||
    segments.includes("..")
  ) {
    return fail(
      "INVALID_CONFIG",
      `${field}[${index}] must stay inside the project root`
    );
  }
  try {
    const glob = new Bun.Glob(normalized);
    glob.match("");
  } catch (error) {
    return fail("INVALID_CONFIG", `${field}[${index}] is not a valid glob`, {
      reason: Error.isError(error) ? error.message : "invalid glob",
    });
  }
  return normalized;
};

const patterns = function patterns(
  value: unknown,
  field: string,
  fallback: string[]
) {
  if (value === undefined) {
    return [...fallback];
  }
  if (!Array.isArray(value) || value.length > maxPatterns) {
    return fail(
      "INVALID_CONFIG",
      `${field} must contain at most ${maxPatterns} relative globs`
    );
  }
  return value.map((pattern, index) => validatePattern(pattern, field, index));
};

const readConfigBytes = function readConfigBytes(configPath: string) {
  const stat = lstatSync(configPath);
  if (stat.isSymbolicLink()) {
    return fail("INVALID_CONFIG", "hivex.json must not be a symlink");
  }
  if (!stat.isFile()) {
    return fail("INVALID_CONFIG", "hivex.json must be a regular file");
  }
  return readFileSync(configPath);
};

const configText = function configText(root: string) {
  const configPath = pathModule.join(root, "hivex.json");
  let bytes: Buffer;
  try {
    bytes = readConfigBytes(configPath);
  } catch (error) {
    if (error instanceof HivexError) {
      throw error;
    }
    if (Error.isError(error) && "code" in error && error.code === "ENOENT") {
      return null;
    }
    return fail("INVALID_CONFIG", "Unable to read hivex.json", {
      reason: Error.isError(error) ? error.message : "unknown read failure",
    });
  }
  return decodeUtf8(bytes, "hivex.json", 64 * 1024);
};

const isRecord = function isRecord(
  value: unknown
): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
};

const parseConfig = function parseConfig(text: string): Config {
  let value: unknown;
  try {
    value = JSON.parse(text.replace(/^\u{FEFF}/u, ""));
  } catch (error) {
    return fail("INVALID_CONFIG", "hivex.json must contain valid JSON", {
      reason: Error.isError(error) ? error.message : "invalid JSON",
    });
  }
  if (!isRecord(value)) {
    return fail("INVALID_CONFIG", "hivex.json must contain an object");
  }
  if ("collections" in value) {
    return fail(
      "LEGACY_CONFIGURATION",
      "hivex.json uses legacy collections; replace it with include and exclude globs"
    );
  }
  const record = value;
  const unknown = Object.keys(record).filter(
    (key) => !["exclude", "history", "include"].includes(key)
  );
  if (unknown.length) {
    return fail(
      "INVALID_CONFIG",
      `hivex.json has unsupported field: ${unknown[0]}`
    );
  }
  return {
    exclude: patterns(value.exclude, "exclude", []),
    history: patterns(value.history, "history", []),
    include: patterns(value.include, "include", defaultInclude),
  };
};

const configFrom = function configFrom(root: string): Config {
  const text = configText(root);
  if (text === null) {
    return { exclude: [], history: [], include: [...defaultInclude] };
  }
  return parseConfig(text);
};

const isExcludedName = function isExcludedName(name: string, config: Config) {
  if (protectedDirectories.has(name)) {
    return true;
  }
  if (!excludedDirectories.has(name) && !name.startsWith(".")) {
    return false;
  }
  return [...config.include, ...config.history].every(
    (pattern) => !pattern.split("/").includes(name)
  );
};

const isMatch = function isMatch(pathName: string, patternsToMatch: string[]) {
  return patternsToMatch.some((pattern) => {
    const glob = new Bun.Glob(pattern);
    return glob.match(pathName);
  });
};

const isExcludedSubtree = function isExcludedSubtree(
  pathName: string,
  config: Config
) {
  const subtrees = config.exclude.filter(
    (pattern) => pattern.endsWith("/**") && !pattern.startsWith("!")
  );
  return isMatch(`${pathName}/`, subtrees);
};

const collectCandidates = function collectCandidates(
  current: string,
  context: CollectionContext
) {
  const { config, root, warnings } = context;
  const candidates: Candidate[] = [];
  let entries;
  try {
    entries = readdirSync(current, { withFileTypes: true }).toSorted(
      (left, right) => left.name.localeCompare(right.name)
    );
  } catch (error) {
    warnings.push({
      message: `Unable to inspect directory: ${Error.isError(error) ? error.message : "unknown error"}`,
      path: pathFor(root, current) || ".",
    });
    return candidates;
  }

  for (const entry of entries) {
    if (isExcludedName(entry.name, config)) {
      continue;
    }
    const absolutePath = pathModule.join(current, entry.name);
    const path = pathFor(root, absolutePath);
    if (entry.isSymbolicLink()) {
      warnings.push({ message: "Skipped symbolic link", path });
    } else if (entry.isDirectory()) {
      if (!isExcludedSubtree(path, config)) {
        candidates.push(...collectCandidates(absolutePath, context));
      }
    } else if (entry.isFile()) {
      candidates.push({ absolutePath, path });
    }
  }
  return candidates;
};

const selected = function selected(candidates: Candidate[], config: Config) {
  const available = candidates
    .filter(({ path: pathName }) => isMarkdownPath(pathName))
    .filter(({ path: pathName }) => !isMatch(pathName, config.exclude))
    .toSorted((left, right) => left.path.localeCompare(right.path));
  return {
    current: available.filter(
      ({ path: pathName }) =>
        isMatch(pathName, config.include) && !isMatch(pathName, config.history)
    ),
    historical: available.filter(({ path: pathName }) =>
      isMatch(pathName, config.history)
    ),
  };
};

const parseCandidate = function parseCandidate(
  candidate: Candidate,
  isHistorical: boolean
): ParsedDocument {
  const text = readUtf8(candidate.absolutePath, candidate.path, maxSourceBytes);
  const source = describeMarkdown(candidate.path, text);
  return {
    document: {
      hash: hash(text),
      historical: isHistorical,
      id: candidate.path,
      links: [],
      path: candidate.path,
      status: source.status,
      text,
      title: source.title,
    },
    rawLinks: source.links,
  };
};

const warningFor = function warningFor(path: string, error: unknown) {
  return {
    message: Error.isError(error)
      ? error.message
      : "Unable to parse Markdown source",
    path,
  };
};

const linkPath = function linkPath(
  root: string,
  source: Document,
  rawLink: string
) {
  if (
    !rawLink ||
    rawLink.startsWith("#") ||
    /^[a-z][a-z0-9+.-]*:/iu.test(rawLink)
  ) {
    return null;
  }
  const fragment = rawLink.search(/[?#]/u);
  const target = fragment === -1 ? rawLink : rawLink.slice(0, fragment);
  if (!target) {
    return null;
  }
  let decoded: string;
  try {
    decoded = decodeURIComponent(target);
  } catch {
    return null;
  }
  const absoluteTarget = pathModule.resolve(
    pathModule.dirname(pathModule.join(root, source.path)),
    decoded
  );
  const relativeTarget = pathFor(root, absoluteTarget);
  if (!relativeTarget || relativeTarget === ".") {
    return null;
  }
  if (
    relativeTarget.startsWith("../") ||
    pathModule.isAbsolute(relativeTarget)
  ) {
    return null;
  }
  if (!isMarkdownPath(relativeTarget)) {
    return null;
  }
  return relativeTarget;
};

const resolveLinks = function resolveLinks(
  root: string,
  documents: ParsedDocument[]
) {
  for (const { document, rawLinks } of documents) {
    const links = new Set<string>();
    for (const rawLink of rawLinks) {
      const link = linkPath(root, document, rawLink);
      if (link !== null) {
        links.add(link);
      }
    }
    document.links = [...links];
  }
};

const snapshotFor = function snapshotFor(
  documents: Document[],
  config: Config
) {
  const identities = documents
    .map((document) => `${document.id}\0${document.hash}`)
    .toSorted(compareSerializedStrings)
    .join("\n");
  const selection = JSON.stringify(
    Object.fromEntries([
      ["include", [...config.include].toSorted(compareSerializedStrings)],
      ["exclude", [...config.exclude].toSorted(compareSerializedStrings)],
      ["history", [...config.history].toSorted(compareSerializedStrings)],
      [
        "ignoredDirectories",
        [...protectedDirectories, ...excludedDirectories].toSorted(
          compareSerializedStrings
        ),
      ],
      ["markdownExtensions", [".md", ".markdown", ".mdown"]],
    ])
  );
  return hash(`${identities}\nselection\0${selection}`);
};

const validateRoot = function validateRoot(requested: string) {
  const stat = lstatSync(requested);
  if (stat.isSymbolicLink()) {
    return fail("INVALID_ROOT", "Project root must not be a symlink");
  }
  if (!stat.isDirectory()) {
    return fail("INVALID_ROOT", "Project root must be a directory");
  }
  return requested;
};

const absoluteRoot = function absoluteRoot(root: string) {
  if (!root.trim()) {
    fail("INVALID_ROOT", "Project root must be a non-empty path");
  }
  const requested = pathModule.resolve(root);
  try {
    return validateRoot(requested);
  } catch (error) {
    if (error instanceof HivexError) {
      throw error;
    }
    return fail("INVALID_ROOT", "Project root is not readable", {
      reason: Error.isError(error) ? error.message : "unknown root failure",
      root: requested,
    });
  }
};

const parseCandidateWithinBudget = function parseCandidateWithinBudget(
  candidate: Candidate,
  isHistorical: boolean,
  sourceBytes: number
) {
  if (sourceBytes + lstatSync(candidate.absolutePath).size > maxCorpusBytes) {
    return fail(
      "CORPUS_LIMIT",
      "Selected Markdown exceeds the 64 MiB memory budget; narrow include paths"
    );
  }
  return parseCandidate(candidate, isHistorical);
};

export const loadProject = function loadProject(root: string): Project {
  const projectRoot = absoluteRoot(root);
  const config = configFrom(projectRoot);
  const warnings: Project["warnings"] = [];
  const candidates = collectCandidates(projectRoot, {
    config,
    root: projectRoot,
    warnings,
  });
  const selection = selected(candidates, config);
  const historicalPaths = new Set(
    selection.historical.map((candidate) => candidate.path)
  );
  const selectedCandidates = [...selection.current, ...selection.historical];
  const parsed: ParsedDocument[] = [];
  let sourceBytes = 0;
  for (const candidate of selectedCandidates.slice(0, maxDocuments)) {
    try {
      const parsedDocument = parseCandidateWithinBudget(
        candidate,
        historicalPaths.has(candidate.path),
        sourceBytes
      );
      sourceBytes += Buffer.byteLength(parsedDocument.document.text);
      parsed.push(parsedDocument);
    } catch (error) {
      warnings.push(warningFor(candidate.path, error));
    }
  }
  if (selectedCandidates.length > maxDocuments) {
    warnings.push({
      message: `Only the first ${maxDocuments} Markdown sources were loaded`,
      path: ".",
    });
  }
  resolveLinks(projectRoot, parsed);
  const documents = parsed
    .map(({ document }) => document)
    .toSorted((left, right) => left.path.localeCompare(right.path));
  return {
    currentDocuments: documents.filter((document) => !document.historical),
    currentSnapshot: snapshotFor(
      documents.filter((document) => !document.historical),
      config
    ),
    documents,
    historicalDocuments: documents.filter((document) => document.historical),
    root: projectRoot,
    snapshot: snapshotFor(documents, config),
    warnings,
  };
};

const positiveInteger = function positiveInteger(
  value: string | undefined,
  label: string,
  fallback?: number
) {
  if (value === undefined) {
    if (fallback !== undefined) {
      return fallback;
    }
    return fail("INVALID_ARGUMENT", `${label} is required`);
  }
  if (!/^\d+$/u.test(value)) {
    return fail("INVALID_ARGUMENT", `${label} must be a positive integer`);
  }
  const number = Number(value);
  if (!Number.isSafeInteger(number) || number < 1) {
    return fail("INVALID_ARGUMENT", `${label} must be positive`);
  }
  return number;
};

const optionalPositiveInteger = function optionalPositiveInteger(
  value: string | undefined,
  label: string
) {
  let result: number | undefined;
  if (value !== undefined) {
    result = positiveInteger(value, label);
  }
  return result;
};

const parseCommandArguments = function parseCommandArguments(
  commandArguments: string[]
) {
  try {
    return parseArgs({
      allowPositionals: true,
      args: commandArguments,
      options: {
        cursor: { type: "string" },
        from: { type: "string" },
        limit: { type: "string" },
        "max-bytes": { type: "string" },
        root: { type: "string" },
        to: { type: "string" },
      },
      strict: true,
    });
  } catch (error) {
    return fail(
      "INVALID_ARGUMENT",
      Error.isError(error) ? error.message : "Invalid command arguments"
    );
  }
};

const validatePositionals: (
  command: string | undefined,
  id: string | undefined,
  extra: string | undefined
) => asserts command is "sources" | "read" = function validatePositionals(
  command: string | undefined,
  id: string | undefined,
  extra: string | undefined
): asserts command is "sources" | "read" {
  if (command !== "sources" && command !== "read") {
    fail("INVALID_ARGUMENT", "Usage: hivex sources | read <id> [options]");
  }
  if (command === "sources" && (id !== undefined || extra !== undefined)) {
    fail("INVALID_ARGUMENT", "sources does not accept a source id");
  }
  if (command === "read" && id === undefined) {
    fail("INVALID_ARGUMENT", "read requires a source id");
  }
  if (command === "read" && extra !== undefined) {
    fail("INVALID_ARGUMENT", "read accepts one source id");
  }
};

const rangeOptions = function rangeOptions(
  command: string,
  values: ParsedValues
) {
  if (command === "sources") {
    if (values.from !== undefined || values.to !== undefined) {
      fail("INVALID_ARGUMENT", "--from and --to are only valid for read");
    }
    return {};
  }
  if (values.limit !== undefined || values.cursor !== undefined) {
    fail("INVALID_ARGUMENT", "--limit and --cursor are only valid for sources");
  }
  return {
    from: optionalPositiveInteger(values.from, "--from"),
    to: optionalPositiveInteger(values.to, "--to"),
  };
};

const commandOptions = function commandOptions(commandArguments: string[]): {
  command: string;
  id: string | undefined;
  options: CommandOptions;
} {
  const parsed = parseCommandArguments(commandArguments);
  const values = parsed.values as ParsedValues;
  const [command, id, extra] = parsed.positionals;
  validatePositionals(command, id, extra);
  const maxBytes = positiveInteger(
    values["max-bytes"],
    "--max-bytes",
    defaultMaxBytes
  );
  if (maxBytes > maxOutputBytes) {
    fail("INVALID_ARGUMENT", `--max-bytes must be at most ${maxOutputBytes}`);
  }
  return {
    command,
    id,
    options: {
      ...rangeOptions(command, values),
      cursor: values.cursor,
      limit: positiveInteger(values.limit, "--limit", 20),
      maxBytes,
      root: values.root ?? process.cwd(),
    },
  };
};

const metadata = function metadata(document: Document) {
  const result = { ...document };
  Reflect.deleteProperty(result, "text");
  return result;
};

const linesFor = function linesFor(text: string) {
  return { lines: rawMarkdownLines(text) };
};

const boundedLines = function boundedLines(window: {
  lines: string[];
  start: number;
  end: number;
  maxBytes: number;
}) {
  const { lines, start, end, maxBytes } = window;
  let text = "";
  let prefix = "";
  let lineEnd = start - 1;
  for (let line = start; line <= end; line += 1) {
    const raw = lines[line - 1] ?? "";
    const current = line === lines.length ? raw : lineContent(raw);
    const next = prefix + current;
    if (Buffer.byteLength(next) > maxBytes) {
      if (lineEnd < start) {
        fail("OUTPUT_LIMIT", "The first requested line exceeds --max-bytes", {
          line,
          maxBytes,
          requiredBytes: Buffer.byteLength(next),
        });
      }
      return { lineEnd, text };
    }
    text = next;
    prefix += raw;
    lineEnd = line;
  }
  return { lineEnd, text };
};

const continuationFor = function continuationFor({
  lineEnd,
  maxBytes,
  requestedEnd,
  totalLines,
}: ContinuationOptions) {
  if (lineEnd >= totalLines) {
    return null;
  }
  const reason = lineEnd < requestedEnd ? "max-bytes" : "range";
  return {
    from: lineEnd + 1,
    maxBytes,
    reason,
    to: totalLines,
  };
};

const readCommand = function readCommand(
  project: Project,
  id: string,
  options: CommandOptions
) {
  const source = project.documents.find((document) => document.id === id);
  if (!source) {
    return fail("SOURCE_NOT_FOUND", `Markdown source was not selected: ${id}`, {
      id,
    });
  }
  const { lines } = linesFor(source.text);
  const start = options.from ?? 1;
  const requestedEnd = options.to ?? lines.length;
  if (
    start > lines.length ||
    requestedEnd > lines.length ||
    start > requestedEnd
  ) {
    fail(
      "INVALID_RANGE",
      `Line range ${start}-${requestedEnd} is outside the source`,
      {
        id,
        lineCount: lines.length,
      }
    );
  }
  const bounded = boundedLines({
    end: requestedEnd,
    lines,
    maxBytes: options.maxBytes,
    start,
  });
  const continuation = continuationFor({
    lineEnd: bounded.lineEnd,
    maxBytes: options.maxBytes,
    requestedEnd,
    totalLines: lines.length,
  });
  return {
    command: "read",
    continuation,
    lineEnd: bounded.lineEnd,
    lineStart: start,
    origin,
    snapshot: project.snapshot,
    source: metadata(source),
    text: bounded.text,
    truncated: continuation !== null,
    warnings: project.warnings,
  };
};

const listSources = function listSources(
  project: Project,
  options: CommandOptions
) {
  const cursor = options.cursor?.match(
    /^s1\.(?<snapshot>[a-f\d]{64})\.(?<start>\d+)$/u
  );
  const cursorSnapshot = cursor?.groups?.snapshot;
  if (options.cursor !== undefined && cursorSnapshot !== project.snapshot) {
    fail(
      "INVALID_CURSOR",
      "Source continuation belongs to a different or invalid snapshot"
    );
  }
  const start = Number(cursor?.groups?.start ?? 0);
  if (
    !Number.isSafeInteger(start) ||
    start < 0 ||
    (start > 0 && start >= project.documents.length)
  ) {
    fail("INVALID_CURSOR", "Source continuation is outside this snapshot");
  }
  const documents: ReturnType<typeof metadata>[] = [];
  const response = function response() {
    return {
      command: "sources",
      continuation:
        start + documents.length < project.documents.length
          ? `s1.${project.snapshot}.${start + documents.length}`
          : null,
      documents,
      origin,
      snapshot: project.snapshot,
      totalDocuments: project.documents.length,
      warnings: project.warnings,
    };
  };
  const selectedDocuments = project.documents.slice(
    start,
    start + Math.min(options.limit, maxDocuments)
  );
  for (const document of selectedDocuments) {
    documents.push(metadata(document));
    if (Buffer.byteLength(JSON.stringify(response())) > options.maxBytes) {
      documents.pop();
      if (!documents.length) {
        fail(
          "OUTPUT_LIMIT",
          "The next source metadata does not fit; increase --max-bytes or narrow the selected sources"
        );
      }
      break;
    }
  }
  if (Buffer.byteLength(JSON.stringify(response())) > options.maxBytes) {
    fail("OUTPUT_LIMIT", "Source-list metadata exceeds --max-bytes");
  }
  return response();
};

export const documentCommand = function documentCommand(
  commandArguments: string[]
): unknown {
  const { command, id, options } = commandOptions(commandArguments);
  const project = loadProject(options.root);
  if (command === "sources") {
    return listSources(project, options);
  }
  return readCommand(project, id ?? "", options);
};
