import { diagnostic } from './cli/diagnostic.ts';
import { argumentsFor } from './cli/arguments.ts';
import { loadSnapshot } from './workspace/snapshot.ts';
import { search } from './retrieval/search.ts';
import { read } from './retrieval/read.ts';
import { relations } from './relations/query.ts';
import { extractCommand } from './ingestion/command.ts';
import { planCommand } from './ingestion/plan.ts';
import { ingestCommand } from './ingestion/run.ts';
import { graphCommand } from './graph/command.ts';
import { sourceReviewCommand } from './graph/source-review.ts';

async function main(args: string[]) {
  if (args[0] === 'graph' && args[1] === 'review') return sourceReviewCommand(args.slice(2));
  if (args[0] === 'graph') return graphCommand(args.slice(1));
  if (args[0] === 'ingest') return ingestCommand(args.slice(1));
  if (args[0] === 'plan') return planCommand(args.slice(1));
  if (args[0] === 'extract') return extractCommand(args.slice(1));
  if (args.length === 1 && args[0] === '--help')
    return {
      application: 'hivex',
      configuration: 'hivex.json',
      commands: [
        {
          name: 'graph',
          usage: 'graph build [--root <repo>] [--store <file>] [--export]',
          inspection: 'graph check --input <file> [--root <repo>] [--against <commit>]',
          query:
            'graph <search|read|neighbors> <query-or-id> --input <file> [--root <repo>] [--against <commit>] [--limit 1..20] [--max-bytes 1024..524288]',
          continuation: 'graph neighbors <id> --input <file> [--cursor <continuation>]',
          review:
            'graph review <source-id> --input <file> [--root <repo>] [--against <commit>] [--codex <binary>] [--deadline-ms 100..900000] [--prepare]',
          modelCalls:
            'Review uses native Luna/max; --prepare and all other graph operations make no model calls. Candidates remain unaccepted.',
        },
        {
          name: 'ingest',
          usage:
            'ingest [--root <repo>] [--store <file>] [--ref <commit>] [--collection <id>] [--codex <binary>] [--max-units 0..2048] [--attempts 1..3] [--deadline-ms 100..900000]',
          inspection:
            'ingest --show <source-id> [--root <repo>] [--store <file>] [--max-bytes 1024..8388608]',
          discard: 'ingest --discard <exact-plan-hash> [--root <repo>] [--store <file>]',
          modelCalls:
            'Persists unaccepted candidates through native Codex; inspection and discard make no model calls.',
        },
        {
          name: 'plan',
          usage:
            'plan [--root <repo>] [--ref <commit>] [--collection <id>] [--cursor <continuation>] [--limit 1..20] [--max-bytes 1024..65536]',
          modelCalls:
            'None. Inventory the declared extraction inputs and their processing contract.',
        },
        {
          name: 'extract',
          usage:
            'extract <source-id> [--root <repo>] [--ref <commit>] [--codex <binary>] [--attempts 1..3] [--deadline-ms 100..900000]',
          modelCalls:
            'Uses the existing Codex ChatGPT subscription and returns an unaccepted candidate.',
        },
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
        {
          name: 'relations',
          usage:
            'relations <source-id> [--root <repo>] [--ref <commit>] [--cursor <continuation>] [--limit 1..20] [--max-bytes 1024..65536]',
        },
      ],
      authority: 'Declarations are exposed; effective currentness is not established.',
    };
  const options = argumentsFor(args);
  const { command, value, root, ref } = options;
  const snapshot = loadSnapshot({
    root: root,
    ref: ref,
    selection: command === 'search' ? { collection: options.collection } : { sourceId: value },
  });
  if (command === 'search') return search(snapshot, value, options);
  if (command === 'relations') return relations({ ...options, snapshot, id: value });
  return read(snapshot, value, options);
}

try {
  const result = await main(Bun.argv.slice(2));
  process.stdout.write(JSON.stringify(result) + '\n');
  if ('status' in result && result.status === 'failed') process.exitCode = 1;
} catch (error) {
  process.stderr.write(JSON.stringify(diagnostic(error)) + '\n');
  process.exitCode = 1;
}
