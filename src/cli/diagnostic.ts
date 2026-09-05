import { HivexError } from '../errors.ts';

function bounded(text: string) {
  let result = '';
  for (const char of text) {
    if (Buffer.byteLength(JSON.stringify(result + char)) > 256) break;
    result += char;
  }
  return result;
}

export function diagnostic(error: unknown) {
  let failure = new HivexError('READ_FAILED', 'Unable to read project knowledge');
  if (error instanceof HivexError) failure = error;
  else if (error instanceof Error) failure = new HivexError('READ_FAILED', error.message);
  let details = failure.details;
  if (details && Buffer.byteLength(JSON.stringify(details)) > 384) details = { omitted: true };
  const message = bounded(failure.message);
  return {
    error: { code: failure.code, message, messageTruncated: message !== failure.message, details },
  };
}
