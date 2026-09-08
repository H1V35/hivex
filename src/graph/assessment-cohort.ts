import { parseArgs } from 'node:util';
import { resolve } from 'node:path';
import { z } from 'zod';
import { parseLimit } from '../cli/arguments.ts';
import { HivexError } from '../errors.ts';
import { knowledgeModel } from '../model/profile.ts';
import { reportedProfileSchema } from './snapshot.ts';

function input(args: string[]) {
  try {
    return parseArgs({
      args,
      strict: true,
      options: {
        all: { type: 'boolean' },
        input: { type: 'string' },
        root: { type: 'string' },
        store: { type: 'string' },
        against: { type: 'string' },
        codex: { type: 'string' },
        'max-units': { type: 'string' },
        'deadline-ms': { type: 'string' },
        show: { type: 'string' },
        export: { type: 'boolean' },
        discard: { type: 'string' },
        'max-bytes': { type: 'string' },
        neighbors: { type: 'string' },
        reuse: { type: 'string' },
        from: { type: 'string' },
      },
    }).values;
  } catch (error) {
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: error instanceof Error ? error.message : 'Invalid cohort review arguments',
    });
  }
}

function validateInspectionFlags(
  values: ReturnType<typeof input>,
  operation: 'review' | 'compare',
) {
  if (
    (values.reuse !== undefined || values.from !== undefined) &&
    (operation !== 'review' || !values.all || !values.reuse || !values.from)
  )
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: '--reuse and --from belong together to source review --all',
    });

  if (
    !values.all &&
    [values.codex, values['max-units'], values['deadline-ms']].some((value) => value !== undefined)
  )
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Inspection and retirement cannot be mixed with execution options',
    });
  if ((values.all || values.discard !== undefined) && values['max-bytes'] !== undefined)
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: '--max-bytes belongs to inspection or export',
    });
  if (values.discard !== undefined && (values.input !== undefined || values.against !== undefined))
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Retirement uses the retained plan hash, not a replacement graph',
    });
}

export function assessmentArguments(args: string[], operation: 'review' | 'compare') {
  const values = input(args);
  if (values.neighbors !== undefined && (operation !== 'compare' || values.discard !== undefined))
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message:
        '--neighbors belongs to comparison execution or inspection, not source review or retirement',
    });
  if (
    (!values.input && !values.discard) ||
    [values.all, values.show !== undefined, values.export, values.discard !== undefined].filter(
      Boolean,
    ).length !== 1 ||
    Object.values(values).some((value) => value === '')
  )
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message:
        'Choose --all, --show, --export or --discard; non-discard operations require --input',
    });
  validateInspectionFlags(values, operation);
  const root = values.root ?? process.cwd();
  return {
    root,
    reuse: values.reuse,
    from: values.from,
    neighbors: parseLimit(values.neighbors, { fallback: 0, minimum: 0, maximum: 8 }),
    input: values.input ?? '',
    against: values.against,
    store:
      values.store ??
      resolve(root, operation === 'review' ? '.hivex/reviews.sqlite' : '.hivex/comparisons.sqlite'),
    binary: values.codex ?? 'codex',
    show: values.show,
    export: values.export ?? false,
    discard: values.discard,
    maxUnits: parseLimit(values['max-units'], { fallback: 20, minimum: 0, maximum: 2048 }),
    maxBytes: parseLimit(values['max-bytes'], {
      fallback: 16384,
      minimum: 1024,
      maximum: 128 * 1024 * 1024,
    }),
    deadlineMilliseconds: parseLimit(values['deadline-ms'], {
      fallback: 600000,
      minimum: 100,
      maximum: 900000,
    }),
  };
}

export function summarize(
  rows: { state: string; result: { report: { usage: { totalTokens: number } | null } } | null }[],
) {
  const count = (state: string) => rows.filter((row) => row.state === state).length;
  const completed = count('reviewed');
  const failed = count('failed');
  const unresolved = count('running');
  let status = 'pending';
  if (completed === rows.length) status = 'reviewed';
  if (failed || unresolved) status = 'failed';
  return {
    status,
    completed,
    failed,
    unresolved,
    pending: count('pending'),
    reportedTokens: rows.reduce(
      (sum, row) => sum + (row.result?.report.usage?.totalTokens ?? 0),
      0,
    ),
    unmeasuredResults: rows.filter((row) => row.result && row.result.report.usage === null).length,
  };
}

export function validateCompletedInvocation(report: { outcome: string; [key: string]: unknown }) {
  if (report.outcome !== 'completed') return;
  const profile = z.looseObject(reportedProfileSchema.shape).safeParse(report.admission);
  if (
    !profile.success ||
    profile.data.model !== knowledgeModel.name ||
    profile.data.effort !== knowledgeModel.effort ||
    profile.data.modelProvider !== knowledgeModel.provider ||
    profile.data.authType !== 'chatgpt' ||
    profile.data.configuredEndpointOrigin !== 'https://chatgpt.com' ||
    report.cleanup !== 'confirmed' ||
    report.turnAccepted !== 'confirmed' ||
    typeof report.threadId !== 'string' ||
    typeof report.turnId !== 'string'
  )
    throw new HivexError({
      code: 'INVALID_REVIEW_STORE',
      message: 'A completed review lacks its admitted native invocation evidence',
    });
}
