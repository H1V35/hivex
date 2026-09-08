import { z } from 'zod';

const identity = z.looseObject({ threadId: z.string(), turnId: z.string() });
const terminal = z.looseObject({
  threadId: z.string(),
  turn: z.looseObject({ id: z.string(), status: z.enum(['completed', 'failed', 'interrupted']) }),
});
const itemEvent = identity.extend({
  item: z.looseObject({ id: z.string(), type: z.string(), text: z.string().optional() }),
});
export const usageSchema = z.object({
  inputTokens: z.number().int().nonnegative(),
  cachedInputTokens: z.number().int().nonnegative(),
  outputTokens: z.number().int().nonnegative(),
  reasoningOutputTokens: z.number().int().nonnegative(),
  totalTokens: z.number().int().nonnegative(),
  cacheWriteInputTokens: z.number().int().nonnegative().optional(),
});
const usageEvent = identity.extend({ tokenUsage: z.looseObject({ total: usageSchema }) });
export type Usage = z.infer<typeof usageSchema>;

function consistentUsage(current: Usage, previous: Usage | undefined) {
  if (current.totalTokens !== current.inputTokens + current.outputTokens) return false;
  if (
    current.cachedInputTokens > current.inputTokens ||
    current.reasoningOutputTokens > current.outputTokens
  )
    return false;
  if (
    current.cacheWriteInputTokens !== undefined &&
    current.cacheWriteInputTokens > current.inputTokens
  )
    return false;
  if (!previous) return true;
  return (
    current.inputTokens >= previous.inputTokens &&
    current.outputTokens >= previous.outputTokens &&
    current.totalTokens >= previous.totalTokens
  );
}

export function captureTranscript() {
  const done = Promise.withResolvers<z.infer<typeof terminal>>();
  const items: z.infer<typeof itemEvent>[] = [];
  const state: {
    invalid: boolean;
    terminalSeen: boolean;
    usageInvalid: boolean;
    counter?: z.infer<typeof usageEvent>;
  } = { invalid: false, terminalSeen: false, usageInvalid: false };
  const identities = new Set<string>();
  const trackIdentity = (value: { threadId: string; turnId: string }) => {
    identities.add(JSON.stringify([value.threadId, value.turnId]));
    if (identities.size > 1) throw new Error('Native evidence mixed thread or turn identities');
  };
  void done.promise.catch(() => undefined);
  const receive = (method: string, params: unknown) => {
    if (method === 'turn/completed') {
      const event = terminal.parse(params);
      trackIdentity({ threadId: event.threadId, turnId: event.turn.id });
      if (state.terminalSeen) throw new Error('Duplicate native terminal event');
      state.terminalSeen = true;
      done.resolve(event);
    }
    if (method === 'thread/tokenUsage/updated') {
      const event = usageEvent.parse(params);
      trackIdentity(event);
      if (!consistentUsage(event.tokenUsage.total, state.counter?.tokenUsage.total)) {
        state.usageInvalid = true;
        throw new Error('Native usage is inconsistent or regressed');
      }
      state.counter = event;
    }
    if (method !== 'item/started' && method !== 'item/completed') return;
    const entry = itemEvent.parse(params);
    trackIdentity(entry);
    if (!['userMessage', 'reasoning', 'agentMessage'].includes(entry.item.type))
      throw new Error('Unexpected knowledge-model tool activity');
    if (method === 'item/completed' && entry.item.type === 'agentMessage') items.push(entry);
  };
  const notification = (method: string, params: unknown) => {
    try {
      receive(method, params);
    } catch (error) {
      state.invalid = true;
      throw error;
    }
  };
  const measured = (expected: { threadId: string; turnId: string }) => {
    const current = state.counter;
    if (
      state.usageInvalid ||
      !current ||
      current.threadId !== expected.threadId ||
      current.turnId !== expected.turnId
    )
      return null;
    return current.tokenUsage.total;
  };
  const assertValid = (expected: { threadId: string; turnId: string }) => {
    if (state.invalid || !identities.has(JSON.stringify([expected.threadId, expected.turnId])))
      throw new Error('Native evidence did not retain one consistent invocation identity');
  };
  return {
    done,
    items,
    notification,
    measured,
    assertValid,
    get invalid() {
      return state.invalid;
    },
  };
}
