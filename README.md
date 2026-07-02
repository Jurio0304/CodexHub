<div align="center">
  <img src="figs/app-logo.png" alt="CodexHub logo" width="104" height="104" />

  <h1>CodexHub</h1>

  <p><strong>Windows desktop control plane for Codex App SSH workspaces.</strong></p>
  <p>Prepare Linux hosts, install or update remote Codex, apply profiles, sync skills, and inspect redacted task logs without writing to Codex App private state.</p>

  <p>
    <a href="docs/zh-CN/README.md">简体中文</a>
    ·
    <a href="#-install">Install</a>
    ·
    <a href="docs/known-limitations.md">Known Limitations</a>
    ·
    <a href="SECURITY.md">Security</a>
  </p>

  <p>
    <img alt="Release" src="https://img.shields.io/badge/release-v0.2.0-2563eb" />
    <img alt="License" src="https://img.shields.io/badge/license-MIT-16a34a" />
    <img alt="Platform" src="https://img.shields.io/badge/platform-Windows-0078D4" />
    <img alt="Tauri" src="https://img.shields.io/badge/Tauri-2-24C8DB" />
    <img alt="React" src="https://img.shields.io/badge/React-18-61DAFB" />
    <img alt="Rust" src="https://img.shields.io/badge/Rust-MSVC-B7410E" />
  </p>
</div>

## 🧭 At a Glance

CodexHub is a Windows-first desktop control plane for one practical workflow: make a Windows machine ready to work with Codex App across SSH-connected Linux hosts.

* Manage local OpenSSH key state and CodexHub-owned SSH aliases.
* Bootstrap new Linux hosts with a one-time password, then switch to key login.
* Probe remote Codex, config, shell, PATH, and skill state before changing anything.
* Preview and apply Codex profiles and skills with backups and redacted logs.
* Hand the verified SSH alias back to Codex App through `Settings > Codex > Connections`.

## 🖼️ Screenshots

【Screenshot placeholder: Dashboard host matrix with SSH, Codex, profile, skill, and task status.】

【Screenshot placeholder: Add Server flow showing one-time password bootstrap and managed SSH config write.】

【Screenshot placeholder: Profiles page with preview/apply controls and redacted task logs.】

【Screenshot placeholder: Skills page with local library, detected targets, and install/uninstall actions.】

## ✨ Core Features

* Reads local Windows OpenSSH status and public keys.
* Generates a non-overwriting Ed25519 key when no suitable key exists.
* Imports safe aliases from `%USERPROFILE%\.ssh\config` in read-only mode.
* Adds, updates, or deletes only CodexHub-managed SSH config blocks with timestamped backups.
* Tests SSH with `ssh <HostAlias> echo ok`.
* Probes Linux remotes for OS, architecture, shell, PATH, Codex CLI, `~/.codex/config.toml`, and skill counts.
* Installs or updates the real remote `codex` command in the remote user's home directory.
* Manages local profile templates and applies rendered TOML to remote `~/.codex/config.toml`.
* Imports local or GitHub skill directories containing `SKILL.md`.
* Shows task history with redacted stdout/stderr, command status, duration, and failure evidence.
* Guides the user to Codex App after CodexHub verifies an SSH alias.

## 🔐 Safety Boundaries

CodexHub is designed to be conservative by default:

* It never stores SSH private keys, passphrases, one-time passwords, or OpenAI API keys in plaintext app files.
* It returns and copies public key text only.
* It does not edit unmanaged SSH config blocks.
* It writes only marked blocks between `# >>> CodexHub managed host: <alias>` and `# <<< CodexHub managed host: <alias>`.
* It does not write Codex App private files, databases, sockets, caches, or undocumented state.
* Remote Codex config uses `env_key` / `apiKeyEnvVar`; local credential-store values are not written to remote hosts.
* Mutating remote operations use previews, backups, explicit apply actions, and task-log evidence.

More detail: [Security policy](SECURITY.md) and [known limitations](docs/known-limitations.md).

## ✅ Requirements

For the full Windows desktop app:

1. Windows 10/11.
2. Microsoft WebView2 Runtime.
3. Windows OpenSSH client: `ssh.exe`, `scp.exe`, and `ssh-keygen.exe`.
4. SSH access to Linux remote hosts where Codex App will run.

For building from source:

1. Node.js 20+ and pnpm.
2. Rust stable MSVC toolchain.

## 🚀 Install

For everyday use, download the latest Windows build from this repository's Releases page.

* Installer: download and run the Windows x64 setup `.exe`.
* Portable: unzip the Windows x64 portable `.zip`, then run `CodexHub.exe`.

GitHub Releases should host binaries. Source builds are an advanced option:

```powershell
pnpm install
pnpm release:portable
```

## ⚡ Quick Start

1. Open CodexHub.
2. In Settings, check Local SSH.
3. Generate an Ed25519 key only if one does not already exist.
4. Add a server with host, user, port, and identity file.
5. Use one-time password setup when the remote does not already accept your key.
6. Test the SSH alias and probe the remote host.
7. Install or update remote Codex.
8. Create a profile, preview it, then apply it to the host.
9. Import a skill and install it to local or remote targets.
10. Open Tasks to inspect redacted logs.
11. In Codex App, go to `Settings > Codex > Connections` and add or enable the verified SSH alias.

## 📘 Guided Workflows

### Add a Host

* Use Hosts > Add Server for a new CodexHub-managed alias.
* Existing aliases can be imported from local SSH config without rewriting unmanaged blocks.
* New managed hosts are written only after password login, public-key install, permission repair, and key-login verification succeed.
* First-time host keys use OpenSSH `StrictHostKeyChecking=accept-new`; changed host keys still fail.

### Install or Update Codex

* Use Profiles or Dashboard actions to run `check-version`, `install`, or `update`.
* The remote command remains `codex`; CodexHub does not install a wrapper.
* Installs target `$HOME/.local/bin` and `$HOME/.codex`.
* PATH repair is an idempotent CodexHub-managed block in `.bashrc` or `.zshrc`.
* Official installer is tried first; mirror and local-upload fallbacks are logged.

### Apply a Profile

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

## ⚠️ Known Limitations

* CodexHub does not automatically register SSH hosts inside Codex App.
* CodexHub does not force Codex App to reconnect.
* Linux remotes are the current target; Windows remotes are not in scope.
* Full install/update depends on remote shell, `scp`, `tar`, and network or local-upload fallback behavior.
* Skill path support follows `~/.codex/skills` and `~/.codex/superpowers/skills`; project-level path drift remains a later capability.

See [docs/known-limitations.md](docs/known-limitations.md).

## 📄 License

MIT. See [LICENSE](LICENSE).
