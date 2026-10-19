---
title: "Python standard"
status: draft
created_at: 2026-10-19
tags: [python, quality]
---

# Python standard

Use [uv](https://docs.astral.sh/uv/) and [Ruff](https://docs.astral.sh/ruff/) for Python projects. Apply this standard to runtime code, scripts, development utilities and tests written in Python. It does not apply to code written in other languages.

## Environment and dependencies

Let uv own the interpreter, environments and locked dependencies; do not maintain `requirements.txt` files or hand-made virtual environments beside it.

- A package or application declares its dependencies in `pyproject.toml` and commits `uv.lock`. Run its commands with `uv run`.
- A standalone script declares its dependencies in its own header ([PEP 723](https://peps.python.org/pep-0723/)) and runs with `uv run <script>.py`; uv installs what it needs:
  ```python
  # /// script
  # requires-python = ">=3.11"
  # dependencies = ["openpyxl"]
  # ///
  ```

Prefer the standard library or an existing dependency over a new one. Name importable modules in `snake_case`; a script that tests import needs an importable name.

## Maintained baseline

Ruff formats and lints. Configure it once for the project. In `ruff.toml`:

```toml
line-length = 100
target-version = "py311"

[lint]
preview = true
explicit-preview-rules = true
select = [
  "E", "W", "F", "I", "N", "UP", "B", "SIM", "RET", "PTH", "ERA", "RUF",
  "C90", "PLR0911", "PLR0912", "PLR0913", "PLR1702",
]

[lint.mccabe]
max-complexity = 10

[lint.pylint]
max-args = 4
max-nested-blocks = 3
```

In `pyproject.toml`, use the same settings under `[tool.ruff]` and rename every table: `[lint]` becomes `[tool.ruff.lint]`, `[lint.mccabe]` becomes `[tool.ruff.lint.mccabe]` and `[lint.pylint]` becomes `[tool.ruff.lint.pylint]`; TOML table headers are absolute, so unprefixed tables are silently ignored. Set `target-version` to the oldest supported Python. `explicit-preview-rules` enables only the named preview rule (`PLR1702`), not the whole preview set. Consult the [rule reference](https://docs.astral.sh/ruff/rules/) when updating Ruff.

Refactor nonconforming code rather than adding `# noqa`, per-file ignores or raised limits to silence findings. Preserve behavior and meaningful responsibilities; do not split functions or introduce abstractions solely to lower a metric.

## Agreed preferences

- Maximum parameters: four per function or method, including `self` or `cls`, as in the Rust standard. `PLR0913` enforces it for functions, but Ruff does not count the receiver, so it lets a method take four parameters besides `self`; review methods for this limit.
- Maximum nesting: three nested blocks inside a function (`PLR1702`).
- Maximum McCabe complexity: 10 per function (`C90`), with Ruff's default limits on branches and returns (`PLR0912`, `PLR0911`). Python has no Ruff cognitive-complexity rule; keep the Rust standard's intent of readable, shallow flow.
- Prefer early returns and `match` over long `if`/`elif` chains; an ordinary `if`/`else` remains useful for two-way choices.
- Annotate the types of public functions. Use `pathlib.Path` for paths, `decimal.Decimal` for money and explicit exceptions with useful messages; never swallow exceptions with a bare `except`.
- Configuration, paths and credentials arrive as arguments or environment values, never as absolute paths or secrets written in code.

## Tests

Test with [pytest](https://docs.pytest.org/), run through uv: `uv run pytest` in a package, or `uv run --python '>=3.11' --with pytest --with-requirements <script>.py pytest` for a standalone script. `--with-requirements` installs the script's inline dependencies but not its interpreter constraint, so give `--python` the script's `requires-python` value. Each test protects an important behavior, a business rule or a demonstrated regression, using observable interfaces and independent expected results. TDD is optional and reserved for critical flows whose intended behavior is sufficiently defined. Keep fixtures small and free of real personal data.

## Adoption and verification

Record the project's actual configuration location, scope and any approved deviation here when adopting the standard. Use this verification sequence, adapted to the project's layout:

```sh
uvx ruff format --check .
uvx ruff check .
uv run pytest
```

Run the same commands in CI on the project's supported platforms. Installing and configuring Ruff, uv and pytest is separate implementation work: copying this guideline does not establish that the checks exist or pass. Respect an explicit owner decision to defer adoption; do not substitute different limits without updating this guideline.
