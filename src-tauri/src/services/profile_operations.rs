use crate::tasks::{TaskStep, TaskStepStatus};
use crate::*;

use super::codex_runtime::{self, CodexRuntimeReconcileResult};

const PROFILE_APPLY_STEP_ID: &str = "profile-apply";
const REMOTE_CODEX_RELOAD_STEP_ID: &str = "remote-codex-reload";

struct ProfileApplyHostExecution {
    config_status: String,
    reload: RemoteCodexReloadResult,
    task: TaskRun,
}

pub(crate) fn apply_profile_to_hosts(
    app: &AppHandle,
    state: &AppState,
    profile: &Profile,
    rendered_toml: &str,
    host_ids: Vec<String>,
    options: ProfileApplyOptions,
    timeout: u64,
) -> Result<ProfileApplyBatchResult, String> {
    let hosts = resolve_apply_hosts(state, &host_ids);
    if hosts.is_empty() {
        let task_id = format!("task-profile-{}", timestamp_millis());
        let running = jobs::begin_task(
            &state.task_store,
            state.task_event_sink.as_ref(),
            &task_id,
            "no-host",
            "No host selected",
            "Apply profile",
        )?;
        let mut logs = running.logs;
        logs.push(basic_log(
            &task_id,
            logs.len() + 1,
            TaskLogLevel::Error,
            "No matching hosts were selected for profile apply.",
        ));
        let task = TaskRun {
            id: task_id.clone(),
            host_id: "no-host".into(),
            host_name: "No host selected".into(),
            action: "Apply profile".into(),
            status: TaskStatus::Failed,
            started_at: running.started_at,
            ended_at: Some(timestamp_label()),
            summary: "No matching hosts were selected for profile apply.".into(),
            steps: Vec::new(),
            logs,
        };
        record_task(state, task.clone())?;
        return Ok(ProfileApplyBatchResult {
            profile_id: profile.id.clone(),
            ok: false,
            outcome: ProfileApplyOutcome::Failed,
            results: vec![ProfileApplyHostResult {
                host_id: "no-host".into(),
                host_name: "No host selected".into(),
                host_alias: String::new(),
                status: "failed".into(),
                target_path: "~/.codex/config.toml".into(),
                backup_path: None,
                message: task.summary.clone(),
                reload: skipped_reload_result(
                    options.remote_codex_reload_mode,
                    "Remote Codex reload was skipped because no matching host was selected.",
                ),
                task: Some(task.clone()),
            }],
            tasks: vec![task],
            profiles: profile_apply_profiles_snapshot(app, state)?,
            hosts: profile_apply_hosts_snapshot(state)?,
        });
    }

    let results: Vec<ProfileApplyHostResult> = hosts
        .into_iter()
        .map(|host| {
            apply_profile_to_host(
                app,
                state,
                profile,
                rendered_toml,
                host.clone(),
                options.clone(),
                timeout,
            )
            .map(|execution| profile_apply_result_from_execution(&host, execution))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let tasks = results
        .iter()
        .filter_map(|result| result.task.clone())
        .collect::<Vec<_>>();
    let outcome = profile_apply_batch_outcome(&results);
    Ok(ProfileApplyBatchResult {
        profile_id: profile.id.clone(),
        ok: outcome == ProfileApplyOutcome::Success,
        outcome,
        results,
        tasks,
        profiles: profile_apply_profiles_snapshot(app, state)?,
        hosts: profile_apply_hosts_snapshot(state)?,
    })
}

pub(crate) fn resolve_apply_hosts(state: &AppState, host_ids: &[String]) -> Vec<Host> {
    let hosts = state.hosts.lock().expect("hosts mutex poisoned").clone();
    let requested: BTreeSet<String> = host_ids.iter().cloned().collect();
    hosts
        .into_iter()
        .filter(|host| requested.contains(&host.id) || requested.contains(&host.host_alias))
        .collect()
}

pub(crate) fn profile_import_export(profiles: Vec<Profile>) -> ProfileImportExport {
    ProfileImportExport {
        schema_version: 1,
        exported_at: timestamp_label(),
        profiles,
    }
}

pub(crate) fn profile_apply_targets(
    hosts: &[Host],
    profile_id: &str,
) -> Vec<ProfileApplyTargetFile> {
    hosts
        .iter()
        .map(|host| {
            let no_change_expected = host.profile_id.as_deref() == Some(profile_id);
            ProfileApplyTargetFile {
                host_id: host.id.clone(),
                host_name: host.name.clone(),
                host_alias: host.host_alias.clone(),
                path: "~/.codex/config.toml".into(),
                backup_expected: host.config_exists.unwrap_or(true) && !no_change_expected,
                no_change_expected,
            }
        })
        .collect()
}

pub(crate) fn profile_apply_preview_result(
    host: &Host,
    profile_id: &str,
) -> ProfileApplyHostResult {
    let no_change_expected = host.profile_id.as_deref() == Some(profile_id);
    ProfileApplyHostResult {
        host_id: host.id.clone(),
        host_name: host.name.clone(),
        host_alias: host.host_alias.clone(),
        status: "pending".into(),
        target_path: "~/.codex/config.toml".into(),
        backup_path: None,
        message: if no_change_expected {
            "Preview expects no remote config changes.".into()
        } else {
            "Preview expects remote config compare, backup when needed, then atomic replace.".into()
        },
        reload: RemoteCodexReloadResult {
            mode: RemoteCodexReloadMode::AppServices,
            status: RemoteCodexReloadStatus::NotRequested,
            targeted_count: 0,
            stopped_count: 0,
            preserved_cli_count: 0,
            replacement_observed: false,
            message: "Remote Codex reload will be decided when the apply is confirmed.".into(),
        },
        task: None,
    }
}

fn profile_apply_result_from_execution(
    host: &Host,
    execution: ProfileApplyHostExecution,
) -> ProfileApplyHostResult {
    ProfileApplyHostResult {
        host_id: host.id.clone(),
        host_name: host.name.clone(),
        host_alias: host.host_alias.clone(),
        status: execution.config_status,
        target_path: "~/.codex/config.toml".into(),
        backup_path: profile_apply_backup_path_from_task(&execution.task),
        message: execution.task.summary.clone(),
        reload: execution.reload,
        task: Some(execution.task),
    }
}

pub(crate) fn profile_apply_batch_outcome(
    results: &[ProfileApplyHostResult],
) -> ProfileApplyOutcome {
    let failed_configs = results
        .iter()
        .filter(|result| result.status == "failed")
        .count();
    if failed_configs == results.len() {
        return ProfileApplyOutcome::Failed;
    }
    if failed_configs > 0 {
        return ProfileApplyOutcome::Partial;
    }
    if results.iter().any(|result| {
        matches!(
            result.reload.status,
            RemoteCodexReloadStatus::Skipped
                | RemoteCodexReloadStatus::ManualRequired
                | RemoteCodexReloadStatus::Failed
        )
    }) {
        return ProfileApplyOutcome::ManualReconnect;
    }
    ProfileApplyOutcome::Success
}

pub(crate) fn profile_apply_backup_path_from_task(task: &TaskRun) -> Option<String> {
    task.logs.iter().find_map(|log| {
        log.stdout
            .as_deref()
            .and_then(|stdout| marker_value(stdout, "CODEXHUB_PROFILE_BACKUP"))
            .filter(|value| !value.is_empty())
    })
}

pub(crate) fn configure_profile_remote_api_key(
    state: &AppState,
    alias: &str,
    profile: &Profile,
    task_id: &str,
    logs: &mut Vec<TaskLog>,
    next_log: &mut usize,
    timeout: u64,
) -> Option<bool> {
    let Some(env_var) = profile
        .api_key_env_var
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return None;
    };
    if !is_valid_env_var_name(env_var) {
        logs.push(basic_log(
            task_id,
            *next_log,
            TaskLogLevel::Error,
            &format!("Remote API env var name `{env_var}` is not valid."),
        ));
        *next_log += 1;
        return Some(false);
    }

    let mut api_key = match load_profile_api_key_local(&profile.id) {
        Ok(value) => value,
        Err(error) => {
            logs.push(basic_log(
                task_id,
                *next_log,
                TaskLogLevel::Error,
                &format!("Could not read local stored API key: {error}"),
            ));
            *next_log += 1;
            return Some(false);
        }
    };
    if api_key.is_none() && profile.source == "cc-switch" {
        api_key = match find_cc_switch_api_key_for_profile(state, profile) {
            Ok(value) => value,
            Err(error) => {
                logs.push(basic_log(
                    task_id,
                    *next_log,
                    TaskLogLevel::Error,
                    &format!("Could not inspect the legacy profile credential source: {error}"),
                ));
                *next_log += 1;
                return Some(false);
            }
        };
    }
    let Some(api_key) = api_key else {
        logs.push(basic_log(
            task_id,
            *next_log,
            TaskLogLevel::Warn,
            &format!(
                "No local stored API key is available for {}; remote env was not updated.",
                profile.name
            ),
        ));
        *next_log += 1;
        return None;
    };
    if api_key.contains('\n') || api_key.contains('\r') {
        logs.push(basic_log(
            task_id,
            *next_log,
            TaskLogLevel::Error,
            "Stored API key contains unsupported line breaks; remote env was not updated.",
        ));
        *next_log += 1;
        return Some(false);
    }

    let script = remote_profile_api_key_script(env_var, &api_key);
    let output = ssh::run_ssh_script(alias, &script, timeout).unwrap_or_else(|error| {
        failed_command_output(
            format!("ssh {alias} configure remote API env"),
            format!("Could not configure remote API environment: {error}"),
        )
    });
    logs.push(command_log(
        task_id,
        *next_log,
        if output.success() {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        if output.success() {
            "Wrote the remote CodexHub-managed API environment without exposing key material."
        } else {
            "Failed to write the remote CodexHub-managed API environment."
        },
        &output,
    ));
    *next_log += 1;
    let runtime_result = reconcile_after_successful_remote_env_write(&output, || {
        codex_runtime::reconcile_remote_codex_runtime(alias, timeout, None, None)
    });
    let Some(runtime_result) = runtime_result else {
        return Some(false);
    };
    match runtime_result {
        Ok(result) => {
            let completed = result.completed();
            logs.push(basic_log(
                task_id,
                *next_log,
                if completed {
                    TaskLogLevel::Info
                } else {
                    TaskLogLevel::Error
                },
                &result.safe_summary(),
            ));
            *next_log += 1;
            Some(completed)
        }
        Err(_) => {
            logs.push(basic_log(
                task_id,
                *next_log,
                TaskLogLevel::Error,
                "Remote Codex runtime reconciliation could not be verified.",
            ));
            *next_log += 1;
            Some(false)
        }
    }
}

pub(crate) fn reconcile_after_successful_remote_env_write<F>(
    env_output: &ssh::SshCommandOutput,
    reconcile: F,
) -> Option<Result<CodexRuntimeReconcileResult, String>>
where
    F: FnOnce() -> Result<CodexRuntimeReconcileResult, String>,
{
    env_output.success().then(reconcile)
}

pub(crate) fn is_valid_env_var_name(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

// Writes the selected profile key only to CodexHub-managed remote env files;
// command logs expose paths and change markers, never the key value.
pub(crate) fn remote_profile_api_key_script(env_var: &str, api_key: &str) -> String {
    format!(
        r##"set -u
env_dir="$HOME/.codex-hub"
env_file="$env_dir/env"
env_name={env_var}
env_value={api_key}
mkdir -p "$env_dir" "$HOME/.local/bin"
chmod 700 "$env_dir"

tmp_file="$env_file.codexhub.tmp.$$"
backup_path=""
if [ -f "$env_file" ]; then
  awk -v key="$env_name" 'index($0, "export " key "=") != 1 {{ print }}' "$env_file" >"$tmp_file"
else
  : >"$tmp_file"
fi
printf 'export %s=%s\n' "$env_name" "$env_value" >>"$tmp_file"
if [ -f "$env_file" ] && cmp -s "$tmp_file" "$env_file"; then
  rm -f "$tmp_file"
  env_changed=no
else
  if [ -f "$env_file" ]; then
    backup_path="$env_file.codexhub.bak.$(date +%Y%m%d%H%M%S)"
    cp -p "$env_file" "$backup_path"
  fi
  mv "$tmp_file" "$env_file"
  chmod 600 "$env_file"
  env_changed=yes
fi

begin_marker="# >>> CodexHub managed env"
end_marker="# <<< CodexHub managed env"
source_line='[ -f "$HOME/.codex-hub/env" ] && . "$HOME/.codex-hub/env"'
source_paths=""
source_backups=""
source_changed=no
repair_source_file() {{
  target=$1
  [ -n "$target" ] || return
  case ";$source_paths;" in
    *";$target;"*) return ;;
  esac
  source_paths="${{source_paths}}${{source_paths:+;}}$target"
  if [ -f "$target" ] &&
    grep -F "$begin_marker" "$target" >/dev/null 2>&1 &&
    grep -F "$end_marker" "$target" >/dev/null 2>&1 &&
    grep -F "$source_line" "$target" >/dev/null 2>&1; then
    return
  fi
  target_backup=""
  if [ -f "$target" ]; then
    target_backup="$target.codexhub.bak.$(date +%Y%m%d%H%M%S)"
    cp -p "$target" "$target_backup"
  else
    : >"$target"
  fi
  tmp_source="$target.codexhub.tmp.$$"
  if grep -F "$begin_marker" "$target" >/dev/null 2>&1 &&
    grep -F "$end_marker" "$target" >/dev/null 2>&1; then
    awk -v begin="$begin_marker" -v end="$end_marker" -v line="$source_line" '
      $0 == begin {{
        print begin
        print line
        print end
        in_block = 1
        next
      }}
      $0 == end && in_block {{
        in_block = 0
        next
      }}
      !in_block {{ print }}
    ' "$target" >"$tmp_source"
    mv "$tmp_source" "$target"
  else
    rm -f "$tmp_source"
    {{
      printf '\n%s\n' "$begin_marker"
      printf '%s\n' "$source_line"
      printf '%s\n' "$end_marker"
    }} >>"$target"
  fi
  source_changed=yes
  if [ -n "$target_backup" ]; then
    source_backups="${{source_backups}}${{source_backups:+;}}$target_backup"
  fi
}}

shell_value=${{SHELL:-}}
shell_name=${{shell_value##*/}}
if [ "$shell_name" = "zsh" ]; then
  shell_config="$HOME/.zshrc"
else
  shell_config="$HOME/.bashrc"
fi
repair_source_file "$shell_config"
repair_source_file "$HOME/.profile"
if [ -f "$HOME/.bash_profile" ]; then
  repair_source_file "$HOME/.bash_profile"
fi
if [ -f "$HOME/.zprofile" ]; then
  repair_source_file "$HOME/.zprofile"
fi

printf 'CODEXHUB_REMOTE_ENV_CHANGED=%s\n' "$env_changed"
printf 'CODEXHUB_REMOTE_ENV_FILE=%s\n' "$env_file"
printf 'CODEXHUB_REMOTE_ENV_BACKUP=%s\n' "$backup_path"
printf 'CODEXHUB_REMOTE_ENV_SOURCE_CHANGED=%s\n' "$source_changed"
printf 'CODEXHUB_REMOTE_ENV_SOURCE_PATHS=%s\n' "$source_paths"
printf 'CODEXHUB_REMOTE_ENV_SOURCE_BACKUPS=%s\n' "$source_backups"
"##,
        env_var = shell_single_quote(env_var),
        api_key = shell_single_quote(&shell_single_quote(api_key))
    )
}

pub(crate) fn check_profile_api_env(
    alias: &str,
    profile: &Profile,
    task_id: &str,
    logs: &mut Vec<TaskLog>,
    next_log: &mut usize,
    timeout: u64,
) -> Option<bool> {
    let Some(env_var) = profile
        .api_key_env_var
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return None;
    };
    let script = format!(
        r#"api_env={env_var}
if printenv "$api_env" >/dev/null 2>&1; then
  printf 'yes\n'
  exit 0
fi
if [ -f "$HOME/.codex-hub/env" ]; then
  set -a
  . "$HOME/.codex-hub/env" >/dev/null 2>&1 || true
  set +a
  if printenv "$api_env" >/dev/null 2>&1; then
    printf 'yes\n'
    exit 0
  fi
fi
printf 'no\n'
exit 1
"#,
        env_var = shell_single_quote(env_var)
    );
    let output = ssh::run_ssh_script(alias, &script, timeout).unwrap_or_else(|error| {
        failed_command_output(
            format!("ssh {alias} check remote API env"),
            format!("Could not check remote API environment variable: {error}"),
        )
    });
    let present = stdout_optional_yes(Some(&output));
    let readiness = if present.is_none() && !output.success() {
        Some(false)
    } else {
        present
    };
    let message = match present {
        Some(true) => format!("Remote {env_var} is present."),
        Some(false) => format!("Remote {env_var} is missing."),
        None => format!("Could not determine remote {env_var} presence."),
    };
    logs.push(command_log(
        task_id,
        *next_log,
        if readiness == Some(false) || !output.success() {
            TaskLogLevel::Warn
        } else {
            TaskLogLevel::Info
        },
        &message,
        &output,
    ));
    *next_log += 1;
    readiness
}

pub(crate) fn profile_api_env_label(profile: &Profile) -> String {
    profile
        .api_key_env_var
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("API environment variable")
        .to_string()
}

fn apply_profile_to_host(
    app: &AppHandle,
    state: &AppState,
    profile: &Profile,
    rendered_toml: &str,
    host: Host,
    options: ProfileApplyOptions,
    timeout: u64,
) -> Result<ProfileApplyHostExecution, String> {
    let task_id = format!("task-profile-{}", timestamp_millis());
    let running = jobs::begin_task(
        &state.task_store,
        state.task_event_sink.as_ref(),
        &task_id,
        &host.id,
        &host.name,
        "Apply profile",
    )?;
    let mut logs = running.logs;
    let mut next_log = logs.len() + 1;
    let alias_result = ssh::validate_ssh_alias(&host.host_alias);
    let alias = alias_result
        .clone()
        .unwrap_or_else(|_| host.host_alias.trim().to_string());

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
    logs.push(command_log(
        &task_id,
        next_log,
        if check_ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        &ssh_check_message(&alias, &check_output, check_ok, timeout),
        &check_output,
    ));
    next_log += 1;

    if !check_ok {
        update_host_check(state, &alias, false, check_output.duration_ms);
        return finish_failed_profile_apply(
            state,
            &task_id,
            &host,
            options.remote_codex_reload_mode,
            "Profile apply skipped because SSH check failed.",
            logs,
        );
    }
    update_host_check(state, &alias, true, check_output.duration_ms);

    let read_output = ssh::run_ssh_script(
        &alias,
        "if [ -f \"$HOME/.codex/config.toml\" ]; then cat \"$HOME/.codex/config.toml\"; fi",
        timeout,
    )
    .unwrap_or_else(|error| {
        failed_command_output(
            format!("ssh {alias} read ~/.codex/config.toml"),
            format!("Could not read remote config: {error}"),
        )
    });
    let read_ok = read_output.success();
    let mut read_log_output = read_output.clone();
    if read_ok {
        read_log_output.stdout = "[redacted remote config]".into();
    }
    logs.push(command_log(
        &task_id,
        next_log,
        if read_ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        if read_ok {
            "Read remote ~/.codex/config.toml state."
        } else {
            "Failed to read remote ~/.codex/config.toml."
        },
        &read_log_output,
    ));
    next_log += 1;

    if !read_ok {
        return finish_failed_profile_apply(
            state,
            &task_id,
            &host,
            options.remote_codex_reload_mode,
            "Profile apply failed before mutation because remote config could not be read.",
            logs,
        );
    }

    let metadata = AppliedProfileMetadata {
        profile_id: profile.id.clone(),
        profile_name: profile.name.clone(),
        applied_at: timestamp_label(),
        codexhub_version: env!("CARGO_PKG_VERSION").into(),
    };
    let metadata_json =
        serde_json::to_string_pretty(&metadata).unwrap_or_else(|_| "{}".to_string());

    let config_unchanged = read_output.stdout == rendered_toml;
    let (commit_ok, backup_path) = if config_unchanged {
        let output = ssh::run_ssh_script(
            &alias,
            &profile_apply_metadata_script(&metadata_json),
            timeout,
        )
        .unwrap_or_else(|error| {
            failed_command_output(
                format!("ssh {alias} write applied-profile metadata"),
                format!("Could not write remote metadata: {error}"),
            )
        });
        let ok = output.success();
        logs.push(command_log(
            &task_id,
            next_log,
            if ok {
                TaskLogLevel::Info
            } else {
                TaskLogLevel::Error
            },
            if ok {
                "Remote config was unchanged; metadata updated without creating a config backup."
            } else {
                "Remote config was unchanged, but metadata update failed."
            },
            &output,
        ));
        next_log += 1;
        (ok, None)
    } else {
        let local_path = match write_profile_temp_file(state, &task_id, rendered_toml) {
            Ok(path) => path,
            Err(error) => {
                let output = failed_command_output("write local profile temp file".into(), error);
                logs.push(command_log(
                    &task_id,
                    next_log,
                    TaskLogLevel::Error,
                    "Could not create local profile temp file.",
                    &output,
                ));
                return finish_failed_profile_apply(
                    state,
                    &task_id,
                    &host,
                    options.remote_codex_reload_mode,
                    "Profile apply failed before upload.",
                    logs,
                );
            }
        };
        let remote_tmp = format!("/tmp/codexhub-profile-{task_id}.toml");
        let upload_output = ssh::upload_file(&alias, &local_path, &remote_tmp, timeout)
            .unwrap_or_else(|error| failed_command_output(format!("scp {remote_tmp}"), error));
        log_best_effort("clean temporary upload", fs::remove_file(&local_path));
        let upload_ok = upload_output.success();
        logs.push(command_log(
            &task_id,
            next_log,
            if upload_ok {
                TaskLogLevel::Info
            } else {
                TaskLogLevel::Error
            },
            if upload_ok {
                "Uploaded rendered profile TOML to remote staging path."
            } else {
                "Failed to upload rendered profile TOML."
            },
            &upload_output,
        ));
        next_log += 1;

        if !upload_ok {
            return finish_failed_profile_apply(
                state,
                &task_id,
                &host,
                options.remote_codex_reload_mode,
                "Profile apply failed during upload; remote config was not changed.",
                logs,
            );
        }

        let sha256 = sha256_hex(rendered_toml.as_bytes());
        let commit_script = profile_apply_commit_script(
            &remote_tmp,
            &sha256,
            rendered_toml.as_bytes().len(),
            &metadata_json,
            &timestamp_label(),
        );
        let commit_output =
            ssh::run_ssh_script(&alias, &commit_script, timeout).unwrap_or_else(|error| {
                failed_command_output(
                    format!("ssh {alias} commit profile config"),
                    format!("Could not commit profile config: {error}"),
                )
            });
        let ok = commit_output.success();
        let backup_path = marker_value(&commit_output.stdout, "CODEXHUB_PROFILE_BACKUP");
        logs.push(command_log(
            &task_id,
            next_log,
            if ok {
                TaskLogLevel::Info
            } else {
                TaskLogLevel::Error
            },
            if ok {
                "Validated staged TOML and committed remote profile config atomically."
            } else {
                "Failed to validate or commit remote profile config."
            },
            &commit_output,
        ));
        next_log += 1;
        (ok, backup_path)
    };

    let remote_env_configured = if commit_ok {
        configure_profile_remote_api_key(
            state,
            &alias,
            profile,
            &task_id,
            &mut logs,
            &mut next_log,
            timeout,
        )
    } else {
        None
    };

    let api_key_env_present = if commit_ok && remote_env_configured != Some(false) {
        check_profile_api_env(&alias, profile, &task_id, &mut logs, &mut next_log, timeout)
    } else {
        None
    };

    let remote_apply_ready =
        commit_ok && remote_env_configured != Some(false) && api_key_env_present != Some(false);
    // Remote files remain the source of truth even when process activation needs manual recovery.
    let local_persist_error = if remote_apply_ready {
        update_host_profile_apply(app, state, &host.id, &alias, profile, api_key_env_present)
            .err()
            .map(|error| redact_error_text(&error))
    } else {
        None
    };
    if let Some(error) = local_persist_error.as_deref() {
        logs.push(basic_log(
            &task_id,
            next_log,
            TaskLogLevel::Error,
            &format!(
                "Remote apply succeeded, but local Host/Profile state failed to persist: {error}"
            ),
        ));
        next_log += 1;
    }
    // A local persistence error must not leave a verified remote process on stale credentials.
    let reload = if remote_apply_ready {
        reload_remote_codex_processes(
            &alias,
            options.remote_codex_reload_mode,
            &task_id,
            &mut logs,
            &mut next_log,
            timeout,
        )
    } else {
        skipped_reload_result(
            options.remote_codex_reload_mode,
            "Remote Codex reload was skipped because profile apply prerequisites failed.",
        )
    };
    // A post-reload retry can reclaim old managed releases without changing the applied profile.
    let cleanup_hard_failed = if remote_apply_ready {
        match codex_runtime::cleanup_remote_codex_releases(
            &alias,
            timeout,
            codex_runtime::CodexReleaseCleanupPolicy::ManagedOnly,
        ) {
            Ok(result) => {
                let hard_failed = result.hard_failed();
                let level = match result.status {
                    codex_runtime::CodexReleaseCleanupStatus::Deferred => TaskLogLevel::Warn,
                    codex_runtime::CodexReleaseCleanupStatus::Failed => TaskLogLevel::Error,
                    _ => TaskLogLevel::Info,
                };
                logs.push(basic_log(&task_id, next_log, level, &result.safe_summary()));
                hard_failed
            }
            Err(error) => {
                logs.push(basic_log(
                    &task_id,
                    next_log,
                    TaskLogLevel::Error,
                    &format!(
                        "Managed runtime cleanup after remote reload could not be verified: {}",
                        redact_error_text(&error)
                    ),
                ));
                true
            }
        }
    } else {
        false
    };
    let base_summary = if config_unchanged {
        format!(
            "{} already matched {} on {}; no config backup was created.",
            profile.name, host.name, alias
        )
    } else {
        match backup_path.as_deref().filter(|value| !value.is_empty()) {
            Some(path) => format!(
                "{} applied to {} with backup {}.",
                profile.name, host.name, path
            ),
            None => format!(
                "{} applied to {}; no previous config backup was needed.",
                profile.name, host.name
            ),
        }
    };
    let summary = if let Some(error) = local_persist_error.as_deref() {
        format!(
            "{} applied to {} remotely, but local state persistence failed: {}",
            profile.name, host.name, error
        )
    } else if !commit_ok {
        format!(
            "{} could not be applied to {}; see task logs.",
            profile.name, host.name
        )
    } else if remote_env_configured == Some(false) {
        format!(
            "{} applied to {}, but remote API env setup failed.",
            profile.name, host.name
        )
    } else if api_key_env_present == Some(false) {
        format!(
            "{} applied to {}, but remote {} is missing.",
            profile.name,
            host.name,
            profile_api_env_label(profile)
        )
    } else if matches!(
        reload.status,
        RemoteCodexReloadStatus::ManualRequired | RemoteCodexReloadStatus::Failed
    ) {
        format!(
            "{} applied to {}, but remote Codex reload did not complete; reconnect manually.",
            profile.name, host.name
        )
    } else if cleanup_hard_failed {
        format!(
            "{} applied to {}; the verified runtime remains active, but managed runtime cleanup failed.",
            profile.name, host.name
        )
    } else {
        base_summary
    };
    let config_succeeded = remote_apply_ready && local_persist_error.is_none();
    let reload_succeeded = reload_result_completed(&reload);
    let task = profile_apply_task(
        &task_id,
        &host,
        profile_apply_task_status(config_succeeded, reload_succeeded, cleanup_hard_failed),
        &summary,
        logs,
        if config_succeeded {
            TaskStepStatus::Success
        } else {
            TaskStepStatus::Failed
        },
        &reload,
    );
    record_task(state, task.clone())?;
    Ok(ProfileApplyHostExecution {
        config_status: if !config_succeeded {
            "failed"
        } else if config_unchanged {
            "no-change"
        } else {
            "success"
        }
        .into(),
        reload,
        task,
    })
}

fn finish_failed_profile_apply(
    state: &AppState,
    task_id: &str,
    host: &Host,
    reload_mode: RemoteCodexReloadMode,
    summary: &str,
    logs: Vec<TaskLog>,
) -> Result<ProfileApplyHostExecution, String> {
    let reload = skipped_reload_result(
        reload_mode,
        "Remote Codex reload was skipped because profile apply did not complete.",
    );
    let task = profile_apply_task(
        task_id,
        host,
        TaskStatus::Failed,
        summary,
        logs,
        TaskStepStatus::Failed,
        &reload,
    );
    record_task(state, task.clone())?;
    Ok(ProfileApplyHostExecution {
        config_status: "failed".into(),
        reload,
        task,
    })
}

fn skipped_reload_result(mode: RemoteCodexReloadMode, message: &str) -> RemoteCodexReloadResult {
    let (status, message) = if mode == RemoteCodexReloadMode::None {
        (
            RemoteCodexReloadStatus::NotRequested,
            "Remote Codex reload was not requested.".to_string(),
        )
    } else {
        (RemoteCodexReloadStatus::Skipped, message.to_string())
    };
    RemoteCodexReloadResult {
        mode,
        status,
        targeted_count: 0,
        stopped_count: 0,
        preserved_cli_count: 0,
        replacement_observed: false,
        message,
    }
}

fn reload_result_completed(result: &RemoteCodexReloadResult) -> bool {
    matches!(
        result.status,
        RemoteCodexReloadStatus::NotRequested
            | RemoteCodexReloadStatus::NotRunning
            | RemoteCodexReloadStatus::Reloaded
            | RemoteCodexReloadStatus::Reconnected
    )
}

pub(crate) fn profile_apply_task_status(
    config_succeeded: bool,
    reload_succeeded: bool,
    cleanup_hard_failed: bool,
) -> TaskStatus {
    if config_succeeded && reload_succeeded && !cleanup_hard_failed {
        TaskStatus::Success
    } else {
        TaskStatus::Failed
    }
}

fn reload_step_status(result: &RemoteCodexReloadResult) -> TaskStepStatus {
    match result.status {
        RemoteCodexReloadStatus::NotRequested | RemoteCodexReloadStatus::Skipped => {
            TaskStepStatus::Skipped
        }
        RemoteCodexReloadStatus::NotRunning
        | RemoteCodexReloadStatus::Reloaded
        | RemoteCodexReloadStatus::Reconnected => TaskStepStatus::Success,
        RemoteCodexReloadStatus::ManualRequired | RemoteCodexReloadStatus::Failed => {
            TaskStepStatus::Failed
        }
    }
}

pub(crate) fn profile_apply_task(
    task_id: &str,
    host: &Host,
    status: TaskStatus,
    summary: &str,
    logs: Vec<TaskLog>,
    config_step_status: TaskStepStatus,
    reload: &RemoteCodexReloadResult,
) -> TaskRun {
    let now = timestamp_label();
    let reload_step_status = reload_step_status(reload);
    let config_step_summary = if matches!(config_step_status, TaskStepStatus::Success) {
        "Remote config, metadata, API environment, and launcher were applied and verified."
    } else {
        "Remote profile files or API environment could not be fully applied and verified."
    };
    let reload_started = !matches!(reload_step_status, TaskStepStatus::Skipped);
    TaskRun {
        id: task_id.to_string(),
        host_id: host.id.clone(),
        host_name: host.name.clone(),
        action: "Apply profile".into(),
        status,
        started_at: now.clone(),
        ended_at: Some(now.clone()),
        summary: summary.to_string(),
        steps: vec![
            TaskStep {
                task_run_id: task_id.to_string(),
                step_id: PROFILE_APPLY_STEP_ID.into(),
                sequence: 0,
                status: config_step_status,
                summary: config_step_summary.into(),
                started_at: Some(now.clone()),
                ended_at: Some(now.clone()),
            },
            TaskStep {
                task_run_id: task_id.to_string(),
                step_id: REMOTE_CODEX_RELOAD_STEP_ID.into(),
                sequence: 1,
                status: reload_step_status,
                summary: reload.message.clone(),
                started_at: reload_started.then_some(now.clone()),
                ended_at: Some(now),
            },
        ],
        logs,
    }
}

pub(crate) fn write_profile_temp_file(
    state: &AppState,
    task_id: &str,
    rendered_toml: &str,
) -> Result<PathBuf, String> {
    let dir = state.paths.cache_file("profile-apply");
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join(format!("{task_id}.toml"));
    let mut file = fs::File::create(&path).map_err(|error| error.to_string())?;
    file.write_all(rendered_toml.as_bytes())
        .map_err(|error| error.to_string())?;
    Ok(path)
}

pub(crate) fn profile_apply_commit_script(
    staged_path: &str,
    expected_sha256: &str,
    expected_bytes: usize,
    metadata_json: &str,
    timestamp: &str,
) -> String {
    format!(
        r#"set -u
staged="{staged_path}"
expected_sha="{expected_sha256}"
expected_bytes="{expected_bytes}"
config_dir="$HOME/.codex"
hub_dir="$HOME/.codex-hub"
config="$config_dir/config.toml"
tmp="$config.codexhub.tmp.{timestamp}"
backup="$config.codexhub.bak.{timestamp}"
mkdir -p "$config_dir" "$hub_dir"
if [ ! -f "$staged" ]; then
  printf 'staged profile file is missing: %s\n' "$staged" >&2
  exit 2
fi
validation="bytes"
actual=""
if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$staged" | awk '{{print $1}}')
  validation="sha256"
elif command -v shasum >/dev/null 2>&1; then
  actual=$(shasum -a 256 "$staged" | awk '{{print $1}}')
  validation="sha256"
fi
if [ "$validation" = "sha256" ]; then
  if [ "$actual" != "$expected_sha" ]; then
    printf 'checksum mismatch for staged profile config\n' >&2
    rm -f "$staged"
    exit 3
  fi
else
  actual=$(wc -c < "$staged" | tr -d '[:space:]')
  if [ "$actual" != "$expected_bytes" ]; then
    printf 'byte-count mismatch for staged profile config\n' >&2
    rm -f "$staged"
    exit 3
  fi
fi
backup_path=""
changed="yes"
if [ -f "$config" ] && cmp -s "$config" "$staged"; then
  changed="no"
  rm -f "$staged"
else
  if [ -f "$config" ]; then
    cp -p "$config" "$backup"
    backup_path="$backup"
  fi
  mv "$staged" "$tmp"
  chmod 600 "$tmp"
  mv "$tmp" "$config"
fi
cat >"$hub_dir/applied-profile.json" <<'CODEXHUB_PROFILE_JSON'
{metadata_json}
CODEXHUB_PROFILE_JSON
chmod 600 "$hub_dir/applied-profile.json"
printf 'CODEXHUB_PROFILE_CHANGED=%s\n' "$changed"
printf 'CODEXHUB_PROFILE_BACKUP=%s\n' "$backup_path"
printf 'CODEXHUB_PROFILE_VALIDATION=%s\n' "$validation"
"#
    )
}

pub(crate) fn profile_apply_metadata_script(metadata_json: &str) -> String {
    format!(
        r#"set -u
hub_dir="$HOME/.codex-hub"
mkdir -p "$hub_dir"
cat >"$hub_dir/applied-profile.json" <<'CODEXHUB_PROFILE_JSON'
{metadata_json}
CODEXHUB_PROFILE_JSON
chmod 600 "$hub_dir/applied-profile.json"
printf 'CODEXHUB_PROFILE_CHANGED=no\n'
printf 'CODEXHUB_PROFILE_BACKUP=\n'
"#
    )
}

// Reloads only identities proven through Linux /proc; broad command-line matching is forbidden.
pub(crate) fn remote_codex_reload_script(mode: RemoteCodexReloadMode) -> String {
    let mode = match mode {
        RemoteCodexReloadMode::None => "none",
        RemoteCodexReloadMode::AppServices => "app-services",
        RemoteCodexReloadMode::AllCodex => "all-codex",
    };
    format!(
        r#"set -u
umask 077
mode='{mode}'
work_dir="${{TMPDIR:-/tmp}}/codexhub-codex-reload.$$"
candidates="$work_dir/candidates"
mkdir -p "$work_dir" || exit 70
: >"$candidates" || exit 70
cleanup() {{
  rm -f "$candidates"
  rmdir "$work_dir" 2>/dev/null || true
}}
trap cleanup EXIT HUP INT TERM

print_result() {{
  printf 'CODEXHUB_RELOAD_STATUS=%s\n' "$1"
  printf 'CODEXHUB_RELOAD_TARGETED=%s\n' "$targeted"
  printf 'CODEXHUB_RELOAD_STOPPED=%s\n' "$stopped"
  printf 'CODEXHUB_RELOAD_PRESERVED_CLI=%s\n' "$preserved_cli"
  printf 'CODEXHUB_RELOAD_REPLACEMENT_OBSERVED=%s\n' "$replacement_observed"
  printf 'CODEXHUB_RELOAD_REASON=%s\n' "$2"
}}

targeted=0
stopped=0
preserved_cli=0
replacement_observed=no
initial_app_services=0
kill_failed=0
identity_uncertain=no
if [ "$mode" = "none" ]; then
  print_result not-requested disabled
  exit 0
fi
if [ ! -d /proc ] || ! command -v id >/dev/null 2>&1 ||
  ! command -v awk >/dev/null 2>&1 || ! command -v sed >/dev/null 2>&1 ||
  ! command -v tr >/dev/null 2>&1 || ! command -v grep >/dev/null 2>&1 ||
  ! command -v sleep >/dev/null 2>&1; then
  print_result manual-required proc-unavailable
  exit 4
fi
current_uid=$(id -u) || {{ print_result manual-required uid-unavailable; exit 4; }}

load_process() {{
  proc_pid=$1
  case "$proc_pid" in *[!0-9]*|'') return 1 ;; esac
  proc_dir="/proc/$proc_pid"
  [ -r "$proc_dir/status" ] && [ -r "$proc_dir/stat" ] && [ -r "$proc_dir/cmdline" ] || return 1
  proc_uid=$(awk '/^Uid:/ {{ print $2; exit }}' "$proc_dir/status" 2>/dev/null)
  [ "$proc_uid" = "$current_uid" ] || return 1
  proc_start=$(sed 's/^[^)]*) //' "$proc_dir/stat" 2>/dev/null | awk '{{ print $20 }}')
  [ -n "$proc_start" ] || return 1
  proc_argv0=$(tr '\000' '\n' <"$proc_dir/cmdline" 2>/dev/null | sed -n '1p')
  proc_arg1=$(tr '\000' '\n' <"$proc_dir/cmdline" 2>/dev/null | sed -n '2p')
  proc_arg2=$(tr '\000' '\n' <"$proc_dir/cmdline" 2>/dev/null | sed -n '3p')
  [ -n "$proc_argv0" ] || return 1
  proc_base=${{proc_argv0##*/}}
  proc_arg1_base=${{proc_arg1##*/}}
  proc_comm=$(sed -n '1p' "$proc_dir/comm" 2>/dev/null || true)
  return 0
}}

has_codex_identity_hint() {{
  hint_pid=$1
  hint_dir="/proc/$hint_pid"
  [ -r "$hint_dir/status" ] && [ -r "$hint_dir/comm" ] || return 1
  hint_uid=$(awk '/^Uid:/ {{ print $2; exit }}' "$hint_dir/status" 2>/dev/null)
  [ "$hint_uid" = "$current_uid" ] || return 1
  hint_comm=$(sed -n '1p' "$hint_dir/comm" 2>/dev/null || true)
  case "$hint_comm" in
    codex|codex-app-server|codex-app-serve|codex-remote-control|codex-remote-co) return 0 ;;
  esac
  return 1
}}

is_suspicious_codex_identity() {{
  case "$proc_base" in
    codex|codex-app-server|codex-remote-control) return 0 ;;
  esac
  case "$proc_comm" in
    codex|codex-app-server|codex-app-serve|codex-remote-control|codex-remote-co) return 0 ;;
  esac
  case "$proc_base:$proc_arg1_base:$proc_arg2" in
    node:codex.js:app-server|node:codex.js:remote-control) return 0 ;;
  esac
  return 1
}}

is_app_service() {{
  case "$proc_base:$proc_comm:$proc_arg1" in
    codex:codex:app-server|codex:codex:remote-control) return 0 ;;
    codex-app-server:codex-app-server:*|codex-app-server:codex-app-serve:*) return 0 ;;
    codex-remote-control:codex-remote-control:*|codex-remote-control:codex-remote-co:*) return 0 ;;
  esac
  return 1
}}

is_all_codex() {{
  is_app_service && return 0
  [ "$proc_base" = "codex" ] && [ "$proc_comm" = "codex" ]
}}

matches_mode() {{
  if [ "$mode" = "app-services" ]; then
    is_app_service
  else
    is_all_codex
  fi
}}

for proc_dir in /proc/[0-9]*; do
  pid=${{proc_dir##*/}}
  if ! load_process "$pid"; then
    if has_codex_identity_hint "$pid"; then
      identity_uncertain=yes
    fi
    continue
  fi
  if ! is_all_codex && is_suspicious_codex_identity; then
    identity_uncertain=yes
    continue
  fi
  if [ "$mode" = "app-services" ] && ! is_app_service; then
    if is_all_codex; then
      preserved_cli=$((preserved_cli + 1))
    fi
    continue
  fi
  matches_mode || continue
  kind=codex
  if is_app_service; then
    kind=app-service
    initial_app_services=$((initial_app_services + 1))
  fi
  printf '%s %s %s\n' "$pid" "$proc_start" "$kind" >>"$candidates"
  targeted=$((targeted + 1))
done

if [ "$targeted" -eq 0 ]; then
  if [ "$identity_uncertain" = "yes" ]; then
    print_result manual-required unverified-process
    exit 4
  fi
  print_result not-running no-matching-process
  exit 0
fi

while IFS=' ' read -r pid expected_start kind; do
  if ! load_process "$pid"; then
    [ -d "/proc/$pid" ] && identity_uncertain=yes
    continue
  fi
  [ "$proc_start" = "$expected_start" ] || continue
  matches_mode || continue
  if ! kill -TERM "$pid" 2>/dev/null; then
    kill_failed=$((kill_failed + 1))
  fi
done <"$candidates"

elapsed=0
remaining=$targeted
while [ "$elapsed" -lt 5 ]; do
  remaining=0
  while IFS=' ' read -r pid expected_start kind; do
    if load_process "$pid"; then
      if [ "$proc_start" = "$expected_start" ]; then
        remaining=$((remaining + 1))
      fi
    elif [ -d "/proc/$pid" ]; then
      identity_uncertain=yes
      remaining=$((remaining + 1))
    fi
  done <"$candidates"
  [ "$remaining" -eq 0 ] && break
  sleep 1
  elapsed=$((elapsed + 1))
done
remaining=0
while IFS=' ' read -r pid expected_start kind; do
  if load_process "$pid"; then
    if [ "$proc_start" = "$expected_start" ]; then
      remaining=$((remaining + 1))
    fi
  elif [ -d "/proc/$pid" ]; then
    identity_uncertain=yes
    remaining=$((remaining + 1))
  fi
done <"$candidates"
stopped=$((targeted - remaining))
if [ "$kill_failed" -ne 0 ]; then
  print_result manual-required term-failed
  exit 4
fi
if [ "$identity_uncertain" = "yes" ]; then
  print_result manual-required unverified-process
  exit 4
fi
if [ "$remaining" -ne 0 ]; then
  print_result manual-required old-process-still-running
  exit 4
fi

if [ "$initial_app_services" -gt 0 ]; then
  while [ "$elapsed" -lt 15 ]; do
    for proc_dir in /proc/[0-9]*; do
      pid=${{proc_dir##*/}}
      load_process "$pid" || continue
      is_app_service || continue
      if ! grep -q "^$pid $proc_start " "$candidates" 2>/dev/null; then
        replacement_observed=yes
        break
      fi
    done
    [ "$replacement_observed" = "yes" ] && break
    sleep 1
    elapsed=$((elapsed + 1))
  done
  if [ "$replacement_observed" = "no" ]; then
    for proc_dir in /proc/[0-9]*; do
      pid=${{proc_dir##*/}}
      load_process "$pid" || continue
      is_app_service || continue
      if ! grep -q "^$pid $proc_start " "$candidates" 2>/dev/null; then
        replacement_observed=yes
        break
      fi
    done
  fi
  if [ "$replacement_observed" = "yes" ]; then
    print_result reconnected replacement-observed
    exit 0
  fi
  print_result manual-required replacement-not-observed
  exit 4
fi

print_result reloaded processes-stopped
exit 0
"#
    )
}

pub(crate) fn parse_remote_codex_reload_result(
    mode: RemoteCodexReloadMode,
    output: &ssh::SshCommandOutput,
) -> RemoteCodexReloadResult {
    let targeted_count = marker_value(&output.stdout, "CODEXHUB_RELOAD_TARGETED")
        .and_then(|value| value.parse::<u32>().ok());
    let stopped_count = marker_value(&output.stdout, "CODEXHUB_RELOAD_STOPPED")
        .and_then(|value| value.parse::<u32>().ok());
    let preserved_cli_count = marker_value(&output.stdout, "CODEXHUB_RELOAD_PRESERVED_CLI")
        .and_then(|value| value.parse::<u32>().ok());
    let replacement_marker = marker_value(&output.stdout, "CODEXHUB_RELOAD_REPLACEMENT_OBSERVED");
    let protocol_valid = targeted_count.is_some()
        && stopped_count.is_some()
        && preserved_cli_count.is_some()
        && matches!(replacement_marker.as_deref(), Some("yes") | Some("no"));
    let targeted_count = targeted_count.unwrap_or(0);
    let stopped_count = stopped_count.unwrap_or(0);
    let preserved_cli_count = preserved_cli_count.unwrap_or(0);
    let replacement_observed = replacement_marker.as_deref() == Some("yes");
    let marker_status = marker_value(&output.stdout, "CODEXHUB_RELOAD_STATUS");
    let reason_marker = marker_value(&output.stdout, "CODEXHUB_RELOAD_REASON");
    let reason = reason_marker.clone().unwrap_or_default();
    let counts_consistent = stopped_count <= targeted_count;
    let status_consistent = match marker_status.as_deref() {
        Some("not-requested") => {
            mode == RemoteCodexReloadMode::None
                && targeted_count == 0
                && stopped_count == 0
                && !replacement_observed
        }
        Some("not-running") => {
            mode != RemoteCodexReloadMode::None
                && targeted_count == 0
                && stopped_count == 0
                && !replacement_observed
        }
        Some("reloaded") => {
            mode != RemoteCodexReloadMode::None
                && targeted_count > 0
                && stopped_count == targeted_count
                && !replacement_observed
        }
        Some("reconnected") => {
            mode != RemoteCodexReloadMode::None
                && targeted_count > 0
                && stopped_count == targeted_count
                && replacement_observed
        }
        Some("manual-required") => mode != RemoteCodexReloadMode::None,
        _ => false,
    };
    let protocol_valid = protocol_valid
        && counts_consistent
        && status_consistent
        && reason_marker
            .as_deref()
            .is_some_and(|value| !value.is_empty());
    let output_completed = output.exit_code.is_some() && !output.timed_out;
    let status = match marker_status.as_deref() {
        Some("not-requested") if output.success() && protocol_valid => {
            RemoteCodexReloadStatus::NotRequested
        }
        Some("not-running") if output.success() && protocol_valid => {
            RemoteCodexReloadStatus::NotRunning
        }
        Some("reloaded") if output.success() && protocol_valid => RemoteCodexReloadStatus::Reloaded,
        Some("reconnected") if output.success() && protocol_valid => {
            RemoteCodexReloadStatus::Reconnected
        }
        Some("manual-required") if output_completed && protocol_valid => {
            RemoteCodexReloadStatus::ManualRequired
        }
        _ => RemoteCodexReloadStatus::Failed,
    };
    let mut message = match status {
        RemoteCodexReloadStatus::NotRequested => "Remote Codex reload was not requested.".into(),
        RemoteCodexReloadStatus::NotRunning => {
            "No matching remote Codex process was running; future sessions will use the applied profile.".into()
        }
        RemoteCodexReloadStatus::Reloaded => format!(
            "Stopped {stopped_count} matching remote Codex process(es)."
        ),
        RemoteCodexReloadStatus::Reconnected => format!(
            "Stopped {stopped_count} old remote Codex process(es) and observed a replacement App service."
        ),
        RemoteCodexReloadStatus::ManualRequired => match reason.as_str() {
            "replacement-not-observed" => "Old Codex App service processes exited, but no replacement appeared within 15 seconds; reconnect the host manually.".into(),
            "old-process-still-running" => format!(
                "Stopped {stopped_count} of {targeted_count} targeted remote Codex process(es); reconnect manually after closing the remaining process(es)."
            ),
            "term-failed" => format!(
                "Stopped {stopped_count} of {targeted_count} targeted remote Codex process(es), but at least one TERM request failed; reconnect manually."
            ),
            "unverified-process" => "A possible Codex process could not be verified safely and was preserved; reconnect the host manually.".into(),
            "proc-unavailable" | "uid-unavailable" => "The remote process identity could not be verified safely; reconnect the host manually.".into(),
            _ => "Remote Codex reload requires manual recovery; reconnect the host manually.".into(),
        },
        RemoteCodexReloadStatus::Failed => {
            "Remote Codex reload could not be confirmed because the SSH command or result protocol failed; reconnect manually.".into()
        }
        RemoteCodexReloadStatus::Skipped => "Remote Codex reload was skipped.".into(),
    };
    if preserved_cli_count > 0 {
        message.push_str(&format!(
            " Preserved {preserved_cli_count} non-App Codex CLI process(es)."
        ));
    }
    RemoteCodexReloadResult {
        mode,
        status,
        targeted_count,
        stopped_count,
        preserved_cli_count,
        replacement_observed,
        message,
    }
}

pub(crate) fn reload_remote_codex_processes(
    alias: &str,
    mode: RemoteCodexReloadMode,
    task_id: &str,
    logs: &mut Vec<TaskLog>,
    next_log: &mut usize,
    timeout: u64,
) -> RemoteCodexReloadResult {
    if mode == RemoteCodexReloadMode::None {
        let result = skipped_reload_result(mode, "Remote Codex reload was not requested.");
        logs.push(basic_log(
            task_id,
            *next_log,
            TaskLogLevel::Info,
            &result.message,
        ));
        *next_log += 1;
        return result;
    }
    let script = remote_codex_reload_script(mode);
    let output = ssh::run_ssh_script(alias, &script, timeout.max(30_000)).unwrap_or_else(|error| {
        failed_command_output(
            format!("ssh {alias} reload remote Codex processes"),
            format!("Could not reload remote Codex processes: {error}"),
        )
    });
    let result = parse_remote_codex_reload_result(mode, &output);
    let level = match result.status {
        RemoteCodexReloadStatus::ManualRequired => TaskLogLevel::Warn,
        RemoteCodexReloadStatus::Failed => TaskLogLevel::Error,
        _ => TaskLogLevel::Info,
    };
    // The reload protocol may be preceded by an SSH banner or other noise. Persist only
    // the parsed result so task history never receives a process argv or secret by accident.
    logs.push(basic_log(
        task_id,
        *next_log,
        level,
        &remote_codex_reload_log_message(&result),
    ));
    *next_log += 1;
    result
}

pub(crate) fn remote_codex_reload_log_message(result: &RemoteCodexReloadResult) -> String {
    let mode = match result.mode {
        RemoteCodexReloadMode::None => "none",
        RemoteCodexReloadMode::AppServices => "app-services",
        RemoteCodexReloadMode::AllCodex => "all-codex",
    };
    let status = match result.status {
        RemoteCodexReloadStatus::NotRequested => "not-requested",
        RemoteCodexReloadStatus::Skipped => "skipped",
        RemoteCodexReloadStatus::NotRunning => "not-running",
        RemoteCodexReloadStatus::Reloaded => "reloaded",
        RemoteCodexReloadStatus::Reconnected => "reconnected",
        RemoteCodexReloadStatus::ManualRequired => "manual-required",
        RemoteCodexReloadStatus::Failed => "failed",
    };
    format!(
        "{} [mode={mode}, status={status}, targeted={}, stopped={}, preservedCli={}, replacementObserved={}].",
        result.message,
        result.targeted_count,
        result.stopped_count,
        result.preserved_cli_count,
        result.replacement_observed
    )
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub(crate) fn update_host_profile_apply(
    app: &AppHandle,
    state: &AppState,
    host_id: &str,
    alias: &str,
    profile: &Profile,
    api_key_env_present: Option<bool>,
) -> Result<storage::RelatedWriteResult, String> {
    let _write_guard = services::profile_links::acquire_write_lock(state)?;
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned").clone();
    if let Some(host) = hosts
        .iter_mut()
        .find(|host| host.id == host_id || host.host_alias.eq_ignore_ascii_case(alias))
    {
        host.status = HostStatus::Online;
        host.profile_id = Some(profile.id.clone());
        host.config_exists = Some(true);
        host.api_config_name = Some(profile.name.clone());
        host.api_config_source = Some("profile".into());
        host.api_key_env_var = profile.api_key_env_var.clone();
        host.api_key_env_present = api_key_env_present;
        host.last_seen = "just now".into();
    }
    let mut profiles = load_profiles(app, state)?;
    sync_profile_host_ids(&mut profiles, &profile.id, host_id, alias);
    services::profile_links::save(
        state,
        &format!("operation-profile-link-{}", timestamp_millis()),
        profiles,
        hosts,
    )
}

pub(crate) fn sync_profile_host_ids(
    profiles: &mut [Profile],
    profile_id: &str,
    host_id: &str,
    alias: &str,
) {
    let canonical_host_id = if host_id.trim().is_empty() {
        alias.trim()
    } else {
        host_id.trim()
    };
    if canonical_host_id.is_empty() {
        return;
    }
    let host_keys = [host_id, alias, canonical_host_id]
        .into_iter()
        .map(normalize_host_link_key)
        .filter(|key| !key.is_empty())
        .collect::<BTreeSet<_>>();

    for profile in profiles.iter_mut() {
        profile
            .host_ids
            .retain(|existing| !host_keys.contains(&normalize_host_link_key(existing)));
        if profile.id == profile_id
            && !profile.host_ids.iter().any(|existing| {
                normalize_host_link_key(existing) == normalize_host_link_key(canonical_host_id)
            })
        {
            profile.host_ids.push(canonical_host_id.to_string());
        }
    }
}

pub(crate) fn clear_profile_host_ids(profiles: &mut [Profile], host_id: &str, alias: &str) {
    let host_keys = [host_id, alias]
        .into_iter()
        .map(normalize_host_link_key)
        .filter(|key| !key.is_empty())
        .collect::<BTreeSet<_>>();
    for profile in profiles.iter_mut() {
        profile
            .host_ids
            .retain(|existing| !host_keys.contains(&normalize_host_link_key(existing)));
    }
}

pub(crate) fn normalize_host_link_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

pub(crate) fn profile_apply_profiles_snapshot(
    app: &AppHandle,
    state: &AppState,
) -> Result<Vec<Profile>, String> {
    load_profiles(app, state)
}

pub(crate) fn profile_apply_hosts_snapshot(state: &AppState) -> Result<Vec<Host>, String> {
    merge_discovered_hosts(state)?;
    Ok(state.hosts.lock().expect("hosts mutex poisoned").clone())
}

pub(crate) fn reconcile_hosts_with_profile_links(state: &AppState, profiles: &[Profile]) {
    let profile_links = profiles
        .iter()
        .flat_map(|profile| {
            profile
                .host_ids
                .iter()
                .map(move |host_id| (normalize_host_link_key(host_id), profile))
        })
        .filter(|(host_key, _)| !host_key.is_empty())
        .collect::<Vec<_>>();
    if profile_links.is_empty() {
        return;
    }

    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");
    for host in hosts.iter_mut() {
        let host_keys = [
            normalize_host_link_key(&host.id),
            normalize_host_link_key(&host.host_alias),
        ];
        if let Some((_, profile)) = profile_links.iter().find(|(linked_host_key, _)| {
            host_keys.iter().any(|host_key| host_key == linked_host_key)
        }) {
            host.profile_id = Some(profile.id.clone());
        }
    }
}

pub(crate) fn record_task(state: &AppState, task: TaskRun) -> Result<(), String> {
    if let Err(error) = jobs::persist_task(&state.task_store, state.task_event_sink.as_ref(), &task)
    {
        let safe_error = redact_error_text(&error);
        if TaskStore::is_payload_invariant_error(&error) {
            return Err(format!(
                "Task finalization rejected an invalid log payload: {safe_error}"
            ));
        }
        if let Ok(mut current) = state.task_storage_error.lock() {
            *current = Some(safe_error.clone());
        }
        return Err(format!(
            "Persistent task storage failed while finalizing the operation: {}",
            safe_error
        ));
    }
    Ok(())
}

pub(crate) fn log_best_effort<T, E>(context: &str, result: Result<T, E>)
where
    E: std::fmt::Display,
{
    if let Err(error) = result {
        eprintln!(
            "Best-effort {context} failed: {}",
            redact_error_text(&error.to_string())
        );
    }
}

pub(crate) fn ensure_task_storage_healthy(state: &AppState) -> Result<(), String> {
    state.paths.ensure_resolved()?;
    let current = state
        .task_storage_error
        .lock()
        .map_err(|_| "Task storage health mutex was poisoned.".to_string())?;
    if let Some(error) = current.as_ref() {
        return Err(format!("Persistent task storage is unavailable: {error}"));
    }
    Ok(())
}

pub(crate) fn ensure_task_storage_for_app(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    ensure_task_storage_healthy(&state)
}

pub(crate) fn run_durable_local<T, F>(
    state: &AppState,
    action: &str,
    domain: &str,
    operation: F,
) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String>,
{
    ensure_task_storage_healthy(state)?;
    jobs::run_local_operation(
        &state.task_store,
        state.task_event_sink.as_ref(),
        action,
        domain,
        operation,
    )
}

pub(crate) fn update_host_check(state: &AppState, alias: &str, ok: bool, duration_ms: u64) {
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");
    if let Some(host) = hosts
        .iter_mut()
        .find(|host| host.host_alias.eq_ignore_ascii_case(alias))
    {
        host.status = if ok {
            HostStatus::Online
        } else {
            HostStatus::Offline
        };
        host.latency_ms = if ok { Some(duration_ms) } else { None };
        if ok {
            host.last_seen = "just now".into();
        }
    }
}

pub(crate) fn persist_host_check(
    state: &AppState,
    alias: &str,
    ok: bool,
    duration_ms: u64,
) -> Result<(), String> {
    // Serialize status-only writes with probe/profile updates so batch results cannot be lost.
    let _write_guard = services::profile_links::acquire_write_lock(state)?;
    update_host_check(state, alias, ok, duration_ms);
    let hosts = state.hosts.lock().expect("hosts mutex poisoned").clone();
    save_hosts_state(state, &hosts)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_host_probe_group_updates(
    host: &mut Host,
    os: &str,
    arch: &str,
    shell: &str,
    path: Option<String>,
    path_has_local_bin: bool,
    codex_command_available: bool,
    codex_installed: bool,
    codex_version: &str,
    config_exists: bool,
    api_config_match: &RemoteApiConfigMatch,
    api_key_env_var: Option<String>,
    api_key_env_present: Option<bool>,
    skills_exists: bool,
    skills_count: u16,
    system_probe_ok: bool,
    codex_probe_ok: bool,
    api_probe_ok: bool,
    skills_probe_ok: bool,
) {
    host.status = HostStatus::Online;
    if system_probe_ok {
        host.os = os.to_string();
        host.arch = arch.to_string();
        host.shell = shell.to_string();
        host.path = path;
        host.path_has_local_bin = Some(path_has_local_bin);
    }
    if codex_probe_ok {
        host.codex_command_available = Some(codex_command_available);
        host.codex_installed = codex_installed;
        host.codex_version = codex_version.to_string();
    }
    if api_probe_ok {
        host.config_exists = Some(config_exists);
        host.api_config_name = Some(api_config_match.name.clone());
        host.api_config_source = Some(api_config_match.source.clone());
        host.api_key_env_var = api_key_env_var;
        host.api_key_env_present = api_key_env_present;
        host.profile_id = api_config_match.profile_id.clone();
    }
    if skills_probe_ok {
        host.skills_exists = Some(skills_exists);
        host.skills_count = Some(skills_count);
    }
    host.last_seen = "just now".into();
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_host_probe(
    app: &AppHandle,
    state: &AppState,
    alias: &str,
    os: &str,
    arch: &str,
    shell: &str,
    path: Option<String>,
    path_has_local_bin: bool,
    codex_command_available: bool,
    codex_installed: bool,
    codex_version: &str,
    config_exists: bool,
    api_config_match: &RemoteApiConfigMatch,
    api_key_env_var: Option<String>,
    api_key_env_present: Option<bool>,
    skills_exists: bool,
    skills_count: u16,
    system_probe_ok: bool,
    codex_probe_ok: bool,
    api_probe_ok: bool,
    skills_probe_ok: bool,
) -> Result<storage::RelatedWriteResult, String> {
    let _write_guard = services::profile_links::acquire_write_lock(state)?;
    let mut probed_host_id = None;
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned").clone();
    if let Some(host) = hosts
        .iter_mut()
        .find(|host| host.host_alias.eq_ignore_ascii_case(alias))
    {
        apply_host_probe_group_updates(
            host,
            os,
            arch,
            shell,
            path,
            path_has_local_bin,
            codex_command_available,
            codex_installed,
            codex_version,
            config_exists,
            api_config_match,
            api_key_env_var,
            api_key_env_present,
            skills_exists,
            skills_count,
            system_probe_ok,
            codex_probe_ok,
            api_probe_ok,
            skills_probe_ok,
        );
        probed_host_id = Some(host.id.clone());
    }
    let mut profiles = load_profiles(app, state)?;
    if api_probe_ok {
        if let Some(host_id) = probed_host_id {
            if let Some(profile_id) = api_config_match.profile_id.as_deref() {
                sync_profile_host_ids(&mut profiles, profile_id, &host_id, alias);
            } else {
                clear_profile_host_ids(&mut profiles, &host_id, alias);
            }
        }
    }
    services::profile_links::save(
        state,
        &format!("operation-probe-host-{}", timestamp_millis()),
        profiles,
        hosts,
    )
}

pub(crate) fn update_host_codex_status(
    state: &AppState,
    alias: &str,
    codex_installed: bool,
    codex_version: &str,
    path_has_local_bin: bool,
    codex_command_available: bool,
) {
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");
    if let Some(host) = hosts
        .iter_mut()
        .find(|host| host.host_alias.eq_ignore_ascii_case(alias))
    {
        host.status = HostStatus::Online;
        host.codex_installed = codex_installed;
        host.codex_version = codex_version.to_string();
        host.path_has_local_bin = Some(path_has_local_bin);
        host.codex_command_available = Some(codex_command_available);
        host.last_seen = "just now".into();
    }
}

pub(crate) fn host_name_for_alias(state: &AppState, alias: &str) -> String {
    state
        .hosts
        .lock()
        .expect("hosts mutex poisoned")
        .iter()
        .find(|host| host.host_alias.eq_ignore_ascii_case(alias))
        .map(|host| host.name.clone())
        .unwrap_or_else(|| alias.to_string())
}

pub(crate) fn host_id_for_alias(state: &AppState, alias: &str) -> String {
    state
        .hosts
        .lock()
        .expect("hosts mutex poisoned")
        .iter()
        .find(|host| host.host_alias.eq_ignore_ascii_case(alias))
        .map(|host| host.id.clone())
        .unwrap_or_else(|| discovered_host_id(alias))
}

pub(crate) fn failed_command_output(command: String, message: String) -> ssh::SshCommandOutput {
    ssh::SshCommandOutput {
        command,
        stdout: String::new(),
        stderr: message,
        exit_code: None,
        duration_ms: 0,
        timed_out: false,
    }
}

pub(crate) fn command_log(
    task_id: &str,
    index: usize,
    level: TaskLogLevel,
    message: &str,
    output: &ssh::SshCommandOutput,
) -> TaskLog {
    TaskLog {
        id: jobs::task_log_id(task_id, index),
        task_run_id: task_id.to_string(),
        step_id: None,
        level,
        timestamp: "now".into(),
        message: message.to_string(),
        command: Some(output.command.clone()),
        stdout: Some(output.stdout.clone()),
        stderr: Some(output.stderr.clone()),
        exit_code: output.exit_code,
        duration_ms: Some(output.duration_ms),
        timed_out: Some(output.timed_out),
    }
}

pub(crate) fn basic_log(
    task_id: &str,
    index: usize,
    level: TaskLogLevel,
    message: &str,
) -> TaskLog {
    TaskLog {
        id: jobs::task_log_id(task_id, index),
        task_run_id: task_id.to_string(),
        step_id: None,
        level,
        timestamp: "now".into(),
        message: message.to_string(),
        command: None,
        stdout: None,
        stderr: None,
        exit_code: None,
        duration_ms: None,
        timed_out: None,
    }
}

pub(crate) fn ssh_check_message(
    alias: &str,
    output: &ssh::SshCommandOutput,
    ok: bool,
    timeout_ms: u64,
) -> String {
    if ok {
        format!("SSH connection to {alias} returned ok.")
    } else if output.timed_out {
        format!("SSH connection to {alias} timed out after {timeout_ms} ms.")
    } else {
        format!(
            "SSH connection to {alias} failed: {}",
            ssh_failure_hint(output)
        )
    }
}

pub(crate) fn ssh_failure_hint(output: &ssh::SshCommandOutput) -> String {
    let detail = command_detail(output);
    let lower = detail.to_ascii_lowercase();
    if lower.contains("host key verification failed") {
        return format!("{detail}. The host key may have changed; CodexHub accepts only first-time new host keys automatically.");
    }
    if lower.contains("permission denied") || lower.contains("no supported authentication methods")
    {
        return format!("{detail}. Use one-time password setup when adding the host so CodexHub can install your local public key, or configure SSH key/agent auth manually.");
    }
    detail
}

pub(crate) fn command_detail(output: &ssh::SshCommandOutput) -> String {
    let stderr = output.stderr.trim();
    if !stderr.is_empty() {
        return stderr.lines().next().unwrap_or(stderr).to_string();
    }
    let stdout = output.stdout.trim();
    if !stdout.is_empty() {
        return stdout.lines().next().unwrap_or(stdout).to_string();
    }
    match output.exit_code {
        Some(code) => format!("exit code {code}"),
        None => "process did not start".into(),
    }
}

pub(crate) fn stdout_optional_yes(output: Option<&ssh::SshCommandOutput>) -> Option<bool> {
    let output = output?;
    let value = output.stdout.trim();
    if value.eq_ignore_ascii_case("yes") {
        Some(true)
    } else if value.eq_ignore_ascii_case("no") {
        Some(false)
    } else {
        None
    }
}

pub(crate) fn path_has_local_bin(path: Option<&str>) -> bool {
    path.unwrap_or_default()
        .split(':')
        .any(|segment| segment == "~/.local/bin" || segment.ends_with("/.local/bin"))
}

pub(crate) fn classify_remote_api_config(
    app: &AppHandle,
    state: &AppState,
    config_exists: bool,
    remote_base_url: Option<&str>,
) -> RemoteApiConfigMatch {
    if !config_exists {
        return RemoteApiConfigMatch {
            name: "No config".into(),
            source: "none".into(),
            profile_id: None,
        };
    }
    let Some(remote_key) = remote_base_url.and_then(normalize_base_url_key) else {
        return RemoteApiConfigMatch {
            name: "Unknown config".into(),
            source: "unknown".into(),
            profile_id: None,
        };
    };
    if let Ok(profiles) = load_profiles(app, state) {
        if let Some(profile) = profiles.into_iter().find(|profile| {
            profile
                .base_url
                .as_deref()
                .and_then(normalize_base_url_key)
                .as_deref()
                == Some(remote_key.as_str())
        }) {
            return RemoteApiConfigMatch {
                source: if profile.source == "cc-switch" {
                    "cc-switch".into()
                } else {
                    "profile".into()
                },
                name: profile.name,
                profile_id: Some(profile.id),
            };
        }
    };
    if let Ok(detected) = detect_cc_switch_profiles_inner(state) {
        if let Some(profile) = detected
            .into_iter()
            .map(|item| item.profile)
            .find(|profile| {
                profile
                    .base_url
                    .as_deref()
                    .and_then(normalize_base_url_key)
                    .as_deref()
                    == Some(remote_key.as_str())
            })
        {
            return RemoteApiConfigMatch {
                name: profile.name,
                source: "cc-switch".into(),
                profile_id: None,
            };
        }
    }
    RemoteApiConfigMatch {
        name: "Unknown config".into(),
        source: "unknown".into(),
        profile_id: None,
    }
}

pub(crate) fn normalize_base_url_key(value: &str) -> Option<String> {
    let mut trimmed = value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string();
    while trimmed.ends_with('/') {
        trimmed.pop();
    }
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_ascii_lowercase())
    }
}
