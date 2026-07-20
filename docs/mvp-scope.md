# CodexHub MVP Scope

Date: 2026-07-17

## MVP Goal

Build a desktop app that helps a user manage Codex App SSH multi-server workflows safely. The first version is a control plane for remote Codex config and skills, not a replacement for Codex App.

## In Scope

- Tauri 2 + React + TypeScript + Vite desktop skeleton.
- Desktop UI and local smoke/mock mode.
- Local server inventory model.
- Read-only parsing of the local SSH config path for the current platform.
- Read-only auto-import of safe existing SSH `Host` aliases into CodexHub inventory.
- Optional SSH host-block generator.
- Optional append/update of CodexHub-managed SSH config blocks with backup.
- SSH connectivity check through the system OpenSSH client.
- Remote system and Codex probe: OS, arch, shell, PATH, installed Codex path, `command -v codex`, `codex --version`, `~/.codex/config.toml`, API env readiness, `~/.codex/skills/`.
- Remote Codex CLI maintenance through SSH: single-host Test, Install, Update, and Uninstall, plus user-triggered batch Test and Update through a fixed six-host sliding concurrency pool. Per-host install/update fallback stages remain sequential, target the remote user's home directory, and keep the user-facing command as `codex`; the primary UI entry is the host table on the Hosts / 主机 page.
- Shared managed-runtime reconciliation for Install/Update and Profile apply: serialize CodexHub runtime writers with a current-UID/PID/starttime lock, re-read a monotonic version floor after locking, keep a verified standalone target on the exact executable selected through `~/.codex/packages/standalone/current`, support legacy/local and official release layouts, verify target/launcher/login-shell versions agree, and reject lower candidates or post-write states.
- Safe old-runtime cleanup after final verification: Install/Profile apply remove only direct, strictly marked CodexHub-managed releases and captures. Update additionally adopts every uniquely laid-out, entry/version-matched release strictly below the verified new version and moves eligible releases, captures, and known residual links into a timestamped staged backup under `~/.codex-hub/deletion-backups/`. Keep current, targeted, same/newer, invalid-marker, ambiguous, or current-UID in-use versions, marking an otherwise eligible in-use old release for a later retry; all cleanup uses the shared writer lock and same-filesystem no-replace moves. Only staged Update may classify and snapshot stable current-user `sshd`, `(sd-pam)`, `sftp-server`, or `fusermount3` helpers whose executable link is unreadable; every later candidate check must match the initial UID/PID/starttime/comm/full-argv0 identity, while ManagedOnly and all unknown or changed processes still defer.
- Durable host-operation steps shared by live progress and Tasks history. Summary cards are collapsed by default, including failures, and reveal redacted command/stdout/stderr diagnostics only after explicit expansion.
- Idempotent remote PATH repair for `~/.local/bin` in `.bashrc` or `.zshrc`, `.profile`, and existing `.bash_profile` / `.zprofile` with backup-before-write and task-log evidence.
- Remote `~/.codex/config.toml` read, diff, backup, render, and replace.
- Local profile templates rendered to remote config, with create, update, delete, import, export, and single or selected-host batch apply.
- Env-var-first API key config: remote TOML uses `env_key` / `apiKeyEnvVar`; applying a profile with a stored local key writes the real value only to the selected host's `~/.codex-hub/env` with restrictive permissions and redacted logs; readiness checks verify only whether the remote env var exists.
- A per-apply confirmation for remote Codex process activation: recommended App-service reload, no reload, or acknowledged termination of all strictly confirmed Codex sessions owned by the current remote SSH UID.
- Safe remote reload through Linux `/proc` identity/start-time checks and `SIGTERM` only: wait up to five seconds for old PIDs, then observe a replacement App service for at most 15 seconds total after TERM. Failed reload preserves the applied configuration and reports manual reconnect guidance.
- `applied-profile.json` metadata and redacted Tasks logs for each profile apply.
- Local skill import and managed-copy persistence for directories containing `SKILL.md`.
- Direct GitHub repository or `tree/<branch>/<skill-path>` URL download/import for skill discovery.
- Local skill inventory detection across the local Codex skills root and remembered host `~/.codex/skills/` lists.
- Skill library table actions for preview, install, uninstall, and delete across the local machine and configured hosts.
- Operation log with backups and restore points.
- Codex App fallback wizard for host enablement and reconnect guidance.

## Explicitly Out Of Scope For MVP

- Mandatory remote Codex wrapper.
- Automatic writes to Codex App private state.
- Automatic Codex App SSH host registration through undocumented interfaces.
- Forced local ChatGPT/Codex App SSH reconnect through undocumented interfaces.
- A standalone Host action for remote Codex reload; the first version exposes it only as part of profile apply.
- Reading or writing local ChatGPT/Codex App private databases, sockets, caches, or IPC.
- Broad cleanup of unmarked releases outside the verified Update adoption policy, automatic permanent deletion of staged Update backups, or cleanup of unmanaged captures, current, targeted, same/newer, in-use, invalid-marker, or identity-ambiguous Codex objects.
- Full terminal emulator or browser-based SSH console.
- Multi-user team server, RBAC, schedules, unattended fleet automation, configurable fleet-wide concurrency, and bulk Codex install/uninstall orchestration beyond the user-triggered batch Test and Update actions.
- Storing private keys, passphrases, or tokens in plaintext.
- Writing local credential-store key names or API key values into remote Codex config.
- Default overwrite of user SSH config, Codex config, shell config, or remote dotfiles.

## MVP Safety Gates

Each mutating operation must have:

- Dry-run preview.
- Diff or clear planned action summary.
- Timestamped backup if a file already exists.
- Idempotent behavior on repeat apply.
- Restore path when possible.
- Redacted operation log.
- Explicit process-impact confirmation before remote reload; stopping all sessions requires an additional acknowledgement.
- A no-downgrade runtime check before an Update/Profile apply is accepted, plus conservative defer-on-uncertainty rules for old managed-release cleanup.

## First Milestones

1. Window 0: research, architecture docs, Tauri skeleton, smoke/mock mode.
2. Window 1: local store and SSH config parser/generator with tests.
3. Window 2: SSH connectivity probe using Windows OpenSSH and mock SSH backend.
4. Window 3: remote SSH probe and Codex status detection.
5. Window 4: single-host remote Codex CLI check/install/update with PATH repair and logs.
6. Window 5: profile/API config CRUD/import/export, remote Codex config read/diff/render/apply with backup, managed-runtime reconciliation and safe release cleanup, post-apply remote Codex reload, `applied-profile.json`, and Tasks logs.
7. Window 6: single-card local skill library, local import, GitHub URL download/import, install/uninstall target selection, backups, and task logs.
8. Window 7: Codex App fallback wizard and end-to-end mock workflow.

## Definition Of Done For Window 0

- `docs/research.md` exists and states public API findings.
- `docs/architecture.md` states direct SSH/SFTP config-management architecture.
- `docs/mvp-scope.md` states MVP/non-MVP boundaries.
- `docs/known-limitations.md` states host-add/reconnect fallback limitations.
- Tauri/React/Rust project skeleton exists.
- README stays user-facing; development startup, validation, and release-channel details live in `docs/`.
- Smoke test or mock mode can run without external services.
