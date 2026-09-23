# Rust standard

Use stable Rust, rustfmt and Clippy for Rust projects. Apply this standard to runtime code, development utilities and tests. It does not apply to code written in other languages.

## Maintained baseline

Configure rustfmt with `tab_spaces = 2`, `hard_tabs = false` and `max_width = 100`. Width is a formatting target: rustfmt can retain longer literals and macro input. Do not require nightly formatting options.

Enable Clippy `all` and `pedantic` with lower group priority than individual lints, and deny warnings in CI. Keep their remaining defaults, including the 100-line function guideline. Explicitly enable `dbg_macro`, `todo`, `unimplemented` and `redundant_else`; do not enable the entire `restriction` or `nursery` group. Consult the [stable lint catalogue](https://rust-lang.github.io/rust-clippy/stable/index.html) and [configuration reference](https://doc.rust-lang.org/clippy/lint_configuration.html) when updating the toolchain.

Refactor nonconforming code rather than adding `allow`/`expect` annotations, excluding source files or raising thresholds to silence findings. Preserve behavior and meaningful responsibilities; do not split functions or introduce abstractions solely to lower a metric.

## Agreed preferences

- Maximum cyclomatic complexity: 20 per function, method or closure.
- Maximum cognitive complexity: 15 per function, method or closure. Equality passes for both limits.
- Maximum nesting: three nested blocks inside a function or block-bodied closure, excluding its body block. Each function or closure starts its own count; explicit inner scopes count, while enclosing modules, traits and `impl` blocks do not. Expression-bodied closures have an implicit body boundary.
- Maximum parameters: four, including the receiver and explicit closure inputs. Captures are not parameters. Foreign declarations without a Rust body are external ABI contracts, not implementation functions.
- No `else if` chains. Prefer early returns, `?`, `let-else` and meaningful `match` expressions. An ordinary `else` remains useful for two-way value selection.

Set Clippy's `too-many-arguments-threshold = 4`. Clippy exempts trait implementations and non-Rust ABI functions; use a source syntax check to cover implementation signatures and closure inputs as well. `redundant_else` does not prohibit all `else if` chains, so check that preference separately, including written macro tokens while ignoring comments and literals.

Use a source syntax check for the function-local nesting definition above. As a supplementary check, enable `excessive_nesting` with `excessive-nesting-threshold = 5`. Clippy also counts the function body and enclosing inline modules, traits and `impl` blocks; out-of-line modules reset its count. Five Clippy levels are not equivalent to three function-local levels, and deeply nested containers can make it stricter. Do not reorganize responsibilities merely to evade counting.

## Complexity and analyzer scope

Use Mozilla [rust-code-analysis](https://github.com/mozilla/rust-code-analysis) for the agreed [metric definitions](https://mozilla.github.io/rust-code-analysis/metrics.html), rather than a handwritten algorithm or Clippy's experimental `cognitive_complexity`. The established baseline is the published development dependency `rust-code-analysis = "=0.0.25"`; preserve its compatible locked parser dependencies and validate any upgrade before adoption. Do not distribute development-only analyzers as runtime dependencies.

Measure each function space's local cognitive and cyclomatic scores and visit nested spaces; file totals, averages or maxima are not per-function scores. Nested closures retain the analyzer's lexical nesting contribution. Include implementation code, tests, utilities and inactive conditional-compilation branches across the project's actual source layout.

The baseline parser cannot handle every modern Rust construct at file scope. Validate the complete original file with `syn` and use original function/method spans from `proc-macro2` for metric analysis. A standalone initializer closure may need a containing item wrapper; do not rewrite its expression or control flow. Fail on unreadable or invalid source, unsupported implementation bodies, missing function metrics, non-finite scores or exceeded limits. Identify the original file and line, and require at least one implementation function to be measured.

Source metrics do not expand declarative or procedural macros. Do not claim that executable bodies inside opaque macro input are measured. Keep substantial executable macro arguments in ordinary Rust functions or local expressions, and review generated behavior. Written-token syntax and dependency checks cover their specific rules without becoming macro expansion or compiler name resolution. Compiler checks remain required.

## Module boundaries

Define permitted dependencies from the project's real modules and responsibilities. Keep implementation submodules private and expose the concrete types and operations consumers need through domain roots. Callers must not reach into another domain's storage, parser, protocol or other implementation modules. Keep production code independent of tests; test-only code may exercise private behavior.

Follow production module declarations and verify imports and qualified paths, including relative paths, macro invocation paths and written macro tokens, against the project's dependency policy. Reject root aliases/glob imports and custom production `#[path]` redirection that obscure this boundary; ordinary local aliases remain usable because their imports establish the dependency and Rust privacy protects the exposed interface. Exclude explicitly test-only modules/functions from dependency checks while retaining their source-quality checks. Record the check's coverage and limitations. Do not copy another project's module names or dependency matrix, impose an acyclic architecture without a domain reason, or create empty layers to satisfy the template.

## Adoption and verification

Record the concrete dependency policy, source coverage, analyzer versions and local/CI verification commands here when adopting the standard. Use rustfmt, compiler checks, Clippy, source-quality and architecture gates, and relevant behavior tests. Run Cargo checks with the committed lockfile. Cover representative passing and failing cases when implementing or changing a custom gate.

Use this verification sequence once the project's source-quality and architecture gates are implemented. `quality` and `architecture` are example test target names; record the actual equivalent targets configured by the adopting project.

```sh
cargo fmt --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --test quality --test architecture
cargo test --locked
```

The gates must be deterministic and make no model calls. Adapt CI and verification of distributed executables to the project's admitted platforms and runners, without assuming a particular operating system or package format. Changes to rules, parser versions or the dependency matrix require updating this guideline and representative positive/negative cases.

Installing and configuring those tools and gates is separate implementation work: copying this guideline does not establish that they exist or pass. Respect an explicit owner decision to defer adoption; do not substitute different lint preferences or arbitrary limits. Link the adopted guideline from the project's documentation map and engineering guide, and keep the documented rules and their checks aligned.
