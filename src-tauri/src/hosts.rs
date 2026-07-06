use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

use super::{AppState, Host};

fn hosts_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .unwrap_or_else(|_| PathBuf::from(".codexhub"))
        .join("hosts.json")
}

pub(crate) fn load_hosts(app: &AppHandle, state: &AppState) -> Result<Vec<Host>, String> {
    let path = hosts_path(app);
    let hosts = if path.exists() {
        let content = fs::read_to_string(&path)
            .map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
        serde_json::from_str::<Vec<Host>>(&content)
            .map_err(|error| format!("Failed to parse {}: {error}", path.display()))?
    } else {
        state.hosts.lock().expect("hosts mutex poisoned").clone()
    };
    *state.hosts.lock().expect("hosts mutex poisoned") = hosts.clone();
    Ok(hosts)
}

pub(crate) fn save_hosts(app: &AppHandle, state: &AppState, hosts: &[Host]) -> Result<(), String> {
    let path = hosts_path(app);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let content = serde_json::to_string_pretty(hosts).map_err(|error| error.to_string())?;
    fs::write(&path, content)
        .map_err(|error| format!("Failed to write {}: {error}", path.display()))?;
    *state.hosts.lock().expect("hosts mutex poisoned") = hosts.to_vec();
    Ok(())
}

pub(crate) fn save_current_hosts(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let hosts = state.hosts.lock().expect("hosts mutex poisoned").clone();
    save_hosts(app, state, &hosts)
}
