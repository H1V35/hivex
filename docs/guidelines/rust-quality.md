---
title: Rust quality and readability
status: accepted
---

# Rust quality and readability

The owner approved this Rust standard in [#115](https://github.com/H1V35/hivex/issues/115). It extends a broad, maintained third-party baseline with project preferences. It applies to runtime code, development utilities and tests. Preserve behavior and readable responsibilities when refactoring; do not split functions, invent interfaces or weaken rules merely to change a metric.

## Maintained baseline

Use stable Rust, rustfmt and Clippy. `rustfmt.toml` selects two spaces, no hard tabs and a 100-column target. Rustfmt may retain long literals and macro input; the width is its formatting target, not a separate byte-length prohibition. Nightly formatting options are unnecessary.

`Cargo.toml` enables Clippy `all` and `pedantic` with lower group priority, so individual decisions remain explicit. CI denies warnings. All pedantic rules retain their defaults, including the 100-line function guideline; there are no lint suppressions or pedantic exclusions introduced for this adoption. The individual restriction lints `dbg_macro`, `todo` and `unimplemented` prevent leftover diagnostics and unfinished code. `redundant_else` is explicitly named to document the early-return preference (it already belongs to pedantic). Do not enable the entire `restriction` or `nursery` group.

Consult the [Clippy lint catalogue](https://rust-lang.github.io/rust-clippy/stable/index.html) and [configuration reference](https://doc.rust-lang.org/clippy/lint_configuration.html) when updating the toolchain. Assess new diagnostics by their meaning before changing project policy. Do not add `allow`/`expect` annotations or raise thresholds just to obtain a passing run.

## Parameters and flow

Functions and methods have at most four parameters, including the receiver (`self`, `&self`, etc.). `too-many-arguments-threshold = 4` configures Clippy. Clippy checks ordinary functions, inherent methods and trait declarations, but exempts trait implementations and non-Rust ABI functions. The syntax gate also checks source function/method signatures, including trait implementations, default methods and closure inputs. Foreign function declarations without a Rust body are external ABI contracts, not implementation functions; they are excluded from this source limit and from complexity measurement. Closure captures are environment, not explicit parameters.

Prefer early returns, `?`, `let-else` and meaningful `match` expressions. No `else if` chains are allowed. `redundant_else` covers an unnecessary else after diverging flow; it does not enforce this full preference. The syntax gate detects chains in expressions and written macro tokens while ignoring comments and string literals. An ordinary `else` remains useful for two-way value selection. Do not replace a clear decision with artificial dispatch abstractions.

Allow at most three nested blocks inside a function or block-bodied closure, excluding its body block. Each function/closure starts its own count; `impl`, traits and inline module containers do not consume this allowance. Explicit inner scope blocks count too. The syntax gate enforces this definition. An expression-bodied closure uses an implicit body boundary, so omitting outer braces cannot grant an extra nesting level.

Clippy's `excessive_nesting` counts the function body and enclosing `impl`, trait and inline module blocks; out-of-line modules reset its count. Its threshold is therefore five total syntactic levels as a supplementary check, not a claim that its number means three body levels. Deep inline containers can make it stricter than the function-local check. This explicit configuration preserves the three-level project rule without moving methods out of their type solely to avoid container counting.

## Complexity and analyzer scope

`cargo test --locked --test quality` enforces a cognitive maximum of 15 and a cyclomatic maximum of 20. Equality passes. It visits Rust files in `src`, `tools` and `tests`, including test code and inactive conditional-compilation branches. It fails on unreadable files, invalid Rust syntax, a Mozilla parse error, absent function metrics, non-finite scores or a threshold violation. Diagnostics identify the file and original source line. At least one implementation function must be measured.

Use Mozilla [rust-code-analysis](https://github.com/mozilla/rust-code-analysis), pinned to the published crate `0.0.25`, rather than another handwritten complexity algorithm. The library is MPL-2.0, published from Mozilla's repository revision `57ab3973b53e7bf392d925cd5e6c6e889b8c22c0`. Its lockfile dependencies include Tree-sitter 0.20.9 and the Rust grammar 0.20.3, other language grammars, numeric, graph, serialization and filesystem utilities. These are development dependencies. The grammar's broad Tree-sitter dependency must resolve to the compatible 0.20.9 API; `--locked` prevents an accidental incompatible resolution. No JavaScript/Bun development runtime or analyzer executable is installed.

The published grammar cannot parse some valid modern Rust at file scope, including `unsafe extern`. The evaluated upstream revision `37e5d83c056c8cbf827223d5814a93c5218df1a9` also failed this syntax and fetched large unrelated test-repository submodules through Cargo. It is not used. Instead, `syn` validates the entire original Rust file and locates original function/method source spans using `proc-macro2`. Mozilla parses those unmodified spans. Standalone initializer closures receive only a `const` item wrapper, without rewriting their expression or control flow. An unsupported function body still fails, so a grammar limitation cannot silently become an approved result. Avoid misleading identifiers or use equivalent standard Rust forms where a parser ambiguity is demonstrated; do not transform control flow solely for the analyzer.

Mozilla reports functions, methods and closures as function spaces. The gate uses each space's local `cognitive()` and `cyclomatic()` score and recurses into child spaces. It does not use file totals, maxima or averages as per-function scores. Nested closures retain Mozilla's lexical nesting contribution; measuring them separately is not a redefinition of that algorithm. Representative tests cover ordinary functions, methods, default trait methods, nested and initializer closures, inclusive thresholds, over-limit functions and malformed input.

These are source metrics under [Mozilla's metric definitions](https://mozilla.github.io/rust-code-analysis/metrics.html), not metrics of expanded or optimized compiler output. Mozilla does not expand macros: functions, closures and control flow inside opaque macro input are not claimed as measured function spaces. Keep substantial executable macro arguments in ordinary Rust functions or local expressions and review generated behavior. The syntax and dependency gates inspect written macro tokens for their specific rules, but do not expand procedural/declarative macros. Cargo check and Clippy remain required for compiler validation. Clippy's experimental `cognitive_complexity` is not an equivalent substitute.

## Domain boundaries

The [domain decision](../adr/0013-domain-modules-and-execution-integrations.md) remains a modular monolith, without hexagonal layers or empty interfaces. Production submodules are private. Each domain root reexports the concrete types and operations its consumers need; callers do not reach into another domain's storage, parser, protocol or other implementation files.

The following direct dependencies are allowed in addition to a domain's own modules and the shared `error`/`compatibility` utilities:

| Consumer | Permitted domains |
| --- | --- |
| documents, foundation | none |
| knowledge | documents, work, execution |
| work | documents, knowledge, execution |
| execution | documents, knowledge, work |
| integrations | execution |
| consultation | documents, knowledge, work, execution, review |
| review | documents, knowledge, work, execution |
| CLI and binary entry point | compose the domains |

Knowledge ingestion, work persistence and execution accounting have concrete collaborations; this matrix does not invent acyclic layering. Execution's runtime validates knowledge output and persists work evidence, while its integration contract stays vendor-neutral. Integration implementations remain isolated by vendor and cannot import knowledge, work or CLI. Choosing an integration, provider, model and its options remains independent.

`cargo test --locked --test architecture` follows production module declarations from `src/main.rs`, checks imports and qualified paths (including relative paths and written macro tokens), and requires private submodules. Root aliases/glob imports and custom production `#[path]` redirection are rejected because they would obscure the dependency boundary. Normal local aliases remain usable: their imports establish the dependency and Rust privacy protects the exposed interface. Explicit test-only modules/functions are excluded from dependency checks; their behavior and source quality are still verified. The gate is a source policy check, not a compiler name-resolution engine or a security sandbox for arbitrary macro expansion.

## Local and CI verification

```sh
cargo fmt --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --test quality --test architecture
cargo test --locked
```

The Quality workflow runs these gates on the existing GitHub-hosted macOS ARM64 runner, then verifies the native package and CLI contracts. Neither gate makes model calls. The analyzer, syntax libraries, development utility and synthetic server are not distributed. Package notices follow only normal/build dependency edges, excluding development-only analyzers. `num-traits`, already present transitively, is an explicit runtime dependency for checked numeric conversions; the v1 numeric behavior remains protected by the existing contracts.

Changes to rules, parser versions or the dependency matrix require updating this authority and representative positive/negative cases. A changed Markdown source does not authorize model calls to refresh derived knowledge; maintain it only within the owner's separately authorized model-work scope.
