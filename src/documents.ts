import { rawMarkdownLines, lineContent } from './markdown.ts';
import { lstatSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, isAbsolute, join, relative, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { TextDecoder } from 'node:util';
import { HivexError } from './errors.ts';
import { describeMarkdown, hash, isMarkdownPath } from './markdown.ts';

export type Document = {
  id: string;
  path: string;
  title: string;
  text: string;
  hash: string;
  status: string | null;
  links: string[];
};

export type Project = {
  root: string;
  snapshot: string;
  documents: Document[];
  warnings: { path: string; message: string }[];
};

type Config = { include: string[]; exclude: string[] };
type Candidate = { absolutePath: string; path: string };
type ParsedDocument = Document & { rawLinks: string[] };
type CommandOptions = {
  root: string;
  maxBytes: number;
  from: number | undefined;
  to: number | undefined;
  limit: number;
  cursor?: string;
};
type ParsedValues = {
  root?: string;
  'max-bytes'?: string;
  limit?: string;
  cursor?: string;
  from?: string;
  to?: string;
};

const DEFAULT_INCLUDE = ['**/*.md', '**/*.markdown', '**/*.mdown'];
const DEFAULT_MAX_BYTES = 16_384;
const MAX_OUTPUT_BYTES = 65_536;
const MAX_SOURCE_BYTES = 32 * 1024 * 1024;
const MAX_CORPUS_BYTES = 64 * 1024 * 1024;
const MAX_DOCUMENTS = 2_048;
const MAX_PATTERNS = 64;
const ORIGIN = 'current-worktree';
const PROTECTED_DIRECTORIES = new Set(['.git', '.hivex', 'node_modules']);
const EXCLUDED_DIRECTORIES = new Set(['vendor', 'dist', 'build']);
const decoder = new TextDecoder('utf-8', { fatal: true });

function fail(code: string, message: string, details?: Record<string, unknown>): never {
  throw new HivexError({ code, message, details });
}

function pathFor(root: string, absolutePath: string) {
  return relative(root, absolutePath).split('\\').join('/');
}

function decodeUtf8(bytes: Buffer, path: string, limit: number) {
  if (bytes.byteLength > limit)
    fail('DOCUMENT_TOO_LARGE', `Markdown source exceeds ${limit} bytes`, {
      path,
      actualBytes: bytes.byteLength,
      maxBytes: limit,
    });
  try {
    return decoder.decode(bytes);
  } catch {
    fail('INVALID_UTF8', 'Markdown source is not valid UTF-8', { path });
  }
}

function readUtf8(absolutePath: string, path: string, limit: number) {
  try {
    return decodeUtf8(readFileSync(absolutePath), path, limit);
  } catch (error) {
    if (error instanceof HivexError) throw error;
    fail('SOURCE_READ_FAILED', 'Unable to read Markdown source', {
      path,
      reason: error instanceof Error ? error.message : 'unknown read failure',
    });
  }
}

function validatePattern(pattern: unknown, field: string, index: number) {
  if (typeof pattern !== 'string' || !pattern.trim())
    fail('INVALID_CONFIG', `${field}[${index}] must be a non-empty relative glob`);
  const normalized = pattern.replaceAll('\\', '/');
  const segments = normalized.split('/');
  if (
    isAbsolute(normalized) ||
    normalized.startsWith('/') ||
    normalized.includes('\0') ||
    segments.includes('..')
  )
    fail('INVALID_CONFIG', `${field}[${index}] must stay inside the project root`);
  try {
    new Bun.Glob(normalized);
  } catch (error) {
    fail('INVALID_CONFIG', `${field}[${index}] is not a valid glob`, {
      reason: error instanceof Error ? error.message : 'invalid glob',
    });
  }
  return normalized;
}

function patterns(value: unknown, field: string, fallback: string[]) {
  if (value === undefined) return [...fallback];
  if (!Array.isArray(value) || value.length > MAX_PATTERNS)
    fail('INVALID_CONFIG', `${field} must contain at most ${MAX_PATTERNS} relative globs`);
  return value.map((pattern, index) => validatePattern(pattern, field, index));
}

function configText(root: string) {
  const path = join(root, 'hivex.json');
  let bytes: Buffer;
  try {
    const stat = lstatSync(path);
    if (stat.isSymbolicLink()) fail('INVALID_CONFIG', 'hivex.json must not be a symlink');
    if (!stat.isFile()) fail('INVALID_CONFIG', 'hivex.json must be a regular file');
    bytes = readFileSync(path);
  } catch (error) {
    if (error instanceof HivexError) throw error;
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return null;
    fail('INVALID_CONFIG', 'Unable to read hivex.json', {
      reason: error instanceof Error ? error.message : 'unknown read failure',
    });
  }
  return decodeUtf8(bytes, 'hivex.json', 64 * 1024);
}

function parseConfig(text: string): Config {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch (error) {
    fail('INVALID_CONFIG', 'hivex.json must contain valid JSON', {
      reason: error instanceof Error ? error.message : 'invalid JSON',
    });
  }
  if (!value || typeof value !== 'object' || Array.isArray(value))
    fail('INVALID_CONFIG', 'hivex.json must contain an object');
  const record = value as Record<string, unknown>;
  if ('collections' in record)
    fail(
      'LEGACY_CONFIGURATION',
      'hivex.json uses legacy collections; replace it with include and exclude globs',
    );
  const unknown = Object.keys(record).filter((key) => key !== 'include' && key !== 'exclude');
  if (unknown.length) fail('INVALID_CONFIG', `hivex.json has unsupported field: ${unknown[0]}`);
  return {
    include: patterns(record.include, 'include', DEFAULT_INCLUDE),
    exclude: patterns(record.exclude, 'exclude', []),
  };
}

function configFrom(root: string): Config {
  const text = configText(root);
  if (text === null) return { include: [...DEFAULT_INCLUDE], exclude: [] };
  return parseConfig(text);
}

function excludedName(name: string, config: Config) {
  if (PROTECTED_DIRECTORIES.has(name)) return true;
  if (!EXCLUDED_DIRECTORIES.has(name) && !name.startsWith('.')) return false;
  return !config.include.some((pattern) => pattern.split('/').includes(name));
}

function collectCandidates(
  root: string,
  current: string,
  config: Config,
  warnings: Project['warnings'],
) {
  const candidates: Candidate[] = [];
  let entries;
  try {
    entries = readdirSync(current, { withFileTypes: true }).sort((left, right) =>
      left.name.localeCompare(right.name),
    );
  } catch (error) {
    warnings.push({
      path: pathFor(root, current) || '.',
      message: `Unable to inspect directory: ${error instanceof Error ? error.message : 'unknown error'}`,
    });
    return candidates;
  }

  for (const entry of entries) {
    const absolutePath = join(current, entry.name);
    const path = pathFor(root, absolutePath);
    if (excludedName(entry.name, config)) continue;
    if (entry.isSymbolicLink()) {
      warnings.push({ path, message: 'Skipped symbolic link' });
      continue;
    }
    if (entry.isDirectory()) {
      candidates.push(...collectCandidates(root, absolutePath, config, warnings));
      continue;
    }
    if (entry.isFile()) candidates.push({ absolutePath, path });
  }
  return candidates;
}

function matches(path: string, patternsToMatch: string[]) {
  return patternsToMatch.some((pattern) => new Bun.Glob(pattern).match(path));
}

function selected(candidates: Candidate[], config: Config) {
  return candidates
    .filter(({ path }) => isMarkdownPath(path))
    .filter(({ path }) => matches(path, config.include))
    .filter(({ path }) => !matches(path, config.exclude))
    .sort((left, right) => left.path.localeCompare(right.path));
}

function parseCandidate(candidate: Candidate): ParsedDocument {
  const text = readUtf8(candidate.absolutePath, candidate.path, MAX_SOURCE_BYTES);
  const source = describeMarkdown(candidate.path, text);
  return {
    id: candidate.path,
    path: candidate.path,
    title: source.title,
    text,
    hash: hash(text),
    status: source.status,
    links: [],
    rawLinks: source.links,
  };
}

function warningFor(path: string, error: unknown) {
  return {
    path,
    message: error instanceof Error ? error.message : 'Unable to parse Markdown source',
  };
}

function linkPath(root: string, source: Document, rawLink: string, ids: Set<string>) {
  if (!rawLink || rawLink.startsWith('#') || /^[a-z][a-z0-9+.-]*:/i.test(rawLink)) return null;
  const fragment = rawLink.search(/[?#]/);
  const target = fragment === -1 ? rawLink : rawLink.slice(0, fragment);
  if (!target) return null;
  let decoded: string;
  try {
    decoded = decodeURIComponent(target);
  } catch {
    return null;
  }
  const absoluteTarget = resolve(dirname(join(root, source.path)), decoded);
  const relativeTarget = pathFor(root, absoluteTarget);
  if (
    !relativeTarget ||
    relativeTarget === '.' ||
    relativeTarget.startsWith('../') ||
    isAbsolute(relativeTarget) ||
    !ids.has(relativeTarget)
  )
    return null;
  return relativeTarget;
}

function resolveLinks(root: string, documents: ParsedDocument[]) {
  const ids = new Set(documents.map((document) => document.id));
  for (const document of documents) {
    const links = new Set<string>();
    for (const rawLink of document.rawLinks) {
      const link = linkPath(root, document, rawLink, ids);
      if (link) links.add(link);
    }
    document.links = [...links];
  }
}

function snapshotFor(documents: Document[], config: Config) {
  const identities = documents
    .map((document) => `${document.id}\0${document.hash}`)
    .sort()
    .join('\n');
  const selection = JSON.stringify({
    include: [...config.include].sort(),
    exclude: [...config.exclude].sort(),
    ignoredDirectories: [...PROTECTED_DIRECTORIES, ...EXCLUDED_DIRECTORIES].sort(),
    markdownExtensions: ['.md', '.markdown', '.mdown'],
  });
  return hash(`${identities}\nselection\0${selection}`);
}

function absoluteRoot(root: string) {
  if (!root.trim()) fail('INVALID_ROOT', 'Project root must be a non-empty path');
  const requested = resolve(root);
  try {
    const stat = lstatSync(requested);
    if (stat.isSymbolicLink()) fail('INVALID_ROOT', 'Project root must not be a symlink');
    if (!stat.isDirectory()) fail('INVALID_ROOT', 'Project root must be a directory');
    return requested;
  } catch (error) {
    if (error instanceof HivexError) throw error;
    fail('INVALID_ROOT', 'Project root is not readable', {
      root: requested,
      reason: error instanceof Error ? error.message : 'unknown root failure',
    });
  }
}

export function loadProject(root: string): Project {
  const projectRoot = absoluteRoot(root);
  const config = configFrom(projectRoot);
  const warnings: Project['warnings'] = [];
  const candidates = collectCandidates(projectRoot, projectRoot, config, warnings);
  const selectedCandidates = selected(candidates, config);
  const parsed: ParsedDocument[] = [];
  let sourceBytes = 0;
  for (const candidate of selectedCandidates.slice(0, MAX_DOCUMENTS)) {
    try {
      if (sourceBytes + lstatSync(candidate.absolutePath).size > MAX_CORPUS_BYTES)
        fail(
          'CORPUS_LIMIT',
          'Selected Markdown exceeds the 64 MiB memory budget; narrow include paths',
        );
      const document = parseCandidate(candidate);
      sourceBytes += Buffer.byteLength(document.text);
      parsed.push(document);
    } catch (error) {
      warnings.push(warningFor(candidate.path, error));
    }
  }
  if (selectedCandidates.length > MAX_DOCUMENTS)
    warnings.push({
      path: '.',
      message: `Only the first ${MAX_DOCUMENTS} Markdown sources were loaded`,
    });
  resolveLinks(projectRoot, parsed);
  const documents = parsed.map(({ rawLinks: _rawLinks, ...document }) => document);
  return {
    root: projectRoot,
    snapshot: snapshotFor(documents, config),
    documents,
    warnings,
  };
}

function positiveInteger(value: string | undefined, label: string, fallback?: number) {
  if (value === undefined) {
    if (fallback !== undefined) return fallback;
    fail('INVALID_ARGUMENT', `${label} is required`);
  }
  if (!/^[0-9]+$/.test(value)) fail('INVALID_ARGUMENT', `${label} must be a positive integer`);
  const number = Number(value);
  if (!Number.isSafeInteger(number) || number < 1)
    fail('INVALID_ARGUMENT', `${label} must be positive`);
  return number;
}

function optionalPositiveInteger(value: string | undefined, label: string) {
  if (value === undefined) return undefined;
  return positiveInteger(value, label);
}

function parseCommandArgs(args: string[]) {
  let parsed: ReturnType<typeof parseArgs>;
  try {
    parsed = parseArgs({
      args,
      allowPositionals: true,
      strict: true,
      options: {
        root: { type: 'string' },
        'max-bytes': { type: 'string' },
        limit: { type: 'string' },
        cursor: { type: 'string' },
        from: { type: 'string' },
        to: { type: 'string' },
      },
    });
  } catch (error) {
    fail('INVALID_ARGUMENT', error instanceof Error ? error.message : 'Invalid command arguments');
  }
  return parsed;
}

function validatePositionals(
  command: string | undefined,
  id: string | undefined,
  extra: string | undefined,
): asserts command is 'sources' | 'read' {
  if (command !== 'sources' && command !== 'read')
    fail('INVALID_ARGUMENT', 'Usage: hivex sources | read <id> [options]');
  if (command === 'sources' && (id !== undefined || extra !== undefined))
    fail('INVALID_ARGUMENT', 'sources does not accept a source id');
  if (command === 'read' && id === undefined) fail('INVALID_ARGUMENT', 'read requires a source id');
  if (command === 'read' && extra !== undefined)
    fail('INVALID_ARGUMENT', 'read accepts one source id');
}

function rangeOptions(command: string, values: ParsedValues) {
  if (command === 'sources') {
    if (values.from !== undefined || values.to !== undefined)
      fail('INVALID_ARGUMENT', '--from and --to are only valid for read');
    return { from: undefined, to: undefined };
  }
  if (values.limit !== undefined || values.cursor !== undefined)
    fail('INVALID_ARGUMENT', '--limit and --cursor are only valid for sources');
  return {
    from: optionalPositiveInteger(values.from, '--from'),
    to: optionalPositiveInteger(values.to, '--to'),
  };
}

function commandOptions(args: string[]): {
  command: string;
  id: string | undefined;
  options: CommandOptions;
} {
  const parsed = parseCommandArgs(args);
  const values = parsed.values as ParsedValues;
  const [command, id, extra] = parsed.positionals;
  validatePositionals(command, id, extra);
  const maxBytes = positiveInteger(values['max-bytes'], '--max-bytes', DEFAULT_MAX_BYTES);
  if (maxBytes > MAX_OUTPUT_BYTES)
    fail('INVALID_ARGUMENT', `--max-bytes must be at most ${MAX_OUTPUT_BYTES}`);
  return {
    command,
    id,
    options: {
      root: values.root ?? process.cwd(),
      maxBytes,
      limit: positiveInteger(values.limit, '--limit', 20),
      cursor: values.cursor,
      ...rangeOptions(command, values),
    },
  };
}

function metadata(document: Document) {
  const { text: _text, ...result } = document;
  return result;
}

function linesFor(text: string) {
  return { lines: rawMarkdownLines(text) };
}

function boundedLines(window: { lines: string[]; start: number; end: number; maxBytes: number }) {
  const { lines, start, end, maxBytes } = window;
  let text = '';
  let prefix = '';
  let lineEnd = start - 1;
  for (let line = start; line <= end; line++) {
    const raw = lines[line - 1] ?? '';
    const current = line === lines.length ? raw : lineContent(raw);
    const next = prefix + current;
    if (Buffer.byteLength(next) > maxBytes) {
      if (lineEnd < start)
        fail('OUTPUT_LIMIT', 'The first requested line exceeds --max-bytes', {
          line,
          maxBytes,
          requiredBytes: Buffer.byteLength(next),
        });
      return { text, lineEnd };
    }
    text = next;
    prefix += raw;
    lineEnd = line;
  }
  return { text, lineEnd };
}

function continuationFor(
  lineEnd: number,
  totalLines: number,
  requestedEnd: number,
  maxBytes: number,
) {
  if (lineEnd >= totalLines) return null;
  let reason = 'range';
  if (lineEnd < requestedEnd) reason = 'max-bytes';
  return {
    from: lineEnd + 1,
    to: totalLines,
    reason,
    maxBytes,
  };
}

function readCommand(project: Project, id: string, options: CommandOptions) {
  const source = project.documents.find((document) => document.id === id);
  if (!source) fail('SOURCE_NOT_FOUND', `Markdown source was not selected: ${id}`, { id });
  const { lines } = linesFor(source.text);
  const start = options.from ?? 1;
  const requestedEnd = options.to ?? lines.length;
  if (start > lines.length || requestedEnd > lines.length || start > requestedEnd)
    fail('INVALID_RANGE', `Line range ${start}-${requestedEnd} is outside the source`, {
      id,
      lineCount: lines.length,
    });
  const bounded = boundedLines({
    lines,
    start,
    end: requestedEnd,
    maxBytes: options.maxBytes,
  });
  const continuation = continuationFor(
    bounded.lineEnd,
    lines.length,
    requestedEnd,
    options.maxBytes,
  );
  return {
    command: 'read',
    origin: ORIGIN,
    snapshot: project.snapshot,
    source: metadata(source),
    text: bounded.text,
    lineStart: start,
    lineEnd: bounded.lineEnd,
    continuation,
    truncated: continuation !== null,
    warnings: project.warnings,
  };
}

function listSources(project: Project, options: CommandOptions) {
  const cursor = options.cursor?.match(/^s1\.([a-f0-9]{64})\.([0-9]+)$/);
  if (options.cursor !== undefined && (!cursor || cursor[1] !== project.snapshot))
    fail('INVALID_CURSOR', 'Source continuation belongs to a different or invalid snapshot');
  const start = Number(cursor?.[2] ?? 0);
  if (!Number.isSafeInteger(start) || start < 0 || (start > 0 && start >= project.documents.length))
    fail('INVALID_CURSOR', 'Source continuation is outside this snapshot');
  const documents: ReturnType<typeof metadata>[] = [];
  const response = () => ({
    command: 'sources',
    origin: ORIGIN,
    snapshot: project.snapshot,
    documents,
    totalDocuments: project.documents.length,
    continuation:
      start + documents.length < project.documents.length
        ? `s1.${project.snapshot}.${start + documents.length}`
        : null,
    warnings: project.warnings,
  });
  for (const document of project.documents.slice(
    start,
    start + Math.min(options.limit, MAX_DOCUMENTS),
  )) {
    documents.push(metadata(document));
    if (Buffer.byteLength(JSON.stringify(response())) <= options.maxBytes) continue;
    documents.pop();
    if (!documents.length)
      fail(
        'OUTPUT_LIMIT',
        'The next source metadata does not fit; increase --max-bytes or narrow the selected sources',
      );
    break;
  }
  if (Buffer.byteLength(JSON.stringify(response())) > options.maxBytes)
    fail('OUTPUT_LIMIT', 'Source-list metadata exceeds --max-bytes');
  return response();
}

export function documentCommand(args: string[]): unknown {
  const { command, id, options } = commandOptions(args);
  const project = loadProject(options.root);
  if (command === 'sources') return listSources(project, options);
  return readCommand(project, id ?? '', options);
}
