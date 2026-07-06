use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

use super::{contains_key_material, refresh_credential_flags, validate_profile, AppState, Profile};

fn profiles_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .unwrap_or_else(|_| PathBuf::from(".codexhub"))
        .join("profiles.json")
}

pub(crate) fn load_profiles(app: &AppHandle, state: &AppState) -> Result<Vec<Profile>, String> {
    let path = profiles_path(app);
    let mut profiles = if path.exists() {
        let content = fs::read_to_string(&path)
            .map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
        serde_json::from_str::<Vec<Profile>>(&content)
            .map_err(|error| format!("Failed to parse {}: {error}", path.display()))?
    } else {
        state
            .profiles
            .lock()
            .expect("profiles mutex poisoned")
            .clone()
    };
    refresh_credential_flags(&mut profiles);
    *state.profiles.lock().expect("profiles mutex poisoned") = profiles.clone();
    Ok(profiles)
}

pub(crate) fn save_profiles(app: &AppHandle, state: &AppState, profiles: &[Profile]) -> Result<(), String> {
    for profile in profiles {
        validate_profile(profile)?;
    }
    let path = profiles_path(app);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let content = serde_json::to_string_pretty(profiles).map_err(|error| error.to_string())?;
    if contains_key_material(&content) {
        return Err("Refusing to persist profile data that looks like API key material.".into());
    }
    fs::write(&path, content)
        .map_err(|error| format!("Failed to write {}: {error}", path.display()))?;
    *state.profiles.lock().expect("profiles mutex poisoned") = profiles.to_vec();
    Ok(())
}
