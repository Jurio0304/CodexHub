use crate::*;

pub(crate) fn apply_profile_to_hosts(
    app: &AppHandle,
    state: &AppState,
    profile: &Profile,
    rendered_toml: &str,
    host_ids: Vec<String>,
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
            logs,
        };
        record_task(state, task.clone())?;
        return Ok(ProfileApplyBatchResult {
            profile_id: profile.id.clone(),
            ok: false,
            results: vec![ProfileApplyHostResult {
                host_id: "no-host".into(),
                host_name: "No host selected".into(),
                host_alias: String::new(),
                status: "failed".into(),
                target_path: "~/.codex/config.toml".into(),
                backup_path: None,
                message: task.summary.clone(),
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
            apply_profile_to_host(app, state, profile, rendered_toml, host.clone(), timeout)
                .map(|task| profile_apply_result_from_task(&host, task))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let tasks = results
        .iter()
        .filter_map(|result| result.task.clone())
        .collect::<Vec<_>>();
    Ok(ProfileApplyBatchResult {
        profile_id: profile.id.clone(),
        ok: results.iter().all(|result| result.status != "failed"),
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
        task: None,
    }
}

pub(crate) fn profile_apply_result_from_task(host: &Host, task: TaskRun) -> ProfileApplyHostResult {
    let status = match &task.status {
        TaskStatus::Success if task.summary.contains("already matched") => "no-change",
        TaskStatus::Success => "success",
        TaskStatus::Failed | TaskStatus::Interrupted => "failed",
        TaskStatus::Queued | TaskStatus::Running => "pending",
    }
    .to_string();
    ProfileApplyHostResult {
        host_id: host.id.clone(),
        host_name: host.name.clone(),
        host_alias: host.host_alias.clone(),
        status,
        target_path: "~/.codex/config.toml".into(),
        backup_path: profile_apply_backup_path_from_task(&task),
        message: task.summary.clone(),
        task: Some(task),
    }
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
            "Wrote remote CodexHub-managed API env and launcher without exposing key material."
        } else {
            "Failed to write remote CodexHub-managed API env or launcher."
        },
        &output,
    ));
    *next_log += 1;
    Some(output.success())
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

launcher="$HOME/.local/bin/codex"
target_file="$env_dir/codex-target"
launcher_backup=""
is_codexhub_launcher() {{
  [ -f "$launcher" ] && head -n 8 "$launcher" 2>/dev/null | grep -F "CodexHub managed launcher" >/dev/null 2>&1
}}
target=""
if [ -f "$target_file" ]; then
  target=$(sed -n '1p' "$target_file")
fi
if [ -z "$target" ] && [ -L "$launcher" ]; then
  target=$(readlink -f "$launcher" 2>/dev/null || true)
fi
if [ -z "$target" ] && [ -x "$HOME/.codex/packages/standalone/current/bin/codex" ]; then
  target="$HOME/.codex/packages/standalone/current/bin/codex"
fi
if [ -z "$target" ] && [ -e "$launcher" ] && ! is_codexhub_launcher; then
  target="$env_dir/codex-original.$(date +%Y%m%d%H%M%S)"
  mv "$launcher" "$target"
  launcher_backup="$target"
fi
launcher_changed=no
if [ -n "$target" ]; then
  printf '%s\n' "$target" >"$target_file"
  chmod 600 "$target_file"
  if [ -e "$launcher" ] && ! is_codexhub_launcher; then
    launcher_backup="$launcher.codexhub.bak.$(date +%Y%m%d%H%M%S)"
    cp -P "$launcher" "$launcher_backup"
  fi
  launcher_tmp="$launcher.codexhub.tmp.$$"
  cat >"$launcher_tmp" <<'CODEXHUB_CODEX_LAUNCHER'
#!/bin/sh
# CodexHub managed launcher: loads remote API env before running real Codex.
if [ -f "$HOME/.codex-hub/env" ]; then
  . "$HOME/.codex-hub/env"
fi
target_file="$HOME/.codex-hub/codex-target"
if [ -f "$target_file" ]; then
  target=$(sed -n '1p' "$target_file")
else
  target="$HOME/.codex/packages/standalone/current/bin/codex"
fi
if [ ! -x "$target" ]; then
  printf 'CodexHub launcher target is not executable: %s\n' "$target" >&2
  exit 127
fi
exec "$target" "$@"
CODEXHUB_CODEX_LAUNCHER
  chmod 700 "$launcher_tmp"
  if [ -f "$launcher" ] && cmp -s "$launcher_tmp" "$launcher"; then
    rm -f "$launcher_tmp"
  else
    mv "$launcher_tmp" "$launcher"
    launcher_changed=yes
  fi
fi

printf 'CODEXHUB_REMOTE_ENV_CHANGED=%s\n' "$env_changed"
printf 'CODEXHUB_REMOTE_ENV_FILE=%s\n' "$env_file"
printf 'CODEXHUB_REMOTE_ENV_BACKUP=%s\n' "$backup_path"
printf 'CODEXHUB_REMOTE_ENV_SOURCE_CHANGED=%s\n' "$source_changed"
printf 'CODEXHUB_REMOTE_ENV_SOURCE_PATHS=%s\n' "$source_paths"
printf 'CODEXHUB_REMOTE_ENV_SOURCE_BACKUPS=%s\n' "$source_backups"
printf 'CODEXHUB_CODEX_LAUNCHER_CHANGED=%s\n' "$launcher_changed"
printf 'CODEXHUB_CODEX_LAUNCHER=%s\n' "$launcher"
printf 'CODEXHUB_CODEX_TARGET=%s\n' "$target"
printf 'CODEXHUB_CODEX_LAUNCHER_BACKUP=%s\n' "$launcher_backup"
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

pub(crate) fn apply_profile_to_host(
    app: &AppHandle,
    state: &AppState,
    profile: &Profile,
    rendered_toml: &str,
    host: Host,
    timeout: u64,
) -> Result<TaskRun, String> {
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
        let task = profile_apply_task(
            &task_id,
            &host,
            TaskStatus::Failed,
            "Profile apply skipped because SSH check failed.",
            logs,
        );
        record_task(state, task.clone())?;
        return Ok(task);
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
        let task = profile_apply_task(
            &task_id,
            &host,
            TaskStatus::Failed,
            "Profile apply failed before mutation because remote config could not be read.",
            logs,
        );
        record_task(state, task.clone())?;
        return Ok(task);
    }

    let metadata = AppliedProfileMetadata {
        profile_id: profile.id.clone(),
        profile_name: profile.name.clone(),
        applied_at: timestamp_label(),
        codexhub_version: env!("CARGO_PKG_VERSION").into(),
    };
    let metadata_json =
        serde_json::to_string_pretty(&metadata).unwrap_or_else(|_| "{}".to_string());

    if read_output.stdout == rendered_toml {
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
        let remote_env_configured = if ok {
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
        let api_key_env_present = if ok && remote_env_configured != Some(false) {
            check_profile_api_env(&alias, profile, &task_id, &mut logs, &mut next_log, timeout)
        } else {
            None
        };
        let local_persist_error = if ok {
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
                &format!("Remote apply succeeded, but local Host/Profile state failed to persist: {error}"),
            ));
        }
        let summary = if let Some(error) = local_persist_error.as_deref() {
            format!(
                "{} matched {} remotely, but local state persistence failed: {}",
                profile.name, alias, error
            )
        } else if ok {
            if remote_env_configured == Some(false) {
                format!(
                    "{} already matched {} on {}, but remote API env setup failed.",
                    profile.name, host.name, alias
                )
            } else if api_key_env_present == Some(false) {
                format!(
                    "{} already matched {} on {}, but remote {} is missing.",
                    profile.name,
                    host.name,
                    alias,
                    profile_api_env_label(profile)
                )
            } else {
                format!(
                    "{} already matched {} on {}; no config backup was created.",
                    profile.name, host.name, alias
                )
            }
        } else {
            format!(
                "{} matched {}, but metadata update failed.",
                profile.name, alias
            )
        };
        let task = profile_apply_task(
            &task_id,
            &host,
            if ok
                && remote_env_configured != Some(false)
                && api_key_env_present != Some(false)
                && local_persist_error.is_none()
            {
                TaskStatus::Success
            } else {
                TaskStatus::Failed
            },
            &summary,
            logs,
        );
        record_task(state, task.clone())?;
        return Ok(task);
    }

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
            let task = profile_apply_task(
                &task_id,
                &host,
                TaskStatus::Failed,
                "Profile apply failed before upload.",
                logs,
            );
            record_task(state, task.clone())?;
            return Ok(task);
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
        let task = profile_apply_task(
            &task_id,
            &host,
            TaskStatus::Failed,
            "Profile apply failed during upload; remote config was not changed.",
            logs,
        );
        record_task(state, task.clone())?;
        return Ok(task);
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
    let commit_ok = commit_output.success();
    let backup_path = marker_value(&commit_output.stdout, "CODEXHUB_PROFILE_BACKUP");
    logs.push(command_log(
        &task_id,
        next_log,
        if commit_ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        if commit_ok {
            "Validated staged TOML and committed remote profile config atomically."
        } else {
            "Failed to validate or commit remote profile config."
        },
        &commit_output,
    ));
    next_log += 1;

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

    let local_persist_error = if commit_ok {
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
    }
    let summary = if let Some(error) = local_persist_error.as_deref() {
        format!(
            "{} applied to {} remotely, but local state persistence failed: {}",
            profile.name, host.name, error
        )
    } else if commit_ok {
        if remote_env_configured == Some(false) {
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
        } else {
            match backup_path.filter(|value| !value.is_empty()) {
                Some(path) => format!(
                    "{} applied to {} with backup {}.",
                    profile.name, host.name, path
                ),
                None => format!(
                    "{} applied to {}; no previous config backup was needed.",
                    profile.name, host.name
                ),
            }
        }
    } else {
        format!(
            "{} could not be applied to {}; see task logs.",
            profile.name, host.name
        )
    };
    let task = profile_apply_task(
        &task_id,
        &host,
        if commit_ok
            && remote_env_configured != Some(false)
            && api_key_env_present != Some(false)
            && local_persist_error.is_none()
        {
            TaskStatus::Success
        } else {
            TaskStatus::Failed
        },
        &summary,
        logs,
    );
    record_task(state, task.clone())?;
    Ok(task)
}

