import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { Database } from "bun:sqlite";
import { expect, test } from "bun:test";
import { knowledgeCommand } from "./knowledge.ts";

const fixture = readFileSync(
  new URL("../test/fixtures/knowledge-cache-v1.sql", import.meta.url),
  "utf-8"
);

test.each([
  { cacheHits: 0, state: "completed" },
  { cacheHits: 1, state: "model-cache" },
])(
  "retains a v1 $state answer with its exhausted budget",
  async ({ cacheHits, state }) => {
    using cleanup = new DisposableStack();
    const root = mkdtempSync(path.join(tmpdir(), "hivex-cache-compat-"));
    cleanup.defer(() => {
      rmSync(root, { force: true, recursive: true });
    });
    writeFileSync(
      path.join(root, "notes.md"),
      "# Policy\nUse bounded work.\nPreserve the budget.\n"
    );
    mkdirSync(path.join(root, ".hivex"));
    using database = new Database(path.join(root, ".hivex/knowledge.sqlite"));
    database.run(fixture);
    if (state === "model-cache") {
      database.run(
        "UPDATE work SET data=json_remove(json_set(data, '$.status', 'pending'), '$.result')"
      );
    }
    const result = await knowledgeCommand([
      "ask",
      "bounded",
      "--source",
      "notes.md",
      "--root",
      root,
      "--max-calls",
      "1",
      "--codex",
      path.join(root, "model-must-not-start"),
    ]);
    expect(result).toMatchObject({
      answer: "Use bounded work and preserve the budget.",
      status: "ready",
      work: {
        cacheHits,
        calls: 1,
        id: "84171802-e68b-43d8-b327-9ff47d302375",
        inputBytes: 100,
        maxCalls: 1,
        maxInputBytes: 131_072,
        totalTokens: 10,
      },
    });
  }
);
