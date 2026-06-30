# CodexHub v1 Release Checklist

Use this checklist before creating a public GitHub release.

## Automated Checks

Run from the repository root:

```powershell
pnpm smoke
pnpm smoke:mock
pnpm typecheck
cargo test --manifest-path src-tauri/Cargo.toml
pnpm build:web
pnpm build:tauri
pnpm audit:public
pnpm release:portable
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\check-release-exe.ps1
git diff --check
```

Expected result:

- Web build succeeds.
- Rust tests pass.
- Tauri release exe builds with `--no-bundle`.
- Portable zip and SHA256 checksums are created under ignored `release-artifacts/`.
- Public audit reports no secret, host, local state, or build-output leaks.
- The release exe starts with temporary app data and does not exit during the startup check.

## Package Review

Inspect `release-artifacts/CodexHub-v0.1.0-windows-x64-portable.zip`:

- Includes `CodexHub.exe`.
- Includes README, license, security notes, known limitations, public scope, release checklist, and Chinese README.
- Does not include local app state.
- Does not include SSH config, known hosts, private keys, `.env*`, logs, local databases, or installer cache.
- SHA256 checksum matches `release-artifacts/SHA256SUMS.txt`.

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

Mention these v1 boundaries:

- Portable zip is the primary Windows artifact.
- MSI packaging requires WiX and may need network/cache preparation.
- CodexHub does not write Codex App private state.
- Live SSH acceptance is host-specific and should be recorded with sanitized evidence only.
