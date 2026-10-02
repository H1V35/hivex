---
title: "Swift standard"
status: draft
created_at: 2026-09-28
tags: [swift, quality]
updated_at: 2026-10-02
---

# Swift standard

Apply this guide to maintained Swift code in libraries, tools and apps, including tests and development utilities. It does not impose an Apple UI framework or a screen architecture on a CLI or server. The project's existing engineering authority owns DDD, domain responsibilities and the Hivex workflow; link that authority when adopting this guide instead of creating another architecture policy.

This is an initial, evidence-backed baseline. Copying it does not configure a compiler, formatter or linter, or establish that an application meets its quality goals. Preserve accepted project decisions and record genuine conflicts, unsupported checks and deferred work explicitly.

## Toolchain and source scope

Record the supported Swift compiler/language mode, platform SDKs, deployment targets, build settings and pinned tool versions. Use Swift 6 language mode for new code, with compiler diagnostics treated as errors in CI. Existing migrations need an explicit plan; do not quietly retain a weaker language mode or turn off checking. [Swift's migration guide](https://www.swift.org/migration/documentation/swift-6-concurrency-migration-guide/enabledataracesafety/) distinguishes Swift 6 data-race checking from incremental checking in Swift 5 mode.

Cover authored production code, tests and utilities. Keep generated clients, macro expansion output, vendored sources and build outputs out of authored-style metrics; compile and test their integration and fix the generator or contract rather than editing generated output. Review authored macro invocations and generated behavior at trust boundaries. List the source roots, exclusions and build configurations explicitly: neither a single compiler configuration nor a linter proves all conditional-compilation paths or platform variants.

## Formatting, language and API design

Use the toolchain's official `swift-format` through `swift format`. Keep its default rules and select two-space indentation and a 100-column formatting target. This agrees with the formatter's defaults; it is not a byte-length ban on unbreakable literals. The [configuration reference](https://github.com/swiftlang/swift-format/blob/main/Documentation/Configuration.md) defines the supported keys. A minimal `.swift-format` is:

```json
{
  "version": 1,
  "lineLength": 100,
  "indentation": { "spaces": 2 }
}
```

Keep one owner for formatting; do not run another formatter or enable contradictory SwiftLint spacing/layout rules. Use the [Swift API Design Guidelines](https://www.swift.org/documentation/api-design-guidelines/) for clear names, argument labels and idiomatic APIs. Prefer value semantics where appropriate, explicit ownership and typed errors. Avoid forced casts, forced tries and forced unwraps of fallible data; represent absence and failure honestly. Keep public/package interfaces small and implementation details private or internal. Introduce protocols, wrappers or abstractions for actual behavior or variation, not one per class or test double.

## Module boundaries

On adoption, record the project's concrete permitted dependencies/imports and map the owning responsibilities to its actual Swift modules and build targets where appropriate. Folders organize files; they do not create access boundaries inside a module. Use separate targets when a real responsibility needs that boundary, not one target per feature by default. Do not copy another project's dependency matrix or impose universally acyclic domain layering.

Swift's [access-control rules](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/accesscontrol/) have different scopes: `private` restricts a declaration to its enclosing declaration and same-file extensions of that declaration; `fileprivate` to its file; `internal` (the default) to its entire module; `package` to modules in the same package, not every module in a workspace; and `public` to importing clients. `open` additionally permits external subclassing/overriding of classes and members. Keep implementation details at the narrowest useful scope and expose only intended capabilities. An `internal` implementation in another feature folder is still reachable within that module.

Record which edges SwiftPM/Xcode manifests declare, which APIs the compiler protects, and how the architectural policy is verified. Build configuration and access control do not reject a semantically forbidden dependency that the build still makes available; the five configured SwiftLint rules do not inspect the project's dependency policy. Review changes to target dependencies, imports and cross-feature implementation access against the permitted edges. Where responsibilities share one module, explicitly record the boundary that requires review. Add a project-specific automated check only for a demonstrated need, with documented scope and representative allowed/forbidden cases; do not claim a check exists merely because the policy is written.

Keep production targets/code independent of test targets, fixtures and test-only helpers. Tests may exercise internal APIs with `@testable import` when the module is built for testing; this does not make those dependencies or access paths production APIs. Record the actual architecture verification commands or required review in the project's engineering guide and update them with the module policy.

## Readability limits and their coverage

The inherited limits remain cyclomatic complexity 20, cognitive complexity 15, control nesting 3 and at most 4 parameters. A tooling gap is not an exception or permission to relax them. Apply them with the definitions below; Swift tool scores are not interchangeable with the Rust or TypeScript scores. Equality passes. Do not split responsibilities, hide parameters in meaningless bags, raise thresholds or suppress findings merely to pass a number.

| Criterion | Swift scope and counting | Enforcement in this baseline |
| --- | --- | --- |
| Parameters: 4 | Count explicit input positions in owned functions, methods, initializers, subscripts and closures, including defaulted/optional parameters and inferred closure inputs. Argument labels do not add inputs; implicit `self`, captures and generic type parameters do not count. A variadic parameter is one input position. Getters/deinitializers have no inputs; setter/property-observer value inputs are language contracts. | SwiftLint checks function declarations, including owned protocol requirements; it skips overrides and does not cover initializers, subscripts or closures. Review those gaps, including synthesized memberwise initializer APIs. |
| Control nesting: 3 | Count nested `if`/`else`, `switch` case bodies, loops and `do`/`catch` scopes within each executable body. Exclude the body's outer braces, enclosing types/extensions and a `guard` condition; its failure body is one control level. Functions, initializers, accessors, observers, deinitializers and closures start their own body count. A case does not add a second level on top of its switch. | Review required. SwiftLint's `nesting` counts nested type/function declarations, not these control levels; do not configure it to 3 and report this rule checked. |
| Cyclomatic: 20 | The automated score is SwiftLint's `cyclomatic_complexity`: it starts at zero, counts `if`, `guard`, loops, catches and switch cases, and discounts fallthrough cases. Nested functions/initializers are separate; branches in nested closures contribute to the enclosing checked body. It is not a normalized McCabe or Mozilla score and does not count every Boolean/ternary decision. | Automatic for function/initializer bodies only. Standalone closures, accessors and deinitializers need explicit review; a clean result does not certify their numerical complexity. Review the uncovered scopes and metric differences; the general limit remains required, and a SwiftLint pass alone is not proof of full cyclomatic compliance. |
| Cognitive: 15 | Required limit for executable function, initializer, accessor, observer, deinitializer and closure bodies. The exact Swift scoring of guards, nested closures and result builders must be established with a compatible maintained analyzer before claiming a numeric result. | Not measured by the selected tools. Manual review is required; assess branching and comprehension cost and record any unverified quantitative scope. Do not relabel cyclomatic scores, invent a regex metric or claim full 20/15 compliance. |

Prefer early exits, `guard`, optional binding and meaningful `switch` expressions/statements. Do not write `else if` ladders. A result builder cannot generally use imperative early returns: use a meaningful `switch` or two-way `if`/`else` for declarative selection, or move genuine calculation into an ordinary function. This is not permission to introduce empty view wrappers or move business logic into a view. If a concrete framework contract requires a policy exception, explain it and obtain explicit approval rather than treating builder syntax as a blanket exemption.

Declarative container nesting is not control-flow nesting: `VStack`, `Group` or `ForEach` composition is not a stack of imperative blocks. Actual `if`/`switch`/loop constructs within a builder still count, and action/task closures have executable bodies of their own. Review large trees for meaningful subviews and observation boundaries instead of extracting a component every three braces. Do not claim metrics of compiler-expanded builder or macro code.

An externally imposed protocol/override/FFI signature is a real contract. Preserve it, document its owner and the narrow signature exception, and keep its authored body subject to normal checks. Owned protocols are not an escape hatch. Any unavoidable rule suppression must be localized to that contract with a reason and independent review; no blanket disables or severity reduction. Broad exceptions for tests, UIKit or entire generated-looking directories are not acceptable.

## Complementary lint configuration

Use SwiftLint for the measured metrics and unsafe forced operations, not as another formatter. The following focused configuration was checked with **SwiftLint 0.65.1**; `only_rules` deliberately does not claim the full SwiftLint default ruleset. Keep formatter defaults and compiler checking alongside it. Pin the admitted release and recheck coverage when upgrading.

```yaml
only_rules:
  - cyclomatic_complexity
  - function_parameter_count
  - force_cast
  - force_try
  - force_unwrapping
cyclomatic_complexity:
  warning: 20
  error: 20
  ignores_case_statements: false
function_parameter_count:
  warning: 4
  error: 4
  ignores_default_parameters: false
```

The pinned implementations of [cyclomatic complexity](https://github.com/realm/SwiftLint/blob/0.65.1/Source/SwiftLintBuiltInRules/Rules/Metrics/CyclomaticComplexityRule.swift), [parameter count](https://github.com/realm/SwiftLint/blob/0.65.1/Source/SwiftLintBuiltInRules/Rules/Metrics/FunctionParameterCountRule.swift) and [nesting](https://github.com/realm/SwiftLint/blob/0.65.1/Source/SwiftLintBuiltInRules/Rules/Metrics/NestingRule.swift) define the actual coverage. This baseline adds no custom analyzer. Evaluate an additional maintained analyzer only for a demonstrated need; document its algorithm, supported syntax and positive/negative cases before making it a gate. An unsupported construct or omitted scope is a gap, not a passing score. Resolve a relevant unverified limit before claiming full compliance; any change to the required limit or exception needs explicit owner approval.

## Concurrency and boundaries

Use structured concurrency where work has a bounded parent lifetime. Give unstructured tasks and asynchronous streams an explicit owner, cancellation path and cleanup. Cancellation is cooperative; it does not prove that a remote effect was cancelled. Recheck the identity/version of work before accepting a delayed result. Keep blocking I/O, decoding, encryption and heavy CPU work away from UI execution when the module has a UI consumer.

Choose actor isolation deliberately per module; do not assume `async`, an actor declaration or `Task {}` automatically moves costly work off the main actor. Record default isolation and upcoming-feature settings. [SE-0461](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0461-async-function-isolation.md) changes nonisolated async behavior under its feature setting and introduces `@concurrent` in Swift 6.2; use syntax only when the admitted toolchain supports it. Avoid routine `Task.detached`, `@unchecked Sendable`, `nonisolated(unsafe)` or relaxed flags as fixes for compiler findings. A necessary interoperability escape requires a narrow rationale and evidence for its safety.

Keep transport models separate from domain and persistence responsibilities. Where an OpenAPI contract is authoritative, regenerate the client instead of maintaining a parallel HTTP schema. Preserve omitted/null/value distinctions where meaningful, stable identifiers, typed errors, byte bounds and compatibility with supported peers. At FFI boundaries, document ownership, borrowed lifetimes, callback executor, cancellation and error conversion; do not introduce a second durable writer or reimplement parsers/cryptography to hide an integration problem.

## Adoption and verification

Link the adopted guide from the project's documentation map and engineering guide, with the exact source/target scope, permitted module dependencies and checks or required review actually enabled. `Sources` and `Tests` below are SwiftPM layout examples, not required directories. Pass only the project's authored roots/files to format and lint; configure exclusions consistently when generated sources live nearby.

```sh
swift --version
swift format lint --strict --recursive Sources Tests
swiftlint lint --strict --config .swiftlint.yml Sources Tests
swift build -Xswiftc -warnings-as-errors
swift test -Xswiftc -warnings-as-errors
```

For an Xcode app use its actual schemes, SDKs and test destinations instead of pretending `swift build` covers the app. Do not ignore unparsable files, warnings or failed targets. Include supported build variants and compiler checking of generated integrations. Applying this document alone is neither installing tools nor verifying their configuration.

Tool evidence, **2026-09-28**: SwiftLint 0.65.1 and Xcode 27.0 (27A266a), Apple Swift 6.4, Swift 6 language mode. The bundled formatter reports `main` as its version; pin the Xcode/toolchain build rather than treating that string as a reproducible release. Small isolated cases verified 20/21 branch and 4/5 parameter boundaries, defaulted parameters, forced operations, the initializer/closure/accessor gaps and the difference between declaration nesting and control/layout depth. A SwiftUI/Observation sample type-checked against the simulator SDK with an iOS 17 deployment target; valid actor access compiled and invalid cross-actor access failed. These are tool checks, not an app, UI, concurrency-race or performance validation, and do not prescribe that deployment target for adopters. Other toolchain/platform combinations and cognitive enforcement remain to validate.
