---
title: Project foundation and adaptive workflow
status: accepted
date: 2026-09-13
---

# Project foundation and adaptive workflow

Adopting Hivex should establish a useful base for autonomous project work as well as reusable knowledge. Distribute a small documentation foundation and five independent skills for design, documentation, implementation, review and Git/triage, alongside the knowledge-support skill. This extends the distribution scope of [ADR 0010](0010-practical-knowledge-assistance.md); the CLI does not become a development-session orchestrator or the principal reviewer.

`init` prepares missing project Markdown, source configuration and Git visibility rules with no model calls. It preserves existing files and local knowledge. The principal agent completes purpose, vision and domain language from actual evidence and owner decisions, and guides migration to the standard where reasonably possible. Preserve useful content, links, history and monorepo/package/module authority. Existing layouts remain valid inputs to knowledge operations.

Use mature, reusable engineering and documentation practices, leaving a consumer's product, stack, host and historical authorization details in that consumer. Prefer DDD and meaningful module responsibilities without imposing hexagonal architecture or speculative abstractions. Work uses the capabilities it needs; neither every planning stage nor additional tickets are compulsory when the change is already defined.

Choose tests by value and risk, including critical flows, stable rules and regressions. TDD is optional when behavior is defined. Use one independent review by default, and another when concrete risk or findings warrant it; the reviewer uses the implementation agent's model and effort. The knowledge model remains a separate role. Start knowledge assistance with local retrieval and source reading, calling the model when it adds value. Preserve budget, history and uncertainty rather than rerunning work to obtain approval.

Use readiness, participation, dependency, type, scope and risk labels for their distinct purposes. Labels neither select models nor grant permission. Work labels such as `epic`, `research`, `prototype` and `decision` should survive replacement of a skill; avoid aliases that repeat existing meaning.

Keep AGENTS.md as a concise entrypoint, with orientation, development/verification guidance and relevant project-specific constraints. Its links identify when an authority matters; they do not force every source into every task. The [open AGENTS.md convention](https://agents.md/) requires no fields beyond ordinary Markdown, so the foundation supplies a recommended shape rather than a proprietary schema or line-count limit.

Skills use focused descriptions and conditional references, preserving the user's scope and existing authorization. Retire superseded general skills only after their useful capabilities and callers are covered; specialized skills remain separate. Communicate in clear language, briefly explaining unfamiliar technical terms when they matter to understanding or deciding.
