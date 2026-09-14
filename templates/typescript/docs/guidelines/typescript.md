# TypeScript standard

Use Ultracite's ESLint and Prettier presets with `eslint-plugin-boundaries` for TypeScript projects. Use the applicable official framework presets and preserve useful framework checks they do not cover. This standard does not apply to code written in other languages.

## Agreed preferences

- Maximum cyclomatic complexity: 20.
- Maximum cognitive complexity: 15.
- Maximum nesting depth: 3.
- Maximum function parameters: 4.
- Prettier print width: 100; use single quotes.
- Allow unused variables, parameters and caught errors whose names begin with `_`.
- Allow `value == null` and `value != null`; retain strict equality for other comparisons.

Keep the remaining preset rules. Refactor nonconforming code rather than adding rule exclusions to silence findings. Adapt source coverage and TypeScript resolution to the project's actual layout; that does not authorize a different set of style or readability preferences.

## Module boundaries

Define permitted imports from real project modules and their responsibilities. Keep server-only behavior out of client code and prevent dependencies that contradict the owning domain. Do not copy another project's module names or create empty layers to satisfy a template.

Record the concrete module policy and verification commands here when adopting the standard. Respect an explicit owner decision to defer adoption; do not replace this standard with an agent's preferred linter or arbitrary rule changes.
