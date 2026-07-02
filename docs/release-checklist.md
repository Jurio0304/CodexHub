# CodexHub v0.2.0 Release Checklist

Use this checklist before creating a public GitHub Release. Do not create a release from the `dev` channel, and do not publish `stable` until the user explicitly approves public availability.

## Channel Gate

CodexHub has exactly two channels:

- `stable`: public release candidate after validation and explicit user approval.
- `dev`: development, testing, preview, and manual acceptance.

Channel identities:

| Channel | productName | identifier | Window title | Local config directory | Local cache directory |
| --- | --- | --- | --- | --- | --- |
| `stable` | `CodexHub` | `com.jurio.codexhub` | `CodexHub` | `%APPDATA%\com.jurio.codexhub` | `%LOCALAPPDATA%\com.jurio.codexhub` |
| `dev` | `CodexHub Dev` | `dev.codexhub.desktop` | `CodexHub Dev` | `%APPDATA%\dev.codexhub.desktop` | `%LOCALAPPDATA%\dev.codexhub.desktop` |

The directory split comes from Tauri `app_config_dir()` and `app_cache_dir()` resolving under the OS roots plus the bundle identifier. This isolates app-owned local state only. It does not automatically isolate `%USERPROFILE%\.ssh\config`, local SSH keys, remote `~/.codex/config.toml`, remote `~/.codex/skills/`, or remote shell files.

## Automated Checks

Run from the repository root:

```powershell
pnpm smoke
pnpm smoke:mock
pnpm typecheck
pnpm build:web
cargo test --manifest-path src-tauri/Cargo.toml
git diff --check
```

Stable release packaging checks:

```powershell
pnpm build:tauri
pnpm audit:public
pnpm release:portable
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\check-release-exe.ps1
```

Optional dev preview packaging:

```powershell
pnpm build:tauri:dev
pnpm release:portable:dev
```

Expected result:

- Web build succeeds.
- Rust tests pass.
- Tauri stable release exe builds with `--no-bundle`.
- Portable stable zip and SHA256 checksums are created under ignored `release-artifacts/`.
- Public audit reports no secret, host, local state, or build-output leaks.
- The release exe starts with temporary app data and does not exit during the startup check.
- `dev` artifacts are clearly named `CodexHub Dev` or `CodexHub-Dev-*` and are not published as stable releases.

## Package Review

Inspect `release-artifacts/CodexHub-v0.2.0-windows-x64-portable.zip`:

- Includes `CodexHub.exe`.
- Includes README, license, security notes, known limitations, public scope, release checklist, release channel docs, and Chinese README.
- Does not include local app state.
- Does not include SSH config, known hosts, private keys, `.env*`, logs, local databases, or installer cache.
- SHA256 checksum matches `release-artifacts/SHA256SUMS.txt`.

If a dev preview package is created, inspect `release-artifacts/CodexHub-Dev-v0.2.0-windows-x64-portable.zip` and confirm it contains `CodexHub Dev.exe`.

## Live SSH Acceptance

Run this only with an explicit test host. Do not use production secrets or personal hosts in public logs.

1. Start from an isolated Windows profile or confirm existing SSH keys are preserved.
2. If no suitable key exists, generate Ed25519 key in CodexHub.
3. Add SSH host with one-time password setup.
4. Confirm the local SSH config write is limited to one CodexHub-managed block.
5. Run `ssh <alias> echo ok` from CodexHub and optionally from PowerShell.
6. Probe the remote host.
7. Install or update remote Codex.
8. Confirm the remote command is the real `codex`.
9. Create a profile.
10. Preview profile apply.
11. Apply profile and confirm backup/no-change behavior.
12. Import a local or GitHub skill.
13. Install the skill to a selected target.
14. Open Tasks and inspect redacted stdout/stderr.
15. In Codex App, open `Settings > Codex > Connections`.
16. Add or enable the verified SSH alias manually.

## Release Notes

Mention these v0.2.0 boundaries:

- Stable keeps the user-visible brand `CodexHub`.
- Dev is branded `CodexHub Dev` and uses a separate identifier and app data/cache directories.
- Portable zip is the primary Windows artifact.
- MSI packaging requires WiX and may need network/cache preparation.
- CodexHub does not write Codex App private state.
- Live SSH acceptance is host-specific and should be recorded with sanitized evidence only.
