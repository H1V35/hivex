# Documentation map

- [Domain language](CONTEXT.md): documents, decisions, relationships, evidence and bounded work.
- [Engineering workflow](engineering.md): development, verification and knowledge maintenance.
- [Practical knowledge assistance](adr/0010-practical-knowledge-assistance.md): the current approved
  contract, staged delivery, autonomy, semantic relationships, uncertainty and cost.
- [Shared knowledge and selective history](adr/0011-shared-knowledge-and-selective-history.md): accepted
  extension for compact ADRs, bounded historical retrieval and a Git-versioned knowledge snapshot;
  implementation is pending.
- [Recommended Markdown convention](../skills/hivex/references/markdown.md): optional organization
  and writing practices for any adopting project.
- [CLI guide](../README.md) and [agent skill](../skills/hivex/SKILL.md): the interface actually available.

Earlier decisions remain in `adr/` as history. ADRs 0004–0009 describe the replaced experimental
cohort/admission workflow; ADR 0010 supersedes its mandatory ceremony. Historical evidence is scoped
to its original revision and is not a current acceptance result. An adopting project retains its own
Markdown at monorepo, package or module level; Hivex does not own that source tree.
