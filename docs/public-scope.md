# Public Repository Scope

CodexHub's public repository is source-only. Binaries and local runtime state belong in ignored build folders or GitHub Releases, not in Git history.

## Commit

These files are expected public source:

- `README.md`
- `LICENSE`
- `SECURITY.md`
- `package.json`, lockfiles, and TypeScript/Vite config.
- `src/`
- `src-tauri/src/`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, icons, capabilities, and Tauri config.
- `scripts/` release, audit, smoke, and dev helpers.
- `docs/` architecture, scope, limitations, release, and localized docs.
- `.agents/` project-local maintainer guidance, if the project owner wants agents to reuse it.

## Do Not Commit

Never commit:

- `dist/`
- `src-tauri/target/`
- `release-artifacts/`
- `dist-release/`
- `node_modules/`
- `.pnpm-store/`
- `.toolchains/`
- `.env` or `.env.*`
- `hosts.json`, `profiles.json`, `tasks.json`, `settings.json`, `skill-inventory.json`, or `codex-latest.json`
- SSH config exports, `known_hosts`, private keys, passphrases, or tokens
- SQLite databases or local app state exports
- Logs, PID files, or temporary package staging folders

## Generated Files To Clean Locally

Before publishing, delete or leave ignored:

```powershell
Remove-Item -Recurse -Force .\release-artifacts -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force .\dist -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force .\src-tauri\target -ErrorAction SilentlyContinue
```

Only run cleanup inside the repository root. Do not delete files from `%USERPROFILE%\.ssh` or app config directories as part of repo cleanup.

## Audit

Run:

```powershell
pnpm audit:public
```

The audit scans tracked and publish-candidate files plus any staged release artifacts for high-confidence leaks and forbidden paths. It is a release gate, not a replacement for human review.
