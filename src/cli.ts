import { diagnostic } from './cli/diagnostic.ts';
import { argumentsFor } from './cli/arguments.ts';
import { loadSnapshot } from './workspace/snapshot.ts';
import { search } from './retrieval/search.ts';
import { read } from './retrieval/read.ts';

function main(args: string[]) {
  if (args.length === 1 && args[0] === '--help')
    return {
      application: 'hivex',
      configuration: 'hivex.json',
      commands: [
        {
          name: 'search',
          usage:
            'search <query> [--root <repo>] [--ref <revision>] [--collection <id>] [--limit 1..20] [--max-bytes 1024..65536]',
        },
        {
          name: 'read',
          usage:
            'read <source-id> [--root <repo>] [--ref <commit>] [--cursor <continuation>] [--max-bytes 1024..65536]',
        },
      ],
      authority: 'Declarations are exposed; effective currentness is not established.',
    };
  const options = argumentsFor(args);
  const { command, value, root, ref } = options;
  const snapshot = loadSnapshot(
    root,
    ref,
    command === 'read' ? { sourceId: value } : { collection: options.collection },
  );
  if (command === 'search') return search(snapshot, value, options);
  return read(snapshot, value, options);
}

try {
  process.stdout.write(JSON.stringify(main(Bun.argv.slice(2))) + '\n');
} catch (error) {
  process.stderr.write(JSON.stringify(diagnostic(error)) + '\n');
  process.exitCode = 1;
}
