import { z } from 'zod';
import { HivexError } from '../errors.ts';

const pattern = z
  .string()
  .min(1)
  .max(256)
  .refine(
    (value) => !value.startsWith('/') && !value.includes('\\') && !value.split('/').includes('..'),
    'Document patterns must stay inside the repository',
  );
const collection = z.strictObject({
  id: z.string().regex(/^[a-z][a-z0-9-]{0,47}$/),
  include: z.array(pattern).min(1).max(64),
  exclude: z.array(pattern).max(64).default([]),
  default: z.boolean().default(true),
  kind: z.enum(['documentation', 'evidence', 'legacy', 'mixed']).default('documentation'),
  aliasPrefix: z
    .string()
    .regex(/^[A-Za-z][A-Za-z0-9_-]{0,31}$/)
    .optional(),
});
const config = z.strictObject({
  version: z.literal(1),
  collections: z.array(collection).min(1).max(32),
});
export type Collection = z.infer<typeof collection>;
export type ProjectConfig = z.infer<typeof config>;

export function parseConfig(text: string): ProjectConfig {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    throw new HivexError('INVALID_CONFIG', 'hivex.json must contain valid JSON');
  }
  const result = config.safeParse(value);
  if (!result.success) throw new HivexError('INVALID_CONFIG', result.error.message);
  const ids = result.data.collections.map((item) => item.id);
  if (new Set(ids).size !== ids.length)
    throw new HivexError('INVALID_CONFIG', 'Collection IDs must be unique');
  return result.data;
}

export function collectionFor(path: string, collections: Collection[]): Collection | undefined {
  const matches = collections.filter(
    (item) =>
      item.include.some((glob) => new Bun.Glob(glob).match(path)) &&
      !item.exclude.some((glob) => new Bun.Glob(glob).match(path)),
  );
  if (matches.length > 1)
    throw new HivexError('AMBIGUOUS_COLLECTION', `Multiple collections include ${path}`);
  return matches[0];
}
