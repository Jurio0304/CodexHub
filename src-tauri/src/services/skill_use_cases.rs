use crate::*;

pub(crate) fn execute_list_local_skills(
    app: AppHandle,
    state: &AppState,
) -> Result<Vec<SkillPack>, String> {
    load_skills(&app, &state)
}

pub(crate) fn execute_list_skill_packs(
    app: AppHandle,
    state: &AppState,
) -> Result<Vec<SkillPack>, String> {
    load_skills(&app, &state)
}

pub(crate) fn execute_import_local_skill(
    app: AppHandle,
    state: &AppState,
    path: String,
) -> Result<SkillImportResult, String> {
    run_durable_local(&state, "Import local skill", "skills", || {
        import_skills_from_path(&app, &state, PathBuf::from(path), "local", None)
    })
}

pub(crate) fn execute_update_library_skill_about(
    app: AppHandle,
    state: &AppState,
    skill_id: String,
    about: String,
) -> Result<Vec<SkillPack>, String> {
    run_durable_local(&state, "Update skill details", "skills", || {
        let mut skills = load_skills(&app, &state)?;
        let Some(skill) = skills.iter_mut().find(|skill| skill.id == skill_id) else {
            return Err(format!("Skill {skill_id} was not found."));
        };
        let details = about.trim().to_string();
        skill.description = details.clone();
        skill.about = details;
        skill.updated_at = timestamp_label();
        save_skills(&app, &state, &skills)?;
        Ok(skills)
    })
}

pub(crate) fn execute_get_skill_inventory_status(
    state: &AppState,
) -> Result<SkillInventoryStatus, String> {
    load_skill_inventory_status(&state)
}

pub(crate) async fn execute_detect_installed_skills(
    app: AppHandle,
    include_hosts: Option<bool>,
    timeout_ms: Option<u64>,
) -> Result<SkillDetectionResult, String> {
    ensure_task_storage_for_app(&app)?;
    run_blocking_command("detect_installed_skills", move || {
        let state = app.state::<AppState>();
        jobs::run_observed_operation(
            &state.task_store,
            state.task_event_sink.as_ref(),
            "Detect installed skills",
            "skills",
            || {
                run_detect_installed_skills(
                    &app,
                    &state,
                    include_hosts.unwrap_or(false),
                    timeout_ms,
                )
            },
            |result| {
                let failed = result.tasks.iter().any(|task| {
                    matches!(task.status, TaskStatus::Failed | TaskStatus::Interrupted)
                });
                (
                    if failed {
                        TaskStatus::Failed
                    } else {
                        TaskStatus::Success
                    },
                    result.message.clone(),
                )
            },
        )
    })
    .await?
}

pub(crate) async fn execute_download_github_skill(
    app: AppHandle,
    repo_url: String,
    timeout_ms: Option<u64>,
) -> Result<SkillImportResult, String> {
    ensure_task_storage_for_app(&app)?;
    run_blocking_command("download_github_skill", move || {
        let state = app.state::<AppState>();
        jobs::run_observed_operation(
            &state.task_store,
            state.task_event_sink.as_ref(),
            "Download GitHub skill",
            "skills",
            || download_and_import_github_skill(&app, &state, repo_url, timeout_ms),
            |result| {
                (
                    if result.skipped.is_empty() && !result.imported.is_empty() {
                        TaskStatus::Success
                    } else {
                        TaskStatus::Failed
                    },
                    result.message.clone(),
                )
            },
        )
    })
    .await?
}

pub(crate) async fn execute_get_skill_targets(
    app: AppHandle,
    skill_id: String,
    timeout_ms: Option<u64>,
) -> Result<SkillTargetsResult, String> {
    run_blocking_command("get_skill_targets", move || {
        let state = app.state::<AppState>();
        run_get_skill_targets(&app, &state, skill_id, timeout_ms)
    })
    .await?
}

