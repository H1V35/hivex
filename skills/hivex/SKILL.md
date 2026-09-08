---
name: hivex
description: Use Hivex to retrieve versioned project decisions, maintain repository Markdown, and run or inspect the knowledge-review workflow. Apply when Hivex is requested or a repository with hivex.json needs documentary context, graph review, or knowledge maintenance.
---

# Hivex

Use the project's installed CLI and its declared sources. Hivex is a knowledge tool; the harness
continues to own implementation, Git and code review.

## Establish the available interface

Read the repository's agent instructions, documentation map and `hivex.json`. Run `hivex --help` once
for the installed version when its interface is unfamiliar. In a project installation, use
`bun hivex`; Bun resolves the locally installed binary. The package is `@h1v35/hivex`. Do not fetch the unrelated unscoped `hivex`
package. In Hivex's own checkout, `bun hivex` runs its development entry point.
Use `bunx --no-install hivex` when a harness explicitly needs binary-only resolution.

Use the installed version's help and package README for uncommon flags. Do not invent commands or
report an unsupported stage as completed. Existing project authorization still applies; this skill
does not require another confirmation for work already authorized.

## Retrieve only the context the task needs

Start with a specific question and a small `search` result set. Open the relevant `read` results,
including the complete rule, conditions and exceptions. A preview or a `status: accepted` declaration
alone does not establish current applicability. Follow replacements and amendments before resolving
an apparent contradiction.

When an admitted graph is available, check its freshness, search its claims, open the selected claims
and expand their neighbors. Return to the cited Markdown to settle meaning. Use IDs and continuation
cursors returned by the CLI. Keep the same snapshot while continuing a read; do not combine evidence
from different revisions silently. Increase a byte budget or follow the cursor when needed instead
of treating truncated or missing evidence as a complete answer.

| Need                                   | CLI operation                                                  |
| -------------------------------------- | -------------------------------------------------------------- |
| Find documentary context               | `search`, optionally scoped with `--collection`                |
| Read complete authored evidence        | `read`, with `--cursor` for continuation                       |
| Inspect an existing declared ADR index | `relations`; this is a compatibility projection, not admission |
| Measure the declared rebuild inputs    | `plan`                                                         |
| Inspect one model extraction           | `extract`; it produces a candidate                             |
| Produce or resume a candidate cohort   | `ingest`; inspect retained work with `--show`                  |
| Correct evidenced extraction omissions | `ingest --revise <source> --input <graph> --feedback <review>` |
| Assemble/check a candidate snapshot    | `graph build`, `graph check`                                   |
| Review extraction fidelity             | `graph review`, or `graph review --all` for the cohort         |
| Reuse unchanged source reviews         | `graph review --all --from <old-graph> --reuse <old-export>`   |
| Select and assess cross-source context | `graph compare-plan`, then `graph compare` or `--all`          |
| Reuse unchanged pair assessments       | `graph compare --all --from <old-graph> --reuse <old-export>`  |
| Admit complete reviewed evidence       | `graph admit`, when supported by the installed version         |
| Ground a specific review claim         | `ground <claim>`; reuse current evidence with `ground --check` |
| Query a graph's bounded evidence       | `graph search`, `graph read`, `graph neighbors`                |

## Preserve the project's work history

Follow the project's spec/ticket process before implementation. In projects with issue tracking,
identify the agreed spec and execution ticket in the owning repository; a cross-repository parent is
coordination, not a replacement. Respect its declared blockers. Reuse an appropriate ticket for findings. Open another only when
strictly necessary, with the reason recorded. Link changes and verification to that work item, and
capture resulting decisions in the repository authority. Ordinary read-only retrieval creates no ticket.

## Maintain Markdown where it belongs

Keep documents with their project, workspace or module. Select those locations through `hivex.json`;
do not move them into a Hivex-owned source tree. Compatible existing Markdown does not need wholesale
reformatting.

For new decision documents, prefer concise frontmatter (`title`, truthful `status`, decision `date`),
a clear purpose/scope, stable headings, the rule with its conditions/exceptions, and the reason and
sources that settled it. Use relative Markdown links to related documents and precise headings for
amendments. State which part is replaced and what remains applicable; a partial change is not a
whole-document revocation.

Update the existing authoritative document when it owns the topic. Link shared rules instead of
copying them. Keep code self-explanatory through clear names, structure and behavior. Repository Markdown is the
source of truth for intent, constraints, decisions and reasons that code cannot explain. Do not write
a parallel manual describing the implementation or compensate for unclear code with documentation. Preserve useful decisions in the repository, not private
agent memory. Historical agent assertions require provenance before becoming current doctrine.

