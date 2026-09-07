import { parseArgs } from 'node:util';
import { HivexError } from '../errors.ts';
import { parseLimit } from '../cli/arguments.ts';

function input(args: string[]) {
  try {
    return parseArgs({
      args,
      strict: true,
      options: {
        root: { type: 'string' },
        ref: { type: 'string' },
        collection: { type: 'string' },
        cursor: { type: 'string' },
        limit: { type: 'string' },
        'max-bytes': { type: 'string' },
      },
    }).values;
  } catch (error) {
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: error instanceof Error ? error.message : 'Invalid plan arguments',
    });
  }
}

export function planArguments(args: string[]) {
  const values = input(args);
  if (Object.values(values).some((value) => value === ''))
    throw new HivexError({ code: 'INVALID_ARGUMENT', message: 'Plan options cannot be empty' });
  if (values.cursor !== undefined && values.cursor.length > 2048)
    throw new HivexError({ code: 'INVALID_ARGUMENT', message: 'Plan cursor is too long' });
  if (values.collection !== undefined && !/^[a-z][a-z0-9-]{0,47}$/.test(values.collection))
    throw new HivexError({ code: 'INVALID_ARGUMENT', message: 'Invalid collection ID' });
  return {
    root: values.root ?? process.cwd(),
    ref: values.ref ?? 'HEAD',
    collection: values.collection,
    cursor: values.cursor,
    limit: parseLimit(values.limit, { fallback: 20, minimum: 1, maximum: 20 }),
    maxBytes: parseLimit(values['max-bytes'], {
      fallback: 16_384,
      minimum: 1024,
      maximum: 65_536,
    }),
  };
}
