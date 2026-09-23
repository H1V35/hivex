# Language-specific project standards

Apply the matching standards when adopting Hivex, updating its installed guidance or changing the project's languages. General workflow skills and the foundation copied by `hivex init` remain language-neutral; the adopting agent selects and applies the language templates.

Identify the languages of maintained source code in each project or workspace package using its sources, manifests and build configuration. A package manager used only to install Hivex does not make a project TypeScript; dependency, vendored and generated code do not determine the project's standards.

| Language | Template | Suggested destination |
| --- | --- | --- |
| TypeScript | [TypeScript standard](typescript/docs/guidelines/typescript.md) | `docs/guidelines/typescript.md` |
| Rust | [Rust standard](rust/docs/guidelines/rust.md) | `docs/guidelines/rust.md` |

For a single-language project, read and apply its matching template. For a mixed project, apply each matching template only to the code and packages written in that language. Use this catalogue from the installed Hivex version; if a language has no available template, preserve its accepted guidelines and report the gap rather than substituting another language's rules. Missing guidance does not authorize a package upgrade.

Integrate the standard into the project's existing guidelines, preserving accepted decisions and explicit deferrals. Resolve genuine conflicts with the owner instead of silently replacing those decisions. Record which language and project/package scope each guideline covers in the documentation map and link the applicable guidelines from the engineering guide. Keep DDD and other shared architecture/workflow policy in that existing language-neutral authority; do not duplicate it in each language guide. Revisit this mapping when the stack or relevant installed guidance changes.

Installing tools and configuring the project's actual dependency boundaries are separate implementation work within the authorized scope. Verify the applicable checks before reporting the standard as enforced; copying a guideline does not establish that its linter or custom gates exist or pass. Record deferred setup and verification explicitly.
