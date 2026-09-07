import js from '@eslint/js';
import { defineConfig } from 'eslint/config';
import globals from 'globals';
import tseslint from 'typescript-eslint';

import hivexRules from './tooling/eslint-rules.js';
import quality from './tooling/eslint-quality.js';

/** Type-aware quality rules shared by Hivex source and tooling. */
export default defineConfig(
  js.configs.recommended,
  ...tseslint.configs.recommendedTypeChecked,
  quality,
  {
    languageOptions: { globals: globals.node },
    plugins: {
      hivex: hivexRules,
    },
    linterOptions: {
      reportUnusedDisableDirectives: 'error',
    },
    rules: {
      // Async test doubles can satisfy a Promise-returning contract without awaiting.
      '@typescript-eslint/require-await': 'off',
      // Deprecated APIs fail at introduction instead of waiting for an audit sweep (#932).
      '@typescript-eslint/no-deprecated': 'error',
      // Re-exporting barrels hide dependency direction; keep source imports explicit.
      'hivex/no-barrel': 'error',
      // Use predictable kebab-case filenames.
      'hivex/filename-case': 'error',
      // The codebase already uses the `_`-prefix convention for intentionally
      // unused parameters/variables; make the rule honour it.
      '@typescript-eslint/no-unused-vars': [
        'error',
        {
          argsIgnorePattern: '^_',
          varsIgnorePattern: '^_',
          caughtErrorsIgnorePattern: '^_',
        },
      ],
    },
  },
  {
    // Tests (and their helpers under test/) assert on raw JSON responses on
    // purpose — poking untyped bodies is the point. The unsafe-any family
    // stays ON for production src.
    files: ['**/*.test.ts', 'test/**/*.ts'],
    rules: {
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-unsafe-assignment': 'off',
      '@typescript-eslint/no-unsafe-member-access': 'off',
      '@typescript-eslint/no-unsafe-argument': 'off',
    },
  },
  {
    // Configuration and standalone JS scripts do not belong to the TS program.
    ...tseslint.configs.disableTypeChecked,
    files: ['**/*.{js,cjs,mjs}'],
  },
);
