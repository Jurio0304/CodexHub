use crate::*;

pub(crate) fn merge_discovered_hosts(state: &AppState) -> Result<(), String> {
    let discovered_hosts = ssh::list_ssh_config_hosts()?;
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");
    for discovered in discovered_hosts {
        merge_discovered_host(&mut hosts, discovered);
    }
    Ok(())
}

pub(crate) fn merge_discovered_host(hosts: &mut Vec<Host>, discovered: ssh::SshConfigHost) {
    if let Some(existing) = hosts
        .iter_mut()
        .find(|host| host.host_alias.eq_ignore_ascii_case(&discovered.alias))
    {
        existing.source = discovered.source.clone();
        existing.address = host_address(&discovered);
        existing.port = discovered.port;
        if !discovered.user.is_empty() {
            existing.username = discovered.user.clone();
        }
        existing.auth_method = if discovered.identity_file.is_empty() {
            AuthMethod::Agent
        } else {
            AuthMethod::SshKey
        };
        ensure_tag(&mut existing.tags, "ssh-config");
        ensure_tag(&mut existing.tags, &discovered.source);
        return;
    }

    let mut tags = vec!["ssh-config".into(), discovered.source.clone()];
    if discovered.managed {
        tags.push("codexhub-managed".into());
    }

    hosts.insert(
        0,
        Host {
            id: discovered_host_id(&discovered.alias),
            name: discovered.alias.clone(),
            host_alias: discovered.alias.clone(),
            source: discovered.source.clone(),
            address: host_address(&discovered),
            port: discovered.port,
            username: discovered.user.clone(),
            auth_method: if discovered.identity_file.is_empty() {
                AuthMethod::Agent
            } else {
                AuthMethod::SshKey
            },
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
            tags,
            last_seen: "not tested".into(),
            latency_ms: None,
        },
    );
}

pub(crate) fn host_address(host: &ssh::SshConfigHost) -> String {
    if host.host_name.is_empty() {
        host.alias.clone()
    } else {
        host.host_name.clone()
    }
}

pub(crate) fn discovered_host_id(alias: &str) -> String {
    let safe = alias
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    format!("ssh-{safe}")
}

pub(crate) fn ensure_tag(tags: &mut Vec<String>, tag: &str) {
    if !tags.iter().any(|item| item == tag) {
        tags.push(tag.to_string());
    }
}

pub(crate) fn run_ssh_check(
    state: &AppState,
    host_alias: String,
    timeout_ms: Option<u64>,
) -> Result<SshCheckResult, String> {
    let alias_result = ssh::validate_ssh_alias(&host_alias);
    let alias = alias_result
        .clone()
        .unwrap_or_else(|_| host_alias.trim().to_string());
    let timeout = ssh::normalize_timeout_ms(timeout_ms);
    let task_id = format!("task-ssh-{}", timestamp_millis());
    let host_name = host_name_for_alias(state, &alias);
    let host_id = host_id_for_alias(state, &alias);
    let running = jobs::begin_task(
        &state.task_store,
        state.task_event_sink.as_ref(),
        &task_id,
        &host_id,
        &host_name,
        "Test SSH connection",
    )?;
    let output = match alias_result {
        Ok(valid_alias) => ssh::run_ssh_echo_ok(&valid_alias, timeout).unwrap_or_else(|error| {
            failed_command_output(
                format!("ssh {valid_alias} echo ok"),
                format!("Could not start ssh: {error}"),
            )
        }),
        Err(error) => failed_command_output("ssh <invalid-alias> echo ok".into(), error),
    };

    let ok = output.success() && output.stdout.trim() == "ok";
    let message = ssh_check_message(&alias, &output, ok, timeout);
    let mut logs = running.logs;
    logs.push(command_log(
        &task_id,
        logs.len() + 1,
        if ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        &message,
        &output,
    ));
    let task = TaskRun {
        id: task_id.clone(),
        host_id: host_id.clone(),
        host_name: host_name.clone(),
        action: "Test SSH connection".into(),
        status: if ok {
            TaskStatus::Success
        } else {
            TaskStatus::Failed
        },
        started_at: running.started_at,
        ended_at: Some(timestamp_label()),
        summary: message.clone(),
        logs,
    };

    update_host_check(state, &alias, ok, output.duration_ms);
    record_task(state, task.clone())?;

    Ok(SshCheckResult {
        host_alias: alias,
        ok,
        latency_ms: if ok { Some(output.duration_ms) } else { None },
        message,
        task,
    })
}

pub(crate) fn run_ssh_bootstrap(
    app: &AppHandle,
    state: &AppState,
    draft: ssh::SshHostDraft,
    password: String,
    timeout_ms: Option<u64>,
    request_id: Option<String>,
) -> Result<SshBootstrapResult, String> {
    let host = ssh::prepare_ssh_config_host(draft)?;
    let write_result = pending_ssh_config_write_result(
        &host,
        "SSH config will be written after password login and key setup succeed.",
    );
    run_bootstrap_for_config_host(
        Some(app),
        state,
        host,
        write_result,
        password,
        timeout_ms,
        request_id,
        true,
    )
}

