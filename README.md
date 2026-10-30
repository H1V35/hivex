# HiveX

Project knowledge for people and agents. HiveX combines reusable workflow guidance with reliable Markdown, explicit decision relationships and focused source retrieval.

[![Latest release](https://img.shields.io/github/v/release/H1V35/hivex)](https://github.com/H1V35/hivex/releases/latest)

[![Support HiveX on Ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/jivssssss)

- Recover project context without reading the whole repository every time.
- Keep current decisions concise, link their dependencies and exceptions, and retain accessible history.
- Give agents shared guidance for design, documentation, implementation, review and Git.

Your Markdown remains the authority. HiveX helps the responsible agent interpret it; it does not approve changes or replace human judgment.

## Get started

Download the archive for your platform from [GitHub Releases](https://github.com/H1V35/hivex/releases) and put `hivex` (`hivex.exe` on Windows) on your `PATH`. Prebuilt binaries cover macOS (Apple silicon and Intel), Linux x86_64 and Windows x86_64. Alternatively, build a release with Cargo:

```sh
cargo install --git https://github.com/H1V35/hivex --tag v0.14.2 --locked hivex --bin hivex --force
```

Then initialize your project from its root:

```sh
hivex init
```

`init` prepares project documents, agent instructions and the six bundled skills, and installs the [ponytail](https://github.com/DietrichGebert/ponytail) plugin in Claude Code and Codex when they are installed. It preserves existing files and makes no model calls. The skills are copied into `.agents/skills/` and linked from `.claude/skills/`, and `hivex.json` pins the installed release so collaborators use the same version. Extensions add skills and project files for a particular kind of project: `hivex init --extension OWNER/REPO` ([guide](docs/guide.md#extensions)). After installing a newer release, `hivex update` refreshes the skill copies, moves the pin forward and brings ponytail to its latest release; it changes no other project file.

```sh
hivex search "cache policy"
hivex relations docs/policy.md
hivex read docs/policy.md --from 20 --to 45
hivex check --source docs/policy.md
hivex --help
```

Run these commands from the project root. All current CLI commands use Markdown directly without model calls or a persistent knowledge store. The executable needs no Node.js or other runtime. Version 0.8 retires the inferred graph runtime; the [migration guide](docs/guide.md#migrate-from-the-inferred-graph-runtime) explains the changed command contract and preservation of historical data. Version 0.12 replaces the npm package; existing projects follow [Migrate from npm](docs/guide.md#migrate-from-npm).

## Documentation

- [Product brief](docs/PRD.md): purpose, users, intended outcomes and scope.
- [CLI guide](docs/guide.md): configuration, source retrieval, authored navigation and migration.
- [Development](docs/guide.md#development): build, tests and CI with Rust.
- [Project decisions](docs/README.md): architecture and engineering conventions.

## License

[MIT](LICENSE)
