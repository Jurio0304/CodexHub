# CodexHub macOS Support

Status: macOS release-build support is merged into `master` for v0.2.0. The current CI artifact is unsigned and still requires real Mac validation before broad public distribution.

CodexHub remains conservative: it writes CodexHub-managed SSH blocks to the user's SSH config, avoids Codex App private state, and keeps remote Codex work on the existing SSH/SFTP path. macOS support is buildable and mock-testable from Windows, but GUI behavior, installed app behavior, Gatekeeper, signing, and notarization require a real Mac.

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

To download the unsigned v0.2.0 release artifact:

1. Open the GitHub repository Actions tab.
2. Run or open `Build macOS Release`.
3. Wait for the `macOS unsigned release` job to finish.
4. Download the `codexhub-macos-v0.2.0-unsigned-release` artifact.
5. Extract the artifact on a real Mac and test the `.app` or `.dmg`.

The workflow uploads artifacts only. It does not create a GitHub Release, does not tag a release, and does not notarize the app. The build uses ad-hoc signing (`APPLE_SIGNING_IDENTITY=-`) until Apple Developer ID signing and notarization are configured.

## Gatekeeper Notes

This artifact is not notarized. On a real Mac, Gatekeeper may block the app on first launch. For testing, use Finder's Open action or the system Privacy & Security prompt to allow the app after you confirm the artifact came from the expected GitHub Actions run.

Do not present the artifact as signed or notarized until the signing pipeline is configured and verified.

## Codex App SSH Bridge

CodexHub writes the verified host to `~/.ssh/config`.

Then open Codex App:

```text
CodexHub has written this host to ~/.ssh/config.
Open Codex App -> Settings / Connections / SSH and add or refresh this host.
```

If Codex App supports the documented `codex://settings/connections/ssh/add?name=<alias>` deep link on the tester's Mac, it can be used as a convenience. CodexHub must still avoid undocumented Codex App files, databases, sockets, and private APIs.

## Real Mac Test Checklist

Mark each unchecked item as:

```text
Requires real macOS test
```

- Launch the `.app` from the downloaded artifact.
- Confirm the Settings platform mode defaults to `auto` and selects macOS appearance.
- Confirm Local SSH shows `~/.ssh`, `~/.ssh/config`, and `~/.ssh/id_ed25519`.
- Generate an Ed25519 key only on a disposable Mac test account with no existing key.
- Confirm existing `~/.ssh/id_ed25519` is never overwritten.
- Add a CodexHub-managed SSH host and verify `~/.ssh/config` is backed up first.
- Repeat the same host write and confirm it is idempotent.
- Confirm unmanaged `Host` blocks remain unchanged.
- Test `ssh <alias> echo ok` through CodexHub.
- Probe a Linux remote and confirm Codex CLI/version/config/skills detection.
- Confirm Local Codex CLI detection finds Homebrew, `/usr/local/bin`, `~/.local/bin`, or `which codex`.
- Open Codex App Settings / Connections / SSH and add or refresh the verified host.
- Confirm the app remains visually usable in light and dark system appearance.

## Known Limitations

- Requires real macOS test for GUI launch, Gatekeeper behavior, `.app`/`.dmg` packaging, and Codex App handoff.
- Developer ID signing and notarization are not configured.
- The macOS release workflow does not publish a GitHub Release.
- Real Mac validation remains required before treating macOS behavior as fully verified.

## Official References

- [Tauri GitHub Actions pipeline guide](https://v2.tauri.app/distribute/pipelines/github/)
- [Tauri macOS signing guide](https://v2.tauri.app/distribute/sign/macos/)
- [OpenAI Codex remote connections](https://developers.openai.com/codex/remote-connections)
- [OpenAI Codex app commands](https://developers.openai.com/codex/app/commands)
- [OpenAI Codex quickstart](https://developers.openai.com/codex/quickstart)
