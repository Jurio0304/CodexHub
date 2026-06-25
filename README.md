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

Window 2 adds local Windows SSH initialization and SSH config management on top of the first desktop UI shell:

- macOS-style sidebar navigation for Dashboard, Hosts, Profiles, Skills, Tasks, and Settings.
- Dashboard server matrix with mock host data, empty-state handling, an Add Server entry, and recent task cards.
- Light/dark mode support with native-feeling cards, tables, and status badges.
- Settings / Appearance includes a three-button theme control plus English / 简体中文 global font and language presets.
- Settings / Local SSH detects Ed25519 and RSA keys, can generate a non-overwriting Ed25519 key, and shows/copies public keys only.
- Hosts can add, update, and delete CodexHub-managed blocks in `%USERPROFILE%\.ssh\config` with timestamped backups and unmanaged block preservation.
- Mock Tauri commands remain for profile, skill-pack, task, and connection-test data until remote SSH/SFTP is connected.

## Prerequisites For Full Desktop Dev

Install these on Windows before running the full Tauri app:

1. Node.js 20+ and pnpm.
2. Rust stable MSVC toolchain.
3. Microsoft WebView2 runtime.
4. Windows OpenSSH client (`ssh.exe`, `scp.exe`, `sftp.exe`).
5. Codex CLI on each remote host where Codex App will run:

```bash
npm install -g @openai/codex
codex --version
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
- `add_host`
- `update_host`
- `delete_host`
- `test_ssh_connection`
- `list_profiles`
- `apply_profile`
- `list_tasks`

The Skills page is also backed by a mock `list_skill_packs` helper command so the first UI shell can render all sidebar sections.

The SSH key and SSH config commands are real Windows local filesystem operations. They never read private key contents; public key text is the only key material returned to the UI.

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
