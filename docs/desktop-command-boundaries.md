# Desktop Command Boundaries

Date: 2026-07-10

CodexHub selects exactly one frontend API mode at build/start time:

- `desktop`: every operation uses the Tauri command bridge. A missing bridge, IPC failure, backend error, or validation error is returned to the UI; no command falls back to Mock data.
- `mock`: selected explicitly with Vite `mode=mock`. Operations use the isolated in-memory/browser Mock API and never access desktop files, the OS credential store, SSH, or remote hosts.

`hasTauriRuntime` only validates the desktop bridge. It never selects Mock mode. Initial UI placeholders are labelled loading or unavailable and are not successful command results.

## Command Matrix

| Policy | Commands | Desktop contract | Explicit Mock contract |
| --- | --- | --- | --- |
| Authoritative read | `app_health`, `get_app_update_status`, `get_settings`, `detect_network_proxy`, `get_ssh_status`, `list_ssh_config_hosts`, `get_local_codex_status`, `list_profiles`, `preview_profile_apply`, `detect_cc_switch_profiles`, `list_local_skills`, `list_skill_packs`, `get_skill_inventory_status`, `get_skill_targets`, `list_tasks`, `query_tasks`, `get_task`, `get_storage_health`, `preview_storage_migration`, `preview_storage_restore` | Invoke Tauri and surface errors. Task queries are paginated and return persistent acknowledgement state. Storage previews are read-only, fingerprinted plans and never mutate data. | Return labelled fixtures or isolated in-memory state. Storage previews never inspect desktop files. |
| Refresh or probe | `check_stable_update`, `list_hosts`, `refresh_discovered_hosts`, `test_ssh_connection`, `ssh_check`, `remote_probe_codex`, `sample_host_resources`, `refresh_latest_codex_version`, `detect_installed_skills` | Invoke Tauri; typed `ok: false` or error states remain failures. Live SSH requires a validated non-empty alias. | Generate synthetic results and Mock tasks only. |
| Local write | `save_settings`, `choose_close_button_behavior`, `generate_ed25519_key`, `upsert_ssh_config_host`, `delete_ssh_config_host`, `add_host`, `update_host`, `delete_host`, `create_profile`, `update_profile`, `delete_profile`, `duplicate_profile`, `import_profiles`, `set_profile_api_key`, `get_profile_api_key`, `delete_profile_api_key`, `import_cc_switch_profiles`, `import_local_skill`, `update_library_skill_about`, `download_github_skill`, `acknowledge_task`, `clear_task_history`, `record_frontend_error`, `apply_storage_migration`, `restore_storage_backup` | Invoke Tauri and reject on validation, persistence, task-journal, OS recycle-bin, or IPC failure. UI state changes only after confirmed backend success. API key retrieval remains sensitive output and may perform a journaled legacy credential migration after the explicit reveal action. Task-history deletion archives complete records to a JSON file in the system recycle bin before deleting completed SQLite rows. Storage apply/restore requires an unchanged preview fingerprint and preserves a recovery backup. | Modify isolated Mock state or explicitly report unsupported behavior. Mock task clearing affects only isolated in-memory state and never claims to access the OS recycle bin. Secret input is discarded after status is recorded; storage commands never access desktop files. |
| App or remote write | `install_stable_update`, `bootstrap_ssh_host`, `bootstrap_existing_ssh_host`, `remote_manage_codex`, `apply_profile`, `install_skill_targets`, `uninstall_skill_targets`, `delete_library_skill`, `download_installed_skill`, `uninstall_installed_skill` | Require the existing preview/confirmation, backup/recovery, idempotency, alias validation, and redacted-log gates. | Produce labelled synthetic results with no local or remote mutation. |

The TypeScript `commandPolicies` registry and Rust `generate_handler!` list must remain identical. `pnpm test:api` enforces exact set equality and prevents `src/api/desktop.ts` from importing Mock or fallback implementations without relying on a brittle fixed command count.

## Settings Authority

- Desktop `settings.json` under Tauri `app_config_dir()` is authoritative.
- Desktop browser storage is a first-render cache and updates only after a successful backend read or write.
- Mock settings use a separate browser-storage key. The legacy mixed key may be read once for Mock migration and is not deleted automatically.
- Desktop writes normalize values, skip unchanged content, retain timestamped managed backups before a change, and atomically replace a validated same-directory staging file.
- A settings failure keeps the last confirmed UI values active and exposes a retry action.

## Sensitive Inputs And SSH

- One-time passwords and API keys are masked by default. Reveal controls require an explicit user action and reset to masked whenever the modal is reopened.
- Passwords and API keys may cross IPC only for the explicit operation that needs them. `get_profile_api_key` is declared as sensitive output; its result is never logged or cached. Command errors and logs never include command arguments.
- Private key contents are never read or returned; only public keys and existence/path metadata are exposed.
- Commands with direct host targets validate aliases in the frontend and again in Rust before starting SSH/SFTP. Missing or invalid aliases produce failures without launching a process.

## Durable Write Gate

Every local durable write and live/long-running operation persists queued and running task state before the adapter is called. SSH/Codex progress is appended to SQLite with redaction and retained when the final state is written. If SQLite is unavailable, the operation does not start. A final task persistence failure is returned to the UI and marks task storage unhealthy for later writes. Event emission is diagnostic-only and cannot turn a failed persistence write into success.

See [feedback and error handling](feedback-error-handling.md) and [storage migration and recovery](storage-migrations.md).
