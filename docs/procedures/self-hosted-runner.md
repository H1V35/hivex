# Self-hosted GitHub Actions runner

Follow the [engineering guidelines](../guidelines/engineering.md#development-and-verification) for runner labels, execution boundaries and verification requirements.

Install Bun, Git, rustup and the macOS Command Line Tools on the host. Quality installs the stable Rust toolchain with rustfmt and Clippy and selects `/Library/Developer/CommandLineTools` for native compilation, independently of the interactive Xcode selection. Keep the Cargo lockfile frozen during CI.

Runner installation is repository administration, outside Hivex's product. Register the admitted macOS ARM64 release from GitHub's runner settings in its own directory, verify the official checksum, and use the generated `svc.sh install/start/status/stop` commands. Keep automatic updates enabled, a stable Homebrew/system PATH and the Mac awake and connected under the logged-in user. The runner work directory must be separate from the developer checkout and other repository runners. Only trusted code may run on this persistent host; review that boundary before public contributions. See [GitHub's runner reference](https://docs.github.com/en/actions/reference/runners/self-hosted-runners).

During a host migration, disable Actions, register and confirm the runner is online, and merge all workflow routes before re-enabling Actions. Then dispatch Quality on that exact revision and check the assigned runner and completed result. The route change alone is not functional verification.
