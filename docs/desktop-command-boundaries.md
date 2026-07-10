# Desktop Command Boundaries

Date: 2026-07-10

CodexHub selects exactly one frontend API mode at build/start time:

- `desktop`: every operation uses the Tauri command bridge. A missing bridge, IPC failure, backend error, or validation error is returned to the UI; no command falls back to Mock data.
- `mock`: selected explicitly with Vite `mode=mock`. Operations use the isolated in-memory/browser Mock API and never access desktop files, the OS credential store, SSH, or remote hosts.

`hasTauriRuntime` only validates the desktop bridge. It never selects Mock mode. Initial UI placeholders are labelled loading or unavailable and are not successful command results.

## Command Matrix

| Policy | Commands | Desktop contract | Explicit Mock contract |
| --- | --- | --- | --- |
| Authoritative read | `app_health`, `get_app_update_status`, `get_settings`, `detect_network_proxy`, `get_ssh_status`, `list_ssh_config_hosts`, `get_local_codex_status`, `list_profiles`, `preview_profile_apply`, `detect_cc_switch_profiles`, `list_local_skills`, `list_skill_packs`, `get_skill_inventory_status`, `get_skill_targets`, `list_tasks`, `get_profile_api_key` | Invoke Tauri and surface errors. Stored API key retrieval occurs only after the user activates the reveal control; the result is sensitive output and remains transient UI state. | Return labelled fixtures. API key reveal returns a synthetic Mock value and never reads or retains a real credential. |
| Refresh or probe | `check_stable_update`, `list_hosts`, `refresh_discovered_hosts`, `test_ssh_connection`, `ssh_check`, `remote_probe_codex`, `sample_host_resources`, `refresh_latest_codex_version`, `detect_installed_skills` | Invoke Tauri; typed `ok: false` or error states remain failures. Live SSH requires a validated non-empty alias. | Generate synthetic results and Mock tasks only. |
| Local write | `save_settings`, `choose_close_button_behavior`, `generate_ed25519_key`, `upsert_ssh_config_host`, `delete_ssh_config_host`, `add_host`, `update_host`, `delete_host`, `create_profile`, `update_profile`, `delete_profile`, `duplicate_profile`, `import_profiles`, `set_profile_api_key`, `delete_profile_api_key`, `import_cc_switch_profiles`, `import_local_skill`, `update_library_skill_about`, `download_github_skill` | Invoke Tauri and reject on validation, persistence, or IPC failure. UI state changes only after confirmed backend success. | Modify isolated Mock state or explicitly report unsupported behavior. Secret input is discarded after status is recorded. |
| App or remote write | `install_stable_update`, `bootstrap_ssh_host`, `bootstrap_existing_ssh_host`, `remote_manage_codex`, `apply_profile`, `install_skill_targets`, `uninstall_skill_targets`, `delete_library_skill`, `download_installed_skill`, `uninstall_installed_skill` | Require the existing preview/confirmation, backup/recovery, idempotency, alias validation, and redacted-log gates. | Produce labelled synthetic results with no local or remote mutation. |

The TypeScript `commandPolicies` registry and Rust `generate_handler!` list must remain identical. `pnpm test:api` enforces the 54-command contract and prevents `src/api/desktop.ts` from importing Mock or fallback implementations.

## Settings Authority

- Desktop `settings.json` under Tauri `app_config_dir()` is authoritative.
- Desktop browser storage is a first-render cache and updates only after a successful backend read or write.
- Mock settings use a separate browser-storage key. The legacy mixed key may be read once for Mock migration and is not deleted automatically.
- Desktop writes normalize values, skip unchanged content, retain `settings.json.bak` before a change, and replace the file through a same-directory temporary file.
- A settings failure keeps the last confirmed UI values active and exposes a retry action.

## Sensitive Inputs And SSH

- One-time passwords and API keys are masked by default. Reveal controls require an explicit user action and reset to masked whenever the modal is reopened.
- Passwords and API keys may cross IPC only for the explicit operation that needs them. `get_profile_api_key` is declared as sensitive output; its result is never logged or cached. Command errors and logs never include command arguments.
- Private key contents are never read or returned; only public keys and existence/path metadata are exposed.
- Commands with direct host targets validate aliases in the frontend and again in Rust before starting SSH/SFTP. Missing or invalid aliases produce failures without launching a process.
