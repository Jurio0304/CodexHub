# CodexHub Release Checklist

Date: 2026-07-02
Version baseline: v0.2.0

Use this checklist before any public `stable` release. The checklist is a gate for local validation and owner acceptance only; it does not upload, tag, push, or create a GitHub Release.

## Channels

CodexHub has exactly two channels:

- `dev`: development acceptance and preview from source. It is for the project owner only and must not be published as a public artifact.
- `stable`: public release candidate only after automated validation, a release build, portable packaging, and full owner manual testing.

Channel identities:

| Channel | productName | identifier | Window title |
| --- | --- | --- | --- |
| `stable` | `CodexHub` | `app.codexhub.desktop` | `CodexHub` |
| `dev` | `CodexHub Dev` | `dev.codexhub.desktop` | `CodexHub Dev` |

Runtime state must use Tauri app-scoped config/cache paths from `app_config_dir()` and `app_cache_dir()`. Do not hand-build paths that include a developer name, local user name, workstation name, or workspace path.

## Dev Validation

Dev validation is for local acceptance only. It may open the app from source for manual testing, but it must not build or publish release artifacts.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-release.ps1 -Channel dev -SkipTauriBuild -SkipPortable -NoLive
```

Optional owner preview from source:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-release.ps1 -Channel dev -SkipTauriBuild -SkipPortable -NoLive -OpenApp
```

## Stable Validation

Stable validation is stricter. It must include owner acceptance, a stable Tauri release build, portable packaging, public leak audit, and startup checking before anything is published.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-release.ps1 -Channel stable -UserTested -NoLive
```

Run live SSH acceptance only when a sanitized test alias is explicitly provided:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-release.ps1 -Channel stable -UserTested -LiveSshAlias <test-alias>
```

The stable script must fail if `-SkipTauriBuild`, `-SkipPortable`, or missing `-UserTested` would allow a publishable build to bypass release gates.

## Live SSH Acceptance

Do not run live SSH acceptance by default. It requires an explicit sanitized test alias and must not use production secrets, personal hosts, or public logs containing private host details.

## Stable Pre-Publish Checklist

- `scripts/validate-release.ps1 -Channel stable -UserTested` completes with zero failures.
- The owner has manually tested the built app end to end.
- The summary lists the stable executable, portable zip, and `SHA256SUMS.txt` artifact paths.
- If stable updater publication is enabled, the build environment injects `CODEXHUB_STABLE_UPDATE_ENDPOINT` and `CODEXHUB_STABLE_UPDATER_PUBKEY`; the private signing key stays outside git and outside app files.
- If stable updater publication is not enabled, Settings must show pending/disabled updater state rather than pretending updates are available or installable.
- `pnpm audit:public` passes and reports no secrets, private hosts, local app state, personal IDs, local home paths, workstation names, or build-output leaks.
- The portable zip contains `CodexHub.exe`, user-facing docs, license, and security notes only.
- The portable zip does not contain dev-only docs, release checklists, local app state, SSH config, known hosts, private keys, `.env*`, logs, databases, `dist/`, `src-tauri/target/`, or installer cache.
- `scripts/check-release-exe.ps1` starts the release executable with temporary app data and confirms it stays running through the startup window.
- Any live SSH acceptance evidence uses a sanitized test host and no production secrets or personal host names.
- No GitHub tag, upload, updater feed change, or GitHub Release is created until the owner explicitly approves publication.

## Stable Updater Publication

The updater foundation is stable-only and disabled until real signing and feed configuration exists. Before publishing a feed, verify:

- `dev` builds do not auto-update and are never referenced by the stable feed.
- The stable feed metadata points only to owner-approved stable artifacts.
- Feed metadata includes valid Tauri signatures for the Windows stable target.
- The Settings install button is disabled before an `available` result and uses Tauri signature verification before running the installer.
- Signing private keys and passwords are supplied only through the trusted release environment.
- Portable users can still download the latest stable portable zip from Releases.

## Manual Acceptance Items

The owner must test at least:

- First-run guide and settings persistence.
- Local SSH status and public-key display without exposing private-key paths.
- Add Server / bootstrap flow with one-time password handling on a safe test host.
- Managed SSH config preview/write/rollback boundaries.
- SSH alias test and remote Codex probe.
- Remote Codex install/update status and redacted task logs.
- Profile create/edit/import, API env-var selection, preview apply, and apply result.
- Skill import/download, target preview, install/uninstall, and task evidence.
- Settings Version info table placement below Local keys, date-time formatting, stable check behavior, and gated update install behavior.
- Codex App fallback instructions for `Settings > Codex > Connections`.