pub(crate) fn profile_apply_task(
    task_id: &str,
    host: &Host,
    status: TaskStatus,
    summary: &str,
    logs: Vec<TaskLog>,
) -> TaskRun {
    TaskRun {
        id: task_id.to_string(),
        host_id: host.id.clone(),
        host_name: host.name.clone(),
        action: "Apply profile".into(),
        status,
        started_at: timestamp_label(),
        ended_at: Some(timestamp_label()),
        summary: summary.to_string(),
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
{safe_reconnect}
printf 'CODEXHUB_PROFILE_CHANGED=%s\n' "$changed"
printf 'CODEXHUB_PROFILE_BACKUP=%s\n' "$backup_path"
printf 'CODEXHUB_PROFILE_VALIDATION=%s\n' "$validation"
"#,
        safe_reconnect = safe_reconnect_shell_fragment()
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
{safe_reconnect}
printf 'CODEXHUB_PROFILE_CHANGED=no\n'
printf 'CODEXHUB_PROFILE_BACKUP=\n'
"#,
        safe_reconnect = safe_reconnect_shell_fragment()
    )
}

pub(crate) fn safe_reconnect_shell_fragment() -> &'static str {
    r#"matches_file="${TMPDIR:-/tmp}/codexhub-reconnect.$$"