pub(crate) fn run_existing_ssh_bootstrap(
    state: &AppState,
    host_alias: String,
    password: String,
    timeout_ms: Option<u64>,
) -> Result<SshBootstrapResult, String> {
    let alias = ssh::validate_ssh_alias(&host_alias)?;
    let mut host = ssh::list_ssh_config_hosts()?
        .into_iter()
        .find(|host| host.alias.eq_ignore_ascii_case(&alias))
        .ok_or_else(|| format!("Host {alias} was not found in SSH config."))?;
    if host.identity_file.trim().is_empty() {
        host.identity_file = ssh::get_ssh_status()?.preferred_identity_file;
    }
    let write_result = ssh::SshConfigWriteResult {
        changed: false,
        action: "unchanged".into(),
        config_path: ssh::get_ssh_status()?.config_path,
        backup_path: None,
        host: Some(host.clone()),
        message: format!("Using existing SSH config Host {alias}; config was not modified."),
    };
    run_bootstrap_for_config_host(
        None,
        state,
        host,
        write_result,
        password,
        timeout_ms,
        None,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_bootstrap_for_config_host(
    app: Option<&AppHandle>,
    state: &AppState,
    host: ssh::SshConfigHost,
    mut write_result: ssh::SshConfigWriteResult,
    password: String,
    timeout_ms: Option<u64>,
    request_id: Option<String>,
    write_config_before_verify: bool,
) -> Result<SshBootstrapResult, String> {
    let timeout = ssh::normalize_timeout_ms(timeout_ms);
    let alias = host.alias.clone();
    let request_id =
        request_id.unwrap_or_else(|| format!("bootstrap-{alias}-{}", timestamp_millis()));
    let task_id = format!("task-bootstrap-{}", timestamp_millis());
    let host_name = host_name_for_alias(state, &alias);
    let host_id = host_id_for_alias(state, &alias);
    let running = jobs::begin_task(
        &state.task_store,
        state.task_event_sink.as_ref(),
        &task_id,
        &host_id,
        &host_name,
        "Bootstrap SSH key",
    )?;
    let mut logs = running.logs;
    let mut private_key_path = host.identity_file.clone();
    let mut public_key_path = String::new();

    let key_pair = match ssh::ensure_identity_keypair(&host.identity_file) {
        Ok(key_pair) => {
            private_key_path = key_pair.private_path.display().to_string();
            public_key_path = key_pair.public_path.display().to_string();
            logs.push(basic_log(
                &task_id,
                logs.len() + 1,
                TaskLogLevel::Info,
                if key_pair.generated {
                    "Generated a local Ed25519 identity key for this host."
                } else {
                    "Using an existing local identity key for this host."
                },
            ));
            key_pair
        }
        Err(error) => {
            let output = failed_command_output("ensure local identity key".into(), error);
            let message = format!(
                "SSH bootstrap for {alias} failed: {}",
                command_detail(&output)
            );
            logs.push(command_log(
                &task_id,
                logs.len() + 1,
                TaskLogLevel::Error,
                &message,
                &output,
            ));
            let task = bootstrap_task(
                &task_id,
                &host_id,
                &host_name,
                TaskStatus::Failed,
                &message,
                logs,
            );
            record_task(state, task.clone())?;
            return Ok(SshBootstrapResult {
                host_alias: alias,
                ok: false,
                latency_ms: None,
                message,
                generated_key: false,
                private_key_path,
                public_key_path,
                write_result,
                task,
            });
        }
    };

    let bootstrap_draft = ssh::SshHostDraft {
        alias: host.alias.clone(),
        host_name: host.host_name.clone(),
        port: host.port,
        user: host.user.clone(),
        identity_file: host.identity_file.clone(),
    };

    let remote_outputs = ssh::run_password_bootstrap_steps(
        &bootstrap_draft,
        &password,
        &key_pair.public_key,
        timeout,
        |step| {
            if let Some(handle) = app {
                emit_bootstrap_progress(
                    handle,
                    state,
                    &task_id,
                    &request_id,
                    &alias,
                    bootstrap_progress_step(step),
                    "running",
                    bootstrap_step_running_message(step),
                    None,
                );
            }
        },
        |step, output| {
            if let Some(handle) = app {
                let ok = output.success();
                let message = bootstrap_step_message(step, &alias, output, ok);
                emit_bootstrap_progress(
                    handle,
                    state,
                    &task_id,
                    &request_id,
                    &alias,
                    bootstrap_progress_step(step),
                    if ok { "success" } else { "failed" },
                    &message,
                    Some(output),
                );
            }
        },
    );

    for (step, output) in &remote_outputs {
        let ok = output.success();
        let message = bootstrap_step_message(*step, &alias, output, ok);
        logs.push(command_log(
            &task_id,
            logs.len() + 1,
            if ok {
                TaskLogLevel::Info
            } else {
                TaskLogLevel::Error
            },
            &message,
            output,
        ));
        if !ok {
            let message = format!(
                "SSH bootstrap for {alias} failed: {}",
                command_detail(output)
            );
            let task = bootstrap_task(
                &task_id,
                &host_id,
                &host_name,
                TaskStatus::Failed,
                &message,
                logs,
            );
            update_host_check(state, &alias, false, output.duration_ms);
            record_task(state, task.clone())?;
            return Ok(SshBootstrapResult {
                host_alias: alias,
                ok: false,
                latency_ms: None,
                message,
                generated_key: key_pair.generated,
                private_key_path,
                public_key_path,
                write_result,
                task,
            });
        }
    }

    if remote_outputs.is_empty() {
        let output = failed_command_output(
            "ssh password login".into(),
            "Password bootstrap did not start.".into(),
        );
        let message = format!(
            "SSH bootstrap for {alias} failed: {}",
            command_detail(&output)
        );
        logs.push(command_log(
            &task_id,
            logs.len() + 1,
            TaskLogLevel::Error,
            &message,
            &output,
        ));
        let task = bootstrap_task(
            &task_id,
            &host_id,
            &host_name,
            TaskStatus::Failed,
            &message,
            logs,
        );
        record_task(state, task.clone())?;
        return Ok(SshBootstrapResult {
            host_alias: alias,
            ok: false,
            latency_ms: None,
            message,
            generated_key: key_pair.generated,
            private_key_path,
            public_key_path,
            write_result,
            task,
        });
    }

    if let Some(handle) = app {
        emit_bootstrap_progress(
            handle,
            state,
            &task_id,
            &request_id,
            &alias,
            "verify_alias_login",
            "running",
            "Writing CodexHub-managed SSH config and testing alias login...",
            None,
        );
    }

    if write_config_before_verify {
        let draft_for_write = ssh::SshHostDraft {
            alias: host.alias.clone(),
            host_name: host.host_name.clone(),
            port: host.port,
            user: host.user.clone(),
            identity_file: host.identity_file.clone(),
        };
        match ssh::upsert_ssh_config_host(draft_for_write) {
            Ok(result) => {
                write_result = result;
                let message = if let Some(backup) = &write_result.backup_path {
                    format!("SSH config saved; backup: {backup}")
                } else {
                    write_result.message.clone()
                };
                logs.push(basic_log(
                    &task_id,
                    logs.len() + 1,
                    TaskLogLevel::Info,
                    &message,
                ));
            }
            Err(error) => {
                let output =
                    failed_command_output("write CodexHub-managed SSH config".into(), error);
                let message = format!(
                    "Could not write SSH config for {alias}: {}",
                    command_detail(&output)
                );
                logs.push(command_log(
                    &task_id,
                    logs.len() + 1,
                    TaskLogLevel::Error,
                    &message,
                    &output,
                ));
                if let Some(handle) = app {
                    emit_bootstrap_progress(
                        handle,
                        state,
                        &task_id,
                        &request_id,
                        &alias,
                        "verify_alias_login",
                        "failed",
                        &message,
                        Some(&output),
                    );
                }
                let task = bootstrap_task(
                    &task_id,
                    &host_id,
                    &host_name,
                    TaskStatus::Failed,
                    &message,
                    logs,
                );
                record_task(state, task.clone())?;
                return Ok(SshBootstrapResult {
                    host_alias: alias,
                    ok: false,
                    latency_ms: None,
                    message,
                    generated_key: key_pair.generated,
                    private_key_path,
                    public_key_path,
                    write_result,
                    task,
                });
            }
        }
    } else {
        logs.push(basic_log(
            &task_id,
            logs.len() + 1,
            TaskLogLevel::Info,
            "Using existing SSH config; unmanaged user blocks were not modified.",
        ));
    }

    let check_output = ssh::run_ssh_echo_ok(&alias, timeout).unwrap_or_else(|error| {
        failed_command_output(
            format!("ssh {alias} echo ok"),
            format!("Could not start ssh: {error}"),
        )
    });
    let ok = check_output.success() && check_output.stdout.trim() == "ok";
    let mut message = if ok {
        format!("SSH bootstrap for {alias} completed; key login returned ok.")
    } else {
        format!(
            "SSH bootstrap installed the key, but key-login test failed: {}",
            command_detail(&check_output)
        )
    };
    logs.push(command_log(
        &task_id,
        logs.len() + 1,
        if ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        &message,
        &check_output,
    ));

    if !ok && write_config_before_verify && write_result.action == "added" {
        match ssh::delete_ssh_config_host(alias.clone()) {
            Ok(mut rollback) => {
                rollback.action = "rolled_back".into();
                rollback.message = format!(
                    "Rolled back CodexHub-managed Host {alias} after failed key-login verification."
                );
                let rollback_message = if let Some(backup) = &rollback.backup_path {
                    format!("{} Backup: {backup}", rollback.message)
                } else {
                    rollback.message.clone()
                };
                logs.push(basic_log(
                    &task_id,
                    logs.len() + 1,
                    TaskLogLevel::Warn,
                    &rollback_message,
                ));
                message = format!("{message} {rollback_message}");
                write_result = rollback;
            }
            Err(error) => {
                let output =
                    failed_command_output("rollback CodexHub-managed SSH config".into(), error);
                let rollback_message = format!(
                    "Could not roll back CodexHub-managed Host {alias}: {}",
                    command_detail(&output)
                );
                logs.push(command_log(
                    &task_id,
                    logs.len() + 1,
                    TaskLogLevel::Error,
                    &rollback_message,
                    &output,
                ));
                message = format!("{message} {rollback_message}");
            }
        }
    }

    if let Some(handle) = app {
        emit_bootstrap_progress(
            handle,
            state,
            &task_id,
            &request_id,
            &alias,
            "verify_alias_login",
            if ok { "success" } else { "failed" },
            &message,
            Some(&check_output),
        );
    }

    let task = bootstrap_task(
        &task_id,
        &host_id,
        &host_name,
        if ok {
            TaskStatus::Success
        } else {
            TaskStatus::Failed
        },
        &message,
        logs,
    );

    if ok {
        merge_discovered_hosts(state)?;
    }
    update_host_check(state, &alias, ok, check_output.duration_ms);
    if ok {
        if let Some(handle) = app {
            save_current_hosts(handle, state)?;
        }
    }
    record_task(state, task.clone())?;

    Ok(SshBootstrapResult {
        host_alias: alias,
        ok,
        latency_ms: if ok {
            Some(check_output.duration_ms)
        } else {
            None
        },
        message,
        generated_key: key_pair.generated,
        private_key_path,
        public_key_path,
        write_result,
        task,
    })
}
pub(crate) fn pending_ssh_config_write_result(
    host: &ssh::SshConfigHost,
    message: &str,
) -> ssh::SshConfigWriteResult {
    ssh::SshConfigWriteResult {
        changed: false,
        action: "pending".into(),
        config_path: ssh::get_ssh_status()
            .map(|status| status.config_path)
            .unwrap_or_else(|_| "%USERPROFILE%\\.ssh\\config".into()),
        backup_path: None,
        host: Some(host.clone()),
        message: message.into(),
    }
}

pub(crate) fn emit_bootstrap_progress(
    app: &AppHandle,
    state: &AppState,
    task_id: &str,
    request_id: &str,
    host_alias: &str,
    step: &str,
    status: &str,
    message: &str,
    output: Option<&ssh::SshCommandOutput>,
) {
    let safe_message = ssh::redact_sensitive(message);
    let level = if status == "failed" {
        TaskLogLevel::Error
    } else {
        TaskLogLevel::Info
    };
    if let Err(error) = jobs::append_message(
        &state.task_store,
        state.task_event_sink.as_ref(),
        task_id,
        level,
        &safe_message,
    ) {
        eprintln!(
            "Could not persist sanitized SSH progress: {}",
            redact_error_text(&error)
        );
    }
    let payload = SshBootstrapProgressEvent {
        request_id: request_id.to_string(),
        host_alias: host_alias.to_string(),
        step: step.to_string(),
        status: status.to_string(),
        message: safe_message,
        detail: output.map(command_detail),
        stdout: output.map(|item| ssh::redact_sensitive(&item.stdout)),
        stderr: output.map(|item| ssh::redact_sensitive(&item.stderr)),
        exit_code: output.and_then(|item| item.exit_code),
        duration_ms: output.map(|item| item.duration_ms),
        timed_out: output.map(|item| item.timed_out),
    };
    if let Err(error) = app.emit("ssh-bootstrap-progress", payload) {
        eprintln!(
            "Could not emit sanitized SSH progress: {}",
            redact_error_text(&error.to_string())
        );
    }
}

pub(crate) fn bootstrap_progress_step(step: ssh::PasswordBootstrapStep) -> &'static str {
    match step {
        ssh::PasswordBootstrapStep::PasswordLogin => "password_login",
        ssh::PasswordBootstrapStep::InstallPublicKey => "install_public_key",
        ssh::PasswordBootstrapStep::SetPermissions => "set_permissions",
    }
}

pub(crate) fn bootstrap_step_running_message(step: ssh::PasswordBootstrapStep) -> &'static str {
    match step {
        ssh::PasswordBootstrapStep::PasswordLogin => {
            "Logging in to the remote host with the one-time password..."
        }
        ssh::PasswordBootstrapStep::InstallPublicKey => {
            "Installing the local public key into remote authorized_keys..."
        }
        ssh::PasswordBootstrapStep::SetPermissions => {
            "Setting remote ~/.ssh and authorized_keys permissions..."
        }
    }
}

