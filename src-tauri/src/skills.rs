use std::env;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

use super::{normalize_skill_pack, AppState, SkillPack};

fn skills_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .unwrap_or_else(|_| PathBuf::from(".codexhub"))
        .join("skills.json")
}

pub(crate) fn managed_skills_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .unwrap_or_else(|_| PathBuf::from(".codexhub"))
        .join("skills")
}

pub(crate) fn skill_clone_cache_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_cache_dir()
        .unwrap_or_else(|_| env::temp_dir())
        .join("skill-clones")
}

pub(crate) fn load_skills(app: &AppHandle, state: &AppState) -> Result<Vec<SkillPack>, String> {
    let path = skills_path(app);
    let mut skills = if path.exists() {
        let content = fs::read_to_string(&path)
            .map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
        serde_json::from_str::<Vec<SkillPack>>(&content)
            .map_err(|error| format!("Failed to parse {}: {error}", path.display()))?
    } else {
        state
            .skill_packs
            .lock()
            .expect("skill packs mutex poisoned")
            .clone()
    };
    for skill in &mut skills {
        normalize_skill_pack(skill);
    }
    skills.sort_by_key(|skill| skill.name.to_ascii_lowercase());
    *state
        .skill_packs
        .lock()
        .expect("skill packs mutex poisoned") = skills.clone();
    Ok(skills)
}

pub(crate) fn save_skills(app: &AppHandle, state: &AppState, skills: &[SkillPack]) -> Result<(), String> {
    let path = skills_path(app);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let content = serde_json::to_string_pretty(skills).map_err(|error| error.to_string())?;
    fs::write(&path, content)
        .map_err(|error| format!("Failed to write {}: {error}", path.display()))?;
    *state
        .skill_packs
        .lock()
        .expect("skill packs mutex poisoned") = skills.to_vec();
    Ok(())
}
