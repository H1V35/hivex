---
title: Engineering workflow
status: accepted
---

# Engineering workflow

Hivex is a TypeScript/Bun product. Modules group behavior by domain responsibility and hide internal
details behind small interfaces. Do not add a second development-session orchestrator or require
an adopting project's layout, tracker or product packages. Codex, Git/GitHub and CI coordinate work.

## Development and verification

Use an existing issue for an already tracked requirement. Create a branch from the current remote
main, keep each PR to one coherent change and preserve commit history when merging. Never push
directly to main or force-push a shared branch. Apply review findings before acceptance; an invalid
review can be rerun, while an adverse finding must be resolved on its merits. Current explicit owner
authorization governs whether the agent may merge.

Choose verification for the affected surfaces. Code changes require typechecking, lint, formatting
and relevant behavior tests; documentation-only changes need formatting and checks of affected
references or declared sources. Record the exact revision and the checks actually completed. A later
change invalidates the affected results. Do not claim an omitted, interrupted or truncated check passed.

GitHub Actions runs the quality workflow on the owner's Mac through an official self-hosted runner,
using `[self-hosted, macOS, ARM64, hivex]`. GitHub retains secrets, logs and PR checks; no hosted
runner fallback is configured. No speed benchmark is required. A queued, skipped or interrupted
run is not a pass. Local verification remains required when the runner is unavailable.

Runner installation is repository administration, outside Hivex's product. Register the admitted
macOS ARM64 release from GitHub's runner settings in its own directory, verify the official checksum,
and use the generated `svc.sh install/start/status/stop` commands. Keep automatic updates enabled,
a stable Homebrew/system PATH and the Mac awake and connected under the logged-in user. The
runner work directory must be separate from the developer checkout and other repository runners.
Only trusted code may run on this persistent host; review that boundary before public contributions.
See [GitHub's runner reference](https://docs.github.com/en/actions/reference/runners/self-hosted-runners).

During a host migration, disable Actions, register and confirm the runner is online, and merge all
workflow routes before re-enabling Actions. Then dispatch Quality on that exact revision and check
the assigned runner and completed result. The route change alone is not functional verification.

Use `bun install` for development and `bun ci` for frozen installation. Bun owns dependency
installation through its native configuration and lockfile; Hivex has no custom installer or
registry verifier. See the [installation decision](adr/0003-independent-bun-installation.md).

The lint configuration owns executable syntax/complexity constraints: cyclomatic complexity 20,
cognitive complexity 15, at most four parameters, nesting depth three and no nested/chained
ternaries. Refactor around meaningful responsibilities rather than adding tiny wrappers merely to
make a number pass. Changes to those limits require a documented decision.

## Tests protect behavior

TDD guides development through meaningful failing examples; it does not require a test for every
function, component, wrapper or line. A test must identify a supported behavior, meaningful invariant
or regression it protects. Prefer the caller's observable interface and results that survive an
internal refactor.

For UI, test visible content, accessibility, interactions and loading/error/empty-state behavior.
Do not freeze arbitrary child arrays, wrapper counts or class/style arrangements. A visual dimension
needs a test only when it is an intentional requirement worth maintaining. For example, displaying
"2 of 4" is a behavior; representing it as exactly three React children is not.

Mocks, call counts, ordering and exact bytes are not automatically wrong. They can protect an
external protocol, idempotency, a query budget or faithful source reproduction. Their justification
must be the contract, not the current arrangement of internal helpers. Expected results must be
independent examples, not the implementation's own calculation repeated in the test.

Review existing tests as retain, rewrite, consolidate or remove. Remove tests for retired behavior
with that behavior; preserve still-needed guarantees at the replacement's actual interface. Do not
port a legacy battery mechanically, chase a test-count target or retain duplicate suites indefinitely.

## Files and runtime data have a lifecycle

Create a source file for a meaningful responsibility and a document for a distinct authoritative
purpose. Do not create files for every helper, task, turn, attempt or handoff merely to satisfy a
layout convention or a lint threshold.

Before introducing persistent state, define its purpose, location, owner and retention. Prefer a small
project-local data store to an unbounded tree of per-event files. A per-unit atomic checkpoint can be
a database transaction; it does not require a separate file. Fewer filenames alone do not bound data
growth: cached data, run history and diagnostics also need size/count/age limits and cleanup behavior.

Normal read-only queries should leave no per-query artifacts. Clean up owned temporary resources
on ordinary completion and handled failures. Interrupted work must remain recoverable without being
silently retried or discarded. Export diagnostic bundles when needed rather than automatically
writing a new report for every successful step. Retention must preserve the accepted state and the
evidence needed by supported historical/recovery operations; it must not invent a successful cleanup.

## Documentation is maintained authority

Documentation records intent, constraints, decisions and reasons that code cannot explain.
Accepted source history and Markdown retain authority; caches, model output and search hits do not.
An accepted status alone does not settle amendments, exceptions or contradictions. Keep unresolved
evidence explicit. Never promote a historical agent's description of an owner ruling without
checking its provenance and applicability.

Capture every decision worth preserving in its appropriate repository document as part of the work.
Do not leave accepted knowledge only in a conversation, issue comment or runtime log. Update the
existing canonical document when it already owns the topic and scope; create a new one only when
it has a distinct purpose. An issue can track the work and preserve discussion, but it is not a
substitute for incorporating the resulting doctrine into the documentation.

Keep docs with the monorepo, workspace or module they describe. Link to common rules instead of
copying them. Recommended new Markdown should state purpose/scope, use stable headings and suitable
metadata, keep a rule with its conditions/exceptions, and link its sources and replacements. Accept
compatible existing Markdown without forcing those authors to adopt our template. Do not generate
empty documentation for every module or add non-Markdown readers to the current scope.

Use repository decisions and review evidence for durable knowledge, not private agent memory.
Checkpoints identify the exact commit, verified work and remaining work. Choose a context handoff
when the task needs it; Hivex does not impose the retired machinery's fixed token thresholds.
Knowledge-model operations use the admitted Luna/max profile and record actual usage, including
failed or interrupted attempts. Deterministic retrieval and maintenance do not require a model.

Grounding complements tests and code review. A claim that an implementation satisfies documented
decisions requires evidence tied to its exact code/diff and the accepted knowledge manifest.
Missing graph admission, unresolved contradictions or insufficient evidence remain unresolved;
neither silence nor an unrelated earlier PASS establishes acceptance.

## Retire mechanisms without losing knowledge

Complete the Hivex cycle before resuming Compi feature development. Necessary Compi adoption and
verification work belongs to that completion. Alternative model integrations are evolutionary work;
keep the current Codex/Luna route usable without spreading its assumptions into the knowledge model.

After knowledge recovery, replacement validation and consumer migration, remove obsolete orchestration
code, scripts, hooks, configuration, tests, dependencies and active instructions. Do not carry an unused
legacy framework into Hivex under another name. Preserve useful decisions in their canonical docs and
retain necessary historical evidence in Git or a bounded private archive outside the active worktree.
Do not rewrite Git history or destroy the accepted Opus graph before its replacement is admitted.

Unfinished ingestion persistence does not yet satisfy every policy above. Its completion is required
work, not a guarantee already delivered by this document. Release packages must exclude private project evidence and retired runtime material.
