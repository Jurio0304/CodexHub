# CodexHub Known Limitations

Date: 2026-07-17

## macOS

macOS release-build support is merged, and its real-device validation baseline was completed. The v0.4.6 artifact remains unsigned/ad-hoc until Apple Developer ID signing and notarization are configured. First launch may require Control-click > Open or Privacy & Security approval after the user confirms the file came from the project GitHub Release.

The following macOS limitations remain:

- Gatekeeper behavior for unsigned/ad-hoc release artifacts.
- Developer ID signing and notarization are not configured.
- Future changes to lifecycle, packaging, SSH path handling, or Codex App handoff must re-run the real Mac checklist in `docs/macos-support.md`.

## Linux Desktop

Linux desktop support targets Ubuntu/Debian x86_64 and arm64 `.deb` packages first. After real Linux desktop validation, signed `.deb` assets are included in the stable updater feed as `linux-x86_64` and `linux-aarch64`.

The following Linux desktop limitations remain:

- rpm, AppImage, Snap, and Flatpak packages are not in scope for v0.4.6.
- Linux packages require real Ubuntu/Debian desktop validation before broad distribution.
- Package-repository, Snap, Flatpak, and rpm upgrade paths remain later work.

## Codex App Integration

No public stable API was found for either of these local App actions:

- Automatically adding or enabling an SSH host inside Codex App.
- Forcing the local ChatGPT/Codex App to reconnect to an SSH remote.

After a profile is applied, CodexHub can use SSH to send `SIGTERM` to strictly confirmed Codex processes owned by the current remote UID. The default mode targets App services and preserves interactive CLI/exec sessions; users may skip reload or explicitly acknowledge stopping all confirmed remote Codex sessions. This can prompt the local App to establish a replacement service, but it does not control the local App itself.

If no replacement App service is observed within 15 seconds, the saved configuration remains active and the result directs the user to `Settings > Codex > Connections` in the local ChatGPT/Codex App. Remote reload is available only inside profile apply; there is no standalone Host reload button.

CodexHub never reads or writes local ChatGPT/Codex App private files, databases, sockets, caches, or IPC. The normal host-registration fallback still uses verified SSH aliases, copyable commands, and manual App settings steps.

If Codex App supports a public documented SSH deep link on the tester's machine, CodexHub may present it as a convenience only after writing `~/.ssh/config`. It must not depend on undocumented Codex App files, databases, sockets, or private IPC.

## Managed Runtime Versions And Disk Cleanup

For a CodexHub-managed standalone installation, runtime reconciliation keeps `~/.codex-hub/codex-target` on the verified executable selected through `~/.codex/packages/standalone/current` (`bin/codex` or the current official `codex` layout) and accepts the operation only when the target, managed launcher, and login-shell `codex` command report one version. Update treats both the active version and a separately verified pre-operation `standalone/current` version as minimums and records the latter's exact release entry/layout. Install/update stages and reconciliation also re-read current, active, and login-shell versions under a shared current-user writer lock, then reject a candidate or post-write state below the highest verified comparable version. Profile apply captures the login-shell-visible version before writing the managed launcher, so neither path may reactivate a lower runtime version.

Old-version cleanup is limited to direct `releases/<entry>` directories with exactly one canonical executable layout and an entry matching the verified binary version. Install and Profile apply still require a valid `.codexhub-managed-release` marker. Only a user-triggered Update, and only after its new runtime passes final verification, may atomically adopt an unmarked release that is strictly older than that verified version. Eligible Update objects are removed from active runtime paths by moving them into a timestamped `~/.codex-hub/deletion-backups/` directory; the backup is retained and is not automatically purged. Same/newer versions and any existing invalid marker are preserved. CodexHub also retains the canonical current release, the target-protected release, and any release used by a current-SSH-UID process; an otherwise eligible in-use old version may be marked for cleanup on a later retry. Preserved `~/.codex-hub/codex-original.<timestamp>.<pid>` launcher captures always require their exact `.codexhub-managed-capture` sidecar, so historical unmarked captures are never adopted.

If the shared writer lock is active or cannot be verified, an object changes during verification, both supported binary layouts coexist, the backup is not on the same filesystem, or any other identity check is ambiguous, cleanup is deferred instead of guessing. Install/Profile managed-only cleanup still requires `/proc` to bind UID/PID/starttime/executable identity for every current-user process. The reversible staged-Update path has one narrow exception when `/proc/<pid>/exe` is unreadable: two stable reads may classify the process as an `sshd` session (with a non-empty `sshd: ` argv0 suffix), `(sd-pam)`, `sftp-server`, `fusermount3`, an exact `/usr/lib/systemd/systemd --user` or `/lib/systemd/systemd --user` process with no extra argument, or a zombie whose state is `Z`, command line is empty, `Threads` count is exactly one, and task directory contains only its leader TID. The initial PID/starttime/state/comm/full-command-line/class snapshot, unreadable executable, and both zombie single-thread proofs where applicable must match at every later release and capture check; a numeric TID that disappears after task-glob expansion is treated as a race. A new PID, changed identity, readable replacement executable, unknown process, multi-thread zombie, or Codex-like process still defers cleanup. The result reports `ignoredSessionProcesses` so this exception is visible. Releases and captures are quarantined within their own managed parent with GNU `mv -T -n` before repeated safety checks; no-replace support is mandatory. Its probe accepts the collision return codes used by GNU coreutils 8.x and 9.4 only after exact collision-file verification and an independent successful positive move, while every other failure remains blocking. Uncertain partial removal is never restored to an executable path. Cleanup receives a dedicated 360-second SSH budget because its cost grows with the number of release and capture candidates; other SSH commands keep the normal 120-second limit. The active verified runtime remains unchanged, and a later successful Update or Profile apply retries cleanup. Staged Update backups keep consuming the same disk space until the user explicitly deletes them after inspection; the current release does not provide an independent backup-management button.

