# CodexHub Release Checklist

Date: 2026-07-04
Version baseline: v0.2.8

Use this checklist before any public `stable` release. The checklist is a gate for local validation and owner acceptance only; it does not upload, tag, push, or create a GitHub Release.

## Channels

CodexHub has exactly two channels:

- `dev`: development acceptance and preview from source. It is for the project owner only and must not be published as a public artifact.
- `stable`: public release candidate only after automated validation, a signed release build, updater/feed checks where applicable, and full owner manual testing.

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

Stable validation is stricter. It must include owner acceptance, a stable Tauri release build, public leak audit, startup checking, and updater/feed checks before anything is published.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-release.ps1 -Channel stable -UserTested -NoLive
```

Run live SSH acceptance only when a sanitized test alias is explicitly provided:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\validate-release.ps1 -Channel stable -UserTested -LiveSshAlias <test-alias>
```

The stable script must fail if `-SkipTauriBuild` or missing `-UserTested` would allow a publishable build to bypass release gates. `-SkipPortable` may be used only when the approved public Windows release path is the updater-enabled setup installer rather than a portable zip.

## Live SSH Acceptance

Do not run live SSH acceptance by default. It requires an explicit sanitized test alias and must not use production secrets, personal hosts, or public logs containing private host details.

## Stable Pre-Publish Checklist

- `scripts/validate-release.ps1 -Channel stable -UserTested` completes with zero failures.
- The owner has manually tested the built app end to end.
- The summary lists the stable executable or installer, updater feed, and `SHA256SUMS.txt` artifact paths.
- If stable updater publication is enabled, the build environment injects `CODEXHUB_STABLE_UPDATE_ENDPOINT` and `CODEXHUB_STABLE_UPDATER_PUBKEY`; `TAURI_SIGNING_PRIVATE_KEY` is supplied only as a GitHub Actions secret or trusted local environment value.
- If stable updater publication is not enabled, Settings Check may be clicked but must report pending configuration; the Update action must remain disabled rather than pretending updates are available or installable.
- Failed Settings update checks open a log dialog and record a `Check app update` run that can be reopened from Tasks.
- `pnpm audit:public` passes and reports no secrets, private hosts, local app state, personal IDs, local home paths, workstation names, or build-output leaks.
- If a portable package is explicitly published in a future release, it must contain only `CodexHub.exe`, user-facing docs, license, and security notes, and it must not contain dev-only docs, release checklists, local app state, SSH config, known hosts, private keys, `.env*`, logs, databases, `dist/`, `src-tauri/target/`, or installer cache.
- `scripts/check-release-exe.ps1` starts the release executable with temporary app data and confirms it stays running through the startup window.
- Any live SSH acceptance evidence uses a sanitized test host and no production secrets or personal host names.
- No GitHub tag, upload, updater feed change, or GitHub Release is created until the owner explicitly approves publication.

## Stable Updater Publication

The updater foundation is stable-only. Windows signed updater assets are built by `.github/workflows/build-windows-release.yml` or by running `pnpm build:installer:nsis:updater` with the release environment configured. The build script generates ignored `src-tauri/tauri.updater.local.json`; do not commit it. Before publishing a feed, verify:

- `dev` builds do not auto-update and are never referenced by the stable feed.
- The updater dependency uses `native-tls` with `zip` and no default Rustls feature, so release checks follow the OS trust store on Windows/macOS.
- GitHub release-download feed URLs resolve through the GitHub Releases API asset endpoint first, with the configured feed URL kept as fallback.
- The stable feed metadata points only to owner-approved stable artifacts.
- Feed metadata includes valid Tauri signatures for the Windows stable target.
- `CODEXHUB_STABLE_UPDATER_PUBKEY` should normally be the Tauri `.key.pub` value generated by `tauri signer generate`; release scripts also accept raw minisign `.pub` text or a bare minisign public key line, but must normalize it to the base64-encoded pub-file text expected by Tauri before writing `tauri.updater.local.json`.
- `latest.json` contains the signed platform URL and signature for each approved stable target, currently `windows-x86_64` and unsigned/ad-hoc `darwin-aarch64`.
- The Windows and macOS workflows upload updater assets to an existing GitHub Release only when manually dispatched with `upload_to_release=true`.
- The Settings install button is disabled before an `available` result and uses Tauri signature verification before running the installer.
- Signing private keys and passwords are supplied only through the trusted release environment.
- Portable packaging remains manual/local for now; v0.2.8 Windows public Release keeps the updater-enabled setup installer as the only Windows app package.

## macOS Release Artifact

The macOS workflow can build unsigned `.app`, `.dmg`, and updater `.app.tar.gz` artifacts on a GitHub-hosted macOS runner. The v0.2.8 public GitHub Release includes unsigned Apple Silicon macOS assets:

```text
.github/workflows/build-macos-release.yml
```

Before treating the macOS artifact as broadly publishable, verify:

- `Build macOS Release` completes on `master`.
- The uploaded CI artifact uses the current package version, for example `codexhub-macos-v<version>-unsigned-release`.
- The artifact is clearly labeled unsigned until Developer ID signing and notarization are configured.
- Documentation and GitHub Release notes explain the unsigned/ad-hoc status and manual trust steps when needed; the app UI does not display unsigned or notarization warnings.
- The real Mac checklist in `docs/macos-support.md` is completed.
- No Apple signing certificate, private key, notarization password, token, or profile is committed to git.

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
- Settings Version info table placement below Local keys, date-time formatting, stable check behavior, failure log dialog, Tasks replay, and gated update install behavior.
- Codex App fallback instructions for `Settings > Codex > Connections`.
