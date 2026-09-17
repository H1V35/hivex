---
title: Engineering workflow
status: accepted
---

# Engineering workflow

Hivex is a Rust CLI, migrated under [#80](https://github.com/H1V35/hivex/issues/80) and [ADR 0010](../adr/0010-practical-knowledge-assistance.md#rust-migration-80). Fixed v1 fixtures and public CLI tests preserve the previous runtime contracts. Modules group behavior by domain responsibility and hide internal details behind small interfaces. Do not add a second development-session orchestrator or require an adopting project's layout, tracker or product packages. Codex, Git/GitHub and CI coordinate work.

Skills provide task knowledge and judgment. The host harness handles agent coordination, effective model/effort configuration, execution permissions and process/session lifecycle. Project Markdown owns workflow policy; Hivex's CLI owns its knowledge state and work accounting. Keep these responsibilities distinct rather than duplicating harness mechanics in each skill, as recorded in [ADR 0012](../adr/0012-project-foundation-and-workflow.md#amendment-skill-and-execution-responsibilities).

## Development and verification

Work is issue-first in `H1V35/hivex`. Use design, documentation, implementation, review and Git as capabilities selected for the task. Resolve open decisions through discovery or a grill, and create additional specs or execution tickets when they help define or divide work. A defined change can proceed directly to implementation and review. Reuse settled scope instead of reopening an interview. The owning repository carries the execution ticket; a cross-repository parent supplies context and coordination, not a substitute for native tracking.

Resolve the existing spec/ticket before changing code and link the PR and verification to it. Absorb review findings into the appropriate existing ticket whenever its scope permits. Open a separate issue only when strictly necessary to preserve independently actionable work, and record why it cannot be absorbed. Read-only retrieval does not need a new ticket. Specs and tickets track work and acceptance; resulting durable decisions also enter their repository authority.

Use an existing issue for an already tracked requirement. Create a branch from the current remote main, keep each PR to one coherent change and preserve commit history when merging. Never push directly to main or force-push a shared branch. Apply review findings before acceptance; an invalid review can be rerun, while an adverse finding must be resolved on its merits. The owner reserves the final merge decision for each PR. Agents prepare reviewed, passing PRs and stop before integration unless the owner explicitly approves that specific PR; a general implementation GO is not continuing merge authorization.

Use one independent review by default, covering scope, correctness and standards; add another when concrete risk or findings justify it. Independent code reviewers use the implementing agent's current model and reasoning effort. Pass that profile explicitly when the subagent default differs, and verify the effective configuration after dispatch. Do not substitute the cheaper knowledge model for code review. Routine implementation subtasks may use an explicitly authorized cheaper model; Hivex's internal knowledge extraction/checking uses the user's knowledge-model configuration independently of the development and code-review model.

Choose verification for the affected surfaces. Code changes require typechecking, lint, formatting and relevant behavior tests; documentation-only changes need formatting and checks of affected references or declared sources. Record the exact revision and the checks actually completed. A later change invalidates the affected results. Do not claim an omitted, interrupted or truncated check passed.

GitHub Actions runs Quality on an ephemeral GitHub-hosted `macos-15` ARM64 runner. The owner approved retiring the personal Mac runner in preparation for public contributions; the workflow checks the hosted environment and architecture before running the native gates. PRs receive read-only repository permissions and no stored credentials or publishing secrets. Do not reconnect a personal workstation or introduce a `pull_request_target` checkout of untrusted code. A queued, skipped or interrupted run is not a pass. Local verification remains required when CI is unavailable. See the [CI procedure](../procedures/self-hosted-runner.md) for the execution boundary and checks.

Use Cargo with the committed lockfile for the runtime, development tools and tests. The stable toolchain in `rust-toolchain.toml`, two-space rustfmt, Clippy all + pedantic with warnings denied, source-quality and domain-dependency gates, and behavioral checks form the development gate. The [Rust quality standard](rust-quality.md) defines their scope, thresholds and analyzer limits. TypeScript, JavaScript and Bun are retired from Hivex development under the owner's clarification of [#80](https://github.com/H1V35/hivex/issues/80). The earlier [Bun installation decision](../adr/0003-independent-bun-installation.md) is historical context.

Follow [ADR 0013](../adr/0013-domain-modules-and-execution-integrations.md) for the modular monolith, execution profile independence and typed work transitions. Keep Rust modules cohesive and private by default. Document discovery and Markdown parsing do not import knowledge, review or model execution; knowledge storage and ingestion may use documents; review may use documents and knowledge. Execution contracts remain independent of vendors; their runtime composes knowledge validation and work accounting. Work may use documents, knowledge and the execution contract; consultation composes them and review. Integrations use the execution contract and shared errors, and CLI commands compose responsibilities and own argument parsing. The exact permitted dependency matrix is recorded in the [Rust quality standard](rust-quality.md#domain-boundaries). Unit tests may use private production modules; production cannot import tests. CLI integration tests use the compiled executable, fixed v1 SQL fixtures and a synthetic Rust app-server. The development package utility and synthetic server are separate binaries and are never distributed.

The owner-approved [Rust quality standard](rust-quality.md), adopted under [#115](https://github.com/H1V35/hivex/issues/115), replaces the earlier instruction against adopting Rust thresholds. It uses maintained Rust tools and explicit project preferences, without claiming equivalence to ESLint or to the TypeScript template. The optional TypeScript documentation template remains applicable to adopting TypeScript projects; it does not configure Hivex development. Repository Markdown remains human-maintained authority. Check changed links, commands and scope without retaining a JavaScript formatter solely for documentation.

Package preparation and verification use the Rust development utility and system `tar`; npm remains the distribution channel. Registry installation and publication are separate release checks, not prerequisites for building or testing from source.

## Tests protect behavior

Choose tests for value and risk, prioritizing critical flows, stable rules and demonstrated regressions. TDD is optional and reserved for critical flows whose behavior is sufficiently defined; explore unresolved behavior before committing its assumptions to tests. Do not require a test for every function, component, wrapper or line. A test must identify a supported behavior, meaningful invariant or regression it protects. Prefer the caller's observable interface and results that survive an internal refactor.

For UI, test visible content, accessibility, interactions and loading/error/empty-state behavior. Do not freeze arbitrary child arrays, wrapper counts or class/style arrangements. A visual dimension needs a test only when it is an intentional requirement worth maintaining. For example, displaying "2 of 4" is a behavior; representing it as exactly three React children is not.

Mocks, call counts, ordering and exact bytes are not automatically wrong. They can protect an external protocol, idempotency, a query budget or faithful source reproduction. Their justification must be the contract, not the current arrangement of internal helpers. Expected results must be independent examples, not the implementation's own calculation repeated in the test.

Review existing tests as retain, rewrite, consolidate or remove. Remove tests for retired behavior with that behavior; preserve still-needed guarantees at the replacement's actual interface. Do not port a legacy battery mechanically, chase a test-count target or retain duplicate suites indefinitely.

## Files and runtime data have a lifecycle

Create a source file for a meaningful responsibility and a document for a distinct authoritative purpose. Do not create files for every helper, task, turn, attempt or handoff merely to satisfy a layout convention or a lint threshold.

Before introducing persistent state, define its purpose, location, owner and retention. Prefer a small project-local data store to an unbounded tree of per-event files. A per-unit atomic checkpoint can be a database transaction; it does not require a separate file. Fewer filenames alone do not bound data growth: cached data, run history and diagnostics also need size/count/age limits and cleanup behavior.

Normal read-only queries should leave no per-query artifacts. Clean up owned temporary resources on ordinary completion and handled failures. Interrupted work must remain recoverable without being silently retried or discarded. Export diagnostic bundles when needed rather than automatically writing a new report for every successful step. Retention must preserve the accepted state and the evidence needed by supported historical/recovery operations; it must not invent a successful cleanup.

## Documentation is maintained authority

Code must be self-explanatory through clear names, structure and behavior. Repository Markdown is the source of truth for intent, constraints, decisions and reasons that code cannot explain. Do not write a parallel implementation manual or use documentation to compensate for unclear code. Accepted source history and Markdown retain authority; caches, model output and search hits do not. An accepted status alone does not settle amendments, exceptions or contradictions. Keep unresolved evidence explicit. Never promote a historical agent's description of an owner ruling without checking its provenance and applicability.

Capture every decision worth preserving in its appropriate repository document as part of the work. Do not leave accepted knowledge only in a conversation, issue comment or runtime log. Update the existing canonical document when it already owns the topic and scope; create a new one only when it has a distinct purpose. An issue can track the work and preserve discussion, but it is not a substitute for incorporating the resulting doctrine into the documentation.

Keep docs with the monorepo, workspace or module they describe. Link to common rules instead of copying them. Recommended new Markdown should state purpose/scope, use stable headings and suitable metadata, keep a rule with its conditions/exceptions, and link its sources and replacements. Accept compatible existing Markdown without forcing those authors to adopt our template. Do not generate empty documentation for every module or add non-Markdown readers to the current scope.

Use repository decisions and review evidence for durable knowledge, not private agent memory. Checkpoints identify the exact commit, verified work and remaining work. Choose a context handoff when the task needs it; Hivex does not impose the retired machinery's fixed token thresholds. Knowledge-model operations use the admitted Luna/max profile and record actual usage, including failed or interrupted attempts. Deterministic retrieval and maintenance do not require a model.

The replacement workflow follows [ADR 0010](../adr/0010-practical-knowledge-assistance.md). It processes bounded document batches with one additional knowledge check, keeps partial knowledge usable and preserves a work budget across phases and resumption. A consultation maintains one pending batch before answering, and a source-based repair replaces interpretations without changing doctrine. Keep check warnings scoped and public evidence limited to source coordinates, version and text. Avoid a new abstraction or protocol unless it protects a concrete requirement. The owner-authorized implementation can replace the earlier cohort/admission pipeline; its historical evidence remains unchanged.

Start knowledge support with local search, neighbors and source reading, reusing current context. Use model-assisted interpretation or review when it adds useful information rather than as a mandatory step for every change. Hivex assists the principal reviewer with decisions, dependencies, exceptions and possible conflicts. The reviewer verifies its findings. Missing context or uncertainty limits the conclusions it affects; a definitive finding must refer to the actual document and implementation versions reviewed.

## Retire mechanisms without losing knowledge

After knowledge recovery and replacement validation, remove obsolete orchestration code, scripts, hooks, configuration, tests, dependencies and active instructions. Do not carry an unused legacy framework into Hivex under another name. Preserve useful decisions in their canonical docs and retain necessary historical evidence in Git or a bounded private archive outside the active worktree. Do not rewrite Git history or destroy the accepted Opus graph before its replacement is admitted.

A replacement workflow can be used before every historical artifact is retired when the remaining work is explicit. Complete closure requires useful historical knowledge in Markdown and obsolete active machinery retired wherever it is no longer needed. Validate the workflow against bounded real cases, not identity with an old model's graph or an exhaustive replay prerequisite. Release packages exclude private project evidence and retired runtime.

## Distribution

The owner selected `@h1v35/hivex` for the npm package on 2026-09-08, retaining `hivex` as the installed command, and approved the MIT license. The scoped name avoids the unrelated existing unscoped npm package. Use an authenticated account authorized for that scope; do not infer npm ownership from a matching GitHub name. Release preparation must verify the packed contents and exclude private project evidence and runtime stores. Project integration follows the normal release process after the complete cycle is validated; this decision alone does not mean a package has been published.

The first native package is restricted to macOS ARM64, the target validated by the project runner. Keep the scoped package and installed command unchanged. Do not ship another target on compilation evidence alone. Package preparation follows the [release procedure](../procedures/releasing.md).

Before publication, inspect and scan the exact package archive for secrets and unintended private content. Record its hash and the completed scan result; a repack requires a fresh check. Publish the same verified artifact, not an unchecked reconstruction from a changed working tree.

## Shared project foundation

The [foundation decision](../adr/0012-project-foundation-and-workflow.md) adds a recommended documentation and workflow baseline to Hivex distribution. The CLI prepares missing files without model calls; the principal agent completes meaningful project content and migrates existing documentation when appropriate. Preserve monorepo, package and module authority and the adopting project's actual decisions.

Keep AGENTS.md as a brief, consistent entrypoint with relevant pointers and indispensable project details. Use clear language, explaining a technical term when it matters to understanding or a decision, without explaining every term or changing the owner's requested level of detail.
