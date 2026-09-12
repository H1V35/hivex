import { mkdtempSync, readdirSync, realpathSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { z } from "zod";
import { startServer } from "./server.ts";
import { knowledgeTurn } from "./profile.ts";
import { startKnowledgeThread } from "./thread.ts";
import { failureDiagnostic, ServerAdmissionFailure } from "./failure.ts";
import { captureTranscript } from "./transcript.ts";
import type { ProfileEvidence } from "./profile.ts";
import type { Usage } from "./transcript.ts";

export type NativeProcessStarted = (nativeProcessId: number) => void;

export interface InvocationOptions {
  binary: string;
  prompt: string;
  schema: Record<string, unknown>;
  deadlineMilliseconds: number;
  onNativeProcessStarted?: NativeProcessStarted;
}
export interface InvocationReport {
  outcome: string;
  code?: string;
  deadlineMilliseconds: number;
  interruption?: string;
  usage: Usage | null;
  turnAccepted?: "confirmed" | "unknown";
  threadId?: string;
  turnId?: string;
  cleanup?: "confirmed" | "failed" | "not-observed";
  startedAt?: string;
  durationMilliseconds?: number;
  admission?: ProfileEvidence & { launchPolicyHash: string };
  diagnostic?: Record<string, unknown>;
  nativeProcessId?: number;
}

class ModelAbortError extends Error {
  name = "ModelAbortError";
  readonly kind: "cancelled" | "timeout";

  constructor(kind: "cancelled" | "timeout", options?: ErrorOptions) {
    super("", options);
    this.kind = kind;
  }
}

const completedWithin = async (options: {
  captured: ReturnType<typeof captureTranscript>;
  server: Awaited<ReturnType<typeof startServer>>;
  milliseconds: number;
  signal?: AbortSignal;
}) => {
  const expired = Promise.withResolvers<never>();
  const cancelled = (): void => {
    expired.reject(new ModelAbortError("cancelled"));
  };
  options.signal?.addEventListener("abort", cancelled, { once: true });
  if (options.signal?.aborted === true) {
    cancelled();
  }
  const timer = setTimeout(() => {
    expired.reject(new ModelAbortError("timeout"));
  }, options.milliseconds);
  try {
    return await Promise.race([
      options.captured.done.promise,
      options.server.rpc.closed,
      expired.promise,
    ]);
  } finally {
    clearTimeout(timer);
    options.signal?.removeEventListener("abort", cancelled);
  }
};

const isInterruptedTurn = function isInterruptedTurn(
  end: Awaited<ReturnType<typeof completedWithin>>,
  threadId: string,
  turnId: string
) {
  return (
    end.threadId === threadId &&
    end.turn.id === turnId &&
    end.turn.status === "interrupted"
  );
};

const isInterruptRequestAccepted = async (options: {
  server: Awaited<ReturnType<typeof startServer>>;
  threadId: string;
  turnId: string;
}) => {
  try {
    await options.server.rpc.request(
      "turn/interrupt",
      { threadId: options.threadId, turnId: options.turnId },
      { timeoutMilliseconds: 5000 }
    );
    return true;
  } catch {
    return false;
  }
};

const isInterrupted = async (options: {
  captured: ReturnType<typeof captureTranscript>;
  server: Awaited<ReturnType<typeof startServer>>;
  threadId: string;
  turnId: string;
}) => {
  const isInterruptAccepted = await isInterruptRequestAccepted(options);
  if (!isInterruptAccepted) {
    return false;
  }
  try {
    const end = await completedWithin({
      captured: options.captured,
      milliseconds: 5000,
      server: options.server,
    });
    return isInterruptedTurn(end, options.threadId, options.turnId);
  } catch {
    return false;
  }
};

const turnStartResponse = z.looseObject({
  turn: z.looseObject({ id: z.string() }),
});

interface RunTurnOptions extends InvocationOptions {
  captured: ReturnType<typeof captureTranscript>;
  server: Awaited<ReturnType<typeof startServer>>;
  signal: AbortSignal;
  threadId: string;
  workspace: string;
}

interface InvocationResult {
  value: unknown;
  report: InvocationReport;
  retry: boolean;
}

const createInvocationResult = (
  value: unknown,
  report: InvocationReport,
  shouldRetry: boolean
): InvocationResult => {
  const valueField = { value };
  const reportField = { report };
  const retryField = { retry: shouldRetry };
  return { ...valueField, ...reportField, ...retryField };
};

const failureCode = (options: { isCancelled: boolean; isTimeout: boolean }) => {
  if (options.isCancelled) {
    return "MODEL_CANCELLED";
  }
  return options.isTimeout ? "MODEL_TIMEOUT" : "MODEL_PROTOCOL_FAILED";
};

const completeTurn = async (
  options: RunTurnOptions & { begin: number; turnId: string }
) => {
  if (options.signal.aborted) {
    throw new ModelAbortError("cancelled");
  }
  const remaining =
    options.deadlineMilliseconds - (performance.now() - options.begin);
  if (remaining <= 0) {
    throw new ModelAbortError("timeout");
  }
  const end = await completedWithin({
    captured: options.captured,
    milliseconds: remaining,
    server: options.server,
    signal: options.signal,
  });
  if (
    end.threadId !== options.threadId ||
    end.turn.id !== options.turnId ||
    end.turn.status !== "completed"
  ) {
    throw new Error("Model completion was not established");
  }
  options.captured.assertValid({
    threadId: options.threadId,
    turnId: options.turnId,
  });
  const final = options.captured.items.at(-1);
  const text = final?.item.text;
  if (
    text === undefined ||
    text === "" ||
    final?.threadId !== options.threadId ||
    final.turnId !== options.turnId
  ) {
    throw new Error("Structured model output is missing");
  }
  if (readdirSync(options.workspace).length !== 0) {
    throw new Error("Knowledge workspace was mutated");
  }
  return text;
};

const runTurn = async (options: RunTurnOptions) => {
  const { server, captured, threadId } = options;
  const begin = performance.now();
  const inputTextType = { type: "text" };
  const inputText = { text: options.prompt };
  const turnInput = {
    ...inputTextType,
    ...inputText,
  };
  const turnThread = { threadId };
  const turnInputParameter = { input: [turnInput] };
  const turnOutputSchema = { outputSchema: options.schema };
  const turnParameters = {
    ...turnThread,
    ...knowledgeTurn,
    ...turnInputParameter,
    ...turnOutputSchema,
  };
  let accepted: z.infer<typeof turnStartResponse> | null;
  try {
    const response = await server.rpc.request("turn/start", turnParameters, {
      timeoutMilliseconds: Math.min(options.deadlineMilliseconds, 30_000),
    });
    accepted = turnStartResponse.parse(response);
  } catch {
    accepted = null;
  }
  if (accepted === null) {
    const reportOutcome = { outcome: "failed" };
    const reportCode = { code: "MODEL_START_UNCONFIRMED" };
    const reportTurnAccepted = { turnAccepted: "unknown" as const };
    const reportThreadId = { threadId };
    const reportDeadlineMilliseconds = {
      deadlineMilliseconds: options.deadlineMilliseconds,
    };
    const reportUsage = { usage: null };
    const report: InvocationReport = {
      ...reportOutcome,
      ...reportCode,
      ...reportTurnAccepted,
      ...reportThreadId,
      ...reportDeadlineMilliseconds,
      ...reportUsage,
    };
    return createInvocationResult(null, report, false);
  }
  const turnId = accepted.turn.id;
  try {
    const value = await completeTurn({ ...options, begin, turnId });
    const reportOutcome = { outcome: "completed" };
    const reportThreadId = { threadId };
    const reportTurnId = { turnId };
    const reportTurnAccepted = { turnAccepted: "confirmed" as const };
    const reportDeadlineMilliseconds = {
      deadlineMilliseconds: options.deadlineMilliseconds,
    };
    const reportUsage = { usage: captured.measured({ threadId, turnId }) };
    const report: InvocationReport = {
      ...reportOutcome,
      ...reportThreadId,
      ...reportTurnId,
      ...reportTurnAccepted,
      ...reportDeadlineMilliseconds,
      ...reportUsage,
    };
    return createInvocationResult(value, report, false);
  } catch (error) {
    const isInterruptionConfirmed = await isInterrupted({
      ...options,
      turnId,
    });
    const isTimeout =
      error instanceof ModelAbortError && error.kind === "timeout";
    const isCancelled =
      error instanceof ModelAbortError && error.kind === "cancelled";
    const reportOutcome = {
      outcome: isTimeout ? "timeout" : "failed",
    };
    const reportThreadId = { threadId };
    const reportTurnId = { turnId };
    const reportCode = { code: failureCode({ isCancelled, isTimeout }) };
    const reportInterruption = {
      interruption: isInterruptionConfirmed ? "confirmed" : "unconfirmed",
    };
    const reportTurnAccepted = { turnAccepted: "confirmed" as const };
    const reportDeadlineMilliseconds = {
      deadlineMilliseconds: options.deadlineMilliseconds,
    };
    const reportUsage = { usage: captured.measured({ threadId, turnId }) };
    const report: InvocationReport = {
      ...reportOutcome,
      ...reportThreadId,
      ...reportTurnId,
      ...reportCode,
      ...reportInterruption,
      ...reportTurnAccepted,
      ...reportDeadlineMilliseconds,
      ...reportUsage,
    };
    return createInvocationResult(
      null,
      report,
      isTimeout && isInterruptionConfirmed
    );
  }
};

const captureFailure = async <Value>(promise: Promise<Value>) => {
  try {
    return { value: await promise };
  } catch (error) {
    return { error };
  }
};

interface InvocationResource {
  server?: Awaited<ReturnType<typeof startServer>>;
  result: InvocationResult;
}

export const invokeModel = async (options: InvocationOptions) => {
  const currentTime = new Date();
  const startedAt = currentTime.toISOString();
  const began = performance.now();
  const workspace = realpathSync(
    mkdtempSync(path.join(tmpdir(), "hivex-model-"))
  );
  const captured = captureTranscript();
  const controller = new AbortController();
  const cancel = (): void => {
    controller.abort();
  };
  process.on("SIGINT", cancel);
  process.on("SIGTERM", cancel);
  const initialReportOutcome = { outcome: "failed" };
  const initialReportCode = { code: "MODEL_ADMISSION_FAILED" };
  const initialReportDeadlineMilliseconds = {
    deadlineMilliseconds: options.deadlineMilliseconds,
  };
  const initialReportUsage = { usage: null };
  const initialReport: InvocationReport = {
    ...initialReportOutcome,
    ...initialReportCode,
    ...initialReportDeadlineMilliseconds,
    ...initialReportUsage,
  };
  const resource: InvocationResource = {
    result: createInvocationResult(null, initialReport, false),
  };
  const execution = await captureFailure(
    (async () => {
      const server = await startServer({
        binary: options.binary,
        interaction: () => {
          throw new Error("Knowledge execution cannot request interaction");
        },
        notification: captured.notification,
        signal: controller.signal,
        workspace,
      });
      resource.server = server;
      if (options.onNativeProcessStarted !== undefined) {
        options.onNativeProcessStarted(server.pid);
      }
      const threadId = await startKnowledgeThread({
        rpc: server.rpc,
        signal: controller.signal,
        workspace,
      });
      controller.signal.throwIfAborted();
      resource.result = await runTurn({
        ...options,
        captured,
        server,
        signal: controller.signal,
        threadId,
        workspace,
      });
    })()
  );
  if ("error" in execution) {
    const { error } = execution;
    if (controller.signal.aborted) {
      initialReport.code = "MODEL_CANCELLED";
    }
    initialReport.diagnostic = failureDiagnostic(error);
    if (error instanceof ServerAdmissionFailure) {
      initialReport.cleanup = error.cleanup;
      initialReport.nativeProcessId = error.processId;
      initialReport.admission = error.admission;
    }
    resource.result = createInvocationResult(null, initialReport, false);
  }
  const cleanup = await captureFailure(
    (async () => {
      await resource.server?.stop();
      rmSync(workspace, { force: true, recursive: true });
      resource.result.report.cleanup =
        resource.server === undefined
          ? (resource.result.report.cleanup ?? "not-observed")
          : "confirmed";
    })()
  );
  if ("error" in cleanup) {
    const cleanupReportOutcome = { outcome: "failed" };
    const cleanupReportCode = { code: "MODEL_CLEANUP_FAILED" };
    const cleanupReportCleanup = { cleanup: "failed" as const };
    const cleanupReport = {
      ...resource.result.report,
      ...cleanupReportOutcome,
      ...cleanupReportCode,
      ...cleanupReportCleanup,
    };
    resource.result = createInvocationResult(null, cleanupReport, false);
  }
  process.off("SIGINT", cancel);
  process.off("SIGTERM", cancel);
  const { report } = resource.result;
  const { threadId, turnId } = report;
  if (
    threadId !== undefined &&
    threadId !== "" &&
    turnId !== undefined &&
    turnId !== ""
  ) {
    report.usage = captured.measured({ threadId, turnId });
  }
  if (captured.invalid && report.outcome === "completed") {
    const protocolReportOutcome = { outcome: "failed" };
    const protocolReportCode = { code: "MODEL_PROTOCOL_FAILED" };
    const protocolReport = {
      ...report,
      ...protocolReportOutcome,
      ...protocolReportCode,
    };
    resource.result = createInvocationResult(null, protocolReport, false);
  }
  const finalReport = resource.result.report;
  const finalAdmission = {
    admission: resource.server?.admission ?? finalReport.admission,
  };
  const finalNativeProcessId = {
    nativeProcessId: resource.server?.pid ?? finalReport.nativeProcessId,
  };
  const finalStartedAt = { startedAt };
  const finalDurationMilliseconds = {
    durationMilliseconds: Math.round(performance.now() - began),
  };
  const reportWithMetadata = {
    ...finalReport,
    ...finalAdmission,
    ...finalNativeProcessId,
    ...finalStartedAt,
    ...finalDurationMilliseconds,
  };
  return { ...resource.result, report: reportWithMetadata };
};
