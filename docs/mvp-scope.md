# CodexHub MVP Scope

Date: 2026-06-26

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
- Remote system and Codex probe: OS, arch, shell, PATH, `codex --version`, `~/.codex/config.toml`, `~/.codex/skills/`.
- Single-host remote Codex CLI maintenance: Test, Install Codex, and Update Codex through SSH, installing to the remote user's home directory without a wrapper; the main UI entry is the compact readiness surface on the Profiles / 配置 page, with Dashboard shortcuts allowed.
- Idempotent remote PATH repair for `~/.local/bin` in `~/.bashrc` or `~/.zshrc` with backup-before-write and task-log evidence.
- Remote `~/.codex/config.toml` read, diff, backup, render, and replace.
- Local profile templates rendered to remote config, with create, update, delete, import, export, and single or selected-host batch apply.
- Env-var-first API key config: remote TOML uses `env_key` / `apiKeyEnvVar`; optional local credential-store keys remain local and are never written to remote hosts.
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
- Forced Codex App SSH reconnect through undocumented interfaces.
- Full terminal emulator or browser-based SSH console.
- Multi-user team server, RBAC, schedules, unattended fleet automation, and broad bulk Codex install/update orchestration beyond the user-triggered outdated-host update action.
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

## First Milestones

1. Window 0: research, architecture docs, Tauri skeleton, smoke/mock mode.
2. Window 1: local store and SSH config parser/generator with tests.
3. Window 2: SSH connectivity probe using Windows OpenSSH and mock SSH backend.
4. Window 3: remote SSH probe and Codex status detection.
5. Window 4: single-host remote Codex CLI check/install/update with PATH repair and logs.
6. Window 5: profile/API config CRUD/import/export, remote Codex config read/diff/render/apply with backup, `applied-profile.json`, and Tasks logs.
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
