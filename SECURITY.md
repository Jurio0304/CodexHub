# Security Policy

CodexHub manages local SSH configuration and remote Codex files, so security boundaries are part of the product contract.

## Supported Version

The first public release line is `0.1.x`.

## Secret Handling

CodexHub must not store these values in plaintext app files, repository files, release archives, or task logs:

- SSH private keys.
- SSH passphrases.
- One-time bootstrap passwords.
- OpenAI API keys or provider tokens.
- Remote host secrets.

The UI may display and copy public keys. Optional profile credentials are stored through the local OS credential store and are represented in profile JSON only as credential state.

## Local SSH Config Boundary

CodexHub may read `%USERPROFILE%\.ssh\config` to discover safe aliases. Writes are limited to marked CodexHub-managed blocks and must create timestamped backups when the file changes.

CodexHub must not rewrite, reorder, delete, or normalize unmanaged user SSH config blocks.

## Remote Boundary

CodexHub writes only explicit remote targets selected by the user, primarily:

- `~/.codex/config.toml`
- `~/.codex/skills/`
- `~/.codex/superpowers/skills/` for detection
- `~/.local/bin/codex`
- managed PATH blocks in `.bashrc` or `.zshrc`

Remote writes should be previewable, backed up when replacing existing files, and logged with redaction.

## Codex App Boundary

CodexHub does not write Codex App private state. Host registration remains a user-guided Codex App step through `Settings > Codex > Connections`.

## Reporting Issues

For security-sensitive reports, avoid posting secrets or live host details in public issues. Share minimal reproduction steps, sanitized logs, and the affected CodexHub version.
