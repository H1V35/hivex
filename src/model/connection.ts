import { createInterface } from "node:readline";
import { z } from "zod";
import { AppServerRpcError, RemoteErrorSchema } from "./rpc-error.ts";
import type { Interface } from "node:readline";
import type { Readable, Writable } from "node:stream";

export { AppServerRpcError } from "./rpc-error.ts";

const observeRejection = async (promise: Promise<unknown>): Promise<void> => {
  try {
    await promise;
  } catch {
    // The rejection is handled by the caller through the public promise.
  }
};

const createEnding = () => {
  const ending = Promise.withResolvers<never>();
  void observeRejection(ending.promise);
  return ending;
};

const IdSchema = z.union([z.string(), z.number().int()]);
const FrameSchema = z.looseObject({
  error: RemoteErrorSchema.optional(),
  id: IdSchema.optional(),
  method: z.string().optional(),
  params: z.unknown().optional(),
  result: z.unknown().optional(),
});
type Frame = z.infer<typeof FrameSchema>;

interface ConnectionOptions {
  readonly input: Writable;
  readonly output: Readable;
  readonly onNotification: (method: string, parameters: unknown) => void;
  readonly onInteractiveRequest?: (method: string) => void;
}
interface RequestOptions {
  readonly timeoutMilliseconds?: number;
  // Cancels the local wait. Remote turns require an explicit turn/interrupt request.
  readonly signal?: AbortSignal;
}
interface PendingRequest {
  readonly resolve: (value: unknown) => void;
  readonly reject: (error: unknown) => void;
  readonly cleanup: () => void;
}

interface ConnectionListeners {
  readonly close: () => void;
  readonly data: (chunk: Buffer) => void;
  readonly error: (error: Error) => void;
  readonly line: (line: string) => void;
}

export class AppServerConnection {
  private readonly options: ConnectionOptions;
  private readonly reader: Interface;
  private readonly pending = new Map<string | number, PendingRequest>();
  private readonly ending = createEnding();
  private failure: Error | undefined;
  private readonly listeners: ConnectionListeners;
  private nextId = 1;
  private trailingBytes = 0;
  private streamBytes = 0;

  constructor(options: ConnectionOptions) {
    this.options = options;
    this.listeners = {
      close: this.receiveClose.bind(this),
      data: this.measureChunk.bind(this),
      error: this.receiveStreamError.bind(this),
      line: this.receiveLine.bind(this),
    };
    options.output.on("data", this.listeners.data);
    this.reader = createInterface({
      crlfDelay: Infinity,
      input: options.output,
    });
    this.reader.on("line", this.listeners.line);
    this.reader.on("close", this.listeners.close);
    options.input.on("error", this.listeners.error);
    options.output.on("error", this.listeners.error);
  }

  get closed(): Promise<never> {
    return this.ending.promise;
  }

  async request(
    method: string,
    parameters: unknown,
    options: RequestOptions = {}
  ): Promise<unknown> {
    if (this.failure !== undefined) {
      throw this.failure;
    }
    if (options.signal?.aborted === true) {
      throw new Error("request cancelled");
    }
    const timeout = options.timeoutMilliseconds ?? 30_000;
    if (!Number.isSafeInteger(timeout) || timeout <= 0) {
      throw new Error("request timeout must be a positive integer");
    }
    const id = this.nextId;
    this.nextId += 1;
    const result = Promise.withResolvers<unknown>();
    const abort = (): void => {
      this.rejectRequest(id, new Error("request cancelled"));
    };
    const timer = setTimeout(() => {
      this.rejectRequest(id, new Error(`request timed out: ${method}`));
    }, timeout);
    this.pending.set(id, {
      cleanup: () => {
        clearTimeout(timer);
        options.signal?.removeEventListener("abort", abort);
      },
      reject: result.reject,
      resolve: result.resolve,
    });
    options.signal?.addEventListener("abort", abort, { once: true });
    this.send({ id, method, params: parameters });
    return await result.promise;
  }

