import { z } from "zod";

export const RemoteErrorSchema = z.looseObject({
  code: z.number().int(),
  data: z.unknown().optional(),
  message: z.string(),
});

export type RemoteError = z.infer<typeof RemoteErrorSchema>;

export class AppServerRpcError extends Error {
  name = "AppServerRpcError";
  readonly code: number;
  readonly data: unknown;

  constructor(error: RemoteError, options?: ErrorOptions) {
    super(error.message, options);
    this.code = error.code;
    this.data = error.data;
  }
}
