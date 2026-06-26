mod ssh;

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Health {
    app: &'static str,
    mode: &'static str,
    remote_wrapper_required: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum AuthMethod {
    SshKey,
    Password,
    Agent,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum HostStatus {
    Online,
    Offline,
    Unknown,
    Testing,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Host {
    id: String,
    name: String,
    host_alias: String,
    source: String,
    address: String,
    port: u16,
    username: String,
    auth_method: AuthMethod,
    status: HostStatus,
    os: String,
    arch: String,
    shell: String,
    path: Option<String>,
    path_has_local_bin: Option<bool>,
    codex_installed: bool,
    codex_version: String,
    config_exists: Option<bool>,
    skills_exists: Option<bool>,
    skills_count: Option<u16>,
    profile_id: Option<String>,
    skill_pack_ids: Vec<String>,
    tags: Vec<String>,
    last_seen: String,
    latency_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostDraft {
    name: String,
    address: String,
    port: u16,
    username: String,
    auth_method: AuthMethod,
    tags: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostPatch {
    name: Option<String>,
    address: Option<String>,
    port: Option<u16>,
    username: Option<String>,
    auth_method: Option<AuthMethod>,
    status: Option<HostStatus>,
    profile_id: Option<String>,
    tags: Option<Vec<String>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Profile {
    id: String,
    name: String,
    description: String,
    model: String,
    approval_policy: String,
    sandbox_mode: String,
    updated_at: String,
    host_ids: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillPack {
    id: String,
    name: String,
    version: String,
    description: String,
    source: String,
    skill_count: u16,
    enabled: bool,
    updated_at: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "lowercase")]
enum TaskStatus {
    Queued,
    Running,
    Success,
    Failed,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "lowercase")]
enum TaskLogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskLog {
    id: String,
    task_run_id: String,
    level: TaskLogLevel,
    timestamp: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timed_out: Option<bool>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskRun {
    id: String,
    host_id: String,
    host_name: String,
    action: String,
    status: TaskStatus,
    started_at: String,
    ended_at: Option<String>,
    summary: String,
    logs: Vec<TaskLog>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionTest {
    ok: bool,
    latency_ms: Option<u64>,
    message: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SshCheckResult {
    host_alias: String,
    ok: bool,
    latency_ms: Option<u64>,
    message: String,
    task: TaskRun,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SshBootstrapResult {
    host_alias: String,
    ok: bool,
    latency_ms: Option<u64>,
    message: String,
    generated_key: bool,
    private_key_path: String,
    public_key_path: String,
    write_result: ssh::SshConfigWriteResult,
    task: TaskRun,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SshBootstrapProgressEvent {
    request_id: String,
    host_alias: String,
    step: String,
    status: String,
    message: String,
    detail: Option<String>,
    stdout: Option<String>,
    stderr: Option<String>,
    exit_code: Option<i32>,
    duration_ms: Option<u64>,
    timed_out: Option<bool>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteProbeResult {
    host_alias: String,
    ssh_status: HostStatus,
    os: String,
    arch: String,
    shell: String,
    path: Option<String>,
    path_has_local_bin: bool,
    codex_installed: bool,
    codex_path: Option<String>,
    codex_version: String,
    config_exists: bool,
    skills_exists: bool,
    skills_count: u16,
    task: TaskRun,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ThemeChoice {
    System,
    Light,
    Dark,
}

#[derive(Clone, Serialize, Deserialize)]
enum FontPreset {
    #[serde(
        rename = "english",
        alias = "system",
        alias = "chinese",
        alias = "cross-platform"
    )]
    English,
    #[serde(rename = "zh-cn")]
    ZhCn,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    theme: ThemeChoice,
    font_preset: FontPreset,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::System,
            font_preset: FontPreset::English,
        }
    }
}

struct AppState {
    hosts: Mutex<Vec<Host>>,
    profiles: Vec<Profile>,
    skill_packs: Vec<SkillPack>,
    tasks: Mutex<Vec<TaskRun>>,
}

const CODEX_RESOLVER_SCRIPT: &str = r#"best_path=""
best_version=""
best_key=""
seen=""

version_numbers() {
  case "$1" in
    codex*|Codex*|[0-9]*|v[0-9]*) ;;
    *) return 0 ;;
  esac
  printf '%s\n' "$1" | sed -n 's/^[^0-9]*\([0-9][0-9]*\)\.\([0-9][0-9]*\)\.\([0-9][0-9]*\).*/\1 \2 \3/p' | head -n 1
}

package_version_for_candidate() {
  candidate="$1"
  target=$(readlink -f "$candidate" 2>/dev/null || printf '%s\n' "$candidate")
  dir=${target%/*}
  depth=0
  while [ -n "$dir" ] && [ "$dir" != "/" ] && [ "$depth" -lt 6 ]; do
    package_json="$dir/package.json"
    if [ -f "$package_json" ]; then
      package_name=$(sed -n 's/^[[:space:]]*"name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$package_json" | head -n 1)
      package_version=$(sed -n 's/^[[:space:]]*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$package_json" | head -n 1)
      if [ "$package_name" = "@openai/codex" ] && [ -n "$package_version" ]; then
        printf 'codex-cli %s\n' "$package_version"
        return 0
      fi
    fi
    next_dir=${dir%/*}
    if [ "$next_dir" = "$dir" ]; then
      break
    fi
    dir="$next_dir"
    depth=$((depth + 1))
  done
  return 1
}

probe_candidate() {
  candidate="$1"
  if [ -z "$candidate" ] || [ ! -x "$candidate" ]; then
    return
  fi
  case ":$seen:" in
    *":$candidate:"*) return ;;
  esac
  seen="$seen:$candidate"

  version=$("$candidate" --version </dev/null 2>&1 | head -n 1)
  version_source="command"
  numbers=$(version_numbers "$version")

  if [ -z "$numbers" ]; then
    package_version=$(package_version_for_candidate "$candidate" 2>/dev/null || true)
    package_numbers=$(version_numbers "$package_version")
    if [ -n "$package_numbers" ]; then
      printf 'candidate %s -> %s (package metadata; --version: %s)\n' "$candidate" "$package_version" "${version:-no version output}" >&2
      version="$package_version"
      version_source="package metadata"
      numbers="$package_numbers"
    else
      printf 'candidate %s -> %s\n' "$candidate" "${version:-unversioned}" >&2
      return
    fi
  else
    printf 'candidate %s -> %s\n' "$candidate" "$version" >&2
  fi

  old_numbers_ifs="$IFS"
  IFS=' '
  set -- $numbers
  IFS="$old_numbers_ifs"
  key=$(printf '%06d%06d%06d' "$1" "$2" "$3")
  if [ -z "$best_key" ] || [ "$key" \> "$best_key" ]; then
    best_key="$key"
    best_path="$candidate"
    best_version="$version"
    best_version_source="$version_source"
  fi
}

probe_path_list() {
  old_ifs="$IFS"
  IFS=:
  for dir in $1; do
    probe_candidate "$dir/codex"
  done
  IFS="$old_ifs"
}

probe_path_list "$PATH"

login_shell="${SHELL:-}"
if [ -n "$login_shell" ] && [ -x "$login_shell" ]; then
  login_path=$("$login_shell" -lc 'printf "%s" "$PATH"' 2>/dev/null || true)
  if [ -n "$login_path" ]; then
    probe_path_list "$login_path"
  fi
  login_codex=$("$login_shell" -lc 'command -v codex 2>/dev/null' 2>/dev/null | head -n 1 || true)
  case "$login_codex" in
    /*) probe_candidate "$login_codex" ;;
  esac
fi

for candidate in \
  "$HOME/.local/bin/codex" \
  "$HOME/.npm-global/bin/codex" \
  "$HOME/.npm-packages/bin/codex" \
  "$HOME/.volta/bin/codex" \
  "$HOME/.bun/bin/codex" \
  "$HOME/.cargo/bin/codex" \
  "$HOME/.local/share/pnpm/codex" \
  "$HOME/.asdf/shims/codex" \
  "$HOME/.local/share/mise/shims/codex" \
  "$HOME/node_modules/.bin/codex" \
  "/usr/local/bin/codex" \
  "/usr/bin/codex" \
  "/snap/bin/codex" \
  "/opt/homebrew/bin/codex" \
  "/home/linuxbrew/.linuxbrew/bin/codex"
do
  probe_candidate "$candidate"
done

for candidate in \
  "$HOME/.nvm/versions/node"/*/bin/codex \
  "$HOME/.fnm/node-versions"/*/installation/bin/codex \
  "$HOME/.local/share/fnm/node-versions"/*/installation/bin/codex \
  "$HOME/.local/share/mise/installs/node"/*/bin/codex \
  "$HOME/.local/share/pnpm/global"/*/node_modules/.bin/codex
do
  probe_candidate "$candidate"
done

if [ -z "$best_path" ]; then
  exit 127
fi
printf 'selected %s -> %s (%s)\n' "$best_path" "$best_version" "$best_version_source" >&2"#;

fn codex_path_probe_script() -> String {
    format!("{CODEX_RESOLVER_SCRIPT}\nprintf '%s\\n' \"$best_path\"")
}

fn codex_version_probe_script() -> String {
    format!("{CODEX_RESOLVER_SCRIPT}\nprintf '%s\\n' \"$best_version\"")
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            hosts: Mutex::new(mock_hosts()),
            profiles: mock_profiles(),
            skill_packs: mock_skill_packs(),
            tasks: Mutex::new(mock_tasks()),
        }
    }
}

#[tauri::command]
fn app_health() -> Health {
    Health {
        app: "CodexHub",
        mode: "tauri",
        remote_wrapper_required: false,
    }
}

#[tauri::command]
fn get_settings(app: AppHandle) -> AppSettings {
    read_settings(&app)
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: AppSettings) -> Result<AppSettings, String> {
    write_settings(&app, &settings)?;
    Ok(settings)
}

#[tauri::command]
fn get_ssh_status() -> Result<ssh::SshStatus, String> {
    ssh::get_ssh_status()
}

#[tauri::command]
fn generate_ed25519_key() -> Result<ssh::SshKeyGenerationResult, String> {
    ssh::generate_ed25519_key()
}

#[tauri::command]
fn list_ssh_config_hosts() -> Result<Vec<ssh::SshConfigHost>, String> {
    ssh::list_ssh_config_hosts()
}

#[tauri::command]
fn upsert_ssh_config_host(draft: ssh::SshHostDraft) -> Result<ssh::SshConfigWriteResult, String> {
    ssh::upsert_ssh_config_host(draft)
}

#[tauri::command]
fn delete_ssh_config_host(alias: String) -> Result<ssh::SshConfigWriteResult, String> {
    ssh::delete_ssh_config_host(alias)
}

#[tauri::command]
fn list_hosts(state: State<'_, AppState>) -> Vec<Host> {
    let _ = merge_discovered_hosts(&state);
    state.hosts.lock().expect("hosts mutex poisoned").clone()
}

#[tauri::command]
fn refresh_discovered_hosts(state: State<'_, AppState>) -> Result<Vec<Host>, String> {
    merge_discovered_hosts(&state)?;
    Ok(state.hosts.lock().expect("hosts mutex poisoned").clone())
}

#[tauri::command]
fn add_host(state: State<'_, AppState>, draft: HostDraft) -> Host {
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
        codex_installed: false,
        codex_version: "pending".into(),
        config_exists: None,
        skills_exists: None,
        skills_count: None,
        profile_id: None,
        skill_pack_ids: Vec::new(),
        tags: draft.tags,
        last_seen: "just added".into(),
        latency_ms: None,
    };

    state
        .hosts
        .lock()
        .expect("hosts mutex poisoned")
        .insert(0, host.clone());
    host
}

#[tauri::command]
fn update_host(state: State<'_, AppState>, id: String, patch: HostPatch) -> Host {
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");

    if let Some(host) = hosts.iter_mut().find(|host| host.id == id) {
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
        return host.clone();
    }

    let host = Host {
        id,
        name: patch.name.unwrap_or_else(|| "Mock Host".into()),
        host_alias: patch.address.clone().unwrap_or_else(|| "127.0.0.1".into()),
        source: "manual".into(),
        address: patch.address.unwrap_or_else(|| "127.0.0.1".into()),
        port: patch.port.unwrap_or(22),
        username: patch.username.unwrap_or_else(|| "codex".into()),
        auth_method: patch.auth_method.unwrap_or(AuthMethod::SshKey),
        status: patch.status.unwrap_or(HostStatus::Unknown),
        os: "Unknown".into(),
        arch: "Unknown".into(),
        shell: "Unknown".into(),
        path: None,
        path_has_local_bin: None,
        codex_installed: false,
        codex_version: "pending".into(),
        config_exists: None,
        skills_exists: None,
        skills_count: None,
        profile_id: patch.profile_id,
        skill_pack_ids: Vec::new(),
        tags: patch.tags.unwrap_or_default(),
        last_seen: "just added".into(),
        latency_ms: None,
    };
    hosts.insert(0, host.clone());
    host
}

#[tauri::command]
fn delete_host(state: State<'_, AppState>, id: String) -> bool {
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");
    let before = hosts.len();
    hosts.retain(|host| host.id != id);
    hosts.len() != before
}

#[tauri::command]
fn test_ssh_connection(state: State<'_, AppState>, id: String) -> ConnectionTest {
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");

    if let Some(host) = hosts.iter_mut().find(|host| host.id == id) {
        let ok = host.id != "linux-runner";
        host.status = if ok {
            HostStatus::Online
        } else {
            HostStatus::Offline
        };
        host.latency_ms = if ok { Some(24) } else { None };
        if ok {
            host.last_seen = "just now".into();
        }

        return ConnectionTest {
            ok,
            latency_ms: host.latency_ms,
            message: if ok {
                format!("Mock SSH handshake to {} completed.", host.name)
            } else {
                format!("Mock SSH handshake to {} timed out.", host.name)
            },
        };
    }

    ConnectionTest {
        ok: false,
        latency_ms: None,
        message: "Host not found.".into(),
    }
}

#[tauri::command]
fn ssh_check(
    state: State<'_, AppState>,
    host_alias: String,
    timeout_ms: Option<u64>,
) -> SshCheckResult {
    run_ssh_check(&state, host_alias, timeout_ms)
}

#[tauri::command]
fn bootstrap_ssh_host(
    app: AppHandle,
    state: State<'_, AppState>,
    draft: ssh::SshHostDraft,
    password: String,
    timeout_ms: Option<u64>,
    request_id: Option<String>,
) -> Result<SshBootstrapResult, String> {
    run_ssh_bootstrap(&app, &state, draft, password, timeout_ms, request_id)
}

#[tauri::command]
fn bootstrap_existing_ssh_host(
    state: State<'_, AppState>,
    host_alias: String,
    password: String,
    timeout_ms: Option<u64>,
) -> Result<SshBootstrapResult, String> {
    run_existing_ssh_bootstrap(&state, host_alias, password, timeout_ms)
}

#[tauri::command]
fn remote_probe_codex(
    state: State<'_, AppState>,
    host_alias: String,
    timeout_ms: Option<u64>,
) -> RemoteProbeResult {
    run_remote_probe(&state, host_alias, timeout_ms)
}

#[tauri::command]
fn list_profiles(state: State<'_, AppState>) -> Vec<Profile> {
    state.profiles.clone()
}

#[tauri::command]
fn list_skill_packs(state: State<'_, AppState>) -> Vec<SkillPack> {
    state.skill_packs.clone()
}

#[tauri::command]
fn apply_profile(state: State<'_, AppState>, profile_id: String, host_ids: Vec<String>) -> TaskRun {
    let profile_name = state
        .profiles
        .iter()
        .find(|profile| profile.id == profile_id)
        .map(|profile| profile.name.clone())
        .unwrap_or_else(|| profile_id.clone());

    let hosts = state.hosts.lock().expect("hosts mutex poisoned");
    let first_host_id = host_ids
        .first()
        .cloned()
        .unwrap_or_else(|| "no-host".into());
    let host_name = hosts
        .iter()
        .find(|host| host.id == first_host_id)
        .map(|host| host.name.clone())
        .unwrap_or_else(|| "No host selected".into());
    drop(hosts);

    let task_id = format!("task-{}", timestamp_millis());
    let task = TaskRun {
        id: task_id.clone(),
        host_id: first_host_id,
        host_name: host_name.clone(),
        action: "Apply profile".into(),
        status: TaskStatus::Success,
        started_at: "now".into(),
        ended_at: Some("now".into()),
        summary: format!(
            "{} applied to {} through mock backend.",
            profile_name, host_name
        ),
        logs: vec![
            basic_log(
                &task_id,
                1,
                TaskLogLevel::Info,
                "Reserved apply_profile command accepted host selection.",
            ),
            basic_log(
                &task_id,
                2,
                TaskLogLevel::Info,
                "No remote files were changed; this is mock data only.",
            ),
        ],
    };

    state
        .tasks
        .lock()
        .expect("tasks mutex poisoned")
        .insert(0, task.clone());
    task
}

#[tauri::command]
fn list_tasks(state: State<'_, AppState>) -> Vec<TaskRun> {
    state.tasks.lock().expect("tasks mutex poisoned").clone()
}

fn merge_discovered_hosts(state: &AppState) -> Result<(), String> {
    let discovered_hosts = ssh::list_ssh_config_hosts()?;
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");
    for discovered in discovered_hosts {
        merge_discovered_host(&mut hosts, discovered);
    }
    Ok(())
}

fn merge_discovered_host(hosts: &mut Vec<Host>, discovered: ssh::SshConfigHost) {
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
            codex_installed: false,
            codex_version: "pending".into(),
            config_exists: None,
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

fn host_address(host: &ssh::SshConfigHost) -> String {
    if host.host_name.is_empty() {
        host.alias.clone()
    } else {
        host.host_name.clone()
    }
}

fn discovered_host_id(alias: &str) -> String {
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

fn ensure_tag(tags: &mut Vec<String>, tag: &str) {
    if !tags.iter().any(|item| item == tag) {
        tags.push(tag.to_string());
    }
}

fn run_ssh_check(state: &AppState, host_alias: String, timeout_ms: Option<u64>) -> SshCheckResult {
    let alias_result = ssh::validate_ssh_alias(&host_alias);
    let alias = alias_result
        .clone()
        .unwrap_or_else(|_| host_alias.trim().to_string());
    let timeout = ssh::normalize_timeout_ms(timeout_ms);
    let task_id = format!("task-ssh-{}", timestamp_millis());
    let host_name = host_name_for_alias(state, &alias);
    let host_id = host_id_for_alias(state, &alias);
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
        started_at: "now".into(),
        ended_at: Some("now".into()),
        summary: message.clone(),
        logs: vec![command_log(
            &task_id,
            1,
            if ok {
                TaskLogLevel::Info
            } else {
                TaskLogLevel::Error
            },
            &message,
            &output,
        )],
    };

    update_host_check(state, &alias, ok, output.duration_ms);
    record_task(state, task.clone());

    SshCheckResult {
        host_alias: alias,
        ok,
        latency_ms: if ok { Some(output.duration_ms) } else { None },
        message,
        task,
    }
}

fn run_ssh_bootstrap(
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

fn run_existing_ssh_bootstrap(
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
fn run_bootstrap_for_config_host(
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
    let mut logs = Vec::new();
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
            record_task(state, task.clone());
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
            record_task(state, task.clone());
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
        record_task(state, task.clone());
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
                record_task(state, task.clone());
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
        let _ = merge_discovered_hosts(state);
    }
    update_host_check(state, &alias, ok, check_output.duration_ms);
    record_task(state, task.clone());

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
fn pending_ssh_config_write_result(
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

fn emit_bootstrap_progress(
    app: &AppHandle,
    request_id: &str,
    host_alias: &str,
    step: &str,
    status: &str,
    message: &str,
    output: Option<&ssh::SshCommandOutput>,
) {
    let payload = SshBootstrapProgressEvent {
        request_id: request_id.to_string(),
        host_alias: host_alias.to_string(),
        step: step.to_string(),
        status: status.to_string(),
        message: message.to_string(),
        detail: output.map(command_detail),
        stdout: output.map(|item| item.stdout.clone()),
        stderr: output.map(|item| item.stderr.clone()),
        exit_code: output.and_then(|item| item.exit_code),
        duration_ms: output.map(|item| item.duration_ms),
        timed_out: output.map(|item| item.timed_out),
    };
    let _ = app.emit("ssh-bootstrap-progress", payload);
}

fn bootstrap_progress_step(step: ssh::PasswordBootstrapStep) -> &'static str {
    match step {
        ssh::PasswordBootstrapStep::PasswordLogin => "password_login",
        ssh::PasswordBootstrapStep::InstallPublicKey => "install_public_key",
        ssh::PasswordBootstrapStep::SetPermissions => "set_permissions",
    }
}

fn bootstrap_step_running_message(step: ssh::PasswordBootstrapStep) -> &'static str {
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

fn bootstrap_step_message(
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

fn bootstrap_task(
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
        started_at: "now".into(),
        ended_at: Some("now".into()),
        summary: summary.to_string(),
        logs,
    }
}

fn run_remote_probe(
    state: &AppState,
    host_alias: String,
    timeout_ms: Option<u64>,
) -> RemoteProbeResult {
    let timeout = ssh::normalize_timeout_ms(timeout_ms);
    let alias_result = ssh::validate_ssh_alias(&host_alias);
    let alias = alias_result
        .clone()
        .unwrap_or_else(|_| host_alias.trim().to_string());
    let task_id = format!("task-probe-{}", timestamp_millis());
    let host_name = host_name_for_alias(state, &alias);
    let host_id = host_id_for_alias(state, &alias);
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
    let mut logs = vec![command_log(
        &task_id,
        1,
        if check_ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        &check_message,
        &check_output,
    )];

    if !check_ok {
        update_host_check(state, &alias, false, check_output.duration_ms);
        let task = TaskRun {
            id: task_id.clone(),
            host_id,
            host_name,
            action: "Probe remote system".into(),
            status: TaskStatus::Failed,
            started_at: "now".into(),
            ended_at: Some("now".into()),
            summary: format!("Remote probe skipped because SSH check failed: {check_message}"),
            logs,
        };
        record_task(state, task.clone());
        return RemoteProbeResult {
            host_alias: alias,
            ssh_status: HostStatus::Offline,
            os: "Unknown".into(),
            arch: "Unknown".into(),
            shell: "Unknown".into(),
            path: None,
            path_has_local_bin: false,
            codex_installed: false,
            codex_path: None,
            codex_version: "not installed".into(),
            config_exists: false,
            skills_exists: false,
            skills_count: 0,
            task,
        };
    }
    update_host_check(state, &alias, true, check_output.duration_ms);

    let codex_path_probe_script = codex_path_probe_script();
    let codex_version_probe_script = codex_version_probe_script();
    let commands = vec![
        ("uname -s", "uname -s", TaskLogLevel::Info),
        ("uname -m", "uname -m", TaskLogLevel::Info),
        (
            "echo $SHELL",
            "printf '%s\n' \"${SHELL:-$(getent passwd \"$(id -un)\" 2>/dev/null | cut -d: -f7)}\"",
            TaskLogLevel::Info,
        ),
        ("resolve codex", codex_path_probe_script.as_str(), TaskLogLevel::Warn),
        ("codex --version", codex_version_probe_script.as_str(), TaskLogLevel::Warn),
        ("echo $PATH", "printf '%s\n' \"$PATH\"", TaskLogLevel::Info),
        (
            "check ~/.codex/config.toml",
            "test -f \"$HOME/.codex/config.toml\" && printf yes || printf no",
            TaskLogLevel::Info,
        ),
        (
            "check ~/.codex/skills",
            "test -d \"$HOME/.codex/skills\" && printf yes || printf no",
            TaskLogLevel::Info,
        ),
        (
            "count ~/.codex/skills",
            "if [ -d \"$HOME/.codex/skills\" ]; then find \"$HOME/.codex/skills\" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | wc -l; else printf 0; fi",
            TaskLogLevel::Info,
        ),
    ];

    let mut outputs = Vec::new();
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
        logs.push(command_log(&task_id, index + 2, level, &message, &output));
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
    let codex_version = if codex_installed {
        outputs
            .get(4)
            .filter(|output| output.success())
            .map(|output| output.stdout.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "unavailable".into())
    } else {
        "not installed".into()
    };
    let path = outputs
        .get(5)
        .filter(|output| output.success())
        .map(|output| output.stdout.trim().to_string())
        .filter(|value| !value.is_empty());
    let path_has_local_bin = path_has_local_bin(path.as_deref());
    let config_exists = stdout_yes(outputs.get(6));
    let skills_exists = stdout_yes(outputs.get(7));
    let skills_count = outputs
        .get(8)
        .and_then(|output| output.stdout.trim().parse::<u16>().ok())
        .unwrap_or(0);
    let summary = format!(
        "Probe completed for {alias}: {os}/{arch}, Codex {}.",
        if codex_installed {
            codex_version.as_str()
        } else {
            "not installed"
        }
    );
    let task = TaskRun {
        id: task_id.clone(),
        host_id,
        host_name,
        action: "Probe remote system".into(),
        status: TaskStatus::Success,
        started_at: "now".into(),
        ended_at: Some("now".into()),
        summary: summary.clone(),
        logs,
    };

    update_host_probe(
        state,
        &alias,
        &os,
        &arch,
        &shell,
        path.clone(),
        path_has_local_bin,
        codex_installed,
        &codex_version,
        config_exists,
        skills_exists,
        skills_count,
    );
    record_task(state, task.clone());

    RemoteProbeResult {
        host_alias: alias,
        ssh_status: HostStatus::Online,
        os,
        arch,
        shell,
        path,
        path_has_local_bin,
        codex_installed,
        codex_path,
        codex_version,
        config_exists,
        skills_exists,
        skills_count,
        task,
    }
}

fn record_task(state: &AppState, task: TaskRun) {
    state
        .tasks
        .lock()
        .expect("tasks mutex poisoned")
        .insert(0, task);
}

fn update_host_check(state: &AppState, alias: &str, ok: bool, duration_ms: u64) {
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
fn update_host_probe(
    state: &AppState,
    alias: &str,
    os: &str,
    arch: &str,
    shell: &str,
    path: Option<String>,
    path_has_local_bin: bool,
    codex_installed: bool,
    codex_version: &str,
    config_exists: bool,
    skills_exists: bool,
    skills_count: u16,
) {
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");
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
        host.codex_installed = codex_installed;
        host.codex_version = codex_version.to_string();
        host.config_exists = Some(config_exists);
        host.skills_exists = Some(skills_exists);
        host.skills_count = Some(skills_count);
        host.last_seen = "just now".into();
    }
}

fn host_name_for_alias(state: &AppState, alias: &str) -> String {
    state
        .hosts
        .lock()
        .expect("hosts mutex poisoned")
        .iter()
        .find(|host| host.host_alias.eq_ignore_ascii_case(alias))
        .map(|host| host.name.clone())
        .unwrap_or_else(|| alias.to_string())
}

fn host_id_for_alias(state: &AppState, alias: &str) -> String {
    state
        .hosts
        .lock()
        .expect("hosts mutex poisoned")
        .iter()
        .find(|host| host.host_alias.eq_ignore_ascii_case(alias))
        .map(|host| host.id.clone())
        .unwrap_or_else(|| discovered_host_id(alias))
}

fn failed_command_output(command: String, message: String) -> ssh::SshCommandOutput {
    ssh::SshCommandOutput {
        command,
        stdout: String::new(),
        stderr: message,
        exit_code: None,
        duration_ms: 0,
        timed_out: false,
    }
}

fn command_log(
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

fn basic_log(task_id: &str, index: usize, level: TaskLogLevel, message: &str) -> TaskLog {
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

fn ssh_check_message(
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

fn ssh_failure_hint(output: &ssh::SshCommandOutput) -> String {
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

fn command_detail(output: &ssh::SshCommandOutput) -> String {
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

fn stdout_or_unknown(output: Option<&ssh::SshCommandOutput>) -> String {
    output
        .filter(|item| item.success())
        .map(|item| item.stdout.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Unknown".into())
}

fn stdout_yes(output: Option<&ssh::SshCommandOutput>) -> bool {
    output
        .filter(|item| item.success())
        .map(|item| item.stdout.trim().eq_ignore_ascii_case("yes"))
        .unwrap_or(false)
}

fn path_has_local_bin(path: Option<&str>) -> bool {
    path.unwrap_or_default()
        .split(':')
        .any(|segment| segment == "~/.local/bin" || segment.ends_with("/.local/bin"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_resolver_checks_login_paths_and_package_metadata() {
        let path_script = codex_path_probe_script();
        let version_script = codex_version_probe_script();

        assert!(path_script.contains("package_version_for_candidate"));
        assert!(path_script.contains("$login_shell\" -lc"));
        assert!(path_script.contains("\"$HOME/.nvm/versions/node\"/*/bin/codex"));
        assert!(path_script.contains("\"$HOME/.local/share/pnpm/codex\""));
        assert!(path_script.contains("\"$HOME/node_modules/.bin/codex\""));
        assert!(path_script.contains("</dev/null"));
        assert!(version_script.ends_with("printf '%s\\n' \"$best_version\""));
    }

    #[test]
    fn ssh_failure_hint_explains_host_key_and_password_cases() {
        let host_key_output = ssh::SshCommandOutput {
            command: "ssh lab echo ok".into(),
            stdout: String::new(),
            stderr: "Host key verification failed.".into(),
            exit_code: Some(255),
            duration_ms: 10,
            timed_out: false,
        };
        let password_output = ssh::SshCommandOutput {
            command: "ssh lab echo ok".into(),
            stdout: String::new(),
            stderr: "Permission denied (publickey,password).".into(),
            exit_code: Some(255),
            duration_ms: 10,
            timed_out: false,
        };

        assert!(ssh_failure_hint(&host_key_output).contains("first-time new host keys"));
        assert!(ssh_failure_hint(&password_output).contains("one-time password setup"));
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            app_health,
            get_settings,
            save_settings,
            get_ssh_status,
            generate_ed25519_key,
            list_ssh_config_hosts,
            upsert_ssh_config_host,
            delete_ssh_config_host,
            list_hosts,
            refresh_discovered_hosts,
            add_host,
            update_host,
            delete_host,
            test_ssh_connection,
            ssh_check,
            bootstrap_ssh_host,
            bootstrap_existing_ssh_host,
            remote_probe_codex,
            list_profiles,
            apply_profile,
            list_tasks,
            list_skill_packs
        ])
        .run(tauri::generate_context!())
        .expect("error while running CodexHub");
}

fn timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn settings_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .unwrap_or_else(|_| PathBuf::from(".codexhub"))
        .join("settings.json")
}

fn read_settings(app: &AppHandle) -> AppSettings {
    let path = settings_path(app);
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<AppSettings>(&content).ok())
        .unwrap_or_default()
}

fn write_settings(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let path = settings_path(app);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let content = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
    fs::write(path, content).map_err(|error| error.to_string())
}

fn mock_hosts() -> Vec<Host> {
    vec![
        Host {
            id: "mac-studio-lab".into(),
            name: "Mac Studio Lab".into(),
            host_alias: "mac-studio-lab".into(),
            source: "mock".into(),
            address: "10.0.8.12".into(),
            port: 22,
            username: "jurio".into(),
            auth_method: AuthMethod::SshKey,
            status: HostStatus::Online,
            os: "macOS 15.5".into(),
            arch: "arm64".into(),
            shell: "/bin/zsh".into(),
            path: Some("/Users/jurio/.local/bin:/usr/local/bin:/usr/bin".into()),
            path_has_local_bin: Some(true),
            codex_installed: true,
            codex_version: "0.32.0".into(),
            config_exists: Some(true),
            skills_exists: Some(true),
            skills_count: Some(5),
            profile_id: Some("research-default".into()),
            skill_pack_ids: vec!["paper-review".into(), "tauri-builder".into()],
            tags: vec!["local".into(), "gpu".into()],
            last_seen: "2 min ago".into(),
            latency_ms: Some(18),
        },
        Host {
            id: "win-workstation".into(),
            name: "Windows Workstation".into(),
            host_alias: "win-workstation".into(),
            source: "mock".into(),
            address: "192.168.31.42".into(),
            port: 22,
            username: "pc".into(),
            auth_method: AuthMethod::Agent,
            status: HostStatus::Unknown,
            os: "Windows 11 Pro".into(),
            arch: "x86_64".into(),
            shell: "Unknown".into(),
            path: None,
            path_has_local_bin: None,
            codex_installed: false,
            codex_version: "pending".into(),
            config_exists: None,
            skills_exists: None,
            skills_count: None,
            profile_id: Some("safe-editing".into()),
            skill_pack_ids: vec!["tauri-builder".into()],
            tags: vec!["desktop".into(), "primary".into()],
            last_seen: "not tested".into(),
            latency_ms: None,
        },
        Host {
            id: "linux-runner".into(),
            name: "Linux Runner".into(),
            host_alias: "linux-runner".into(),
            source: "mock".into(),
            address: "172.20.4.8".into(),
            port: 2222,
            username: "codex".into(),
            auth_method: AuthMethod::SshKey,
            status: HostStatus::Offline,
            os: "Ubuntu 24.04 LTS".into(),
            arch: "x86_64".into(),
            shell: "/bin/bash".into(),
            path: Some("/home/codex/.local/bin:/usr/local/bin:/usr/bin".into()),
            path_has_local_bin: Some(true),
            codex_installed: true,
            codex_version: "0.31.1".into(),
            config_exists: Some(false),
            skills_exists: Some(true),
            skills_count: Some(1),
            profile_id: None,
            skill_pack_ids: vec!["paper-review".into()],
            tags: vec!["remote".into(), "ci".into()],
            last_seen: "yesterday".into(),
            latency_ms: None,
        },
    ]
}

fn mock_profiles() -> Vec<Profile> {
    vec![
        Profile {
            id: "research-default".into(),
            name: "Research Default".into(),
            description: "Balanced model and approval policy for literature review, repo browsing, and report drafting.".into(),
            model: "gpt-5-codex".into(),
            approval_policy: "on-request".into(),
            sandbox_mode: "workspace-write".into(),
            updated_at: "2026-06-24 22:10".into(),
            host_ids: vec!["mac-studio-lab".into()],
        },
        Profile {
            id: "safe-editing".into(),
            name: "Safe Editing".into(),
            description: "Conservative profile for protected repos, narrow write scope, and explicit publish steps.".into(),
            model: "gpt-5-codex".into(),
            approval_policy: "on-failure".into(),
            sandbox_mode: "workspace-write".into(),
            updated_at: "2026-06-23 18:35".into(),
            host_ids: vec!["win-workstation".into()],
        },
        Profile {
            id: "diagnostics".into(),
            name: "Diagnostics".into(),
            description: "Read-mostly profile for host checks, logs, and environment inspection.".into(),
            model: "gpt-5-mini".into(),
            approval_policy: "never".into(),
            sandbox_mode: "read-only".into(),
            updated_at: "2026-06-21 09:42".into(),
            host_ids: Vec::new(),
        },
    ]
}

fn mock_skill_packs() -> Vec<SkillPack> {
    vec![
        SkillPack {
            id: "paper-review".into(),
            name: "Paper Review".into(),
            version: "0.4.1".into(),
            description: "Summarize papers, extract claims, and prepare structured reading notes."
                .into(),
            source: "~/.codex/skills/paper-review".into(),
            skill_count: 5,
            enabled: true,
            updated_at: "2026-06-24".into(),
        },
        SkillPack {
            id: "tauri-builder".into(),
            name: "Tauri Builder".into(),
            version: "0.2.0".into(),
            description:
                "Scaffold, test, and package Tauri desktop features with React and Rust boundaries."
                    .into(),
            source: "./skills/tauri-builder".into(),
            skill_count: 3,
            enabled: true,
            updated_at: "2026-06-20".into(),
        },
        SkillPack {
            id: "windows-diagnostics".into(),
            name: "Windows Diagnostics".into(),
            version: "0.1.5".into(),
            description:
                "Collect reproducible PowerShell checks for network, shell, and toolchain issues."
                    .into(),
            source: "./skills/windows-diagnostics".into(),
            skill_count: 4,
            enabled: false,
            updated_at: "2026-06-18".into(),
        },
    ]
}

fn mock_tasks() -> Vec<TaskRun> {
    vec![
        TaskRun {
            id: "task-1042".into(),
            host_id: "mac-studio-lab".into(),
            host_name: "Mac Studio Lab".into(),
            action: "Apply profile".into(),
            status: TaskStatus::Success,
            started_at: "2026-06-25 09:14".into(),
            ended_at: Some("2026-06-25 09:15".into()),
            summary:
                "Research Default rendered to ~/.codex/config.toml with backup codexhub-1042.toml."
                    .into(),
            logs: vec![
                basic_log(
                    "task-1042",
                    1,
                    TaskLogLevel::Info,
                    "Opened SFTP session and created remote backup.",
                ),
                basic_log(
                    "task-1042",
                    2,
                    TaskLogLevel::Info,
                    "Rendered profile preview matched expected TOML sections.",
                ),
            ],
        },
        TaskRun {
            id: "task-1039".into(),
            host_id: "linux-runner".into(),
            host_name: "Linux Runner".into(),
            action: "Test SSH connection".into(),
            status: TaskStatus::Failed,
            started_at: "2026-06-24 22:02".into(),
            ended_at: Some("2026-06-24 22:02".into()),
            summary:
                "Connection timed out. Check VPN route or host firewall before applying profiles."
                    .into(),
            logs: vec![
                basic_log(
                    "task-1039",
                    1,
                    TaskLogLevel::Warn,
                    "Mock check marks linux-runner offline for UI validation.",
                ),
                basic_log(
                    "task-1039",
                    2,
                    TaskLogLevel::Error,
                    "Connection timeout is simulated; no SSH socket was opened.",
                ),
            ],
        },
        TaskRun {
            id: "task-1035".into(),
            host_id: "win-workstation".into(),
            host_name: "Windows Workstation".into(),
            action: "Sync skill pack".into(),
            status: TaskStatus::Queued,
            started_at: "2026-06-24 18:25".into(),
            ended_at: None,
            summary: "Queued Paper Review skill pack for the next available SSH session.".into(),
            logs: vec![basic_log(
                "task-1035",
                1,
                TaskLogLevel::Info,
                "Task created from mock backend reservation.",
            )],
        },
        TaskRun {
            id: "task-1031".into(),
            host_id: "win-workstation".into(),
            host_name: "Windows Workstation".into(),
            action: "Preview profile".into(),
            status: TaskStatus::Running,
            started_at: "2026-06-24 18:10".into(),
            ended_at: None,
            summary: "Generating a profile diff preview for the safe editing profile.".into(),
            logs: vec![basic_log(
                "task-1031",
                1,
                TaskLogLevel::Info,
                "Mock worker is holding this run in progress for UI coverage.",
            )],
        },
    ]
}