  notify(method: string, parameters?: unknown): void {
    if (this.failure !== undefined) {
      throw this.failure;
    }
    this.send({ method, params: parameters });
  }

  dispose(): void {
    this.fail(new Error("connection closed"));
    this.reader.off("line", this.listeners.line);
    this.reader.off("close", this.listeners.close);
    this.reader.close();
    this.options.input.off("error", this.listeners.error);
    this.options.output.off("error", this.listeners.error);
    this.options.output.off("data", this.listeners.data);
  }

  protected receiveClose(): void {
    this.fail(new Error("connection closed"));
  }

  protected receiveStreamError(error: Error): void {
    this.fail(error);
  }

  protected measureChunk(chunk: Buffer): void {
    if (this.failure !== undefined) {
      return;
    }
    this.streamBytes += chunk.byteLength;
    if (this.streamBytes > 33_554_432) {
      this.fail(new Error("app-server stream exceeded 32 MiB"));
      return;
    }
    let offset = 0;
    for (
      let newline = chunk.indexOf(10);
      newline !== -1;
      newline = chunk.indexOf(10, offset)
    ) {
      this.trailingBytes += newline - offset;
      if (this.trailingBytes > 4_194_304) {
        this.fail(new Error("app-server frame exceeded 4 MiB"));
        return;
      }
      this.trailingBytes = 0;
      offset = newline + 1;
    }
    this.trailingBytes += chunk.byteLength - offset;
    if (this.trailingBytes > 4_194_304) {
      this.fail(new Error("app-server frame exceeded 4 MiB"));
    }
  }

  protected receiveLine(line: string): void {
    if (this.failure !== undefined) {
      return;
    }
    try {
      const frame = FrameSchema.parse(JSON.parse(line) as unknown);
      this.receiveFrame(frame);
    } catch (error: unknown) {
      this.fail(new Error("invalid app-server frame", { cause: error }));
    }
  }

  protected receiveFrame(frame: Frame): void {
    if (frame.method !== undefined) {
      if (Object.hasOwn(frame, "result") || frame.error !== undefined) {
        throw new Error("a method frame cannot also be a response");
      }
      if (frame.id === undefined) {
        this.options.onNotification(frame.method, frame.params);
      } else {
        this.declineInteractiveRequest(frame.id, frame.method);
      }
      return;
    }
    if (
      frame.id === undefined ||
      Object.hasOwn(frame, "result") === (frame.error !== undefined)
    ) {
      throw new Error("a response requires an id and exactly one outcome");
    }
    const pending = this.pending.get(frame.id);
    if (pending === undefined) {
      return;
    }
    this.pending.delete(frame.id);
    pending.cleanup();
    if (frame.error === undefined) {
      pending.resolve(frame.result);
      return;
    }
    pending.reject(new AppServerRpcError(frame.error));
  }

  protected declineInteractiveRequest(
    id: string | number,
    method: string
  ): void {
    this.options.onInteractiveRequest?.(method);
    if (
      method === "item/commandExecution/requestApproval" ||
      method === "item/fileChange/requestApproval"
    ) {
      this.send({ id, result: { decision: "decline" } });
      return;
    }
    this.send({
      error: {
        code: -32_601,
        message: "interactive requests are outside this pilot",
      },
      id,
    });
  }

  protected send(frame: unknown): void {
    try {
      this.options.input.write(`${JSON.stringify(frame)}\n`);
    } catch (error: unknown) {
      this.fail(new Error("app-server write failed", { cause: error }));
    }
  }

  protected rejectRequest(id: string | number, error: Error): void {
    const pending = this.pending.get(id);
    if (pending === undefined) {
      return;
    }
    this.pending.delete(id);
    pending.cleanup();
    pending.reject(error);
  }

  protected fail(error: Error): void {
    if (this.failure !== undefined) {
      return;
    }
    this.failure = error;
    this.reader.close();
    for (const id of this.pending.keys()) {
      this.rejectRequest(id, error);
    }
    this.ending.reject(error);
  }
}
