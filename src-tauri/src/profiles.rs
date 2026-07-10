use tauri::AppHandle;

use super::{
    contains_key_material, refresh_credential_flags, storage, validate_profile, AppState, Profile,
};

pub(crate) fn load_profiles(_app: &AppHandle, state: &AppState) -> Result<Vec<Profile>, String> {
    let mut profiles =
        storage::load_document(&state.paths, "profiles", "profiles.json", Vec::new())?.data;
    refresh_credential_flags(&mut profiles)?;
    *state.profiles.lock().expect("profiles mutex poisoned") = profiles.clone();
    Ok(profiles)
}

pub(crate) fn save_profiles(
    _app: &AppHandle,
    state: &AppState,
    profiles: &[Profile],
) -> Result<(), String> {
    for profile in profiles {
        validate_profile(profile)?;
    }
    let content = serde_json::to_string_pretty(profiles).map_err(|error| error.to_string())?;
    if contains_key_material(&content) {
        return Err("Refusing to persist profile data that looks like API key material.".into());
    }
    storage::save_document(
        &state.paths,
        &state.task_store,
        "profiles",
        "profiles.json",
        profiles,
    )?;
    *state.profiles.lock().expect("profiles mutex poisoned") = profiles.to_vec();
    Ok(())
}
