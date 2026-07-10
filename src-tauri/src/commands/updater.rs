use crate::*;

#[tauri::command]
pub(crate) fn app_health() -> Health {
    services::updater_operations::app_health()
}

#[tauri::command]
pub(crate) fn get_app_update_status(app: AppHandle) -> AppUpdateStatus {
    services::updater_operations::get_app_update_status(app)
}

#[tauri::command]
pub(crate) async fn check_stable_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AppUpdateStatus, String> {
    services::updater_operations::check_stable_update(app, &state).await
}

#[tauri::command]
pub(crate) async fn install_stable_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AppUpdateStatus, String> {
    services::updater_operations::install_stable_update(app, &state).await
}
