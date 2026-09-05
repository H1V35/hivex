import { parseArgs } from 'node:util';
import { HivexError } from '../errors.ts';

function parseInput(args: string[]) {
  try {
    return parseArgs({
      args,
      allowPositionals: true,
      strict: true,
      options: {
        root: { type: 'string' },
        ref: { type: 'string' },
        'max-bytes': { type: 'string' },
        cursor: { type: 'string' },
        collection: { type: 'string' },
        limit: { type: 'string' },
      },
    });
  } catch (error) {
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: error instanceof Error ? error.message : 'Invalid command arguments',
    });
  }
}

function commandFor(positionals: string[]) {
  const [command, value] = positionals;
  if (positionals.length !== 2 || !value || (command !== 'search' && command !== 'read'))
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Usage: hivex search <query> | read <source-id> [options]',
    });
  if (Buffer.byteLength(value) > 4096 || !value.trim())
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Query or source ID must contain 1 to 4096 UTF-8 bytes',
    });
  return { command, value };
}

function parseLimit(
  value: string | undefined,
  bounds: { fallback: number; maximum: number; minimum: number },
): number {
  const { fallback, maximum, minimum } = bounds;
  if (value === undefined) return fallback;
  if (!/^[0-9]+$/.test(value))
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Limits must be positive decimal integers',
    });
  const number = Number(value);
  if (!Number.isSafeInteger(number) || number < minimum || number > maximum)
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: `Limit must be between ${minimum} and ${maximum}`,
    });
  return number;
}

function validateMode(command: string, values: ReturnType<typeof parseInput>['values']) {
  if (command === 'read' && (values.collection !== undefined || values.limit !== undefined))
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Only search accepts --collection and --limit',
    });
  if (command === 'search' && values.cursor !== undefined)
    throw new HivexError({ code: 'INVALID_ARGUMENT', message: 'Only read accepts --cursor' });
  if (values.cursor !== undefined && (values.cursor.length === 0 || values.cursor.length > 2048))
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Continuation cursor must contain 1 to 2048 characters',
    });
  if (values.collection !== undefined && !/^[a-z][a-z0-9-]{0,47}$/.test(values.collection))
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Collection must be a valid declared ID',
    });
}

export function argumentsFor(args: string[]) {
  const parsed = parseInput(args);
  const { command, value } = commandFor(parsed.positionals);
  validateMode(command, parsed.values);
  const defaultBudget = command === 'search' ? 4096 : 16_384;
  return {
    command,
    value,
    root: parsed.values.root ?? process.cwd(),
    ref: parsed.values.ref ?? 'HEAD',
    limit: parseLimit(parsed.values.limit, { fallback: 8, maximum: 20, minimum: 1 }),
    maxBytes: parseLimit(parsed.values['max-bytes'], {
      fallback: defaultBudget,
      maximum: 65_536,
      minimum: 1024,
    }),
    cursor: parsed.values.cursor,
    collection: parsed.values.collection,
  };
}
