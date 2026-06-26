# CodexHub Known Limitations

Date: 2026-06-25

## Codex App Integration

No public stable API was found for either of these actions:

- Automatically adding or enabling an SSH host inside Codex App.
- Forcing Codex App to reconnect to an SSH remote.

MVP mitigation: CodexHub provides a clear UI fallback with verified SSH aliases, copyable commands, and manual Codex App settings steps. CodexHub must not write Codex App private state.

## Remote Host Requirements

The documented Codex App remote flow targets Linux remote machines with SSH access, POSIX-compatible shell behavior, writable home directory, and `scp` support. Windows remote hosts are not an MVP target.

## Skills Path Drift

OpenAI public docs currently show both `.agents/skills` style paths and `~/.codex/skills` references in different Codex pages. MVP follows the product requirement and manages `~/.codex/skills/`, but the backend keeps the skill root configurable and should later detect path support per host.

## Local Toolchain

This repository contains a Tauri 2 skeleton. Full `pnpm dev` requires local Node/pnpm, Rust stable MSVC toolchain, and WebView2. The current smoke test is dependency-light and validates the skeleton without compiling Rust.

## SSH Config Discovery And Writes

CodexHub may scan `%USERPROFILE%\.ssh\config` to auto-import safe `Host` aliases, but this discovery path is read-only. User-owned unmanaged `Host` blocks must not be modified, deleted, reordered, or reformatted.

Any SSH config write must be explicit, backed up, scoped to a CodexHub-managed marker block, and idempotent. If an alias already exists in an unmanaged block, CodexHub must reject the write instead of overwriting it.

New CodexHub-managed hosts may be bootstrapped with a one-time remote password. CodexHub uses that password only for the current request, does not store it, installs the selected local public key to remote `~/.ssh/authorized_keys`, sets `~/.ssh` and `authorized_keys` permissions, and then verifies key login with system OpenSSH. The modal shows each step in real time and stops on the first failure. Successful OpenSSH checks use `StrictHostKeyChecking=accept-new`, so first-time host keys are trusted automatically while changed host keys still fail.

## No Wrapper Dependency

The MVP intentionally does not require a remote Codex wrapper. This limits runtime control over live Codex sessions but keeps the first version aligned with public interfaces and minimizes remote installation risk.

## Security

CodexHub must not store SSH private keys, passphrases, OpenAI tokens, or remote secrets in plaintext. Initial local data should only contain host aliases, paths, non-secret preferences, and operation metadata.
