use crate::*;

pub(crate) async fn execute_get_ssh_status() -> Result<ssh::SshStatus, String> {
    run_blocking_command("get_ssh_status", ssh::get_ssh_status).await?
}

pub(crate) fn execute_generate_ed25519_key(
    app: AppHandle,
) -> Result<ssh::SshKeyGenerationResult, String> {
    let state = app.state::<AppState>();
    run_durable_local(
        &state,
        "Generate Ed25519 key",
        "ssh",
        ssh::generate_ed25519_key,
    )
}

pub(crate) async fn execute_list_ssh_config_hosts() -> Result<Vec<ssh::SshConfigHost>, String> {
    run_blocking_command("list_ssh_config_hosts", ssh::list_ssh_config_hosts).await?
}

pub(crate) fn execute_upsert_ssh_config_host(
    app: AppHandle,
    draft: ssh::SshHostDraft,
) -> Result<ssh::SshConfigWriteResult, String> {
    let state = app.state::<AppState>();
    run_durable_local(&state, "Save SSH Host", "hosts", || {
        ssh::upsert_ssh_config_host(draft)
    })
}

pub(crate) fn execute_delete_ssh_config_host(
    app: AppHandle,
    alias: String,
    state: &AppState,
) -> Result<SshConfigDeleteResult, String> {
    ensure_task_storage_healthy(&state)?;
    let normalized_alias = alias.trim().to_string();
    let host_id = host_id_for_alias(&state, &normalized_alias);
    let host_name = host_name_for_alias(&state, &normalized_alias);
    let task_id = format!("task-delete-host-{}", timestamp_millis());
    let mut task = jobs::begin_task(
        &state.task_store,
        state.task_event_sink.as_ref(),
        &task_id,
        &host_id,
        &host_name,
        "Delete SSH Host",
    )?;
    let operation = (|| -> Result<ssh::SshConfigWriteResult, String> {
        let result = ssh::delete_ssh_config_host(alias.clone())?;
        if result.changed {
            let mut next_hosts = state.hosts.lock().expect("hosts mutex poisoned").clone();
            next_hosts.retain(|host| !host.host_alias.eq_ignore_ascii_case(alias.trim()));
            save_hosts(&app, &state, &next_hosts).map_err(|error| {
                format!(
                    "partial-failure: SSH config changed, but the Host inventory failed to persist: {error}"
                )
            })?;
        }
        Ok(result)
    })();
    let result = match operation {
        Ok(result) => {
            task.status = TaskStatus::Success;
            task.summary = result.message.clone();
            result
        }
        Err(error) => {
            task.status = TaskStatus::Failed;
            task.summary = redact_error_text(&error);
            task.ended_at = Some(timestamp_label());
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
    task.ended_at = Some(timestamp_label());
    let mut final_log = basic_log(
        &task_id,
        task.logs.len() + 1,
        TaskLogLevel::Info,
        &task.summary,
    );
    final_log.command = Some(format!("delete_ssh_config_host {normalized_alias}"));
    task.logs.push(final_log);
    record_task(&state, task.clone())?;
    Ok(SshConfigDeleteResult {
        write_result: result,
        task,
    })
}

pub(crate) fn execute_list_hosts(app: AppHandle, state: &AppState) -> Result<Vec<Host>, String> {
    let hosts = load_hosts(&app, &state)?;
    *state.hosts.lock().expect("hosts mutex poisoned") = hosts;
    let profiles = profile_apply_profiles_snapshot(&app, &state)?;
    reconcile_hosts_with_profile_links(&state, &profiles);
    apply_skill_inventory_to_hosts(&state)?;
    let next_hosts = state.hosts.lock().expect("hosts mutex poisoned").clone();
    Ok(next_hosts)
}

pub(crate) fn execute_refresh_discovered_hosts(
    app: AppHandle,
    state: &AppState,
) -> Result<Vec<Host>, String> {
    run_durable_local(&state, "Refresh discovered hosts", "hosts", || {
        let original = load_hosts(&app, &state)?;
        let result = (|| {
            merge_discovered_hosts(&state)?;
            let profiles = load_profiles(&app, &state)?;
            reconcile_hosts_with_profile_links(&state, &profiles);
            let next_hosts = state.hosts.lock().expect("hosts mutex poisoned").clone();
            save_hosts(&app, &state, &next_hosts)?;
            Ok(next_hosts)
        })();
        if result.is_err() {
            *state.hosts.lock().expect("hosts mutex poisoned") = original;
        }
        result
    })
}

pub(crate) fn execute_add_host(
    app: AppHandle,
    state: &AppState,
    draft: HostDraft,
) -> Result<Host, String> {
    run_durable_local(&state, "Add host", "hosts", || {
        let host = Host {
            id: format!("host-{}", timestamp_millis()),
            name: draft.name,
            host_alias: draft.address.clone(),
            source: "manual".into(),
            address: draft.address,
            port: draft.port,
            username: draft.username,
            auth_method: draft.auth_method,
            status: HostStatus::Unknown,
            os: "Unknown".into(),
            arch: "Unknown".into(),
            shell: "Unknown".into(),
            path: None,
            path_has_local_bin: None,
            codex_command_available: None,
            codex_installed: false,
            codex_version: "pending".into(),
            config_exists: None,
            api_config_name: None,
            api_config_source: None,
            api_key_env_var: None,
            api_key_env_present: None,
            skills_exists: None,
            skills_count: None,
            profile_id: None,
            skill_pack_ids: Vec::new(),
            tags: draft.tags,
            last_seen: "just added".into(),
            latency_ms: None,
        };

        let mut next_hosts = load_hosts(&app, &state)?;
        next_hosts.insert(0, host.clone());
        save_hosts(&app, &state, &next_hosts)?;
        Ok(host)
    })
}

pub(crate) fn execute_update_host(
    app: AppHandle,
    state: &AppState,
    id: String,
    patch: HostPatch,
) -> Result<Host, String> {
    run_durable_local(&state, "Update host", "hosts", || {
        let mut next_hosts = load_hosts(&app, &state)?;

        if let Some(host) = next_hosts.iter_mut().find(|host| host.id == id) {
            if let Some(name) = patch.name {
                host.name = name;
            }
            if let Some(address) = patch.address {
                host.address = address;
            }
            if let Some(port) = patch.port {
                host.port = port;
            }
            if let Some(username) = patch.username {
                host.username = username;
            }
            if let Some(auth_method) = patch.auth_method {
                host.auth_method = auth_method;
            }
            if let Some(status) = patch.status {
                host.status = status;
            }
            if let Some(profile_id) = patch.profile_id {
                host.profile_id = Some(profile_id);
            }
            if let Some(tags) = patch.tags {
                host.tags = tags;
            }
            let updated = host.clone();
            save_hosts(&app, &state, &next_hosts)?;
            return Ok(updated);
        }

        Err(format!("Host {id} was not found."))
    })
}

pub(crate) fn execute_delete_host(
    app: AppHandle,
    state: &AppState,
    id: String,
) -> Result<bool, String> {
    run_durable_local(&state, "Delete host", "hosts", || {
        let mut next_hosts = load_hosts(&app, &state)?;
        let before = next_hosts.len();
        next_hosts.retain(|host| host.id != id);
        let changed = next_hosts.len() != before;
        if changed {
            save_hosts(&app, &state, &next_hosts)?;
        }
        Ok(changed)
    })
}

pub(crate) async fn execute_test_ssh_connection(
    app: AppHandle,
    id: String,
) -> Result<ConnectionTest, String> {
    ensure_task_storage_for_app(&app)?;
    run_blocking_command("test_ssh_connection", move || {
        let state = app.state::<AppState>();
        let host_alias = test_connection_host_alias(&state, &id)?;
        let result = run_ssh_check(&state, host_alias, Some(10_000))?;
        save_current_hosts(&app, &state)?;
        Ok(ConnectionTest {
            ok: result.ok,
            latency_ms: result.latency_ms,
            message: result.message,
        })
    })
    .await?
}

pub(crate) fn test_connection_host_alias(state: &AppState, id: &str) -> Result<String, String> {
    let host_alias = state
        .hosts
        .lock()
        .expect("hosts mutex poisoned")
        .iter()
        .find(|host| host.id == id)
        .map(|host| host.host_alias.clone())
        .ok_or_else(|| format!("Host {id} was not found."))?;
    ssh::validate_ssh_alias(&host_alias)
}

pub(crate) async fn execute_ssh_check(
    app: AppHandle,
    host_alias: String,
    timeout_ms: Option<u64>,
) -> Result<SshCheckResult, String> {
    ensure_task_storage_for_app(&app)?;
    run_blocking_command("ssh_check", move || {
        let state = app.state::<AppState>();
        run_ssh_check(&state, host_alias, timeout_ms)
    })
    .await?
}

pub(crate) fn execute_bootstrap_ssh_host(
    app: AppHandle,
    state: &AppState,
    draft: ssh::SshHostDraft,
    password: String,
    timeout_ms: Option<u64>,
    request_id: Option<String>,
) -> Result<SshBootstrapResult, String> {
    ensure_task_storage_healthy(&state)?;
    run_ssh_bootstrap(&app, &state, draft, password, timeout_ms, request_id)
}

pub(crate) fn execute_bootstrap_existing_ssh_host(
    app: AppHandle,
    state: &AppState,
    host_alias: String,
    password: String,
    timeout_ms: Option<u64>,
) -> Result<SshBootstrapResult, String> {
    ensure_task_storage_healthy(&state)?;
    let result = run_existing_ssh_bootstrap(&state, host_alias, password, timeout_ms)?;
    if result.ok {
        save_current_hosts(&app, &state)?;
    }
    Ok(result)
}

pub(crate) async fn execute_remote_probe_codex(
    app: AppHandle,
    host_alias: String,
    timeout_ms: Option<u64>,
) -> Result<RemoteProbeResult, String> {
    ensure_task_storage_for_app(&app)?;
    {
        let state = app.state::<AppState>();
        storage::ensure_stores_current(&state.paths, &["profiles", "hosts"])?;
    }
    run_blocking_command("remote_probe_codex", move || {
        let state = app.state::<AppState>();
        run_remote_probe(&app, &state, host_alias, timeout_ms)
    })
    .await?
}

pub(crate) async fn execute_sample_host_resources(
    app: AppHandle,
    host_aliases: Vec<String>,
    timeout_ms: Option<u64>,
    should_record_task: bool,
) -> Result<resource_monitor::HostResourceBatchResult, String> {
    if should_record_task {
        ensure_task_storage_for_app(&app)?;
    }
    run_blocking_command("sample_host_resources", move || {
        let state = app.state::<AppState>();
        run_resource_sample(&state, host_aliases, timeout_ms, should_record_task)
    })
    .await?
}

/// Initial and manual refreshes are durable; scheduled polling is taskless.
fn run_resource_sample(
    state: &AppState,
    host_aliases: Vec<String>,
    timeout_ms: Option<u64>,
    should_record_task: bool,
) -> Result<resource_monitor::HostResourceBatchResult, String> {
    if !should_record_task {
        return Ok(resource_monitor::sample_host_resources(
            host_aliases,
            timeout_ms,
        ));
    }

    let task_id = format!("task-resource-sample-{}", timestamp_millis());
    let mut task = jobs::begin_task(
        &state.task_store,
        state.task_event_sink.as_ref(),
        &task_id,
        "resource-monitor",
        &format!("{} host(s)", host_aliases.len()),
        "Sample host resources",
    )?;
    let result = resource_monitor::sample_host_resources(host_aliases, timeout_ms);
    let (total, partial, failed) = result.outcome_counts();
    task.status = if partial > 0 || failed > 0 {
        TaskStatus::Failed
    } else {
        TaskStatus::Success
    };
    task.ended_at = Some(timestamp_label());
    task.summary = format!("Sampled {total} host(s): {partial} partial, {failed} failed.");
    task.logs.push(basic_log(
        &task_id,
        task.logs.len() + 1,
        if partial > 0 || failed > 0 {
            TaskLogLevel::Warn
        } else {
            TaskLogLevel::Info
        },
        &task.summary,
    ));
    jobs::persist_task(&state.task_store, state.task_event_sink.as_ref(), &task)?;
    Ok(result)
}

pub(crate) async fn execute_remote_manage_codex(
    app: AppHandle,
    host_alias: String,
    action: RemoteCodexAction,
    timeout_ms: Option<u64>,
    request_id: Option<String>,
) -> Result<RemoteCodexMaintenanceResult, String> {
    ensure_task_storage_for_app(&app)?;
    run_blocking_command("remote_manage_codex", move || {
        let state = app.state::<AppState>();
        run_remote_manage_codex(&app, &state, host_alias, action, timeout_ms, request_id)
    })
    .await?
}

pub(crate) async fn execute_refresh_latest_codex_version(
    app: AppHandle,
    force: Option<bool>,
    timeout_ms: Option<u64>,
) -> Result<LatestCodexVersion, String> {
    ensure_task_storage_for_app(&app)?;
    run_blocking_command("refresh_latest_codex_version", move || {
        let state = app.state::<AppState>();
        jobs::run_observed_operation(
            &state.task_store,
            state.task_event_sink.as_ref(),
            "Refresh latest Codex version",
            "updater",
            || run_refresh_latest_codex_version(&state, force.unwrap_or(false), timeout_ms),
            |result| match result.error.as_ref() {
                Some(error) => (TaskStatus::Failed, redact_error_text(error)),
                None => (
                    TaskStatus::Success,
                    format!(
                        "Latest Codex version is {} via {}.",
                        result.version.as_deref().unwrap_or("unknown"),
                        result.source
                    ),
                ),
            },
        )
    })
    .await?
}

pub(crate) async fn execute_get_local_codex_status() -> Result<LocalCodexStatus, String> {
    run_blocking_command("get_local_codex_status", run_get_local_codex_status).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_resource_sample_does_not_persist_a_task() {
        let state = AppState::new(storage::TaskStore::in_memory());
        let result = run_resource_sample(&state, vec!["bad alias!".into()], Some(3_000), false)
            .expect("run automatic resource sample");

        assert_eq!(result.outcome_counts().0, 1);
        assert!(state
            .task_store
            .list(10)
            .expect("read automatic sample tasks")
            .is_empty());
    }

    #[test]
    fn user_requested_resource_sample_persists_one_task() {
        let state = AppState::new(storage::TaskStore::in_memory());
        run_resource_sample(&state, vec!["bad alias!".into()], Some(3_000), true)
            .expect("run recorded resource sample");

        let tasks = state
            .task_store
            .list(10)
            .expect("read recorded sample tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].action, "Sample host resources");
        assert!(matches!(tasks[0].status, TaskStatus::Failed));
    }
}
