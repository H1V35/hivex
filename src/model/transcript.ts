import { z } from 'zod';

const identity = z.looseObject({ threadId: z.string(), turnId: z.string() });
const terminal = z.looseObject({
  threadId: z.string(),
  turn: z.looseObject({ id: z.string(), status: z.enum(['completed', 'failed', 'interrupted']) }),
});
const itemEvent = identity.extend({
  item: z.looseObject({ id: z.string(), type: z.string(), text: z.string().optional() }),
});
const usage = z.object({
  inputTokens: z.number().int().nonnegative(),
  cachedInputTokens: z.number().int().nonnegative(),
  outputTokens: z.number().int().nonnegative(),
  reasoningOutputTokens: z.number().int().nonnegative(),
  totalTokens: z.number().int().nonnegative(),
  cacheWriteInputTokens: z.number().int().nonnegative().optional(),
});
const usageEvent = identity.extend({ tokenUsage: z.looseObject({ total: usage }) });
export type Usage = z.infer<typeof usage>;

export function captureTranscript() {
  const done = Promise.withResolvers<z.infer<typeof terminal>>();
  const items: z.infer<typeof itemEvent>[] = [];
  const counters: z.infer<typeof usageEvent>[] = [];
  void done.promise.catch(() => undefined);
  const notification = (method: string, params: unknown) => {
    if (method === 'turn/completed') done.resolve(terminal.parse(params));
    if (method === 'thread/tokenUsage/updated')
      counters.splice(0, counters.length, usageEvent.parse(params));
    if (method !== 'item/started' && method !== 'item/completed') return;
    const entry = itemEvent.parse(params);
    if (!['userMessage', 'reasoning', 'agentMessage'].includes(entry.item.type))
      throw new Error('Unexpected knowledge-model tool activity');
    if (method === 'item/completed' && entry.item.type === 'agentMessage') items.push(entry);
  };
  const measured = (expected: { threadId: string; turnId: string }) => {
    const current = counters.at(-1);
    if (current && (current.threadId !== expected.threadId || current.turnId !== expected.turnId))
      throw new Error('Usage identity changed');
    return current?.tokenUsage.total ?? null;
  };
  return { done, items, notification, measured };
}
