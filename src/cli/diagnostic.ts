import { HivexError } from "../errors.ts";

const bounded = (text: string) => {
  let result = "";
  for (const char of text) {
    if (Buffer.byteLength(JSON.stringify(result + char)) > 256) {
      break;
    }
    result += char;
  }
  return result;
};
export const diagnostic = (error: unknown) => {
  let failure = new HivexError({
    code: "READ_FAILED",
    message: "Unable to read project knowledge",
  });
  if (error instanceof HivexError) {
    failure = error;
  } else if (Error.isError(error)) {
    failure = new HivexError({ code: "READ_FAILED", message: error.message });
  }
  let { details } = failure;
  if (details && Buffer.byteLength(JSON.stringify(details)) > 384) {
    details = { omitted: true };
  }
  const message = bounded(failure.message);
  return {
    error: {
      code: failure.code,
      details,
      message,
      messageTruncated: message !== failure.message,
    },
  };
};
