---
title: Independent Bun installation with preserved dependency policy
status: accepted
date: 2026-09-07
---

# Independent Bun installation with preserved dependency policy

## Context

The owner selected Bun as Hivex's runtime and package manager and explicitly requested version
1.4.2. Compi's transition remains tracked in [#1162](https://github.com/H1V35/compi/issues/1162);
its Expo/native compatibility is a separate admission requirement. Hivex must install, check and
execute without a parent workspace or pnpm CLI.

Local Bun 1.4.2 canaries found that ordinary incremental frozen installation can accept a changed
lock integrity when it reuses cached content. It can also retain altered installed files or copy
altered extracted cache files. Fresh-cache, forced installation with cache reads disabled rejected
the altered integrity and restored the fixture contents. These observations motivate the admission
command; they do not establish a general operating-system integrity guarantee.

## Decision

Hivex pins Bun 1.4.2, one Bun lock and an isolated dependency layout. Its verified installer uses
`--frozen-lockfile --force --no-cache --ignore-scripts` and a new task-owned cache directory.
It leaves home directories and global package configuration unchanged. The dependency policy
retains a seven-day publication floor, no publishing-trust downgrade, exact script dispositions
and the existing `eslint-plugin-sonarjs>eslint` major-10 allowance. The owner's explicit Bun runtime
upgrade does not relax the age policy for unrelated packages.

A checked-in bootstrap bundle permits registry preflight before any target dependency is installed
or imported. Its public pnpm resolution verifier is a library running under Bun; it does not invoke
pnpm or implement another publishing-trust algorithm. Separately, Hivex validates each locked
canonical package identity, SHA-512 integrity against the version's registry metadata and an explicit
per-version publication time. Missing evidence fails. Only npm registry packages and the supported
Bun lock/config versions are admitted by this installer.

Registry metadata is fetched once per distinct package name, retained with hashes and bounded in
size, concurrency and time. Internal verifier lookups over that metadata are counted separately
from HTTP downloads. A publishing-trust result does not substitute for the integrity comparison.

The installer verifies its input files have not changed before or during installation. Afterwards,
it checks installed canonical name/version coverage against the platform-applicable locked set
and resolves peers from each installed consumer. An absent optional peer is allowed; an installed
incompatible optional peer still fails. A parent-specific exception does not allow other consumers.
This coverage check supports the current standalone cohort; it is not a general dependency resolver
or proof of Compi's pnpm-to-Bun dependency-edge parity.

Hivex currently requires no dependency lifecycle scripts. `trustedDependencies` is explicitly empty;
the optional `msgpackr-extract@3.0.4` install script is explicitly denied. Verified installation always
disables scripts. Adding a required build script needs an explicit change to this admission path.

The bootstrap source, generated bundle and build record are reviewed together. The record binds
the source hashes, build lock, Bun version/revision and bundle hash. A matching hash establishes
consistency with that record, not independent trust in an unreviewed checkout. Dependency updates
must pass the existing trusted bootstrap before a replacement bundle is accepted.

The bundle fixes msgpackr's documented native-acceleration switch to disabled and keeps its optional
native addon external. It must not contain a construction directory or a bundled native loader.
This uses the library's JavaScript implementation without changing the publishing-trust verifier.
Provenance maps have stable key order. Reproduction is checked across distinct physical checkouts;
rebuilding twice at the same path is insufficient evidence of portability.

## Consequences

Hivex owns its TypeScript compiler, formatting and lint configuration. The native TypeScript 7
compiler uses the unscoped `typescript-native` alias so Bun installs its native optional package;
the separately named TypeScript 6 package provides the compatibility API needed by typed ESLint.
Only Hivex's filename and barrel rules are retained from Compi's custom lint plugin. Product UI
rules and architectural layer restrictions do not belong in this tool.

Fresh verified installs cost more downloads than incremental installs. They are the admission
boundary for CI and dependency changes, while ordinary CLI queries do not reinstall packages or
contact the registry. Tests, typechecking, lint and formatting remain required after installation.
An install failure preserves its evidence and refuses acceptance; it does not claim to restore an
earlier dependency directory. Compi's package-manager cutover and native proof remain separate.
