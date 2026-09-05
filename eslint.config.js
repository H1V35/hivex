import { defineConfig } from 'eslint/config';
import base from '../eslint.base.config.js';

export default defineConfig(...base, {
  files: ['**/*.{ts,tsx,mts,cts}'],
  languageOptions: {
    parserOptions: { projectService: true, tsconfigRootDir: import.meta.dirname },
  },
  rules: {
    'no-restricted-imports': [
      'error',
      {
        patterns: [
          {
            group: [
              '@compi/*',
              '**/orchestration/**',
              '**/app/**',
              '**/backend/**',
              '**/contracts/**',
            ],
            message: 'Hivex must remain independent of Compi and legacy orchestration.',
          },
        ],
      },
    ],
  },
});
