# Hivex

Project knowledge for people and agents. Hivex combines reusable workflow guidance with reliable Markdown, explicit decision relationships and focused source retrieval.

[![npm version](https://img.shields.io/npm/v/%40h1v35%2Fhivex)](https://www.npmjs.com/package/@h1v35/hivex)

[![Support Hivex on Ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/jivssssss)

- Recover project context without reading the whole repository every time.
- Keep current decisions concise, link their dependencies and exceptions, and retain accessible history.
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
npx hivex relations docs/policy.md
npx hivex read docs/policy.md --from 20 --to 45
npx hivex check --source docs/policy.md
npx hivex --help
```

Run these commands from the project root. In Bun projects, use `bun hivex …` instead.

The native package supports **macOS ARM64**. All current CLI commands use Markdown directly without model calls or a persistent knowledge store. Version 0.8 retires the inferred graph runtime; the [migration guide](docs/guide.md#migrate-from-the-inferred-graph-runtime) explains the changed command contract and preservation of historical data.

## Documentation

- [Product brief](docs/PRD.md): purpose, users, intended outcomes and scope.
- [CLI guide](docs/guide.md): configuration, source retrieval, authored navigation and migration.
- [Development](docs/guide.md#development): build, tests and package verification with Rust.
- [Project decisions](docs/README.md): architecture and engineering conventions.

## License

[MIT](LICENSE)
