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
    files: ['test/**/*.{ts,mjs}', 'scripts/**/*.mjs'],
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
              allow: { to: { file: { categories: 'test' } } },
              from: { file: { categories: 'test' } },
            },
            {
              allow: { to: { file: { categories: { anyOf: ['scripts', 'configuration'] } } } },
              from: { file: { categories: 'scripts' } },
            },
          ],
        },
      ],
    },
    settings: {
      'boundaries/files': [
        { category: 'test', pattern: 'test/**' },
        { category: 'scripts', pattern: 'scripts/**' },
        { category: 'configuration', pattern: 'package.json' },
      ],
      'boundaries/root-path': import.meta.dirname,
      'import/resolver': { typescript: { project: './tsconfig.json' } },
    },
  },
];
