import { z } from "zod";

const identity = z.looseObject({ threadId: z.string(), turnId: z.string() });
const terminal = z.looseObject({
  threadId: z.string(),
  turn: z.looseObject({
    id: z.string(),
    status: z.enum(["completed", "failed", "interrupted"]),
  }),
});
const itemEvent = identity.extend({
  item: z.looseObject({
    id: z.string(),
    text: z.string().optional(),
    type: z.string(),
  }),
});
export const usageSchema = z.object({
  cacheWriteInputTokens: z.number().int().nonnegative().optional(),
  cachedInputTokens: z.number().int().nonnegative(),
  inputTokens: z.number().int().nonnegative(),
  outputTokens: z.number().int().nonnegative(),
  reasoningOutputTokens: z.number().int().nonnegative(),
  totalTokens: z.number().int().nonnegative(),
});
const usageEvent = identity.extend({
  tokenUsage: z.looseObject({ total: usageSchema }),
});
export type Usage = z.infer<typeof usageSchema>;

const consistentUsage = (current: Usage, previous: Usage | undefined) => {
  if (current.totalTokens !== current.inputTokens + current.outputTokens) {
    return false;
  }
  if (
    current.cachedInputTokens > current.inputTokens ||
    current.reasoningOutputTokens > current.outputTokens
  ) {
    return false;
  }
  if (
    current.cacheWriteInputTokens !== undefined &&
    current.cacheWriteInputTokens > current.inputTokens
  ) {
    return false;
  }
  if (previous === undefined) {
    return true;
  }
  return (
    current.inputTokens >= previous.inputTokens &&
    current.outputTokens >= previous.outputTokens &&
    current.totalTokens >= previous.totalTokens
  );
};

const observeRejection = async (promise: Promise<unknown>): Promise<void> => {
  try {
    await promise;
  } catch {
    // The rejection is handled by the caller through the public promise.
  }
};

export const captureTranscript = () => {
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
    if (identities.size > 1) {
      throw new Error("Native evidence mixed thread or turn identities");
    }
  };
  void observeRejection(done.promise);
  const receive = (method: string, parameters: unknown) => {
    switch (method) {
      case "turn/completed": {
        const event = terminal.parse(parameters);
        trackIdentity({ threadId: event.threadId, turnId: event.turn.id });
        if (state.terminalSeen) {
          throw new Error("Duplicate native terminal event");
        }
        state.terminalSeen = true;
        done.resolve(event);
        break;
      }
      case "thread/tokenUsage/updated": {
        const event = usageEvent.parse(parameters);
        trackIdentity(event);
        if (
          !consistentUsage(
            event.tokenUsage.total,
            state.counter?.tokenUsage.total
          )
        ) {
          state.usageInvalid = true;
          throw new Error("Native usage is inconsistent or regressed");
        }
        state.counter = event;
        break;
      }
      case "item/completed":
      case "item/started": {
        const entry = itemEvent.parse(parameters);
        trackIdentity(entry);
        if (
          !["userMessage", "reasoning", "agentMessage"].includes(
            entry.item.type
          )
        ) {
          throw new Error("Unexpected knowledge-model tool activity");
        }
        if (method === "item/completed" && entry.item.type === "agentMessage") {
          items.push(entry);
        }
        break;
      }
      default: {
        break;
      }
    }
  };
  const notification = (method: string, parameters: unknown) => {
    try {
      receive(method, parameters);
    } catch (error) {
      state.invalid = true;
      throw error;
    }
  };
  const measured = (expected: { threadId: string; turnId: string }) => {
    const current = state.counter;
    if (
      state.usageInvalid ||
      current?.threadId !== expected.threadId ||
      current.turnId !== expected.turnId
    ) {
      return null;
    }
    return current.tokenUsage.total;
  };
  const assertValid = (expected: { threadId: string; turnId: string }) => {
    if (
      state.invalid ||
      !identities.has(JSON.stringify([expected.threadId, expected.turnId]))
    ) {
      throw new Error(
        "Native evidence did not retain one consistent invocation identity"
      );
    }
  };
  return {
    assertValid,
    done,
    get invalid() {
      return state.invalid;
    },
    items,
    measured,
    notification,
  };
};
