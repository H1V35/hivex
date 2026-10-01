---
title: Project foundation and adaptive workflow
status: accepted
created_at: 2026-09-13
updated_at: 2026-10-01
---

# Project foundation and adaptive workflow

Hivex distributes a small project documentation foundation, five independent workflow skills for design, documentation, implementation, review and Git/triage, plus the Hivex knowledge-support skill. The foundation extends [ADR 0010](0010-practical-knowledge-assistance.md); it does not turn the CLI into a development-session orchestrator or replace the principal reviewer.

## Initialize without taking ownership

`init` prepares only missing project Markdown, source configuration and Git visibility rules, without model calls. It preserves existing files and local knowledge. The responsible agent completes purpose, vision and domain language from project evidence and owner decisions, and may migrate useful material toward the recommended structure. Existing monorepo, package and module documentation authorities remain valid. Templates are optional starting points, not a required repository layout; retain useful content, links and history.

Project-local skill installation remains supported. `init` prepares a `CLAUDE.md` import of `AGENTS.md` and relative links for the six packaged skills in `.agents/skills` and `.claude/skills`. Existing skill entries remain project-owned, including custom or stale links; inspect them before changing them. Parent symlinks and incompatible path types fail validation before writes. The complete package supplies skill assets; moving its executable alone does not install them. Keep the presentation README short and use the packaged [CLI guide](../guide.md) for command details.

## Keep capabilities focused

Use the capabilities a task needs. Design, documentation, implementation, review and Git/triage skills remain independent; planning stages and extra tickets are not compulsory when the work is already defined. Triage labels describe readiness, participation, dependency, type, scope and risk for their distinct purposes. Labels neither select models nor grant permission. Preserve useful work labels such as `epic`, `research`, `prototype` and `decision` when skills change; avoid aliases that duplicate existing meaning.

Skills provide task knowledge and criteria. The host harness coordinates agents, applies model and effort configuration, and manages execution permissions, processes and session limits. Project Markdown owns workflow policy; Hivex's CLI owns source retrieval, versions, explicit coverage and authored navigation. Keep those responsibilities distinct instead of duplicating harness mechanics in skills. The CLI invokes no knowledge model.

## Engineering and independent review

Use mature engineering and documentation practices. Prefer domain-driven design and meaningful module responsibilities without imposing hexagonal architecture or speculative abstractions. Keep product, stack and host details in the adopting project. Work uses the checks that add value; no planning stage, extra ticket or testing ritual is required by itself.

Choose tests by value and risk, including critical flows, stable rules and regressions. TDD is optional and reserved for sufficiently defined critical behavior. Use one independent review by default; add another only when concrete risk or findings justify it. The reviewer uses the implementation agent's model and effort. The principal reviewer verifies conflicts with relevant decisions, dependencies and exceptions. CLI retrieval supports the responsible agent and is separate from independent implementation review.

Keep `AGENTS.md` a concise entrypoint with orientation, development and verification guidance, and indispensable project constraints. Links explain when an authority applies; they do not require every source on every task. Use the shared creation/update/archive date convention. Other metadata remains purpose-specific; the foundation imposes no proprietary knowledge schema or line-count limit. Skills use focused descriptions and conditional references, preserving the user's scope and existing authorization.

## Source-led integration

[ADR 0014](0014-reliable-markdown-and-explicit-relationships.md) governs knowledge before integration. Start from the documentation map and relevant Markdown authorities, then follow explicit dependency, exception and replacement links as the task requires. Review the affected sources together with the changed implementation. Resolve demonstrated contradictions, missing decisions needed for supported behavior, broken references and actionable defects. A model finding is a claim to check against the sources and code. Bound review to a defined source and implementation revision; close it when concrete findings are resolved; report a blocking owner decision explicitly, and handle new evidence as a bounded update rather than repeated whole-corpus certification.

The inferred graph runtime is retired, and its historical status is not an integration gate. Preserve graph snapshots and local work history, including failed results, quality marks, uncertainty and consumption. Do not reset accounting, erase evidence or relabel a historical/native result as success to clear a warning. A stale graph or unrelated warning alone does not block integration; a real current defect or unresolved decision affecting supported behavior does.

The former graph-completeness and zero-warning gate is retained in the [historical foundation decision](../archive/adr/0012-project-foundation-and-workflow.md#knowledge-before-merge-2026-09-18). ADR 0014 supersedes that gate while preserving project-specific owner decisions and existing merge authorization.

## Relationships

- Extends [Product contract](0010-practical-knowledge-assistance.md): adds the project foundation and independent capabilities without making every phase mandatory.
- Depends on [Reliable Markdown](0014-reliable-markdown-and-explicit-relationships.md): source-led integration preserves explicit authorities, revisions and applicable defects.