pub(crate) fn bootstrap_step_message(
    step: ssh::PasswordBootstrapStep,
    alias: &str,
    output: &ssh::SshCommandOutput,
    ok: bool,
) -> String {
    match (step, ok) {
        (ssh::PasswordBootstrapStep::PasswordLogin, true) => {
            format!("Password login to {alias} succeeded.")
        }
        (ssh::PasswordBootstrapStep::PasswordLogin, false) => {
            format!(
                "Password login to {alias} failed: {}",
                command_detail(output)
            )
        }
        (ssh::PasswordBootstrapStep::InstallPublicKey, true) => {
            "Installed or confirmed the local public key in remote authorized_keys.".into()
        }
        (ssh::PasswordBootstrapStep::InstallPublicKey, false) => {
            format!(
                "Could not install public key on {alias}: {}",
                command_detail(output)
            )
        }
        (ssh::PasswordBootstrapStep::SetPermissions, true) => {
            "Remote ~/.ssh permissions were set.".into()
        }
        (ssh::PasswordBootstrapStep::SetPermissions, false) => {
            format!(
                "Could not set remote SSH permissions on {alias}: {}",
                command_detail(output)
            )
        }
    }
}

pub(crate) fn bootstrap_task(
    task_id: &str,
    host_id: &str,
    host_name: &str,
    status: TaskStatus,
    summary: &str,
    logs: Vec<TaskLog>,
) -> TaskRun {
    TaskRun {
        id: task_id.to_string(),
        host_id: host_id.to_string(),
        host_name: host_name.to_string(),
        action: "Bootstrap SSH key".into(),
        status,
        started_at: timestamp_label(),
        ended_at: Some(timestamp_label()),
        summary: summary.to_string(),
        logs,
    }
}

pub(crate) fn run_remote_probe(
    app: &AppHandle,
    state: &AppState,
    host_alias: String,
    timeout_ms: Option<u64>,
) -> Result<RemoteProbeResult, String> {
    let timeout = ssh::normalize_timeout_ms(timeout_ms);
    let alias_result = ssh::validate_ssh_alias(&host_alias);
    let alias = alias_result
        .clone()
        .unwrap_or_else(|_| host_alias.trim().to_string());
    let task_id = format!("task-probe-{}", timestamp_millis());
    let host_name = host_name_for_alias(state, &alias);
    let host_id = host_id_for_alias(state, &alias);
    let running = jobs::begin_task(
        &state.task_store,
        state.task_event_sink.as_ref(),
        &task_id,
        &host_id,
        &host_name,
        "Probe remote system",
    )?;
    let check_output = match alias_result {
        Ok(valid_alias) => ssh::run_ssh_echo_ok(&valid_alias, timeout).unwrap_or_else(|error| {
            failed_command_output(
                format!("ssh {valid_alias} echo ok"),
                format!("Could not start ssh: {error}"),
            )
        }),
        Err(error) => failed_command_output("ssh <invalid-alias> echo ok".into(), error),
    };
    let check_ok = check_output.success() && check_output.stdout.trim() == "ok";
    let check_message = ssh_check_message(&alias, &check_output, check_ok, timeout);
    let started_at = running.started_at;
    let mut logs = running.logs;
    logs.push(command_log(
        &task_id,
        logs.len() + 1,
        if check_ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        &check_message,
        &check_output,
    ));

    if !check_ok {
        update_host_check(state, &alias, false, check_output.duration_ms);
        let task = TaskRun {
            id: task_id.clone(),
            host_id,
            host_name,
            action: "Probe remote system".into(),
            status: TaskStatus::Failed,
            started_at: started_at.clone(),
            ended_at: Some(timestamp_label()),
            summary: format!("Remote probe skipped because SSH check failed: {check_message}"),
            logs,
        };
        record_task(state, task.clone())?;
        return Ok(RemoteProbeResult {
            host_alias: alias,
            ssh_status: HostStatus::Offline,
            latency_ms: None,
            os: "Unknown".into(),
            arch: "Unknown".into(),
            shell: "Unknown".into(),
            path: None,
            path_has_local_bin: false,
            codex_command_available: false,
            codex_installed: false,
            codex_path: None,
            codex_version: "not installed".into(),
            config_exists: false,
            api_config_name: "No config".into(),
            api_config_source: "none".into(),
            api_key_env_var: None,
            api_key_env_present: None,
            skills_exists: false,
            skills_count: 0,
            task,
        });
    }
    update_host_check(state, &alias, true, check_output.duration_ms);

    let codex_path_probe_script = codex_path_probe_script();
    let codex_version_probe_script = codex_version_probe_script();
    let remote_skill_count_script = remote_skill_count_script();
    let commands = vec![
        ("uname -s", "uname -s", TaskLogLevel::Info),
        ("uname -m", "uname -m", TaskLogLevel::Info),
        (
            "echo $SHELL",
            "printf '%s\n' \"${SHELL:-$(getent passwd \"$(id -un)\" 2>/dev/null | cut -d: -f7)}\"",
            TaskLogLevel::Info,
        ),
        ("resolve codex", codex_path_probe_script.as_str(), TaskLogLevel::Warn),
        (
            "check codex command in PATH",
            CODEX_COMMAND_AVAILABLE_SCRIPT,
            TaskLogLevel::Warn,
        ),
        ("codex --version", codex_version_probe_script.as_str(), TaskLogLevel::Warn),
        ("echo $PATH", "printf '%s\n' \"$PATH\"", TaskLogLevel::Info),
        (
            "check ~/.codex/config.toml",
            "test -f \"$HOME/.codex/config.toml\" && printf yes || printf no",
            TaskLogLevel::Info,
        ),
        (
            "read ~/.codex/config.toml base URL",
            "if [ -f \"$HOME/.codex/config.toml\" ]; then sed -n -E 's/^[[:space:]]*(openai_base_url|base_url)[[:space:]]*=[[:space:]]*\"([^\"]*)\".*/\\2/p' \"$HOME/.codex/config.toml\" 2>/dev/null | head -n 1; fi",
            TaskLogLevel::Info,
        ),
        (
            "read ~/.codex/config.toml API env",
            REMOTE_CONFIG_API_ENV_VAR_SCRIPT,
            TaskLogLevel::Info,
        ),
        (
            "check remote API env",
            REMOTE_API_ENV_PRESENT_SCRIPT,
            TaskLogLevel::Warn,
        ),
        (
            "check ~/.codex/skills",
            "test -d \"$HOME/.codex/skills\" && printf yes || printf no",
            TaskLogLevel::Info,
        ),
        (
            "count ~/.codex/skills",
            remote_skill_count_script,
            TaskLogLevel::Info,
        ),
    ];

    let mut outputs = Vec::new();
    let first_probe_log = logs.len() + 1;
    for (index, (label, script, failure_level)) in commands.iter().enumerate() {
        let output = ssh::run_ssh_script(&alias, script, timeout).unwrap_or_else(|error| {
            failed_command_output(
                format!("ssh {alias} {label}"),
                format!("Could not run ssh: {error}"),
            )
        });
        let level = if output.success() {
            TaskLogLevel::Info
        } else {
            (*failure_level).clone()
        };
        let message = if output.success() {
            format!("{label} completed.")
        } else if output.timed_out {
            format!("{label} timed out after {timeout} ms.")
        } else {
            format!("{label} failed: {}", command_detail(&output))
        };
        logs.push(command_log(
            &task_id,
            first_probe_log + index,
            level,
            &message,
            &output,
        ));
        outputs.push(output);
    }

    let os = stdout_or_unknown(outputs.get(0));
    let arch = stdout_or_unknown(outputs.get(1));
    let shell = stdout_or_unknown(outputs.get(2));
    let codex_path = outputs
        .get(3)
        .filter(|output| output.success())
        .map(|output| output.stdout.trim().to_string())
        .filter(|value| !value.is_empty());
    let codex_installed = codex_path.is_some();
    let codex_command_available = outputs
        .get(4)
        .map(|output| output.success())
        .unwrap_or(false);
    let codex_version = if codex_installed {
        outputs
            .get(5)
            .filter(|output| output.success())
            .map(|output| output.stdout.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "unavailable".into())
    } else {
        "not installed".into()
    };
    let path = outputs
        .get(6)
        .filter(|output| output.success())
        .map(|output| output.stdout.trim().to_string())
        .filter(|value| !value.is_empty());
    let path_has_local_bin = path_has_local_bin(path.as_deref());
    let config_exists = stdout_yes(outputs.get(7));
    let remote_config_base_url = outputs
        .get(8)
        .filter(|output| output.success())
        .map(|output| output.stdout.trim().to_string())
        .filter(|value| !value.is_empty());
    let api_key_env_var = outputs
        .get(9)
        .filter(|output| output.success())
        .map(|output| output.stdout.trim().to_string())
        .filter(|value| !value.is_empty());
    let api_key_env_present = stdout_optional_yes(outputs.get(10));
    let api_config_match =
        classify_remote_api_config(app, state, config_exists, remote_config_base_url.as_deref());
    let skills_exists = stdout_yes(outputs.get(11));
    let skills_count = outputs
        .get(12)
        .and_then(|output| output.stdout.trim().parse::<u16>().ok())
        .unwrap_or(0);
    let mut readiness_notes = Vec::new();
    if codex_installed && !codex_command_available {
        readiness_notes.push("codex command is not on the current shell PATH");
    }
    if api_key_env_present == Some(false) {
        readiness_notes.push("API environment variable is missing");
    }
    let readiness_suffix = if readiness_notes.is_empty() {
        String::new()
    } else {
        format!(" Warnings: {}.", readiness_notes.join("; "))
    };
    let summary = format!(
        "Probe completed for {alias}: {os}/{arch}, Codex {}.{readiness_suffix}",
        if codex_installed {
            codex_version.as_str()
        } else {
            "not installed"
        }
    );
    let mut task = TaskRun {
        id: task_id.clone(),
        host_id,
        host_name,
        action: "Probe remote system".into(),
        status: TaskStatus::Success,
        started_at,
        ended_at: Some(timestamp_label()),
        summary: summary.clone(),
        logs,
    };

    if let Err(error) = update_host_probe(
        app,
        state,
        &alias,
        &os,
        &arch,
        &shell,
        path.clone(),
        path_has_local_bin,
        codex_command_available,
        codex_installed,
        &codex_version,
        config_exists,
        &api_config_match,
        api_key_env_var.clone(),
        api_key_env_present,
        skills_exists,
        skills_count,
    ) {
        let safe_error = redact_error_text(&error);
        task.status = TaskStatus::Failed;
        task.summary = format!(
            "Remote probe completed, but local Host/Profile state failed to persist: {safe_error}"
        );
        task.logs.push(basic_log(
            &task_id,
            task.logs.len() + 1,
            TaskLogLevel::Error,
            &task.summary,
        ));
    }
    record_task(state, task.clone())?;

    Ok(RemoteProbeResult {
        host_alias: alias,
        ssh_status: HostStatus::Online,
        latency_ms: Some(check_output.duration_ms),
        os,
        arch,
        shell,
        path,
        path_has_local_bin,
        codex_command_available,
        codex_installed,
        codex_path,
        codex_version,
        config_exists,
        api_config_name: api_config_match.name,
        api_config_source: api_config_match.source,
        api_key_env_var,
        api_key_env_present,
        skills_exists,
        skills_count,
        task,
    })
}

