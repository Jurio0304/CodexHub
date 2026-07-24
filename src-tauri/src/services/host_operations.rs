use crate::tasks::{TaskStep, TaskStepStatus};
use crate::*;

use super::codex_runtime::{
    self, CodexReleaseCleanupPolicy, CodexReleaseCleanupStatus, CodexRuntimeReconcileStatus,
};
use super::updater_operations::{
    preflight_remote_codex_proxy_tunnel, remote_codex_proxy_tunnel_candidates,
};

const OFFICIAL_INSTALLER_SSH_TIMEOUT_MS: u64 = 360_000;
const PROBE_STEP_IDS: &[(&str, &str)] = &[
    ("ssh-check", "Checking SSH connectivity."),
    ("system", "Reading the remote system environment."),
    ("codex", "Detecting the Codex installation and version."),
    ("api", "Inspecting the remote API configuration."),
    ("skills", "Inspecting installed Codex skills."),
];
const INSTALL_STEP_IDS: &[(&str, &str)] = &[
    ("preparation", "Preparing the remote host."),
    (
        "official-installer",
        "Waiting to try the official installer.",
    ),
    (
        "remote-native-mirror",
        "Waiting to try the remote native mirror package.",
    ),
    ("remote-npm-mirror", "Waiting to try the remote npm mirror."),
    (
        "local-upload",
        "Waiting to download and upload a native package.",
    ),
    (
        "runtime-reconcile",
        "Waiting to reconcile the managed Codex runtime.",
    ),
    (
        "final-verification",
        "Waiting to verify the final Codex installation.",
    ),
    (
        "release-cleanup",
        "Waiting to clean eligible managed Codex releases.",
    ),
];
const UNINSTALL_STEP_IDS: &[(&str, &str)] = &[
    ("preparation", "Preparing the remote host."),
    (
        "uninstall",
        "Waiting to remove the managed Codex installation.",
    ),
    (
        "final-verification",
        "Waiting to verify that Codex is no longer available.",
    ),
];

struct HostProgressContext<'a> {
    app: &'a AppHandle,
    state: &'a AppState,
    task_id: &'a str,
    request_id: &'a str,
    host_alias: &'a str,
    operation: HostOperationKind,
}

fn operation_steps(task_id: &str, definitions: &[(&str, &str)]) -> Vec<TaskStep> {
    definitions
        .iter()
        .enumerate()
        .map(|(sequence, (step_id, summary))| TaskStep {
            task_run_id: task_id.to_string(),
            step_id: (*step_id).to_string(),
            sequence: sequence as u32,
            status: TaskStepStatus::Pending,
            summary: (*summary).to_string(),
            started_at: None,
            ended_at: None,
        })
        .collect()
}

fn persist_and_emit_step(
    context: &HostProgressContext<'_>,
    step_id: &str,
    status: TaskStepStatus,
    summary: impl Into<String>,
    mut log: Option<TaskLog>,
) -> Result<TaskStep, String> {
    let persisted = context
        .state
        .task_store
        .get(context.task_id)?
        .ok_or_else(|| format!("Task {} was not found.", context.task_id))?;
    let previous = persisted
        .steps
        .iter()
        .find(|step| step.step_id == step_id)
        .cloned()
        .ok_or_else(|| format!("Task step {step_id} was not initialized."))?;
    let now = timestamp_label();
    let terminal = matches!(
        status,
        TaskStepStatus::Success | TaskStepStatus::Failed | TaskStepStatus::Skipped
    );
    let step = TaskStep {
        task_run_id: context.task_id.to_string(),
        step_id: step_id.to_string(),
        sequence: previous.sequence,
        status: status.clone(),
        summary: summary.into(),
        started_at: if matches!(status, TaskStepStatus::Pending | TaskStepStatus::Skipped) {
            previous.started_at
        } else {
            previous.started_at.or_else(|| Some(now.clone()))
        },
        ended_at: terminal.then_some(now),
    };
    if let Some(log) = log.as_mut() {
        log.step_id = Some(step_id.to_string());
    }
    let update = jobs::persist_step_update(
        &context.state.task_store,
        context.state.task_event_sink.as_ref(),
        context.task_id,
        &step,
        log.as_ref(),
        Some(&step.summary),
    )?;
    debug_assert_eq!(update.task.id, context.task_id);
    if let Err(error) = context.app.emit(
        "host-operation-progress",
        HostOperationProgressEvent {
            request_id: context.request_id.to_string(),
            task_id: context.task_id.to_string(),
            host_alias: context.host_alias.to_string(),
            operation: context.operation.clone(),
            step: update.step.clone(),
            log: update.log.clone(),
        },
    ) {
        eprintln!(
            "Could not emit host operation progress: {}",
            redact_error_text(&error.to_string())
        );
    }
    Ok(update.step)
}

fn initialize_operation_steps(
    state: &AppState,
    task: &mut TaskRun,
    definitions: &[(&str, &str)],
) -> Result<(), String> {
    task.steps = operation_steps(&task.id, definitions);
    if let Some((first_step_id, _)) = definitions.first() {
        assign_existing_logs_to_step(task, first_step_id);
    }
    jobs::persist_task(&state.task_store, state.task_event_sink.as_ref(), task)
}

fn assign_existing_logs_to_step(task: &mut TaskRun, step_id: &str) {
    for log in &mut task.logs {
        if log.step_id.is_none() {
            log.step_id = Some(step_id.to_string());
        }
    }
}

fn persist_step_logs(
    context: &HostProgressContext<'_>,
    step_id: &str,
    logs: &mut [TaskLog],
) -> Result<(), String> {
    for log in logs {
        log.step_id = Some(step_id.to_string());
        persist_and_emit_step(
            context,
            step_id,
            TaskStepStatus::Running,
            log.message.clone(),
            Some(log.clone()),
        )?;
    }
    Ok(())
}

fn host_operation_request_id(request_id: Option<String>, prefix: &str) -> String {
    request_id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("{prefix}-{}", timestamp_millis()))
}

fn host_operation_kind(action: &RemoteCodexAction) -> HostOperationKind {
    match action {
        RemoteCodexAction::Install => HostOperationKind::CodexInstall,
        RemoteCodexAction::Update | RemoteCodexAction::CheckVersion => {
            HostOperationKind::CodexUpdate
        }
        RemoteCodexAction::Uninstall => HostOperationKind::CodexUninstall,
    }
}

fn next_installation_method(attempt_outcomes: &[bool]) -> Option<usize> {
    if attempt_outcomes.iter().any(|succeeded| *succeeded) {
        None
    } else {
        (attempt_outcomes.len() < 4).then_some(attempt_outcomes.len())
    }
}

fn official_installer_network_failure(output: &ssh::SshCommandOutput) -> bool {
    if output.success() {
        return false;
    }
    if output.timed_out {
        return true;
    }
    if output.exit_code == Some(255) {
        return false;
    }
    if matches!(
        output.exit_code,
        Some(4 | 5 | 6 | 7 | 28 | 35 | 47 | 52 | 55 | 56 | 124)
    ) {
        return true;
    }
    let detail = format!("{}\n{}", output.stdout, output.stderr).to_ascii_lowercase();
    [
        "curl: (5)",
        "curl: (6)",
        "curl: (7)",
        "curl: (28)",
        "could not resolve host",
        "temporary failure in name resolution",
        "network is unreachable",
        "connection timed out",
        "operation timed out",
        "failed to connect",
        "connection reset by peer",
        "tls connect error",
        "wget: unable to resolve host address",
    ]
    .iter()
    .any(|marker| detail.contains(marker))
}

fn remote_forward_setup_failure(output: &ssh::SshCommandOutput) -> bool {
    let detail = format!("{}\n{}", output.stdout, output.stderr).to_ascii_lowercase();
    detail.contains("remote port forwarding failed")
        || detail.contains("cannot listen to port")
        || detail.contains("port forwarding is disabled")
        || detail.contains("administratively prohibited")
}

fn remote_proxy_port_candidates(task_id: &str) -> [u16; 3] {
    const START: u16 = 42_000;
    const RANGE: u64 = 10_000;
    let seed = task_id
        .bytes()
        .fold(1_469_598_103_934_665_603u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(1_099_511_628_211)
        });
    [0u64, 3_301, 6_607].map(|offset| START + ((seed + offset) % RANGE) as u16)
}

fn final_verification_failures(
    method_succeeded: bool,
    has_path: bool,
    has_version: bool,
    _current_shell_available: bool,
    login_shell_available: bool,
) -> Vec<&'static str> {
    // Non-login SSH shells may skip user PATH startup files. The verified login
    // shell is therefore the command-availability gate; current-shell failure is diagnostic.
    let checks = [
        (method_succeeded, "verified installation method"),
        (has_path, "Codex path"),
        (has_version, "Codex version"),
        (login_shell_available, "login-shell command"),
    ];
    checks
        .into_iter()
        .filter_map(|(passed, label)| (!passed).then_some(label))
        .collect()
}

fn codex_command_available_in_any_shell(
    current_shell_available: bool,
    login_shell_available: bool,
) -> bool {
    current_shell_available || login_shell_available
}

fn final_verification_success_message(
    action_label: &str,
    alias: &str,
    version_label: &str,
    current_shell_available: bool,
) -> String {
    let completed = format!("{action_label} completed on {alias}: {version_label}.");
    if current_shell_available {
        completed
    } else {
        format!(
            "{completed} Warning: the current non-login SSH shell cannot resolve `codex`; its PATH may not include `~/.local/bin`, while the verified login shell is usable."
        )
    }
}

fn detected_codex_installed(has_path: bool, has_version: bool) -> bool {
    has_path && has_version
}

fn uninstall_target_reached(
    has_path: bool,
    has_version: bool,
    current_shell_available: bool,
    login_shell_available: bool,
) -> bool {
    !has_path && !has_version && !current_shell_available && !login_shell_available
}

fn retained_codex_versions(version: Option<String>) -> (Option<String>, Option<String>) {
    (version.clone(), version)
}

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
    let timeout = ssh::normalize_health_check_timeout_ms(timeout_ms);
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
        steps: Vec::new(),
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
        steps: Vec::new(),
        logs,
    }
}

#[derive(Clone)]
struct SystemProbeData {
    os: String,
    arch: String,
    shell: String,
    path: Option<String>,
}

#[derive(Clone)]
struct CodexProbeData {
    installed: bool,
    command_available: bool,
    path: Option<String>,
    version: String,
}

#[derive(Clone)]
struct ApiProbeData {
    config_exists: bool,
    base_url: Option<String>,
    env_var: Option<String>,
    env_present: Option<bool>,
}

#[derive(Clone)]
struct SkillsProbeData {
    exists: bool,
    count: u16,
}

fn system_probe_group_script() -> &'static str {
    r#"set -u
printf 'CODEXHUB_OS=%s\n' "$(uname -s)"
printf 'CODEXHUB_ARCH=%s\n' "$(uname -m)"
shell_value=${SHELL:-$(getent passwd "$(id -un)" 2>/dev/null | cut -d: -f7)}
printf 'CODEXHUB_SHELL=%s\n' "$shell_value"
printf 'CODEXHUB_PATH=%s\n' "$PATH"
"#
}

fn codex_probe_group_script() -> String {
    let path_script = shell_single_quote(&codex_path_probe_script());
    let version_script = shell_single_quote(&codex_version_probe_script());
    let command_script = shell_single_quote(CODEX_COMMAND_AVAILABLE_SCRIPT);
    format!(
        r#"set -u
codex_path=$(sh -c {path_script} 2>/dev/null || true)
codex_version=$(sh -c {version_script} 2>/dev/null || true)
if codex_command=$(sh -c {command_script} 2>/dev/null); then
  command_available=yes
else
  command_available=no
fi
if [ -n "$codex_path" ]; then installed=yes; else installed=no; fi
printf 'CODEXHUB_CODEX_INSTALLED=%s\n' "$installed"
printf 'CODEXHUB_CODEX_COMMAND_AVAILABLE=%s\n' "$command_available"
printf 'CODEXHUB_CODEX_PATH=%s\n' "$codex_path"
printf 'CODEXHUB_CODEX_VERSION=%s\n' "$codex_version"
"#
    )
}

fn api_probe_group_script() -> &'static str {
    r#"set -u
config="$HOME/.codex/config.toml"
# read ~/.codex/config.toml base URL and API env metadata without printing secrets
if [ ! -f "$config" ]; then
  printf 'CODEXHUB_CONFIG_EXISTS=no\n'
  printf 'CODEXHUB_API_BASE_URL=\nCODEXHUB_API_ENV_VAR=\nCODEXHUB_API_ENV_PRESENT=unknown\n'
  exit 0
fi
base_url=$(sed -n -E 's/^[[:space:]]*(openai_base_url|base_url)[[:space:]]*=[[:space:]]*"([^"]*)".*/\2/p' "$config" 2>/dev/null | head -n 1)
api_env=$(sed -n -E 's/^[[:space:]]*(env_key|apiKeyEnvVar)[[:space:]]*=[[:space:]]*"([^"]*)".*/\2/p' "$config" 2>/dev/null | head -n 1)
env_present=unknown
case "$api_env" in
  "" | [0-9]* | *[!A-Za-z0-9_]*) ;;
  *)
    if printenv "$api_env" >/dev/null 2>&1; then
      env_present=yes
    elif [ -f "$HOME/.codex-hub/env" ]; then
      set -a
      . "$HOME/.codex-hub/env" >/dev/null 2>&1 || true
      set +a
      if printenv "$api_env" >/dev/null 2>&1; then env_present=yes; else env_present=no; fi
    else
      env_present=no
    fi
    ;;
esac
printf 'CODEXHUB_CONFIG_EXISTS=yes\n'
printf 'CODEXHUB_API_BASE_URL=%s\n' "$base_url"
printf 'CODEXHUB_API_ENV_VAR=%s\n' "$api_env"
printf 'CODEXHUB_API_ENV_PRESENT=%s\n' "$env_present"
"#
}

fn skills_probe_group_script() -> String {
    format!(
        r#"set -u
if [ -d "$HOME/.codex/skills" ]; then skills_exists=yes; else skills_exists=no; fi
skills_count=$(
{}
)
printf 'CODEXHUB_SKILLS_EXISTS=%s\n' "$skills_exists"
printf 'CODEXHUB_SKILLS_COUNT=%s\n' "$skills_count"
"#,
        remote_skill_count_script()
    )
}

fn marker_required(output: &ssh::SshCommandOutput, marker: &str) -> Result<String, String> {
    marker_value(&output.stdout, marker)
        .ok_or_else(|| format!("Remote probe did not return {marker}."))
}

