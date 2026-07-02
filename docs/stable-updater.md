# CodexHub Stable Updater Foundation

Date: 2026-07-02
Version baseline: v0.2.0

This document records the internal updater foundation. Public user-facing install instructions stay in `README.md`.

## Implemented Foundation

- `stable` is the only channel eligible for Tauri updater checks.
- `dev` never auto-updates. It remains limited to local source builds, preview packages, and test artifacts.
- The Rust backend initializes `tauri-plugin-updater` and exposes `get_app_update_status`, `check_stable_update`, and `install_stable_update`.
- Settings shows a compact `Version info` table below Local keys with software name, current version, install time, latest version, and last update-check time.
- The check button is available on `stable` builds. If the feed or public key is absent, the backend returns `pending-configuration` instead of pretending a real update check ran.
- The install button is disabled unless the latest stable check returns `available`; it uses Tauri signature verification before launching the Windows installer.
- Channel, feed, and signing state remain backend status fields used for status and install gating; they are not exposed as noisy end-user rows in the compact Settings card.

## Pending Configuration

Real feed-backed stable update checks remain pending until all of these are configured outside git:

- `CODEXHUB_STABLE_UPDATE_ENDPOINT`: stable update feed URL injected at build time.
- `CODEXHUB_STABLE_UPDATER_PUBKEY`: Tauri updater public signing key injected at build time.
- `TAURI_SIGNING_PRIVATE_KEY`: private signing key available only in the trusted release environment.
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: optional release-environment password if the private key is encrypted.
- A signed feed such as `latest.json` with valid version, URL, and signature fields for the Windows stable target.

Do not commit tokens, private feed URLs, signing private keys, `.env` files, generated signatures with private context, or release credentials.

Until these values exist in the signed stable build environment, clicking Settings > Version info > Check should keep the UI honest by reporting the pending configuration state. The Update button must remain disabled.

## Publisher Flow

Before enabling a public stable update feed:

1. Complete the stable release checklist and owner acceptance.
2. Confirm the owner explicitly approves public stable availability.
3. Generate or retrieve the Tauri updater signing key pair from secure storage.
4. Inject only the public key and stable feed URL into the stable build environment.
5. Build signed stable installer/updater artifacts with the private key in environment variables only.
6. Upload the stable installer artifact, signature, and feed metadata to the approved release host.
7. Verify the Settings stable check reports either `up-to-date` or `available` against the public feed.
8. For an `available` result, verify the Settings install button downloads the signed artifact, launches the installer, and closes CodexHub for Windows to apply the update.

The release scripts still do not push, tag, upload, create a GitHub Release, or enable a feed without explicit owner approval.

## Dev And Portable Boundaries

`dev` does not use automatic updates because it represents local development, previews, and acceptance artifacts that should never become a public update source.

The existing portable package remains a manual release artifact. Portable users get new stable versions from Releases. Do not rely on portable builds for automatic install flows until a separate tested portable update story exists.
