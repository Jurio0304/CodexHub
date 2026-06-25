# CodexHub Research Notes

Date: 2026-06-25  
Phase: Window 0 - public research and architecture baseline  
Scope: Codex App SSH remote workflow, Codex CLI/config/skills, Windows OpenSSH, Tauri 2 desktop baseline, and SSH-management UI references.

## Executive Findings

- Codex App remote development has a public SSH path: OpenAI documents "Remote connections" for Linux remote machines and says the app leverages the Remote-SSH protocol.
- The documented remote bootstrap flow is user-mediated: install/runnable Codex CLI on the remote, then add or enable the SSH host in Codex App settings under Codex > Connections.
- No public, stable API was found that lets an external desktop app silently add an SSH host to Codex App or force an existing Codex App SSH connection to reconnect. CodexHub MVP must not write Codex App private state.
- Codex configuration is TOML-based and centered on `~/.codex/config.toml`; profile switching can be implemented by rendering/replacing remote config rather than requiring a remote wrapper.
- Skill-path documentation appears version-sensitive: the skills guide documents `.agents/skills`, while the app-server page still references user-level `~/.codex/skills`. Per product requirement, CodexHub MVP will target remote `~/.codex/skills/`, but the path must be a setting and the backend should detect both paths before later hardening.
- Windows support should rely on Windows OpenSSH (`ssh.exe`, `scp.exe`, `sftp.exe`) and `%USERPROFILE%\.ssh\config`; CodexHub should never overwrite SSH config without preview, backup, and idempotent append/update logic.

## Sources Reviewed

- OpenAI Codex Remote Connections: https://developers.openai.com/codex/remote-connections
- OpenAI Codex Quickstart / CLI: https://developers.openai.com/codex/quickstart and https://developers.openai.com/codex/cli
- OpenAI Codex Windows setup: https://developers.openai.com/codex/windows
- OpenAI Codex config reference: https://developers.openai.com/codex/config-reference
- OpenAI Codex skills guide: https://developers.openai.com/codex/skills
- OpenAI Codex app-server guide: https://developers.openai.com/codex/app-server
- Microsoft OpenSSH install and first use: https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh_install_firstuse
- Microsoft OpenSSH key management: https://learn.microsoft.com/en-us/windows-server/administration/openssh/openssh_keymanagement
- OpenBSD `ssh_config` reference: https://man.openbsd.org/ssh_config
- Tauri 2 create-project and prerequisites: https://v2.tauri.app/start/create-project/ and https://v2.tauri.app/start/prerequisites/
- Semaphore UI repository: https://github.com/semaphoreui/semaphore
- Web-based SSH background pattern: https://en.wikipedia.org/wiki/Web-based_SSH

## Codex App SSH Remote Mechanism

OpenAI's public docs describe Codex App remote environments as SSH-based remote connections for Linux hosts. The important constraints for CodexHub are:

- Remote host must be reachable by SSH and must provide a POSIX-compatible shell.
- Remote home directory must be writable.
- `scp` support is part of the documented requirement, which implies file transfer is a supported primitive for setup and server bootstrap.
- Codex CLI must be installed and runnable on the remote before first use; the documented command is `npm install -g @openai/codex`.
- Codex App setup is intentionally user-facing: the user adds or enables a host in Settings > Codex > Connections, then the app installs and launches its own remote server before the first task.

CodexHub architectural implication: use public SSH/SFTP operations for remote config and skill management, and surface a clear "Open Codex App settings / copy host name / verify command" fallback instead of editing Codex App internals.

## Codex CLI Installation

The public installation baseline is:

```bash
npm install -g @openai/codex
codex
```

For remote hosts, CodexHub should verify the installation with a non-invasive command such as:

```bash
codex --version
```

CodexHub should not install Codex CLI automatically in MVP unless the user explicitly runs an assisted command, because remote package manager state and permissions are host-specific.

## Codex Config, Profiles, And Skills

### Config

Codex public docs use `~/.codex/config.toml` as the persistent configuration file. The config reference documents a TOML file with top-level options and named profile tables. A representative profile pattern is:

```toml
model = "gpt-5-codex"
approval_policy = "on-request"

[profiles.safe]
approval_policy = "on-request"
sandbox_mode = "workspace-write"

[profiles.autonomous]
approval_policy = "never"
sandbox_mode = "workspace-write"
```

CodexHub MVP should read, render, preview, back up, and atomically replace the remote `~/.codex/config.toml`. It should not depend on a remote wrapper to switch profiles.

### Profile Switching

Profile switching can be modeled as a rendered remote config state:

1. User selects a managed profile in CodexHub.
2. CodexHub renders the desired TOML from structured local data.
3. CodexHub shows a diff and writes the resulting `~/.codex/config.toml` through SSH/SFTP after backup.
4. User starts or restarts Codex sessions through the documented Codex App workflow.

This does not require `codex --profile` wrapper invocation in MVP. A wrapper can be added later for hosts where the user wants runtime profile selection without replacing the default config.

### Skills

The current public Codex skills guide documents project skills under `.agents/skills/<skill-name>/SKILL.md` and personal skills under `$HOME/.agents/skills/<skill-name>/SKILL.md`. The app-server guide still includes user-level `~/.codex/skills` in its skill-loading example.

MVP product decision: CodexHub manages remote `~/.codex/skills/` directly, because this is an explicit requirement for first release. To avoid lock-in to a path that may drift, the implementation must keep the remote skill root configurable and should detect existing `$HOME/.agents/skills` and `$HOME/.codex/skills` directories before future write-policy decisions.

## Windows OpenSSH Notes

CodexHub is Windows-first, so SSH behavior must be predictable on a standard Windows machine:

- Prefer the Windows OpenSSH client (`ssh.exe`, `scp.exe`, `sftp.exe`) instead of bundling a custom SSH stack in MVP.
- Default user SSH config path is `%USERPROFILE%\.ssh\config`.
- Key generation should prefer Ed25519 where supported:

```powershell
ssh-keygen -t ed25519 -C "codexhub"
```

- CodexHub may offer an SSH host-stanza generator, but the default action should be "copy to clipboard" or "append with backup". It must not overwrite the whole config.
- Edits must be idempotent: mark CodexHub-managed blocks, update only those blocks, preserve user comments and unrelated hosts, and create timestamped backups.

## Public API Check: Codex App Host Add / Reconnect

Research result for 2026-06-25:

- Public docs describe a settings-driven flow to add or enable SSH hosts.
- No public stable URL scheme, command-line API, IPC API, local socket API, or file contract was found for programmatically adding a host to Codex App.
- No public stable API was found for forcing Codex App to reconnect to a remote SSH host.

MVP rule: CodexHub must implement safe UI fallback only:

- Show detected SSH host aliases and exact steps to enable them in Codex App.
- Provide copy buttons for `ssh <alias>`, `codex --version`, and expected remote paths.
- Show connection diagnostics from CodexHub's own SSH check.
- Avoid writing private Codex App state, caches, or databases.

## UI / Implementation Reference Projects

The named comparison set is useful mainly for product patterns rather than direct integration:

- Semaphore UI: server inventory, task templates, schedules, credentials/variable groups, and task logs. CodexHub should borrow the separation of inventory, reusable operation templates, and execution history, while avoiding Semaphore's heavier multi-user automation surface in MVP.
- Web-based SSH tools: typical implementation is browser terminal plus WebSocket/proxy to SSH. CodexHub intentionally avoids this in MVP; it only needs targeted SSH/SFTP operations and diagnostics, not a full terminal emulator.
- VibeShell, R-Shell, Janus, Termix, and mcp-ssh-manager: use as visual and workflow references for host lists, connection status, file/config editors, and command palettes. Before copying any concrete implementation, perform a source-level license and architecture review for the specific repository selected.

## Research Decisions For MVP

- Remote wrapper is not required.
- Direct SSH/SFTP config management is required.
- Codex App automation is fallback-only unless OpenAI publishes an official API.
- The backend should start with OpenSSH process orchestration, not a native SSH library, to keep Windows MVP simple and debuggable.
- Local persistence should be SQLite or JSON/TOML. For the skeleton, use typed JSON-compatible models; add SQLite when CRUD is implemented.
