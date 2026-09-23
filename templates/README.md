# Language-specific project standards

Apply a template only to a project using that language, keeping the standard in the project's own guidelines. General workflow skills and the foundation copied by `hivex init` remain language-neutral.

| Language | Template | Suggested destination |
| --- | --- | --- |
| TypeScript | [TypeScript standard](typescript/docs/guidelines/typescript.md) | `docs/guidelines/typescript.md` |
| Rust | [Rust standard](rust/docs/guidelines/rust.md) | `docs/guidelines/rust.md` |

The adopting agent applies the standard and links it from the project's documentation map and engineering guide. Respect an explicit owner decision to postpone adoption. Installing tools and configuring the project's actual dependency boundaries are separate implementation work; copying a guideline does not establish that the linter is installed or passing.
