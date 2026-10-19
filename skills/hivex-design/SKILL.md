---
name: hivex-design
description: Resolve product, interface or architectural decisions through focused research, a grill or a useful prototype, and define scope when needed.
---

# Resolve what matters

Recover the request, relevant Markdown and Hivex's local evidence before asking questions. Reuse settled decisions and inspect facts available in the repository or primary sources. A defined change can proceed to implementation without an interview or another planning artifact.

For a grill, ask one consequential question at a time, explain the tradeoff and recommend a course of action. Follow dependencies between decisions and wait for the owner's answer before implementing a design still under discussion.

Use research to settle factual uncertainty and a small prototype to answer a behavior or design question. Make alternatives meaningfully different; expose the relevant states or interactions. Preserve the conclusion and useful evidence. A disposable presentation does not make validated domain logic disposable or establish production readiness.

## Design an interface people can assess

Recover the screen's purpose, audience, primary action, current design authority and relevant visual reference. Separate settled product choices from open composition or interaction questions. An example of another screen can suggest a pattern; it cannot demonstrate fidelity to this one. Do not expand functionality to fill a layout.

For composition and reference comparisons, consult the relevant [visual review criteria](references/visual-review.md). Use the project's applicable platform guide; the [template catalogue](../hivex/templates/README.md) supplies a starting point when one is missing. Native Swift iOS/iPadOS work uses the [Apple-based iOS guidance](../hivex/templates/swift/docs/guidelines/ios.md#visual-design-from-intent-to-rendered-evidence), preserving the adopting project's decisions rather than imposing a template's appearance.

When visual direction is open, use a representative rendered screen or interaction to resolve it before spreading the composition across a flow. With an accepted direction, reuse the existing components and proceed without another design ceremony. Inspect the actual output and relevant states before claiming visual fidelity; code, compilation and passing functional tests alone do not establish it. Keep references and evidence scoped to the decision, not a growing catalogue of every past iteration.

## Shape the domain and the work

Check terms and responsibilities against the project's glossary, code, rules and exceptions. Resolve ambiguity in the owning context instead of inventing synonyms. Prefer interfaces that hide useful complexity; question layers that only forward calls. Add an abstraction for a concrete responsibility, not to impose hexagonal architecture.

Capture the outcome, boundaries and useful acceptance evidence in the existing issue or spec. Split only for independently verifiable results or real dependencies. Cross-layer slices should deliver a meaningful behavior; a broad mechanical refactor may instead need introduction, migration and removal in stages.

Use `hivex-document` for durable decisions and `hivex-git` for authorized tracker work. Neither extra tickets nor a fixed number of alternatives, agents or prototypes is required.
