import sonarjs from 'eslint-plugin-sonarjs';

/** Shared syntax/complexity policy; workspace presets still own their platform and types. */
export default {
  plugins: {
    // Only this audited rule is exposed; no broad or legacy recommended preset.
    sonarjs: {
      meta: sonarjs.meta,
      rules: { 'cognitive-complexity': sonarjs.rules['cognitive-complexity'] },
    },
  },
  rules: {
    complexity: ['error', 20],
    'sonarjs/cognitive-complexity': ['error', 15],
    'max-params': ['error', 4],
    'max-depth': ['error', 3],
    eqeqeq: ['error', 'always', { null: 'ignore' }],
    'no-nested-ternary': 'error',
    'no-restricted-syntax': [
      'error',
      {
        selector: 'ConditionalExpression ConditionalExpression',
        message: 'Nested or chained ternaries are forbidden. Use a guard clause or a named helper.',
      },
    ],
    'no-else-return': ['error', { allowElseIf: false }],
    'no-unneeded-ternary': 'error',
  },
};