pub(crate) fn run_remote_manage_codex(
    app: &AppHandle,
    state: &AppState,
    host_alias: String,
    action: RemoteCodexAction,
    timeout_ms: Option<u64>,
    request_id: Option<String>,
) -> Result<RemoteCodexMaintenanceResult, String> {
    let timeout = ssh::normalize_timeout_ms(timeout_ms.or(Some(120_000)));
    let alias_result = ssh::validate_ssh_alias(&host_alias);
    let alias = alias_result
        .clone()
        .unwrap_or_else(|_| host_alias.trim().to_string());
    let task_id = format!("task-codex-{}", timestamp_millis());
    let host_name = host_name_for_alias(state, &alias);
    let host_id = host_id_for_alias(state, &alias);
    let action_label = remote_codex_action_label(&action);
    let running = jobs::begin_task(
        &state.task_store,
        state.task_event_sink.as_ref(),
        &task_id,
        &host_id,
        &host_name,
        action_label,
    )?;
    let progress = CodexProgressContext {
        app,
        state,
        task_id: &task_id,
        request_id: request_id.as_deref(),
        host_alias: &alias,
        action: &action,
    };

    emit_remote_codex_progress(
        Some(&progress),
        "ssh-check",
        "running",
        format!("Checking SSH connection to {alias}."),
        None,
        None,
        None,
        None,
        None,
        None,
    );
    let check_output = match alias_result {
        Ok(valid_alias) => ssh::run_ssh_echo_ok(&valid_alias, timeout).unwrap_or_else(|error| {
            failed_command_output(
                format!("ssh {valid_alias} echo ok"),
                format!("Could not start ssh: {error}"),
            )
        }),
        Err(error) => failed_command_output("ssh <invalid-alias> echo ok".into(), error),
    };
    let check_ok = check_output.success() && check_output.stdout.trim() == "ok";
    let check_message = ssh_check_message(&alias, &check_output, check_ok, timeout);
    let mut logs = running.logs;
    logs.push(command_log(
        &task_id,
        logs.len() + 1,
        if check_ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        &check_message,
        &check_output,
    ));
    emit_remote_codex_progress_for_output(
        Some(&progress),
        "ssh-check",
        if check_ok { "success" } else { "failed" },
        &check_message,
        &check_output,
    );

    if !check_ok {
        update_host_check(state, &alias, false, check_output.duration_ms);
        let message = format!("{action_label} skipped because SSH check failed: {check_message}");
        let task = codex_maintenance_task(
            &task_id,
            &host_id,
            &host_name,
            action_label,
            TaskStatus::Failed,
            &message,
            logs,
        );
        emit_remote_codex_progress(
            Some(&progress),
            "summary",
            "failed",
            message.clone(),
            None,
            None,
            None,
            None,
            None,
            None,
        );
        record_task(state, task.clone())?;
        return Ok(RemoteCodexMaintenanceResult {
            host_alias: alias.clone(),
            ok: false,
            action: action.clone(),
            before_version: None,
            after_version: None,
            codex_path: None,
            codex_command_available: false,
            install_method: None,
            path_changed: false,
            shell_config_path: None,
            backup_path: None,
            message,
            task,
        });
    }
    update_host_check(state, &alias, true, check_output.duration_ms);

    let mut next_log_index = logs.len() + 1;
    let before_path = run_codex_step(
        &alias,
        &task_id,
        &mut logs,
        &mut next_log_index,
        "resolve existing codex",
        &codex_path_probe_script(),
        timeout,
        TaskLogLevel::Warn,
        Some(&progress),
    );
    let before_command_available_output = run_codex_step(
        &alias,
        &task_id,
        &mut logs,
        &mut next_log_index,
        "check existing codex command in PATH",
        CODEX_COMMAND_AVAILABLE_SCRIPT,
        timeout,
        TaskLogLevel::Warn,
        Some(&progress),
    );
    let before_command_available = before_command_available_output.success();
    let before_version_output = run_codex_step(
        &alias,
        &task_id,
        &mut logs,
        &mut next_log_index,
        "existing codex --version",
        &codex_version_probe_script(),
        timeout,
        TaskLogLevel::Warn,
        Some(&progress),
    );
    let before_version = output_trimmed(&before_version_output);

    if action == RemoteCodexAction::CheckVersion {
        let path_output = run_codex_step(
            &alias,
            &task_id,
            &mut logs,
            &mut next_log_index,
            "echo $PATH",
            "printf '%s\n' \"$PATH\"",
            timeout,
            TaskLogLevel::Info,
            Some(&progress),
        );
        let codex_path = output_trimmed(&before_path);
        let installed = codex_path.is_some() && before_version.is_some();
        let ok = installed && before_command_available;
        let version_label = before_version
            .clone()
            .unwrap_or_else(|| "not installed".into());
        let message = if ok {
            format!("Codex is available on {alias}: {version_label}.")
        } else if installed {
            format!("Codex is installed on {alias}: {version_label}, but `codex` is not available on the current shell PATH.")
        } else {
            format!("Codex is not available on {alias}.")
        };
        let task = codex_maintenance_task(
            &task_id,
            &host_id,
            &host_name,
            action_label,
            if ok {
                TaskStatus::Success
            } else {
                TaskStatus::Failed
            },
            &message,
            logs,
        );
        update_host_codex_status(
            state,
            &alias,
            installed,
            &version_label,
            path_has_local_bin(output_trimmed(&path_output).as_deref()),
            before_command_available,
        );
        emit_remote_codex_progress(
            Some(&progress),
            "summary",
            if ok { "success" } else { "failed" },
            message.clone(),
            None,
            None,
            None,
            None,
            None,
            None,
        );
        record_task(state, task.clone())?;
        return Ok(RemoteCodexMaintenanceResult {
            host_alias: alias.clone(),
            ok,
            action: action.clone(),
            before_version: before_version.clone(),
            after_version: before_version,
            codex_path,
            codex_command_available: before_command_available,
            install_method: None,
            path_changed: false,
            shell_config_path: None,
            backup_path: None,
            message,
            task,
        });
    }

    if action == RemoteCodexAction::Uninstall {
        let uninstall_output = run_codex_step(
            &alias,
            &task_id,
            &mut logs,
            &mut next_log_index,
            action_label,
            CODEX_UNINSTALL_SCRIPT,
            timeout,
            TaskLogLevel::Error,
            Some(&progress),
        );
        let uninstall_method = marker_value(&uninstall_output.stdout, "CODEXHUB_UNINSTALL_METHOD")
            .filter(|value| value != "unsupported");
        let backup_path = marker_value(&uninstall_output.stdout, "CODEXHUB_BACKUP_PATH");
        let after_path = run_codex_step(
            &alias,
            &task_id,
            &mut logs,
            &mut next_log_index,
            "resolve codex after uninstall",
            &codex_path_probe_script(),
            timeout,
            TaskLogLevel::Warn,
            Some(&progress),
        );
        let after_command_available_output = run_codex_step(
            &alias,
            &task_id,
            &mut logs,
            &mut next_log_index,
            "check codex command in PATH after uninstall",
            CODEX_COMMAND_AVAILABLE_SCRIPT,
            timeout,
            TaskLogLevel::Warn,
            Some(&progress),
        );
        let after_version_output = run_codex_step(
            &alias,
            &task_id,
            &mut logs,
            &mut next_log_index,
            "codex --version after uninstall",
            &codex_version_probe_script(),
            timeout,
            TaskLogLevel::Warn,
            Some(&progress),
        );
        let path_output = run_codex_step(
            &alias,
            &task_id,
            &mut logs,
            &mut next_log_index,
            "echo $PATH after uninstall",
            "printf '%s\n' \"$PATH\"",
            timeout,
            TaskLogLevel::Info,
            Some(&progress),
        );

        let codex_path = output_trimmed(&after_path);
        let codex_command_available = after_command_available_output.success();
        let after_version = output_trimmed(&after_version_output);
        let installed = codex_path.is_some() || after_version.is_some();
        let ok = uninstall_output.success() && !installed && !codex_command_available;
        let version_label = after_version.clone().unwrap_or_else(|| {
            if codex_path.is_some() {
                "unknown".into()
            } else {
                "not installed".into()
            }
        });
        let message = if ok {
            format!("{action_label} completed on {alias}; Codex is no longer available.")
        } else if uninstall_output.success() {
            format!(
                "{action_label} completed on {alias}, but another Codex command is still available: {version_label}."
            )
        } else {
            format!(
                "{action_label} failed on {alias}: {}",
                command_detail(&uninstall_output)
            )
        };
        let task = codex_maintenance_task(
            &task_id,
            &host_id,
            &host_name,
            action_label,
            if ok {
                TaskStatus::Success
            } else {
                TaskStatus::Failed
            },
            &message,
            logs,
        );
        update_host_codex_status(
            state,
            &alias,
            installed,
            &version_label,
            path_has_local_bin(output_trimmed(&path_output).as_deref()),
            codex_command_available,
        );
        emit_remote_codex_progress(
            Some(&progress),
            "summary",
            if ok { "success" } else { "failed" },
            message.clone(),
            uninstall_method.clone(),
            None,
            None,
            None,
            None,
            None,
        );
        record_task(state, task.clone())?;
        return Ok(RemoteCodexMaintenanceResult {
            host_alias: alias.clone(),
            ok,
            action: action.clone(),
            before_version,
            after_version,
            codex_path,
            codex_command_available,
            install_method: uninstall_method,
            path_changed: false,
            shell_config_path: None,
            backup_path,
            message,
            task,
        });
    }

    let path_repair_output = run_codex_step(
        &alias,
        &task_id,
        &mut logs,
        &mut next_log_index,
        "ensure ~/.local/bin and shell PATH",
        CODEX_PATH_REPAIR_SCRIPT,
        timeout,
        TaskLogLevel::Error,
        Some(&progress),
    );
    let path_changed = marker_value(&path_repair_output.stdout, "CODEXHUB_PATH_CHANGED")
        .map(|value| value == "yes")
        .unwrap_or(false);
    let shell_config_path = marker_value(&path_repair_output.stdout, "CODEXHUB_SHELL_CONFIG_PATH");
    let backup_path = marker_value(&path_repair_output.stdout, "CODEXHUB_BACKUP_PATH");

    let mut install_output = run_codex_step(
        &alias,
        &task_id,
        &mut logs,
        &mut next_log_index,
        action_label,
        CODEX_INSTALL_SCRIPT,
        timeout,
        TaskLogLevel::Error,
        Some(&progress),
    );
    if !install_output.success() {
        install_output = run_local_upload_codex_fallback(
            &state.paths,
            &alias,
            &task_id,
            &mut logs,
            &mut next_log_index,
            timeout,
            Some(&progress),
        );
    }
    let install_method = marker_value(&install_output.stdout, "CODEXHUB_INSTALL_METHOD")
        .filter(|value| value != "failed");

    let after_path = run_codex_step(
        &alias,
        &task_id,
        &mut logs,
        &mut next_log_index,
        "resolve codex after maintenance",
        &codex_path_probe_script(),
        timeout,
        TaskLogLevel::Error,
        Some(&progress),
    );
    let after_command_available_output = run_codex_step(
        &alias,
        &task_id,
        &mut logs,
        &mut next_log_index,
        "check codex command in PATH after maintenance",
        CODEX_COMMAND_AVAILABLE_SCRIPT,
        timeout,
        TaskLogLevel::Warn,
        Some(&progress),
    );
    let after_version_output = run_codex_step(
        &alias,
        &task_id,
        &mut logs,
        &mut next_log_index,
        "codex --version after maintenance",
        &codex_version_probe_script(),
        timeout,
        TaskLogLevel::Error,
        Some(&progress),
    );
    let path_output = run_codex_step(
        &alias,
        &task_id,
        &mut logs,
        &mut next_log_index,
        "echo $PATH after maintenance",
        "printf '%s\n' \"$PATH\"",
        timeout,
        TaskLogLevel::Info,
        Some(&progress),
    );

    let codex_path = output_trimmed(&after_path);
    let codex_command_available = after_command_available_output.success();
    let after_version = output_trimmed(&after_version_output);
    let installed = install_output.success() && codex_path.is_some() && after_version.is_some();
    let ok = installed && codex_command_available;
    let version_label = after_version
        .clone()
        .unwrap_or_else(|| "not installed".into());
    let message = if ok {
        format!("{action_label} completed on {alias}: {version_label}.")
    } else if installed {
        format!("{action_label} installed Codex on {alias}: {version_label}, but `codex` is not available on the current shell PATH.")
    } else {
        format!(
            "{action_label} failed on {alias}: {}",
            command_detail(&install_output)
        )
    };
    let task = codex_maintenance_task(
        &task_id,
        &host_id,
        &host_name,
        action_label,
        if ok {
            TaskStatus::Success
        } else {
            TaskStatus::Failed
        },
        &message,
        logs,
    );
    update_host_codex_status(
        state,
        &alias,
        installed,
        &version_label,
        path_has_local_bin(output_trimmed(&path_output).as_deref()),
        codex_command_available,
    );
    emit_remote_codex_progress(
        Some(&progress),
        "summary",
        if ok { "success" } else { "failed" },
        message.clone(),
        install_method.clone(),
        None,
        None,
        None,
        None,
        None,
    );
    record_task(state, task.clone())?;

    Ok(RemoteCodexMaintenanceResult {
        host_alias: alias.clone(),
        ok,
        action: action.clone(),
        before_version,
        after_version,
        codex_path,
        codex_command_available,
        install_method,
        path_changed,
        shell_config_path,
        backup_path,
        message,
        task,
    })
}

