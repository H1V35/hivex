---
title: "Self-hosted runner pool"
status: draft
created_at: 2026-10-03
tags: [ci, runner, security]
---

# Self-hosted runner pool

Apply this procedure only when the project runs GitHub Actions on self-hosted runners; delete it otherwise. Use self-hosted runners only for private repositories and repository-owned branches. Public repositories and fork pull requests run on GitHub-hosted runners: a persistent personal machine must not execute untrusted code.

## Size the pool

Each runner executes one job at a time, so a single runner queues every other job. Register a pool, four by default, sized to the machine and to other projects' runners on it. Give each runner a distinct name, its own directory and its own launchd service, and the same routing label used by the workflows. Runner names are unique per repository.

## Install each runner

Download the official runner release, verify its checksum, and register each runner with a short-lived token, its name and the routing label. Never commit registration credentials. Before installing the service:

- Set a private `HOME` by adding `HOME=<runner directory>/home` to the runner's `.env`. Setup actions such as `setup-bun` or rustup install under `$HOME`; without a private home they replace the owner's own toolchain and concurrent jobs replace each other's binaries.
- Keep the service `PATH` in `.path` stable. Remove per-shell directories, such as `fnm_multishells`, which disappear with their shell.

Then install and start the service with the runner's `svc.sh`. Restart it after changing `.env` or `.path`.

## Keep parallel jobs isolated

Give every run its own external resources, such as a per-run database branch named from the run ID, and clean them up; use no fixed ports or other shared mutable state. Concurrency groups per pull request or ref cancel superseded runs without serialising unrelated work.

## Change the pool

GitHub cannot rename a runner. To rename or retire one, wait until it is idle, then stop and uninstall its service and remove its registration with a removal token; register it again under the new name if needed. Verify that every runner is `online` with the expected labels, and that parallel jobs succeed using their private toolchains.