fn parse_yes_marker(output: &ssh::SshCommandOutput, marker: &str) -> Result<bool, String> {
    match marker_required(output, marker)?.as_str() {
        "yes" => Ok(true),
        "no" => Ok(false),
        value => Err(format!(
            "Remote probe returned invalid {marker} value: {value}."
        )),
    }
}

fn parse_system_probe(output: &ssh::SshCommandOutput) -> Result<SystemProbeData, String> {
    Ok(SystemProbeData {
        os: marker_required(output, "CODEXHUB_OS")?,
        arch: marker_required(output, "CODEXHUB_ARCH")?,
        shell: marker_required(output, "CODEXHUB_SHELL")?,
        path: marker_value(&output.stdout, "CODEXHUB_PATH"),
    })
}

fn parse_codex_probe(output: &ssh::SshCommandOutput) -> Result<CodexProbeData, String> {
    let installed = parse_yes_marker(output, "CODEXHUB_CODEX_INSTALLED")?;
    Ok(CodexProbeData {
        installed,
        command_available: parse_yes_marker(output, "CODEXHUB_CODEX_COMMAND_AVAILABLE")?,
        path: marker_value(&output.stdout, "CODEXHUB_CODEX_PATH"),
        version: if installed {
            marker_value(&output.stdout, "CODEXHUB_CODEX_VERSION")
                .unwrap_or_else(|| "unavailable".into())
        } else {
            "not installed".into()
        },
    })
}

fn parse_api_probe(output: &ssh::SshCommandOutput) -> Result<ApiProbeData, String> {
    let env_present = match marker_required(output, "CODEXHUB_API_ENV_PRESENT")?.as_str() {
        "yes" => Some(true),
        "no" => Some(false),
        "unknown" => None,
        value => {
            return Err(format!(
                "Remote probe returned invalid API env state: {value}."
            ))
        }
    };
    Ok(ApiProbeData {
        config_exists: parse_yes_marker(output, "CODEXHUB_CONFIG_EXISTS")?,
        base_url: marker_value(&output.stdout, "CODEXHUB_API_BASE_URL"),
        env_var: marker_value(&output.stdout, "CODEXHUB_API_ENV_VAR"),
        env_present,
    })
}

fn parse_skills_probe(output: &ssh::SshCommandOutput) -> Result<SkillsProbeData, String> {
    Ok(SkillsProbeData {
        exists: parse_yes_marker(output, "CODEXHUB_SKILLS_EXISTS")?,
        count: marker_required(output, "CODEXHUB_SKILLS_COUNT")?
            .parse::<u16>()
            .map_err(|error| format!("Remote skill count was invalid: {error}"))?,
    })
}

fn probe_group_output(
    alias: &str,
    step_id: &str,
    script: &str,
    timeout: u64,
) -> ssh::SshCommandOutput {
    ssh::run_ssh_script(alias, script, timeout).unwrap_or_else(|error| {
        failed_command_output(
            format!("ssh {alias} probe-{step_id}"),
            format!("Could not run ssh: {error}"),
        )
    })
}

pub(crate) fn run_remote_probe(
    app: &AppHandle,
    state: &AppState,
    host_alias: String,
    timeout_ms: Option<u64>,
    request_id: Option<String>,
) -> Result<RemoteProbeResult, String> {
    let timeout = ssh::normalize_health_check_timeout_ms(timeout_ms);
    let alias_result = ssh::validate_ssh_alias(&host_alias);
    let alias = alias_result
        .clone()
        .unwrap_or_else(|_| host_alias.trim().to_string());
    let task_id = format!("task-probe-{}", timestamp_millis());
    let host_name = host_name_for_alias(state, &alias);
    let host_id = host_id_for_alias(state, &alias);
    let mut running = jobs::begin_task(
        &state.task_store,
        state.task_event_sink.as_ref(),
        &task_id,
        &host_id,
        &host_name,
        "Probe remote system",
    )?;
    initialize_operation_steps(state, &mut running, PROBE_STEP_IDS)?;
    let request_id = host_operation_request_id(request_id, "host-test");
    let progress = HostProgressContext {
        app,
        state,
        task_id: &task_id,
        request_id: &request_id,
        host_alias: &alias,
        operation: HostOperationKind::HostTest,
    };
    persist_and_emit_step(
        &progress,
        "ssh-check",
        TaskStepStatus::Running,
        format!("Checking SSH connectivity to {alias}."),
        None,
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
    let ssh_log = logs.last().cloned();
    persist_and_emit_step(
        &progress,
        "ssh-check",
        if check_ok {
            TaskStepStatus::Success
        } else {
            TaskStepStatus::Failed
        },
        check_message.clone(),
        ssh_log,
    )?;

    if !check_ok {
        let status_persist_error =
            persist_host_check(state, &alias, false, check_output.duration_ms)
                .err()
                .map(|error| redact_error_text(&error));
        for (step_id, _) in PROBE_STEP_IDS.iter().skip(1) {
            persist_and_emit_step(
                &progress,
                step_id,
                TaskStepStatus::Skipped,
                "Not run because SSH preparation failed.",
                None,
            )?;
        }
        let mut summary = format!("Remote probe skipped because SSH check failed: {check_message}");
        if let Some(error) = status_persist_error {
            let persist_message = format!("The offline Host status failed to persist: {error}");
            logs.push(basic_log(
                &task_id,
                logs.len() + 1,
                TaskLogLevel::Error,
                &persist_message,
            ));
            summary.push_str(&format!(" {persist_message}"));
        }
        let task = TaskRun {
            id: task_id.clone(),
            host_id,
            host_name,
            action: "Probe remote system".into(),
            status: TaskStatus::Failed,
            started_at: started_at.clone(),
            ended_at: Some(timestamp_label()),
            summary,
            steps: state
                .task_store
                .get(&task_id)?
                .map(|task| task.steps)
                .unwrap_or_default(),
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
    let scripts = vec![
        ("system", system_probe_group_script().to_string()),
        ("codex", codex_probe_group_script()),
        ("api", api_probe_group_script().to_string()),
        ("skills", skills_probe_group_script()),
    ];
    for (step_id, _) in &scripts {
        persist_and_emit_step(
            &progress,
            step_id,
            TaskStepStatus::Running,
            format!("Running the {step_id} probe."),
            None,
        )?;
    }

    let mut system_data = None;
    let mut codex_data = None;
    let mut api_data = None;
    let mut skills_data = None;
    let mut failed_groups = 0usize;
    let mut probe_timed_out = false;
    // Each logical probe owns one SSH process; the receiver closes cards in completion order.
    let probe_result = run_parallel_completions(
        scripts,
        |step_id, script| probe_group_output(&alias, step_id, &script, timeout),
        |step_id, output| -> Result<(), String> {
            probe_timed_out |= output.timed_out;
            let parsed = if !output.success() {
                Err(command_detail(&output))
            } else {
                match step_id {
                    "system" => parse_system_probe(&output).map(|value| system_data = Some(value)),
                    "codex" => parse_codex_probe(&output).map(|value| codex_data = Some(value)),
                    "api" => parse_api_probe(&output).map(|value| api_data = Some(value)),
                    "skills" => parse_skills_probe(&output).map(|value| skills_data = Some(value)),
                    _ => Err(format!("Unknown probe group {step_id}.")),
                }
            };
            let (status, message, level) = match parsed {
                Ok(()) => (
                    TaskStepStatus::Success,
                    format!("The {step_id} probe completed."),
                    TaskLogLevel::Info,
                ),
                Err(error) => {
                    failed_groups += 1;
                    (
                        TaskStepStatus::Failed,
                        format!("The {step_id} probe failed: {}", redact_error_text(&error)),
                        TaskLogLevel::Error,
                    )
                }
            };
            let mut log = command_log(&task_id, logs.len() + 1, level, &message, &output);
            log.step_id = Some(step_id.to_string());
            logs.push(log.clone());
            persist_and_emit_step(&progress, step_id, status, message, Some(log))?;
            Ok(())
        },
    );
    probe_result?;

    let existing = state
        .hosts
        .lock()
        .expect("hosts mutex poisoned")
        .iter()
        .find(|host| host.host_alias.eq_ignore_ascii_case(&alias))
        .cloned();
    let os = system_data
        .as_ref()
        .map(|data| data.os.clone())
        .or_else(|| existing.as_ref().map(|host| host.os.clone()))
        .unwrap_or_else(|| "Unknown".into());
    let arch = system_data
        .as_ref()
        .map(|data| data.arch.clone())
        .or_else(|| existing.as_ref().map(|host| host.arch.clone()))
        .unwrap_or_else(|| "Unknown".into());
    let shell = system_data
        .as_ref()
        .map(|data| data.shell.clone())
        .or_else(|| existing.as_ref().map(|host| host.shell.clone()))
        .unwrap_or_else(|| "Unknown".into());
    let path = system_data
        .as_ref()
        .and_then(|data| data.path.clone())
        .or_else(|| existing.as_ref().and_then(|host| host.path.clone()));
    let path_has_local_bin = path_has_local_bin(path.as_deref());
    let codex_path = codex_data.as_ref().and_then(|data| data.path.clone());
    let codex_installed = codex_data
        .as_ref()
        .map(|data| data.installed)
        .or_else(|| existing.as_ref().map(|host| host.codex_installed))
        .unwrap_or(false);
    let codex_command_available = codex_data
        .as_ref()
        .map(|data| data.command_available)
        .or_else(|| {
            existing
                .as_ref()
                .and_then(|host| host.codex_command_available)
        })
        .unwrap_or(false);
    let codex_version = codex_data
        .as_ref()
        .map(|data| data.version.clone())
        .or_else(|| existing.as_ref().map(|host| host.codex_version.clone()))
        .unwrap_or_else(|| "not installed".into());
    let config_exists = api_data
        .as_ref()
        .map(|data| data.config_exists)
        .or_else(|| existing.as_ref().and_then(|host| host.config_exists))
        .unwrap_or(false);
    let api_key_env_var = api_data
        .as_ref()
        .and_then(|data| data.env_var.clone())
        .or_else(|| {
            existing
                .as_ref()
                .and_then(|host| host.api_key_env_var.clone())
        });
    let api_key_env_present = api_data
        .as_ref()
        .and_then(|data| data.env_present)
        .or_else(|| existing.as_ref().and_then(|host| host.api_key_env_present));
    let api_config_match = if let Some(data) = api_data.as_ref() {
        classify_remote_api_config(app, state, data.config_exists, data.base_url.as_deref())
    } else {
        RemoteApiConfigMatch {
            name: existing
                .as_ref()
                .and_then(|host| host.api_config_name.clone())
                .unwrap_or_else(|| "Unknown config".into()),
            source: existing
                .as_ref()
                .and_then(|host| host.api_config_source.clone())
                .unwrap_or_else(|| "unknown".into()),
            profile_id: existing.as_ref().and_then(|host| host.profile_id.clone()),
        }
    };
    let skills_exists = skills_data
        .as_ref()
        .map(|data| data.exists)
        .or_else(|| existing.as_ref().and_then(|host| host.skills_exists))
        .unwrap_or(false);
    let skills_count = skills_data
        .as_ref()
        .map(|data| data.count)
        .or_else(|| existing.as_ref().and_then(|host| host.skills_count))
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
    let summary = if probe_timed_out {
        format!(
            "Remote probe timed out for {alias}; the host was marked offline. {failed_groups} group(s) failed."
        )
    } else {
        format!(
            "Probe completed for {alias}: {os}/{arch}, Codex {}. {} group(s) failed.{readiness_suffix}",
            if codex_installed {
                codex_version.as_str()
            } else {
                "not installed"
            },
            failed_groups,
        )
    };
    let mut task = TaskRun {
        id: task_id.clone(),
        host_id,
        host_name,
        action: "Probe remote system".into(),
        status: if failed_groups == 0 {
            TaskStatus::Success
        } else {
            TaskStatus::Failed
        },
        started_at,
        ended_at: Some(timestamp_label()),
        summary: summary.clone(),
        steps: state
            .task_store
            .get(&task_id)?
            .map(|task| task.steps)
            .unwrap_or_default(),
        logs,
    };

    // 任一探测 SSH 超时都优先确认离线，避免部分成功数据把状态写回在线。
    let state_update = if probe_timed_out {
        persist_host_check(state, &alias, false, 0).map(|_| ())
    } else {
        update_host_probe(
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
            system_data.is_some(),
            codex_data.is_some(),
            api_data.is_some(),
            skills_data.is_some(),
        )
        .map(|_| ())
    };
    if let Err(error) = state_update {
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
        ssh_status: if probe_timed_out {
            HostStatus::Offline
        } else {
            HostStatus::Online
        },
        latency_ms: (!probe_timed_out).then_some(check_output.duration_ms),
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

fn run_bounded_ordered<I, O, F>(items: Vec<I>, operation: F) -> Vec<O>
where
    I: Send,
    O: Send,
    F: Fn(I) -> O + Sync,
{
    let item_count = items.len();
    if item_count == 0 {
        return Vec::new();
    }
    let queue = Arc::new(Mutex::new(
        items
            .into_iter()
            .enumerate()
            .collect::<std::collections::VecDeque<_>>(),
    ));
    let results = Arc::new(Mutex::new(
        (0..item_count).map(|_| None).collect::<Vec<Option<O>>>(),
    ));
    let worker_count = item_count.min(HOST_OPERATION_MAX_CONCURRENCY);
    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            let queue = Arc::clone(&queue);
            let results = Arc::clone(&results);
            let operation = &operation;
            scope.spawn(move || loop {
                let Some((index, item)) = queue
                    .lock()
                    .expect("bounded operation queue mutex poisoned")
                    .pop_front()
                else {
                    break;
                };
                let output = operation(item);
                results
                    .lock()
                    .expect("bounded operation result mutex poisoned")[index] = Some(output);
            });
        }
    });
    let ordered = results
        .lock()
        .expect("bounded operation result mutex poisoned")
        .iter_mut()
        .map(|item| {
            item.take()
                .expect("bounded operation worker returned a result")
        })
        .collect();
    ordered
}

fn run_parallel_completions<K, V, O, E, F, C>(
    items: Vec<(K, V)>,
    operation: F,
    mut on_complete: C,
) -> Result<(), E>
where
    K: Clone + Send,
    V: Send,
    O: Send,
    F: Fn(&K, V) -> O + Sync,
    C: FnMut(K, O) -> Result<(), E>,
{
    std::thread::scope(|scope| {
        let (sender, receiver) = std::sync::mpsc::channel();
        for (key, value) in items {
            let sender = sender.clone();
            let operation = &operation;
            scope.spawn(move || {
                let output = operation(&key, value);
                let _ = sender.send((key, output));
            });
        }
        drop(sender);
        for (key, output) in receiver {
            on_complete(key, output)?;
        }
        Ok(())
    })
}

fn run_with_parallel_latest<L, R, FL, FR>(latest: FL, batch: FR) -> (Result<L, String>, R)
where
    L: Send,
    FL: FnOnce() -> L + Send,
    FR: FnOnce() -> R,
{
    std::thread::scope(|scope| {
        let latest = scope.spawn(latest);
        let batch_result = batch();
        let latest_result = latest
            .join()
            .map_err(|_| "Latest Codex version worker panicked.".to_string());
        (latest_result, batch_result)
    })
}

fn run_probe_batch_items(
    app: &AppHandle,
    state: &AppState,
    host_aliases: Vec<String>,
    timeout_ms: Option<u64>,
    request_id: &str,
) -> Vec<RemoteProbeBatchItem> {
    run_bounded_ordered(host_aliases, |host_alias| {
        let item = match run_remote_probe(
            app,
            state,
            host_alias.clone(),
            timeout_ms,
            Some(request_id.to_string()),
        ) {
            Ok(result) => RemoteProbeBatchItem {
                host_alias,
                ok: matches!(&result.task.status, TaskStatus::Success),
                result: Some(result),
                error: None,
            },
            Err(error) => RemoteProbeBatchItem {
                host_alias,
                ok: false,
                result: None,
                error: Some(redact_error_text(&error)),
            },
        };
        // 单台任务与 Host 状态均落定后再通知前端，避免步骤事件抢跑。
        if let Err(error) = app.emit(
            "remote-probe-batch-item-completed",
            RemoteProbeBatchItemCompletedEvent {
                request_id: request_id.to_string(),
                item: item.clone(),
            },
        ) {
            eprintln!(
                "Could not emit completed remote probe item: {}",
                redact_error_text(&error.to_string())
            );
        }
        item
    })
}

pub(crate) fn run_batch_remote_probe_codex(
    app: &AppHandle,
    state: &AppState,
    host_aliases: Vec<String>,
    timeout_ms: Option<u64>,
    request_id: Option<String>,
) -> Result<RemoteProbeBatchResult, String> {
    let request_id = host_operation_request_id(request_id, "batch-host-test");
    let (latest_worker, results) = run_with_parallel_latest(
        || run_refresh_latest_codex_version(state, true, timeout_ms),
        || run_probe_batch_items(app, state, host_aliases, timeout_ms, &request_id),
    );
    let latest_codex_version = latest_worker
        .and_then(|latest| latest)
        .unwrap_or_else(|error| LatestCodexVersion {
            version: None,
            checked_at: None,
            source: CODEX_LATEST_SOURCE.into(),
            error: Some(redact_error_text(&error)),
        });
    Ok(RemoteProbeBatchResult {
        request_id,
        latest_codex_version,
        results,
    })
}

struct CodexStateChecks {
    codex_path: ssh::SshCommandOutput,
    current_command_available: ssh::SshCommandOutput,
    login_command_available: ssh::SshCommandOutput,
    version: ssh::SshCommandOutput,
    shell_path: ssh::SshCommandOutput,
}

#[derive(Clone)]
struct PreparationProbeData {
    install_platform_ok: bool,
    uninstall_platform_ok: bool,
    sh_ok: bool,
    install_write_ok: bool,
    uninstall_write_ok: bool,
    detail: String,
}

fn preparation_probe_script() -> &'static str {
    r#"set -u
os=$(uname -s 2>/dev/null || true)
arch=$(uname -m 2>/dev/null || true)
case "$os:$arch" in
  Linux:x86_64 | Linux:amd64 | Linux:aarch64 | Linux:arm64) install_platform_ok=yes ;;
  *) install_platform_ok=no ;;
