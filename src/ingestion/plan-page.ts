import { HivexError } from '../errors.ts';
import { encodedBytes } from '../retrieval/read.ts';
import type { createPlan } from './plan.ts';
import type { planArguments } from './plan-arguments.ts';

type Plan = ReturnType<typeof createPlan>;

function position(plan: Plan, cursor: string | undefined) {
  if (cursor === undefined) return 0;
  const match = /^p1\.([a-f0-9]{64})\.([0-9]+)$/.exec(cursor);
  if (!match)
    throw new HivexError({ code: 'INVALID_CURSOR', message: 'Invalid plan continuation cursor' });
  const offset = Number(match[2]);
  if (match[1] !== plan.planHash || !Number.isSafeInteger(offset) || offset >= plan.units.length)
    throw new HivexError({
      code: 'CURSOR_MISMATCH',
      message: 'Resume the same source cohort, processing contract and Git snapshot',
    });
  return offset;
}

export function pagePlan(plan: Plan, options: ReturnType<typeof planArguments>) {
  const start = position(plan, options.cursor);
  let next = start;
  const units: Plan['units'] = [];
  const response = (end: number) => ({
    ...plan,
    units,
    page: { start, end },
    continuation: end < plan.units.length ? `p1.${plan.planHash}.${end}` : null,
  });
  for (const unit of plan.units.slice(start)) {
    units.push(unit);
    const requiredBytes = encodedBytes(response(next + 1));
    if (requiredBytes > options.maxBytes) {
      units.pop();
      if (!units.length)
        throw new HivexError({
          code: 'PLAN_UNIT_EXCEEDS_BUDGET',
          message: 'The next complete plan unit does not fit; increase --max-bytes',
          details: { requiredBytes, maximumBytes: 65_536, source: unit.id },
        });
      break;
    }
    next += 1;
    if (units.length === options.limit) break;
  }
  const result = response(next);
  if (encodedBytes(result) > options.maxBytes)
    throw new HivexError({
      code: 'OUTPUT_BUDGET',
      message: 'Plan metadata exceeds the output budget',
    });
  return result;
}
