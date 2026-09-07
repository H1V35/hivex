import { z } from 'zod';
import { AppServerConnection } from './connection.ts';
import { knowledgeModel, knowledgeThread } from './profile.ts';

const startedThread = z.looseObject({
  thread: z.looseObject({ id: z.string(), ephemeral: z.literal(true) }),
  model: z.literal(knowledgeModel.name),
  modelProvider: z.literal('openai'),
  reasoningEffort: z.literal('max'),
  cwd: z.string(),
  sandbox: z.looseObject({ type: z.literal('readOnly') }),
  instructionSources: z.array(z.unknown()).length(0),
});
const mcpInventory = z.looseObject({
  data: z.array(
    z.looseObject({
      runtimeStatus: z.literal('disabled'),
      tools: z.record(z.string(), z.unknown()).refine((value) => !Object.keys(value).length),
      resources: z.array(z.unknown()).length(0),
      resourceTemplates: z.array(z.unknown()).length(0),
    }),
  ),
  nextCursor: z.null().optional(),
});

export async function startKnowledgeThread(options: {
  rpc: AppServerConnection;
  workspace: string;
  signal: AbortSignal;
}) {
  const { rpc, workspace, signal } = options;
  const started = startedThread.parse(
    await rpc.request(
      'thread/start',
      {
        ...knowledgeThread,
        cwd: workspace,
      },
      { signal },
    ),
  );
  if (started.cwd !== workspace) throw new Error('Native Codex workspace changed');
  const threadId = started.thread.id;
  mcpInventory.parse(
    await rpc.request('mcpServerStatus/list', { threadId, limit: 100 }, { signal }),
  );
  return threadId;
}
