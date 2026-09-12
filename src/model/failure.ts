import { z } from "zod";
import { AppServerRpcError } from "./rpc-error.ts";
import type { ProfileEvidence } from "./profile.ts";

class ServerAdmissionFailureError extends Error {
  name = "ServerAdmissionFailureError";
  readonly admission:
    | (ProfileEvidence & { launchPolicyHash: string })
    | undefined;
  readonly cleanup: "confirmed" | "failed";
  readonly processId: number;

  constructor(
    parameters: {
      admission?: ProfileEvidence & { launchPolicyHash: string };
      cause: unknown;
      cleanup: "confirmed" | "failed";
      processId: number;
    },
    options?: ErrorOptions
  ) {
    super("Native server admission failed", options);
    Object.defineProperty(this, "cause", {
      configurable: true,
      enumerable: false,
      value: parameters.cause,
      writable: true,
    });
    this.admission = parameters.admission;
    this.cleanup = parameters.cleanup;
    this.processId = parameters.processId;
  }
}

export { ServerAdmissionFailureError as ServerAdmissionFailure };

export const failureDiagnostic = (error: unknown): Record<string, unknown> => {
  let current = error;
  while (current instanceof ServerAdmissionFailureError) {
    current = current.cause;
  }
  if (current instanceof z.ZodError) {
    const diagnosticKind = { kind: "invalid-native-response" };
    const diagnosticFields = {
      fields: current.issues.map((issue) => issue.path.join(".")),
    };
    return { ...diagnosticKind, ...diagnosticFields };
  }
  if (current instanceof AppServerRpcError) {
    const diagnosticKind = { kind: "rpc-rejection" };
    const diagnosticCode = { code: current.code };
    return { ...diagnosticKind, ...diagnosticCode };
  }
  if (Error.isError(current) && current.constructor === Error) {
    return { kind: "native-admission", message: current.message };
  }
  return { kind: "native-failure" };
};
