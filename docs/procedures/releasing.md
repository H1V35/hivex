# Native package release

The npm package is `@h1v35/hivex`; its command is `hivex`. Prepare a release from the reviewed Git revision on the admitted macOS ARM64 runner. The native package declares `os: darwin` and `cpu: arm64`; do not add another target before its runtime behavior and package have been tested.

Keep `Cargo.toml` and `package.json` versions aligned. Run the checks in [development guide](../guide.md#development), then prepare the native archive:

```sh
cargo run --locked --bin hivex-dev -- pack
```

This builds only the locked `hivex` release target, stages the public allowlist with distribution metadata and invokes system `tar` from Rust. `dist/` receives the npm-compatible `.tgz` and a JSON report with its file list, native binary hash and archive SHA-256. No publication occurs and no JavaScript package manager runs. Existing archives are never overwritten; supply an explicit different archive path for a new verification build.

Inspect the exact archive. It must contain the executable, package metadata, public documentation, six skills, language templates and license notices. Cargo dependency license texts are included in `THIRD-PARTY-NOTICES.txt`. Exclude project evidence, SQLite state, credentials, development sources, test fixtures and build caches. Inspect and scan the extracted content for secrets and unintended private material, and record what was checked together with the archive hash. A changed archive requires a fresh inspection.

Run `cargo run --locked --bin hivex-dev -- verify [archive.tgz]` against that exact archive. It checks the hash and public allowlist, rejects unsafe paths and links, extracts to a clean temporary project and exercises the native executable with a PATH that excludes Bun and Node.js. Synthetic Markdown and v1 SQLite fixtures verify executable mode, JSON behavior and preservation of retained answers, graph, caches and budget. Run model scenarios only through the Rust synthetic app-server. CI also runs the CLI contract suite against the release executable. A successful source build alone is insufficient.

Before registry publication, additionally install the exact archive with npm in a clean temporary prefix, with `--ignore-scripts --no-audit --no-fund`, and verify the installed command and metadata. This release-only compatibility check uses the registry client's Node.js installation; source development, tests and artifact preparation do not require it.

Obtain the publication approval applicable to the concrete archive. Earlier version approval does not authorize another artifact. Confirm an npm account authorized for the scope, then publish the same checked tarball:

```sh
npm publish /absolute/path/to/h1v35-hivex-VERSION.tgz --access public --registry https://registry.npmjs.org
```

Record the published version, exact artifact hash and registry result. Do not rebuild or repack between approval and publication. Keep the artifact until publication and any required verification complete. Git integration and completed branch cleanup follow the [tracker procedure](issue-tracker.md).
