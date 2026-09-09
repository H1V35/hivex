import { parseArgs } from 'node:util';
import { HivexError } from './errors.ts';
import { KnowledgeStore } from './knowledge-store.ts';

const DEFAULT_KEEP_COMPLETED = 8;
const DEFAULT_KEEP_CACHES = 64;

function retention(value: string | undefined, name: string, fallback: number) {
  if (value === undefined) return fallback;
  const number = Number(value);
  if (!Number.isInteger(number) || number < 0 || number > 4096)
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: `${name} must be an integer between 0 and 4096`,
    });
  return number;
}

export function knowledgeMaintenance(args: string[]) {
  const parsed = parseArgs({
    args,
    allowPositionals: true,
    strict: true,
    options: {
      root: { type: 'string' },
      'keep-completed': { type: 'string' },
      'keep-caches': { type: 'string' },
      'acknowledge-uncertain': { type: 'boolean' },
    },
  });
  const command = parsed.positionals[0];
  if (!command || parsed.positionals.length !== 1 || !['recover', 'prune'].includes(command))
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message:
        'Use recover [--acknowledge-uncertain] or prune [--keep-completed <count>] [--keep-caches <count>]',
    });
  if (
    command === 'recover' &&
    (parsed.values['keep-completed'] !== undefined || parsed.values['keep-caches'] !== undefined)
  )
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'recover does not accept retention options',
    });
  if (command === 'prune' && parsed.values['acknowledge-uncertain'])
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'prune does not accept --acknowledge-uncertain',
    });

  const root = parsed.values.root ?? process.cwd();
  using store = new KnowledgeStore(root);
  if (command === 'recover')
    return {
      command,
      modelCalls: 0,
      ...store.recover({
        acknowledgeUncertain: parsed.values['acknowledge-uncertain'] ?? false,
      }),
    };

  using _lease = store.updateLease();
  return {
    command,
    modelCalls: 0,
    ...store.prune({
      keepCompleted: retention(
        parsed.values['keep-completed'],
        '--keep-completed',
        DEFAULT_KEEP_COMPLETED,
      ),
      keepCaches: retention(parsed.values['keep-caches'], '--keep-caches', DEFAULT_KEEP_CACHES),
    }),
  };
}
