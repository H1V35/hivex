import publicInterface from "eslint-plugin-boundaries";
import config from "ultracite/eslint/core";

export default [
  ...config,
  {
    languageOptions: { globals: { Bun: "readonly" } },
    rules: {
      "n/hashbang": [
        "error",
        {
          additionalExecutables: ["test/codex-server.mjs"],
          executableMap: { ".mjs": "bun", ".ts": "bun" },
        },
      ],
    },
  },
  {
    files: ["src/**/*.ts", "test/**/*.mjs"],
    plugins: { boundaries: publicInterface },
    rules: {
      ...publicInterface.configs.strict.rules,
      "boundaries/dependencies": [
        "error",
        {
          default: "disallow",
          policies: [
            { allow: { to: { module: { origin: ["external", "core"] } } } },
            {
              allow: {
                to: {
                  file: { categories: { anyOf: ["documents", "shared"] } },
                },
              },
              from: { file: { categories: "documents" } },
            },
            {
              allow: {
                to: {
                  file: {
                    categories: { anyOf: ["knowledge", "documents", "shared"] },
                  },
                },
              },
              from: { file: { categories: "knowledge" } },
            },
            {
              allow: {
                to: {
                  file: {
                    categories: {
                      anyOf: ["review", "knowledge", "documents", "shared"],
                    },
                  },
                },
              },
              from: { file: { categories: "review" } },
            },
            {
              allow: { to: { file: { categories: "model" } } },
              from: { file: { categories: "model" } },
            },
            {
              allow: {
                to: {
                  file: { categories: { anyOf: ["retrieval", "shared"] } },
                },
              },
              from: { file: { categories: "retrieval" } },
            },
            {
              allow: { to: { file: { categories: { noneOf: ["test"] } } } },
              from: { file: { categories: "commands" } },
            },
            {
              allow: { to: { file: { categories: "*" } } },
              from: { file: { categories: "test" } },
            },
          ],
        },
      ],
    },
    settings: {
      "boundaries/files": [
        { category: "documents", pattern: "src/{documents,markdown}.ts" },
        {
          category: "knowledge",
          pattern:
            "src/{knowledge-model,ingestion-units,knowledge-store,knowledge-snapshot}.ts",
        },
        { category: "review", pattern: "src/{implementation,review}.ts" },
        { category: "model", pattern: "src/model/**" },
        { category: "retrieval", pattern: "src/retrieval/**" },
        {
          category: "commands",
          pattern: [
            "src/{cli,knowledge,knowledge-maintenance,snapshot-command}.ts",
            "src/cli/**",
          ],
        },
        { category: "shared", pattern: "src/{errors,ordering,runtime.d}.ts" },
        { category: "test", pattern: ["src/**/*.test.ts", "test/**"] },
      ],
      "boundaries/root-path": import.meta.dirname,
      "import/resolver": { typescript: { project: "./tsconfig.json" } },
    },
  },
];
