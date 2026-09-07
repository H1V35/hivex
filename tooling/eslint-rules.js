import path from 'node:path';

const COMPOUND_SUFFIXES = new Set([
  'android',
  'config',
  'd',
  'integration',
  'ios',
  'native',
  'test',
  'web',
]);
const UPPERCASE_CONVENTION_FILENAMES = new Map(
  ['AGENTS.md', 'CLAUDE.md', 'README.md'].map((filename) => [filename.toLowerCase(), filename]),
);

function contextFilename(context) {
  return context.filename ?? context.getFilename();
}

function contextCwd(context) {
  return context.cwd ?? context.getCwd();
}

function relativeFilename(context) {
  return path.relative(contextCwd(context), contextFilename(context)).split(path.sep).join('/');
}

const noBarrel = {
  meta: {
    type: 'problem',
    docs: { description: 'Forbid re-exporting index.ts barrels under src.' },
    messages: { barrel: 'Re-exporting index.ts barrels are forbidden under src.' },
    schema: [
      {
        type: 'object',
        properties: { allow: { type: 'array', items: { type: 'string' }, uniqueItems: true } },
        additionalProperties: false,
      },
    ],
  },
  create(context) {
    const filename = relativeFilename(context);
    const allowed = new Set(context.options[0]?.allow ?? []);
    if (
      !filename.startsWith('src/') ||
      path.posix.basename(filename) !== 'index.ts' ||
      allowed.has(filename)
    ) {
      return {};
    }
    return {
      ExportAllDeclaration(node) {
        context.report({ node, messageId: 'barrel' });
      },
      ExportNamedDeclaration(node) {
        if (node.source !== null) context.report({ node, messageId: 'barrel' });
      },
    };
  },
};

const filenameCase = {
  meta: {
    type: 'suggestion',
    docs: { description: 'Require kebab-case source filenames.' },
    messages: {
      conventionFilename:
        'Documentation filename `{{filename}}` must preserve the `{{expected}}` uppercase convention.',
      filename: 'Source filename `{{filename}}` must be kebab-case.',
    },
    schema: [],
  },
  create(context) {
    return {
      Program(node) {
        const filename = relativeFilename(context);
        const filenameBasename = path.posix.basename(filename);
        const uppercaseConvention = UPPERCASE_CONVENTION_FILENAMES.get(
          filenameBasename.toLowerCase(),
        );
        if (uppercaseConvention !== undefined) {
          if (filenameBasename !== uppercaseConvention) {
            context.report({
              node,
              messageId: 'conventionFilename',
              data: { expected: uppercaseConvention, filename: filenameBasename },
            });
          }
          return;
        }
        if (!/\.tsx?$/.test(filename)) return;
        const basename = filenameBasename.replace(/\.tsx?$/, '');
        if (
          basename.startsWith('_') ||
          basename.startsWith('+') ||
          (basename.startsWith('[') && basename.endsWith(']'))
        ) {
          return;
        }
        const parts = basename.split('.');
        while (parts.length > 1 && COMPOUND_SUFFIXES.has(parts.at(-1))) parts.pop();
        const stem = parts.join('.');
        if (/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(stem)) return;
        context.report({
          node,
          messageId: 'filename',
          data: { filename: path.posix.basename(filename) },
        });
      },
    };
  },
};

export default {
  meta: { name: 'hivex-rules', version: '1.0.0' },
  rules: { 'filename-case': filenameCase, 'no-barrel': noBarrel },
};
