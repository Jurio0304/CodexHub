# CodexHub Release Channels

Date: 2026-07-04
Version baseline: v0.2.6

CodexHub uses exactly two release channels: `dev` and `stable`.

## Channel Contract

| Channel | Purpose | Tauri config | productName | identifier | Window title |
| --- | --- | --- | --- | --- | --- |
| `stable` | Public release candidate only after the requested validation passes and the user explicitly approves public availability. | `src-tauri/tauri.conf.json` | `CodexHub` | `app.codexhub.desktop` | `CodexHub` |
| `dev` | Development, test runs, previews, and manual acceptance before promotion. | `src-tauri/tauri.dev.conf.json` | `CodexHub Dev` | `dev.codexhub.desktop` | `CodexHub Dev` |

Do not add extra channels such as alpha, beta, nightly, staging, rc, or preview. Preview and manual acceptance builds belong to `dev`.

## Local Data Isolation

Runtime app state must use Tauri's app-scoped paths:

- Config state uses `app.path().app_config_dir()`.
- Cache state uses `app.path().app_cache_dir()`.
- Tauri resolves those paths under the OS config/cache roots plus the bundle identifier.

On Windows, this means:

| Channel | Config directory | Cache directory |
| --- | --- | --- |
| `stable` | `%APPDATA%\app.codexhub.desktop` | `%LOCALAPPDATA%\app.codexhub.desktop` |
| `dev` | `%APPDATA%\dev.codexhub.desktop` | `%LOCALAPPDATA%\dev.codexhub.desktop` |

The persisted files under those directories include `settings.json`, `hosts.json`, `profiles.json`, `skills.json`, `skills-inventory.json`, `codex-latest.json`, managed skill copies, profile-apply temp files, and cloned skill cache.

## What Is Not Isolated Automatically

Channel isolation covers the local Tauri app identity and app-owned config/cache directories only. It does not automatically isolate:

- `%USERPROFILE%\.ssh\config`
- `%USERPROFILE%\.ssh\known_hosts`
- Local SSH key files
- Remote `~/.codex/config.toml`
- Remote `~/.codex/skills/`
- Remote shell files such as `.bashrc` or `.zshrc`

Any operation touching those shared local or remote surfaces must keep the existing CodexHub safety rules: explicit user action, scoped writes, backups before mutation, idempotent behavior, and redacted task logs.

## Build Entry Points

Stable is the default package identity:

```powershell
pnpm build:tauri
pnpm build:installer:nsis
pnpm build:installer:msi
pnpm build:installer:nsis:updater
```

Dev uses the dev Tauri override:

```powershell
pnpm dev
pnpm build:tauri:dev
pnpm build:installer:nsis:dev
pnpm build:installer:msi:dev
pnpm release:portable:dev
```

Do not create a GitHub Release from the `dev` channel. Do not create any GitHub Release until the user explicitly approves publishing the `stable` build.

## Stable Updater Boundary

Only `stable` may use the Tauri updater. The current foundation wires the updater plugin, Settings status/check UI, a gated install action, update-check Task history, a Windows signed-updater release workflow, and an unsigned/ad-hoc Apple Silicon macOS updater release path. The stable Check button remains clickable so formal builds can report their updater state; without both `CODEXHUB_STABLE_UPDATE_ENDPOINT` and `CODEXHUB_STABLE_UPDATER_PUBKEY`, the backend returns `pending-configuration`. Failed checks open a log dialog and remain reviewable from Tasks. Install stays disabled until a signed stable feed returns `available`.

`dev` must not auto-update from any feed. Dev builds are local source runs, preview packages, or test artifacts only.

`pnpm release:portable` remains available for manual/local packaging, but the v0.2.6 Windows public Release keeps only the signed updater-enabled setup installer. macOS uses an unsigned/ad-hoc Apple Silicon `.dmg` for user installation and `.app.tar.gz` for updater delivery. Portable builds do not participate in automatic update feeds unless a separate portable update story is designed and tested.

See [stable updater details](stable-updater.md).