if command -v ps >/dev/null 2>&1; then
  ps -u "$(id -u)" -o pid=,comm=,args= 2>/dev/null | awk '
    {
      pid=$1
      comm=$2
      args=$0
      sub(/^[[:space:]]*[0-9]+[[:space:]]+[^[:space:]]+[[:space:]]*/, "", args)
      comm_ok=(comm=="codex" || comm=="codex-app-server" || comm=="codex-remote-control")
      args_ok=(args ~ /(^|[[:space:]])codex([[:space:]].*)?(app-server|remote-control)/ || args ~ /codex[[:space:]]+(app-server|remote-control)/)
      if (comm_ok && args_ok) print pid
    }
  ' > "$matches_file"
  count=$(wc -l < "$matches_file" | tr -d '[:space:]')
  if [ "$count" = "1" ]; then
    pid=$(cat "$matches_file")
    if kill -TERM "$pid" 2>/dev/null; then
      printf 'CODEXHUB_RECONNECT=term:%s\n' "$pid"
    else
      printf 'CODEXHUB_RECONNECT=manual:term-failed\n'
    fi
  elif [ "$count" = "0" ]; then
    printf 'CODEXHUB_RECONNECT=manual:no-safe-process-match\n'
  else
    printf 'CODEXHUB_RECONNECT=manual:ambiguous-process-match\n'
  fi
  rm -f "$matches_file"
