import { mkdtempSync, readdirSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { z } from 'zod';
import { startServer } from './server.ts';
import { knowledgeTurn } from './profile.ts';
import { captureTranscript, type Usage } from './transcript.ts';
import { startKnowledgeThread } from './thread.ts';

type InvocationOptions = {
  binary: string;
  prompt: string;
  schema: Record<string, unknown>;
  deadlineMilliseconds: number;
};
export type InvocationReport = {
  outcome: string;
  code?: string;
  deadlineMilliseconds: number;
  interruption?: string;
  usage: Usage | null;
  turnAccepted?: 'confirmed' | 'unknown';
  threadId?: string;
  turnId?: string;
  cleanup?: 'confirmed' | 'failed' | 'not-observed';
  startedAt?: string;
  durationMilliseconds?: number;
};

class ModelTimeout extends Error {}
class ModelCancelled extends Error {}

async function completedWithin(options: {
  captured: ReturnType<typeof captureTranscript>;
  server: Awaited<ReturnType<typeof startServer>>;
  milliseconds: number;
  signal?: AbortSignal;
}) {
  const expired = Promise.withResolvers<never>();
  const cancelled = () => expired.reject(new ModelCancelled());
  options.signal?.addEventListener('abort', cancelled, { once: true });
  if (options.signal?.aborted) cancelled();
  const timer = setTimeout(() => expired.reject(new ModelTimeout()), options.milliseconds);
  try {
    return await Promise.race([
      options.captured.done.promise,
      options.server.rpc.closed,
      expired.promise,
    ]);
  } finally {
    clearTimeout(timer);
    options.signal?.removeEventListener('abort', cancelled);
  }
}

async function interrupt(options: {
  captured: ReturnType<typeof captureTranscript>;
  server: Awaited<ReturnType<typeof startServer>>;
  threadId: string;
  turnId: string;
}) {
  try {
    await options.server.rpc.request(
      'turn/interrupt',
      { threadId: options.threadId, turnId: options.turnId },
      { timeoutMilliseconds: 5000 },
    );
    const end = await completedWithin({
      captured: options.captured,
      server: options.server,
      milliseconds: 5000,
    });
    return (
      end.threadId === options.threadId &&
      end.turn.id === options.turnId &&
      end.turn.status === 'interrupted'
    );
  } catch {
    return false;
  }
}

async function runTurn(
  options: InvocationOptions & {
    server: Awaited<ReturnType<typeof startServer>>;
    captured: ReturnType<typeof captureTranscript>;
    workspace: string;
    threadId: string;
    signal: AbortSignal;
  },
) {
  const { server, captured, threadId } = options;
  const begin = performance.now();
  const accepted = await server.rpc
    .request(
      'turn/start',
      {
        threadId,
        ...knowledgeTurn,
        input: [{ type: 'text', text: options.prompt }],
        outputSchema: options.schema,
      },
      { timeoutMilliseconds: Math.min(options.deadlineMilliseconds, 30_000) },
    )
    .then((response) => z.looseObject({ turn: z.looseObject({ id: z.string() }) }).parse(response))
    .catch(() => null);
  if (!accepted) {
    const report: InvocationReport = {
      outcome: 'failed',
      code: 'MODEL_START_UNCONFIRMED',
      turnAccepted: 'unknown',
      threadId,
      deadlineMilliseconds: options.deadlineMilliseconds,
      usage: null,
    };
    return { value: null, report, retry: false };
  }
  const turnId = accepted.turn.id;
  try {
    if (options.signal.aborted) throw new ModelCancelled();
    const remaining = options.deadlineMilliseconds - (performance.now() - begin);
    if (remaining <= 0) throw new ModelTimeout();
    const end = await completedWithin({ ...options, milliseconds: remaining });
    if (end.threadId !== threadId || end.turn.id !== turnId || end.turn.status !== 'completed')
      throw new Error('Model completion was not established');
    const final = captured.items.at(-1);
    if (!final || final.threadId !== threadId || final.turnId !== turnId || !final.item.text)
      throw new Error('Structured model output is missing');
    if (readdirSync(options.workspace).length) throw new Error('Knowledge workspace was mutated');
    const report: InvocationReport = {
      outcome: 'completed',
      threadId,
      turnId,
      turnAccepted: 'confirmed',
      deadlineMilliseconds: options.deadlineMilliseconds,
      usage: captured.measured({ threadId, turnId }),
    };
    return { value: final.item.text, report, retry: false };
  } catch (error) {
    const confirmed = await interrupt({ ...options, turnId });
    const timeout = error instanceof ModelTimeout;
    const cancelled = error instanceof ModelCancelled;
    const report: InvocationReport = {
      outcome: timeout ? 'timeout' : 'failed',
      threadId,
      turnId,
      code: failureCode({ timeout, cancelled }),
      interruption: confirmed ? 'confirmed' : 'unconfirmed',
      turnAccepted: 'confirmed',
      deadlineMilliseconds: options.deadlineMilliseconds,
      usage: captured.measured({ threadId, turnId }),
    };
    return { value: null, report, retry: timeout && confirmed };
  }
}

export async function invokeModel(options: InvocationOptions) {
  const startedAt = new Date().toISOString();
  const began = performance.now();
  const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'hivex-model-')));
  const captured = captureTranscript();
  const controller = new AbortController();
  const cancel = () => controller.abort();
  process.on('SIGINT', cancel);
  process.on('SIGTERM', cancel);
  const initialReport: InvocationReport = {
    outcome: 'failed',
    code: 'MODEL_ADMISSION_FAILED',
    deadlineMilliseconds: options.deadlineMilliseconds,
    usage: null,
  };
  const resource: {
    server?: Awaited<ReturnType<typeof startServer>>;
    result: { value: unknown; report: InvocationReport; retry: boolean };
  } = { result: { value: null, report: initialReport, retry: false } };
  try {
    resource.server = await startServer({
      binary: options.binary,
      workspace,
      signal: controller.signal,
      notification: captured.notification,
      interaction: () => {
        throw new Error('Knowledge execution cannot request interaction');
      },
    });
    const threadId = await startKnowledgeThread({
      rpc: resource.server.rpc,
      workspace,
      signal: controller.signal,
    });
    controller.signal.throwIfAborted();
    resource.result = await runTurn({
      ...options,
      captured,
      workspace,
      server: resource.server,
      threadId,
      signal: controller.signal,
    });
  } catch {
    if (controller.signal.aborted) initialReport.code = 'MODEL_CANCELLED';
    resource.result = { value: null, report: initialReport, retry: false };
  } finally {
    try {
      await resource.server?.stop();
      rmSync(workspace, { recursive: true, force: true });
      resource.result.report.cleanup = resource.server ? 'confirmed' : 'not-observed';
    } catch {
      resource.result = {
        value: null,
        retry: false,
        report: {
          ...resource.result.report,
          outcome: 'failed',
          code: 'MODEL_CLEANUP_FAILED',
          cleanup: 'failed',
        },
      };
    }
    process.off('SIGINT', cancel);
    process.off('SIGTERM', cancel);
  }
  return {
    ...resource.result,
    report: {
      ...resource.result.report,
      startedAt,
      durationMilliseconds: Math.round(performance.now() - began),
    },
  };
}

function failureCode(options: { timeout: boolean; cancelled: boolean }) {
  if (options.cancelled) return 'MODEL_CANCELLED';
  return options.timeout ? 'MODEL_TIMEOUT' : 'MODEL_PROTOCOL_FAILED';
}
