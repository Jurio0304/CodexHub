use crate::*;

#[tauri::command]
pub(crate) fn list_profiles(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<Profile>, String> {
    services::profile_use_cases::execute_list_profiles(app, &state)
}

#[tauri::command]
pub(crate) fn create_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    draft: ProfileDraft,
) -> Result<Profile, String> {
    services::profile_use_cases::execute_create_profile(app, &state, draft)
}

#[tauri::command]
pub(crate) fn update_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    patch: ProfilePatch,
) -> Result<Profile, String> {
    services::profile_use_cases::execute_update_profile(app, &state, id, patch)
}

#[tauri::command]
pub(crate) fn delete_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<DeleteOperationResult, String> {
    services::profile_use_cases::execute_delete_profile(app, &state, id)
}

#[tauri::command]
pub(crate) fn duplicate_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
) -> Result<Profile, String> {
    services::profile_use_cases::execute_duplicate_profile(app, &state, id, name)
}

#[tauri::command]
pub(crate) fn import_profiles(
    app: AppHandle,
    state: State<'_, AppState>,
    bundle: ProfileImportExport,
    replace: Option<bool>,
) -> Result<ProfileImportExport, String> {
    services::profile_use_cases::execute_import_profiles(app, &state, bundle, replace)
}

#[tauri::command]
pub(crate) fn set_profile_api_key(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
    api_key: String,
) -> Result<Profile, String> {
    services::profile_use_cases::execute_set_profile_api_key(app, &state, profile_id, api_key)
}

#[tauri::command]
// Sensitive output is used only by the explicit profile-editor reveal action.
pub(crate) fn get_profile_api_key(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<ProfileApiKeyResult, String> {
    services::profile_use_cases::execute_get_profile_api_key(app, &state, profile_id)
}

#[tauri::command]
pub(crate) fn delete_profile_api_key(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<Profile, String> {
    services::profile_use_cases::execute_delete_profile_api_key(app, &state, profile_id)
}

#[tauri::command]
pub(crate) fn preview_profile_apply(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
    host_ids: Vec<String>,
) -> Result<ProfileApplyPreview, String> {
    services::profile_use_cases::execute_preview_profile_apply(app, &state, profile_id, host_ids)
}

#[tauri::command]
pub(crate) async fn apply_profile(
    app: AppHandle,
    profile_id: String,
    host_ids: Vec<String>,
    options: Option<ProfileApplyOptions>,
    timeout_ms: Option<u64>,
) -> Result<ProfileApplyBatchResult, String> {
    services::profile_use_cases::execute_apply_profile(
        app, profile_id, host_ids, options, timeout_ms,
    )
    .await
}

#[tauri::command]
pub(crate) fn detect_cc_switch_profiles(
    state: State<'_, AppState>,
) -> Result<CcSwitchDetection, String> {
    services::profile_use_cases::execute_detect_cc_switch_profiles(&state)
}

#[tauri::command]
pub(crate) fn import_cc_switch_profiles(
    app: AppHandle,
    state: State<'_, AppState>,
    replace: Option<bool>,
) -> Result<ProfileImportExport, String> {
    services::profile_use_cases::execute_import_cc_switch_profiles(app, &state, replace)
}
