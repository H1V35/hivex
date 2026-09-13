---
name: hivex-document
description: Establish or maintain project purpose, domain language, decisions, guidelines and procedures. Use for Hivex adoption, documentation migration, PRDs, glossaries, ADRs, or recording durable project knowledge.
---

# Maintain the project's knowledge

Start with the project's documentation map, relevant sources and current owner decisions. Markdown owns intent and reasons; the graph helps recover them. Update the document that already owns the topic and scope. Create another only for a distinct purpose.

## Adopt the foundation

Use the installed `hivex init` when available to prepare missing documents and configuration. The CLI scaffolds files; the principal agent completes their meaning. Read its report before continuing. Do not invent a project's vision, requirements or domain terms to fill a template.

Prefer migration to Hivex's standard structure when reasonably possible, preserving useful content, history, authority and links. Keep knowledge at its monorepo, package or module scope. Inspect existing instructions and source selection before moving files; when graph sources move, use the supported `hivex snapshot relocate` operation and normal incremental maintenance.

Complete the PRD and glossary from existing evidence and the owner. Ask only for essential open decisions. Keep unresolved sections explicitly provisional, and finish meaningful content before spending calls to index empty scaffolding.

Use the foundation [document templates](../hivex/assets/project/docs/README.md) as starting material. Apply the project's own product, stack and operating details; do not import another repository's identities, host paths or historical authorizations.

## Give each document a purpose

- PRD: why the project exists, who it serves, its vision, intended outcomes and scope.
- CONTEXT: agreed domain terms with concise definitions; resolve ambiguity without adding a generic programming glossary.
- ADR: a decision and its rationale, scope, alternatives or exceptions when they matter to future work.
- Guideline: maintained rules for ongoing work.
- Procedure: enough context, steps and verification to perform a particular operation. Add prerequisites or recovery details when they are relevant.

Keep rules with their conditions and exceptions. An accepted status or a historical quotation does not settle a later replacement. When a decision changes, make the change and history traceable; move replaced detail to an archive when that helps keep the current authority usable.

## Keep AGENTS.md lean

Follow the open AGENTS.md convention with a consistent small entrypoint: orientation and knowledge pointers, development/verification guidance, and project-specific constraints only when needed. The [foundation entrypoint](../hivex/assets/project/AGENTS.md) provides the starting shape.

Do not duplicate the PRD, glossary, guidelines or skills. A pointer should say when its target matters, not require every document on every task. Add local instructions only for real scope-specific needs. Use no arbitrary line-count gate.

After documentation changes, check affected links and source selection. Use Hivex's installed maintenance and snapshot capabilities when knowledge changes, preserving existing work and accounting. Check the relevant sources before treating a graph warning as a new product decision.