## Rebuild and review deliberately

Use deterministic retrieval for ordinary implementation questions; a code-only change does not
require model ingestion. A knowledge rebuild follows the available CLI stages: plan, ingest, build,
source fidelity, comparison selection, comparison when needed, admission. One claim-bearing source
needs no invented pair. Use the configured knowledge profile.
`--prepare` on a source review or pair comparison exposes the request without a model call.

Choose a bounded neighbor count appropriate to the corpus; four is a starting point, not a guarantee.
Keep the same selection settings when resuming, inspecting and admitting that cohort. Reuse retained
results. Investigate failed or interrupted work before an explicit retry; do not discard uncertain
work to make the workflow appear green. Export needed evidence before an authorized retirement.

Use `ingest --revise` for a retained candidate with evidenced omissions or distortions and sufficient
fidelity context. Inspect its exact request with `--prepare`; preserve the original review and usage.
This operation changes one candidate, never the source's authority or the review's verdict. A passing,
uncertain or insufficient-context review cannot justify revision. A replacement still needs fidelity
review and admission; unchanged output is a failure, not a reason to repeat the same adverse review.

When a graph changes, preserve its old graph and complete review export before using `--from` and
`--reuse` with the new `--input`. Start with `--max-units 0` to transfer evidence without model calls.
Only identical complete source-review inputs qualify; inspect `association` for the current binding
while the original graph/prompt/verdict/usage remain intact. Changed sources need new review, retained
negative results remain blockers and unresolved invocations prevent cohort replacement.

For comparisons, preserve the old graph and complete comparison export, then use the same `--from`
and `--reuse` transition with `graph compare --all --max-units 0`. Choose the new `--neighbors` setting
and keep it for execution, inspection and admission; the old export retains its original settings.
Only pairs selected in the new graph with identical complete comparison inputs qualify. Inspect the
association's current and original selection identities, keep the old export's full provenance, and
execute only pending pairs. Reuse preserves adverse results and never retries uncertain invocations.

For a failed source review or comparison, inspect its complete report before an explicit
`graph review|compare --all --retry-failed <source-or-pair-id> --attempts 2 --max-units 1` on the
same input/store and selection. This permits only safely ended invocation failures, never a completed
adverse assessment or uncertain start/interruption/cleanup. The attempt limit includes earlier work,
with at most three attempts total. `previousAttempts` preserves the full earlier receipts and unknown
consumption, including across compatible graph transitions. Retain complete cohort exports before
retirement; admitted graphs carry current successful assessments, not the earlier failure history.

If a later comparison questions a candidate claim that earlier fidelity missed, preserve the original
results and use `graph review <source-id> --input <original-graph> --feedback <comparison-result-or-show>
--prepare`. The comparison must be safely completed and adverse, with a concrete unresolved claim
from this source. Use its original receipt/graph, not a reassociated copy. Preparation validates the
full evidence and exposes the new bounded request without a model call. An executed reassessment may
uphold the candidate. Only a completed adverse fidelity result satisfying the usual sufficient-context
and evidence rules can feed `ingest --revise`; the comparison itself cannot. Keep both old and new
receipts and their usage. Ordinary and feedback model requests use short local IDs with exact
per-source schema counts; the stored review preserves the graph bindings and model-output hash.
Inspect `rejectedOutput` after validation failure before deciding on another explicit attempt;
that text is untrusted diagnostics, not an assessed verdict. This standalone operation does not replace
working review rows or admit a graph.

Save full exports as artifacts rather than dumping them into model context. Retain and version the
admitted snapshot according to the project workflow. Never hand-edit a projection to invent acceptance.
Admission covers its declared selection; it does not prove exhaustive consistency or approve code.

Before accepting an implementation, check its exact code/diff against applicable decisions and the
exact admitted manifest, explicitly applying amendments, conditions and exceptions. Use `ground <review-claim> --input <admitted-graph> --base <revision>` when available.
Use `--prepare` to inspect the complete request without invoking a model and repeat `--source` to add
needed documentary context. Use `--context-file <revision>:<path>` when an approved prototype or
unchanged dependency is needed; select exact Git evidence rather than copying code into Markdown.
Prefer immutable commits and keep the complete request within its budget. Context code does not
replace evidence of the changed implementation. The checkout must contain the clean, committed implementation.
A supported or contradicted claim is not approval of the entire change. Preserve unresolved evidence
and invocation usage; use `ground --check <result> --input <admitted-graph>` to revalidate saved evidence
without another model call. Changed inputs require a new corresponding assessment. If that capability or sufficient evidence is absent, report the
check as incomplete; tests or graph admission alone do not constitute implementation grounding.
