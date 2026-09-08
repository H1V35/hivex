import { parseArgs } from 'node:util';
import { HivexError } from '../errors.ts';

export function extractionArguments(args: string[]) {
  const parsed = parseArgs({
    args,
    allowPositionals: true,
    strict: true,
    options: {
      root: { type: 'string' },
      ref: { type: 'string' },
      codex: { type: 'string' },
      attempts: { type: 'string' },
      'deadline-ms': { type: 'string' },
    },
  });
  const [id] = parsed.positionals;
  if (!id || parsed.positionals.length !== 1)
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Extraction requires one declared source ID',
    });
  return {
    id,
    root: parsed.values.root ?? process.cwd(),
    ref: parsed.values.ref ?? 'HEAD',
    binary: parsed.values.codex ?? 'codex',
    attempts: limit({ value: parsed.values.attempts, fallback: 3, minimum: 1, maximum: 3 }),
    deadlineMilliseconds: limit({
      value: parsed.values['deadline-ms'],
      fallback: 600_000,
      minimum: 100,
      maximum: 1_800_000,
    }),
  };
}

function limit(options: {
  value: string | undefined;
  fallback: number;
  minimum: number;
  maximum: number;
}) {
  if (options.value === undefined) return options.fallback;
  const number = Number(options.value);
  if (
    !/^[0-9]+$/.test(options.value) ||
    !Number.isSafeInteger(number) ||
    number < options.minimum ||
    number > options.maximum
  )
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: `Extraction limit must be between ${options.minimum} and ${options.maximum}`,
    });
  return number;
}
