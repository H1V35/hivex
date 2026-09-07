# Bun 1.4.2 standalone admission

This evidence covers Hivex's independent installation and bootstrap. It does not admit Compi's
package-manager/native cutover or the documentation graph rebuild and grounding requirements.

## Source and runtime

The imported product history is the `hivex/` subtree of Compi
`e2c972d5d402add4f6883c87e06378c2a86b9c20`, split without squashing as
`ad3ca85791cd1cab01eaba4dc2fd22182eaa8846`. The initial import commit is
`81c05ff28ca6f735f21d34fcc08fb4ed3aaa829a`; its bootstrap portability defect is corrected by the
subsequent change containing this evidence. Historical extraction and plan evidence retains its
original revisions and scope.

Bun reports `1.4.2`, revision `744846f844374847c902b5e7fd59b4342a51ef99`. The macOS arm64 binary
SHA-256 is `35d20dd0263e5c950194434b925454fdfa9ba6e4467da960410fa05b08a7a5b5`. The owner explicitly
requested this runtime release; unrelated dependencies retain the seven-day floor.

## Dependency assessment

[Registry admission](registry-admission.json) covers all 279 locked registry entries, with no
reported trust, integrity or publication-age failures. It binds the Bun lock
`f375fb06b0b9f3eb4182c29f725dfba06df496e8e287c6d87a851756ea611fb0`. Its configuration hashes identify
the recorded install inputs; later test-script/documentation changes are not silently folded into
that earlier record. There are 269 distinct metadata downloads; verifier lookups use those local
responses and are counted separately.

[Direct dependencies](direct-dependencies.json) records exact versions, publication dates, publishers,
maintainers, repository declarations, direct transitive requirements, install hooks and SHA-512
integrities for the added verification/formatting tools. The full transitive package list and
publishing-trust result are retained in the registry report. All six selected direct packages have
no install lifecycle script and clear seven days. `semver`, `@types/semver` and Prettier have zero
runtime dependencies; the public pnpm verifier and its logger/worker peers bring the pnpm dependency
set. This is a deliberate reuse of the already evaluated public verifier from
[Compi #1619](https://github.com/H1V35/compi/issues/1619), avoiding another trust implementation.
No pnpm CLI or private pnpm import is used.

The pnpm packages retain publisher `pnpmuser` and maintainers `pnpmuser`/`zkochan`. Their old registry
repository paths have moved; the public source identities were verified under
[pnpm11/resolving/npm-resolver](https://github.com/pnpm/pnpm/tree/main/pnpm11/resolving/npm-resolver),
[pnpm11/core/logger](https://github.com/pnpm/pnpm/tree/main/pnpm11/core/logger) and
[pnpm11/worker](https://github.com/pnpm/pnpm/tree/main/pnpm11/worker). These current paths establish
repository continuity, not byte-equivalence to a historical source tag.

The `semver@7.8.5` manifest matches its recorded
[source commit](https://github.com/npm/node-semver/tree/6e05b7637396ac66522cff8731f07cfe0ef49a29).
`@types/semver` is published by the existing DefinitelyTyped `types` account; its declared
[source directory](https://github.com/DefinitelyTyped/DefinitelyTyped/tree/master/types/semver)
exists with the expected package identity. Its source uses the normal development version placeholder;
this is not a claim of an exact publication checkout. Prettier retains its established maintainers
and the [3.8.4 source tag](https://github.com/prettier/prettier/tree/3.8.4) has the matching manifest.
Its registry `gitHead` was not accessible, so it is not presented as verified.

The verifier parses public registry JSON and Bun lock data in the local tooling process. It is
JavaScript with bounded metadata reads. The bootstrap disables msgpackr's optional native
acceleration at build time; an owned sentinel test proves it does not load an available external
addon. The installed dependency's optional `msgpackr-extract@3.0.4` hook remains explicitly denied.

[Installed peers](installed-peers.json) records 246 platform-applicable identities on macOS arm64,
without missing or unexpected identities. The existing SonarJS-to-ESLint-major-10 exception is
scoped to that consumer. This does not prove Compi's full dependency-edge parity.

## Bootstrap and behavior

[Cross-root reproduction](cross-root-build.json) compares all five generated artifacts from two
physical checkouts with independent installed dependency directories. Every artifact is byte-identical.
The build refuses a construction-directory literal or a bundled `msgpackr-extract` input. Provenance
maps use stable ordering; all shipped third-party notices are preserved in the generated artifacts.
The reproduction record binds artifacts, not a future source revision.

The maintained tests invoke the actual Bun installer against owned fixtures. They prove rejection
of altered lock integrity after a warm installation, repair of altered installed content, the
configured age floor and replacement of Bun's default script trust by an explicit list. Separate
entrypoint tests exercise the installer's refusal of an altered bootstrap, adverse preflight,
mutated inputs, installation failure and adverse installed check, plus successful phase completion.
These tests create no external package or service; the registry canaries use an ephemeral loopback
server and their own tarballs.

The product suite invokes the real CLI in temporary Git repositories for help, search, read,
relations, extraction protocol fixtures and planning. It checks source hashes, anchors, limits,
continuations and complete Markdown blocks. Its fake native servers make no model call.

macOS typecheck, lint, formatting and tests run locally. The Ubuntu workflow executes installation,
those same checks and bootstrap reproduction on the exact PR head. Until that run passes, Linux
admission is pending. No iOS/Expo result is implied by this independent Hivex evidence.
