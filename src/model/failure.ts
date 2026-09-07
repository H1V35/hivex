import { z } from 'zod';
import { AppServerRpcError } from './connection.ts';

export class ServerAdmissionFailure extends Error {
  readonly cleanup: 'confirmed' | 'failed';
  readonly processId: number;
  constructor(options: { cause: unknown; cleanup: 'confirmed' | 'failed'; processId: number }) {
    super('Native server admission failed', { cause: options.cause });
    this.cleanup = options.cleanup;
    this.processId = options.processId;
  }
}

export function failureDiagnostic(error: unknown): Record<string, unknown> {
  if (error instanceof ServerAdmissionFailure) return failureDiagnostic(error.cause);
  if (error instanceof z.ZodError)
    return {
      kind: 'invalid-native-response',
      fields: error.issues.map((issue) => issue.path.join('.')),
    };
  if (error instanceof AppServerRpcError) return { kind: 'rpc-rejection', code: error.code };
  if (error instanceof Error && error.constructor === Error)
    return { kind: 'native-admission', message: error.message };
  return { kind: 'native-failure' };
}
