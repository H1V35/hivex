---
title: Independent Bun installation
status: accepted
date: 2026-09-07
---

# Independent Bun installation

## Context

The owner selected Bun 1.4.2 as Hivex's runtime and package manager. The first independent version added a custom installer, registry verifier and generated bootstrap to reproduce a prior dependency policy. The owner explicitly rejected that extra machinery: installation should work as in a project that started with Bun. This amendment replaces the initial decision; its original implementation and evidence remain in Git history, while the current tree keeps only reusable decisions.

## Decision

Use `bun install` for development and commit `bun.lock`. Use `bun ci` for frozen installation in CI and fresh checkouts. Bun 1.4.2 is the minimum runtime and the pinned development/CI baseline; compatible newer runtimes are accepted. Keep Bun's isolated dependency layout, seven-day minimum release age and explicit `trustedDependencies` allowlist, currently empty. Review required lifecycle-script additions with the dependency change.

The owner clarified on 2026-09-14 that routine tool updates must not be rejected solely because a version number differs. The consumer engine range reflects the supported minimum; the package-manager and CI pins retain a reproducible test baseline. Native Codex compatibility is established through the protocol and effective capabilities Hivex uses, with its actual version recorded in invocation evidence.

Do not maintain a parallel installer, publishing-trust verifier, registry metadata cache or generated bootstrap. Bun owns installation behavior; its native controls are not a claim of parity with every former pnpm policy. Remove code, dependencies and tests that served only the retired installer.

## Consequences

Hivex installs and runs independently of caller package managers. It owns its compiler, formatter and lint configuration. The native TypeScript 7 compiler uses the unscoped `typescript-native` alias so Bun installs its native optional package. The separate TypeScript 6 package provides the compatibility API needed by typed ESLint.

Typechecking, lint, formatting and relevant product tests validate dependency changes. They do not require a second installation framework.