pub(crate) fn run_codex_step(
    alias: &str,
    task_id: &str,
    logs: &mut Vec<TaskLog>,
    next_log_index: &mut usize,
    label: &str,
    script: &str,
    timeout: u64,
    failure_level: TaskLogLevel,
    progress: Option<&CodexProgressContext<'_>>,
) -> ssh::SshCommandOutput {
    emit_remote_codex_progress(
        progress,
        label,
        "running",
        format!("{label} started."),
        None,
        None,
        None,
        None,
        None,
        None,
    );
    let output = if let Some(progress) = progress {
        ssh::run_ssh_script_streaming(alias, script, timeout, |event| {
            emit_remote_codex_stream_event(progress, label, event);
        })
    } else {
        ssh::run_ssh_script(alias, script, timeout)
    }
    .unwrap_or_else(|error| {
        failed_command_output(
            format!("ssh {alias} {label}"),
            format!("Could not run ssh: {error}"),
        )
    });
    let status = if output.success() {
        "success"
    } else {
        "failed"
    };
    let message = command_step_message(label, &output, timeout);
    emit_remote_codex_progress_for_output(progress, label, status, &message, &output);
    push_command_step_log(
        task_id,
        logs,
        next_log_index,
        label,
        &output,
        timeout,
        failure_level,
    );
    output
}

