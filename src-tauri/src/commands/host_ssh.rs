use crate::*;

#[tauri::command]
pub(crate) async fn get_ssh_status() -> Result<ssh::SshStatus, String> {
    services::host_use_cases::execute_get_ssh_status().await
}

#[tauri::command]
pub(crate) fn generate_ed25519_key(app: AppHandle) -> Result<ssh::SshKeyGenerationResult, String> {
    services::host_use_cases::execute_generate_ed25519_key(app)
}

#[tauri::command]
pub(crate) async fn list_ssh_config_hosts() -> Result<Vec<ssh::SshConfigHost>, String> {
    services::host_use_cases::execute_list_ssh_config_hosts().await
}

#[tauri::command]
pub(crate) fn upsert_ssh_config_host(
    app: AppHandle,
    draft: ssh::SshHostDraft,
) -> Result<ssh::SshConfigWriteResult, String> {
    services::host_use_cases::execute_upsert_ssh_config_host(app, draft)
}

#[tauri::command]
pub(crate) fn delete_ssh_config_host(
    app: AppHandle,
    alias: String,
    state: State<'_, AppState>,
) -> Result<SshConfigDeleteResult, String> {
    services::host_use_cases::execute_delete_ssh_config_host(app, alias, &state)
}

#[tauri::command]
pub(crate) fn list_hosts(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<Host>, String> {
    services::host_use_cases::execute_list_hosts(app, &state)
}

#[tauri::command]
pub(crate) fn refresh_discovered_hosts(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<Host>, String> {
    services::host_use_cases::execute_refresh_discovered_hosts(app, &state)
}

#[tauri::command]
pub(crate) fn add_host(
    app: AppHandle,
    state: State<'_, AppState>,
    draft: HostDraft,
) -> Result<Host, String> {
    services::host_use_cases::execute_add_host(app, &state, draft)
}

#[tauri::command]
pub(crate) fn update_host(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    patch: HostPatch,
) -> Result<Host, String> {
    services::host_use_cases::execute_update_host(app, &state, id, patch)
}

#[tauri::command]
pub(crate) fn delete_host(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<bool, String> {
    services::host_use_cases::execute_delete_host(app, &state, id)
}

#[tauri::command]
pub(crate) async fn test_ssh_connection(
    app: AppHandle,
    id: String,
) -> Result<ConnectionTest, String> {
    services::host_use_cases::execute_test_ssh_connection(app, id).await
}

#[tauri::command]
pub(crate) async fn ssh_check(
    app: AppHandle,
    host_alias: String,
    timeout_ms: Option<u64>,
) -> Result<SshCheckResult, String> {
    services::host_use_cases::execute_ssh_check(app, host_alias, timeout_ms).await
}

#[tauri::command]
pub(crate) fn bootstrap_ssh_host(
    app: AppHandle,
    state: State<'_, AppState>,
    draft: ssh::SshHostDraft,
    password: String,
    timeout_ms: Option<u64>,
    request_id: Option<String>,
) -> Result<SshBootstrapResult, String> {
    services::host_use_cases::execute_bootstrap_ssh_host(
        app, &state, draft, password, timeout_ms, request_id,
    )
}

#[tauri::command]
pub(crate) fn bootstrap_existing_ssh_host(
    app: AppHandle,
    state: State<'_, AppState>,
    host_alias: String,
    password: String,
    timeout_ms: Option<u64>,
) -> Result<SshBootstrapResult, String> {
    services::host_use_cases::execute_bootstrap_existing_ssh_host(
        app, &state, host_alias, password, timeout_ms,
    )
}

#[tauri::command]
pub(crate) async fn remote_probe_codex(
    app: AppHandle,
    host_alias: String,
    timeout_ms: Option<u64>,
) -> Result<RemoteProbeResult, String> {
    services::host_use_cases::execute_remote_probe_codex(app, host_alias, timeout_ms).await
}

#[tauri::command]
pub(crate) async fn sample_host_resources(
    app: AppHandle,
    host_aliases: Vec<String>,
    timeout_ms: Option<u64>,
    record_task: Option<bool>,
) -> Result<resource_monitor::HostResourceBatchResult, String> {
    services::host_use_cases::execute_sample_host_resources(
        app,
        host_aliases,
        timeout_ms,
        record_task.unwrap_or(true),
    )
    .await
}

#[tauri::command]
pub(crate) async fn remote_manage_codex(
    app: AppHandle,
    host_alias: String,
    action: RemoteCodexAction,
    timeout_ms: Option<u64>,
    request_id: Option<String>,
) -> Result<RemoteCodexMaintenanceResult, String> {
    services::host_use_cases::execute_remote_manage_codex(
        app, host_alias, action, timeout_ms, request_id,
    )
    .await
}

#[tauri::command]
pub(crate) async fn refresh_latest_codex_version(
    app: AppHandle,
    force: Option<bool>,
    timeout_ms: Option<u64>,
) -> Result<LatestCodexVersion, String> {
    services::host_use_cases::execute_refresh_latest_codex_version(app, force, timeout_ms).await
}

#[tauri::command]
pub(crate) async fn get_local_codex_status() -> Result<LocalCodexStatus, String> {
    services::host_use_cases::execute_get_local_codex_status().await
}
