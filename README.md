# CodexHub

CodexHub is a Windows-first desktop control plane for managing Codex App SSH multi-server workflows.

MVP direction:

- Tauri 2 desktop app.
- React + TypeScript + Vite frontend.
- Rust backend through Tauri commands.
- Direct SSH/SFTP management of remote `~/.codex/config.toml` and `~/.codex/skills/`.
- No mandatory remote Codex wrapper in MVP.
- No writes to private Codex App state.

## Current Status

Window 5 adds Profile/API config management on top of SSH host discovery, OpenSSH checks, remote system probes, and single-host remote Codex CLI maintenance:

- macOS-style sidebar navigation for Dashboard, Hosts, Profiles, Skills, Tasks, and Settings.
- Dashboard server matrix with host-level Check Version, Install Codex, and Update Codex quick actions; batch install/update is a UI placeholder only.
- Light/dark mode support with native-feeling cards, tables, and status badges.
- Settings / Appearance includes a three-button theme control plus English / 简体中文 global font and language presets.
- Settings / Local SSH detects Ed25519 and RSA keys, can generate a non-overwriting Ed25519 key, and shows/copies public keys only.
- Hosts auto-detect safe aliases from `%USERPROFILE%\.ssh\config` in read-only mode and import them into the in-memory inventory.
- Hosts can still add, update, and delete only CodexHub-managed blocks in `%USERPROFILE%\.ssh\config` with timestamped backups; unmanaged user blocks are never modified or overwritten.
- Real desktop commands can run `ssh <HostAlias> echo ok` with timeout control and probe remote Linux hosts for OS, arch, shell, PATH, Codex CLI, config presence, and skills count.
- Profiles / 配置 now manages local Codex profiles with create, update, delete, import, and export flows, plus a compact host-apply surface for single-host or selected-host batch apply.
- API keys are env-var-first: profiles render remote config with `env_key` / `apiKeyEnvVar` references, and a remembered local credential-store key is optional local metadata only.
- Stored local credential keys are never written to remote hosts. Remote `~/.codex/config.toml` writes contain the environment variable reference, not the local credential key or API key value.
- Profile apply reads and diffs the remote config, creates a timestamped backup when needed, writes the rendered config, records `applied-profile.json` metadata, and emits redacted Tasks logs.
- Profiles / 配置 still keeps the all-host Codex readiness list for single-host `codex --version` checks plus user-directory install/update flows. Host pages remain focused on connection details and diagnostics. The remote command remains the real `codex`; CodexHub does not install a wrapper.
- Remote Codex install/update creates `~/.local/bin`, repairs shell PATH through an idempotent CodexHub-managed block in `~/.bashrc` or `~/.zshrc` with backup-before-write, runs the official standalone installer first, falls back to a npmmirror native package install, can locally download and `scp` that native package when the remote network is blocked, then falls back to npm with `--prefix "$HOME/.local"`.
- Probe and Codex maintenance commands run through the backend blocking worker pool so the desktop window stays responsive; install/update opens a compact progress modal backed by `remote-codex-progress` events with streaming stdout/stderr lines and heartbeat messages.
- Task logs now capture each SSH/probe/install command with redacted stdout/stderr, exit code, duration, and timeout state.
- Skill sync remains reserved for a later window; profile apply is now the implemented remote config write path.

## Prerequisites For Full Desktop Dev

Install these on Windows before running the full Tauri app:

1. Node.js 20+ and pnpm.
2. Rust stable MSVC toolchain.
3. Microsoft WebView2 runtime.
4. Windows OpenSSH client (`ssh.exe`, `scp.exe`, `sftp.exe`).
5. SSH access to each Linux remote host where Codex App will run. CodexHub can install or update the remote Codex CLI without root:

```bash
ssh <HostAlias> echo ok
```

## Install

```bash
pnpm install
```

## Run The Tauri App

```bash
pnpm dev
```

This runs Vite and starts the Tauri desktop window. Equivalent commands:

```bash
pnpm tauri dev
npm run dev
npm run tauri -- dev
```

The desktop app currently exposes these Rust commands:

- `get_ssh_status`
- `generate_ed25519_key`
- `list_ssh_config_hosts`
- `upsert_ssh_config_host`
- `delete_ssh_config_host`
- `list_hosts`
- `refresh_discovered_hosts`
- `add_host`
- `update_host`
- `delete_host`
- `test_ssh_connection`
- `ssh_check`
- `bootstrap_ssh_host`
- `bootstrap_existing_ssh_host`
- `remote_probe_codex`
- `remote_manage_codex`
- `list_profiles`
- `create_profile`
- `update_profile`
- `delete_profile`
- `duplicate_profile`
- `import_profiles`
- `export_profiles`
- `set_profile_api_key`
- `delete_profile_api_key`
- `preview_profile_apply`
- `apply_profile`
- `detect_cc_switch_profiles`
- `import_cc_switch_profiles`
- `list_tasks`

The Skills page is also backed by a mock `list_skill_packs` helper command so the first UI shell can render all sidebar sections.

The SSH key and SSH config commands are real Windows local filesystem operations. They never read private key contents; public key text is the only key material returned to the UI. New CodexHub-managed hosts use a one-time password connection flow: log in to the remote host, install the local public key into `~/.ssh/authorized_keys`, set SSH permissions, write only a managed SSH config block, and verify with `ssh <HostAlias> echo ok`. The password is not stored or written to task logs.

## Settings Persistence

Appearance settings are applied immediately. In the desktop app, `get_settings` and `save_settings` persist the selected theme and English / 简体中文 font-language preset to the Tauri app config directory. In web-only mode, the same values fall back to `localStorage` under `codexhub.settings.v1`; this fallback can be migrated to a shared backend settings store later.

## Web-Only Dev

```bash
pnpm dev:web
```

## Mock Mode

Use this when the full Tauri/Rust toolchain is not installed yet:

```bash
pnpm dev:mock
```

Then open the URL printed by the script.

## Smoke Test

The smoke test validates the docs and skeleton without requiring downloaded npm dependencies or Rust compilation:

```bash
pnpm smoke
```

If pnpm is not available yet, run the dependency-light smoke test directly:

```bash
node scripts/smoke-test.mjs
```

## Documentation

- `docs/research.md` - public research notes and source links.
- `docs/architecture.md` - MVP architecture and write-safety model.
- `docs/mvp-scope.md` - what is in/out of the first version.
- `docs/known-limitations.md` - current integration limitations and fallbacks.
