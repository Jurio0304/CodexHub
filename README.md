# CodexHub

CodexHub is a Windows-first desktop control plane for Codex App SSH workflows. It helps one user prepare Linux SSH hosts, install or update the remote Codex CLI, apply Codex configuration profiles, sync skills, and keep an auditable task log without writing to Codex App private state.

Chinese documentation: [简体中文 README](docs/zh-CN/README.md)

## Screenshots

【Screenshot placeholder: Dashboard host matrix with SSH, Codex, profile, skill, and task status.】

【Screenshot placeholder: Add Server flow showing one-time password bootstrap and managed SSH config write.】

【Screenshot placeholder: Profiles page with preview/apply controls and redacted task logs.】

【Screenshot placeholder: Skills page with local library, detected targets, and install/uninstall actions.】

## What CodexHub Does

* Reads local Windows OpenSSH status and public keys.
* Generates a non-overwriting Ed25519 key when no suitable key exists.
* Imports safe aliases from `%USERPROFILE%\\.ssh\\config` in read-only mode.
* Adds, updates, or deletes only CodexHub-managed SSH config blocks with timestamped backups.
* Tests SSH with `ssh <HostAlias> echo ok`.
* Probes Linux remotes for OS, architecture, shell, PATH, Codex CLI, `\~/.codex/config.toml`, and skill counts.
* Installs or updates the real remote `codex` command in the remote user's home directory.
* Manages local profile templates and applies rendered TOML to remote `\~/.codex/config.toml` with preview, diff-aware no-op behavior, backup, and redacted logs.
* Imports local or GitHub skill directories containing `SKILL.md`, detects local/remote installs, and installs or uninstalls skills through explicit target selection.
* Shows task history with redacted stdout/stderr, command status, duration, and failure evidence.
* Guides the user to Codex App `Settings > Codex > Connections` after CodexHub verifies an SSH alias.

## Safety Boundaries

CodexHub is designed to be conservative by default:

* It never stores SSH private keys, passphrases, one-time passwords, or OpenAI API keys in plaintext app files.
* It returns and copies public key text only.
* It does not edit unmanaged SSH config blocks.
* It writes only marked blocks between `# >>> CodexHub managed host: <alias>` and `# <<< CodexHub managed host: <alias>`.
* It does not write Codex App private files, databases, sockets, caches, or undocumented state.
* Remote Codex config uses `env\_key` / `apiKeyEnvVar`; local credential-store values are not written to remote hosts.
* Mutating remote operations use previews, backups, explicit apply actions, and task-log evidence.

More detail: [Security policy](SECURITY.md) and [known limitations](docs/known-limitations.md).

## Requirements

For the full Windows desktop app:

1. Windows 10/11.
2. Microsoft WebView2 Runtime.
3. Windows OpenSSH client: `ssh.exe`, `scp.exe`, and `ssh-keygen.exe`.
4. Node.js 20+ and pnpm for development builds.
5. Rust stable MSVC toolchain for Tauri builds.
6. SSH access to Linux remote hosts where Codex App will run.

Mock mode only needs Node.js.

## Install From Source

```powershell
pnpm install
```

## Run

Full desktop app:

```powershell
pnpm dev
```

Web-only Vite UI:

```powershell
pnpm dev:web
```

Dependency-light mock server:

```powershell
pnpm dev:mock
```

## Quick Start

1. Open CodexHub.
2. In Settings, check Local SSH.
3. Generate an Ed25519 key only if one does not already exist.
4. Add a server with host, user, port, and identity file.
5. Use one-time password setup when the remote does not already accept your key.
6. Confirm CodexHub wrote only its managed SSH block.
7. Test the connection.
8. Probe the remote host.
9. Install or update the remote Codex CLI.
10. Create a profile and preview/apply it to the host.
11. Import a skill and install it to local or remote targets.
12. Open Tasks to inspect redacted logs.
13. In Codex App, go to `Settings > Codex > Connections` and add or enable the verified SSH alias.

## User Tutorial

### Add A Host

* Use Hosts > Add Server for a new CodexHub-managed alias.
* Existing aliases can be imported from local SSH config without rewriting unmanaged blocks.
* New managed hosts are written only after password login, public-key install, permission repair, and key-login verification succeed.
* First-time host keys use OpenSSH `StrictHostKeyChecking=accept-new`; changed host keys still fail.

### Install Or Update Codex

* Use Profiles or Dashboard actions to run `check-version`, `install`, or `update`.
* The remote command remains `codex`; CodexHub does not install a wrapper.
* Installs target `$HOME/.local/bin` and `$HOME/.codex`.
* PATH repair is an idempotent CodexHub-managed block in `.bashrc` or `.zshrc`.
* Official installer is tried first; mirror and local-upload fallbacks are logged.

### Apply A Profile

* Profiles render to TOML.
* API keys are configured as environment variable references.
* Preview before applying.
* If the remote config already matches, CodexHub reports no changes and does not create a backup.
* If the file changes, CodexHub creates a timestamped backup and records the result in Tasks.

### Install Skills

* Import a local folder with `SKILL.md`, or import a GitHub repository/subdirectory URL.
* CodexHub stores a managed local copy in the app config directory.
* Target checks use cached inventory, so run detection before installing to a new host.
* Uninstall moves local and remote skill directories to backups instead of hard-deleting them.

## Developer Setup

Common commands:

```powershell
pnpm smoke
pnpm smoke:mock
pnpm typecheck
cargo test --manifest-path src-tauri/Cargo.toml
pnpm build:web
pnpm build:tauri
```

When the system `node` is not on `PATH`, prepend the bundled Codex runtime Node/pnpm paths before running the same commands.

## Release Checklist

Run the automated release checks:

```powershell
pnpm smoke
pnpm smoke:mock
pnpm typecheck
cargo test --manifest-path src-tauri/Cargo.toml
pnpm build:web
pnpm build:tauri
pnpm audit:public
pnpm release:portable
powershell -NoProfile -ExecutionPolicy Bypass -File .\\scripts\\check-release-exe.ps1
git diff --check
```

Then follow the live SSH checklist in [docs/release-checklist.md](docs/release-checklist.md). Live SSH acceptance requires a real host supplied by the user; mock and static checks do not prove a specific remote machine.

## Known Limitations

* CodexHub does not automatically register SSH hosts inside Codex App.
* CodexHub does not force Codex App to reconnect.
* Linux remotes are the current target; Windows remotes are not in scope.
* Full install/update depends on remote shell, `scp`, `tar`, and network or local-upload fallback behavior.
* Skill path support follows `\~/.codex/skills` and `\~/.codex/superpowers/skills`; project-level path drift remains a later capability.

See [docs/known-limitations.md](docs/known-limitations.md).

## License

MIT. See [LICENSE](LICENSE).
