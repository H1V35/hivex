import { createInterface, type Interface } from 'node:readline';
import type { Readable, Writable } from 'node:stream';
import { z } from 'zod';

const IdSchema = z.union([z.string(), z.number().int()]);
const RemoteErrorSchema = z.looseObject({
  code: z.number().int(),
  message: z.string(),
  data: z.unknown().optional(),
});
const FrameSchema = z.looseObject({
  id: IdSchema.optional(),
  method: z.string().optional(),
  params: z.unknown().optional(),
  result: z.unknown().optional(),
  error: RemoteErrorSchema.optional(),
});
type Frame = z.infer<typeof FrameSchema>;

interface ConnectionOptions {
  readonly input: Writable;
  readonly output: Readable;
  readonly onNotification: (method: string, params: unknown) => void;
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

export class AppServerRpcError extends Error {
  readonly code: number;
  readonly data: unknown;

  constructor(error: z.infer<typeof RemoteErrorSchema>) {
    super(error.message);
    this.name = 'AppServerRpcError';
    this.code = error.code;
    this.data = error.data;
  }
}

export class AppServerConnection {
  private readonly options: ConnectionOptions;
  private readonly reader: Interface;
  private readonly pending = new Map<string | number, PendingRequest>();
  private readonly ending = Promise.withResolvers<never>();
  private failure: Error | undefined;
  private nextId = 1;
  private trailingBytes = 0;
  private streamBytes = 0;
  readonly closed = this.ending.promise;

  constructor(options: ConnectionOptions) {
    this.options = options;
    options.output.on('data', this.measureChunk);
    this.reader = createInterface({ input: options.output, crlfDelay: Infinity });
    this.reader.on('line', this.receiveLine);
    this.reader.on('close', this.receiveClose);
    options.input.on('error', this.receiveStreamError);
    options.output.on('error', this.receiveStreamError);
    void this.closed.catch(() => undefined);
  }

  request(method: string, params: unknown, options: RequestOptions = {}): Promise<unknown> {
    if (this.failure !== undefined) return Promise.reject(this.failure);
    if (options.signal?.aborted) return Promise.reject(new Error('request cancelled'));
    const timeout = options.timeoutMilliseconds ?? 30_000;
    if (!Number.isSafeInteger(timeout) || timeout <= 0) {
      return Promise.reject(new Error('request timeout must be a positive integer'));
    }
    const id = this.nextId++;
    const result = Promise.withResolvers<unknown>();
    const abort = () => this.rejectRequest(id, new Error('request cancelled'));
    const timer = setTimeout(
      () => this.rejectRequest(id, new Error(`request timed out: ${method}`)),
      timeout,
    );
    this.pending.set(id, {
      resolve: result.resolve,
      reject: result.reject,
      cleanup: () => {
        clearTimeout(timer);
        options.signal?.removeEventListener('abort', abort);
      },
    });
    options.signal?.addEventListener('abort', abort, { once: true });
    this.send({ id, method, params });
    return result.promise;
  }

  notify(method: string, params?: unknown): void {
    if (this.failure !== undefined) throw this.failure;
    this.send({ method, params });
  }

  dispose(): void {
    this.fail(new Error('connection closed'));
    this.reader.off('line', this.receiveLine);
    this.reader.off('close', this.receiveClose);
    this.reader.close();
    this.options.input.off('error', this.receiveStreamError);
    this.options.output.off('error', this.receiveStreamError);
    this.options.output.off('data', this.measureChunk);
  }

  private readonly receiveClose = (): void => this.fail(new Error('connection closed'));
  private readonly receiveStreamError = (error: Error): void => this.fail(error);

  private readonly measureChunk = (chunk: Buffer): void => {
    if (this.failure) return;
    this.streamBytes += chunk.byteLength;
    if (this.streamBytes > 33_554_432) {
      this.fail(new Error('app-server stream exceeded 32 MiB'));
      return;
    }
    let offset = 0;
    for (let newline = chunk.indexOf(10); newline !== -1; newline = chunk.indexOf(10, offset)) {
      this.trailingBytes += newline - offset;
      if (this.trailingBytes > 4_194_304) {
        this.fail(new Error('app-server frame exceeded 4 MiB'));
        return;
      }
      this.trailingBytes = 0;
      offset = newline + 1;
    }
    this.trailingBytes += chunk.byteLength - offset;
    if (this.trailingBytes > 4_194_304) this.fail(new Error('app-server frame exceeded 4 MiB'));
  };

  private readonly receiveLine = (line: string): void => {
    if (this.failure !== undefined) return;
    try {
      const frame = FrameSchema.parse(JSON.parse(line) as unknown);
      this.receiveFrame(frame);
    } catch (error: unknown) {
      this.fail(new Error('invalid app-server frame', { cause: error }));
    }
  };

  private receiveFrame(frame: Frame): void {
    if (frame.method !== undefined) {
      if (Object.hasOwn(frame, 'result') || frame.error !== undefined) {
        throw new Error('a method frame cannot also be a response');
      }
      if (frame.id === undefined) this.options.onNotification(frame.method, frame.params);
      else this.declineInteractiveRequest(frame.id, frame.method);
      return;
    }
    if (frame.id === undefined || Object.hasOwn(frame, 'result') === (frame.error !== undefined)) {
      throw new Error('a response requires an id and exactly one outcome');
    }
    const pending = this.pending.get(frame.id);
    if (pending === undefined) return;
    this.pending.delete(frame.id);
    pending.cleanup();
    if (frame.error !== undefined) pending.reject(new AppServerRpcError(frame.error));
    else pending.resolve(frame.result);
  }

  private declineInteractiveRequest(id: string | number, method: string): void {
    this.options.onInteractiveRequest?.(method);
    if (
      method === 'item/commandExecution/requestApproval' ||
      method === 'item/fileChange/requestApproval'
    ) {
      this.send({ id, result: { decision: 'decline' } });
      return;
    }
    this.send({
      id,
      error: { code: -32601, message: 'interactive requests are outside this pilot' },
    });
  }

  private send(frame: unknown): void {
    try {
      this.options.input.write(`${JSON.stringify(frame)}\n`, (error) => {
        if (error !== null && error !== undefined) this.fail(error);
      });
    } catch (error: unknown) {
      this.fail(new Error('app-server write failed', { cause: error }));
    }
  }

  private rejectRequest(id: string | number, error: Error): void {
    const pending = this.pending.get(id);
    if (pending === undefined) return;
    this.pending.delete(id);
    pending.cleanup();
    pending.reject(error);
  }

  private fail(error: Error): void {
    if (this.failure !== undefined) return;
    this.failure = error;
    this.reader.close();
    for (const id of this.pending.keys()) this.rejectRequest(id, error);
    this.ending.reject(error);
  }
}
