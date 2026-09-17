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

## GitHub release and linked package

After the owner approves integration and the exact archive is published on npm, create a new `vVERSION` tag on the reviewed release revision and publish its GitHub release. Never move an existing release tag. Link the notes to the exact npm version and record the archive hash.

The `Publish GitHub Package` workflow mirrors that stable npm version into GitHub Packages so it appears in the repository's Packages section. It runs only for owner-triggered published releases or an explicit dispatch for an existing release. The workflow uses a repository-scoped `GITHUB_TOKEN`, checks package identity and SHA-512, and publishes the same archive without extracting executable code, rebuilding it or running installation scripts. An existing mirror must have identical integrity; a mismatched or unauthorized registry response fails instead of being silently replaced.

Node/npm is used only as the registry client in this publication job. Rust still owns development, package preparation and verification. The pinned npm client honors the explicit GitHub registry flag over the archive's default npmjs publish configuration; the primary archive is not rewritten. npmjs.org remains the normal installation source for consuming projects.

On the first GitHub Packages publication, verify its repository association and visibility. GitHub packages start private even when linked to a public repository; set this package public in its settings before claiming that visitors can see it. GitHub's npm registry still requires authentication to download public packages. npmjs.org installation remains available without that requirement.