pub(crate) fn command_step_message(
    label: &str,
    output: &ssh::SshCommandOutput,
    timeout: u64,
) -> String {
    if output.success() {
        format!("{label} completed.")
    } else if output.timed_out {
        format!("{label} timed out after {timeout} ms.")
    } else {
        format!("{label} failed: {}", command_detail(output))
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_remote_codex_progress(
    progress: Option<&CodexProgressContext<'_>>,
    step: &str,
    status: &str,
    message: String,
    detail: Option<String>,
    stdout: Option<String>,
    stderr: Option<String>,
    exit_code: Option<i32>,
    duration_ms: Option<u64>,
    timed_out: Option<bool>,
) {
    let Some(progress) = progress else {
        return;
    };
    let message = ssh::redact_sensitive(&message);
    let level = match status {
        "failed" => TaskLogLevel::Error,
        "stderr" => TaskLogLevel::Warn,
        _ => TaskLogLevel::Info,
    };
    if let Err(error) = jobs::append_message(
        &progress.state.task_store,
        progress.state.task_event_sink.as_ref(),
        progress.task_id,
        level,
        &message,
    ) {
        eprintln!(
            "Could not persist sanitized Codex progress: {}",
            redact_error_text(&error)
        );
    }
    let Some(request_id) = progress.request_id else {
        return;
    };

    let payload = RemoteCodexProgressEvent {
        request_id: request_id.to_string(),
        host_alias: progress.host_alias.to_string(),
        action: progress.action.clone(),
        step: step.to_string(),
        status: status.to_string(),
        message,
        detail: detail.map(|value| ssh::redact_sensitive(&value)),
        stdout: stdout.map(|value| ssh::redact_sensitive(&value)),
        stderr: stderr.map(|value| ssh::redact_sensitive(&value)),
        exit_code,
        duration_ms,
        timed_out,
    };
    if let Err(error) = progress.app.emit("remote-codex-progress", payload) {
        eprintln!(
            "Could not emit sanitized Codex progress: {}",
            redact_error_text(&error.to_string())
        );
    }
}

pub(crate) fn emit_remote_codex_stream_event(
    progress: &CodexProgressContext<'_>,
    step: &str,
    event: ssh::ProcessStreamEvent,
) {
    match event.kind {
        ssh::ProcessStreamKind::Stdout => emit_remote_codex_progress(
            Some(progress),
            step,
            "stdout",
            event.line.clone(),
            None,
            Some(event.line),
            None,
            None,
            Some(event.elapsed_ms),
            None,
        ),
        ssh::ProcessStreamKind::Stderr => emit_remote_codex_progress(
            Some(progress),
            step,
            "stderr",
            event.line.clone(),
            None,
            None,
            Some(event.line),
            None,
            Some(event.elapsed_ms),
            None,
        ),
        ssh::ProcessStreamKind::Heartbeat => emit_remote_codex_progress(
            Some(progress),
            step,
            "heartbeat",
            event.line.clone(),
            Some(step.to_string()),
            None,
            None,
            None,
            Some(event.elapsed_ms),
            None,
        ),
    }
}

pub(crate) fn emit_remote_codex_progress_for_output(
    progress: Option<&CodexProgressContext<'_>>,
    step: &str,
    status: &str,
    message: &str,
    output: &ssh::SshCommandOutput,
) {
    emit_remote_codex_progress(
        progress,
        step,
        status,
        message.to_string(),
        None,
        first_output_line(&output.stdout),
        first_output_line(&output.stderr),
        output.exit_code,
        Some(output.duration_ms),
        Some(output.timed_out),
    );
}

pub(crate) fn first_output_line(value: &str) -> Option<String> {
    value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

pub(crate) fn push_command_step_log(
    task_id: &str,
    logs: &mut Vec<TaskLog>,
    next_log_index: &mut usize,
    label: &str,
    output: &ssh::SshCommandOutput,
    timeout: u64,
    failure_level: TaskLogLevel,
) {
    let level = if output.success() {
        TaskLogLevel::Info
    } else {
        failure_level
    };
    let message = command_step_message(label, output, timeout);
    logs.push(command_log(
        task_id,
        *next_log_index,
        level,
        &message,
        output,
    ));
    *next_log_index += 1;
}

pub(crate) fn run_local_upload_codex_fallback(
    paths: &AppPaths,
    alias: &str,
    task_id: &str,
    logs: &mut Vec<TaskLog>,
    next_log_index: &mut usize,
    timeout: u64,
    progress: Option<&CodexProgressContext<'_>>,
) -> ssh::SshCommandOutput {
    let platform_output = run_codex_step(
        alias,
        task_id,
        logs,
        next_log_index,
        "detect Codex native package platform",
        CODEX_NATIVE_PLATFORM_SCRIPT,
        timeout,
        TaskLogLevel::Error,
        progress,
    );
    if !platform_output.success() {
        return platform_output;
    }

    let platform = match marker_value(&platform_output.stdout, "CODEXHUB_NATIVE_PLATFORM") {
        Some(value) => value,
        None => {
            let output = failed_command_output(
                "parse remote Codex native platform".into(),
                "Remote platform probe did not return CODEXHUB_NATIVE_PLATFORM.".into(),
            );
            push_command_step_log(
                task_id,
                logs,
                next_log_index,
                "parse Codex native package platform",
                &output,
                timeout,
                TaskLogLevel::Error,
            );
            emit_remote_codex_progress_for_output(
                progress,
                "parse Codex native package platform",
                "failed",
                "Remote platform probe did not return CODEXHUB_NATIVE_PLATFORM.",
                &output,
            );
            return output;
        }
    };
    let target = match marker_value(&platform_output.stdout, "CODEXHUB_NATIVE_TARGET") {
        Some(value) => value,
        None => {
            let output = failed_command_output(
                "parse remote Codex native target".into(),
                "Remote platform probe did not return CODEXHUB_NATIVE_TARGET.".into(),
            );
            push_command_step_log(
                task_id,
                logs,
                next_log_index,
                "parse Codex native package target",
                &output,
                timeout,
                TaskLogLevel::Error,
            );
            emit_remote_codex_progress_for_output(
                progress,
                "parse Codex native package target",
                "failed",
                "Remote platform probe did not return CODEXHUB_NATIVE_TARGET.",
                &output,
            );
            return output;
        }
    };

    let (package, download_output) =
        download_codex_native_package_locally(paths, &platform, &target, timeout, progress);
    push_command_step_log(
        task_id,
        logs,
        next_log_index,
        "download Codex native package locally",
        &download_output,
        timeout,
        TaskLogLevel::Error,
    );
    let Some(package) = package else {
        return download_output;
    };

    let remote_tarball = format!(
        "/tmp/codexhub-codex-{}-{}.tgz",
        timestamp_millis(),
        package.version
    );
    emit_remote_codex_progress(
        progress,
        "upload Codex native package",
        "running",
        "Uploading Codex native package to remote host.".into(),
        None,
        None,
        None,
        None,
        None,
        None,
    );
    let upload_output = if let Some(progress) = progress {
        ssh::upload_file_streaming(
            alias,
            &package.tarball_path,
            &remote_tarball,
            timeout,
            |event| {
                emit_remote_codex_stream_event(progress, "upload Codex native package", event);
            },
        )
    } else {
        ssh::upload_file(alias, &package.tarball_path, &remote_tarball, timeout)
    }
    .unwrap_or_else(|error| {
        failed_command_output(
            format!(
                "scp {} {alias}:{remote_tarball}",
                path_string(&package.tarball_path)
            ),
            format!("Could not upload Codex native package: {error}"),
        )
    });
    emit_remote_codex_progress_for_output(
        progress,
        "upload Codex native package",
        if upload_output.success() {
            "success"
        } else {
            "failed"
        },
        &command_step_message("upload Codex native package", &upload_output, timeout),
        &upload_output,
    );
    push_command_step_log(
        task_id,
        logs,
        next_log_index,
        "upload Codex native package",
        &upload_output,
        timeout,
        TaskLogLevel::Error,
    );
    if !upload_output.success() {
        log_best_effort(
            "clean downloaded package directory",
            fs::remove_dir_all(&package.temp_dir),
        );
        return upload_output;
    }

    let install_script =
        codex_install_uploaded_package_script(&remote_tarball, &package.version, &package.target);
    let install_output = run_codex_step(
        alias,
        task_id,
        logs,
        next_log_index,
        "install uploaded Codex native package",
        &install_script,
        timeout,
        TaskLogLevel::Error,
        progress,
    );
    log_best_effort(
        "clean downloaded package directory",
        fs::remove_dir_all(&package.temp_dir),
    );
    install_output
}

pub(crate) fn download_codex_native_package_locally(
    paths: &AppPaths,
    platform: &str,
    target: &str,
    timeout: u64,
    progress: Option<&CodexProgressContext<'_>>,
) -> (Option<LocalCodexNativePackage>, ssh::SshCommandOutput) {
    let temp_dir = paths
        .cache_file("codex-downloads")
        .join(format!("native-{}", timestamp_millis()));
    if let Err(error) = fs::create_dir_all(&temp_dir) {
        return (
            None,
            failed_command_output(
                "create local Codex package temp directory".into(),
                format!("Could not create local temp directory: {error}"),
            ),
        );
    }

    let metadata_path = temp_dir.join("codex-metadata.json");
    let metadata_url = "https://registry.npmmirror.com/@openai/codex";
    let metadata_output = local_curl_download(
        metadata_url,
        &metadata_path,
        "download @openai/codex metadata from npmmirror",
        timeout,
        progress,
    );
    if !metadata_output.success() {
        log_best_effort("clean temporary directory", fs::remove_dir_all(&temp_dir));
        return (None, metadata_output);
    }

    let metadata = match fs::read_to_string(&metadata_path) {
        Ok(value) => value,
        Err(error) => {
            log_best_effort("clean temporary directory", fs::remove_dir_all(&temp_dir));
            return (
                None,
                failed_command_output(
                    "read local @openai/codex metadata".into(),
                    format!("Could not read downloaded npmmirror metadata: {error}"),
                ),
            );
        }
    };
    let (version, tarball_url) = match parse_npmmirror_native_metadata(&metadata, platform) {
        Ok(value) => value,
        Err(error) => {
            log_best_effort("clean temporary directory", fs::remove_dir_all(&temp_dir));
            return (
                None,
                failed_command_output("parse local @openai/codex metadata".into(), error),
            );
        }
    };
    if !is_safe_codex_package_version(&version) {
        log_best_effort("clean temporary directory", fs::remove_dir_all(&temp_dir));
        return (
            None,
            failed_command_output(
                "validate local @openai/codex package version".into(),
                format!("npmmirror returned an unsafe Codex package version: {version}"),
            ),
        );
    }

    let tarball_path = temp_dir.join(format!("codex-{version}-{platform}.tgz"));
    let tarball_output = local_curl_download(
        &tarball_url,
        &tarball_path,
        "download @openai/codex native package from npmmirror",
        timeout,
        progress,
    );
    if !tarball_output.success() {
        log_best_effort("clean temporary directory", fs::remove_dir_all(&temp_dir));
        return (None, tarball_output);
    }

    let stdout = format!(
        "CODEXHUB_LOCAL_PACKAGE_PLATFORM={platform}\nCODEXHUB_LOCAL_PACKAGE_TARGET={target}\nCODEXHUB_LOCAL_PACKAGE_VERSION={version}\nCODEXHUB_LOCAL_PACKAGE_TARBALL={tarball_url}\n"
    );
    let output = ssh::SshCommandOutput {
        command: "local npmmirror native package download".into(),
        stdout,
        stderr: tarball_output.stderr.clone(),
        exit_code: Some(0),
        duration_ms: metadata_output.duration_ms + tarball_output.duration_ms,
        timed_out: false,
    };

    (
        Some(LocalCodexNativePackage {
            version,
            target: target.to_string(),
            tarball_path,
            temp_dir,
        }),
        output,
    )
}

pub(crate) fn run_refresh_latest_codex_version(
    state: &AppState,
    force: bool,
    timeout_ms: Option<u64>,
) -> Result<LatestCodexVersion, String> {
    let cached = read_latest_codex_version_cache(&state.paths)?;
    let now = Local::now().fixed_offset();
    if !force {
        if let Some(cache) = cached.as_ref() {
            if latest_codex_cache_is_fresh(cache, now) {
                return Ok(LatestCodexVersion {
                    error: None,
                    ..cache.clone()
                });
            }
        }
    }

    let timeout = ssh::normalize_timeout_ms(timeout_ms.or(Some(30_000)));
    let fetched = fetch_latest_codex_version(&state.paths, timeout);
    let should_write = fetched.is_ok();
    let latest = latest_codex_result_from_fetch(fetched, cached, now);
    if should_write {
        write_latest_codex_version_cache(&state.paths, &latest)?;
    }
    Ok(latest)
}

pub(crate) fn run_get_local_codex_status() -> LocalCodexStatus {
    let platform = platform::get_platform();
    let mut search_paths = platform::codex_binary_candidates_for_current_home()
        .into_iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    search_paths.push(match platform {
        platform::RuntimePlatform::Windows => "where codex".into(),
        platform::RuntimePlatform::MacOS | platform::RuntimePlatform::Linux => "which codex".into(),
    });
    let detected_path = platform::detect_codex_binary_path();
    let version = detected_path
        .as_ref()
        .and_then(|path| platform::run_version_command(path));

    LocalCodexStatus {
        platform,
        detected: detected_path.is_some(),
        path: detected_path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
        version,
        search_paths,
        install_hint: local_codex_install_hint(platform),
    }
}

pub(crate) fn local_codex_install_hint(platform: platform::RuntimePlatform) -> String {
    match platform {
        platform::RuntimePlatform::MacOS => {
            "Install Codex CLI with the official OpenAI/Codex installer, then ensure /opt/homebrew/bin, /usr/local/bin, or ~/.local/bin is on PATH.".into()
        }
        platform::RuntimePlatform::Windows => {
            "Install or update Codex CLI from the official OpenAI/Codex installer, then refresh this status.".into()
        }
        platform::RuntimePlatform::Linux => {
            "Install Codex CLI with the official OpenAI/Codex installer or a supported package manager, then refresh this status.".into()
        }
    }
}

pub(crate) fn latest_codex_result_from_fetch(
    fetched: Result<String, String>,
    cached: Option<LatestCodexVersion>,
    now: DateTime<FixedOffset>,
) -> LatestCodexVersion {
    match fetched {
        Ok(version) => LatestCodexVersion {
            version: Some(version),
            checked_at: Some(now.to_rfc3339()),
            source: CODEX_LATEST_SOURCE.into(),
            error: None,
        },
        Err(error) => {
            if let Some(cache) = cached {
                if cache.version.is_some() {
                    return LatestCodexVersion {
                        error: Some(error),
                        ..cache
                    };
                }
            }
            LatestCodexVersion {
                version: None,
                checked_at: None,
                source: CODEX_LATEST_SOURCE.into(),
                error: Some(error),
            }
        }
    }
}

pub(crate) fn read_latest_codex_version_cache(
    paths: &AppPaths,
) -> Result<Option<LatestCodexVersion>, String> {
    storage::load_cache_document(paths, "codex-latest.json")
}

pub(crate) fn write_latest_codex_version_cache(
    paths: &AppPaths,
    latest: &LatestCodexVersion,
) -> Result<(), String> {
    storage::save_cache_document(paths, "codex-latest.json", latest)
}

pub(crate) fn latest_codex_cache_is_fresh(
    cache: &LatestCodexVersion,
    now: DateTime<FixedOffset>,
) -> bool {
    let Some(checked_at) = cache.checked_at.as_deref() else {
        return false;
    };
    if cache
        .version
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return false;
    }
    let Ok(checked_at) = DateTime::parse_from_rfc3339(checked_at) else {
        return false;
    };
    checked_at >= latest_codex_refresh_boundary(now)
}

pub(crate) fn latest_codex_refresh_boundary(now: DateTime<FixedOffset>) -> DateTime<FixedOffset> {
    let date = now.date_naive();
    let today = now
        .timezone()
        .from_local_datetime(
            &date
                .and_hms_opt(CODEX_LATEST_REFRESH_HOUR, 0, 0)
                .expect("valid refresh hour"),
        )
        .single()
        .unwrap_or(now);
    if now < today {
        today - ChronoDuration::days(1)
    } else {
        today
    }
}

pub(crate) fn fetch_latest_codex_version(paths: &AppPaths, timeout: u64) -> Result<String, String> {
    let temp_dir = paths
        .cache_file("codex-downloads")
        .join(format!("latest-{}", timestamp_millis()));
    fs::create_dir_all(&temp_dir)
        .map_err(|error| format!("Could not create local temp directory: {error}"))?;
    let metadata_path = temp_dir.join("codex-npm-metadata.json");
    let output = local_curl_download(
        CODEX_NPM_REGISTRY_URL,
        &metadata_path,
        "download @openai/codex metadata from npm",
        timeout,
        None,
    );
    if !output.success() {
        log_best_effort("clean temporary directory", fs::remove_dir_all(&temp_dir));
        return Err(command_detail(&output));
    }
    let metadata = match fs::read_to_string(&metadata_path) {
        Ok(value) => value,
        Err(error) => {
            log_best_effort("clean temporary directory", fs::remove_dir_all(&temp_dir));
            return Err(format!("Could not read downloaded npm metadata: {error}"));
        }
    };
    let latest = parse_npm_latest_metadata(&metadata);
    log_best_effort("clean temporary directory", fs::remove_dir_all(&temp_dir));
    latest
}

pub(crate) fn parse_npm_latest_metadata(metadata: &str) -> Result<String, String> {
    let lower = metadata.to_ascii_lowercase();
    if lower.contains("<html")
        || lower.contains("authentication is required")
        || lower.contains("captive")
        || lower.contains("portal")
    {
        return Err(
            "npm metadata response was HTML instead of JSON; the network may require captive portal authentication before downloads can work."
                .into(),
        );
    }
    let data: serde_json::Value = serde_json::from_str(metadata)
        .map_err(|error| format!("npm metadata response was not JSON: {error}"))?;
    let latest = data
        .get("dist-tags")
        .and_then(|value| value.get("latest"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            "npm metadata did not include dist-tags.latest for @openai/codex.".to_string()
        })?
        .trim()
        .to_string();
    if !is_safe_codex_package_version(&latest) {
        return Err(format!(
            "npm returned an unsafe Codex package version: {latest}"
        ));
    }
    Ok(latest)
}

pub(crate) fn local_curl_download(
    url: &str,
    output_path: &Path,
    label: &str,
    timeout: u64,
    progress: Option<&CodexProgressContext<'_>>,
) -> ssh::SshCommandOutput {
    let timeout_secs = ((timeout + 999) / 1000).clamp(1, 120).to_string();
    let args = vec![
        "-fsSL".into(),
        "--connect-timeout".into(),
        "15".into(),
        "--max-time".into(),
        timeout_secs,
        url.into(),
        "-o".into(),
        output_path.to_string_lossy().to_string(),
    ];
    let command = format!("curl -fsSL {url} -o {}", path_string(output_path));
    emit_remote_codex_progress(
        progress,
        label,
        "running",
        format!("{label} started."),
        Some(url.into()),
        None,
        None,
        None,
        None,
        None,
    );
    let output = if let Some(progress) = progress {
        ssh::run_local_process_streaming("curl", &args, &command, timeout, |event| {
            emit_remote_codex_stream_event(progress, label, event);
        })
    } else {
        ssh::run_local_process("curl", &args, &command, timeout)
    }
    .unwrap_or_else(|error| {
        failed_command_output(
            label.to_string(),
            format!("Could not start local curl: {error}"),
        )
    });
    emit_remote_codex_progress_for_output(
        progress,
        label,
        if output.success() {
            "success"
        } else {
            "failed"
        },
        &command_step_message(label, &output, timeout),
        &output,
    );
    output
}

pub(crate) fn parse_npmmirror_native_metadata(
    metadata: &str,
    platform: &str,
) -> Result<(String, String), String> {
    let lower = metadata.to_ascii_lowercase();
    if lower.contains("<html")
        || lower.contains("authentication is required")
        || lower.contains("net2.zju.edu.cn")
        || lower.contains("captive")
        || lower.contains("portal")
    {
        return Err(
            "npmmirror metadata response was HTML instead of JSON; the network may require captive portal authentication before downloads can work."
                .into(),
        );
    }

    let data: serde_json::Value = serde_json::from_str(metadata).map_err(|error| {
        format!(
            "npmmirror metadata response was not JSON; the network may require captive portal authentication before downloads can work: {error}"
        )
    })?;
    let latest = data
        .get("dist-tags")
        .and_then(|value| value.get("latest"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "npmmirror metadata did not include dist-tags.latest for @openai/codex.".to_string()
        })?;
    let package_key = format!("{latest}-{platform}");
    let package = data
        .get("versions")
        .and_then(|value| value.get(&package_key))
        .ok_or_else(|| {
            format!("npmmirror metadata did not include package version {package_key}.")
        })?;
    let tarball = package
        .get("dist")
        .and_then(|value| value.get("tarball"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| value.starts_with("https://registry.npmmirror.com/"))
        .ok_or_else(|| {
            format!("npmmirror metadata returned an unexpected tarball URL for {package_key}.")
        })?;

    Ok((latest.to_string(), tarball.to_string()))
}

pub(crate) fn is_safe_codex_package_version(version: &str) -> bool {
    !version.is_empty()
        && version
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '+'))
}

pub(crate) fn codex_install_uploaded_package_script(
    remote_tarball: &str,
    version: &str,
    target: &str,
) -> String {
    r#"set -u
export CODEX_INSTALL_DIR="$HOME/.local/bin"
export CODEX_HOME="$HOME/.codex"
export PATH="$HOME/.local/bin:$PATH"
remote_tarball=__REMOTE_TARBALL__
version=__VERSION__
target=__TARGET__
tmp_dir="${TMPDIR:-/tmp}/codexhub-upload-install.$$"
stage_dir=""
mkdir -p "$CODEX_INSTALL_DIR" "$CODEX_HOME/packages/standalone/releases" "$tmp_dir"
trap 'rm -rf "$tmp_dir"; [ -n "${stage_dir:-}" ] && rm -rf "$stage_dir"; rm -f "$remote_tarball"' EXIT HUP INT TERM

if [ ! -s "$remote_tarball" ]; then
  printf 'Uploaded Codex native package is missing or empty: %s\n' "$remote_tarball" >&2
  exit 2
fi
if ! tar -tzf "$remote_tarball" >/dev/null 2>&1; then
  printf 'Uploaded Codex native package is not a readable gzip tarball.\n' >&2
  exit 2
fi
if tar -tzf "$remote_tarball" | grep -Eq '(^|/)\.\.(/|$)|^/'; then
  printf 'Uploaded Codex native package archive contains unsafe paths.\n' >&2
  exit 2
fi

extract_dir="$tmp_dir/native-extract"
release_dir="$CODEX_HOME/packages/standalone/releases/$version"
stage_dir="$release_dir.tmp.$$"
rm -rf "$extract_dir" "$stage_dir"
mkdir -p "$extract_dir" "$stage_dir"
tar -xzf "$remote_tarball" -C "$extract_dir"
vendor_dir="$extract_dir/package/vendor/$target"
if [ ! -x "$vendor_dir/bin/codex" ]; then
  printf 'Uploaded Codex native package did not contain vendor/%s/bin/codex.\n' "$target" >&2
  exit 2
fi

cp -R "$vendor_dir/." "$stage_dir/"
chmod 0755 "$stage_dir/bin/codex"
[ -f "$stage_dir/codex-path/rg" ] && chmod 0755 "$stage_dir/codex-path/rg"
[ -f "$stage_dir/codex-resources/bwrap" ] && chmod 0755 "$stage_dir/codex-resources/bwrap"
rm -rf "$release_dir"
mv "$stage_dir" "$release_dir"
stage_dir=""
ln -sfn "$release_dir" "$CODEX_HOME/packages/standalone/current"
ln -sfn "$release_dir/bin/codex" "$CODEX_INSTALL_DIR/codex"
printf 'CODEXHUB_INSTALL_METHOD=npm-mirror-native-local-upload\n'
"#
    .replace("__REMOTE_TARBALL__", &shell_single_quote(remote_tarball))
    .replace("__VERSION__", &shell_single_quote(version))
    .replace("__TARGET__", &shell_single_quote(target))
}

pub(crate) fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

pub(crate) fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

pub(crate) fn output_trimmed(output: &ssh::SshCommandOutput) -> Option<String> {
    output
        .success()
        .then(|| output.stdout.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) fn marker_value(stdout: &str, marker: &str) -> Option<String> {
    let prefix = format!("{marker}=");
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn remote_codex_action_label(action: &RemoteCodexAction) -> &'static str {
    match action {
        RemoteCodexAction::CheckVersion => "Check Codex version",
        RemoteCodexAction::Install => "Install Codex",
        RemoteCodexAction::Update => "Update Codex",
        RemoteCodexAction::Uninstall => "Uninstall Codex",
    }
}

pub(crate) fn codex_maintenance_task(
    task_id: &str,
    host_id: &str,
    host_name: &str,
    action: &str,
    status: TaskStatus,
    summary: &str,
    logs: Vec<TaskLog>,
) -> TaskRun {
    TaskRun {
        id: task_id.to_string(),
        host_id: host_id.to_string(),
        host_name: host_name.to_string(),
        action: action.to_string(),
        status,
        started_at: timestamp_label(),
        ended_at: Some(timestamp_label()),
        summary: summary.to_string(),
        logs,
    }
}
