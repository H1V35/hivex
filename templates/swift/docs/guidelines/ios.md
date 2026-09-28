# Native iOS presentation standard

Apply this guide alongside the [Swift standard](swift.md) to Swift iOS/iPadOS app targets and their presentation code. Do not apply its screen, lifecycle or visual rules to a general Swift CLI, server or platform-independent library. DDD, domain responsibilities, testing philosophy and the Hivex workflow remain in the project's existing engineering authority; this guide supplies iOS-specific application of those decisions.

This initial standard is grounded in accepted presentation and client-authority choices and primary platform guidance. Refine it using the project's real iOS implementation and measurements. It is not a claim that an unbuilt app has been validated, nor an Xcode starter, MVVM framework or component library.

## Toolchain and platform contract

Record supported Xcode/Swift builds, language mode, SDKs, deployment targets, device families, orientations/windows and capability requirements. Do not copy another project's minimum OS or CI provider. SwiftUI is the default; use UIKit when a demonstrated UX, accessibility, integration or performance requirement makes it useful. UIKit is not obsolete.

Use [Observation](https://developer.apple.com/documentation/swiftui/managing-model-data-in-your-app) as the model-observation baseline where supported: SwiftUI integration is available from iOS/iPadOS 17. If the project's supported OS range needs another approach, document the compatibility decision instead of blindly adopting old `ObservableObject`/Combine or RxSwift recipes. Newer SDK availability is not permission to call new APIs on older supported systems. Liquid Glass and other system materials belong to the project's supported OS/design contract; use native behavior and availability handling, not a universal minimum OS or a hand-built imitation.

## Presentation MVVM without empty layers

Organize code by feature and real responsibility. Use a Swift ViewModel for a screen/flow that coordinates meaningful presentation state, loading, actions or errors. Simple/decorative views may use local state or observe model data directly. There is no requirement for one ViewModel per view, `BaseViewModel`, a protocol per class, forwarding services, MVVM-C, TCA, Clean Architecture or a global coordinator.

A SwiftUI view renders state and emits actions. Use `@Observable` models with `@MainActor` isolation when they own UI presentation state, and `@State`/`Binding` for appropriate local ownership and editing. Observation tracks changes; it is not synchronization by itself. Keep dependencies explicit at the feature's composition boundary, without a universal dependency-injection framework.

Separate authoritative data, derived presentation projections and ephemeral visual state. Own models and tasks for the lifetime of their screen/flow and account; do not recreate them or start side effects on each evaluation of `body`, or retain every screen in a global store. Resolve navigation and deep links using native navigation/state and the smallest structure the actual flows require. Deep links do not bypass authentication or authorization.

MVVM is this presentation baseline, not an official requirement of SwiftUI or an asserted industry winner. Apple's [WWDC26 SwiftUI Group Lab](https://developer.apple.com/videos/play/wwdc2026/8006/?time=63) explicitly leaves architecture open. Extract views/models because they improve responsibility, state ownership or update behavior, not to satisfy a file count or hide complexity.

## Client authority and useful offline behavior

For a connected product, keep authoritative business transitions, authorization/visibility, quotas/entitlements, purchases, availability and ranking on the owning backend. It must supply projections already authorized for the recipient; never send private data for the UI to hide. ViewModels must not reconstruct those rules. Code embedded in another language is still client code, not server authority.

The client owns UI, navigation, formatting/localization, feedback, basic input validation for UX, OS integration, local security, cache/persistence, durable offline intents and explicitly local capabilities. Thin presentation does not mean a network request for every gesture or removing useful cached/offline behavior. Local validation does not replace server validation, and optimistic state must distinguish saved locally, pending, confirmed and rejected/uncertain outcomes.

Keep one durable owner for each dataset and operation queue. If an existing shared core owns persistence/sync, ViewModels request operations and observe its projections; they do not add another outbox, database writer or deduplication policy. A pending intent survives the screen and process within its permitted lifetime. Outside that shared scope, native Swift services and generated HTTP clients remain valid. No shared Rust core, UniFFI, SQLite driver, translation service, encryption protocol or cloud provider is mandated by this guide.

If a project chooses a shared core, define and verify the actual Swift/FFI contract: ownership, lifecycle, cancellation, typed errors and callbacks delivered to the correct actor. Never block UI execution with synchronous database, cryptography or decode work. Use maintained integrations; do not rewrite the engine or duplicate its tests in every client.

## Lifecycles, cancellation and state integrity

Use structured async work where possible. A view-bound task can end with the view; durable work needs ownership outside that lifetime. Cancellation and actor isolation do not prevent stale logical results or actor reentrancy across `await`. Capture and revalidate request identity and session/account generation before applying results or side effects: search A must not overwrite search B, and a delayed 401 from account A must not clear account B's session.

Pending operations, errors and retries retain account, author, operation identity and expiry. Distinguish definitive rejection, transient failure and an unknown outcome. A lost response is not proof that an effect failed; reconcile before retrying and preserve the server's idempotency contract. Recheck permissions on the server, and apply known revocations locally before allowing protected access or replay. Keep account-switch cleanup scoped to that account's tasks, caches, files and keys; do not discard another account's valid pending work.

Specify behavior across foreground/background, process termination, cancellation during writes, disk exhaustion, local schema migrations and restored backups. Test the states that could lose or resurrect work. [Background Tasks](https://developer.apple.com/documentation/backgroundtasks) provides system-managed execution opportunities, not a permanent process or guaranteed schedule. Make work resumable and handle expiration; do not promise continuous background synchronization. Account limits, offline leases and retention durations belong to the product's authority, not to template defaults.

## Local security and transport

Use [Keychain services](https://developer.apple.com/documentation/security/keychain-services) and appropriate native data protection for secrets; select accessibility/backup behavior for the product's locked-device and recovery requirements. Keychain is not the database for all application data. Separate accounts' stores, queues, files and keys. Do not put tokens, keys, private messages or personal data in logs, fixtures, crash metadata or diagnostics; sanitize before telemetry leaves the device.

Respect system permissions and least-privilege capabilities even when business authority is remote. Hiding a button is not access control. Use platform TLS and maintained cryptographic libraries; do not add an authentication method, recovery workflow or end-to-end encryption protocol from a style guideline. Local-only features must remain local if that is the accepted product policy, including failure behavior without an unapproved cloud fallback.

Apply the Swift guide's transport and FFI rules to HTTP and shared-core boundaries. Local persistence needs its own schema and migration policy; a generated transport DTO is not automatically that model. Verify compatibility across admitted app/server versions.

## Native UX, accessibility and measured performance

Prefer native controls, pickers, navigation and semantic actions. Support Dynamic Type, VoiceOver, sufficient contrast, reduced motion/transparency, focus/keyboard interaction, meaningful disabled/loading/empty/error states, localization and adaptive iPhone/iPad windows. Test real flows with accessibility settings; invisible gestures are not replacements for discoverable accessible controls. SwiftUI supplies a useful [accessibility foundation](https://developer.apple.com/documentation/swiftui/accessibility-fundamentals), but composed screens and UIKit bridges still require verification.

Where a product spans platforms, share semantic design foundations such as color roles, typography roles, spacing, content and interaction states. Keep their native implementations coherent and document intentional differences. This does not require React components in Swift, a wrapper for every system control, pixel-identical layouts or simultaneous releases. Choose token tooling only for actual consumers; no design-service dependency is implied.

Keep `body` free of heavy computation and side effects. Use stable list identities and narrow observation dependencies; bound media decoding, paging, prefetch, cache and memory, and cancel obsolete requests. Opening a screen must not download an entire history or gallery. Extract a subview for meaningful state/update boundaries; do not introduce preventive caches, type erasure or memoization everywhere.

Measure representative startup, frame responsiveness, CPU, memory, battery, binary size and network work with Instruments and relevant native tooling. Record device/OS, data size, network conditions, cold/warm state and distributions, not only successful averages. Use real devices when judging battery, memory, OS lifecycle or native UX. MVVM, Swift, Rust and compiler success do not demonstrate performance; product-specific resolution, quotas, TTLs and latency targets stay in product documentation.

## Verification and adoption evidence

Use [Swift Testing](https://developer.apple.com/documentation/testing) for new non-UI logic tests where appropriate, and [XCTest](https://developer.apple.com/documentation/xctest) with XCUIAutomation for important UI/system flows and performance. Do not mix the two APIs in one test or duplicate every scenario across them. Existing valuable Maestro or other tests may remain where they cover a real risk; adoption does not require another harness by symmetry.

Follow Hivex's risk/value approach: TDD is optional for critical behavior sufficiently defined in advance. No test per function, ViewModel, view, button or wrapper; no coverage-percentage target, implementation-shaped mocks, massive snapshots or re-testing library guarantees. Keep a few valuable end-to-end flows. Test shared core rules once and add minimal real bridge/storage/Keychain/lifecycle integration where the client boundary can fail. Mocks alone do not demonstrate an actual race, permission, database transaction or system capability.

Prioritize relevant scenarios: response loss and duplicates; rapid search/account changes and late errors; revocation; purchase/entitlement uncertainty; pending work across termination; permission denial; expiry, migrations and restoration. Exercise the real boundary for the risk, with synthetic isolated data. Do not enable production services or create a prototype merely to decorate documentation evidence.

On adoption, record the app's real build/test commands, configurations, schemes, simulator/device destinations and signing/entitlement checks. For example, replace the uppercase values before running:

```sh
xcodebuild -version
xcodebuild -list -project PROJECT.xcodeproj
xcodebuild -showdestinations -project PROJECT.xcodeproj -scheme SCHEME
xcodebuild -project PROJECT.xcodeproj -scheme SCHEME \
  -destination 'platform=iOS Simulator,id=SIMULATOR_UDID' build test
```

Use `-workspace` instead of `-project` when that is the actual build container; do not assume a scheme or install a CI provider. Run the Swift formatting/lint/compiler checks from the language guide as well. A simulator build does not validate device-only entitlements, notifications, background opportunities, FFI behavior or performance. Record what passed and what remains unverified before claiming readiness. Installing Hivex or copying these guides is not applying the checks, migrating an app or authorizing a release.
