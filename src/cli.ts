#!/usr/bin/env bun
import { groundingCommand } from './grounding/command.ts';
import { admitCommand } from './graph/admission.ts';
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
import { reviewCohortCommand } from './graph/review-cohort.ts';
import { comparisonCommand } from './graph/comparison.ts';
import { comparisonCohortCommand } from './graph/comparison-cohort.ts';
import { comparisonPlanCommand } from './graph/comparison-plan.ts';

function isCohort(args: string[]) {
  return args.some((arg) =>
    ['--all', '--show', '--export', '--discard'].includes(arg.split('=')[0] ?? ''),
  );
}

function dispatchGraph(args: string[]) {
  if (args[0] === 'admit') return admitCommand(args.slice(1));
  if (args[0] === 'compare-plan') return comparisonPlanCommand(args.slice(1));
  if (args[0] === 'compare') {
    const comparisonArgs = args.slice(1);
    if (isCohort(comparisonArgs)) return comparisonCohortCommand(comparisonArgs);
    return comparisonCommand(comparisonArgs);
  }
  if (args[0] === 'review') {
    const reviewArgs = args.slice(1);
    if (isCohort(reviewArgs)) return reviewCohortCommand(reviewArgs);
    return sourceReviewCommand(reviewArgs);
  }
  return graphCommand(args);
}

async function main(args: string[]) {
  if (args[0] === 'ground') return groundingCommand(args.slice(1));
  if (args[0] === 'graph') return dispatchGraph(args.slice(1));
  if (args[0] === 'ingest') return ingestCommand(args.slice(1));
  if (args[0] === 'plan') return planCommand(args.slice(1));
  if (args[0] === 'extract') return extractCommand(args.slice(1));
  if (args.length === 1 && args[0] === '--help')
    return {
      application: 'hivex',
      configuration: 'hivex.json',
      commands: [
        {
          name: 'ground',
          usage:
            'ground <review-claim> --input <admitted-graph> --base <revision> [--root <repo>] [--source <source-id>] [--context-file <revision>:<path>] [--prepare] [--codex <binary>] [--deadline-ms 100..1800000]',
          inspection: 'ground --check <retained-result> --input <admitted-graph> [--root <repo>]',
          scope:
            'Grounds the supplied review claim against complete committed changed files and selected documentary evidence; never approves the whole implementation.',
          modelCalls: 'Native Luna/max for execution; --prepare and --check make no model calls.',
        },
        {
          name: 'graph',
          usage: 'graph build [--root <repo>] [--store <file>] [--export]',
          admission:
            'graph admit --input <file> [--root <repo>] [--reviews <store>] [--comparisons <store>] [--neighbors 0..8] [--export] [--max-bytes 1024..268435456]',
          inspection: 'graph check --input <file> [--root <repo>] [--against <commit>]',
          query:
            'graph <search|read|neighbors> <query-or-id> --input <file> [--root <repo>] [--against <commit>] [--limit 1..20] [--max-bytes 1024..524288]',
          continuation: 'graph neighbors <id> --input <file> [--cursor <continuation>]',
          review:
            'graph review <source-id> --input <file> [--feedback <comparison-result>] [--root <repo>] [--against <commit>] [--codex <binary>] [--deadline-ms 100..1800000] [--prepare]',
          reviewCohort:
            'graph review --all --input <file> [--store <file>] [--root <repo>] [--max-units 0..2048] [--codex <binary>] [--deadline-ms 100..1800000]',
          reviewReuse:
            'graph review --all --input <new-graph> --from <old-graph> --reuse <complete-review-export> [--store <file>] [--root <repo>] [--max-units 0..2048]',
          reviewEvidence:
            'graph review <--show <source-id>|--export> --input <file> [--store <file>] [--max-bytes 1024..134217728]',
          reviewRetirement:
            'graph review --discard <exact-plan-hash> [--store <file>] [--root <repo>]',
          reviewRecovery:
            'graph review --all --retry-failed <source-id> --input <file> [--attempts 1..3] [--max-units 1..2048] [--root <repo>] [--store <file>] [--deadline-ms 100..1800000]',
          comparison:
            'graph compare <source-id> <other-source-id> --input <file> [--root <repo>] [--against <commit>] [--codex <binary>] [--deadline-ms 100..1800000] [--prepare]',
          comparisonCohort:
            'graph compare --all|--show <pair-id>|--export --input <file> [--root <repo>] [--store <file>] [--max-units 0..2048] [--max-bytes 1024..134217728] [--neighbors 0..8] [--deadline-ms 100..1800000]',
          comparisonRetirement:
            'graph compare --discard <plan-hash> [--root <repo>] [--store <file>]',
          comparisonRecovery:
            'graph compare --all --retry-failed <pair-id> --input <file> [--attempts 1..3] [--max-units 1..2048] [--neighbors 0..8] [--root <repo>] [--store <file>] [--deadline-ms 100..1800000]',
          comparisonReuse:
            'graph compare --all --input <new-graph> --from <old-graph> --reuse <complete-comparison-export> [--root <repo>] [--store <file>] [--neighbors 0..8] [--max-units 0..2048]',
          comparisonPlan:
            'graph compare-plan --input <file> [--root <repo>] [--against <commit>] [--max-bytes 1024..8388608] [--neighbors 0..8]',
          modelCalls:
            'Review and compare use native Luna/max; --prepare and deterministic graph operations make no model calls. Candidates remain unaccepted.',
        },
        {
          name: 'ingest',
          usage:
            'ingest [--root <repo>] [--store <file>] [--ref <commit>] [--collection <id>] [--codex <binary>] [--max-units 0..2048] [--attempts 1..3] [--deadline-ms 100..1800000]',
          inspection:
            'ingest --show <source-id> | --export [--root <repo>] [--store <file>] [--max-bytes 1024..134217728]',
          discard: 'ingest --discard <exact-plan-hash> [--root <repo>] [--store <file>]',
          reuse:
            'ingest --reuse <complete-export> --ref <commit> --max-units 0 [--root <repo>] [--store <file>]',
          retry:
            'ingest --retry-failed <source-id> [--root <repo>] [--store <file>] [--attempts 1..3]',
          revise:
            'ingest --revise <source-id> --input <candidate-graph> --feedback <review-result-or-export> [--root <repo>] [--store <file>] [--codex <binary>] [--prepare] [--attempts 1..3] [--deadline-ms 100..1800000]',
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
            'extract <source-id> [--root <repo>] [--ref <commit>] [--codex <binary>] [--attempts 1..3] [--deadline-ms 100..1800000]',
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
