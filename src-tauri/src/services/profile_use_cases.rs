use crate::*;

pub(crate) fn execute_list_profiles(
    app: AppHandle,
    state: &AppState,
) -> Result<Vec<Profile>, String> {
    load_profiles(&app, &state)
}

pub(crate) fn execute_create_profile(
    app: AppHandle,
    state: &AppState,
    draft: ProfileDraft,
) -> Result<Profile, String> {
    run_durable_local(&state, "Create profile", "profiles", || {
        let mut profiles = load_profiles(&app, &state)?;
        let mut profile = profile_from_draft(draft)?;
        ensure_unique_profile_id(&mut profile, &profiles);
        validate_profile(&profile)?;
        profile.credential_stored = false;
        profiles.push(profile.clone());
        save_profiles(&app, &state, &profiles)?;
        Ok(profile)
    })
}

pub(crate) fn execute_update_profile(
    app: AppHandle,
    state: &AppState,
    id: String,
    patch: ProfilePatch,
) -> Result<Profile, String> {
    run_durable_local(&state, "Update profile", "profiles", || {
        let mut profiles = load_profiles(&app, &state)?;
        let profile = profiles
            .iter_mut()
            .find(|profile| profile.id == id)
            .ok_or_else(|| format!("Profile {id} was not found."))?;
        apply_profile_patch(profile, patch)?;
        profile.updated_at = timestamp_label();
        validate_profile(profile)?;
        let updated = profile.clone();
        save_profiles(&app, &state, &profiles)?;
        Ok(updated)
    })
}

