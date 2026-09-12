import { spawnSync } from "node:child_process";
import { lstatSync, readFileSync, readlinkSync, realpathSync } from "node:fs";
import pathModule from "node:path";
import { compareSerializedStrings } from "./ordering.ts";
import { HivexError } from "./errors.ts";
import { digest } from "./knowledge-model.ts";
import { rawMarkdownLines, lineContent } from "./markdown.ts";

interface Version {
  version: string;
  lines: [number, string][];
}
export interface Implementation {
  baseCommit: string;
  fingerprint: string;
  diff: string;
  files: { path: string; before: Version | null; after: Version | null }[];
  warnings: string[];
}
interface GitContext {
  base: string;
  root: string;
  warnings: string[];
}
const maxBytes = 256 * 1024;
const maxFileBytes = 4 * 1024 * 1024;
const protectedDirectories = new Set([
  ".git",
  ".hivex",
  "node_modules",
  ".codex",
]);
const gitOptionalLocks = "GIT_OPTIONAL_LOCKS";
const localeAll = "LC_ALL";
const decoder = new TextDecoder("utf-8", { fatal: true });

const isMissingFile = function isMissingFile(error: unknown) {
  return (
    error !== null &&
    typeof error === "object" &&
    "code" in error &&
    error.code === "ENOENT"
  );
};

const git = function git(root: string, gitArguments: string[]) {
  const executable = Bun.which("git") ?? "git";
  const result = spawnSync(
    executable,
    ["--literal-pathspecs", ...gitArguments],
    {
      cwd: root,
      env: {
        ...process.env,
        [gitOptionalLocks]: "0",
        [localeAll]: "C",
      },
      maxBuffer: maxFileBytes + 1,
      timeout: 30_000,
    }
  );
  if (result.error || result.status !== 0) {
    const isTooLarge =
      result.error !== undefined &&
      "code" in result.error &&
      result.error.code === "ENOBUFS";
    throw new HivexError({
      code: isTooLarge ? "IMPLEMENTATION_TOO_LARGE" : "GIT_COMMAND_FAILED",
      message:
        result.error?.message ??
        result.stderr.toString("utf-8").trim().slice(0, 1024),
    });
  }
  return result.stdout;
};

const text = function text(root: string, gitArguments: string[]) {
  return decoder.decode(git(root, gitArguments));
};

const checkSize = function checkSize(bytes: number, limit = maxBytes) {
  if (bytes > limit) {
    throw new HivexError({
      code: "IMPLEMENTATION_TOO_LARGE",
      message: `Implementation exceeds ${limit} bytes; split the change into coherent reviews.`,
    });
  }
};

const unsupportedVersion = function unsupportedVersion(
  bytes: Buffer,
  label: string,
  warnings: string[]
): null {
  warnings.push(
    `Unsupported binary or invalid UTF-8 content: ${label} (${digest(bytes.toBase64())})`
  );
  return null;
};

const version = function version(
  bytes: Buffer,
  label: string,
  warnings: string[]
): Version | null {
  checkSize(bytes.byteLength, maxFileBytes);
  let content: string;
  try {
    content = decoder.decode(bytes);
  } catch {
    return unsupportedVersion(bytes, label, warnings);
  }
  if (content.includes("\u{0}")) {
    return unsupportedVersion(bytes, label, warnings);
  }
  return {
    lines: rawMarkdownLines(content).map<[number, string]>((line, index) => {
      const lineNumber = index + 1;
      return [lineNumber, lineContent(line)];
    }),
    version: digest(bytes),
  };
};

const beforeVersion = function beforeVersion(
  context: GitContext,
  path: string
) {
  const { base, root, warnings } = context;
  const entry = text(root, ["ls-tree", "-z", base, "--", path])
    .split("\0")
    .find((row) => row.slice(row.indexOf("\t") + 1) === path);
  if (entry === undefined) {
    return null;
  }
  const header = entry.slice(0, entry.indexOf("\t"));
  const [mode, kind, object] = header.split(" ", 3);
  if (kind !== "blob" || mode === undefined || object === undefined) {
    warnings.push(`Unsupported base file: ${path} (${mode} ${object})`);
    return null;
  }
  if (!mode.startsWith("100")) {
    warnings.push(`Unsupported base file: ${path} (${mode} ${object})`);
    return null;
  }
  return version(
    git(root, ["cat-file", "blob", object]),
    `before ${path}`,
    warnings
  );
};

