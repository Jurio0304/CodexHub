# CodexHub macOS Support

Status: macOS release-build support is merged into `master`, and the real-device validation baseline was completed. The v0.4.7 macOS public artifact remains unsigned/ad-hoc.

CodexHub remains conservative: it writes CodexHub-managed SSH blocks to the user's SSH config, avoids Codex App private state, and keeps remote Codex work on the existing SSH/SFTP path. macOS support is buildable and mock-testable from Windows, with GUI behavior, installed app behavior, Gatekeeper handoff, and Codex App SSH handoff requiring a real Mac check for each public macOS artifact.

## Supported Paths

| Item | macOS path |
| --- | --- |
| SSH config | `~/.ssh/config` |
| Default SSH key | `~/.ssh/id_ed25519` |
| Codex config | `~/.codex/config.toml` |
| Codex skills | `~/.codex/skills` |

Codex CLI local detection checks these paths first:

1. `/opt/homebrew/bin/codex`
2. `/usr/local/bin/codex`
3. `~/.local/bin/codex`
4. `which codex`

Use the official Codex installer or official Codex CLI installation guidance. Do not run third-party install scripts for local Codex setup.

## GitHub Actions Release Build

The macOS release workflow is `.github/workflows/build-macos-release.yml`.

To download the unsigned macOS CI artifact:

1. Open the GitHub repository Actions tab.
2. Run or open `Build macOS Release`.
3. Wait for the `macOS unsigned release` job to finish.
4. Download the `codexhub-macos-v<version>-unsigned-release` artifact for the package version being tested.
5. Extract the artifact on a real Mac and test the `.app` or `.dmg`.

Normal push and pull-request runs upload CI artifacts only. Manual dispatch with `upload_to_release=true` may upload the unsigned Apple Silicon `.dmg`, the `.app.tar.gz` updater archive, merged `latest.json`, and `SHA256SUMS.txt` to an existing GitHub Release. The workflow does not tag a release and does not notarize the app. The build uses ad-hoc signing (`APPLE_SIGNING_IDENTITY=-`) until Apple Developer ID signing and notarization are configured.

## Gatekeeper Notes

This artifact is not notarized. On a real Mac, Gatekeeper may block the app on first launch. Use Control-click > Open, Finder's Open action, or the system Privacy & Security prompt to allow the app after you confirm the artifact came from the expected GitHub Release.

Do not present the artifact as signed or notarized until the signing pipeline is configured and verified.

Do not add unsigned/notarization warnings to the app UI. Keep that information in documentation and GitHub Release notes.

## Codex App SSH Bridge

CodexHub writes the verified host to `~/.ssh/config`.

Then open Codex App:

```text
CodexHub has written this host to ~/.ssh/config.
Open Codex App -> Settings / Connections / SSH and add or refresh this host.
```

If Codex App supports the documented `codex://settings/connections/ssh/add?name=<alias>` deep link on the tester's Mac, it can be used as a convenience. CodexHub must still avoid undocumented Codex App files, databases, sockets, and private APIs.

## Real Mac Validation Status

The real-device validation baseline was completed for the following behavior:

- Launching the `.app` from the downloaded artifact.
- Settings platform mode defaulting to `auto` and selecting macOS appearance.
- Local SSH paths for `~/.ssh`, `~/.ssh/config`, and `~/.ssh/id_ed25519`.
- Non-overwriting Ed25519 key behavior.
- CodexHub-managed SSH host writes with backups.
- Idempotent repeat writes.
- Preservation of unmanaged `Host` blocks.
- `ssh <alias> echo ok` through CodexHub.
- Linux remote probe for Codex CLI/version/config/skills detection.
- Local Codex CLI discovery through Homebrew, `/usr/local/bin`, `~/.local/bin`, or `which codex`.
- Codex App Settings / Connections / SSH handoff for a verified host.
- Menu bar/status item restore behavior.
- Close-to-hidden behavior.
- `Cmd+Q` and app menu Quit true-exit behavior.
- Light and dark system appearance.

Re-run this checklist after any macOS lifecycle, packaging, signing, SSH path, or Codex App handoff change.

## Known Limitations

- v0.4.7 remains unsigned/ad-hoc, so Gatekeeper approval may still be required on first launch.
- Developer ID signing and notarization are not configured.
- The macOS release workflow publishes to a GitHub Release only on manual dispatch with `upload_to_release=true`.
- Future macOS behavior or packaging changes must re-run the real Mac validation checklist.

## Official References

- [Tauri GitHub Actions pipeline guide](https://v2.tauri.app/distribute/pipelines/github/)
- [Tauri macOS signing guide](https://v2.tauri.app/distribute/sign/macos/)
- [OpenAI Codex remote connections](https://developers.openai.com/codex/remote-connections)
- [OpenAI Codex app commands](https://developers.openai.com/codex/app/commands)
- [OpenAI Codex quickstart](https://developers.openai.com/codex/quickstart)