pub(crate) fn execute_delete_profile(
    app: AppHandle,
    state: &AppState,
    id: String,
) -> Result<DeleteOperationResult, String> {
    ensure_task_storage_healthy(&state)?;
    let mut profiles = load_profiles(&app, &state)?;
    let profile_name = profiles
        .iter()
        .find(|profile| profile.id == id)
        .map(|profile| profile.name.clone())
        .unwrap_or_else(|| id.clone());
    let before = profiles.len();
    profiles.retain(|profile| profile.id != id);
    let deleted = profiles.len() != before;
    let task_id = format!("task-delete-profile-{}", timestamp_millis());
    let mut task = jobs::begin_task(
        &state.task_store,
        state.task_event_sink.as_ref(),
        &task_id,
        &id,
        &profile_name,
        "Delete profile",
    )?;
    let mut credential_error = None;
    if deleted {
        let mut hosts = state.hosts.lock().expect("hosts mutex poisoned").clone();
        clear_profile_from_host_list(&mut hosts, &id);
        let operation_id = format!("operation-delete-profile-{}", timestamp_millis());
        let related_write =
            match services::profile_links::save(&state, &operation_id, profiles, hosts) {
                Ok(result) => result,
                Err(error) => {
                    task.status = TaskStatus::Failed;
                    task.ended_at = Some(timestamp_label());
                    task.summary = redact_error_text(&error);
                    task.logs.push(basic_log(
                        &task_id,
                        task.logs.len() + 1,
                        TaskLogLevel::Error,
                        &task.summary,
                    ));
                    record_task(&state, task)?;
                    return Err(jobs::task_error(&task_id, &error));
                }
            };
        task.logs.push(basic_log(
            &task_id,
            task.logs.len() + 1,
            TaskLogLevel::Info,
            &format!(
                "Committed related stores: {}; created {} backup(s).",
                related_write.changed_stores.join(", "),
                related_write.backup_paths.len()
            ),
        ));
        if let Err(error) = delete_profile_api_key_local(&id) {
            credential_error = Some(redact_error_text(&error));
        }
    }
    let message = if let Some(error) = credential_error.as_ref() {
        format!("Deleted profile {profile_name}, but its orphaned credential could not be removed: {error}")
    } else if deleted {
        format!("Deleted profile {profile_name}.")
    } else {
        format!("Profile {profile_name} was not found.")
    };
    task.status = if deleted && credential_error.is_none() {
        TaskStatus::Success
    } else {
        TaskStatus::Failed
    };
    task.ended_at = Some(timestamp_label());
    task.summary = message.clone();
    let mut final_log = basic_log(
        &task_id,
        task.logs.len() + 1,
        if matches!(task.status, TaskStatus::Success) {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        &message,
    );
    final_log.command = Some(format!("delete_profile {id}"));
    task.logs.push(final_log);
    record_task(&state, task.clone())?;
    Ok(DeleteOperationResult {
        ok: deleted && credential_error.is_none(),
        deleted,
        message,
        task,
    })
}

pub(crate) fn execute_duplicate_profile(
    app: AppHandle,
    state: &AppState,
    id: String,
    name: Option<String>,
) -> Result<Profile, String> {
    run_durable_local(&state, "Duplicate profile", "profiles", || {
        let mut profiles = load_profiles(&app, &state)?;
        let mut profile = profiles
            .iter()
            .find(|profile| profile.id == id)
            .cloned()
            .ok_or_else(|| format!("Profile {id} was not found."))?;
        profile.id = format!("{}-copy-{}", slugify(&profile.name), timestamp_millis());
        profile.name = name.unwrap_or_else(|| format!("{} Copy", profile.name));
        profile.created_at = timestamp_label();
        profile.updated_at = profile.created_at.clone();
        profile.source = "duplicate".into();
        profile.credential_stored = false;
        profile.host_ids.clear();
        validate_profile(&profile)?;
        profiles.push(profile.clone());
        save_profiles(&app, &state, &profiles)?;
        Ok(profile)
    })
}

pub(crate) fn execute_import_profiles(
    app: AppHandle,
    state: &AppState,
    bundle: ProfileImportExport,
    replace: Option<bool>,
) -> Result<ProfileImportExport, String> {
    run_durable_local(&state, "Import profiles", "profiles", || {
        let result =
            import_profiles_inner(&app, &state, bundle.profiles, replace.unwrap_or(false))?;
        Ok(profile_import_export(result.imported))
    })
}

pub(crate) fn execute_set_profile_api_key(
    app: AppHandle,
    state: &AppState,
    profile_id: String,
    api_key: String,
) -> Result<Profile, String> {
    run_durable_local(&state, "Store profile credential", "profiles", || {
        if api_key.trim().is_empty() {
            return Err("API key value cannot be empty.".into());
        }
        let mut profiles = load_profiles(&app, &state)?;
        let profile = profiles
            .iter_mut()
            .find(|profile| profile.id == profile_id)
            .ok_or_else(|| format!("Profile {profile_id} was not found."))?;
        profile.credential_stored = true;
        profile.updated_at = timestamp_label();
        let updated = profile.clone();
        services::credentials::apply_with_metadata(
            &adapters::OsCredentialAdapter,
            &profile_id,
            Some(&api_key),
            || {
                save_profiles(&app, &state, &profiles)?;
                Ok(updated)
            },
        )
    })
}

// This command has sensitive output and is used only by the explicit profile-editor reveal action.
pub(crate) fn execute_get_profile_api_key(
    app: AppHandle,
    state: &AppState,
    profile_id: String,
) -> Result<ProfileApiKeyResult, String> {
    let mut profiles = load_profiles(&app, &state)?;
    let profile = profiles
        .iter()
        .find(|profile| profile.id == profile_id)
        .cloned()
        .ok_or_else(|| format!("Profile {profile_id} was not found."))?;
    let mut api_key = load_profile_api_key_local(&profile_id)?;
    if api_key.is_none() && profile.source == "cc-switch" {
        api_key = run_durable_local(&state, "Migrate profile credential", "profiles", || {
            let migrated = find_cc_switch_api_key_for_profile(&state, &profile)?;
            let Some(value) = migrated.clone() else {
                return Ok(None);
            };
            for item in &mut profiles {
                if item.id == profile_id {
                    item.credential_stored = true;
                }
            }
            services::credentials::apply_with_metadata(
                &adapters::OsCredentialAdapter,
                &profile_id,
                Some(&value),
                || {
                    save_profiles(&app, &state, &profiles)?;
                    Ok(migrated)
                },
            )
        })?;
    }
    Ok(ProfileApiKeyResult {
        profile_id,
        exists: api_key.is_some(),
        api_key,
    })
}

pub(crate) fn execute_delete_profile_api_key(
    app: AppHandle,
    state: &AppState,
    profile_id: String,
) -> Result<Profile, String> {
    run_durable_local(&state, "Delete profile credential", "profiles", || {
        let mut profiles = load_profiles(&app, &state)?;
        let profile = profiles
            .iter_mut()
            .find(|profile| profile.id == profile_id)
            .ok_or_else(|| format!("Profile {profile_id} was not found."))?;
        profile.credential_stored = false;
        profile.updated_at = timestamp_label();
        let updated = profile.clone();
        services::credentials::apply_with_metadata(
            &adapters::OsCredentialAdapter,
            &profile_id,
            None,
            || {
                save_profiles(&app, &state, &profiles)?;
                Ok(updated)
            },
        )
    })
}

pub(crate) fn execute_preview_profile_apply(
    app: AppHandle,
    state: &AppState,
    profile_id: String,
    host_ids: Vec<String>,
) -> Result<ProfileApplyPreview, String> {
    let profile = find_profile(&app, &state, &profile_id)?;
    let rendered_toml = render_profile_toml(&profile)?;
    let hosts = resolve_apply_hosts(&state, &host_ids);
    let target_files = profile_apply_targets(&hosts, &profile.id);
    let host_results = hosts
        .iter()
        .map(|host| profile_apply_preview_result(host, &profile.id))
        .collect();
    Ok(ProfileApplyPreview {
        profile_id: profile.id.clone(),
        profile_name: profile.name.clone(),
        rendered_toml,
        target_files,
        host_results,
        warnings: profile_preview_warnings(&profile),
    })
}

pub(crate) async fn execute_apply_profile(
    app: AppHandle,
    profile_id: String,
    host_ids: Vec<String>,
    timeout_ms: Option<u64>,
) -> Result<ProfileApplyBatchResult, String> {
    ensure_task_storage_for_app(&app)?;
    run_blocking_command("apply_profile", move || {
        let state = app.state::<AppState>();
        let profile = find_profile(&app, &state, &profile_id)?;
        let rendered_toml = render_profile_toml(&profile)?;
        let timeout = ssh::normalize_timeout_ms(timeout_ms.or(Some(30_000)));
        let result =
            apply_profile_to_hosts(&app, &state, &profile, &rendered_toml, host_ids, timeout)?;
        Ok(result)
    })
    .await?
}

pub(crate) fn execute_detect_cc_switch_profiles(
    state: &AppState,
) -> Result<CcSwitchDetection, String> {
    let detected = detect_cc_switch_profiles_inner(&state)?;
    let source_path = detected.first().map(|item| item.source_path.clone());
    let profiles = detected
        .into_iter()
        .map(|item| item.profile)
        .collect::<Vec<_>>();
    let count = profiles.len();
    Ok(CcSwitchDetection {
        detected: count > 0,
        source_path,
        message: if count > 0 {
            format!("{count} cc-switch profiles detected.")
        } else {
            "No cc-switch profiles detected.".into()
        },
        import_export: profile_import_export(profiles),
    })
}

pub(crate) fn execute_import_cc_switch_profiles(
    app: AppHandle,
    state: &AppState,
    replace: Option<bool>,
) -> Result<ProfileImportExport, String> {
    run_durable_local(&state, "Import cc-switch profiles", "profiles", || {
        let detected = detect_cc_switch_profiles_inner(&state)?;
        let credential_by_key = detected
            .iter()
            .filter_map(|item| {
                item.api_key
                    .as_ref()
                    .map(|api_key| (cc_switch_profile_import_key(&item.profile), api_key.clone()))
            })
            .collect::<HashMap<_, _>>();
        let profiles = detected
            .into_iter()
            .map(|item| item.profile)
            .collect::<Vec<_>>();
        let (mut all_profiles, mut result) =
            prepare_profiles_import(&app, &state, profiles, replace.unwrap_or(false))?;
        let mut credential_changes = Vec::new();
        for profile in &mut result.imported {
            if let Some(api_key) = credential_by_key.get(&cc_switch_profile_import_key(profile)) {
                profile.credential_stored = true;
                credential_changes.push((profile.id.clone(), api_key.clone()));
            }
        }
        for profile in &mut all_profiles {
            if credential_changes
                .iter()
                .any(|(profile_id, _)| profile_id == &profile.id)
            {
                profile.credential_stored = true;
            }
        }
        let output = profile_import_export(result.imported);
        services::credentials::apply_batch_with_metadata(
            &adapters::OsCredentialAdapter,
            &credential_changes,
            || {
                save_profiles(&app, &state, &all_profiles)?;
                Ok(output)
            },
        )
    })
}

pub(crate) fn find_cc_switch_api_key_for_profile(
    state: &AppState,
    profile: &Profile,
) -> Result<Option<String>, String> {
    let import_key = cc_switch_profile_import_key(profile);
    let detected = detect_cc_switch_profiles_inner(state)?;
    let api_key = detected
        .into_iter()
        .find(|item| cc_switch_profile_import_key(&item.profile) == import_key)
        .and_then(|item| item.api_key);
    Ok(api_key)
}