else
  printf 'CODEXHUB_RECONNECT=manual:ps-unavailable\n'
fi"#
}

#[allow(dead_code)]
pub(crate) fn safe_reconnect_decision_from_ps(ps_output: &str) -> SafeReconnectDecision {
    let pids = safe_reconnect_candidate_pids(ps_output);
    match pids.as_slice() {
        [pid] => SafeReconnectDecision::Terminate(*pid),
        [] => SafeReconnectDecision::Manual("no-safe-process-match".into()),
        _ => SafeReconnectDecision::Manual("ambiguous-process-match".into()),
    }
}

#[allow(dead_code)]
pub(crate) fn safe_reconnect_candidate_pids(ps_output: &str) -> Vec<u32> {
    ps_output
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let pid = parts.next()?.parse::<u32>().ok()?;
            let comm = parts.next()?;
            let args = parts.collect::<Vec<_>>().join(" ");
            if is_safe_reconnect_process(comm, &args) {
                Some(pid)
            } else {
                None
            }
        })
        .collect()
}

#[allow(dead_code)]
pub(crate) fn is_safe_reconnect_process(comm: &str, args: &str) -> bool {
    let comm_ok = matches!(comm, "codex" | "codex-app-server" | "codex-remote-control");
    let args_lower = args.to_ascii_lowercase();
    comm_ok
        && args_lower.contains("codex")
        && (args_lower.contains("app-server") || args_lower.contains("remote-control"))
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
        if let Ok(mut current) = state.task_storage_error.lock() {
            *current = Some(redact_error_text(&error));
        }
        return Err(format!(
            "Persistent task storage failed while finalizing the operation: {}",
            redact_error_text(&error)
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
) -> Result<storage::RelatedWriteResult, String> {
    let _write_guard = services::profile_links::acquire_write_lock(state)?;
    let mut probed_host_id = None;
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned").clone();
    if let Some(host) = hosts
        .iter_mut()
        .find(|host| host.host_alias.eq_ignore_ascii_case(alias))
    {
        host.status = HostStatus::Online;
        host.os = os.to_string();
        host.arch = arch.to_string();
        host.shell = shell.to_string();
        host.path = path;
        host.path_has_local_bin = Some(path_has_local_bin);
        host.codex_command_available = Some(codex_command_available);
        host.codex_installed = codex_installed;
        host.codex_version = codex_version.to_string();
        host.config_exists = Some(config_exists);
        host.api_config_name = Some(api_config_match.name.clone());
        host.api_config_source = Some(api_config_match.source.clone());
        host.api_key_env_var = api_key_env_var;
        host.api_key_env_present = api_key_env_present;
        host.profile_id = api_config_match.profile_id.clone();
        host.skills_exists = Some(skills_exists);
        host.skills_count = Some(skills_count);
        host.last_seen = "just now".into();
        probed_host_id = Some(host.id.clone());
    }
    let mut profiles = load_profiles(app, state)?;
    if let Some(host_id) = probed_host_id {
        if let Some(profile_id) = api_config_match.profile_id.as_deref() {
            sync_profile_host_ids(&mut profiles, profile_id, &host_id, alias);
        } else {
            clear_profile_host_ids(&mut profiles, &host_id, alias);
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
        id: format!("{task_id}-log-{index}"),
        task_run_id: task_id.to_string(),
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
        id: format!("{task_id}-log-{index}"),
        task_run_id: task_id.to_string(),
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

pub(crate) fn stdout_or_unknown(output: Option<&ssh::SshCommandOutput>) -> String {
    output
        .filter(|item| item.success())
        .map(|item| item.stdout.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Unknown".into())
}

pub(crate) fn stdout_yes(output: Option<&ssh::SshCommandOutput>) -> bool {
    output
        .filter(|item| item.success())
        .map(|item| item.stdout.trim().eq_ignore_ascii_case("yes"))
        .unwrap_or(false)
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
