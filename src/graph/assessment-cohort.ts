import { parseArgs } from 'node:util';
import { resolve } from 'node:path';
import { z } from 'zod';
import { parseLimit } from '../cli/arguments.ts';
import { HivexError } from '../errors.ts';
import { knowledgeModel } from '../model/profile.ts';
import { reportedProfileSchema, digest } from './snapshot.ts';
import { hash } from '../sources/markdown.ts';

export const rejectedOutputSchema = z.strictObject({
  text: z.string().max(8 * 1024 * 1024),
  hash: digest,
});

export function validateRejectedOutput(
  rejected: z.infer<typeof rejectedOutputSchema> | null | undefined,
  outcome: string,
  assessment: unknown,
) {
  if (
    rejected &&
    (assessment !== null || outcome !== 'invalid-output' || hash(rejected.text) !== rejected.hash)
  )
    throw new HivexError({
      code: 'INVALID_REVIEW_STORE',
      message: 'Rejected model output is altered or presented as an assessment',
    });
}

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
        'retry-failed': { type: 'string' },
        attempts: { type: 'string' },
      },
    }).values;
  } catch (error) {
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: error instanceof Error ? error.message : 'Invalid cohort review arguments',
    });
  }
}

function validateInspectionFlags(values: ReturnType<typeof input>) {
  if (
    (values['retry-failed'] !== undefined && values.reuse !== undefined) ||
    (values.attempts !== undefined && values['retry-failed'] === undefined)
  )
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message:
        'Choose an explicit --all --retry-failed assessment; --attempts belongs only to recovery',
    });
  if (
    (values.reuse !== undefined || values.from !== undefined) &&
    (!values.all || !values.reuse || !values.from)
  )
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: '--reuse and --from belong together to cohort --all',
    });

  if (
    !values.all &&
    [values.codex, values['max-units'], values['deadline-ms'], values['retry-failed']].some(
      (value) => value !== undefined,
    )
  )
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Inspection and retirement cannot be mixed with execution options',
    });
  if (values['max-bytes'] !== undefined && !values.show && !values.export)
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
  validateInspectionFlags(values);
  const maxUnits = parseLimit(values['max-units'], { fallback: 20, minimum: 0, maximum: 2048 });
  if (values['retry-failed'] !== undefined && maxUnits === 0)
    throw new HivexError({ code: 'INVALID_ARGUMENT', message: 'Retry requires a processing slot' });
  const root = values.root ?? process.cwd();
  return {
    root,
    reuse: values.reuse,
    from: values.from,
    retry: values['retry-failed'],
    maximumAttempts: parseLimit(values.attempts, { fallback: 3, minimum: 1, maximum: 3 }),
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
    maxUnits,
    maxBytes: parseLimit(values['max-bytes'], {
      fallback: 16384,
      minimum: 1024,
      maximum: 128 * 1024 * 1024,
    }),
    deadlineMilliseconds: parseLimit(values['deadline-ms'], {
      fallback: 600000,
      minimum: 100,
      maximum: 1_800_000,
    }),
  };
}

export function summarize(
  rows: {
    state: string;
    result: { report: { usage: { totalTokens: number } | null } } | null;
    previousAttempts?: { report: { usage: { totalTokens: number } | null } }[];
  }[],
) {
  const results = rows.flatMap((row) => [
    ...(row.previousAttempts ?? []),
    ...(row.result ? [row.result] : []),
  ]);
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
    recordedAttempts: results.length,
    reportedTokens: results.reduce(
      (sum, result) => sum + (result.report.usage?.totalTokens ?? 0),
      0,
    ),
    unmeasuredResults: results.filter((result) => result.report.usage === null).length,
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

export function unresolvedInvocation(report: { outcome: string; [key: string]: unknown }) {
  if (
    report.turnAccepted === 'unknown' ||
    report.cleanup === 'failed' ||
    report.interruption === 'unconfirmed'
  )
    return true;
  if (report.turnAccepted !== 'confirmed')
    return !(
      report.turnAccepted === undefined &&
      report.code === 'MODEL_ADMISSION_FAILED' &&
      ['confirmed', 'not-observed'].includes(String(report.cleanup))
    );
  return (
    report.cleanup !== 'confirmed' ||
    (!['completed', 'invalid-output'].includes(report.outcome) &&
      report.interruption !== 'confirmed')
  );
}

export function retryableAssessment(result: {
  status: string;
  report: { outcome: string; [key: string]: unknown };
}) {
  return (
    result.status === 'failed' &&
    result.report.outcome !== 'completed' &&
    !unresolvedInvocation(result.report)
  );
}