pub(crate) async fn execute_install_skill_targets(
    app: AppHandle,
    skill_id: String,
    targets: Vec<SkillTargetRequest>,
    timeout_ms: Option<u64>,
) -> Result<SkillTargetOperationResult, String> {
    ensure_task_storage_for_app(&app)?;
    run_blocking_command("install_skill_targets", move || {
        let state = app.state::<AppState>();
        jobs::run_observed_operation(
            &state.task_store,
            state.task_event_sink.as_ref(),
            "Install skill targets",
            "skills",
            || run_install_skill_targets(&app, &state, skill_id, targets, timeout_ms),
            |result| {
                (
                    if result.ok {
                        TaskStatus::Success
                    } else {
                        TaskStatus::Failed
                    },
                    result.message.clone(),
                )
            },
        )
    })
    .await?
}

pub(crate) async fn execute_uninstall_skill_targets(
    app: AppHandle,
    skill_id: String,
    targets: Vec<SkillTargetRequest>,
    timeout_ms: Option<u64>,
) -> Result<SkillTargetOperationResult, String> {
    ensure_task_storage_for_app(&app)?;
    run_blocking_command("uninstall_skill_targets", move || {
        let state = app.state::<AppState>();
        jobs::run_observed_operation(
            &state.task_store,
            state.task_event_sink.as_ref(),
            "Uninstall skill targets",
            "skills",
            || run_uninstall_skill_targets(&app, &state, skill_id, targets, timeout_ms),
            |result| {
                (
                    if result.ok {
                        TaskStatus::Success
                    } else {
                        TaskStatus::Failed
                    },
                    result.message.clone(),
                )
            },
        )
    })
    .await?
}

pub(crate) async fn execute_delete_library_skill(
    app: AppHandle,
    skill_id: String,
    uninstall_first: bool,
    timeout_ms: Option<u64>,
) -> Result<SkillTargetOperationResult, String> {
    ensure_task_storage_for_app(&app)?;
    run_blocking_command("delete_library_skill", move || {
        let state = app.state::<AppState>();
        jobs::run_observed_operation(
            &state.task_store,
            state.task_event_sink.as_ref(),
            "Delete library skill",
            "skills",
            || run_delete_library_skill(&app, &state, skill_id, uninstall_first, timeout_ms),
            |result| {
                (
                    if result.ok {
                        TaskStatus::Success
                    } else {
                        TaskStatus::Failed
                    },
                    result.message.clone(),
                )
            },
        )
    })
    .await?
}

pub(crate) async fn execute_download_installed_skill(
    app: AppHandle,
    request: InstalledSkillRequest,
    timeout_ms: Option<u64>,
) -> Result<InstalledSkillDownloadResult, String> {
    ensure_task_storage_for_app(&app)?;
    run_blocking_command("download_installed_skill", move || {
        let state = app.state::<AppState>();
        jobs::run_observed_operation(
            &state.task_store,
            state.task_event_sink.as_ref(),
            "Download installed skill",
            "skills",
            || run_download_installed_skill(&app, &state, request, timeout_ms),
            |result| {
                let failed = !result.skipped.is_empty()
                    || result.tasks.iter().any(|task| {
                        matches!(task.status, TaskStatus::Failed | TaskStatus::Interrupted)
                    });
                (
                    if failed {
                        TaskStatus::Failed
                    } else {
                        TaskStatus::Success
                    },
                    result.message.clone(),
                )
            },
        )
    })
    .await?
}

pub(crate) async fn execute_uninstall_installed_skill(
    app: AppHandle,
    request: InstalledSkillRequest,
    timeout_ms: Option<u64>,
) -> Result<SkillTargetOperationResult, String> {
    ensure_task_storage_for_app(&app)?;
    run_blocking_command("uninstall_installed_skill", move || {
        let state = app.state::<AppState>();
        jobs::run_observed_operation(
            &state.task_store,
            state.task_event_sink.as_ref(),
            "Uninstall installed skill",
            "skills",
            || run_uninstall_installed_skill(&app, &state, request, timeout_ms),
            |result| {
                (
                    if result.ok {
                        TaskStatus::Success
                    } else {
                        TaskStatus::Failed
                    },
                    result.message.clone(),
                )
            },
        )
    })
    .await?
}
