import { parseArgs } from 'node:util';
import { HivexError } from './errors.ts';
import { KnowledgeStore } from './knowledge-store.ts';

const defaultKeepCompleted = 8;
const defaultKeepCaches = 64;

const retention = function retention(value: string | undefined, name: string, fallback: number) {
  if (value === undefined) {
    return fallback;
  }
  const number = Number(value);
  if (!Number.isSafeInteger(number) || number < 0 || number > 4096) {
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: `${name} must be an integer between 0 and 4096`,
    });
  }
  return number;
};

export const knowledgeMaintenance = function knowledgeMaintenance(argumentsList: string[]) {
  const parsed = parseArgs({
    allowPositionals: true,
    args: argumentsList,
    options: {
      'acknowledge-uncertain': { type: 'boolean' },
      'keep-caches': { type: 'string' },
      'keep-completed': { type: 'string' },
      root: { type: 'string' },
    },
    strict: true,
  });
  const [command] = parsed.positionals;
  if (
    command === undefined ||
    command === '' ||
    !['recover', 'prune'].includes(command) ||
    parsed.positionals.length !== 1
  ) {
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message:
        'Use recover [--acknowledge-uncertain] or prune [--keep-completed <count>] [--keep-caches <count>]',
    });
  }
  if (
    command === 'recover' &&
    (parsed.values['keep-completed'] !== undefined || parsed.values['keep-caches'] !== undefined)
  ) {
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'recover does not accept retention options',
    });
  }
  if (command === 'prune' && parsed.values['acknowledge-uncertain'] === true) {
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'prune does not accept --acknowledge-uncertain',
    });
  }

  const root = parsed.values.root ?? process.cwd();
  using store = new KnowledgeStore(root, {
    update: command === 'prune',
  });
  if (command === 'recover') {
    return {
      command,
      modelCalls: 0,
      ...store.recover({
        acknowledgeUncertain: parsed.values['acknowledge-uncertain'] ?? false,
      }),
    };
  }

  return {
    command,
    modelCalls: 0,
    ...store.prune({
      keepCaches: retention(parsed.values['keep-caches'], '--keep-caches', defaultKeepCaches),
      keepCompleted: retention(
        parsed.values['keep-completed'],
        '--keep-completed',
        defaultKeepCompleted
      ),
    }),
  };
};
