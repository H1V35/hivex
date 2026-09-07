import { z } from 'zod';
import { HivexError } from '../errors.ts';
import { hash } from '../sources/markdown.ts';
import { repositoryPath } from '../workspace/config.ts';

export const MAX_RELATION_RECORDS = 10_000;

const status = z.enum(['live', 'superseded', 'partially-amended', 'unresolved']);
const reference = z.strictObject({
  path: repositoryPath,
  section: z.string().max(4096),
  anchor: z.string().min(1).max(1024),
});
export type IndexedReference = z.infer<typeof reference>;
const record = z.strictObject({
  id: z.string().min(1).max(2048),
  type: z.enum(['rule', 'unresolved']),
  adr: z.string().regex(/^\d{4}$/),
  kind: z.enum(['summary', 'decision', 'addendum', 'amendment', 'scope', 'strikethrough']),
  text: z.string().max(131_072),
  status,
  source: reference,
  supersededBy: z
    .array(z.strictObject({ adr: z.string().regex(/^\d{4}$/), source: reference }))
    .max(128),
  reason: z.string().max(16_384).optional(),
});
const header = z.strictObject({
  type: z.literal('schema'),
  schemaVersion: z.literal(1),
  format: z.literal('compi-adr-supersession-index'),
  statuses: z
    .array(status)
    .length(4)
    .refine((items) => new Set(items).size === 4),
});

function parsedLine(options: { path: string; text: string; line: number }): unknown {
  try {
    return JSON.parse(options.text);
  } catch {
    throw new HivexError({
      code: 'INVALID_RELATION_INDEX',
      message: 'Relation index must contain valid JSONL',
      details: { path: options.path, line: options.line },
    });
  }
}

export function parseIndex(options: { path: string; text: string }) {
  const { path, text } = options;
  const lines = text.split('\n');
  const first = parsedLine({ path, text: lines[0] ?? '', line: 1 });
  if (!header.safeParse(first).success)
    throw new HivexError({
      code: 'INVALID_RELATION_INDEX',
      message: 'Expected ADR supersession index schema v1',
      details: { path, line: 1 },
    });
  if (lines.slice(1).filter((line) => line.trim()).length > MAX_RELATION_RECORDS)
    throw new HivexError({
      code: 'TOO_MANY_RELATION_RECORDS',
      message: 'At most 10000 indexed records may be loaded per query',
    });
  const ids = new Set<string>();
  const entries = lines.slice(1).flatMap((text, index) => {
    if (!text.trim()) return [];
    const line = index + 2;
    const result = record.safeParse(parsedLine({ path, text, line }));
    if (!result.success)
      throw new HivexError({
        code: 'INVALID_RELATION_INDEX',
        message: 'Invalid ADR supersession record',
        details: { path, line },
      });
    if (ids.has(result.data.id))
      throw new HivexError({
        code: 'DUPLICATE_RELATION_ID',
        message: 'Relation record IDs must be unique within an index',
        details: { path, line, id: result.data.id },
      });
    ids.add(result.data.id);
    return [{ ...result.data, line }];
  });
  return { path, hash: hash(text), entries };
}