esac
case "$os" in Linux | Darwin) uninstall_platform_ok=yes ;; *) uninstall_platform_ok=no ;; esac
if command -v sh >/dev/null 2>&1; then sh_ok=yes; else sh_ok=no; fi
if command -v tar >/dev/null 2>&1; then tar_ok=yes; else tar_ok=no; fi
if command -v curl >/dev/null 2>&1; then curl_ok=yes; else curl_ok=no; fi
if command -v wget >/dev/null 2>&1; then wget_ok=yes; else wget_ok=no; fi
if command -v python3 >/dev/null 2>&1; then python_ok=yes; else python_ok=no; fi
if command -v npm >/dev/null 2>&1; then npm_ok=yes; else npm_ok=no; fi
install_write_ok=yes
uninstall_write_ok=yes
[ -n "${HOME:-}" ] && [ -d "$HOME" ] && [ -w "$HOME" ] || { install_write_ok=no; uninstall_write_ok=no; }
for dir in "$HOME/.local" "$HOME/.local/bin" "$HOME/.codex"; do
  if [ -e "$dir" ] && [ ! -w "$dir" ]; then install_write_ok=no; fi
done
for dir in "$HOME/.local" "$HOME/.codex" "$HOME/.codex-hub" "$HOME/.cache/codex" "$HOME/.config/codex" "$HOME/.local/share/codex" "$HOME/.local/state/codex"; do
  if [ -e "$dir" ] && [ ! -w "$dir" ]; then uninstall_write_ok=no; fi
done
printf 'CODEXHUB_INSTALL_PLATFORM_OK=%s\n' "$install_platform_ok"
printf 'CODEXHUB_UNINSTALL_PLATFORM_OK=%s\n' "$uninstall_platform_ok"
printf 'CODEXHUB_SH_OK=%s\n' "$sh_ok"
printf 'CODEXHUB_INSTALL_WRITE_OK=%s\n' "$install_write_ok"
printf 'CODEXHUB_UNINSTALL_WRITE_OK=%s\n' "$uninstall_write_ok"
printf 'CODEXHUB_PREPARATION_DETAIL=%s/%s; sh=%s; tar=%s; curl=%s; wget=%s; python3=%s; npm=%s; install-dirs-writable=%s; uninstall-dirs-writable=%s\n' "$os" "$arch" "$sh_ok" "$tar_ok" "$curl_ok" "$wget_ok" "$python_ok" "$npm_ok" "$install_write_ok" "$uninstall_write_ok"
"#
}

fn parse_preparation_probe(output: &ssh::SshCommandOutput) -> Result<PreparationProbeData, String> {
    Ok(PreparationProbeData {
        install_platform_ok: parse_yes_marker(output, "CODEXHUB_INSTALL_PLATFORM_OK")?,
        uninstall_platform_ok: parse_yes_marker(output, "CODEXHUB_UNINSTALL_PLATFORM_OK")?,
        sh_ok: parse_yes_marker(output, "CODEXHUB_SH_OK")?,
        install_write_ok: parse_yes_marker(output, "CODEXHUB_INSTALL_WRITE_OK")?,
        uninstall_write_ok: parse_yes_marker(output, "CODEXHUB_UNINSTALL_WRITE_OK")?,
        detail: marker_required(output, "CODEXHUB_PREPARATION_DETAIL")?,
    })
}

fn install_prerequisites_ready(probe: &Result<PreparationProbeData, String>) -> bool {
    probe
        .as_ref()
        .map(|data| data.install_platform_ok && data.sh_ok && data.install_write_ok)
        .unwrap_or(false)
}

fn uninstall_prerequisites_ready(probe: &Result<PreparationProbeData, String>) -> bool {
    probe
        .as_ref()
        .map(|data| data.uninstall_platform_ok && data.sh_ok && data.uninstall_write_ok)
        .unwrap_or(false)
}

fn run_codex_state_checks_parallel(alias: &str, timeout: u64, phase: &str) -> CodexStateChecks {
    fn run(alias: &str, label: &str, script: &str, timeout: u64) -> ssh::SshCommandOutput {
        ssh::run_ssh_script(alias, script, timeout).unwrap_or_else(|error| {
            failed_command_output(
                format!("ssh {alias} {label}"),
                format!("Could not run ssh: {error}"),
            )
        })
    }
    std::thread::scope(|scope| {
        let path_script = codex_path_probe_script();
        let version_script = codex_version_probe_script();
        let path = scope.spawn(move || {
            run(
                alias,
                &format!("resolve codex {phase}"),
                &path_script,
                timeout,
            )
        });
        let current_available = scope.spawn(|| {
            run(
                alias,
                &format!("check codex command in current shell {phase}"),
                "command -v codex",
                timeout,
            )
        });
        let login_available = scope.spawn(|| {
            run(
                alias,
                &format!("check codex command in login shell {phase}"),
                r#"login_shell=${SHELL:-}
[ -n "$login_shell" ] && [ -x "$login_shell" ] || exit 127
"$login_shell" -lc 'command -v codex'"#,
                timeout,
            )
        });
        let version = scope.spawn(move || {
            run(
                alias,
                &format!("codex --version {phase}"),
                &version_script,
                timeout,
            )
        });
        let shell_path = scope.spawn(|| {
            run(
                alias,
                &format!("echo PATH {phase}"),
                "printf '%s\\n' \"$PATH\"",
                timeout,
            )
        });
        CodexStateChecks {
            codex_path: path.join().unwrap_or_else(|_| {
                failed_command_output("resolve codex".into(), "Codex path worker panicked.".into())
            }),
            current_command_available: current_available.join().unwrap_or_else(|_| {
                failed_command_output(
                    "check codex command in current shell".into(),
                    "Current-shell Codex worker panicked.".into(),
                )
            }),
            login_command_available: login_available.join().unwrap_or_else(|_| {
                failed_command_output(
                    "check codex command in login shell".into(),
                    "Login-shell Codex worker panicked.".into(),
                )
            }),
            version: version.join().unwrap_or_else(|_| {
                failed_command_output(
                    "codex --version".into(),
                    "Codex version worker panicked.".into(),
                )
            }),
            shell_path: shell_path.join().unwrap_or_else(|_| {
                failed_command_output("echo PATH".into(), "Remote PATH worker panicked.".into())
            }),
        }
    })
}

