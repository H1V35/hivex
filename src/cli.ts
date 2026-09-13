#!/usr/bin/env bun
import { checkReview } from './review.ts';
import { documentCommand } from './documents.ts';
import { knowledgeMaintenance } from './knowledge-maintenance.ts';
import { knowledgeCommand } from './knowledge.ts';
import { projectInitializationCommand } from './project-initialization.ts';
import { snapshotCommand } from './snapshot-command.ts';
import { diagnostic } from './cli/diagnostic.ts';

const main = async (input: string[]) => {
  if (!input.length || input.includes('--help')) {
    return {
      application: 'hivex',
      commands: [
        {
          modelCalls: 0,
          name: 'sources',
          usage:
            'sources [--root <project>] [--limit <count>] [--cursor <continuation>] [--max-bytes <bytes>]',
        },
        {
          modelCalls: 0,
          name: 'read',
          usage:
            'read <document> [--root <project>] [--from <line>] [--to <line>] [--max-bytes <bytes>]',
        },
        {
          modelCalls:
            'One extraction and one check per batch; retained work resumes within the same budget.',
          name: 'update',
          usage:
            'update [--root <project>] [--max-calls <total>] [--max-input-bytes <total>] [--repair <document> --reason <correction>]',
        },
        {
          modelCalls: 0,
          name: 'search',
          usage: 'search <query> [--root <project>] [--limit <count>]',
        },
        {
          modelCalls: 0,
          name: 'neighbors',
          usage: 'neighbors <decision-id> [--root <project>] [--limit <count>]',
        },
        {
          modelCalls:
            'One bounded automatic update/check batch and Luna/max assistance share a total budget; identical retained answers are reused.',
          name: 'ask',
          usage:
            'ask <task> [--root <project>] [--source <document>] [--max-calls <total>] [--max-input-bytes <total>] [--max-context-bytes <bytes>]',
        },
        {
          modelCalls:
            'One automatic update/check batch and a review share one budget. Checking saved versions needs no model.',
          name: 'review',
          usage:
            'review <task> --base <git-ref> [--root <project>] [--source <document>] [--max-calls <total>] [--max-context-bytes <bytes>] | review --check <saved-report.json> [--root <project>]',
        },
        {
          modelCalls: 0,
          name: 'recover',
          usage: 'recover [--root <project>] [--acknowledge-uncertain]',
        },
        {
          modelCalls: 0,
          name: 'prune',
          usage: 'prune [--root <project>] [--keep-completed <count>] [--keep-caches <count>]',
        },
        {
          modelCalls: 0,
          name: 'snapshot',
          usage: 'snapshot export | import | relocate <from> <to> [--root <project>]',
        },
        { modelCalls: 0, name: 'status', usage: 'status [--root <project>]' },
        { modelCalls: 0, name: 'init', usage: 'init [--root <project>]' },
      ],
      configuration:
        'Optional hivex.json with include/exclude/history relative Markdown globs; history sources are focused evidence.',
      modelOptions: '--codex <native-binary> --deadline-ms <100..1800000> --retry-failed',
      purpose: 'Project decisions, dependencies and exceptions for the responsible agent.',
      stage: 'Incremental project knowledge and task/diff assistance for the principal agent.',
    };
  }
  if (input[0] === 'review' && input.includes('--check')) {
    return checkReview(input);
  }
  if (input[0] === 'sources' || input[0] === 'read') {
    return documentCommand(input);
  }
  if (input[0] === 'snapshot') {
    return snapshotCommand(input);
  }
  if (input[0] === 'init') {
    return projectInitializationCommand(input);
  }
  if (input[0] === 'recover' || input[0] === 'prune') {
    return knowledgeMaintenance(input);
  }
  return await knowledgeCommand(input);
};
const printResult = async () => {
  const result: unknown = await main(process.argv.slice(2));
  process.stdout.write(`${JSON.stringify(result)}\n`);
  if (result === null || typeof result !== 'object' || !('status' in result)) {
    return;
  }
  if (result.status === 'failed' || result.status === 'blocked') {
    process.exitCode = 1;
  }
};
try {
  await printResult();
} catch (error) {
  process.stderr.write(`${JSON.stringify(diagnostic(error))}\n`);
  process.exitCode = 1;
}
