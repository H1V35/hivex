# GitHub Actions execution

Quality runs on an ephemeral GitHub-hosted `macos-15` ARM64 runner. The owner approved this replacement of the personal Mac runner when preparing the repository for public contributions. This document keeps its existing path for published-package and historical links.

The workflow verifies `github-hosted`, macOS and ARM64 before checking the native package. It uses the image's selected Xcode toolchain and installs stable Rust with rustfmt and Clippy. Cargo, the Rust synthetic app-server, the package utility and system `tar` perform all checks; source verification does not require Bun or Node.js. The committed Cargo lockfile remains frozen.

Pull requests run with read-only repository permissions, checkout credentials are not persisted, and the workflow receives no publishing credentials or repository secrets. Keep package publication separate. Do not execute public contributions on a personal workstation or use `pull_request_target` to check out and execute an untrusted branch. GitHub's fork-approval policy is an additional control, not a substitute for isolated execution.

## Verification

Check the actual job's runner image, architecture and completed result. Quality runs formatting, compilation, Clippy with warnings denied, unit tests, package preparation, the complete CLI contracts against the release executable and exact-archive verification. A queued, skipped or interrupted run is not a pass; retain applicable local verification if CI is unavailable.

Standard GitHub-hosted runners are free for public repositories. While the repository is private, jobs use the account's included minutes and applicable billing. This workflow does not change account budgets or purchase capacity.

## Retiring the personal runner

After the hosted workflow passes, stop and uninstall only Hivex's local runner service, remove its repository registration, and verify that Hivex has no self-hosted runner remaining. Other repositories' runners are outside this change.

Preserve needed historical verification privately before deleting old Actions logs containing workstation details. Retain Git history, author attribution and useful decisions. Removing current files or logs does not purge historical commits or already published packages. Changing repository visibility is a separate explicit action.

## References

- [Hosted runner specifications](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
- [Secure use of Actions](https://docs.github.com/en/actions/reference/security/secure-use)
- [Removing self-hosted runners](https://docs.github.com/en/actions/how-tos/manage-runners/self-hosted-runners/remove-runners)