fn append_state_check_logs(
    task_id: &str,
    logs: &mut Vec<TaskLog>,
    next_log_index: &mut usize,
    checks: &CodexStateChecks,
    phase: &str,
) {
    for (label, output, failure_level) in [
        ("resolve codex", &checks.codex_path, TaskLogLevel::Warn),
        (
            "check codex command in current shell",
            &checks.current_command_available,
            TaskLogLevel::Warn,
        ),
        (
            "check codex command in login shell",
            &checks.login_command_available,
            TaskLogLevel::Warn,
        ),
        ("codex --version", &checks.version, TaskLogLevel::Warn),
        ("echo PATH", &checks.shell_path, TaskLogLevel::Info),
    ] {
        let level = if output.success() {
            TaskLogLevel::Info
        } else {
            failure_level
        };
        let message = if output.success() {
            format!("{label} {phase} completed.")
        } else {
            format!("{label} {phase} failed: {}", command_detail(output))
        };
        logs.push(command_log(
            task_id,
            *next_log_index,
            level,
            &message,
            output,
        ));
        *next_log_index += 1;
    }
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
    let mut running = jobs::begin_task(
        &state.task_store,
        state.task_event_sink.as_ref(),
        &task_id,
        &host_id,
        &host_name,
        action_label,
    )?;
    let definitions = match action {
        RemoteCodexAction::Uninstall => UNINSTALL_STEP_IDS,
        RemoteCodexAction::Install | RemoteCodexAction::Update => INSTALL_STEP_IDS,
        RemoteCodexAction::CheckVersion => &[
            ("preparation", "Preparing to inspect Codex."),
            (
                "final-verification",
                "Waiting to inspect the current Codex state.",
            ),
        ],
    };
    initialize_operation_steps(state, &mut running, definitions)?;
    let request_id = host_operation_request_id(request_id, "codex-operation");
    let progress = CodexProgressContext {
        app,
        request_id: Some(&request_id),
        host_alias: &alias,
        action: &action,
    };
    let host_progress = HostProgressContext {
        app,
        state,
        task_id: &task_id,
        request_id: &request_id,
        host_alias: &alias,
        operation: host_operation_kind(&action),
    };
    persist_and_emit_step(
        &host_progress,
        "preparation",
        TaskStepStatus::Running,
        format!("Preparing {alias} for {action_label}."),
        None,
    )?;

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
    let mut next_log_index = logs.len() + 1;
    logs.push(command_log(
        &task_id,
        next_log_index,
        if check_ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        &check_message,
        &check_output,
    ));
    next_log_index += 1;
    if let Some(log) = logs.last_mut() {
        log.step_id = Some("preparation".into());
    }
    emit_remote_codex_progress_for_output(
        Some(&progress),
        "ssh-check",
        if check_ok { "success" } else { "failed" },
        &check_message,
        &check_output,
    );

    if check_ok {
        persist_and_emit_step(
            &host_progress,
            "preparation",
            TaskStepStatus::Running,
            check_message.clone(),
            logs.last().cloned(),
        )?;
    }

    if !check_ok {
        update_host_check(state, &alias, false, check_output.duration_ms);
        let message = format!("{action_label} skipped because SSH check failed: {check_message}");
        persist_and_emit_step(
            &host_progress,
            "preparation",
            TaskStepStatus::Failed,
            check_message.clone(),
            logs.last().cloned(),
        )?;
        for (step_id, _) in definitions.iter().skip(1) {
            persist_and_emit_step(
                &host_progress,
                step_id,
                TaskStepStatus::Skipped,
                "Not run because preparation failed.",
                None,
            )?;
        }
        let mut task = codex_maintenance_task(
            &task_id,
            &host_id,
            &host_name,
            action_label,
            TaskStatus::Failed,
            &message,
            logs,
        );
        task.steps = state
            .task_store
            .get(&task_id)?
            .map(|task| task.steps)
            .unwrap_or_default();
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

    let preparation_log_start = logs.len();
    let (before_checks, preparation_output) = std::thread::scope(|scope| {
        let preparation = scope.spawn(|| {
            probe_group_output(&alias, "preparation", preparation_probe_script(), timeout)
        });
        let before_checks = run_codex_state_checks_parallel(&alias, timeout, "before maintenance");
        let preparation_output = preparation.join().unwrap_or_else(|_| {
            failed_command_output(
                "probe host preparation".into(),
                "Host preparation worker panicked.".into(),
            )
        });
        (before_checks, preparation_output)
    });
    append_state_check_logs(
        &task_id,
        &mut logs,
        &mut next_log_index,
        &before_checks,
        "before maintenance",
    );
    let state_check_log_end = logs.len();
    let preparation_data = if preparation_output.success() {
        parse_preparation_probe(&preparation_output)
    } else {
        Err(command_detail(&preparation_output))
    };
    let preparation_message = match &preparation_data {
        Ok(data) => format!("Host prerequisites checked: {}.", data.detail),
        Err(error) => format!(
            "Host prerequisite checks failed: {}",
            redact_error_text(error)
        ),
    };
    logs.push(command_log(
        &task_id,
        next_log_index,
        if preparation_data.is_ok() {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        &preparation_message,
        &preparation_output,
    ));
    next_log_index += 1;
    let before_command_available = before_checks.current_command_available.success()
        || before_checks.login_command_available.success();
    let before_version = output_trimmed(&before_checks.version);

    if action == RemoteCodexAction::CheckVersion {
        persist_step_logs(
            &host_progress,
            "preparation",
            &mut logs[state_check_log_end..],
        )?;
        persist_and_emit_step(
            &host_progress,
            "preparation",
            TaskStepStatus::Success,
            "SSH preparation completed.",
            None,
        )?;
        persist_and_emit_step(
            &host_progress,
            "final-verification",
            TaskStepStatus::Running,
            "Inspecting the current Codex state.",
            None,
        )?;
        persist_step_logs(
            &host_progress,
            "final-verification",
            &mut logs[preparation_log_start..state_check_log_end],
        )?;
        let codex_path = output_trimmed(&before_checks.codex_path);
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
        persist_and_emit_step(
            &host_progress,
            "final-verification",
            if ok {
                TaskStepStatus::Success
            } else {
                TaskStepStatus::Failed
            },
            message.clone(),
            None,
        )?;
        let mut task = codex_maintenance_task(
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
        task.steps = state
            .task_store
            .get(&task_id)?
            .map(|task| task.steps)
            .unwrap_or_default();
        update_host_codex_status(
            state,
            &alias,
            installed,
            &version_label,
            path_has_local_bin(output_trimmed(&before_checks.shell_path).as_deref()),
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
        persist_step_logs(
            &host_progress,
            "preparation",
            &mut logs[preparation_log_start..],
        )?;
        if !uninstall_prerequisites_ready(&preparation_data) {
            let message = match &preparation_data {
                Ok(data) => format!(
                    "{action_label} cannot start because uninstall prerequisites are not satisfied: {}.",
                    data.detail
                ),
                Err(error) => format!(
                    "{action_label} cannot start because host prerequisites could not be checked: {}",
                    redact_error_text(error)
                ),
            };
            persist_and_emit_step(
                &host_progress,
                "preparation",
                TaskStepStatus::Failed,
                message.clone(),
                None,
            )?;
            for (step_id, _) in UNINSTALL_STEP_IDS.iter().skip(1) {
                persist_and_emit_step(
                    &host_progress,
                    step_id,
                    TaskStepStatus::Skipped,
                    "Not run because uninstall prerequisites failed.",
                    None,
                )?;
            }
            let mut task = codex_maintenance_task(
                &task_id,
                &host_id,
                &host_name,
                action_label,
                TaskStatus::Failed,
                &message,
                logs,
            );
            task.steps = state
                .task_store
                .get(&task_id)?
                .map(|task| task.steps)
                .unwrap_or_default();
            record_task(state, task.clone())?;
            let (result_before_version, result_after_version) =
                retained_codex_versions(before_version);
            return Ok(RemoteCodexMaintenanceResult {
                host_alias: alias,
                ok: false,
                action,
                before_version: result_before_version,
                after_version: result_after_version,
                codex_path: output_trimmed(&before_checks.codex_path),
                codex_command_available: before_command_available,
                install_method: None,
                path_changed: false,
                shell_config_path: None,
                backup_path: None,
                message,
                task,
            });
        }
        persist_and_emit_step(
            &host_progress,
            "preparation",
            TaskStepStatus::Success,
            "SSH and current Codex checks completed.",
            None,
        )?;
        persist_and_emit_step(
            &host_progress,
            "uninstall",
            TaskStepStatus::Running,
            "Removing the managed Codex installation.",
            None,
        )?;
        let uninstall_log_start = logs.len();
        let uninstall_script =
            codex_runtime::with_remote_codex_runtime_writer_lock(CODEX_UNINSTALL_SCRIPT);
        let uninstall_output = run_codex_step(
            &alias,
            &task_id,
            &mut logs,
            &mut next_log_index,
            action_label,
            &uninstall_script,
            timeout,
            TaskLogLevel::Error,
            Some(&progress),
        );
        persist_step_logs(
            &host_progress,
            "uninstall",
            &mut logs[uninstall_log_start..],
        )?;
        let uninstall_method = marker_value(&uninstall_output.stdout, "CODEXHUB_UNINSTALL_METHOD")
            .filter(|value| value != "unsupported");
        let backup_path = marker_value(&uninstall_output.stdout, "CODEXHUB_BACKUP_PATH");
        persist_and_emit_step(
            &host_progress,
            "uninstall",
            if uninstall_output.success() {
                TaskStepStatus::Success
            } else {
                TaskStepStatus::Failed
            },
            if uninstall_output.success() {
                "The uninstall command completed."
            } else {
                "The uninstall command failed."
            },
            None,
        )?;
        persist_and_emit_step(
            &host_progress,
            "final-verification",
            TaskStepStatus::Running,
            "Verifying that Codex is no longer available.",
            None,
        )?;
        let verification_log_start = logs.len();
        let after_checks = run_codex_state_checks_parallel(&alias, timeout, "after uninstall");
        append_state_check_logs(
            &task_id,
            &mut logs,
            &mut next_log_index,
            &after_checks,
            "after uninstall",
        );
        persist_step_logs(
            &host_progress,
            "final-verification",
            &mut logs[verification_log_start..],
        )?;

        let codex_path = output_trimmed(&after_checks.codex_path);
        let codex_command_available = after_checks.current_command_available.success()
            || after_checks.login_command_available.success();
        let after_version = output_trimmed(&after_checks.version);
        let installed = codex_path.is_some() || after_version.is_some();
        let ok = uninstall_target_reached(
            codex_path.is_some(),
            after_version.is_some(),
            after_checks.current_command_available.success(),
            after_checks.login_command_available.success(),
        );
        let version_label = after_version.clone().unwrap_or_else(|| {
            if codex_path.is_some() {
                "unknown".into()
            } else {
                "not installed".into()
            }
        });
        let message = if ok && uninstall_output.success() {
            format!("{action_label} completed on {alias}; Codex is no longer available.")
        } else if ok {
            format!(
                "{action_label} reached its final target on {alias}; Codex is no longer available even though the uninstall command reported: {}",
                command_detail(&uninstall_output)
            )
        } else if uninstall_output.success() {
            format!(
                "{action_label} completed on {alias}, but another Codex command is still available: {version_label}."
            )
        } else {
            format!(
                "{action_label} failed on {alias}, and final verification still found Codex {version_label}: {}",
                command_detail(&uninstall_output),
            )
        };
        persist_and_emit_step(
            &host_progress,
            "final-verification",
            if ok {
                TaskStepStatus::Success
            } else {
                TaskStepStatus::Failed
            },
            message.clone(),
            None,
        )?;
        let mut task = codex_maintenance_task(
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
        task.steps = state
            .task_store
            .get(&task_id)?
            .map(|task| task.steps)
            .unwrap_or_default();
        update_host_codex_status(
            state,
            &alias,
            installed,
            &version_label,
            path_has_local_bin(output_trimmed(&after_checks.shell_path).as_deref()),
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

    let strict_current_runtime =
        codex_runtime::probe_remote_strict_current_version(&alias, timeout);
    let current_floor_message = match &strict_current_runtime {
        Ok(Some(runtime)) => format!(
            "Verified standalone/current {} as an additional update downgrade floor.",
            runtime.version
        ),
        Ok(None) => "No standalone/current release was present before maintenance.".into(),
        Err(error) => format!(
            "Could not establish a safe standalone/current update floor: {}",
            redact_error_text(error)
        ),
    };
    logs.push(basic_log(
        &task_id,
        next_log_index,
        if strict_current_runtime.is_ok() {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        &current_floor_message,
    ));
    next_log_index += 1;
    let prerequisites_ready =
        install_prerequisites_ready(&preparation_data) && strict_current_runtime.is_ok();
    if !prerequisites_ready {
        persist_step_logs(
            &host_progress,
            "preparation",
            &mut logs[preparation_log_start..],
        )?;
        let message = if let Err(error) = &strict_current_runtime {
            format!(
                "{action_label} cannot start because standalone/current could not be verified safely: {}",
                redact_error_text(error)
            )
        } else {
            match preparation_data {
                Ok(data) => format!(
                    "{action_label} cannot start because host prerequisites are not satisfied: {}.",
                    data.detail
                ),
                Err(error) => format!(
                "{action_label} cannot start because host prerequisites could not be checked: {}",
                redact_error_text(&error)
            ),
            }
        };
        persist_and_emit_step(
            &host_progress,
            "preparation",
            TaskStepStatus::Failed,
            message.clone(),
            None,
        )?;
        for (step_id, _) in INSTALL_STEP_IDS.iter().skip(1) {
            persist_and_emit_step(
                &host_progress,
                step_id,
                TaskStepStatus::Skipped,
                "Not run because host prerequisites failed.",
                None,
            )?;
        }
        let mut task = codex_maintenance_task(
            &task_id,
            &host_id,
            &host_name,
            action_label,
            TaskStatus::Failed,
            &message,
            logs,
        );
        task.steps = state
            .task_store
            .get(&task_id)?
            .map(|task| task.steps)
            .unwrap_or_default();
        record_task(state, task.clone())?;
        return Ok(RemoteCodexMaintenanceResult {
            host_alias: alias,
            ok: false,
            action,
            before_version,
            after_version: None,
            codex_path: output_trimmed(&before_checks.codex_path),
            codex_command_available: before_command_available,
            install_method: None,
            path_changed: false,
            shell_config_path: None,
            backup_path: None,
            message,
            task,
        });
    }
    let strict_current_runtime = strict_current_runtime
        .expect("strict current version was checked by the preparation guard");

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
    persist_step_logs(
        &host_progress,
        "preparation",
        &mut logs[preparation_log_start..],
    )?;
    if !path_repair_output.success() {
        let message = format!(
            "{action_label} stopped because host preparation failed: {}",
            command_detail(&path_repair_output)
        );
        persist_and_emit_step(
            &host_progress,
            "preparation",
            TaskStepStatus::Failed,
            message.clone(),
            None,
        )?;
        for (step_id, _) in INSTALL_STEP_IDS.iter().skip(1) {
            persist_and_emit_step(
                &host_progress,
                step_id,
                TaskStepStatus::Skipped,
                "Not run because preparation failed.",
                None,
            )?;
        }
        let mut task = codex_maintenance_task(
            &task_id,
            &host_id,
            &host_name,
            action_label,
            TaskStatus::Failed,
            &message,
            logs,
        );
        task.steps = state
            .task_store
            .get(&task_id)?
            .map(|task| task.steps)
            .unwrap_or_default();
        record_task(state, task.clone())?;
        return Ok(RemoteCodexMaintenanceResult {
            host_alias: alias,
            ok: false,
            action,
            before_version,
            after_version: None,
            codex_path: output_trimmed(&before_checks.codex_path),
            codex_command_available: before_command_available,
            install_method: None,
            path_changed,
            shell_config_path,
            backup_path,
            message,
            task,
        });
    }
    persist_and_emit_step(
        &host_progress,
        "preparation",
        TaskStepStatus::Success,
        "SSH checks, current-state checks, and PATH preparation completed.",
        None,
    )?;

    let method_steps = [
        "official-installer",
        "remote-native-mirror",
        "remote-npm-mirror",
        "local-upload",
    ];
    let proxy_settings = read_settings(&state.paths).unwrap_or_else(|error| {
        eprintln!(
            "Could not read proxy settings for remote Codex installation: {}",
            redact_error_text(&error)
        );
        let mut settings = AppSettings::default();
        settings.network_proxy_mode = NetworkProxyMode::Direct;
        settings
    });
    let mut successful_install = None;
    let mut installation_failure_summary = None;
    let mut attempt_outcomes = Vec::new();
    while let Some(method_index) = next_installation_method(&attempt_outcomes) {
        let step_id = method_steps[method_index];
        persist_and_emit_step(
            &host_progress,
            step_id,
            TaskStepStatus::Running,
            format!("Trying installation method: {step_id}."),
            None,
        )?;
        let method_log_start = logs.len();
        let output = match step_id {
            "official-installer" => run_official_codex_installer(
                &alias,
                &task_id,
                &mut logs,
                &mut next_log_index,
                before_version.as_deref(),
                strict_current_runtime
                    .as_ref()
                    .map(|runtime| runtime.version.as_str()),
                &proxy_settings,
                Some(&progress),
            ),
            "remote-native-mirror" => run_remote_native_mirror_install(
                &alias,
                &task_id,
                &mut logs,
                &mut next_log_index,
                timeout,
                before_version.as_deref(),
                strict_current_runtime
                    .as_ref()
                    .map(|runtime| runtime.version.as_str()),
                Some(&progress),
            ),
            "remote-npm-mirror" => run_remote_npm_mirror_install(
                &alias,
                &task_id,
                &mut logs,
                &mut next_log_index,
                timeout,
                before_version.as_deref(),
                strict_current_runtime
                    .as_ref()
                    .map(|runtime| runtime.version.as_str()),
                Some(&progress),
            ),
            "local-upload" => run_local_upload_codex_fallback(
                &state.paths,
                &alias,
                &task_id,
                &mut logs,
                &mut next_log_index,
                timeout,
                before_version.as_deref(),
                strict_current_runtime
                    .as_ref()
                    .map(|runtime| runtime.version.as_str()),
                Some(&progress),
            ),
            _ => unreachable!("stable installation step"),
        };
        let mut method_succeeded = output.success();
        let mut validation_failure = None;
        if method_succeeded {
            let candidate_output = probe_group_output(
                &alias,
                &format!("{step_id}-candidate"),
                &codex_probe_group_script(),
                timeout,
            );
            let candidate = if candidate_output.success() {
                parse_codex_probe(&candidate_output)
            } else {
                Err(command_detail(&candidate_output))
            };
            method_succeeded = candidate
                .as_ref()
                .map(|candidate| {
                    candidate.installed
                        && candidate.path.is_some()
                        && !candidate.version.trim().is_empty()
                        && candidate.version != "unavailable"
                })
                .unwrap_or(false);
            let candidate_message = if method_succeeded {
                format!("Resolved a working Codex candidate after {step_id}.")
            } else {
                let detail = candidate
                    .err()
                    .unwrap_or_else(|| "No versioned Codex candidate was found.".into());
                validation_failure = Some(detail.clone());
                format!(
                    "Candidate validation after {step_id} failed: {}",
                    redact_error_text(&detail)
                )
            };
            logs.push(command_log(
                &task_id,
                next_log_index,
                if method_succeeded {
                    TaskLogLevel::Info
                } else {
                    TaskLogLevel::Error
                },
                &candidate_message,
                &candidate_output,
            ));
            next_log_index += 1;
        }
        attempt_outcomes.push(method_succeeded);
        if !method_succeeded {
            installation_failure_summary = Some(
                validation_failure
                    .clone()
                    .unwrap_or_else(|| command_detail(&output)),
            );
        }
        persist_step_logs(&host_progress, step_id, &mut logs[method_log_start..])?;
        if method_succeeded {
            let method = marker_value(&output.stdout, "CODEXHUB_INSTALL_METHOD")
                .filter(|value| value != "failed")
                .unwrap_or_else(|| step_id.to_string());
            persist_and_emit_step(
                &host_progress,
                step_id,
                TaskStepStatus::Success,
                format!("Installation method succeeded: {method}."),
                None,
            )?;
            successful_install = Some(method);
            for skipped in method_steps.iter().skip(method_index + 1) {
                persist_and_emit_step(
                    &host_progress,
                    skipped,
                    TaskStepStatus::Skipped,
                    "Not needed because an earlier installation method succeeded.",
                    None,
                )?;
            }
            continue;
        }
        persist_and_emit_step(
            &host_progress,
            step_id,
            TaskStepStatus::Failed,
            validation_failure
                .map(|error| {
                    format!(
                        "The method ran, but candidate validation failed: {}. Trying the next method.",
                        redact_error_text(&error)
                    )
                })
                .unwrap_or_else(|| {
                    "This installation method failed; trying the next method.".into()
                }),
            None,
        )?;
    }
    let install_method = successful_install.clone();
    let has_runtime_recovery_floor = before_version.is_some() || strict_current_runtime.is_some();
    let should_reconcile_runtime = successful_install.is_some() || has_runtime_recovery_floor;

    let runtime_reconcile_result = if should_reconcile_runtime {
        persist_and_emit_step(
            &host_progress,
            "runtime-reconcile",
            TaskStepStatus::Running,
            if successful_install.is_some() {
                "Reconciling the managed launcher, runtime target, and login-shell command."
            } else {
                "Restoring and verifying the pre-maintenance Codex runtime after all installation methods failed."
            },
            None,
        )?;
        let result = codex_runtime::reconcile_remote_codex_runtime(
            &alias,
            timeout,
            before_version.as_deref(),
            strict_current_runtime.as_ref(),
        );
        match result {
            Ok(result)
                if result.completed()
                    && matches!(
                        result.status,
                        CodexRuntimeReconcileStatus::Coordinated
                            | CodexRuntimeReconcileStatus::Unchanged
                    ) =>
            {
                let summary = result.safe_summary();
                let mut log = basic_log(&task_id, next_log_index, TaskLogLevel::Info, &summary);
                next_log_index += 1;
                log.step_id = Some("runtime-reconcile".into());
                logs.push(log.clone());
                persist_and_emit_step(
                    &host_progress,
                    "runtime-reconcile",
                    TaskStepStatus::Success,
                    summary,
                    Some(log),
                )?;
                Some(result)
            }
            Ok(result) => {
                let summary = format!(
                    "Managed runtime reconciliation did not produce a verified installed runtime: {}",
                    result.safe_summary()
                );
                let mut log = basic_log(&task_id, next_log_index, TaskLogLevel::Error, &summary);
                next_log_index += 1;
                log.step_id = Some("runtime-reconcile".into());
                logs.push(log.clone());
                persist_and_emit_step(
                    &host_progress,
                    "runtime-reconcile",
                    TaskStepStatus::Failed,
                    summary,
                    Some(log),
                )?;
                None
            }
            Err(error) => {
                let summary = format!(
                    "Managed runtime reconciliation failed: {}",
                    redact_error_text(&error)
                );
                let mut log = basic_log(&task_id, next_log_index, TaskLogLevel::Error, &summary);
                next_log_index += 1;
                log.step_id = Some("runtime-reconcile".into());
                logs.push(log.clone());
                persist_and_emit_step(
                    &host_progress,
                    "runtime-reconcile",
                    TaskStepStatus::Failed,
                    summary,
                    Some(log),
                )?;
                None
            }
        }
    } else {
        persist_and_emit_step(
            &host_progress,
            "runtime-reconcile",
            TaskStepStatus::Skipped,
            "Not run because no installation method produced a verified candidate and no pre-maintenance runtime floor was available.",
            None,
        )?;
        None
    };

    persist_and_emit_step(
        &host_progress,
        "final-verification",
        TaskStepStatus::Running,
        "Verifying the final Codex path, version, and shell availability.",
        None,
    )?;
    let verification_log_start = logs.len();
    let after_checks = run_codex_state_checks_parallel(&alias, timeout, "after maintenance");
    append_state_check_logs(
        &task_id,
        &mut logs,
        &mut next_log_index,
        &after_checks,
        "after maintenance",
    );
    persist_step_logs(
        &host_progress,
        "final-verification",
        &mut logs[verification_log_start..],
    )?;

    let codex_path = output_trimmed(&after_checks.codex_path);
    let current_command_available = after_checks.current_command_available.success();
    let login_command_available = after_checks.login_command_available.success();
    let codex_command_available =
        codex_command_available_in_any_shell(current_command_available, login_command_available);
    let after_version = output_trimmed(&after_checks.version);
    // Persist the final remote truth independently from whether this operation
    // installed anything. A failed update can still restore a valid old runtime.
    let detected_installed =
        detected_codex_installed(codex_path.is_some(), after_version.is_some());
    let mut verification_failures = final_verification_failures(
        successful_install.is_some(),
        codex_path.is_some(),
        after_version.is_some(),
        current_command_available,
        login_command_available,
    );
    if runtime_reconcile_result.is_none() {
        verification_failures.push("managed runtime reconciliation");
    }
    let runtime_version_matches = runtime_reconcile_result.as_ref().is_some_and(|result| {
        let target = result.target_version.as_deref();
        let after = after_version
            .as_deref()
            .and_then(codex_runtime::normalized_codex_version);
        target == after.as_deref()
    });
    if runtime_reconcile_result.is_some() && !runtime_version_matches {
        verification_failures.push("reconciled runtime version");
    }
    let verification_ok = verification_failures.is_empty();
    let version_label = after_version
        .clone()
        .unwrap_or_else(|| "not installed".into());
    let verification_message = if verification_ok {
        final_verification_success_message(
            action_label,
            &alias,
            &version_label,
            current_command_available,
        )
    } else if successful_install.is_none() {
        format!(
            "{action_label} failed on {alias}: no installation method produced a verifiable Codex candidate. Last failure: {}",
            installation_failure_summary
                .as_deref()
                .unwrap_or("no method completed successfully")
        )
    } else {
        format!(
            "{action_label} installed a candidate on {alias}, but final verification is missing: {}.",
            verification_failures.join(", ")
        )
    };
    persist_and_emit_step(
        &host_progress,
        "final-verification",
        if verification_ok {
            TaskStepStatus::Success
        } else {
            TaskStepStatus::Failed
        },
        verification_message.clone(),
        None,
    )?;

    let cleanup_ok = if verification_ok {
        let update_cleanup = action == RemoteCodexAction::Update;
        persist_and_emit_step(
            &host_progress,
            "release-cleanup",
            TaskStepStatus::Running,
            if update_cleanup {
                "Adopting strictly identifiable older releases and moving them into a verified staged backup."
            } else {
                "Checking obsolete managed standalone releases and launcher captures for safe removal."
            },
            None,
        )?;
        let cleanup_result = if update_cleanup {
            after_version.as_ref().map_or_else(
                || {
                    Err(
                        "The verified update version is unavailable for old-release cleanup."
                            .into(),
                    )
                },
                |version| {
                    codex_runtime::cleanup_remote_codex_releases(
                        &alias,
                        timeout,
                        CodexReleaseCleanupPolicy::VerifiedOlderThan(version.clone()),
                    )
                },
            )
        } else {
            codex_runtime::cleanup_remote_codex_releases(
                &alias,
                timeout,
                CodexReleaseCleanupPolicy::ManagedOnly,
            )
        };
        match cleanup_result {
            Ok(result) => {
                let cleanup_ok = !result.hard_failed();
                let level = match result.status {
                    CodexReleaseCleanupStatus::Deferred => TaskLogLevel::Warn,
                    CodexReleaseCleanupStatus::Failed => TaskLogLevel::Error,
                    _ => TaskLogLevel::Info,
                };
                let summary = result.safe_summary();
                let mut log = basic_log(&task_id, next_log_index, level, &summary);
                log.step_id = Some("release-cleanup".into());
                logs.push(log.clone());
                let step_status = match result.status {
                    CodexReleaseCleanupStatus::Deferred => TaskStepStatus::Skipped,
                    CodexReleaseCleanupStatus::Failed => TaskStepStatus::Failed,
                    CodexReleaseCleanupStatus::Completed
                    | CodexReleaseCleanupStatus::NotApplicable => TaskStepStatus::Success,
                };
                persist_and_emit_step(
                    &host_progress,
                    "release-cleanup",
                    step_status,
                    summary,
                    Some(log),
                )?;
                cleanup_ok
            }
            Err(error) => {
                let summary = format!(
                    "Managed runtime cleanup could not be verified: {}",
                    redact_error_text(&error)
                );
                let mut log = basic_log(&task_id, next_log_index, TaskLogLevel::Error, &summary);
                log.step_id = Some("release-cleanup".into());
                logs.push(log.clone());
                persist_and_emit_step(
                    &host_progress,
                    "release-cleanup",
                    TaskStepStatus::Failed,
                    summary,
                    Some(log),
                )?;
                false
            }
        }
    } else {
        persist_and_emit_step(
            &host_progress,
            "release-cleanup",
            TaskStepStatus::Skipped,
            "Not run because final runtime verification failed.",
            None,
        )?;
        true
    };
    let ok = verification_ok && cleanup_ok;
    let message = if verification_ok && !cleanup_ok {
        format!(
            "{verification_message} The verified new runtime remains active, but managed runtime cleanup failed."
        )
    } else {
        verification_message
    };
    let mut task = codex_maintenance_task(
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
    task.steps = state
        .task_store
        .get(&task_id)?
        .map(|task| task.steps)
        .unwrap_or_default();
    update_host_codex_status(
        state,
        &alias,
        detected_installed,
        &version_label,
        path_has_local_bin(output_trimmed(&after_checks.shell_path).as_deref()),
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

pub(crate) fn run_batch_remote_update_codex(
    app: &AppHandle,
    state: &AppState,
    host_aliases: Vec<String>,
    timeout_ms: Option<u64>,
    request_id: Option<String>,
) -> Result<RemoteCodexBatchResult, String> {
    let request_id = host_operation_request_id(request_id, "batch-codex-update");
    let item_count = host_aliases.len();
    if item_count == 0 {
        return Ok(RemoteCodexBatchResult {
            request_id,
            action: RemoteCodexAction::Update,
            results: Vec::new(),
        });
    }
    // Workers pull the next host immediately after finishing, forming a sliding pool.
    let results = run_bounded_ordered(host_aliases, |host_alias| {
        match run_remote_manage_codex(
            app,
            state,
            host_alias.clone(),
            RemoteCodexAction::Update,
            timeout_ms,
            Some(request_id.clone()),
        ) {
            Ok(result) => RemoteCodexBatchItem {
                host_alias,
                ok: result.ok,
                result: Some(result),
                error: None,
            },
            Err(error) => RemoteCodexBatchItem {
                host_alias,
                ok: false,
                result: None,
                error: Some(redact_error_text(&error)),
            },
        }
    });
    Ok(RemoteCodexBatchResult {
        request_id,
        action: RemoteCodexAction::Update,
        results,
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
    run_codex_step_with_tunnel(
        alias,
        task_id,
        logs,
        next_log_index,
        label,
        script,
        timeout,
        failure_level,
        None,
        false,
        progress,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_codex_step_with_tunnel(
    alias: &str,
    task_id: &str,
    logs: &mut Vec<TaskLog>,
    next_log_index: &mut usize,
    label: &str,
    script: &str,
    timeout: u64,
    failure_level: TaskLogLevel,
    tunnel: Option<&ssh::ReverseProxyTunnel>,
    extended_timeout: bool,
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
        match (extended_timeout, tunnel) {
            (true, Some(tunnel)) => {
                ssh::run_ssh_script_streaming_with_extended_timeout_and_reverse_proxy(
                    alias,
                    script,
                    timeout,
                    tunnel,
                    |event| emit_remote_codex_stream_event(progress, label, event),
                )
            }
            (true, None) => ssh::run_ssh_script_streaming_with_extended_timeout(
                alias,
                script,
                timeout,
                |event| emit_remote_codex_stream_event(progress, label, event),
            ),
            (false, Some(tunnel)) => ssh::run_ssh_script_streaming_with_reverse_proxy(
                alias,
                script,
                timeout,
                tunnel,
                |event| emit_remote_codex_stream_event(progress, label, event),
            ),
            (false, None) => ssh::run_ssh_script_streaming(alias, script, timeout, |event| {
                emit_remote_codex_stream_event(progress, label, event);
            }),
        }
    } else {
        match (extended_timeout, tunnel) {
            (true, Some(tunnel)) => ssh::run_ssh_script_with_extended_timeout_and_reverse_proxy(
                alias, script, timeout, tunnel,
            ),
            (true, None) => ssh::run_ssh_script_with_extended_timeout(alias, script, timeout),
            (false, Some(tunnel)) => {
                ssh::run_ssh_script_with_reverse_proxy(alias, script, timeout, tunnel)
            }
            (false, None) => ssh::run_ssh_script(alias, script, timeout),
        }
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

fn official_codex_installer_script(
    minimum_version: Option<&str>,
    minimum_current_version: Option<&str>,
    proxy_url: Option<&str>,
) -> String {
    let proxy_exports = proxy_url
        .map(|url| {
            let url = shell_single_quote(url);
            format!(
                "export HTTPS_PROXY={url}\nexport https_proxy={url}\nexport HTTP_PROXY={url}\nexport http_proxy={url}\nexport ALL_PROXY={url}\nexport all_proxy={url}\n"
            )
        })
        .unwrap_or_default();
    codex_runtime::remote_version_floor_prelude(minimum_version, minimum_current_version)
        .map(|prelude| {
            format!(
                "{prelude}\n{proxy_exports}{}",
                codex_runtime::with_remote_codex_runtime_writer_lock(CODEX_OFFICIAL_INSTALL_SCRIPT)
            )
        })
        .unwrap_or_else(|_| {
            "printf 'Codex update version floor was invalid.\\n' >&2; exit 69".into()
        })
}

pub(crate) fn run_official_codex_installer(
    alias: &str,
    task_id: &str,
    logs: &mut Vec<TaskLog>,
    next_log_index: &mut usize,
    minimum_version: Option<&str>,
    minimum_current_version: Option<&str>,
    proxy_settings: &AppSettings,
    progress: Option<&CodexProgressContext<'_>>,
) -> ssh::SshCommandOutput {
    let direct_script =
        official_codex_installer_script(minimum_version, minimum_current_version, None);
    // 官方包通过本地代理反向隧道下载时可能超过通用 SSH 的 120 秒上限。
    let direct_output = run_codex_step_with_tunnel(
        alias,
        task_id,
        logs,
        next_log_index,
        "official Codex installer (direct)",
        &direct_script,
        OFFICIAL_INSTALLER_SSH_TIMEOUT_MS,
        TaskLogLevel::Error,
        None,
        true,
        progress,
    );
    if direct_output.success() || !official_installer_network_failure(&direct_output) {
        return direct_output;
    }
    if let Some(log) = logs.last_mut() {
        log.level = TaskLogLevel::Warn;
    }

    emit_remote_codex_progress(
        progress,
        "official Codex installer proxy retry",
        "running",
        "Direct network access failed; checking local proxy tunnel routes.".into(),
        None,
        None,
        None,
        None,
        None,
        None,
    );
    let (routes, notes) = remote_codex_proxy_tunnel_candidates(proxy_settings);
    if routes.is_empty() {
        let detail = if notes.is_empty() {
            "No eligible localhost proxy route was detected.".to_string()
        } else {
            format!(
                "No eligible localhost proxy route was detected: {}.",
                notes.join("; ")
            )
        };
        logs.push(basic_log(
            task_id,
            *next_log_index,
            TaskLogLevel::Warn,
            &detail,
        ));
        *next_log_index += 1;
        return direct_output;
    }

    let (selected_route, preflight_errors) =
        select_proxy_route_with_preflight(routes, preflight_remote_codex_proxy_tunnel);
    for error in preflight_errors {
        logs.push(basic_log(
            task_id,
            *next_log_index,
            TaskLogLevel::Warn,
            &redact_error_text(&error),
        ));
        *next_log_index += 1;
    }
    let Some((route, preflight_confirmed)) = selected_route else {
        return direct_output;
    };
    let (level, message) = if preflight_confirmed {
        (
            TaskLogLevel::Info,
            format!(
                "Local proxy route {} reached the official Codex endpoints.",
                route.source
            ),
        )
    } else {
        (
            TaskLogLevel::Warn,
            format!(
                "All local proxy endpoint preflights failed; attempting route {} through a real SSH tunnel.",
                route.source
            ),
        )
    };
    logs.push(basic_log(task_id, *next_log_index, level, &message));
    *next_log_index += 1;

    let mut last_tunnel_output = None;
    for remote_port in remote_proxy_port_candidates(task_id) {
        let tunnel = ssh::ReverseProxyTunnel::new(route.local_port, remote_port)
            .expect("validated proxy tunnel ports");
        let proxy_url = match route.proxy_environment_url(remote_port) {
            Ok(url) => url,
            Err(error) => {
                logs.push(basic_log(
                    task_id,
                    *next_log_index,
                    TaskLogLevel::Error,
                    &redact_error_text(&error),
                ));
                *next_log_index += 1;
                return direct_output;
            }
        };
        let tunnel_script = official_codex_installer_script(
            minimum_version,
            minimum_current_version,
            Some(&proxy_url),
        );
        let output = run_codex_step_with_tunnel(
            alias,
            task_id,
            logs,
            next_log_index,
            "official Codex installer (local proxy tunnel)",
            &tunnel_script,
            OFFICIAL_INSTALLER_SSH_TIMEOUT_MS,
            TaskLogLevel::Error,
            Some(&tunnel),
            true,
            progress,
        );
        if output.success() || !remote_forward_setup_failure(&output) {
            return output;
        }
        if let Some(log) = logs.last_mut() {
            log.level = TaskLogLevel::Warn;
        }
        last_tunnel_output = Some(output);
    }
    last_tunnel_output.unwrap_or(direct_output)
}

/// Prefer a preflight-confirmed route, but keep the first eligible route as an advisory fallback.
fn select_proxy_route_with_preflight<T, F>(
    routes: Vec<T>,
    mut preflight: F,
) -> (Option<(T, bool)>, Vec<String>)
where
    F: FnMut(&T) -> Result<(), String>,
{
    let mut fallback = None;
    let mut errors = Vec::new();
    for route in routes {
        match preflight(&route) {
            Ok(()) => return (Some((route, true)), errors),
            Err(error) => {
                errors.push(error);
                if fallback.is_none() {
                    fallback = Some(route);
                }
            }
        }
    }
    (fallback.map(|route| (route, false)), errors)
}

pub(crate) fn run_remote_native_mirror_install(
    alias: &str,
    task_id: &str,
    logs: &mut Vec<TaskLog>,
    next_log_index: &mut usize,
    timeout: u64,
    minimum_version: Option<&str>,
    minimum_current_version: Option<&str>,
    progress: Option<&CodexProgressContext<'_>>,
) -> ssh::SshCommandOutput {
    let script =
        codex_runtime::remote_version_floor_prelude(minimum_version, minimum_current_version)
            .map(|prelude| {
                format!(
                    "{prelude}\n{}",
                    codex_runtime::with_remote_codex_runtime_writer_lock(
                        CODEX_REMOTE_NATIVE_MIRROR_SCRIPT
                    )
                )
            })
            .unwrap_or_else(|_| {
                "printf 'Codex update version floor was invalid.\\n' >&2; exit 69".into()
            });
    run_codex_step(
        alias,
        task_id,
        logs,
        next_log_index,
        "remote npmmirror native package",
        &script,
        timeout,
        TaskLogLevel::Error,
        progress,
    )
}

pub(crate) fn run_remote_npm_mirror_install(
    alias: &str,
    task_id: &str,
    logs: &mut Vec<TaskLog>,
    next_log_index: &mut usize,
    timeout: u64,
    minimum_version: Option<&str>,
    minimum_current_version: Option<&str>,
    progress: Option<&CodexProgressContext<'_>>,
) -> ssh::SshCommandOutput {
    let script =
        codex_runtime::remote_version_floor_prelude(minimum_version, minimum_current_version)
            .map(|prelude| {
                format!(
                    "{prelude}\n{}",
                    codex_runtime::with_remote_codex_runtime_writer_lock(
                        CODEX_REMOTE_NPM_MIRROR_SCRIPT
                    )
                )
            })
            .unwrap_or_else(|_| {
                "printf 'Codex update version floor was invalid.\\n' >&2; exit 69".into()
            });
    run_codex_step(
        alias,
        task_id,
        logs,
        next_log_index,
        "remote npm mirror installation",
        &script,
        timeout,
        TaskLogLevel::Error,
        progress,
    )
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
    minimum_version: Option<&str>,
    minimum_current_version: Option<&str>,
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

    let (package, download_output, validation_output) =
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
    if let Some(validation_output) = validation_output.as_ref() {
        push_command_step_log(
            task_id,
            logs,
            next_log_index,
            "validate downloaded Codex native package locally",
            validation_output,
            timeout.min(30_000),
            TaskLogLevel::Error,
        );
    }
    let Some(package) = package else {
        return validation_output.unwrap_or(download_output);
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
        codex_runtime::remote_version_floor_prelude(minimum_version, minimum_current_version)
            .map(|prelude| {
                let install = codex_install_uploaded_package_script(
                    &remote_tarball,
                    &package.version,
                    &package.target,
                );
                format!(
                    "{prelude}\n{}",
                    codex_runtime::with_remote_codex_runtime_writer_lock(&install)
                )
            })
            .unwrap_or_else(|_| {
                "printf 'Codex update version floor was invalid.\\n' >&2; exit 69".into()
            });
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
) -> (
    Option<LocalCodexNativePackage>,
    ssh::SshCommandOutput,
    Option<ssh::SshCommandOutput>,
) {
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
            None,
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
        return (None, metadata_output, None);
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
                None,
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
                None,
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
            None,
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
        return (None, tarball_output, None);
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

    let validation_output =
        validate_local_codex_native_package(&tarball_path, target, timeout.min(30_000), progress);
    if !validation_output.success() {
        log_best_effort("clean temporary directory", fs::remove_dir_all(&temp_dir));
        return (None, output, Some(validation_output));
    }

    (
        Some(LocalCodexNativePackage {
            version,
            target: target.to_string(),
            tarball_path,
            temp_dir,
        }),
        output,
        Some(validation_output),
    )
}

fn validate_local_codex_native_package(
    tarball_path: &Path,
    target: &str,
    timeout: u64,
    progress: Option<&CodexProgressContext<'_>>,
) -> ssh::SshCommandOutput {
    let timeout = timeout.clamp(5_000, 30_000);
    let args = vec![
        "-tzf".to_string(),
        tarball_path.to_string_lossy().to_string(),
    ];
    let command = format!("tar -tzf {}", path_string(tarball_path));
    emit_remote_codex_progress(
        progress,
        "validate downloaded Codex native package locally",
        "running",
        "Validating the downloaded archive before SCP upload.".into(),
        None,
        None,
        None,
        None,
        None,
        None,
    );
    let output = ssh::run_local_process("tar", &args, &command, timeout).unwrap_or_else(|error| {
        failed_command_output(command, format!("Could not start local tar: {error}"))
    });
    let output = validate_local_tar_listing_output(output, target);
    emit_remote_codex_progress_for_output(
        progress,
        "validate downloaded Codex native package locally",
        if output.success() {
            "success"
        } else {
            "failed"
        },
        &command_step_message(
            "validate downloaded Codex native package locally",
            &output,
            timeout,
        ),
        &output,
    );
    output
}

fn validate_local_tar_listing_output(
    mut output: ssh::SshCommandOutput,
    target: &str,
) -> ssh::SshCommandOutput {
    if !output.success() {
        return output;
    }
    match validate_codex_native_archive_listing(&output.stdout, target) {
        Ok(entries) => {
            output.stdout = format!(
                "Validated {entries} archive entries; package/vendor/{target}/bin/codex is present.\n"
            );
        }
        Err(error) => {
            output.exit_code = Some(65);
            output.stderr = error;
        }
    }
    output
}

fn validate_codex_native_archive_listing(listing: &str, target: &str) -> Result<usize, String> {
    if target.is_empty()
        || !target
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("The expected Codex native target is unsafe.".into());
    }
    let expected = format!("package/vendor/{target}/bin/codex");
    let mut entry_count = 0usize;
    let mut found_candidate = false;
    for raw_entry in listing.lines() {
        let entry = raw_entry.trim().trim_start_matches("./");
        if entry.is_empty() {
            continue;
        }
        entry_count += 1;
        let bytes = entry.as_bytes();
        let has_drive_prefix = bytes.len() >= 2 && bytes[1] == b':';
        let has_parent = entry.split(['/', '\\']).any(|component| component == "..");
        if entry.starts_with('/') || entry.starts_with('\\') || has_drive_prefix || has_parent {
            return Err(format!(
                "Downloaded Codex archive contains an unsafe path: {entry}"
            ));
        }
        if entry == expected {
            found_candidate = true;
        }
    }
    if entry_count == 0 {
        return Err("Downloaded Codex archive did not contain any entries.".into());
    }
    if !found_candidate {
        return Err(format!(
            "Downloaded Codex archive did not contain {expected}."
        ));
    }
    Ok(entry_count)
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
release_root="$CODEX_HOME/packages/standalone/releases"
marker_name=".codexhub-managed-release"
mkdir -p "$CODEX_INSTALL_DIR" "$release_root"
mkdir "$tmp_dir" || exit 70
trap 'codexhub_exit_status=$?; trap - EXIT; rm -rf "$tmp_dir"; [ -n "${stage_dir:-}" ] && rm -rf "$stage_dir"; rm -f "$remote_tarball"; if [ "$codexhub_exit_status" -ne 0 ]; then codexhub_runtime_restore_locked_current >/dev/null 2>&1 || codexhub_exit_status=76; fi; codexhub_runtime_lock_release >/dev/null 2>&1 || true; exit "$codexhub_exit_status"' EXIT

is_safe_release_name() {
  codexhub_safe_release_name_value=$1
  case "$codexhub_safe_release_name_value" in "" | "." | ".." | */* | *[!A-Za-z0-9.+-]*) return 1 ;; esac
  printf '%s\n' "$codexhub_safe_release_name_value" | awk '
    BEGIN { ok = 0 }
    {
      split($0, build_parts, "+")
      split(build_parts[1], prerelease_parts, "-")
      count = split(prerelease_parts[1], numbers, ".")
      if (count < 2 || count > 4) exit 1
      for (component_index = 1; component_index <= count; component_index += 1) if (numbers[component_index] !~ /^[0-9]+$/) exit 1
      ok = 1
    }
    END { exit(ok ? 0 : 1) }
  '
}

binary_version() {
  codexhub_binary_version_path=$1
  "$codexhub_binary_version_path" --version 2>/dev/null | awk '
    NF { count += 1; value = $NF }
    END {
      sub(/^v/, "", value)
      if (count != 1 || value !~ /^[0-9A-Za-z.+-]+$/) exit 1
      print value
    }
  '
}

# POSIX sh has no portable local variables, so helper scratch names stay prefixed.
marker_valid() {
  codexhub_marker_check_dir=$1
  codexhub_marker_check_version=$2
  codexhub_marker_check_path="$codexhub_marker_check_dir/$marker_name"
  [ -f "$codexhub_marker_check_path" ] && [ ! -L "$codexhub_marker_check_path" ] || return 1
  [ "$(wc -l <"$codexhub_marker_check_path" 2>/dev/null | tr -d '[:space:]')" = 2 ] || return 1
  [ "$(sed -n '1p' "$codexhub_marker_check_path" 2>/dev/null)" = "CodexHub managed standalone release v1" ] || return 1
  [ "$(sed -n '2p' "$codexhub_marker_check_path" 2>/dev/null)" = "version=$codexhub_marker_check_version" ] || return 1
}

write_verified_marker() {
  codexhub_marker_write_dir=$1
  codexhub_marker_write_version=$2
  codexhub_marker_write_path="$codexhub_marker_write_dir/$marker_name"
  if [ -e "$codexhub_marker_write_path" ] || [ -L "$codexhub_marker_write_path" ]; then
    marker_valid "$codexhub_marker_write_dir" "$codexhub_marker_write_version"
    return $?
  fi
  codexhub_marker_write_tmp="$codexhub_marker_write_dir/$marker_name.tmp.$$"
  [ ! -e "$codexhub_marker_write_tmp" ] && [ ! -L "$codexhub_marker_write_tmp" ] || return 1
  {
    printf 'CodexHub managed standalone release v1\n'
    printf 'version=%s\n' "$codexhub_marker_write_version"
  } >"$codexhub_marker_write_tmp" || return 1
  chmod 600 "$codexhub_marker_write_tmp" || { rm -f "$codexhub_marker_write_tmp"; return 1; }
  mv "$codexhub_marker_write_tmp" "$codexhub_marker_write_path"
}

is_safe_existing_release() {
  codexhub_existing_dir=$1
  codexhub_existing_version=$2
  [ -d "$codexhub_existing_dir" ] && [ ! -L "$codexhub_existing_dir" ] || return 1
  codexhub_existing_root_real=$(readlink -f "$release_root" 2>/dev/null) || return 1
  codexhub_existing_real=$(readlink -f "$codexhub_existing_dir" 2>/dev/null) || return 1
  [ "$codexhub_existing_real" = "$codexhub_existing_root_real/$codexhub_existing_version" ] || return 1
  codexhub_existing_selected_binary="$codexhub_existing_dir/bin/codex"
  codexhub_existing_selected_relative=bin/codex
  codexhub_existing_compat="$codexhub_existing_dir/codex"
  [ -e "$codexhub_existing_selected_binary" ] && [ ! -L "$codexhub_existing_selected_binary" ] || return 1
  if [ -e "$codexhub_existing_compat" ] || [ -L "$codexhub_existing_compat" ]; then
    # The official package adds only this exact compatibility link.
    [ -L "$codexhub_existing_compat" ] || return 1
    [ "$(readlink "$codexhub_existing_compat" 2>/dev/null)" = bin/codex ] || return 1
    codexhub_existing_compat_real=$(readlink -f "$codexhub_existing_compat" 2>/dev/null) || return 1
    [ "$codexhub_existing_compat_real" = "$codexhub_existing_selected_binary" ] || return 1
  fi
  [ -f "$codexhub_existing_selected_binary" ] && [ -x "$codexhub_existing_selected_binary" ] && [ ! -L "$codexhub_existing_selected_binary" ] || return 1
  codexhub_existing_binary_real=$(readlink -f "$codexhub_existing_selected_binary" 2>/dev/null) || return 1
  [ "$codexhub_existing_binary_real" = "$codexhub_existing_real/$codexhub_existing_selected_relative" ] || return 1
  codexhub_existing_reported_version=$(binary_version "$codexhub_existing_selected_binary") || return 1
  [ "$codexhub_existing_reported_version" = "$codexhub_existing_version" ] || return 1
  write_verified_marker "$codexhub_existing_dir" "$codexhub_existing_version"
}

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
is_safe_release_name "$version" || { printf 'Uploaded Codex version is unsafe.\n' >&2; exit 2; }
codexhub_version_meets_floors "$version" || { printf 'Uploaded Codex version is below a verified pre-operation runtime floor.\n' >&2; exit 69; }
[ -d "$release_root" ] && [ ! -L "$release_root" ] || { printf 'Standalone release root identity is unsafe.\n' >&2; exit 2; }
release_dir="$release_root/$version"
stage_dir="$release_dir.tmp.$$"
mkdir "$extract_dir" "$stage_dir" || { printf 'Could not create isolated Codex release staging directories.\n' >&2; exit 2; }
tar -xzf "$remote_tarball" -C "$extract_dir" || { printf 'Could not extract the uploaded Codex package.\n' >&2; exit 2; }
vendor_dir="$extract_dir/package/vendor/$target"
if [ ! -x "$vendor_dir/bin/codex" ]; then
  printf 'Uploaded Codex native package did not contain vendor/%s/bin/codex.\n' "$target" >&2
  exit 2
fi

cp -R "$vendor_dir/." "$stage_dir/"
chmod 0755 "$stage_dir/bin/codex"
[ -f "$stage_dir/codex-path/rg" ] && chmod 0755 "$stage_dir/codex-path/rg"
[ -f "$stage_dir/codex-resources/bwrap" ] && chmod 0755 "$stage_dir/codex-resources/bwrap"
staged_version=$(binary_version "$stage_dir/bin/codex") || { printf 'Uploaded Codex binary version could not be verified.\n' >&2; exit 2; }
[ "$staged_version" = "$version" ] || { printf 'Uploaded Codex binary version did not match package metadata.\n' >&2; exit 2; }
write_verified_marker "$stage_dir" "$version" || { printf 'Could not write the managed release marker.\n' >&2; exit 2; }
if [ -e "$release_dir" ] || [ -L "$release_dir" ]; then
  if ! is_safe_existing_release "$release_dir" "$version"; then
    printf 'Existing same-version release directory is not safely adoptable.\n' >&2
    exit 2
  fi
else
  stage_basename=${stage_dir##*/}
  mv "$stage_dir" "$release_dir" || { printf 'Could not commit the staged Codex release.\n' >&2; exit 2; }
  if [ -e "$release_dir/$stage_basename" ] || [ -L "$release_dir/$stage_basename" ]; then
    stage_dir=""
    printf 'The same-version release directory changed during commit; refusing the raced identity.\n' >&2
    exit 2
  fi
  stage_dir=""
  is_safe_existing_release "$release_dir" "$version" || { printf 'Committed Codex release identity could not be re-verified.\n' >&2; exit 2; }
fi
ln -sfn "$release_dir" "$CODEX_HOME/packages/standalone/current"
ln -sfn "$release_dir/bin/codex" "$CODEX_INSTALL_DIR/codex"
codexhub_runtime_verify_post_mutation_floor || { printf 'The uploaded native runtime did not preserve the locked version floor.\n' >&2; exit 69; }
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
        steps: Vec::new(),
        logs,
    }
}

#[cfg(test)]
mod host_operation_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    fn installer_output(
        exit_code: Option<i32>,
        stderr: &str,
        timed_out: bool,
    ) -> ssh::SshCommandOutput {
        ssh::SshCommandOutput {
            command: "official installer fixture".into(),
            stdout: String::new(),
            stderr: stderr.into(),
            exit_code,
            duration_ms: 1,
            timed_out,
        }
    }

    #[test]
    fn official_installer_has_a_dedicated_six_minute_ssh_budget() {
        assert_eq!(OFFICIAL_INSTALLER_SSH_TIMEOUT_MS, 360_000);
    }

    #[test]
    fn official_installer_retries_proxy_only_for_remote_network_failures() {
        assert!(official_installer_network_failure(&installer_output(
            Some(28),
            "curl: (28) Operation timed out",
            false,
        )));
        assert!(official_installer_network_failure(&installer_output(
            None, "", true,
        )));
        assert!(official_installer_network_failure(&installer_output(
            Some(1),
            "wget: unable to resolve host address 'chatgpt.com'",
            false,
        )));

        assert!(!official_installer_network_failure(&installer_output(
            Some(0),
            "",
            false,
        )));
        assert!(!official_installer_network_failure(&installer_output(
            Some(2),
            "installer rejected an invalid version",
            false,
        )));
        assert!(!official_installer_network_failure(&installer_output(
            Some(255),
            "ssh: connect to host timed out",
            false,
        )));
    }

    #[test]
    fn remote_proxy_ports_are_deterministic_high_and_distinct() {
        let first = remote_proxy_port_candidates("task-install-alpha");
        let second = remote_proxy_port_candidates("task-install-alpha");

        assert_eq!(first, second);
        assert!(first.iter().all(|port| (42_000..52_000).contains(port)));
        assert_eq!(
            first
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            3
        );
    }

    #[test]
    fn official_installer_proxy_environment_is_scoped_to_the_remote_script() {
        let proxy_url = "http://127.0.0.1:43210/";
        let tunneled = official_codex_installer_script(None, None, Some(proxy_url));
        for name in [
            "HTTPS_PROXY",
            "https_proxy",
            "HTTP_PROXY",
            "http_proxy",
            "ALL_PROXY",
            "all_proxy",
        ] {
            assert!(tunneled.contains(&format!("export {name}='{proxy_url}'")));
        }
        assert_eq!(tunneled.matches(proxy_url).count(), 6);
        assert!(tunneled.contains("https://chatgpt.com/codex/install.sh"));

        let direct = official_codex_installer_script(None, None, None);
        assert!(!direct.contains("export HTTPS_PROXY="));
        assert!(!direct.contains("export ALL_PROXY="));
    }

    #[test]
    fn only_remote_forward_setup_errors_rotate_tunnel_ports() {
        assert!(remote_forward_setup_failure(&installer_output(
            Some(255),
            "Error: remote port forwarding failed for listen port 43210",
            false,
        )));
        assert!(remote_forward_setup_failure(&installer_output(
            Some(255),
            "administratively prohibited",
            false,
        )));
        assert!(!remote_forward_setup_failure(&installer_output(
            Some(7),
            "curl: (7) Failed to connect through proxy",
            false,
        )));
    }

    #[test]
    fn proxy_route_selection_prefers_a_later_preflight_success() {
        let (selected, errors) =
            select_proxy_route_with_preflight(vec!["first", "second"], |route| match *route {
                "second" => Ok(()),
                _ => Err(format!("{route} connection failed")),
            });

        assert_eq!(selected, Some(("second", true)));
        assert_eq!(errors, vec!["first connection failed"]);
    }

    #[test]
    fn proxy_route_selection_keeps_first_route_when_all_preflights_fail() {
        let (selected, errors) =
            select_proxy_route_with_preflight(vec!["first", "second"], |route| {
                Err(format!("{route} timed out"))
            });

        assert_eq!(selected, Some(("first", false)));
        assert_eq!(errors, vec!["first timed out", "second timed out"]);
    }

    #[test]
    fn bounded_pool_limits_concurrency_refills_and_preserves_input_order() {
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let slow_finished = AtomicBool::new(false);
        let replacement_started_before_slow_finished = AtomicBool::new(false);
        let output = run_bounded_ordered((0usize..9).collect(), |item| {
            let current = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(current, Ordering::SeqCst);
            if item == 6 && !slow_finished.load(Ordering::SeqCst) {
                replacement_started_before_slow_finished.store(true, Ordering::SeqCst);
            }
            if item == 0 {
                std::thread::sleep(Duration::from_millis(60));
                slow_finished.store(true, Ordering::SeqCst);
            } else {
                std::thread::sleep(Duration::from_millis(5));
            }
            active.fetch_sub(1, Ordering::SeqCst);
            item
        });

        assert_eq!(output, (0usize..9).collect::<Vec<_>>());
        assert!(peak.load(Ordering::SeqCst) <= HOST_OPERATION_MAX_CONCURRENCY);
        assert_eq!(peak.load(Ordering::SeqCst), HOST_OPERATION_MAX_CONCURRENCY);
        assert!(replacement_started_before_slow_finished.load(Ordering::SeqCst));
    }

    #[test]
    fn four_probe_groups_run_together_and_complete_fastest_first() {
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let barrier = std::sync::Barrier::new(4);
        let mut completed = Vec::new();
        run_parallel_completions(
            vec![("slow", 50u64), ("medium", 25), ("quick", 12), ("fast", 2)],
            |_, delay| {
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(current, Ordering::SeqCst);
                barrier.wait();
                std::thread::sleep(Duration::from_millis(delay));
            },
            |key, ()| -> Result<(), ()> {
                active.fetch_sub(1, Ordering::SeqCst);
                completed.push(key);
                Ok(())
            },
        )
        .expect("run parallel probes");

        assert_eq!(peak.load(Ordering::SeqCst), 4);
        assert_eq!(completed.first(), Some(&"fast"));
        assert_eq!(completed.last(), Some(&"slow"));
    }

    #[test]
    fn batch_probe_refreshes_latest_version_exactly_once() {
        let calls = AtomicUsize::new(0);
        let (latest, hosts) = run_with_parallel_latest(
            || {
                calls.fetch_add(1, Ordering::SeqCst);
                "1.2.3"
            },
            || vec!["one", "two", "three"],
        );
        assert_eq!(latest.expect("latest worker"), "1.2.3");
        assert_eq!(hosts.len(), 3);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn completed_batch_probe_event_serializes_request_and_item() {
        let event = RemoteProbeBatchItemCompletedEvent {
            request_id: "batch-probe-1".into(),
            item: RemoteProbeBatchItem {
                host_alias: "alpha".into(),
                ok: false,
                result: None,
                error: Some("fixture".into()),
            },
        };
        let json = serde_json::to_value(event).expect("serialize completed batch probe event");

        assert_eq!(json["requestId"], "batch-probe-1");
        assert_eq!(json["item"]["hostAlias"], "alpha");
        assert_eq!(json["item"]["ok"], false);
    }

    #[test]
    fn installation_fallback_stops_only_after_validated_success() {
        assert_eq!(next_installation_method(&[]), Some(0));
        assert_eq!(next_installation_method(&[true]), None);
        assert_eq!(next_installation_method(&[false]), Some(1));
        assert_eq!(next_installation_method(&[false, true]), None);
        assert_eq!(next_installation_method(&[false, false]), Some(2));
        assert_eq!(next_installation_method(&[false, false, true]), None);
        assert_eq!(next_installation_method(&[false, false, false]), Some(3));
        assert_eq!(next_installation_method(&[false, false, false, true]), None);
        assert_eq!(
            next_installation_method(&[false, false, false, false]),
            None
        );

        // An exit-zero command with failed candidate validation records false and advances.
        let command_succeeded_but_candidate_failed = [false];
        assert_eq!(
            next_installation_method(&command_succeeded_but_candidate_failed),
            Some(1)
        );
    }

    #[test]
    fn final_verification_requires_method_path_version_and_login_shell() {
        assert!(final_verification_failures(true, true, true, true, true).is_empty());
        assert!(final_verification_failures(true, true, true, false, true).is_empty());
        let cases = [
            (
                false,
                true,
                true,
                true,
                true,
                "verified installation method",
            ),
            (true, false, true, true, true, "Codex path"),
            (true, true, false, true, true, "Codex version"),
            (true, true, true, true, false, "login-shell command"),
            (true, true, true, false, false, "login-shell command"),
        ];
        for (method, path, version, current, login, expected) in cases {
            let failures = final_verification_failures(method, path, version, current, login);
            assert!(failures.contains(&expected));
        }
    }

    #[test]
    fn codex_command_availability_accepts_either_shell() {
        assert!(codex_command_available_in_any_shell(false, true));
        assert!(codex_command_available_in_any_shell(true, false));
        assert!(codex_command_available_in_any_shell(true, true));
        assert!(!codex_command_available_in_any_shell(false, false));
    }

    #[test]
    fn final_verification_success_keeps_current_shell_path_warning() {
        let warned = final_verification_success_message("Update", "host", "0.144.6", false);
        assert!(warned.contains("current non-login SSH shell"));
        assert!(warned.contains("PATH"));
        assert!(warned.contains("verified login shell is usable"));

        let clean = final_verification_success_message("Update", "host", "0.144.6", true);
        assert_eq!(clean, "Update completed on host: 0.144.6.");
    }

    #[test]
    fn failed_install_operation_preserves_detected_restored_runtime_state() {
        let operation_failures = final_verification_failures(false, true, true, true, true);
        assert_eq!(operation_failures, vec!["verified installation method"]);
        assert!(detected_codex_installed(true, true));
        assert!(!detected_codex_installed(false, true));
        assert!(!detected_codex_installed(true, false));
    }

    #[test]
    fn concurrent_operation_ids_are_unique() {
        let ids = Arc::new(Mutex::new(Vec::new()));
        std::thread::scope(|scope| {
            for _ in 0..32 {
                let ids = Arc::clone(&ids);
                scope.spawn(move || {
                    ids.lock()
                        .expect("ids mutex poisoned")
                        .push(timestamp_millis());
                });
            }
        });
        let ids = ids.lock().expect("ids mutex poisoned");
        assert_eq!(ids.len(), 32);
        assert_eq!(
            ids.iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            32
        );
    }

    #[test]
    fn structured_probe_treats_absent_optional_resources_as_valid() {
        let output = ssh::SshCommandOutput {
            command: "probe".into(),
            stdout: "CODEXHUB_CODEX_INSTALLED=no\nCODEXHUB_CODEX_COMMAND_AVAILABLE=no\nCODEXHUB_CODEX_PATH=\nCODEXHUB_CODEX_VERSION=\n".into(),
            stderr: String::new(),
            exit_code: Some(0),
            duration_ms: 1,
            timed_out: false,
        };
        let codex = parse_codex_probe(&output).expect("parse absent Codex");
        assert!(!codex.installed);
        assert_eq!(codex.version, "not installed");

        let api = ssh::SshCommandOutput {
            stdout: "CODEXHUB_CONFIG_EXISTS=no\nCODEXHUB_API_BASE_URL=\nCODEXHUB_API_ENV_VAR=\nCODEXHUB_API_ENV_PRESENT=unknown\n".into(),
            ..output.clone()
        };
        assert!(
            !parse_api_probe(&api)
                .expect("parse absent API config")
                .config_exists
        );
        let skills = ssh::SshCommandOutput {
            stdout: "CODEXHUB_SKILLS_EXISTS=no\nCODEXHUB_SKILLS_COUNT=0\n".into(),
            ..output
        };
        let skills = parse_skills_probe(&skills).expect("parse absent skills");
        assert!(!skills.exists);
        assert_eq!(skills.count, 0);
    }

    #[test]
    fn candidate_and_next_method_logs_use_distinct_ids() {
        let output = ssh::SshCommandOutput {
            command: "test".into(),
            stdout: String::new(),
            stderr: String::new(),
            exit_code: Some(0),
            duration_ms: 1,
            timed_out: false,
        };
        let mut next = 3usize;
        let candidate = command_log("task", next, TaskLogLevel::Info, "candidate", &output);
        next += 1;
        let method = command_log("task", next, TaskLogLevel::Info, "method", &output);
        assert_ne!(candidate.id, method.id);
    }

    #[test]
    fn task_log_ids_remain_unique_when_a_sequence_hint_is_reused() {
        let output = failed_command_output("test command".into(), "test failure".into());
        let first = command_log("task-log-id", 16, TaskLogLevel::Warn, "first", &output);
        let second = basic_log("task-log-id", 16, TaskLogLevel::Warn, "second");

        assert_ne!(first.id, second.id);
    }

    #[test]
    fn state_check_logs_advance_the_maintenance_sequence() {
        let output = ssh::SshCommandOutput {
            command: "test".into(),
            stdout: "ok".into(),
            stderr: String::new(),
            exit_code: Some(0),
            duration_ms: 1,
            timed_out: false,
        };
        let checks = CodexStateChecks {
            codex_path: output.clone(),
            current_command_available: output.clone(),
            login_command_available: output.clone(),
            version: output.clone(),
            shell_path: output,
        };
        let mut logs = vec![basic_log(
            "task-state-check",
            1,
            TaskLogLevel::Info,
            "started",
        )];
        let mut next_log_index = 2;

        append_state_check_logs(
            "task-state-check",
            &mut logs,
            &mut next_log_index,
            &checks,
            "after maintenance",
        );

        assert_eq!(logs.len(), 6);
        assert_eq!(next_log_index, 7);
    }

    #[test]
    fn local_native_archive_listing_requires_safe_paths_and_target_binary() {
        let target = "x86_64-unknown-linux-musl";
        let valid = format!(
            "package/package.json\npackage/vendor/{target}/bin/codex\npackage/vendor/{target}/codex-path/rg\n"
        );
        assert_eq!(
            validate_codex_native_archive_listing(&valid, target).expect("valid archive listing"),
            3
        );

        for unsafe_listing in [
            format!("/package/vendor/{target}/bin/codex\n"),
            format!("package/vendor/../{target}/bin/codex\n"),
            format!("C:/package/vendor/{target}/bin/codex\n"),
            format!("..\\package\\vendor\\{target}\\bin\\codex\n"),
        ] {
            assert!(validate_codex_native_archive_listing(&unsafe_listing, target).is_err());
        }
        assert!(validate_codex_native_archive_listing("package/package.json\n", target).is_err());
    }

    #[test]
    fn local_tar_command_failure_never_becomes_a_valid_archive() {
        let failed = ssh::SshCommandOutput {
            command: "tar -tzf broken.tgz".into(),
            stdout: "package/vendor/x86_64-unknown-linux-musl/bin/codex\n".into(),
            stderr: "gzip: invalid header".into(),
            exit_code: Some(2),
            duration_ms: 3,
            timed_out: false,
        };
        let checked =
            validate_local_tar_listing_output(failed.clone(), "x86_64-unknown-linux-musl");
        assert_eq!(checked, failed);
        assert!(!checked.success());
    }

    #[test]
    fn local_tar_listing_validation_returns_a_compact_success_log() {
        let output = ssh::SshCommandOutput {
            command: "tar -tzf codex.tgz".into(),
            stdout: "package/package.json\npackage/vendor/aarch64-unknown-linux-musl/bin/codex\n"
                .into(),
            stderr: String::new(),
            exit_code: Some(0),
            duration_ms: 3,
            timed_out: false,
        };
        let checked = validate_local_tar_listing_output(output, "aarch64-unknown-linux-musl");
        assert!(checked.success());
        assert!(checked.stdout.contains("Validated 2 archive entries"));
        assert!(checked
            .stdout
            .contains("package/vendor/aarch64-unknown-linux-musl/bin/codex is present"));
    }

    #[test]
    fn install_and_uninstall_prerequisites_use_operation_specific_gates() {
        let linux_without_method_specific_tools = Ok(PreparationProbeData {
            install_platform_ok: true,
            uninstall_platform_ok: true,
            sh_ok: true,
            install_write_ok: true,
            uninstall_write_ok: true,
            detail: "Linux/x86_64; sh=yes; tar=no; npm=yes".into(),
        });
        assert!(install_prerequisites_ready(
            &linux_without_method_specific_tools
        ));
        assert!(uninstall_prerequisites_ready(
            &linux_without_method_specific_tools
        ));

        let darwin = Ok(PreparationProbeData {
            install_platform_ok: false,
            uninstall_platform_ok: true,
            sh_ok: true,
            install_write_ok: true,
            uninstall_write_ok: true,
            detail: "Darwin/arm64".into(),
        });
        assert!(!install_prerequisites_ready(&darwin));
        assert!(uninstall_prerequisites_ready(&darwin));

        let no_uninstall_write = Ok(PreparationProbeData {
            uninstall_write_ok: false,
            ..darwin.expect("Darwin probe")
        });
        assert!(!uninstall_prerequisites_ready(&no_uninstall_write));
        assert!(!install_prerequisites_ready(&Err("probe failed".into())));
        assert!(!uninstall_prerequisites_ready(&Err("probe failed".into())));
    }

    #[test]
    fn uninstall_success_is_decided_by_final_absence_only() {
        assert!(uninstall_target_reached(false, false, false, false));
        assert!(!uninstall_target_reached(true, false, false, false));
        assert!(!uninstall_target_reached(false, true, false, false));
        assert!(!uninstall_target_reached(false, false, true, false));
        assert!(!uninstall_target_reached(false, false, false, true));
        let (before, after) = retained_codex_versions(Some("codex-cli 1.2.3".into()));
        assert_eq!(before.as_deref(), Some("codex-cli 1.2.3"));
        assert_eq!(after, before);
    }

    #[test]
    fn lifecycle_logs_are_assigned_to_the_first_structured_step() {
        let mut task = TaskRun {
            id: "task".into(),
            host_id: "host".into(),
            host_name: "Host".into(),
            action: "Install Codex".into(),
            status: TaskStatus::Running,
            started_at: "now".into(),
            ended_at: None,
            summary: "running".into(),
            steps: Vec::new(),
            logs: vec![
                basic_log("task", 1, TaskLogLevel::Info, "queued"),
                basic_log("task", 2, TaskLogLevel::Info, "started"),
            ],
        };
        assign_existing_logs_to_step(&mut task, "preparation");
        assert!(task
            .logs
            .iter()
            .all(|log| log.step_id.as_deref() == Some("preparation")));
    }

    #[test]
    fn stable_step_ids_and_preparation_details_do_not_drift() {
        assert_eq!(
            PROBE_STEP_IDS.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            ["ssh-check", "system", "codex", "api", "skills"]
        );
        assert_eq!(
            INSTALL_STEP_IDS
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>(),
            [
                "preparation",
                "official-installer",
                "remote-native-mirror",
                "remote-npm-mirror",
                "local-upload",
                "runtime-reconcile",
                "final-verification",
                "release-cleanup"
            ]
        );
        let script = preparation_probe_script();
        for token in [
            "sh=%s",
            "tar=%s",
            "curl=%s",
            "wget=%s",
            "python3=%s",
            "npm=%s",
            "CODEXHUB_INSTALL_PLATFORM_OK",
            "CODEXHUB_UNINSTALL_PLATFORM_OK",
        ] {
            assert!(script.contains(token));
        }
    }

    fn trusted_test_host() -> Host {
        Host {
            id: "host-1".into(),
            name: "Host".into(),
            host_alias: "lab".into(),
            source: "manual".into(),
            address: "lab".into(),
            port: 22,
            username: "user".into(),
            auth_method: AuthMethod::SshKey,
            status: HostStatus::Unknown,
            os: "OldOS".into(),
            arch: "old-arch".into(),
            shell: "/bin/old".into(),
            path: Some("/old/bin".into()),
            path_has_local_bin: Some(false),
            codex_command_available: Some(false),
            codex_installed: false,
            codex_version: "old-version".into(),
            config_exists: Some(true),
            api_config_name: Some("Old config".into()),
            api_config_source: Some("profile".into()),
            api_key_env_var: Some("OLD_KEY".into()),
            api_key_env_present: Some(true),
            skills_exists: Some(true),
            skills_count: Some(1),
            profile_id: Some("old-profile".into()),
            skill_pack_ids: Vec::new(),
            tags: Vec::new(),
            last_seen: "old".into(),
            latency_ms: Some(42),
        }
    }

    #[test]
    fn failed_probe_groups_preserve_old_fields_while_successful_groups_update() {
        let mut host = trusted_test_host();
        let api_match = RemoteApiConfigMatch {
            name: "New config".into(),
            source: "new".into(),
            profile_id: Some("new-profile".into()),
        };
        apply_host_probe_group_updates(
            &mut host,
            "NewOS",
            "new-arch",
            "/bin/new",
            Some("/new/bin".into()),
            true,
            true,
            true,
            "2.0.0",
            false,
            &api_match,
            Some("NEW_KEY".into()),
            Some(false),
            false,
            7,
            false,
            true,
            false,
            true,
        );

        assert_eq!(host.os, "OldOS");
        assert_eq!(host.path.as_deref(), Some("/old/bin"));
        assert_eq!(host.codex_version, "2.0.0");
        assert!(host.codex_installed);
        assert_eq!(host.api_config_name.as_deref(), Some("Old config"));
        assert_eq!(host.profile_id.as_deref(), Some("old-profile"));
        assert_eq!(host.skills_count, Some(7));
        assert_eq!(host.skills_exists, Some(false));
        assert!(matches!(host.status, HostStatus::Online));
    }

    #[test]
    fn failed_ssh_check_persists_offline_without_erasing_trusted_details() {
        let state = AppState::new(storage::TaskStore::in_memory());
        let original = trusted_test_host();
        save_hosts_state(&state, std::slice::from_ref(&original)).expect("persist initial host");

        persist_host_check(&state, "LAB", false, 10_000).expect("persist failed SSH status");
        let stored =
            storage::load_document(&state.paths, "hosts", "hosts.json", Vec::<Host>::new())
                .expect("reload persisted hosts")
                .data;
        let host = stored.first().expect("stored host");

        assert!(matches!(host.status, HostStatus::Offline));
        assert_eq!(host.latency_ms, None);
        assert_eq!(host.os, original.os);
        assert_eq!(host.codex_version, original.codex_version);
        assert_eq!(host.api_config_name, original.api_config_name);
        assert_eq!(host.skills_count, original.skills_count);
        assert_eq!(host.shell, original.shell);
        assert_eq!(host.last_seen, original.last_seen);
    }
}
