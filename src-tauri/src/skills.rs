use std::path::PathBuf;
use tauri::AppHandle;

use super::{normalize_skill_pack, storage, AppState, SkillPack};

pub(crate) fn managed_skills_dir(state: &AppState) -> PathBuf {
    state.paths.config_file("skills")
}

pub(crate) fn skill_clone_cache_dir(state: &AppState) -> PathBuf {
    state.paths.cache_file("skill-clones")
}

pub(crate) fn load_skills(_app: &AppHandle, state: &AppState) -> Result<Vec<SkillPack>, String> {
    let mut skills =
        storage::load_document(&state.paths, "skills", "skills.json", Vec::new())?.data;
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

pub(crate) fn save_skills(
    _app: &AppHandle,
    state: &AppState,
    skills: &[SkillPack],
) -> Result<(), String> {
    storage::save_document(
        &state.paths,
        &state.task_store,
        "skills",
        "skills.json",
        skills,
    )?;
    *state
        .skill_packs
        .lock()
        .expect("skill packs mutex poisoned") = skills.to_vec();
    Ok(())
}
