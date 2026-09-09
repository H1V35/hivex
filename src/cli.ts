#!/usr/bin/env bun
import { documentCommand } from './documents.ts';
import { knowledgeMaintenance } from './knowledge-maintenance.ts';
import { knowledgeCommand } from './knowledge.ts';
import { diagnostic } from './cli/diagnostic.ts';

async function main(args: string[]) {
  if (!args.length || args.includes('--help'))
    return {
      application: 'hivex',
      purpose: 'Project decisions, dependencies and exceptions for the responsible agent.',
      commands: [
        {
          name: 'sources',
          usage:
            'sources [--root <project>] [--limit <count>] [--cursor <continuation>] [--max-bytes <bytes>]',
          modelCalls: 0,
        },
        {
          name: 'read',
          usage:
            'read <document> [--root <project>] [--from <line>] [--to <line>] [--max-bytes <bytes>]',
          modelCalls: 0,
        },
        {
          name: 'update',
          usage: 'update [--root <project>] [--max-calls <total>] [--max-input-bytes <total>]',
          modelCalls:
            'One extraction and one check per batch; retained work resumes within the same budget.',
        },
        {
          name: 'search',
          usage: 'search <query> [--root <project>] [--limit <count>]',
          modelCalls: 0,
        },
        {
          name: 'neighbors',
          usage: 'neighbors <decision-id> [--root <project>] [--limit <count>]',
          modelCalls: 0,
        },
        {
          name: 'ask',
          usage:
            'ask <task> [--root <project>] [--source <document>] [--max-calls <total>] [--max-input-bytes <total>] [--max-context-bytes <bytes>]',
          modelCalls:
            'Bounded Luna/max assistance over current indexed evidence; identical retained answers are reused.',
        },
        {
          name: 'recover',
          usage: 'recover [--root <project>] [--acknowledge-uncertain]',
          modelCalls: 0,
        },
        {
          name: 'prune',
          usage: 'prune [--root <project>] [--keep-completed <count>] [--keep-caches <count>]',
          modelCalls: 0,
        },
        { name: 'status', usage: 'status [--root <project>]', modelCalls: 0 },
      ],
      modelOptions: '--codex <native-binary> --deadline-ms <100..1800000> --retry-failed',
      configuration: 'Optional hivex.json with include/exclude relative Markdown globs.',
      stage:
        'Explicit initial updates and task context. Automatic maintenance and diff-review assistance are subsequent deliveries.',
    };
  if (args[0] === 'sources' || args[0] === 'read') return documentCommand(args);
  if (args[0] === 'recover' || args[0] === 'prune') return knowledgeMaintenance(args);
  return knowledgeCommand(args);
}

try {
  const result = await main(process.argv.slice(2));
  process.stdout.write(JSON.stringify(result) + '\n');
  if (
    result &&
    typeof result === 'object' &&
    'status' in result &&
    (result.status === 'failed' || result.status === 'blocked')
  )
    process.exitCode = 1;
} catch (error) {
  process.stderr.write(JSON.stringify(diagnostic(error)) + '\n');
  process.exitCode = 1;
}
