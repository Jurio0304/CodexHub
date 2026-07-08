# CodexHub Linux Desktop Support

Status: Ubuntu/Debian x86_64 and arm64 release-build support is implemented as `.deb` packaging. Linux uses the macOS-style appearance when Settings > Platform is `Auto`.

CodexHub on Linux keeps the same safety contract as Windows and macOS: it reads local OpenSSH state, writes only CodexHub-managed SSH blocks after explicit action, avoids Codex App private state, and manages remote Linux hosts through the existing SSH/SFTP path.

## Supported Scope

| Item | Linux path |
| --- | --- |
| App config | Tauri `app_config_dir()` under the stable/dev bundle identifier |
| SSH config | `~/.ssh/config` |
| Default SSH key | `~/.ssh/id_ed25519` |
| Local Codex config | `~/.codex/config.toml` |
| Local Codex skills | `~/.codex/skills` |
| Local Codex binary candidates | `~/.local/bin/codex`, `~/.npm-global/bin/codex`, then `command -v codex` |

Initial Linux desktop support is limited to Ubuntu/Debian x86_64 and arm64 `.deb` packages. rpm, AppImage, Snap, and Flatpak packages are not part of this release path.

## Release Artifacts

The Linux release workflow is `.github/workflows/build-linux-release.yml`.

Normal push and pull-request runs upload CI artifacts only. Manual dispatch with `upload_to_release=true` may upload:

- `CodexHub_<version>_amd64.deb`
- `CodexHub_<version>_arm64.deb`
- merged `SHA256SUMS.txt`

Linux `.deb` packages are for manual install or manual upgrade and are not used by the Tauri updater feed. Linux is not added to `latest.json` until a lighter signed updater story is designed and tested.

## Validation Checklist

For each public Linux desktop artifact, verify on a real Ubuntu/Debian desktop matching the package architecture:

- `.deb` installs and launches from the desktop environment.
- Ubuntu/Debian x86_64 installs `CodexHub_<version>_amd64.deb`.
- Ubuntu/Debian arm64 installs `CodexHub_<version>_arm64.deb`.
- Settings defaults to the macOS-style appearance when Platform is `Auto`, and the Windows style can still be selected manually.
- Local SSH status, public-key display, SSH config import, and managed-host writes use `~/.ssh/config`.
- Host SSH test and remote Codex probe work against a safe Linux host.
- Profiles credential storage works through the Linux credential-store backend.
- Skills import, install, download, and uninstall keep task-log evidence and redaction.
- Monitor page refreshes remembered Linux hosts without background polling when inactive.
- Close-button behavior, tray/status item restore, and Quit behavior match the current desktop lifecycle contract.
- Settings update check remains honest: Linux `.deb` packages do not participate in the updater feed yet.

## Build Dependencies

Linux CI installs the Tauri/Linux desktop dependencies used by the release workflow, including WebKitGTK 4.1, GTK/AppIndicator, xdo, OpenSSL, DBus/keyutils, librsvg, patchelf, and pkg-config development packages.

Re-run this checklist after any Linux lifecycle, packaging, updater, credential-store, SSH path, or Codex App handoff change.
