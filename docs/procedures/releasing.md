# Native package release

The npm package is `@h1v35/hivex`; its command is `hivex`. Prepare a release from the reviewed Git revision on the admitted macOS ARM64 runner. The native package declares `os: darwin` and `cpu: arm64`; do not add another target before its runtime behavior and package have been tested.

Keep `Cargo.toml` and `package.json` versions aligned. Run the checks in [README](../../README.md#development), then prepare the native archive:

```sh
bun run pack:native
```

This builds the locked release target, stages only the declared public files and invokes npm's packer without installation hooks. `dist/` receives the `.tgz` and a JSON report with its file list, native binary hash and archive SHA-256. No publication occurs. npm uses a temporary private cache, so package preparation does not change a user's global npm cache.

Inspect the exact archive. It must contain the executable, package metadata, public documentation, six skills, language templates and license notices. Cargo dependency license texts are included in `THIRD-PARTY-NOTICES.txt`. Exclude project evidence, SQLite state, credentials, development sources, test fixtures and build caches. Inspect and scan the extracted content for secrets and unintended private material, and record what was checked together with the archive hash. A changed archive requires a fresh inspection.

Install that tarball in a clean temporary project with scripts disabled. Run the installed `hivex` executable with a PATH that excludes Bun and Node.js, using synthetic Markdown and v1 SQLite/snapshot fixtures. Check executable mode, JSON/exit behavior and preservation of retained data. Run model scenarios only through the synthetic app-server. The quality workflow also verifies this installed artifact; a successful source build alone is insufficient.

Obtain the publication approval applicable to the concrete archive. Earlier version approval does not authorize another artifact. Confirm an npm account authorized for the scope, then publish the same checked tarball:

```sh
npm publish /absolute/path/to/h1v35-hivex-VERSION.tgz --access public --registry https://registry.npmjs.org
```

Record the published version, exact artifact hash and registry result. Do not rebuild or repack between approval and publication. Keep the artifact until publication and any required verification complete. Git integration and completed branch cleanup follow the [tracker procedure](issue-tracker.md).
