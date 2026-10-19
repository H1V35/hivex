---
title: "Release publication"
status: accepted
created_at: 2026-09-17
updated_at: 2026-10-19
tags: [release, distribution]
---

# Release publication

A release is a Git tag `vX.Y.Z` on `main` whose version equals the one in `Cargo.toml`. Pushing the tag runs the `Release` workflow (`.github/workflows/release.yml`), which builds the `hivex` executable for `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu` and `x86_64-pc-windows-msvc` and publishes it as a GitHub release. Nothing is published to npm or another registry; the npm package `@h1v35/hivex` is deprecated after 0.11.1.

## Prepare the version

1. Set the new version in `Cargo.toml` and run `cargo update --workspace` so `Cargo.lock` records it.
2. Update the release-specific examples in `README.md` and the [guide](../guide.md#install-the-cli), and the documents whose content changes with the release.
3. Merge the change into `main` through a reviewed pull request whose `quality` check passed, under the [tracker procedure](issue-tracker.md).

Pushing the tag publishes the release, so it requires the owner's approval for that version and revision. Approval of another version or revision does not carry over.

## Tag and publish

```sh
git switch main
git pull --ff-only
git tag vX.Y.Z
git push origin vX.Y.Z
```

Each build job first fails when the tag differs from the `Cargo.toml` version. It builds the locked release executable for its target, writes the license notices with `cargo run --locked --bin hivex-dev -- notices <target> <output>` and packs `hivex-vX.Y.Z-<target>.tar.gz` (`.zip` for Windows) containing the executable, `LICENSE` and `THIRD-PARTY-NOTICES.txt`, with a `.sha256` checksum file. The notices cover normal and build dependency edges, excluding development-only analyzers. Only when every target has built does the publish job create the release `HiveX X.Y.Z` with generated notes and those eight files, so a failed build publishes nothing. Rerun failed jobs for a transient failure; a correction that changes the repository is published as a new version.

## Verify the release

Check that the release lists the four archives and their four checksum files, and verify a download:

```sh
gh release view vX.Y.Z --repo H1V35/hivex
gh release download vX.Y.Z --repo H1V35/hivex --pattern "*aarch64-apple-darwin.tar.gz*"
shasum -a 256 -c hivex-vX.Y.Z-aarch64-apple-darwin.tar.gz.sha256
```

Run the extracted `hivex --help` and `hivex init` in a temporary project, and check that `cargo install --git https://github.com/H1V35/hivex --tag vX.Y.Z --locked hivex --bin hivex --force` installs the same version. The Quality workflow tests the CLI contracts on macOS ARM64, Linux x86_64 and Windows x86_64; the `x86_64-apple-darwin` executable is built on the ARM64 runner without running them.

Never move, delete or recreate the tag of a published release, and never replace its archives with a local build. Publish a correction as a new version.
