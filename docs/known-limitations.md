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

## SSH Config Writes

CodexHub will not modify `%USERPROFILE%\.ssh\config` by default. Any future write must be explicit, backed up, scoped to a CodexHub-managed block, and idempotent.

## No Wrapper Dependency

The MVP intentionally does not require a remote Codex wrapper. This limits runtime control over live Codex sessions but keeps the first version aligned with public interfaces and minimizes remote installation risk.

## Security

CodexHub must not store SSH private keys, passphrases, OpenAI tokens, or remote secrets in plaintext. Initial local data should only contain host aliases, paths, non-secret preferences, and operation metadata.
