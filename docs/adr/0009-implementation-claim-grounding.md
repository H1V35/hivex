---
title: Ground review claims against exact implementation evidence
status: accepted
date: 2026-09-08
---

# Ground review claims against exact implementation evidence

Ground a specific review claim against the committed implementation and a fresh admitted knowledge
manifest. The harness still owns discovering review findings, testing and accepting the change.
Hivex distinguishes whether the supplied claim is supported, contradicted or unresolved; a contradicted
allegation may correctly dismiss a finding. Neither definitive outcome approves the whole implementation.
Every result keeps `accepted: false`, `implementationAccepted: false` and its claim scope explicit.

Require a clean checkout, current `HEAD` and an explicit base revision. Compare `HEAD` with the merge
base of that revision, recording both resolved commits and a digest of the complete changed-file
manifest. Supply complete before/after versions, object IDs, paths and modes rather than unbound
snippets. Reject empty changesets, binary/invalid UTF-8 content, symlink/submodule changes and requests
that cannot fit their declared bounds. A checkout changed during execution cannot produce current
review evidence. Historical code inspection is not current implementation acceptance.

In model requests, each code version uses `lines: [[number, text], ...]` with one-based original
line numbers. Each line retains its exact text, including empty lines and CRLF carriage returns;
joining the texts with LF reconstructs the original blob. This replaces the prompt's unnumbered
text without duplicating it. It changes presentation and the prompt hash, not code, manifests or
citation validation. The model must still return the correct original range and literal code.

Select up to eight lexical claim matches and optional explicit source IDs. Expand the complete
source neighborhood along supersedes, exception-to and requires relationships in both directions.
Supply full selected Markdown with original line positions, declarations, extracted claims and
local/cross-source relationships. Disclose other unexpanded relationships and the non-exhaustive
selection. Lexical matches do not prove semantic completeness; the assessment must report insufficient
context when an omitted source or unchanged dependency is needed. A caller can then supply additional source IDs, explicit versioned code context or revise the review scope. No implicit model retry, automatic context truncation or global
consistency claim is allowed.

The caller may supply up to 16 contextual code files as explicit Git revision/path pairs, such as an
approved prototype or an unchanged dependency. Read complete regular UTF-8 blobs from those commits;
never follow filesystem symlinks or infer extra files. Retain the requested reference, resolved commit,
path, mode and blob ID. Prefer a commit ID for immutable reference evidence; moving a named reference
invalidates corresponding evidence during execution or revalidation. Identical request arguments are coalesced. Different references resolving to the same file revision
are rejected so one reference cannot silently hide the other reference's lifecycle. Context files use separate `e` identifiers and the `context` citation revision, while
changed implementation uses `f` identifiers with `before`/`after`. Reference code cannot substitute for
citing the changed implementation when resolving the claim or precedence.

The model assesses every supplied changed/context file and source exactly once, including relevance and
coverage. Every supersession or exception needs an explicit disposition and affected scope, with
literal evidence from both endpoint sources and from the implementation when resolved. Conditions,
negations and exceptions remain applicable; dates, IDs and accepted labels alone do not establish
precedence. Partial scope cannot become whole-claim revocation. Validate original citation ranges and
text, all coverage identities and all supplied precedence relations before interpreting the verdict.
Each precedence assessment must cite every distinct endpoint source, including inapplicable relations.
A definitive verdict needs cited relevant changed code and documentation, complete coverage, sufficient
context and no unresolved applicability. These structural checks support semantic assessment; they
do not prove that the model interpreted a valid quote correctly.

The shared documentary citation check tries the original literal match first. Its conservative
Markdown fallback may join soft line wraps and continuation indentation only within one parsed
paragraph, including a single list item and the original section ranges. It preserves inline spacing
and allows partial joining or changed continuation indentation on either side. Source and quote must
each respect paragraph boundaries; a converted quote wrap must map to source text, never code.
Markdown tokens, code, HTML, tables, frontmatter and hard breaks remain strict. Code citations
always use literal matching. This comparison never rewrites quotes, source bytes, hashes or receipts,
and never promotes a retained failed result. Prompts and schemas remain unchanged.

Use the admitted native Luna/max route. Retain positive, negative and unresolved assessments with
actual invocation evidence, usage and unmeasured failures. A rejected model response is retained as
hashed, untrusted diagnostic text; it cannot become an assessment or be implicitly retried. The result binds the admitted graph hash,
code snapshot, source/relationship aliases, selection, prompt/schema/policy hashes, assessment hash and invocation hash. The invocation hash binds the full
original receipt, including usage and transport identities, before schema normalization during a check.
It is one stdout artifact owned by the caller, with no per-finding files or persistent grounding store.
Preserve needed evidence in the project's review workflow. A deterministic check reconstructs the
request against the current checkout and revalidates those bindings, citations, coverage, invocation
and verdict without calling a model. Changed code, documentation, graph or processing contract requires
a new corresponding assessment. The reader reconstructs the prior unnumbered requests with and
without contextual code files, plus the earlier no-context prompt preceding the clarification
requiring citations from both precedence endpoints. These known variants retain the same validator
and response schema. Their original prompt hashes and receipts remain intact; arbitrary historical
contracts are not admitted. A checksum is an integrity binding, not proof of authorship.

Bound each operation to 32 complete changed text files plus 16 contextual code files, 16 complete
sources, 512 claims, 256 selected
relationships and a 256 KiB request. Existing Git blob limits still apply. A retained result must be
a regular file within 8 MiB. Exceeding a bound fails explicitly instead of returning partial success.

This interface is the first implementation-grounding slice of [Hivex #19](https://github.com/H1V35/hivex/issues/19).
The real Luna evaluations, applicable historical gold and Compi's `610/r1/f03` composition regression
remain acceptance requirements; simulated protocol tests alone do not satisfy them. Retained historical
evidence must distinguish missing inputs from semantic disagreement, never turn an unreplayable case
into a pass or a newly invented owner ruling.
