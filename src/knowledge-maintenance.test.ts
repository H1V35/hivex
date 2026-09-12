import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import nodePath from "node:path";
import { Database } from "bun:sqlite";
import { expect, test } from "bun:test";
import { emptyGraph } from "./knowledge-model.ts";
import { knowledgeMaintenance } from "./knowledge-maintenance.ts";
import { KnowledgeStore } from "./knowledge-store.ts";

type JsonRecord = Record<string, unknown>;

interface StoredAttempt {
  recoveryAcknowledgement?: JsonRecord;
  report?: JsonRecord;
  result?: unknown;
}

interface StoredWork {
  attempts: StoredAttempt[];
  calls: number;
  inputBytes: number;
  nativeProcessId?: number;
  remaining: string[];
  status: string;
  totalTokens: number;
}

const isRecord = (value: unknown): value is JsonRecord =>
  typeof value === "object" && value !== null && !Array.isArray(value);

const isStringArray = (value: unknown): value is string[] =>
  Array.isArray(value) && value.every((entry) => typeof entry === "string");

const numberField = (value: unknown, name: string): number => {
  if (!isRecord(value) || typeof value[name] !== "number") {
    throw new Error(`Expected numeric field ${name}`);
  }
  return value[name];
};

const isStoredAttempt = function isStoredAttempt(
  value: unknown
): value is StoredAttempt {
  if (!isRecord(value)) {
    return false;
  }
  if (value.report !== undefined && !isRecord(value.report)) {
    return false;
  }
  return (
    value.recoveryAcknowledgement === undefined ||
    isRecord(value.recoveryAcknowledgement)
  );
};

const isStoredWork = function isStoredWork(
  value: unknown
): value is StoredWork {
  if (!isRecord(value)) {
    return false;
  }
  if (
    !Array.isArray(value.attempts) ||
    !value.attempts.every(isStoredAttempt)
  ) {
    return false;
  }
  if (typeof value.calls !== "number" || typeof value.inputBytes !== "number") {
    return false;
  }
  if (
    value.nativeProcessId !== undefined &&
    typeof value.nativeProcessId !== "number"
  ) {
    return false;
  }
  return (
    isStringArray(value.remaining) &&
    typeof value.status === "string" &&
    typeof value.totalTokens === "number"
  );
};

const parseJson = (text: string): unknown => JSON.parse(text);

const temporaryProject = function temporaryProject(
  run: (root: string) => void
) {
  const root = mkdtempSync(nodePath.join(tmpdir(), "hivex-maintenance-"));
  try {
    run(root);
  } finally {
    rmSync(root, { force: true, recursive: true });
  }
};

const deadPid = function deadPid() {
  for (let processId = 2_000_000_000; processId > 1_000_000; processId -= 1) {
    try {
      process.kill(processId, 0);
    } catch (error) {
      if (Error.isError(error) && "code" in error && error.code === "ESRCH") {
        return processId;
      }
    }
  }
  throw new Error("Could not find a dead PID for the fixture");
};

const storedWork = function storedWork(root: string, id: string): StoredWork {
  using database = new Database(
    nodePath.join(root, ".hivex", "knowledge.sqlite"),
    {
      readonly: true,
    }
  );
  const row = database
    .query<{ data: string }, [string]>("SELECT data FROM work WHERE id=?")
    .get(id);
  if (!row) {
    throw new Error(`Missing fixture work ${id}`);
  }
  const value = parseJson(row.data);
  if (!isStoredWork(value)) {
    throw new Error(`Malformed fixture work ${id}`);
  }
  return value;
};

