use crate::*;

#[tauri::command]
pub(crate) fn list_local_skills(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<SkillPack>, String> {
    services::skill_use_cases::execute_list_local_skills(app, &state)
}

#[tauri::command]
pub(crate) fn list_skill_packs(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<SkillPack>, String> {
    services::skill_use_cases::execute_list_skill_packs(app, &state)
}

#[tauri::command]
pub(crate) fn import_local_skill(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<SkillImportResult, String> {
    services::skill_use_cases::execute_import_local_skill(app, &state, path)
}

#[tauri::command]
pub(crate) fn update_library_skill_about(
    app: AppHandle,
    state: State<'_, AppState>,
    skill_id: String,
    about: String,
) -> Result<Vec<SkillPack>, String> {
    services::skill_use_cases::execute_update_library_skill_about(app, &state, skill_id, about)
}

#[tauri::command]
pub(crate) fn get_skill_inventory_status(
    state: State<'_, AppState>,
) -> Result<SkillInventoryStatus, String> {
    services::skill_use_cases::execute_get_skill_inventory_status(&state)
}

#[tauri::command]
pub(crate) async fn detect_installed_skills(
    app: AppHandle,
    include_hosts: Option<bool>,
    timeout_ms: Option<u64>,
) -> Result<SkillDetectionResult, String> {
    services::skill_use_cases::execute_detect_installed_skills(app, include_hosts, timeout_ms).await
}

#[tauri::command]
pub(crate) async fn download_github_skill(
    app: AppHandle,
    repo_url: String,
    timeout_ms: Option<u64>,
) -> Result<SkillImportResult, String> {
    services::skill_use_cases::execute_download_github_skill(app, repo_url, timeout_ms).await
}

#[tauri::command]
pub(crate) async fn get_skill_targets(
    app: AppHandle,
    skill_id: String,
    timeout_ms: Option<u64>,
) -> Result<SkillTargetsResult, String> {
    services::skill_use_cases::execute_get_skill_targets(app, skill_id, timeout_ms).await
}

#[tauri::command]
pub(crate) async fn install_skill_targets(
    app: AppHandle,
    skill_id: String,
    targets: Vec<SkillTargetRequest>,
    timeout_ms: Option<u64>,
) -> Result<SkillTargetOperationResult, String> {
    services::skill_use_cases::execute_install_skill_targets(app, skill_id, targets, timeout_ms)
        .await
}

#[tauri::command]
pub(crate) async fn uninstall_skill_targets(
    app: AppHandle,
    skill_id: String,
    targets: Vec<SkillTargetRequest>,
    timeout_ms: Option<u64>,
) -> Result<SkillTargetOperationResult, String> {
    services::skill_use_cases::execute_uninstall_skill_targets(app, skill_id, targets, timeout_ms)
        .await
}

#[tauri::command]
pub(crate) async fn delete_library_skill(
    app: AppHandle,
    skill_id: String,
    uninstall_first: bool,
    timeout_ms: Option<u64>,
) -> Result<SkillTargetOperationResult, String> {
    services::skill_use_cases::execute_delete_library_skill(
        app,
        skill_id,
        uninstall_first,
        timeout_ms,
    )
    .await
}

#[tauri::command]
pub(crate) async fn download_installed_skill(
    app: AppHandle,
    request: InstalledSkillRequest,
    timeout_ms: Option<u64>,
) -> Result<InstalledSkillDownloadResult, String> {
    services::skill_use_cases::execute_download_installed_skill(app, request, timeout_ms).await
}

#[tauri::command]
pub(crate) async fn uninstall_installed_skill(
    app: AppHandle,
    request: InstalledSkillRequest,
    timeout_ms: Option<u64>,
) -> Result<SkillTargetOperationResult, String> {
    services::skill_use_cases::execute_uninstall_installed_skill(app, request, timeout_ms).await
}
