use tauri::AppHandle;

use super::{storage, AppState, Host};

pub(crate) fn load_hosts(_app: &AppHandle, state: &AppState) -> Result<Vec<Host>, String> {
    let hosts = storage::load_document(&state.paths, "hosts", "hosts.json", Vec::new())?.data;
    *state.hosts.lock().expect("hosts mutex poisoned") = hosts.clone();
    Ok(hosts)
}

pub(crate) fn save_hosts(_app: &AppHandle, state: &AppState, hosts: &[Host]) -> Result<(), String> {
    storage::save_document(
        &state.paths,
        &state.task_store,
        "hosts",
        "hosts.json",
        hosts,
    )?;
    *state.hosts.lock().expect("hosts mutex poisoned") = hosts.to_vec();
    Ok(())
}

pub(crate) fn save_current_hosts(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let hosts = state.hosts.lock().expect("hosts mutex poisoned").clone();
    save_hosts(app, state, &hosts)
}
