---
title: Source workflow validation and local measurements
status: historical
created_at: 2026-10-01
archived_at: 2026-10-01
updated_at: 2026-10-01
---

# Source workflow validation and local measurements

This dated record supports the source-first transition in [ADR 0014](../../adr/0014-reliable-markdown-and-explicit-relationships.md). It records observed cases and local performance, not a new rule or a certificate of semantic completeness.

## Scope and method

The current Rust source workflow exposes decisions through authored Markdown and fixed relationship types without invoking a model. Public CLI cases cover a retention/privacy conflict, an exception limited to public incident records, an indirect prerequisite, a partial replacement with live privacy conditions, explicit archive access, an accepted future choice whose implementation has not started, missing lexical evidence and a later source change.

Run `cargo test --locked --test cli contracts::source_workflow -- --nocapture` for the curated evidence case. Its independent expectations check the specific sources, direction, historical flags, current versions and visible conditions. These are evidence-exposure checks; no implementing agent was evaluated for natural-language interpretation. A missing lexical match is not proof that the repository contains no relevant decision.

A local debug observation performed nine CLI operations, returned 6819 JSON context bytes and took 90 ms, with zero Hivex model calls. That tiny fixture is smaller than its aggregate JSON responses: direct reading can be cheaper when the authority is already known. The shared method therefore uses capabilities adaptively, reuses current context and visited ranges, and does not require every command for every task.

## Real repository measurements

Native release executable on macOS ARM64, warm local filesystem, five sequential runs per operation. The corpus contained 47 ordinary sources: 185572 bytes after the small discovery-guide amendment. The preceding corpus had 185406 bytes; response byte counts were unchanged. The local Rust `target/` tree contained 114279 generated files and occupied about 8 GiB.

The prior measurements used source snapshot `0d5bc3bf50694450244303c0fb1fee9cbdb6ec987d521ca5bf7592899c0e4cc0` and native executable SHA-256 `5cca67d89d34758038638fab31d09e6ee436e923eb16a3733ed0a1d8dd6ec14c`, retained with the local measurement archive. Both measurements identify the working-copy source versions rather than claiming a published release.

Before optimization, source discovery traversed `target/` despite explicit project documentation selection. Adding that conventional generated directory to default exclusions avoids the walk. A named include/archive pattern can still select intended documentation there; existing exclusions inside that selected path continue to apply.

| Operation | Prior median ms | Optimized median ms | Response UTF-8 bytes |
|---|---:|---:|---:|
| structural check | 290.06 | 11.84 | 409 |
| global lexical lookup | 295.85 | 10.74 | 6089 |
| known state authority | 284.58 | 8.41 | 3587 |
| direct relationships | 287.35 | 8.55 | 4077 |

The optimized measurement used source snapshot `5630989b42f425b57e651ad11c3b8d67eb90354ece36471cedaf2fa239fec2b0` before this record was added, and native executable SHA-256 `f08e2d034e721004b8817875084938da53238d0244ad9d1ef76b7efc35bd7fc4`. The measurements precede this archived record and its map/delivery links; those documentation additions change later snapshots and may alter later query responses. Command arguments were:

```sh
hivex check --max-bytes 65536
hivex search authority --limit 3 --max-bytes 65536
hivex read skills/hivex/assets/project/docs/adr/README.md --max-bytes 65536
hivex relations docs/adr/0014-reliable-markdown-and-explicit-relationships.md --direction outgoing --max-bytes 65536
```

The observed lexical response exposes three source passages in 6089 bytes, rather than loading the entire ordinary corpus into the agent's context. It does not measure the additional evidence a complete implementation task might need, model tokenization, cold machines, other repository sizes or an end-to-end quality comparison with prior graph maintenance. All reported operations made zero Hivex model calls.

## Reliability boundaries

Structural checks validate selected local Markdown targets/anchors and formal relationship grammar. Authors/reviewers still assess applicability, undeclared dependencies, authority uniqueness, contradictions and permission. Coverage warnings, bounds and continuation remain explicit. Current defects are repaired alongside code; historical graph failures and consumption remain preserved, without reopening checks to obtain a green label.

Regressions also protect Unicode/CRLF source ranges, stale continuations, JSON byte bounds, explicit history, URL query versus exact fragment data, protocol-relative external links, encoded Markdown paths, bounded reads/indexing of 16 MiB blank sources, repeated target references, per-document and query-wide anchor bounds, and preservation of legacy file bytes.
