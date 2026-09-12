import { z } from 'zod';
import { knowledgeModel, knowledgeThread } from './profile.ts';
import type { AppServerConnection } from './connection.ts';

const startedThread = z.looseObject({
  cwd: z.string(),
  instructionSources: z.array(z.string()),
  model: z.literal(knowledgeModel.name),
  modelProvider: z.literal('openai'),
  reasoningEffort: z.literal('max'),
  sandbox: z.looseObject({ type: z.literal('readOnly') }),
  thread: z.looseObject({ ephemeral: z.literal(true), id: z.string() }),
  // Discovery paths can be reported even with project_doc_max_bytes=0, verified at admission.
});
const mcpInventory = z.looseObject({
  data: z.array(
    z.looseObject({
      resourceTemplates: z.array(z.unknown()).length(0),
      resources: z.array(z.unknown()).length(0),
      runtimeStatus: z.literal('disabled'),
      tools: z.record(z.string(), z.unknown()).refine((value) => !Object.keys(value).length),
    })
  ),
  nextCursor: z.null().optional(),
});
const threadStartTimeoutMilliseconds = 90_000;

export const startKnowledgeThread = async (options: {
  rpc: AppServerConnection;
  workspace: string;
  signal: AbortSignal;
}) => {
  const { rpc, workspace, signal } = options;
  const started = startedThread.parse(
    await rpc.request(
      'thread/start',
      {
        ...knowledgeThread,
        cwd: workspace,
      },
      { signal, timeoutMilliseconds: threadStartTimeoutMilliseconds }
    )
  );
  if (started.cwd !== workspace) {
    throw new Error('Native Codex workspace changed');
  }
  const threadId = started.thread.id;
  const threadStatusThreadId = { threadId };
  const threadStatusLimit = { limit: 100 };
  const threadStatusParameters = {
    ...threadStatusThreadId,
    ...threadStatusLimit,
  };
  mcpInventory.parse(
    await rpc.request('mcpServerStatus/list', threadStatusParameters, {
      signal,
    })
  );
  return threadId;
};