const afterVersion = function afterVersion(context: GitContext, path: string) {
  const { root, warnings } = context;
  const absolute = pathModule.resolve(root, path);
  let stat;
  try {
    stat = lstatSync(absolute);
  } catch (error) {
    if (isMissingFile(error)) {
      return null;
    }
    throw error;
  }
  if (stat.isSymbolicLink()) {
    warnings.push(
      `Unsupported working symlink: ${path} (${digest(readlinkSync(absolute))})`
    );
    return null;
  }
  if (!stat.isFile() || !realpathSync(absolute).startsWith(`${root}/`)) {
    warnings.push(`Unsupported working file: ${path}`);
    return null;
  }
  checkSize(stat.size, maxFileBytes);
  return version(readFileSync(absolute), `after ${path}`, warnings);
};

const patch = function patch(root: string, base: string, paths: string[]) {
  if (!paths.length) {
    return "";
  }
  return text(root, [
    "diff",
    "--no-ext-diff",
    "--no-textconv",
    "--no-renames",
    "--no-color",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "--unified=3",
    base,
    "--",
    ...paths,
  ]);
};

const fileContext = function fileContext(
  context: GitContext,
  file: Implementation["files"][number]
) {
  const { base, root, warnings } = context;
  if (Buffer.byteLength(JSON.stringify(file)) <= 32_768) {
    return file;
  }
  const hunks = patch(root, base, [file.path])
    .matchAll(
      /^@@ -(?<beforeStart>\d+)(?:,(?<beforeLength>\d+))? \+(?<afterStart>\d+)(?:,(?<afterLength>\d+))? @@/gmu
    )
    .toArray();
  if (!hunks.length) {
    return file;
  }
  const excerpt = function excerpt(
    documentVersion: Version | null,
    startGroup: "beforeStart" | "afterStart",
    lengthGroup: "beforeLength" | "afterLength"
  ) {
    if (documentVersion === null) {
      return null;
    }
    const isLineInHunk = function isLineInHunk([line]: [number, string]) {
      return hunks.some((hunk) => {
        const { groups } = hunk;
        if (groups === undefined) {
          return false;
        }
        const start = Number(groups[startGroup]);
        const length = Number(groups[lengthGroup] ?? 1);
        return line >= start && line < start + length;
      });
    };
    return {
      lines: documentVersion.lines.filter(isLineInHunk),
      version: documentVersion.version,
    };
  };
  warnings.push(
    `Only changed ranges are supplied for ${file.path}; unchanged code is omitted.`
  );
  return {
    ...file,
    after: excerpt(file.after, "afterStart", "afterLength"),
    before: excerpt(file.before, "beforeStart", "beforeLength"),
  };
};

export const captureImplementation = function captureImplementation(
  root: string,
  base: string
): Implementation {
  const actualRoot = realpathSync(pathModule.resolve(root));
  if (
    realpathSync(text(actualRoot, ["rev-parse", "--show-toplevel"]).trim()) !==
    actualRoot
  ) {
    throw new HivexError({
      code: "INVALID_ROOT",
      message: "Review from the Git project root.",
    });
  }
  const baseCommit = text(actualRoot, [
    "rev-parse",
    "--verify",
    "--end-of-options",
    base.concat("^{commit}"),
  ]).trim();
  const tracked = text(actualRoot, [
    "diff",
    "--no-ext-diff",
    "--no-textconv",
    "--no-renames",
    "--name-only",
    "-z",
    baseCommit,
    "--",
  ]).split("\0");
  const untracked = text(actualRoot, [
    "ls-files",
    "--others",
    "--exclude-standard",
    "-z",
  ]).split("\0");
  const changedPaths = new Set(Iterator.concat(tracked, untracked));
  const paths = changedPaths
    .values()
    .filter((path) => {
      const isOutsideProtectedDirectory = path
        .split("/")
        .every((part) => !protectedDirectories.has(part));
      return path.length > 0 && isOutsideProtectedDirectory;
    })
    .toArray()
    .toSorted(compareSerializedStrings);
  if (paths.length > 64) {
    throw new HivexError({
      code: "IMPLEMENTATION_TOO_LARGE",
      message:
        "Implementation exceeds 64 files; split the change into coherent reviews.",
    });
  }
  const warnings: string[] = [];
  const context = { base: baseCommit, root: actualRoot, warnings };
  const files = paths.map((path) => {
    const before = beforeVersion(context, path);
    const after = afterVersion(context, path);
    return fileContext(context, { after, before, path });
  });
  let diff = patch(actualRoot, baseCommit, paths);
  diff += paths
    .filter((path) => untracked.includes(path))
    .map((path) => `\nNew untracked file: ${JSON.stringify(path)}\n`)
    .join("");
  const packet = { baseCommit, diff, files, warnings };
  checkSize(Buffer.byteLength(JSON.stringify(packet)));
  // Retain the nested field order of the original persisted fingerprint format.
  const fields = [
    "baseCommit",
    "diff",
    "files",
    "warnings",
    "path",
    "before",
    "after",
    "version",
    "lines",
  ];
  return { ...packet, fingerprint: digest(JSON.stringify(packet, fields)) };
};
