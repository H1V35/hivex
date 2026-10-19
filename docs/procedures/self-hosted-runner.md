---
title: "GitHub Actions execution"
status: accepted
created_at: 2026-09-13
updated_at: 2026-10-19
tags: [ci, security]
---

# GitHub Actions execution

Quality runs on ephemeral GitHub-hosted runners: `macos-15` (ARM64), `ubuntu-latest` and `windows-latest` (x86_64). The owner approved hosted runners in place of the personal Mac runner when preparing the repository for public contributions. This document keeps its existing path for published-package and historical links.

Each platform job uses the stable toolchain with rustfmt and Clippy selected by `rust-toolchain.toml`. Cargo performs all checks; source verification does not require Bun or Node.js. A final `quality` job, which the `main` ruleset requires, passes only when every platform job passed. The committed Cargo lockfile remains frozen.

Pull requests run with read-only repository permissions, checkout credentials are not persisted, and the workflow receives no publishing credentials or repository secrets. Release publication runs only in the tag-triggered Release workflow under the [release procedure](releasing.md). Do not execute public contributions on a personal workstation or use `pull_request_target` to check out and execute an untrusted branch. GitHub's fork-approval policy is an additional control, not a substitute for isolated execution.

## Verification

Check each platform job's runner image and completed result as well as the aggregate `quality` check. On each platform, Quality runs formatting, compilation, Clippy with warnings denied, a release build and the complete tests, with the CLI contracts run against that release executable. Contract tests that create symbolic links run only on macOS and Linux. A queued, skipped or interrupted run is not a pass; retain applicable local verification if CI is unavailable.

Standard GitHub-hosted runners are free for public repositories. While the repository is private, jobs use the account's included minutes and applicable billing. This workflow does not change account budgets or purchase capacity.

## Retiring the personal runner

After the hosted workflow passes, stop and uninstall only HiveX's local runner service, remove its repository registration, and verify that HiveX has no self-hosted runner remaining. Other repositories' runners are outside this change.

Preserve needed historical verification privately before deleting old Actions logs containing workstation details. Retain Git history, author attribution and useful decisions. Removing current files or logs does not purge historical commits or already published packages. Changing repository visibility is a separate explicit action.

## References

- [Hosted runner specifications](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
- [Secure use of Actions](https://docs.github.com/en/actions/reference/security/secure-use)
- [Removing self-hosted runners](https://docs.github.com/en/actions/how-tos/manage-runners/self-hosted-runners/remove-runners)
