import publicInterface from 'eslint-plugin-boundaries';
import config from 'ultracite/eslint/core';

const limits = {
  cognitiveComplexity: 15,
  cyclomaticComplexity: 20,
  depth: 3,
  parameters: 4,
};
const unusedVariables = {
  argsIgnorePattern: '^_',
  caughtErrorsIgnorePattern: '^_',
  varsIgnorePattern: '^_',
};

export default [
  ...config,
  {
    languageOptions: { globals: { Bun: 'readonly' } },
    rules: {
      complexity: ['error', limits.cyclomaticComplexity],
      eqeqeq: ['error', 'always', { null: 'ignore' }],
      'max-depth': ['error', limits.depth],
      'n/hashbang': [
        'error',
        {
          additionalExecutables: ['test/codex-server.mjs'],
          executableMap: { '.mjs': 'bun', '.ts': 'bun' },
        },
      ],
      'no-eq-null': 'off',
      'sonarjs/cognitive-complexity': ['error', limits.cognitiveComplexity],
      'sonarjs/no-unused-vars': 'off',
      'unicorn/prefer-optional-catch-binding': 'off',
      'unused-imports/no-unused-vars': ['error', unusedVariables],
    },
  },
  {
    files: ['**/*.{ts,tsx,mts,cts}'],
    rules: {
      '@typescript-eslint/max-params': ['error', { max: limits.parameters }],
      '@typescript-eslint/naming-convention': [
        'error',
        { format: ['camelCase', 'PascalCase', 'snake_case'], selector: 'default' },
        { format: null, modifiers: ['requiresQuotes'], selector: 'objectLiteralProperty' },
        {
          format: ['camelCase', 'PascalCase', 'snake_case'],
          leadingUnderscore: 'allow',
          modifiers: ['unused'],
          selector: 'variableLike',
        },
      ],
      '@typescript-eslint/no-unused-vars': ['error', unusedVariables],
    },
  },
  {
    files: ['**/*.{js,mjs,cjs}'],
    rules: {
      'max-params': ['error', limits.parameters],
      'no-unused-vars': ['error', unusedVariables],
    },
  },
  {
    files: ['src/**/*.ts', 'test/**/*.mjs'],
    plugins: { boundaries: publicInterface },
    rules: {
      ...publicInterface.configs.strict.rules,
      'boundaries/dependencies': [
        'error',
        {
          default: 'disallow',
          policies: [
            { allow: { to: { module: { origin: ['external', 'core'] } } } },
            {
              allow: {
                to: {
                  file: { categories: { anyOf: ['documents', 'shared'] } },
                },
              },
              from: { file: { categories: 'documents' } },
            },
            {
              allow: {
                to: {
                  file: {
                    categories: { anyOf: ['knowledge', 'documents', 'shared'] },
                  },
                },
              },
              from: { file: { categories: 'knowledge' } },
            },
            {
              allow: {
                to: {
                  file: {
                    categories: {
                      anyOf: ['review', 'knowledge', 'documents', 'shared'],
                    },
                  },
                },
              },
              from: { file: { categories: 'review' } },
            },
            {
              allow: { to: { file: { categories: 'model' } } },
              from: { file: { categories: 'model' } },
            },
            {
              allow: {
                to: {
                  file: { categories: { anyOf: ['retrieval', 'shared'] } },
                },
              },
              from: { file: { categories: 'retrieval' } },
            },
            {
              allow: { to: { file: { categories: { noneOf: ['test'] } } } },
              from: { file: { categories: 'commands' } },
            },
            {
              allow: { to: { file: { categories: '*' } } },
              from: { file: { categories: 'test' } },
            },
          ],
        },
      ],
    },
    settings: {
      'boundaries/files': [
        { category: 'documents', pattern: 'src/{documents,markdown}.ts' },
        {
          category: 'knowledge',
          pattern:
            'src/{knowledge-model,ingestion-units,knowledge-store,knowledge-snapshot,knowledge-serialization,source-relocation}.ts',
        },
        { category: 'review', pattern: 'src/{implementation,review}.ts' },
        { category: 'model', pattern: 'src/model/**' },
        { category: 'retrieval', pattern: 'src/retrieval/**' },
        {
          category: 'commands',
          pattern: [
            'src/{cli,knowledge,knowledge-maintenance,knowledge-warnings,project-initialization,snapshot-command}.ts',
            'src/cli/**',
          ],
        },
        { category: 'shared', pattern: 'src/{errors,ordering,runtime.d}.ts' },
        { category: 'test', pattern: ['src/**/*.test.ts', 'test/**'] },
      ],
      'boundaries/root-path': import.meta.dirname,
      'import/resolver': { typescript: { project: './tsconfig.json' } },
    },
  },
];