test("recover marks a dead native invocation failed and preserves its work state", () => {
  temporaryProject((root) => {
    let id: string;
    {
      using store = new KnowledgeStore(root);
      const work = store.begin({
        key: "fixture",
        kind: "ask",
        remaining: [],
        snapshot: "fixture",
      });
      store.reserve(work, {
        inputBytes: 123,
        inputHash: "input-hash",
        stage: "extract",
      });
      const attempt = work.attempts.at(-1);
      if (!attempt) {
        throw new Error("Fixture did not reserve an attempt");
      }
      attempt.result = { retained: true };
      work.ownerPid = deadPid();
      store.recordNativeProcess(work, deadPid());
      ({ id } = work);
    }

    const inspection = knowledgeMaintenance(["recover", "--root", root]);
    expect(inspection).toMatchObject({
      command: "recover",
      interruptedWorks: 0,
      lock: "absent",
      modelCalls: 0,
      status: "blocked",
    });

    const result = knowledgeMaintenance([
      "recover",
      "--acknowledge-uncertain",
      "--root",
      root,
    ]);
    expect(result).toMatchObject({
      acknowledgedWorks: 1,
      command: "recover",
      interruptedWorks: 1,
      lock: "absent",
      modelCalls: 0,
      status: "recovered",
    });
    expect(existsSync(nodePath.join(root, ".hivex", "knowledge.lock"))).toBe(
      false
    );
    const work = storedWork(root, id);
    expect(work).toMatchObject({
      calls: 1,
      inputBytes: 123,
      remaining: [],
      status: "failed",
    });
    expect(work.nativeProcessId).toBeUndefined();
    const [firstAttempt] = work.attempts;
    expect(firstAttempt?.recoveryAcknowledgement).toMatchObject({
      type: "uncertain-invocation",
    });
    expect(
      numberField(firstAttempt?.recoveryAcknowledgement, "nativeProcessId")
    ).toBeGreaterThan(0);
    const second = knowledgeMaintenance(["recover", "--root", root]);
    expect(second).toMatchObject({ acknowledgedWorks: 0, modelCalls: 0 });
    writeFileSync(
      nodePath.join(root, ".hivex", "knowledge.lock"),
      JSON.stringify({ id: "later-dead-owner", pid: deadPid() })
    );
    expect(knowledgeMaintenance(["recover", "--root", root])).toMatchObject({
      acknowledgedWorks: 0,
      status: "recovered",
    });
    expect(existsSync(nodePath.join(root, ".hivex", "knowledge.lock"))).toBe(
      false
    );
    expect(work.attempts).toHaveLength(1);
    expect(firstAttempt?.result).toEqual({ retained: true });
    expect(firstAttempt?.report).toMatchObject({
      cleanup: "not-observed",
      outcome: "interrupted",
      turnAccepted: "unknown",
      usage: null,
    });
  });
});

test("acknowledges a saved uncertain failure without rewriting its report", () => {
  temporaryProject((root) => {
    let id: string;
    let originalReport: Record<string, unknown>;
    {
      using store = new KnowledgeStore(root);
      const work = store.begin({
        key: "saved-failure",
        kind: "ask",
        remaining: [],
        snapshot: "fixture",
      });
      store.reserve(work, {
        inputBytes: 17,
        inputHash: "input-hash",
        stage: "ask",
      });
      const attempt = work.attempts.at(-1);
      if (!attempt) {
        throw new Error("Fixture did not reserve an attempt");
      }
      work.ownerPid = deadPid();
      originalReport = {
        cleanup: "not-observed",
        code: "MODEL_TIMEOUT",
        interruption: "unconfirmed",
        nativeProcessId: deadPid(),
        outcome: "failed",
        turnAccepted: "unknown",
        usage: null,
      };
      attempt.report = originalReport;
      work.status = "failed";
      store.save(work);
      ({ id } = work);
    }

    expect(knowledgeMaintenance(["recover", "--root", root])).toMatchObject({
      acknowledgedWorks: 0,
      lock: "absent",
      status: "blocked",
    });
    const result = knowledgeMaintenance([
      "recover",
      "--acknowledge-uncertain",
      "--root",
      root,
    ]);
    expect(result).toMatchObject({
      acknowledgedWorks: 1,
      interruptedWorks: 0,
      lock: "absent",
      modelCalls: 0,
      status: "recovered",
    });
    const work = storedWork(root, id);
    const [firstAttempt] = work.attempts;
    expect(firstAttempt?.report).toEqual(originalReport);
    expect(firstAttempt?.recoveryAcknowledgement).toMatchObject({
      nativeProcessId: originalReport.nativeProcessId,
      type: "uncertain-invocation",
    });
  });
});

