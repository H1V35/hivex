---
title: Project foundation and adaptive workflow
status: accepted
date: 2026-09-13
---

# Project foundation and adaptive workflow

Adopting Hivex should establish a useful base for autonomous project work as well as reusable knowledge. Distribute a small documentation foundation and five independent skills for design, documentation, implementation, review and Git/triage, alongside the knowledge-support skill. This extends the distribution scope of [ADR 0010](0010-practical-knowledge-assistance.md); the CLI does not become a development-session orchestrator or the principal reviewer.

`init` prepares missing project Markdown, source configuration and Git visibility rules with no model calls. It preserves existing files and local knowledge. The principal agent completes purpose, vision and domain language from actual evidence and owner decisions, and guides migration to the standard where reasonably possible. Preserve useful content, links, history and monorepo/package/module authority. Existing layouts remain valid inputs to knowledge operations.

The owner retained project-local package installation when simplifying onboarding. `init` also prepares a `CLAUDE.md` import of `AGENTS.md` and relative links for the six packaged skills in `.agents/skills` and `.claude/skills`. Existing skill entries remain owned by the project and are preserved, including custom or stale links; the agent inspects those before replacing them. Parent symlinks and incompatible path types fail validation before any writes. The complete package supplies the skill assets; moving its executable alone does not install them. The presentation README stays short and the packaged [CLI guide](../guide.md) owns detailed usage.

Use mature, reusable engineering and documentation practices, leaving a consumer's product, stack, host and historical authorization details in that consumer. Prefer DDD and meaningful module responsibilities without imposing hexagonal architecture or speculative abstractions. Work uses the capabilities it needs; neither every planning stage nor additional tickets are compulsory when the change is already defined.

Choose tests by value and risk, including critical flows, stable rules and regressions. TDD is optional and reserved for critical flows whose behavior is sufficiently defined. Use one independent review by default, and another when concrete risk or findings warrant it; the reviewer uses the implementation agent's model and effort. The knowledge model remains a separate role. Start knowledge assistance with local retrieval and source reading, calling the model when it adds value. Preserve budget, history and uncertainty rather than rerunning work to obtain approval.

Use readiness, participation, dependency, type, scope and risk labels for their distinct purposes. Labels neither select models nor grant permission. Work labels such as `epic`, `research`, `prototype` and `decision` should survive replacement of a skill; avoid aliases that repeat existing meaning.

Keep AGENTS.md as a concise entrypoint, with orientation, development/verification guidance and relevant project-specific constraints. Its links identify when an authority matters; they do not force every source into every task. The [open AGENTS.md convention](https://agents.md/) requires no fields beyond ordinary Markdown, so the foundation supplies a recommended shape rather than a proprietary schema or line-count limit.

Skills use focused descriptions and conditional references, preserving the user's scope and existing authorization. Retire superseded general skills only after their useful capabilities and callers are covered; specialized skills remain separate. Communicate in clear language, briefly explaining unfamiliar technical terms when they matter to understanding or deciding.

## Amendment: compact skills and critical-flow TDD

The owner clarified this foundation on 2026-09-13: replace the former external general skill set completely after covering active callers, including its generic personal, writing and provider-specific utilities. Preserve only non-obvious guidance that improves the adopted workflow; keep unrelated technical specialties separate. Handoff needs no dedicated skill or reference.

Apply the same economy to entrypoints and supporting references. Do not recreate the old catalogue as a reference library, prescribe an arbitrary line cap, or include tutorials for capabilities the model already provides. Existing guidance can be sufficient without porting another resource. TDD remains optional even for critical flows and is reserved for sufficiently defined critical behavior. Other checks and tests require concrete value, not compliance with a testing ritual.

## Amendment: skill and execution responsibilities

On 2026-09-16, the owner approved separating reusable task guidance from execution mechanics. Skills provide domain knowledge and criteria for their capability. The host agent harness coordinates agents, applies their model/effort configuration and manages execution permissions, processes and session limits. Skills do not implement another orchestration or permission layer.

Project Markdown retains workflow policy, including independent review and matching the responsible implementer's model and effort. The harness must support that policy; removing duplicated skill instructions does not configure it automatically. Hivex's own versioned evidence, persisted work budgets, recovery and uncertainty remain CLI contracts. Its skill retains the conditions needed to use those capabilities correctly and routes advanced mechanics to the existing CLI manual.

## Knowledge before merge (2026-09-18)

The owner extended the foundation with a mandatory knowledge-before-merge policy for adopting projects. Maintain valuable Markdown and its derived graph before integration, resolve active warnings with evidence and leave no pending current-source ingestion/checks or stale/unavailable current dependencies. Keep changes in the owning authority and export the graph with its Markdown; later source edits require renewed freshness checks.

A native `partial` result does not alone establish a current defect: historical references and native quality marks may remain after review. Record the independent reviewer's disposition against current sources without deleting history, promoting native quality or repeating checks for a green label. Actual contradictions, missing decisions needed by supported behavior and uncertain execution still block. An explicitly deferred capability may be disposed of while its authority prohibits implementation or activation until its prerequisites are met; roadmap inclusion alone does not make it currently supported behavior. Being unrelated to one PR is insufficient.

The canonical reusable policy is in the [engineering foundation](../../skills/hivex/assets/project/docs/guidelines/engineering.md#knowledge-before-merge), linked from its tracker procedure. Adoption and Git skills route to that authority instead of implementing a harness or CI orchestrator. `init` supplies it to new projects and preserves existing files; the adopting agent must incorporate it into an existing project's appropriate authority, preserving explicit owner decisions. Installing an updated package does not retroactively rewrite project policy. This gate supplements implementation review, behavioral verification and the project's merge-authorization policy.
