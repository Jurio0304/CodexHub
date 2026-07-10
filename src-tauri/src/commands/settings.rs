use crate::settings::{AppSettings, CloseButtonBehavior, SettingsSaveResult};
use crate::{
    detect_network_proxy_status, hide_main_window, read_settings, run_durable_local,
    write_settings, AppState, NetworkProxyStatus,
};
use tauri::{AppHandle, State};

#[tauri::command]
pub(crate) fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    read_settings(&state.paths)
}

#[tauri::command]
pub(crate) fn save_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<SettingsSaveResult, String> {
    run_durable_local(&state, "Save settings", "settings", || {
        write_settings(&state.paths, &state.task_store, &settings)
    })
}

#[tauri::command]
pub(crate) fn detect_network_proxy(
    state: State<'_, AppState>,
) -> Result<NetworkProxyStatus, String> {
    let settings = read_settings(&state.paths)?;
    Ok(detect_network_proxy_status(&settings))
}

#[tauri::command]
pub(crate) fn choose_close_button_behavior(
    app: AppHandle,
    state: State<'_, AppState>,
    behavior: CloseButtonBehavior,
) -> Result<SettingsSaveResult, String> {
    let saved = run_durable_local(&state, "Choose close button behavior", "settings", || {
        let mut settings = read_settings(&state.paths)?;
        settings.close_button_behavior = behavior.clone();
        write_settings(&state.paths, &state.task_store, &settings)
    })?;

    match behavior {
        CloseButtonBehavior::Ask => {}
        CloseButtonBehavior::Exit => app.exit(0),
        CloseButtonBehavior::MinimizeToTray => hide_main_window(&app),
    }
    Ok(saved)
}
