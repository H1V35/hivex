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
    throw new HivexError(
      'INVALID_ARGUMENT',
      error instanceof Error ? error.message : 'Invalid command arguments',
    );
  }
}

function commandFor(positionals: string[]) {
  const [command, value] = positionals;
  if (positionals.length !== 2 || !value || (command !== 'search' && command !== 'read'))
    throw new HivexError(
      'INVALID_ARGUMENT',
      'Usage: hivex search <query> | read <source-id> [options]',
    );
  if (Buffer.byteLength(value) > 4096 || !value.trim())
    throw new HivexError(
      'INVALID_ARGUMENT',
      'Query or source ID must contain 1 to 4096 UTF-8 bytes',
    );
  return { command, value };
}

function parseLimit(
  value: string | undefined,
  fallback: number,
  maximum: number,
  minimum: number,
): number {
  if (value === undefined) return fallback;
  if (!/^[0-9]+$/.test(value))
    throw new HivexError('INVALID_ARGUMENT', 'Limits must be positive decimal integers');
  const number = Number(value);
  if (!Number.isSafeInteger(number) || number < minimum || number > maximum)
    throw new HivexError('INVALID_ARGUMENT', `Limit must be between ${minimum} and ${maximum}`);
  return number;
}

function validateMode(command: string, values: ReturnType<typeof parseInput>['values']) {
  if (command === 'read' && (values.collection !== undefined || values.limit !== undefined))
    throw new HivexError('INVALID_ARGUMENT', 'Only search accepts --collection and --limit');
  if (command === 'search' && values.cursor !== undefined)
    throw new HivexError('INVALID_ARGUMENT', 'Only read accepts --cursor');
  if (values.cursor && values.cursor.length > 2048)
    throw new HivexError('INVALID_ARGUMENT', 'Continuation cursor is too long');
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
    limit: parseLimit(parsed.values.limit, 8, 20, 1),
    maxBytes: parseLimit(parsed.values['max-bytes'], defaultBudget, 65_536, 1024),
    cursor: parsed.values.cursor,
    collection: parsed.values.collection,
  };
}
