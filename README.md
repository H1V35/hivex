# Hivex

Project knowledge for people and agents. Hivex turns your Markdown into searchable decisions, dependencies and exceptions, with citations back to the original source.

- Recover project context without reading the whole repository every time.
- Keep knowledge current as documents change, with resumable work and explicit budgets.
- Give agents shared guidance for design, documentation, implementation, review and Git.

Your Markdown remains the authority. Hivex helps the responsible agent interpret it; it does not approve changes or replace human judgment.

## Get started

Install in your project, then initialize it:

```sh
npm install -D -E @h1v35/hivex
npx hivex init
```

`init` prepares project documents, agent instructions and the six bundled skills. It preserves existing files and makes no model calls. The dependency stays local and pinned to your project.

```sh
npx hivex search "cache policy"
npx hivex ask "Which rules apply to this change?"
npx hivex update
npx hivex --help
```

Run these commands from the project root. In Bun projects, use `bun hivex …` instead.

The native package currently supports **macOS ARM64**. Model-assisted commands require an authenticated Codex session; the default is Luna/max, with explicit model selection available. Local search, source reading and initialization need no model.

## Documentation

- [CLI guide](docs/guide.md): configuration, commands, budgets, recovery and snapshots.
- [Development](docs/guide.md#development): build, tests and package verification with Rust.
- [Project decisions](docs/README.md): architecture and engineering conventions.

## License

[MIT](LICENSE)
