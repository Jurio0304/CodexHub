# CodexHub Known Limitations

Date: 2026-07-01

## macOS

macOS release-build support is merged, but the current CI artifact is unsigned unless Apple Developer ID signing and notarization are configured. Real Mac validation remains required before treating macOS behavior as fully verified. macOS artifacts are not part of the v0.2.1 public GitHub Release.

The following items require real macOS test:

- Launching the `.app` and `.dmg`.
- Gatekeeper behavior for unsigned/ad-hoc release artifacts.
- `~/.ssh/config` writes and backups on a real Mac account.
- Local Codex CLI detection through Homebrew, `/usr/local/bin`, `~/.local/bin`, and `which codex`.
- Codex App Settings / Connections / SSH handoff.

## Codex App Integration

No public stable API was found for either of these actions:

- Automatically adding or enabling an SSH host inside Codex App.
- Forcing Codex App to reconnect to an SSH remote.

MVP mitigation: CodexHub provides a clear UI fallback with verified SSH aliases, copyable commands, and manual Codex App settings steps. CodexHub must not write Codex App private state.

If Codex App supports a public documented SSH deep link on the tester's machine, CodexHub may present it as a convenience only after writing `~/.ssh/config`. It must not depend on undocumented Codex App files, databases, sockets, or private IPC.

## Remote Host Requirements

The documented Codex App remote flow targets Linux remote machines with SSH access, POSIX-compatible shell behavior, writable home directory, and `scp` support. Windows remote hosts are not an MVP target.

Remote Codex install/update also expects a writable `~/.local/bin` and `~/.codex`. The first install path needs `curl` or `wget` for the official standalone installer. If the official endpoint is unreachable, CodexHub falls back to the npmmirror native package path, which currently needs `python3`, `tar`, and a supported Linux CPU architecture (`x86_64` or `aarch64`). If that fallback is unavailable, CodexHub tries `npm install -g @openai/codex --prefix "$HOME/.local" --registry=https://registry.npmmirror.com`. If remote downloads are blocked or redirected but SSH/SCP still works, CodexHub can download the npmmirror native package on the local Windows machine, upload the tarball with `scp`, and install it into the same user-owned remote paths.

Some SSH non-interactive shells do not read `~/.bashrc` or `~/.zshrc`, so a plain `ssh <HostAlias> 'codex --version'` may still miss the repaired PATH until the next interactive shell. CodexHub's resolver checks `~/.local/bin/codex` directly so Check Version and follow-up install/update verification still work.

If a remote host's CA bundle rejects HTTPS downloads with a self-signed certificate error, the safer long-term fix is to repair that host's trust store. For first-run recovery, CodexHub keeps the official installer strict but may retry npmmirror native package downloads with certificate checks disabled, limited to npmmirror URLs and marked as `npm-mirror-native-insecure-tls` in the task log. If the insecure retry returns HTML instead of package metadata, CodexHub reports the likely captive portal or network authentication issue and then attempts the local-download plus `scp` upload fallback.

The Profiles / 配置 page owns local profile editing, import/export, API env-var selection, single-host apply, selected-host batch apply, and the compact Codex readiness/actions list. Host pages stay focused on SSH identity, remote probes, and diagnostics.

Long SSH, probe, install, and update operations are dispatched through backend blocking workers so the WebView remains responsive. They are still bounded by per-step timeouts, and a full install/update can take longer than a single timeout because official download, mirror fallback, local download, upload, install, and verification are separate steps.

## Skills Path Drift

OpenAI public docs currently show both `.agents/skills` style paths and `~/.codex/skills` references in different Codex pages. MVP follows the product requirement and manages `~/.codex/skills/`, but the backend keeps the skill root configurable and should later detect path support per host.

Window 6 online skill discovery accepts direct GitHub repository URLs and GitHub `tree/<branch>/<skill-path>` subdirectory URLs. The repository is downloaded only after the URL passes the `https://github.com/<owner>/<repo>` allowlist and the clone is validated for `SKILL.md` in the repository root, selected tree subdirectory, or immediate child directories. Remote skill install assumes Linux SSH/SCP targets with `tar`; missing tools are reported in the task log.

## Local Toolchain

This repository contains a Tauri 2 skeleton. Full `pnpm dev` requires local Node/pnpm, Rust stable MSVC toolchain, and WebView2. The current smoke test is dependency-light and validates the skeleton without compiling Rust.

## SSH Config Discovery And Writes

CodexHub may scan `%USERPROFILE%\.ssh\config` to auto-import safe `Host` aliases, but this discovery path is read-only. User-owned unmanaged `Host` blocks must not be modified, deleted, reordered, or reformatted.

Any SSH config write must be explicit, backed up, scoped to a CodexHub-managed marker block, and idempotent. If an alias already exists in an unmanaged block, CodexHub must reject the write instead of overwriting it.

New CodexHub-managed hosts may be bootstrapped with a one-time remote password. CodexHub uses that password only for the current request, does not store it, installs the selected local public key to remote `~/.ssh/authorized_keys`, sets `~/.ssh` and `authorized_keys` permissions, and then verifies key login with system OpenSSH. The modal shows each step in real time and stops on the first failure. Successful OpenSSH checks use `StrictHostKeyChecking=accept-new`, so first-time host keys are trusted automatically while changed host keys still fail.

## No Wrapper Dependency

The MVP intentionally does not require a remote Codex wrapper. This limits runtime control over live Codex sessions but keeps the first version aligned with public interfaces and minimizes remote installation risk.

## Security

CodexHub must not store SSH private keys, passphrases, OpenAI tokens, or remote secrets in plaintext. Local profile data may contain a credential-store key reference, but not the credential value.

Profile API key handling is env-var-first. Remote config writes must use `env_key` / `apiKeyEnvVar` so the remote host reads its own environment variable. The optional stored local credential key is never written to remote `~/.codex/config.toml`, `applied-profile.json`, or task logs.