## Remote Host Requirements

The documented Codex App remote flow targets Linux remote machines with SSH access, POSIX `/bin/sh` behavior, writable home directory, GNU coreutils-compatible `mv -T -n`, and `scp` support. Post-apply reload also requires readable `/proc/<pid>/status`, `stat`, `comm`, and `cmdline` for current-user processes plus `id`, `awk`, `sed`, `tr`, `grep`, and `sleep`. Managed-runtime locking and cleanup additionally require `ln`, `od`, `stat` starttime, canonical path checks, and normally a reliable `/proc/<pid>/exe`; only the staged-Update session-helper exception described above may proceed without that executable link. Missing or ambiguous identity evidence outside that exception is preserved and reported for manual recovery. Windows remote hosts are not an MVP target.

Remote Codex install/update also expects a writable `~/.local/bin` and `~/.codex`. CodexHub tries four methods sequentially on each host: the strict-TLS official standalone installer, a validated remote npmmirror native package, remote npm with the npmmirror registry and user-owned prefix, then a locally downloaded and validated native package uploaded with SCP. The mirror-native methods need `python3`, `tar`, and a supported Linux CPU architecture (`x86_64` or `aarch64`); the npm method needs remote npm; the local-upload fallback still needs working SSH/SCP. A method-level success is provisional until final path/version/shell verification passes.

Some SSH non-interactive shells do not read user startup files, so a plain `ssh <HostAlias> 'codex --version'` may still miss the repaired PATH until the next login or interactive shell. CodexHub repairs `.bashrc` or `.zshrc`, `.profile`, and existing `.bash_profile` / `.zprofile`; the resolver also checks `~/.local/bin/codex` directly, and probes run `command -v codex` in both the current shell and the configured login shell. Install/Update final verification requires the login-shell command; current non-login shell visibility remains diagnostic and produces a PATH warning without blocking successful runtime verification or old-release cleanup.

If a remote host's CA bundle rejects HTTPS downloads with a self-signed certificate error, the safer long-term fix is to repair that host's trust store. For first-run recovery, CodexHub keeps the official installer strict but may retry npmmirror native package downloads with certificate checks disabled, limited to npmmirror URLs and marked as `npm-mirror-native-insecure-tls` in the task log. If the insecure retry returns HTML instead of package metadata, CodexHub reports the likely captive portal or network authentication issue and continues through the remaining npm-mirror and local-upload fallbacks.

The Hosts / 主机 page owns SSH identity, diagnostics, host tests, and Codex install/update/uninstall actions. Profiles / 配置 owns local profile editing, import/export, API env-var selection, single-host apply, selected-host batch apply, and the associated remote process reload confirmation. There is no independent reload action on Hosts.

Long SSH, probe, install, update, and uninstall operations are dispatched through backend blocking workers so the WebView remains responsive. They are still bounded by per-step timeouts, and a full install/update can take longer than a single timeout because preparation, four fallback methods, and final verification are separate stages.

User-triggered batch Test and Update use a fixed maximum of six concurrent hosts. The limit is not configurable, per-host results are independent rather than transactional across the batch, and install/uninstall remain single-host actions. Hiding a progress dialog keeps the backend task running; detailed command output is retained under collapsed step cards in Tasks rather than streamed into the main UI by default.

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

The MVP intentionally does not require a separate remote Codex wrapper command. The user-facing command stays `codex`; when a stored profile key is applied, CodexHub may install a same-name `~/.local/bin/codex` launcher only to source `~/.codex-hub/env` and exec the real binary. Post-apply reload is a direct SSH `/proc` operation and does not add another user-facing command.

## Security

CodexHub must not store SSH private keys, passphrases, OpenAI tokens, or remote secrets in plaintext. Local profile data may contain a credential-store key reference, but not the credential value. One-time passwords and stored profile API keys can be revealed only through an explicit UI action; the revealed value stays in the active form state and is not written to browser storage or task logs.

Profile API key handling is env-var-first. Remote config writes must use `env_key` / `apiKeyEnvVar` so the remote host reads its own environment variable. When a profile with a stored local credential is explicitly applied, CodexHub writes the value only to the selected host's `~/.codex-hub/env` with restrictive permissions and shell-source backups. The key is never written to remote `~/.codex/config.toml`, `applied-profile.json`, app JSON, or task logs. Probes and profile apply tasks check whether the referenced remote env var exists without printing its value. Reload task history stores only the parsed mode, status, counts, replacement flag, and safe summary; raw SSH stdout/stderr and process command lines are discarded.
