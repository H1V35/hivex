---
title: Independent Bun installation
status: accepted
date: 2026-09-07
---

# Independent Bun installation

## Context

The owner selected Bun 1.4.2 as Hivex's runtime and package manager. The first independent version
added a custom installer, registry verifier and generated bootstrap to reproduce Compi's dependency
policy. The owner explicitly rejected that extra machinery: installation should work as in a project
that started with Bun. This amendment replaces the initial decision; its original implementation and
evidence remain in Git history and the bounded historical evidence directory.

## Decision

Use `bun install` for development and commit `bun.lock`. Use `bun ci` for frozen installation in CI
and fresh checkouts. Pin Bun 1.4.2 in the package manifest and CI. Keep Bun's isolated dependency
layout, seven-day minimum release age and explicit `trustedDependencies` allowlist, currently empty.
Review required lifecycle-script additions with the dependency change.

Do not maintain a parallel installer, publishing-trust verifier, registry metadata cache or generated
bootstrap. Bun owns installation behavior; its native controls are not a claim of parity with every
former pnpm policy. Remove code, dependencies and tests that served only the retired installer.

## Consequences

Hivex installs and runs independently of Compi and pnpm. It owns its compiler, formatter and lint
configuration. The native TypeScript 7 compiler uses the unscoped `typescript-native` alias so Bun
installs its native optional package. The separate TypeScript 6 package provides the compatibility
API needed by typed ESLint.

Typechecking, lint, formatting and relevant product tests validate dependency changes. They do not
require a second installation framework. Compi's Bun conversion and Expo/native compatibility remain
separate work under [#1162](https://github.com/H1V35/compi/issues/1162).