test("recover leaves a live owner and its work untouched", () => {
  temporaryProject((root) => {
    let id: string;
    {
      using store = new KnowledgeStore(root);
      const work = store.begin({
        key: "fixture",
        kind: "update",
        remaining: ["unit-1"],
        snapshot: "fixture",
      });
      store.reserve(work, {
        inputBytes: 10,
        inputHash: "input-hash",
        stage: "extract",
      });
      store.recordNativeProcess(work, process.pid);
      ({ id } = work);
    }
    writeFileSync(
      nodePath.join(root, ".hivex", "knowledge.lock"),
      JSON.stringify({ id: "live-owner", pid: process.pid })
    );

    const result = knowledgeMaintenance(["recover", "--root", root]);
    expect(result).toMatchObject({
      interruptedWorks: 0,
      lock: "held",
      modelCalls: 0,
      status: "blocked",
    });
    expect(existsSync(nodePath.join(root, ".hivex", "knowledge.lock"))).toBe(
      true
    );
    expect(storedWork(root, id)).toMatchObject({ calls: 1, status: "running" });
  });
});

test("prune removes only old completed work and caches", () => {
  temporaryProject((root) => {
    let unfinishedId: string;
    {
      using store = new KnowledgeStore(root);
      store.saveGraph(emptyGraph());
      const unfinished = store.begin({
        key: "unfinished",
        kind: "update",
        remaining: ["unit-1"],
        snapshot: "fixture",
      });
      unfinished.calls = 4;
      unfinished.inputBytes = 321;
      unfinished.totalTokens = 19;
      unfinished.status = "failed";
      unfinished.attempts.push({
        inputBytes: 7,
        inputHash: "retained",
        result: { retained: true },
        stage: "extract",
      });
      store.save(unfinished);
      unfinishedId = unfinished.id;

      for (const key of ["done-1", "done-2", "done-3"]) {
        const completed = store.begin({
          key,
          kind: "ask",
          remaining: [],
          snapshot: "fixture",
        });
        completed.status = "done";
        completed.result = { key };
        store.save(completed);
      }
      store.cache("cache-1", { value: 1 });
      store.cache("cache-2", { value: 2 });
      store.cache("cache-3", { value: 3 });
    }

    const result = knowledgeMaintenance([
      "prune",
      "--root",
      root,
      "--keep-completed",
      "1",
      "--keep-caches",
      "1",
    ]);
    expect(result).toMatchObject({
      command: "prune",
      deletedCaches: 2,
      deletedCompletedWorks: 2,
      modelCalls: 0,
      retainedCaches: 1,
      retainedCompletedWorks: 1,
      unfinishedWorks: 1,
    });
    expect(storedWork(root, unfinishedId)).toMatchObject({
      attempts: [expect.objectContaining({ result: { retained: true } })],
      calls: 4,
      inputBytes: 321,
      remaining: ["unit-1"],
      status: "failed",
      totalTokens: 19,
    });
    using store = new KnowledgeStore(root);
    expect(store.graph()).toEqual(emptyGraph());
  });
});

test("recover handles a dead owner before the native turn starts, without an acknowledgement", () => {
  temporaryProject((root) => {
    let id: string;
    {
      using store = new KnowledgeStore(root);
      const work = store.begin({
        key: "before-turn",
        kind: "ask",
        remaining: [],
        snapshot: "fixture",
      });
      store.reserve(work, {
        inputBytes: 40,
        inputHash: "input",
        stage: "ask",
      });
      work.ownerPid = deadPid();
      store.save(work);
      ({ id } = work);
    }
    const result = knowledgeMaintenance(["recover", "--root", root]);
    expect(result).toMatchObject({
      acknowledgedWorks: 0,
      interruptedWorks: 1,
      modelCalls: 0,
      status: "recovered",
    });
    expect(storedWork(root, id)).toMatchObject({
      attempts: [
        {
          report: {
            code: "MODEL_INTERRUPTED_BEFORE_TURN",
            usage: null,
          },
        },
      ],
      calls: 1,
      inputBytes: 40,
      status: "failed",
    });
  });
});
