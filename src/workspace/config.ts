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
export const repositoryPath = pattern.refine(
  (value) =>
    !/[#*?[\]{}!]/u.test(value) &&
    ![...value].some((char) => char.charCodeAt(0) < 32) &&
    !value.split('/').some((part) => part === '' || part === '.'),
  'Paths must be exact repository paths',
);

const collection = z.strictObject({
  id: z.string().regex(/^[a-z][a-z0-9-]{0,47}$/),
  include: z
    .array(
      z.union([
        pattern,
        z.strictObject({
          path: repositoryPath,
          anchor: z
            .string()
            .min(1)
            .max(256)
            .regex(/^[^\s#]+$/u),
        }),
      ]),
    )
    .min(1)
    .max(64),
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
  relationIndexes: z
    .array(
      z.strictObject({
        path: repositoryPath,
        format: z.literal('compi-adr-supersession-index'),
      }),
    )
    .max(8)
    .default([]),
});
export type Collection = z.infer<typeof collection>;
export type ProjectConfig = z.infer<typeof config>;

export function parseConfig(text: string): ProjectConfig {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    throw new HivexError({ code: 'INVALID_CONFIG', message: 'hivex.json must contain valid JSON' });
  }
  const result = config.safeParse(value);
  if (!result.success)
    throw new HivexError({ code: 'INVALID_CONFIG', message: result.error.message });
  const ids = result.data.collections.map((item) => item.id);
  if (new Set(ids).size !== ids.length)
    throw new HivexError({ code: 'INVALID_CONFIG', message: 'Collection IDs must be unique' });
  const paths = result.data.relationIndexes.map((item) => item.path);
  if (new Set(paths).size !== paths.length)
    throw new HivexError({
      code: 'INVALID_CONFIG',
      message: 'Relation index paths must be unique',
    });
  return result.data;
}

export type Selection = { collection: Collection; anchor: string | null };

export function selectionsFor(path: string, collections: Collection[]): Selection[] {
  const selections = collections.flatMap((collection) => {
    if (collection.exclude.some((glob) => new Bun.Glob(glob).match(path))) return [];
    const sections = collection.include.flatMap((include) =>
      typeof include !== 'string' && include.path === path
        ? [{ collection, anchor: include.anchor }]
        : [],
    );
    const whole = collection.include.some(
      (include) => typeof include === 'string' && new Bun.Glob(include).match(path),
    );
    const selected: Selection[] = sections;
    if (whole) selected.push({ collection, anchor: null });
    return selected;
  });
  if (selections.length > 1 && selections.some((item) => item.anchor === null))
    throw new HivexError({
      code: 'AMBIGUOUS_COLLECTION',
      message: `Overlapping selections include ${path}`,
    });
  return selections;
}

export function validateSectionPaths(
  config: ProjectConfig,
  declared: { path: string; selections: Selection[] }[],
) {
  const available = new Set(
    declared.flatMap((file) =>
      file.selections.map((item) => JSON.stringify([file.path, item.anchor, item.collection.id])),
    ),
  );
  for (const collection of config.collections) {
    for (const include of collection.include) {
      if (
        typeof include !== 'string' &&
        !available.has(JSON.stringify([include.path, include.anchor, collection.id]))
      )
        throw new HivexError({
          code: 'SECTION_NOT_FOUND',
          message: `Section path must be a declared Markdown document: ${include.path}`,
        });
    }
  }
}
