mod hosts;
mod platform;
mod profiles;
mod resource_monitor;
mod settings;
mod skills;
mod ssh;
mod tasks;
mod updater;

use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, Duration as ChronoDuration, FixedOffset, Local, TimeZone};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use hosts::{load_hosts, save_current_hosts, save_hosts};
use profiles::{load_profiles, save_profiles};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};
use std::env;
use std::fmt::Display;
use std::fs;
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tar::{Archive as TarArchive, Builder as TarBuilder};
use tauri::{
    menu::MenuBuilder,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, Window, WindowEvent,
};
use tauri_plugin_updater::UpdaterExt;
use settings::{
    read_settings, write_settings, AppSettings, CloseButtonBehavior, NetworkProxyMode,
};
use skills::{load_skills, managed_skills_dir, save_skills, skill_clone_cache_dir};
use tasks::{TaskLog, TaskLogLevel, TaskRun, TaskStatus};
use toml::map::Map as TomlMap;
use toml::Value as TomlValue;
use updater::{AppUpdateState, AppUpdateStatus, GitHubReleaseResponse, StableUpdaterConfig};
use url::Url;

const CODEX_NPM_REGISTRY_URL: &str = "https://registry.npmjs.org/@openai/codex";
const CODEX_LATEST_SOURCE: &str = "npm";
const CODEX_LATEST_REFRESH_HOUR: u32 = 4;
const STABLE_UPDATE_ENDPOINT_ENV: &str = "CODEXHUB_STABLE_UPDATE_ENDPOINT";
const STABLE_UPDATER_PUBKEY_ENV: &str = "CODEXHUB_STABLE_UPDATER_PUBKEY";
const STABLE_IDENTIFIER: &str = "app.codexhub.desktop";
const DEV_IDENTIFIER: &str = "dev.codexhub.desktop";
const APP_UPDATE_CHECK_TIMEOUT_SECS: u64 = 30;
const GITHUB_API_ACCEPT: &str = "application/vnd.github+json";
const OCTET_STREAM_ACCEPT: &str = "application/octet-stream";
const MAIN_WINDOW_LABEL: &str = "main";
const CLOSE_BUTTON_BEHAVIOR_REQUESTED_EVENT: &str = "close-button-behavior-requested";
const TRAY_ID: &str = "codexhub-main-tray";
const TRAY_MENU_SHOW_ID: &str = "show_codexhub";
const TRAY_MENU_QUIT_ID: &str = "quit_codexhub";

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
    codex_command_available: Option<bool>,
    codex_installed: bool,
    codex_version: String,
    config_exists: Option<bool>,
    api_config_name: Option<String>,
    api_config_source: Option<String>,
    api_key_env_var: Option<String>,
    api_key_env_present: Option<bool>,
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

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Profile {
    id: String,
    name: String,
    description: String,
    model: String,
    provider: String,
    base_url: Option<String>,
    api_key_env_var: Option<String>,
    model_reasoning_effort: Option<String>,
    plan_mode_reasoning_effort: Option<String>,
    fast_mode: bool,
    service_tier: Option<String>,
    approval_policy: String,
    sandbox_mode: String,
    extra_toml: String,
    created_at: String,
    updated_at: String,
    source: String,
    credential_stored: bool,
    host_ids: Vec<String>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileDraft {
    name: String,
    description: Option<String>,
    model: String,
    provider: Option<String>,
    base_url: Option<String>,
    api_key_env_var: Option<String>,
    model_reasoning_effort: Option<String>,
    plan_mode_reasoning_effort: Option<String>,
    fast_mode: Option<bool>,
    service_tier: Option<String>,
    approval_policy: Option<String>,
    sandbox_mode: Option<String>,
    extra_toml: Option<String>,
    source: Option<String>,
    host_ids: Option<Vec<String>>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfilePatch {
    name: Option<String>,
    description: Option<String>,
    model: Option<String>,
    provider: Option<String>,
    base_url: Option<String>,
    api_key_env_var: Option<String>,
    model_reasoning_effort: Option<String>,
    plan_mode_reasoning_effort: Option<String>,
    fast_mode: Option<bool>,
    service_tier: Option<String>,
    approval_policy: Option<String>,
    sandbox_mode: Option<String>,
    extra_toml: Option<String>,
    source: Option<String>,
    credential_stored: Option<bool>,
    host_ids: Option<Vec<String>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileApplyPreview {
    profile_id: String,
    profile_name: String,
    rendered_toml: String,
    target_files: Vec<ProfileApplyTargetFile>,
    host_results: Vec<ProfileApplyHostResult>,
    warnings: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileApplyTargetFile {
    host_id: String,
    host_name: String,
    host_alias: String,
    path: String,
    backup_expected: bool,
    no_change_expected: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileApplyHostResult {
    host_id: String,
    host_name: String,
    host_alias: String,
    status: String,
    target_path: String,
    backup_path: Option<String>,
    message: String,
    task: Option<TaskRun>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileApplyBatchResult {
    profile_id: String,
    ok: bool,
    results: Vec<ProfileApplyHostResult>,
    tasks: Vec<TaskRun>,
    profiles: Vec<Profile>,
    hosts: Vec<Host>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileImportExport {
    schema_version: u16,
    exported_at: String,
    profiles: Vec<Profile>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileApiKeyResult {
    profile_id: String,
    exists: bool,
    api_key: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileImportResult {
    imported: Vec<Profile>,
    skipped: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DetectedCcSwitchProfile {
    source_path: String,
    profile: Profile,
    #[serde(skip_serializing)]
    api_key: Option<String>,
}

#[derive(Clone)]
struct CcSwitchProfileRecord {
    profile: Profile,
    api_key: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CcSwitchDetection {
    detected: bool,
    source_path: Option<String>,
    message: String,
    import_export: ProfileImportExport,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppliedProfileMetadata {
    profile_id: String,
    profile_name: String,
    applied_at: String,
    codexhub_version: String,
}

#[derive(Clone)]
struct RemoteApiConfigMatch {
    name: String,
    source: String,
    profile_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
enum SafeReconnectDecision {
    Terminate(u32),
    Manual(String),
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillPack {
    id: String,
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    about: String,
    source_type: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    original_path: Option<String>,
    #[serde(default)]
    managed_path: String,
    #[serde(default)]
    has_skill_md: bool,
    #[serde(default)]
    skill_count: u16,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    added_at: String,
    #[serde(default)]
    updated_at: String,
    #[serde(default)]
    applications: Vec<SkillApplication>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillApplication {
    target_type: String,
    label: String,
    host_alias: Option<String>,
    path: String,
    detected_at: String,
    has_skill_md: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillImportResult {
    imported: Vec<SkillPack>,
    skipped: Vec<String>,
    message: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteSkill {
    name: String,
    path: String,
    has_skill_md: bool,
    status: String,
    #[serde(default)]
    description: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteSkillListResult {
    host_alias: String,
    root_path: String,
    count: u16,
    valid_count: u16,
    invalid_count: u16,
    skills: Vec<RemoteSkill>,
    task: TaskRun,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RemoteSkillScope {
    User,
    Project,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum SkillConflictPolicy {
    Backup,
    Skip,
    Overwrite,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteSkillInstallResult {
    host_alias: String,
    ok: bool,
    skill_id: String,
    skill_name: String,
    scope: RemoteSkillScope,
    target_path: String,
    backup_path: Option<String>,
    skipped: bool,
    message: String,
    task: TaskRun,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteSkillDeleteResult {
    host_alias: String,
    ok: bool,
    skill_name: String,
    target_path: String,
    backup_path: Option<String>,
    message: String,
    task: TaskRun,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillInventoryStatus {
    #[serde(default)]
    first_host_scan_completed: bool,
    #[serde(default)]
    local_skill_root: String,
    #[serde(default)]
    local_skills: Vec<RemoteSkill>,
    #[serde(default)]
    host_inventories: Vec<HostSkillInventory>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostSkillInventory {
    host_alias: String,
    scanned_at: String,
    ok: bool,
    message: String,
    skills: Vec<RemoteSkill>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillDetectionResult {
    skills: Vec<SkillPack>,
    status: SkillInventoryStatus,
    tasks: Vec<TaskRun>,
    message: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillTargetRequest {
    target_type: String,
    host_alias: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillTarget {
    target_type: String,
    label: String,
    host_alias: Option<String>,
    path: String,
    installed: bool,
    can_install: bool,
    can_uninstall: bool,
    status: String,
    message: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillTargetsResult {
    skill_id: String,
    skill_name: String,
    targets: Vec<SkillTarget>,
    tasks: Vec<TaskRun>,
    message: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillTargetOperationItem {
    target_type: String,
    label: String,
    host_alias: Option<String>,
    ok: bool,
    message: String,
    task: Option<TaskRun>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillTargetOperationResult {
    ok: bool,
    skills: Vec<SkillPack>,
    tasks: Vec<TaskRun>,
    results: Vec<SkillTargetOperationItem>,
    message: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstalledSkillRequest {
    target_type: String,
    host_alias: Option<String>,
    skill_name: String,
    path: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstalledSkillDownloadResult {
    imported: Vec<SkillPack>,
    skipped: Vec<String>,
    skills: Vec<SkillPack>,
    status: SkillInventoryStatus,
    tasks: Vec<TaskRun>,
    message: String,
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
struct SshConfigDeleteResult {
    #[serde(flatten)]
    write_result: ssh::SshConfigWriteResult,
    task: TaskRun,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeleteOperationResult {
    ok: bool,
    deleted: bool,
    message: String,
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
    latency_ms: Option<u64>,
    os: String,
    arch: String,
    shell: String,
    path: Option<String>,
    path_has_local_bin: bool,
    codex_command_available: bool,
    codex_installed: bool,
    codex_path: Option<String>,
    codex_version: String,
    config_exists: bool,
    api_config_name: String,
    api_config_source: String,
    api_key_env_var: Option<String>,
    api_key_env_present: Option<bool>,
    skills_exists: bool,
    skills_count: u16,
    task: TaskRun,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct LatestCodexVersion {
    version: Option<String>,
    checked_at: Option<String>,
    source: String,
    error: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalCodexStatus {
    platform: platform::RuntimePlatform,
    detected: bool,
    path: Option<String>,
    version: Option<String>,
    search_paths: Vec<String>,
    install_hint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum RemoteCodexAction {
    CheckVersion,
    Install,
    Update,
    Uninstall,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteCodexMaintenanceResult {
    host_alias: String,
    ok: bool,
    action: RemoteCodexAction,
    before_version: Option<String>,
    after_version: Option<String>,
    codex_path: Option<String>,
    codex_command_available: bool,
    install_method: Option<String>,
    path_changed: bool,
    shell_config_path: Option<String>,
    backup_path: Option<String>,
    message: String,
    task: TaskRun,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteCodexProgressEvent {
    request_id: String,
    host_alias: String,
    action: RemoteCodexAction,
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

struct CodexProgressContext<'a> {
    app: &'a AppHandle,
    request_id: Option<&'a str>,
    host_alias: &'a str,
    action: &'a RemoteCodexAction,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NetworkProxyCandidate {
    source: String,
    url: Option<String>,
    available: bool,
    message: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NetworkProxyStatus {
    mode: NetworkProxyMode,
    proxy_url: Option<String>,
    source: Option<String>,
    message: String,
    candidates: Vec<NetworkProxyCandidate>,
}

struct AppState {
    hosts: Mutex<Vec<Host>>,
    profiles: Mutex<Vec<Profile>>,
    skill_packs: Mutex<Vec<SkillPack>>,
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

const CODEX_COMMAND_AVAILABLE_SCRIPT: &str = r#"if command -v codex >/dev/null 2>&1; then
  command -v codex
  exit 0
fi
login_shell="${SHELL:-}"
if [ -n "$login_shell" ] && [ -x "$login_shell" ]; then
  login_codex=$("$login_shell" -lc 'command -v codex 2>/dev/null' 2>/dev/null | head -n 1 || true)
  if [ -n "$login_codex" ]; then
    printf '%s\n' "$login_codex"
    exit 0
  fi
fi
printf 'no\n'
exit 1
"#;

const REMOTE_CONFIG_API_ENV_VAR_SCRIPT: &str = r#"if [ -f "$HOME/.codex/config.toml" ]; then
  sed -n -E 's/^[[:space:]]*(env_key|apiKeyEnvVar)[[:space:]]*=[[:space:]]*"([^"]*)".*/\2/p' "$HOME/.codex/config.toml" 2>/dev/null | head -n 1
fi
"#;

const REMOTE_API_ENV_PRESENT_SCRIPT: &str = r#"if [ ! -f "$HOME/.codex/config.toml" ]; then
  printf 'unknown\n'
  exit 0
fi
api_env=$(sed -n -E 's/^[[:space:]]*(env_key|apiKeyEnvVar)[[:space:]]*=[[:space:]]*"([^"]*)".*/\2/p' "$HOME/.codex/config.toml" 2>/dev/null | head -n 1)
case "$api_env" in
  "" | [0-9]* | *[!A-Za-z0-9_]*)
    printf 'unknown\n'
    exit 0
    ;;
esac
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
"#;

fn remote_skill_count_script() -> &'static str {
    r#"count=0
count_skill_dir() {
  dir=$1
  [ -d "$dir" ] || return
  count=$((count + 1))
}
scan_child_dir() {
  dir=$1
  [ -d "$dir" ] || return
  if [ -f "$dir/SKILL.md" ]; then
    count_skill_dir "$dir"
    return
  fi
  before=$count
  for nested in "$dir"/* "$dir"/.[!.]* "$dir"/..?*; do
    [ -d "$nested" ] || continue
    [ -f "$nested/SKILL.md" ] || continue
    count_skill_dir "$nested"
  done
  if [ "$count" = "$before" ]; then
    count_skill_dir "$dir"
  fi
}
scan_root() {
  root=$1
  [ -d "$root" ] || return
  if [ -f "$root/SKILL.md" ]; then
    count_skill_dir "$root"
  else
    for dir in "$root"/* "$root"/.[!.]* "$root"/..?*; do
      scan_child_dir "$dir"
    done
  fi
}
scan_root "$HOME/.codex/skills"
scan_root "$HOME/.codex/superpowers/skills"
printf '%s\n' "$count"
"#
}

const CODEX_PATH_REPAIR_SCRIPT: &str = r##"set -u
local_bin="$HOME/.local/bin"
mkdir -p "$local_bin"

shell_value=${SHELL:-}
shell_name=${shell_value##*/}
if [ "$shell_name" = "zsh" ]; then
  shell_config="$HOME/.zshrc"
elif [ "$shell_name" = "bash" ]; then
  shell_config="$HOME/.bashrc"
elif [ -f "$HOME/.zshrc" ] && [ ! -f "$HOME/.bashrc" ]; then
  shell_config="$HOME/.zshrc"
else
  shell_config="$HOME/.bashrc"
fi

begin_marker="# >>> CodexHub managed PATH"
end_marker="# <<< CodexHub managed PATH"
path_line='case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) export PATH="$HOME/.local/bin:$PATH" ;; esac'
changed=no
checked_paths=""
backup_paths=""

repair_path_file() {
  target=$1
  [ -n "$target" ] || return
  case ";$checked_paths;" in
    *";$target;"*) return ;;
  esac
  checked_paths="${checked_paths}${checked_paths:+;}$target"

  if [ -f "$target" ]; then
    if grep -F "$begin_marker" "$target" >/dev/null 2>&1 &&
      grep -F "$end_marker" "$target" >/dev/null 2>&1 &&
      grep -F "$path_line" "$target" >/dev/null 2>&1; then
      printf 'CodexHub PATH block is already present in %s\n' "$target"
      return
    fi
    if grep -F '$HOME/.local/bin' "$target" >/dev/null 2>&1 ||
      grep -F "$local_bin" "$target" >/dev/null 2>&1 ||
      grep -F '~/.local/bin' "$target" >/dev/null 2>&1; then
      printf 'PATH entry for %s already appears in %s\n' "$local_bin" "$target"
      return
    fi
  fi

  backup_path=""
  if [ -f "$target" ]; then
    backup_path="$target.codexhub.bak.$(date +%Y%m%d%H%M%S)"
    cp -p "$target" "$backup_path"
  else
    : >"$target"
  fi

  tmp_file="$target.codexhub.tmp.$$"
  if grep -F "$begin_marker" "$target" >/dev/null 2>&1 &&
    grep -F "$end_marker" "$target" >/dev/null 2>&1; then
    awk -v begin="$begin_marker" -v end="$end_marker" -v line="$path_line" '
      $0 == begin {
        print begin
        print line
        print end
        in_block = 1
        next
      }
      $0 == end && in_block {
        in_block = 0
        next
      }
      !in_block { print }
    ' "$target" >"$tmp_file"
    mv "$tmp_file" "$target"
  else
    rm -f "$tmp_file"
    {
      printf '\n%s\n' "$begin_marker"
      printf '%s\n' "$path_line"
      printf '%s\n' "$end_marker"
    } >>"$target"
  fi

  changed=yes
  if [ -n "$backup_path" ]; then
    backup_paths="${backup_paths}${backup_paths:+;}$backup_path"
  fi
  printf 'Added CodexHub PATH block to %s\n' "$target"
}

repair_path_file "$shell_config"
repair_path_file "$HOME/.profile"
if [ -f "$HOME/.bash_profile" ]; then
  repair_path_file "$HOME/.bash_profile"
fi
if [ -f "$HOME/.zprofile" ]; then
  repair_path_file "$HOME/.zprofile"
fi

printf 'CODEXHUB_PATH_CHANGED=%s\n' "$changed"
printf 'CODEXHUB_SHELL_CONFIG_PATH=%s\n' "$checked_paths"
printf 'CODEXHUB_BACKUP_PATH=%s\n' "$backup_paths"
"##;

const CODEX_INSTALL_SCRIPT: &str = r##"set -u
export CODEX_INSTALL_DIR="$HOME/.local/bin"
export CODEX_HOME="$HOME/.codex"
export CODEX_NON_INTERACTIVE=1
export PATH="$HOME/.local/bin:$PATH"

mkdir -p "$CODEX_INSTALL_DIR" "$CODEX_HOME"
tmp_dir="${TMPDIR:-/tmp}/codexhub-codex-install.$$"
mkdir -p "$tmp_dir"
trap 'rm -rf "$tmp_dir"' EXIT HUP INT TERM
insecure_tls_fallback=no

is_tls_cert_error() {
  grep -qiE 'certificate|self[- ]signed|local issuer|certificate verify failed|x509' "$1" 2>/dev/null
}

allow_insecure_for_url() {
  case "$1" in
    https://registry.npmmirror.com/* | https://*.npmmirror.com/*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

download_file_url() {
  url="$1"
  output="$2"
  allow_insecure="${3:-no}"
  phase_label="${4:-download}"
  connect_timeout="${5:-15}"
  max_time="${6:-60}"
  err_file="$tmp_dir/download.err"
  last_status=127

  printf '[CodexHub] Starting %s from %s\n' "$phase_label" "$url" >&2

  if [ "$allow_insecure" = "yes" ] && ! allow_insecure_for_url "$url"; then
    printf 'Insecure TLS fallback is limited to npmmirror URLs; refusing disabled verification for %s\n' "$url" >&2
    allow_insecure=no
  fi

  if command -v curl >/dev/null 2>&1; then
    rm -f "$err_file"
    if curl -fsSL --connect-timeout "$connect_timeout" --max-time "$max_time" "$url" -o "$output" 2>"$err_file"; then
      printf '[CodexHub] Finished %s.\n' "$phase_label" >&2
      return 0
    fi
    last_status=$?
    cat "$err_file" >&2
    if [ "$allow_insecure" = "yes" ] && is_tls_cert_error "$err_file"; then
      printf 'TLS certificate verification failed for %s; retrying npmmirror download with certificate checks disabled.\n' "$url" >&2
      rm -f "$err_file"
      if curl -k -fsSL --connect-timeout "$connect_timeout" --max-time "$max_time" "$url" -o "$output" 2>"$err_file"; then
        insecure_tls_fallback=yes
        printf '[CodexHub] Finished %s with insecure TLS fallback.\n' "$phase_label" >&2
        return 0
      fi
      last_status=$?
      cat "$err_file" >&2
    fi
  fi
  if command -v wget >/dev/null 2>&1; then
    rm -f "$err_file"
    if wget --timeout="$max_time" --tries=1 -qO "$output" "$url" 2>"$err_file"; then
      printf '[CodexHub] Finished %s.\n' "$phase_label" >&2
      return 0
    fi
    last_status=$?
    cat "$err_file" >&2
    if [ "$allow_insecure" = "yes" ] && is_tls_cert_error "$err_file"; then
      printf 'TLS certificate verification failed for %s; retrying npmmirror download with certificate checks disabled.\n' "$url" >&2
      rm -f "$err_file"
      if wget --timeout="$max_time" --tries=1 --no-check-certificate -qO "$output" "$url" 2>"$err_file"; then
        insecure_tls_fallback=yes
        printf '[CodexHub] Finished %s with insecure TLS fallback.\n' "$phase_label" >&2
        return 0
      fi
      last_status=$?
      cat "$err_file" >&2
    fi
  fi
  printf '[CodexHub] %s failed with status %s.\n' "$phase_label" "$last_status" >&2
  return "$last_status"
}

looks_like_captive_portal() {
  grep -qiE '<html|authentication is required|net2\.zju\.edu\.cn|captive|portal|login' "$1" 2>/dev/null
}

extract_npmmirror_metadata() {
  metadata_file="$1"
  package_platform="$2"

  if looks_like_captive_portal "$metadata_file"; then
    printf 'npmmirror metadata response was HTML instead of JSON; the remote network may require captive portal authentication before downloads can work.\n' >&2
    return 65
  fi

  python3 - "$metadata_file" "$package_platform" <<'PY'
import json
import sys

metadata_file = sys.argv[1]
package_platform = sys.argv[2]

try:
    with open(metadata_file, encoding="utf-8") as handle:
        data = json.load(handle)
except json.JSONDecodeError as exc:
    print(
        "npmmirror metadata response was not JSON; the remote network may require captive portal authentication before downloads can work: "
        + str(exc),
        file=sys.stderr,
    )
    sys.exit(65)
except OSError as exc:
    print("Could not read npmmirror metadata response: " + str(exc), file=sys.stderr)
    sys.exit(65)

latest = data.get("dist-tags", {}).get("latest")
if not isinstance(latest, str) or not latest:
    print("npmmirror metadata did not include dist-tags.latest for @openai/codex.", file=sys.stderr)
    sys.exit(66)

package_key = latest + "-" + package_platform
package = data.get("versions", {}).get(package_key)
if not isinstance(package, dict):
    print("npmmirror metadata did not include package version " + package_key + ".", file=sys.stderr)
    sys.exit(66)

tarball = package.get("dist", {}).get("tarball")
if not isinstance(tarball, str) or not tarball.startswith("https://registry.npmmirror.com/"):
    print("npmmirror metadata returned an unexpected tarball URL for " + package_key + ".", file=sys.stderr)
    sys.exit(66)

print("CODEXHUB_NATIVE_VERSION=" + latest)
print("CODEXHUB_NATIVE_TARBALL=" + tarball)
PY
}

official_status=127
printf '[CodexHub] Trying official Codex installer download.\n' >&2
if command -v curl >/dev/null 2>&1; then
  if curl -fsSL --connect-timeout 15 --max-time 45 "https://chatgpt.com/codex/install.sh" -o "$tmp_dir/install.sh" 2>"$tmp_dir/official.err"; then
    printf '[CodexHub] Official installer downloaded; starting installer.\n' >&2
    if command -v timeout >/dev/null 2>&1; then
      timeout 75 sh "$tmp_dir/install.sh"
    else
      sh "$tmp_dir/install.sh"
    fi
    official_status=$?
  else
    official_status=$?
    cat "$tmp_dir/official.err" >&2
    if is_tls_cert_error "$tmp_dir/official.err"; then
      printf 'Official Codex installer TLS verification failed; falling back to npmmirror. CodexHub will not disable TLS verification for the official installer.\n' >&2
    fi
  fi
elif command -v wget >/dev/null 2>&1; then
  if wget --timeout=15 --tries=1 -qO "$tmp_dir/install.sh" "https://chatgpt.com/codex/install.sh" 2>"$tmp_dir/official.err"; then
    if command -v timeout >/dev/null 2>&1; then
      timeout 75 sh "$tmp_dir/install.sh"
    else
      sh "$tmp_dir/install.sh"
    fi
    official_status=$?
  else
    official_status=$?
    cat "$tmp_dir/official.err" >&2
    if is_tls_cert_error "$tmp_dir/official.err"; then
      printf 'Official Codex installer TLS verification failed; falling back to npmmirror. CodexHub will not disable TLS verification for the official installer.\n' >&2
    fi
  fi
else
  printf 'curl or wget is not available for the official Codex installer.\n' >&2
fi

if [ "$official_status" -eq 0 ]; then
  printf '[CodexHub] Official Codex installer completed successfully.\n' >&2
  printf 'CODEXHUB_INSTALL_METHOD=official\n'
  exit 0
fi

printf 'Official Codex installer failed with status %s; trying npmmirror native package fallback.\n' "$official_status" >&2
native_status=127
if command -v python3 >/dev/null 2>&1; then
  arch=$(uname -m)
  case "$arch" in
    x86_64 | amd64)
      platform="linux-x64"
      target="x86_64-unknown-linux-musl"
      ;;
    aarch64 | arm64)
      platform="linux-arm64"
      target="aarch64-unknown-linux-musl"
      ;;
    *)
      platform=""
      target=""
      ;;
  esac

  version=""
  tarball=""
  if [ -n "$platform" ] && download_file_url "https://registry.npmmirror.com/@openai/codex" "$tmp_dir/codex-metadata.json" yes "npmmirror Codex metadata" 15 30; then
    metadata_out="$tmp_dir/codex-metadata.out"
    if extract_npmmirror_metadata "$tmp_dir/codex-metadata.json" "$platform" >"$metadata_out"; then
      version=$(sed -n 's/^CODEXHUB_NATIVE_VERSION=//p' "$metadata_out" | head -n 1)
      tarball=$(sed -n 's/^CODEXHUB_NATIVE_TARBALL=//p' "$metadata_out" | head -n 1)
    else
      native_status=$?
    fi
    if [ -n "$version" ] && [ -n "$tarball" ] && download_file_url "$tarball" "$tmp_dir/codex-platform.tgz" yes "npmmirror Codex native package" 15 75; then
      extract_dir="$tmp_dir/native-extract"
      release_dir="$CODEX_HOME/packages/standalone/releases/$version"
      stage_dir="$release_dir.tmp.$$"
      rm -rf "$extract_dir" "$stage_dir"
      mkdir -p "$extract_dir" "$stage_dir" "$CODEX_HOME/packages/standalone/releases"
      if ! tar -tzf "$tmp_dir/codex-platform.tgz" >/dev/null 2>&1; then
        printf 'npmmirror native package download was not a readable gzip tarball; the remote network may be returning an HTML login page instead of the package.\n' >&2
        native_status=65
      elif tar -tzf "$tmp_dir/codex-platform.tgz" | grep -Eq '(^|/)\.\.(/|$)|^/'; then
        printf 'npmmirror native package archive contains unsafe paths.\n' >&2
        native_status=66
      elif tar -xzf "$tmp_dir/codex-platform.tgz" -C "$extract_dir"; then
        vendor_dir="$extract_dir/package/vendor/$target"
        if [ -x "$vendor_dir/bin/codex" ]; then
          cp -R "$vendor_dir/." "$stage_dir/"
          chmod 0755 "$stage_dir/bin/codex"
          [ -f "$stage_dir/codex-path/rg" ] && chmod 0755 "$stage_dir/codex-path/rg"
          [ -f "$stage_dir/codex-resources/bwrap" ] && chmod 0755 "$stage_dir/codex-resources/bwrap"
          rm -rf "$release_dir"
          mv "$stage_dir" "$release_dir"
          ln -sfn "$release_dir" "$CODEX_HOME/packages/standalone/current"
          ln -sfn "$release_dir/bin/codex" "$CODEX_INSTALL_DIR/codex"
          native_status=0
        else
          printf 'npmmirror native package did not contain vendor/%s/bin/codex.\n' "$target" >&2
          native_status=66
        fi
      else
        native_status=$?
      fi
      rm -rf "$stage_dir"
    fi
  elif [ -z "$platform" ]; then
    printf 'npmmirror native package fallback does not support remote architecture %s.\n' "$arch" >&2
  fi
fi

if [ "$native_status" -eq 0 ]; then
  if [ "$insecure_tls_fallback" = "yes" ]; then
    printf 'CODEXHUB_INSTALL_METHOD=npm-mirror-native-insecure-tls\n'
  else
    printf 'CODEXHUB_INSTALL_METHOD=npm-mirror-native\n'
  fi
  exit 0
fi

printf 'npmmirror native package fallback failed with status %s; trying npm command fallback.\n' "$native_status" >&2
if ! command -v npm >/dev/null 2>&1; then
  printf 'npm is not available for the npmmirror fallback.\n' >&2
  printf 'CODEXHUB_INSTALL_METHOD=failed\n'
  exit 127
fi

printf '[CodexHub] Starting npm install fallback.\n' >&2
npm install -g @openai/codex --prefix "$HOME/.local" --registry=https://registry.npmmirror.com
npm_status=$?
if [ "$npm_status" -eq 0 ]; then
  printf '[CodexHub] npm install fallback completed successfully.\n' >&2
  printf 'CODEXHUB_INSTALL_METHOD=npm-mirror\n'
  exit 0
fi

printf 'CODEXHUB_INSTALL_METHOD=failed\n'
exit "$npm_status"
"##;

const CODEX_UNINSTALL_SCRIPT: &str = r##"set -u
export CODEX_INSTALL_DIR="${CODEX_INSTALL_DIR:-$HOME/.local/bin}"
export CODEX_HOME="${CODEX_HOME:-$HOME/.codex}"
export PATH="$CODEX_INSTALL_DIR:$PATH"

bin_path="$CODEX_INSTALL_DIR/codex"
standalone_root="$CODEX_HOME/packages/standalone"
hub_dir="$HOME/.codex-hub"
hub_target_file="$hub_dir/codex-target"
removed_paths=""

append_marker_value() {
  current=$1
  value=$2
  if [ -n "$current" ]; then
    printf '%s;%s\n' "$current" "$value"
  else
    printf '%s\n' "$value"
  fi
}

record_removed() {
  target=$1
  removed_paths=$(append_marker_value "$removed_paths" "$target")
}

remove_path() {
  target=$1
  case "$target" in
    "" | "/" | "$HOME" | "$HOME/")
      printf 'Refusing unsafe Codex uninstall path: %s\n' "$target" >&2
      exit 2
      ;;
  esac
  if [ -e "$target" ] || [ -L "$target" ]; then
    rm -rf "$target"
    record_removed "$target"
    printf 'Deleted %s\n' "$target"
  fi
}

remove_marked_block() {
  target=$1
  begin=$2
  end=$3
  [ -f "$target" ] || return 0
  if ! grep -F "$begin" "$target" >/dev/null 2>&1; then
    return 0
  fi
  tmp_file="$target.codexhub.uninstall.tmp.$$"
  awk -v begin="$begin" -v end="$end" '
    $0 == begin { in_block = 1; next }
    $0 == end && in_block { in_block = 0; next }
    !in_block { print }
  ' "$target" >"$tmp_file"
  mv "$tmp_file" "$target"
  printf 'Removed shell block from %s\n' "$target"
}

remove_shell_blocks() {
  checked=""
  for target in "$HOME/.bashrc" "$HOME/.zshrc" "$HOME/.profile" "$HOME/.bash_profile" "$HOME/.zprofile"; do
    case ";$checked;" in
      *";$target;"*) continue ;;
    esac
    checked="${checked}${checked:+;}$target"
    remove_marked_block "$target" "# >>> Codex installer >>>" "# <<< Codex installer <<<"
    remove_marked_block "$target" "# >>> CodexHub managed PATH" "# <<< CodexHub managed PATH"
    remove_marked_block "$target" "# >>> CodexHub managed env" "# <<< CodexHub managed env"
  done
}

is_codexhub_launcher() {
  [ -f "$bin_path" ] && head -n 8 "$bin_path" 2>/dev/null | grep -F "CodexHub managed launcher" >/dev/null 2>&1
}

read_real_path() {
  target=$1
  if command -v readlink >/dev/null 2>&1; then
    readlink -f "$target" 2>/dev/null || readlink "$target" 2>/dev/null || printf '%s\n' "$target"
  else
    printf '%s\n' "$target"
  fi
}

actual_path=""
launcher_is_codexhub=no
if is_codexhub_launcher; then
  launcher_is_codexhub=yes
  if [ -f "$hub_target_file" ]; then
    actual_path=$(sed -n '1p' "$hub_target_file")
  fi
fi
if [ -z "$actual_path" ]; then
  if [ -e "$bin_path" ] || [ -L "$bin_path" ]; then
    actual_path="$bin_path"
  else
    actual_path=$(command -v codex 2>/dev/null || true)
  fi
fi
real_path=""
if [ -n "$actual_path" ]; then
  real_path=$(read_real_path "$actual_path")
fi

method=""
command_to_run=""
case "$real_path" in
  "$standalone_root"/*)
    method="official-standalone"
    ;;
esac
if [ -z "$method" ] && { [ -e "$standalone_root" ] || [ -L "$standalone_root" ]; }; then
  case "$actual_path" in
    "$bin_path" | "")
      method="official-standalone"
      ;;
  esac
fi
if [ -z "$method" ] && [ -n "$actual_path" ]; then
  case "$actual_path" in
    /opt/homebrew/* | /usr/local/*)
      if [ "$(uname -s 2>/dev/null || true)" = "Darwin" ]; then
        method="brew"
        command_to_run="brew uninstall --cask codex"
      fi
      ;;
  esac
fi
if [ -z "$method" ] && [ -f "$actual_path" ] && grep -F "#!/usr/bin/env node" "$actual_path" >/dev/null 2>&1; then
  case "$actual_path" in
    *".bun"*)
      method="bun"
      command_to_run="bun remove -g @openai/codex"
      ;;
    *)
      method="npm"
      case "$actual_path" in
        "$HOME/.local/"* | "$CODEX_INSTALL_DIR/"*)
          command_to_run="npm uninstall -g @openai/codex --prefix \"$HOME/.local\""
          ;;
        *)
          command_to_run="npm uninstall -g @openai/codex"
          ;;
      esac
      ;;
  esac
fi

if [ -z "$method" ]; then
  if [ -z "$actual_path" ] && { [ ! -e "$CODEX_HOME" ] && [ ! -L "$CODEX_HOME" ]; } && { [ ! -e "$hub_dir" ] && [ ! -L "$hub_dir" ]; }; then
    printf 'Codex is already absent from the current user PATH.\n'
    printf 'CODEXHUB_UNINSTALL_METHOD=not-installed\n'
    printf 'CODEXHUB_REMOVED_PATH=\n'
    printf 'CODEXHUB_BACKUP_PATH=\n'
    exit 0
  fi
  method="direct-known-paths"
fi

case "$method" in
  official-standalone)
    printf 'Removing official standalone Codex files.\n'
    ;;
  brew | bun | npm)
    command_name=${command_to_run%% *}
    if ! command -v "$command_name" >/dev/null 2>&1; then
      printf '%s is required for official %s-managed Codex uninstall.\n' "$command_name" "$method" >&2
      printf 'CODEXHUB_UNINSTALL_METHOD=%s\n' "$method"
      printf 'CODEXHUB_REMOVED_PATH=%s\n' "$removed_paths"
      printf 'CODEXHUB_BACKUP_PATH=\n'
      exit 127
    fi
    printf 'Running official uninstall command: %s\n' "$command_to_run"
    sh -c "$command_to_run"
    status=$?
    if [ "$status" -ne 0 ]; then
      printf 'Official %s-managed Codex uninstall failed with status %s.\n' "$method" "$status" >&2
      printf 'CODEXHUB_UNINSTALL_METHOD=%s\n' "$method"
      printf 'CODEXHUB_REMOVED_PATH=%s\n' "$removed_paths"
      printf 'CODEXHUB_BACKUP_PATH=\n'
      exit "$status"
    fi
    ;;
  direct-known-paths)
    printf 'Removing Codex files from known user-scoped paths.\n'
    ;;
esac

remove_path "$bin_path"
remove_path "$CODEX_HOME"
remove_path "$hub_dir"
remove_path "$HOME/.cache/codex"
remove_path "$HOME/.config/codex"
remove_path "$HOME/.local/share/codex"
remove_path "$HOME/.local/state/codex"
remove_path "$HOME/.local/lib/node_modules/@openai/codex"
remove_path "$HOME/.local/lib/node_modules/.bin/codex"
if [ -d "$CODEX_INSTALL_DIR" ]; then
  find "$CODEX_INSTALL_DIR" -mindepth 1 -maxdepth 1 -name '.codex.*' -exec rm -f {} +
fi
remove_shell_blocks

printf 'CODEXHUB_UNINSTALL_METHOD=%s\n' "$method"
printf 'CODEXHUB_REMOVED_PATH=%s\n' "$removed_paths"
printf 'CODEXHUB_BACKUP_PATH=\n'
"##;

const CODEX_NATIVE_PLATFORM_SCRIPT: &str = r#"set -u
arch=$(uname -m)
case "$arch" in
  x86_64 | amd64)
    platform="linux-x64"
    target="x86_64-unknown-linux-musl"
    ;;
  aarch64 | arm64)
    platform="linux-arm64"
    target="aarch64-unknown-linux-musl"
    ;;
  *)
    printf 'CodexHub local upload fallback does not support remote architecture %s.\n' "$arch" >&2
    exit 2
    ;;
esac

printf 'CODEXHUB_NATIVE_PLATFORM=%s\n' "$platform"
printf 'CODEXHUB_NATIVE_TARGET=%s\n' "$target"
"#;

struct LocalCodexNativePackage {
    version: String,
    target: String,
    tarball_path: PathBuf,
    temp_dir: PathBuf,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            hosts: Mutex::new(mock_hosts()),
            profiles: Mutex::new(mock_profiles()),
            skill_packs: Mutex::new(mock_skill_packs()),
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
fn get_app_update_status(app: AppHandle) -> AppUpdateStatus {
    app_update_status_for_channel(
        current_app_channel(&app),
        current_app_version(&app),
        None,
        None,
    )
}

#[tauri::command]
async fn check_stable_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AppUpdateStatus, String> {
    let (status, attempts) = check_stable_update_status(app).await;
    record_task(&state, app_update_check_task(&status, &attempts));
    Ok(status)
}

async fn check_stable_update_status(app: AppHandle) -> (AppUpdateStatus, Vec<String>) {
    let channel = current_app_channel(&app);
    let current_version = current_app_version(&app);
    if channel != "stable" {
        return (
            app_update_status_for_channel(
                channel,
                current_version,
                Some(AppUpdateState::Disabled),
                None,
            ),
            Vec::new(),
        );
    }

    let config = stable_updater_config();
    if !stable_updater_configured(&config) {
        return (
            app_update_status_for_channel(
                channel,
                current_version,
                Some(AppUpdateState::PendingConfiguration),
                None,
            ),
            Vec::new(),
        );
    }

    let endpoint = match config
        .endpoint
        .as_deref()
        .and_then(|value| Url::parse(value).ok())
    {
        Some(endpoint) => endpoint,
        None => {
            return (
                app_update_status(
                    channel,
                    &current_version,
                    AppUpdateState::Error,
                    &config,
                    None,
                    Some(update_checked_at()),
                    "Stable updater endpoint is configured but invalid. Rebuild with a valid HTTPS feed URL.".into(),
                ),
                Vec::new(),
            );
        }
    };
    if endpoint.scheme() != "https" {
        return (
            app_update_status(
                channel,
                &current_version,
                AppUpdateState::Error,
                &config,
                None,
                Some(update_checked_at()),
                "Stable updater endpoint must use HTTPS.".into(),
            ),
            Vec::new(),
        );
    }

    let pubkey = config.pubkey.clone().unwrap_or_default();
    let settings = read_settings(&app);
    let (routes, route_notes) = stable_update_network_routes(&settings);
    let mut attempts = route_notes;
    let mut last_error = None;

    for route in routes {
        let label = route.label();
        let endpoints = stable_update_endpoints(&endpoint, route.proxy.as_ref()).await;
        let updater = match stable_updater(&app, pubkey.clone(), endpoints, route.proxy.clone()) {
            Ok(updater) => updater,
            Err(error) => {
                let message = updater_error_message(
                    "Stable updater could not initialize",
                    error,
                    "Verify the release feed and public signing key.",
                );
                attempts.push(format!("{label}: {message}"));
                last_error = Some(message);
                continue;
            }
        };

        match updater.check().await {
            Ok(Some(update)) => {
                attempts.push(format!("{label}: update {} is available", update.version));
                return (
                    app_update_status(
                        channel,
                        &current_version,
                        AppUpdateState::Available,
                        &config,
                        Some(update.version),
                        Some(update_checked_at()),
                        format!("A signed stable update is available via {label}. Use Install update to let Windows apply it."),
                    ),
                    attempts,
                );
            }
            Ok(None) => {
                attempts.push(format!("{label}: CodexHub stable is up to date"));
                return (
                    app_update_status(
                        channel,
                        &current_version,
                        AppUpdateState::UpToDate,
                        &config,
                        None,
                        Some(update_checked_at()),
                        format!("CodexHub stable is up to date via {label}."),
                    ),
                    attempts,
                );
            }
            Err(error) => {
                let message = updater_error_message(
                    "Stable update check failed",
                    error,
                    "Verify the configured feed, signatures, and network path.",
                );
                attempts.push(format!("{label}: {message}"));
                last_error = Some(message);
            }
        }
    }

    (
        app_update_status(
            channel,
            &current_version,
            AppUpdateState::Error,
            &config,
            None,
            Some(update_checked_at()),
            format!(
                "Stable update check failed across all network routes. {}",
                last_error.unwrap_or_else(|| "No updater route was available.".into())
            ),
        ),
        attempts,
    )
}

#[tauri::command]
async fn install_stable_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AppUpdateStatus, String> {
    let channel = current_app_channel(&app);
    let current_version = current_app_version(&app);
    if channel != "stable" {
        return Ok(app_update_status_for_channel(
            channel,
            current_version,
            Some(AppUpdateState::Disabled),
            None,
        ));
    }

    let config = stable_updater_config();
    if !stable_updater_configured(&config) {
        return Ok(app_update_status_for_channel(
            channel,
            current_version,
            Some(AppUpdateState::PendingConfiguration),
            None,
        ));
    }

    let endpoint = match config
        .endpoint
        .as_deref()
        .and_then(|value| Url::parse(value).ok())
    {
        Some(endpoint) => endpoint,
        None => {
            return Ok(app_update_status(
                channel,
                &current_version,
                AppUpdateState::Error,
                &config,
                None,
                Some(update_checked_at()),
                "Stable updater endpoint is configured but invalid. Rebuild with a valid HTTPS feed URL.".into(),
            ));
        }
    };
    if endpoint.scheme() != "https" {
        return Ok(app_update_status(
            channel,
            &current_version,
            AppUpdateState::Error,
            &config,
            None,
            Some(update_checked_at()),
            "Stable updater endpoint must use HTTPS.".into(),
        ));
    }

    let pubkey = config.pubkey.clone().unwrap_or_default();
    let settings = read_settings(&app);
    let (routes, route_notes) = stable_update_network_routes(&settings);
    let mut attempts = route_notes;
    let mut last_error = None;

    for route in routes {
        let label = route.label();
        let endpoints = stable_update_endpoints(&endpoint, route.proxy.as_ref()).await;
        let updater = match stable_updater(&app, pubkey.clone(), endpoints, route.proxy.clone()) {
            Ok(updater) => updater,
            Err(error) => {
                let message = updater_error_message(
                    "Stable updater could not initialize",
                    error,
                    "Verify the release feed and public signing key.",
                );
                attempts.push(format!("{label}: {message}"));
                last_error = Some(message);
                continue;
            }
        };

        match updater.check().await {
            Ok(Some(update)) => {
                let latest_version = update.version.clone();
                attempts.push(format!("{label}: downloading update {latest_version}"));
                match update.download_and_install(|_, _| {}, || {}).await {
                    Ok(()) => {
                        attempts.push(format!("{label}: installer started"));
                        let status = app_update_status(
                            channel,
                            &current_version,
                            AppUpdateState::Installing,
                            &config,
                            Some(latest_version),
                            Some(update_checked_at()),
                            format!("Stable update installer started via {label}. CodexHub will close while Windows applies the update."),
                        );
                        record_task(&state, app_update_install_task(&status, &attempts));
                        return Ok(status);
                    }
                    Err(error) => {
                        let message = updater_error_message(
                            "Stable update install failed",
                            error,
                            "Verify the signed artifact, feed metadata, installer path, and proxy route.",
                        );
                        attempts.push(format!("{label}: {message}"));
                        last_error = Some(message);
                    }
                }
            }
            Ok(None) => {
                attempts.push(format!("{label}: CodexHub stable is up to date"));
                let status = app_update_status(
                    channel,
                    &current_version,
                    AppUpdateState::UpToDate,
                    &config,
                    None,
                    Some(update_checked_at()),
                    format!("CodexHub stable is up to date via {label}."),
                );
                record_task(&state, app_update_install_task(&status, &attempts));
                return Ok(status);
            }
            Err(error) => {
                let message = updater_error_message(
                    "Stable update check failed",
                    error,
                    "Verify the configured feed, signatures, and network path.",
                );
                attempts.push(format!("{label}: {message}"));
                last_error = Some(message);
            }
        }
    }

    let status = app_update_status(
        channel,
        &current_version,
        AppUpdateState::Error,
        &config,
        None,
        Some(update_checked_at()),
        format!(
            "Stable update install failed across all network routes. {}",
            last_error.unwrap_or_else(|| "No updater route was available.".into())
        ),
    );
    record_task(&state, app_update_install_task(&status, &attempts));
    Ok(status)
}

fn current_app_channel(app: &AppHandle) -> &'static str {
    match app.config().identifier.as_str() {
        STABLE_IDENTIFIER => "stable",
        DEV_IDENTIFIER => "dev",
        _ => "dev",
    }
}

fn current_app_version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

fn stable_updater_config() -> StableUpdaterConfig {
    StableUpdaterConfig {
        endpoint: non_empty_compile_env(option_env!("CODEXHUB_STABLE_UPDATE_ENDPOINT")),
        pubkey: option_env!("CODEXHUB_STABLE_UPDATER_PUBKEY").and_then(normalize_updater_pubkey),
    }
}

fn non_empty_compile_env(value: Option<&'static str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_updater_pubkey(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(bytes) = general_purpose::STANDARD.decode(trimmed) {
        if let Ok(decoded) = String::from_utf8(bytes) {
            if decoded.contains("minisign public key") {
                if let Some(pub_file) = normalize_minisign_pub_file_text(&decoded) {
                    return Some(general_purpose::STANDARD.encode(pub_file.as_bytes()));
                }
            }
        }
        if let Some(pub_file) = minisign_pub_file_from_key_line(trimmed) {
            return Some(general_purpose::STANDARD.encode(pub_file.as_bytes()));
        }
    }
    if trimmed.contains("minisign public key") || trimmed.contains('\n') {
        if let Some(pub_file) = normalize_minisign_pub_file_text(trimmed) {
            return Some(general_purpose::STANDARD.encode(pub_file.as_bytes()));
        }
    }
    None
}

fn normalize_minisign_pub_file_text(value: &str) -> Option<String> {
    let key_line = extract_minisign_public_key_line(value)?;
    let comment = value
        .lines()
        .map(str::trim)
        .find(|line| line.contains("minisign public key"))
        .map(ToOwned::to_owned)
        .or_else(|| {
            minisign_key_id(&key_line)
                .map(|key_id| format!("untrusted comment: minisign public key: {key_id}"))
        })?;
    Some(format!("{comment}\n{key_line}\n"))
}

fn extract_minisign_public_key_line(value: &str) -> Option<String> {
    value
        .lines()
        .map(str::trim)
        .find(|line| minisign_key_id(line).is_some())
        .map(ToOwned::to_owned)
}

fn minisign_pub_file_from_key_line(value: &str) -> Option<String> {
    let key_id = minisign_key_id(value)?;
    Some(format!(
        "untrusted comment: minisign public key: {key_id}\n{value}\n"
    ))
}

fn minisign_key_id(value: &str) -> Option<String> {
    let bytes = general_purpose::STANDARD.decode(value).ok()?;
    if bytes.len() != 42 {
        return None;
    }
    if bytes.first() != Some(&0x45) || !matches!(bytes.get(1).copied(), Some(0x64 | 0x44)) {
        return None;
    }
    Some(
        bytes[2..10]
            .iter()
            .rev()
            .map(|byte| format!("{byte:02X}"))
            .collect::<Vec<_>>()
            .join(""),
    )
}

fn stable_updater_configured(config: &StableUpdaterConfig) -> bool {
    config.endpoint.is_some() && config.pubkey.is_some()
}

#[derive(Clone)]
struct StableUpdateNetworkRoute {
    source: String,
    proxy: Option<Url>,
}

impl StableUpdateNetworkRoute {
    fn direct() -> Self {
        Self {
            source: "direct".into(),
            proxy: None,
        }
    }

    fn label(&self) -> String {
        match &self.proxy {
            Some(proxy) => format!("{} {}", self.source, redact_proxy_url(proxy)),
            None => self.source.clone(),
        }
    }
}

const LOCAL_PROXY_PORTS: &[u16] = &[7890, 7897, 7891, 1080, 10808, 8080, 9090, 20171];
const PROXY_ENV_NAMES: &[&str] = &[
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
    "HTTP_PROXY",
    "http_proxy",
];

fn normalize_proxy_url(value: &str) -> Option<Url> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(port) = trimmed.parse::<u16>() {
        return Url::parse(&format!("http://127.0.0.1:{port}")).ok();
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("http://{trimmed}")
    };
    let url = Url::parse(&candidate).ok()?;
    match url.scheme() {
        "http" | "https" | "socks4" | "socks5" | "socks5h" => Some(url),
        _ => None,
    }
}

fn redact_proxy_url(url: &Url) -> String {
    let mut redacted = url.clone();
    if !redacted.username().is_empty() {
        let _ = redacted.set_username("redacted");
    }
    if redacted.password().is_some() {
        let _ = redacted.set_password(Some("redacted"));
    }
    redacted.to_string()
}

fn proxy_is_localhost(url: &Url) -> bool {
    matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"))
}

fn localhost_proxy_port_is_open(port: u16) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&address, Duration::from_millis(180)).is_ok()
}

fn proxy_candidate(source: String, url: Url) -> NetworkProxyCandidate {
    let available = if proxy_is_localhost(&url) {
        url.port()
            .map(localhost_proxy_port_is_open)
            .unwrap_or(false)
    } else {
        true
    };
    let message = if available {
        "Proxy route is available for updater retry.".into()
    } else {
        "Local proxy port did not accept a TCP connection.".into()
    };
    NetworkProxyCandidate {
        source,
        url: Some(redact_proxy_url(&url)),
        available,
        message,
    }
}

fn env_proxy_candidates() -> Vec<(String, Url)> {
    let mut entries = Vec::new();
    for name in PROXY_ENV_NAMES {
        if let Ok(value) = env::var(name) {
            if let Some(url) = normalize_proxy_url(&value) {
                entries.push((format!("env:{name}"), url));
            }
        }
    }
    dedupe_proxy_entries(entries)
}

fn local_proxy_candidates() -> Vec<(String, Url)> {
    LOCAL_PROXY_PORTS
        .iter()
        .filter(|port| localhost_proxy_port_is_open(**port))
        .filter_map(|port| {
            normalize_proxy_url(&format!("http://127.0.0.1:{port}"))
                .map(|url| (format!("local-port:{port}"), url))
        })
        .collect()
}

fn dedupe_proxy_entries(entries: Vec<(String, Url)>) -> Vec<(String, Url)> {
    let mut seen = BTreeSet::new();
    let mut deduped = Vec::new();
    for (source, url) in entries {
        let key = url.to_string();
        if seen.insert(key) {
            deduped.push((source, url));
        }
    }
    deduped
}

fn stable_update_network_routes(
    settings: &AppSettings,
) -> (Vec<StableUpdateNetworkRoute>, Vec<String>) {
    let mut routes = vec![StableUpdateNetworkRoute::direct()];
    let mut notes = Vec::new();
    let mut seen = BTreeSet::from(["direct".to_string()]);
    let mut push_proxy = |routes: &mut Vec<StableUpdateNetworkRoute>, source: String, url: Url| {
        let key = url.to_string();
        if seen.insert(key) {
            routes.push(StableUpdateNetworkRoute {
                source,
                proxy: Some(url),
            });
        }
    };

    match settings.network_proxy_mode {
        NetworkProxyMode::Direct => {}
        NetworkProxyMode::Manual => {
            if let Some(url) = normalize_proxy_url(&settings.network_proxy_url) {
                push_proxy(&mut routes, "manual".into(), url);
            } else if !settings.network_proxy_url.trim().is_empty() {
                notes.push("manual proxy URL is invalid".into());
            }
        }
        NetworkProxyMode::Auto => {
            for (source, url) in env_proxy_candidates() {
                push_proxy(&mut routes, source, url);
            }
            for (source, url) in local_proxy_candidates() {
                push_proxy(&mut routes, source, url);
            }
        }
    }

    (routes, notes)
}

fn detect_network_proxy_status(settings: &AppSettings) -> NetworkProxyStatus {
    if settings.network_proxy_mode == NetworkProxyMode::Direct {
        return NetworkProxyStatus {
            mode: settings.network_proxy_mode.clone(),
            proxy_url: None,
            source: None,
            message: "Network proxy is disabled; stable updater will use direct connections only."
                .into(),
            candidates: Vec::new(),
        };
    }

    let mut candidates = Vec::new();
    if let Some(manual) = normalize_proxy_url(&settings.network_proxy_url) {
        candidates.push(proxy_candidate("manual".into(), manual));
    } else if !settings.network_proxy_url.trim().is_empty() {
        candidates.push(NetworkProxyCandidate {
            source: "manual".into(),
            url: None,
            available: false,
            message: "Manual proxy URL is invalid.".into(),
        });
    }
    for (source, url) in env_proxy_candidates() {
        candidates.push(proxy_candidate(source, url));
    }
    for port in LOCAL_PROXY_PORTS {
        let url =
            normalize_proxy_url(&format!("http://127.0.0.1:{port}")).expect("local proxy URL");
        candidates.push(proxy_candidate(format!("local-port:{port}"), url));
    }

    let selected = candidates
        .iter()
        .find(|candidate| candidate.available && candidate.url.is_some());
    NetworkProxyStatus {
        mode: settings.network_proxy_mode.clone(),
        proxy_url: selected.and_then(|candidate| candidate.url.clone()),
        source: selected.map(|candidate| candidate.source.clone()),
        message: selected
            .map(|candidate| format!("Detected updater proxy route from {}.", candidate.source))
            .unwrap_or_else(|| "No local proxy port is currently reachable.".into()),
        candidates,
    }
}

async fn stable_update_endpoints(endpoint: &Url, proxy: Option<&Url>) -> Vec<Url> {
    let mut endpoints = Vec::new();
    if let Some(github_asset_endpoint) =
        resolve_github_latest_json_asset_endpoint(endpoint, proxy).await
    {
        endpoints.push(github_asset_endpoint);
    }
    endpoints.push(endpoint.clone());
    endpoints.dedup();
    endpoints
}

fn stable_updater(
    app: &AppHandle,
    pubkey: String,
    endpoints: Vec<Url>,
    proxy: Option<Url>,
) -> std::result::Result<tauri_plugin_updater::Updater, tauri_plugin_updater::Error> {
    let use_asset_api = endpoints.iter().any(is_github_release_asset_api_endpoint);
    let mut builder = app
        .updater_builder()
        .pubkey(pubkey)
        .endpoints(endpoints)?
        .timeout(Duration::from_secs(APP_UPDATE_CHECK_TIMEOUT_SECS));
    if let Some(proxy) = proxy {
        builder = builder.proxy(proxy);
    }
    if use_asset_api {
        builder = builder.header("Accept", OCTET_STREAM_ACCEPT)?;
    }
    builder.build()
}

async fn resolve_github_latest_json_asset_endpoint(
    endpoint: &Url,
    proxy: Option<&Url>,
) -> Option<Url> {
    let api_url = github_release_api_url(endpoint)?;
    let mut client_builder = reqwest::Client::builder()
        .user_agent("CodexHub updater feed resolver")
        .timeout(Duration::from_secs(APP_UPDATE_CHECK_TIMEOUT_SECS));
    if let Some(proxy) = proxy {
        client_builder = client_builder.proxy(reqwest::Proxy::all(proxy.as_str()).ok()?);
    }
    let client = client_builder.build().ok()?;
    let response = client
        .get(api_url)
        .header("Accept", GITHUB_API_ACCEPT)
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let release = response.json::<GitHubReleaseResponse>().await.ok()?;
    let asset = release
        .assets
        .into_iter()
        .find(|asset| asset.name == "latest.json")?;
    Url::parse(&asset.url).ok()
}

fn github_release_api_url(endpoint: &Url) -> Option<String> {
    if endpoint.scheme() != "https" || endpoint.host_str() != Some("github.com") {
        return None;
    }
    let segments = endpoint.path_segments()?.collect::<Vec<_>>();
    match segments.as_slice() {
        [owner, repo, "releases", "latest", "download", "latest.json"] => Some(format!(
            "https://api.github.com/repos/{owner}/{repo}/releases/latest"
        )),
        [owner, repo, "releases", "download", tag, "latest.json"] => Some(format!(
            "https://api.github.com/repos/{owner}/{repo}/releases/tags/{tag}"
        )),
        _ => None,
    }
}

fn is_github_release_asset_api_endpoint(endpoint: &Url) -> bool {
    endpoint.scheme() == "https"
        && endpoint.host_str() == Some("api.github.com")
        && endpoint.path().contains("/releases/assets/")
}

fn app_update_status_for_channel(
    channel: &'static str,
    current_version: String,
    state_override: Option<AppUpdateState>,
    latest_version: Option<String>,
) -> AppUpdateStatus {
    let config = stable_updater_config();
    let state = state_override.unwrap_or_else(|| {
        if channel != "stable" {
            AppUpdateState::Disabled
        } else if stable_updater_configured(&config) {
            AppUpdateState::Ready
        } else {
            AppUpdateState::PendingConfiguration
        }
    });
    let message = app_update_message(channel, &state);
    app_update_status(
        channel,
        &current_version,
        state,
        &config,
        latest_version,
        None,
        message,
    )
}

fn app_update_status(
    channel: &'static str,
    current_version: &str,
    state: AppUpdateState,
    config: &StableUpdaterConfig,
    latest_version: Option<String>,
    checked_at: Option<String>,
    message: String,
) -> AppUpdateStatus {
    let latest_version = match state {
        AppUpdateState::UpToDate => latest_version.or_else(|| Some(current_version.into())),
        _ => latest_version,
    };

    AppUpdateStatus {
        software_name: app_name_for_channel(channel).into(),
        channel: channel.into(),
        current_version: current_version.into(),
        installed_at: current_app_installed_at(),
        configured: stable_updater_configured(config),
        feed_configured: config.endpoint.is_some(),
        signing_configured: config.pubkey.is_some(),
        latest_version,
        checked_at,
        state,
        message,
    }
}

fn app_update_check_task(status: &AppUpdateStatus, attempts: &[String]) -> TaskRun {
    app_update_task(
        "task-app-update-check",
        "Check app update",
        "check_stable_update",
        status,
        attempts,
    )
}

fn app_update_install_task(status: &AppUpdateStatus, attempts: &[String]) -> TaskRun {
    app_update_task(
        "task-app-update-install",
        "Install app update",
        "install_stable_update",
        status,
        attempts,
    )
}

fn app_update_task(
    id_prefix: &str,
    action: &str,
    command: &str,
    status: &AppUpdateStatus,
    attempts: &[String],
) -> TaskRun {
    let task_id = format!("{id_prefix}-{}", timestamp_millis());
    let failed = matches!(&status.state, AppUpdateState::Error);
    let log_level = match &status.state {
        AppUpdateState::Error => TaskLogLevel::Error,
        AppUpdateState::Disabled | AppUpdateState::PendingConfiguration => TaskLogLevel::Warn,
        _ => TaskLogLevel::Info,
    };
    let latest_version = status.latest_version.as_deref().unwrap_or("not checked");
    let checked_at = status.checked_at.as_deref().unwrap_or("not checked");
    let task_time = status.checked_at.clone().unwrap_or_else(update_checked_at);
    let attempt_details = if attempts.is_empty() {
        "networkRoutes: no route attempts recorded".into()
    } else {
        format!("networkRoutes:\n{}", attempts.join("\n"))
    };
    let details = format!(
        "softwareName: {}\nchannel: {}\ncurrentVersion: {}\nlatestVersion: {}\nstate: {}\ncheckedAt: {}\nfeedConfigured: {}\nsigningConfigured: {}\n{}",
        status.software_name,
        status.channel,
        status.current_version,
        latest_version,
        app_update_state_label(&status.state),
        checked_at,
        status.feed_configured,
        status.signing_configured,
        attempt_details
    );

    TaskRun {
        id: task_id.clone(),
        host_id: "local-app".into(),
        host_name: status.software_name.clone(),
        action: action.into(),
        status: if failed {
            TaskStatus::Failed
        } else {
            TaskStatus::Success
        },
        started_at: task_time.clone(),
        ended_at: Some(task_time.clone()),
        summary: status.message.clone(),
        logs: vec![TaskLog {
            id: format!("{task_id}-log-1"),
            task_run_id: task_id,
            level: log_level,
            timestamp: task_time,
            message: status.message.clone(),
            command: Some(command.into()),
            stdout: Some(details),
            stderr: if failed {
                Some(status.message.clone())
            } else {
                Some(String::new())
            },
            exit_code: Some(if failed { 1 } else { 0 }),
            duration_ms: None,
            timed_out: Some(false),
        }],
    }
}

fn app_update_state_label(state: &AppUpdateState) -> &'static str {
    match state {
        AppUpdateState::Disabled => "disabled",
        AppUpdateState::PendingConfiguration => "pending-configuration",
        AppUpdateState::Ready => "ready",
        AppUpdateState::UpToDate => "up-to-date",
        AppUpdateState::Available => "available",
        AppUpdateState::Installing => "installing",
        AppUpdateState::Error => "error",
    }
}

fn app_name_for_channel(channel: &'static str) -> &'static str {
    match channel {
        "dev" => "CodexHub Dev",
        _ => "CodexHub",
    }
}

fn app_update_message(channel: &'static str, state: &AppUpdateState) -> String {
    match (channel, state) {
        ("dev", _) => "Dev channel auto-updates are disabled. Use local builds, preview packages, or test artifacts.".into(),
        ("stable", AppUpdateState::PendingConfiguration) => format!(
            "Stable updater is pending configuration. Set {STABLE_UPDATE_ENDPOINT_ENV} and {STABLE_UPDATER_PUBKEY_ENV} during the signed stable release build."
        ),
        ("stable", AppUpdateState::Ready) => "Stable updater feed and public key are configured. Run a manual check when ready.".into(),
        ("stable", AppUpdateState::UpToDate) => "CodexHub stable is up to date.".into(),
        ("stable", AppUpdateState::Available) => {
            "A signed stable update is available. Use Install update to let Windows apply it.".into()
        }
        ("stable", AppUpdateState::Installing) => {
            "Stable update installer started. CodexHub will close while Windows applies the update.".into()
        }
        ("stable", AppUpdateState::Error) => "Stable update check failed. Verify the configured feed, signatures, and network path.".into(),
        _ => "Stable updater status is unknown.".into(),
    }
}

fn updater_error_message(prefix: &str, error: impl Display, guidance: &str) -> String {
    format!("{prefix}: {error}. {guidance}")
}

fn update_checked_at() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn current_app_installed_at() -> Option<String> {
    env::current_exe()
        .ok()
        .and_then(|path| fs::metadata(path).ok())
        .and_then(|metadata| metadata.created().or_else(|_| metadata.modified()).ok())
        .map(format_system_time)
}

fn format_system_time(time: SystemTime) -> String {
    let local: DateTime<Local> = time.into();
    local.format("%Y-%m-%d %H:%M:%S").to_string()
}

async fn run_blocking_command<T, F>(label: &'static str, command: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(command)
        .await
        .map_err(|error| format!("{label} worker failed: {error}"))
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
fn detect_network_proxy(app: AppHandle) -> NetworkProxyStatus {
    let settings = read_settings(&app);
    detect_network_proxy_status(&settings)
}

#[tauri::command]
fn choose_close_button_behavior(
    app: AppHandle,
    behavior: CloseButtonBehavior,
) -> Result<AppSettings, String> {
    let mut settings = read_settings(&app);
    settings.close_button_behavior = behavior.clone();
    write_settings(&app, &settings)?;

    match behavior {
        CloseButtonBehavior::Ask => {}
        CloseButtonBehavior::Exit => app.exit(0),
        CloseButtonBehavior::MinimizeToTray => hide_main_window(&app),
    }

    Ok(settings)
}

#[tauri::command]
async fn get_ssh_status() -> Result<ssh::SshStatus, String> {
    run_blocking_command("get_ssh_status", ssh::get_ssh_status).await?
}

#[tauri::command]
fn generate_ed25519_key() -> Result<ssh::SshKeyGenerationResult, String> {
    ssh::generate_ed25519_key()
}

#[tauri::command]
async fn list_ssh_config_hosts() -> Result<Vec<ssh::SshConfigHost>, String> {
    run_blocking_command("list_ssh_config_hosts", ssh::list_ssh_config_hosts).await?
}

#[tauri::command]
fn upsert_ssh_config_host(draft: ssh::SshHostDraft) -> Result<ssh::SshConfigWriteResult, String> {
    ssh::upsert_ssh_config_host(draft)
}

#[tauri::command]
fn delete_ssh_config_host(
    app: AppHandle,
    alias: String,
    state: State<'_, AppState>,
) -> Result<SshConfigDeleteResult, String> {
    let normalized_alias = alias.trim().to_string();
    let host_id = host_id_for_alias(&state, &normalized_alias);
    let host_name = host_name_for_alias(&state, &normalized_alias);
    let result = ssh::delete_ssh_config_host(alias.clone())?;
    if result.changed {
        let next_hosts = {
            let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");
            hosts.retain(|host| !host.host_alias.eq_ignore_ascii_case(alias.trim()));
            hosts.clone()
        };
        save_hosts(&app, &state, &next_hosts)?;
    }
    let task_id = format!("task-delete-host-{}", timestamp_millis());
    let task = delete_task(
        &task_id,
        &host_id,
        &host_name,
        "Delete SSH Host",
        &result.message,
        true,
        Some(format!("delete_ssh_config_host {}", normalized_alias)),
    );
    record_task(&state, task.clone());
    Ok(SshConfigDeleteResult {
        write_result: result,
        task,
    })
}

#[tauri::command]
fn list_hosts(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<Host>, String> {
    let hosts = load_hosts(&app, &state)?;
    *state.hosts.lock().expect("hosts mutex poisoned") = hosts;
    let profiles = profile_apply_profiles_snapshot(&app, &state);
    reconcile_hosts_with_profile_links(&state, &profiles);
    apply_skill_inventory_to_hosts(&app, &state)?;
    let next_hosts = state.hosts.lock().expect("hosts mutex poisoned").clone();
    save_hosts(&app, &state, &next_hosts)?;
    Ok(next_hosts)
}

#[tauri::command]
fn refresh_discovered_hosts(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<Host>, String> {
    let hosts = load_hosts(&app, &state)?;
    *state.hosts.lock().expect("hosts mutex poisoned") = hosts;
    merge_discovered_hosts(&state)?;
    let profiles = load_profiles(&app, &state)?;
    reconcile_hosts_with_profile_links(&state, &profiles);
    let next_hosts = state.hosts.lock().expect("hosts mutex poisoned").clone();
    save_hosts(&app, &state, &next_hosts)?;
    Ok(next_hosts)
}

#[tauri::command]
fn add_host(app: AppHandle, state: State<'_, AppState>, draft: HostDraft) -> Result<Host, String> {
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

    let next_hosts = {
        let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");
        hosts.insert(0, host.clone());
        hosts.clone()
    };
    save_hosts(&app, &state, &next_hosts)?;
    Ok(host)
}

#[tauri::command]
fn update_host(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    patch: HostPatch,
) -> Result<Host, String> {
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
        let updated = host.clone();
        let next_hosts = hosts.clone();
        drop(hosts);
        save_hosts(&app, &state, &next_hosts)?;
        return Ok(updated);
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
        profile_id: patch.profile_id,
        skill_pack_ids: Vec::new(),
        tags: patch.tags.unwrap_or_default(),
        last_seen: "just added".into(),
        latency_ms: None,
    };
    hosts.insert(0, host.clone());
    let next_hosts = hosts.clone();
    drop(hosts);
    save_hosts(&app, &state, &next_hosts)?;
    Ok(host)
}

#[tauri::command]
fn delete_host(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");
    let before = hosts.len();
    hosts.retain(|host| host.id != id);
    let changed = hosts.len() != before;
    let next_hosts = hosts.clone();
    drop(hosts);
    if changed {
        save_hosts(&app, &state, &next_hosts)?;
    }
    Ok(changed)
}

#[tauri::command]
fn test_ssh_connection(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<ConnectionTest, String> {
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");

    if let Some(host) = hosts.iter_mut().find(|host| host.id == id) {
        let ok = true;
        host.status = if ok {
            HostStatus::Online
        } else {
            HostStatus::Offline
        };
        host.latency_ms = if ok { Some(24) } else { None };
        if ok {
            host.last_seen = "just now".into();
        }

        let result = ConnectionTest {
            ok,
            latency_ms: host.latency_ms,
            message: if ok {
                format!("Mock SSH handshake to {} completed.", host.name)
            } else {
                format!("Mock SSH handshake to {} timed out.", host.name)
            },
        };
        let next_hosts = hosts.clone();
        drop(hosts);
        save_hosts(&app, &state, &next_hosts)?;
        return Ok(result);
    }

    Ok(ConnectionTest {
        ok: false,
        latency_ms: None,
        message: "Host not found.".into(),
    })
}

#[tauri::command]
async fn ssh_check(
    app: AppHandle,
    host_alias: String,
    timeout_ms: Option<u64>,
) -> Result<SshCheckResult, String> {
    run_blocking_command("ssh_check", move || {
        let state = app.state::<AppState>();
        run_ssh_check(&state, host_alias, timeout_ms)
    })
    .await
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
    app: AppHandle,
    state: State<'_, AppState>,
    host_alias: String,
    password: String,
    timeout_ms: Option<u64>,
) -> Result<SshBootstrapResult, String> {
    let result = run_existing_ssh_bootstrap(&state, host_alias, password, timeout_ms)?;
    if result.ok {
        save_current_hosts(&app, &state)?;
    }
    Ok(result)
}

#[tauri::command]
async fn remote_probe_codex(
    app: AppHandle,
    host_alias: String,
    timeout_ms: Option<u64>,
) -> Result<RemoteProbeResult, String> {
    run_blocking_command("remote_probe_codex", move || {
        let state = app.state::<AppState>();
        run_remote_probe(&app, &state, host_alias, timeout_ms)
    })
    .await
}

#[tauri::command]
async fn sample_host_resources(
    host_aliases: Vec<String>,
    timeout_ms: Option<u64>,
) -> Result<resource_monitor::HostResourceBatchResult, String> {
    run_blocking_command("sample_host_resources", move || {
        resource_monitor::sample_host_resources(host_aliases, timeout_ms)
    })
    .await
}

#[tauri::command]
async fn remote_manage_codex(
    app: AppHandle,
    host_alias: String,
    action: RemoteCodexAction,
    timeout_ms: Option<u64>,
    request_id: Option<String>,
) -> Result<RemoteCodexMaintenanceResult, String> {
    run_blocking_command("remote_manage_codex", move || {
        let state = app.state::<AppState>();
        run_remote_manage_codex(&app, &state, host_alias, action, timeout_ms, request_id)
    })
    .await
}

#[tauri::command]
async fn refresh_latest_codex_version(
    app: AppHandle,
    force: Option<bool>,
    timeout_ms: Option<u64>,
) -> Result<LatestCodexVersion, String> {
    run_blocking_command("refresh_latest_codex_version", move || {
        run_refresh_latest_codex_version(&app, force.unwrap_or(false), timeout_ms)
    })
    .await
}

#[tauri::command]
async fn get_local_codex_status() -> Result<LocalCodexStatus, String> {
    run_blocking_command("get_local_codex_status", run_get_local_codex_status).await
}

#[tauri::command]
fn list_profiles(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<Profile>, String> {
    load_profiles(&app, &state)
}

#[tauri::command]
fn create_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    draft: ProfileDraft,
) -> Result<Profile, String> {
    let mut profiles = load_profiles(&app, &state)?;
    let mut profile = profile_from_draft(draft)?;
    ensure_unique_profile_id(&mut profile, &profiles);
    validate_profile(&profile)?;
    profile.credential_stored = false;
    profiles.push(profile.clone());
    save_profiles(&app, &state, &profiles)?;
    Ok(profile)
}

#[tauri::command]
fn update_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    patch: ProfilePatch,
) -> Result<Profile, String> {
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
}

#[tauri::command]
fn delete_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<DeleteOperationResult, String> {
    let mut profiles = load_profiles(&app, &state)?;
    let profile_name = profiles
        .iter()
        .find(|profile| profile.id == id)
        .map(|profile| profile.name.clone())
        .unwrap_or_else(|| id.clone());
    let before = profiles.len();
    profiles.retain(|profile| profile.id != id);
    let deleted = profiles.len() != before;
    if deleted {
        save_profiles(&app, &state, &profiles)?;
        clear_profile_from_hosts(&state, &id);
        save_current_hosts(&app, &state)?;
        let _ = delete_profile_api_key_local(&id);
    }
    let message = if deleted {
        format!("Deleted profile {profile_name}.")
    } else {
        format!("Profile {profile_name} was not found.")
    };
    let task_id = format!("task-delete-profile-{}", timestamp_millis());
    let task = delete_task(
        &task_id,
        &id,
        &profile_name,
        "Delete profile",
        &message,
        deleted,
        Some(format!("delete_profile {id}")),
    );
    record_task(&state, task.clone());
    Ok(DeleteOperationResult {
        ok: deleted,
        deleted,
        message,
        task,
    })
}

#[tauri::command]
fn duplicate_profile(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
) -> Result<Profile, String> {
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
}

#[tauri::command]
fn import_profiles(
    app: AppHandle,
    state: State<'_, AppState>,
    bundle: ProfileImportExport,
    replace: Option<bool>,
) -> Result<ProfileImportExport, String> {
    let result = import_profiles_inner(&app, &state, bundle.profiles, replace.unwrap_or(false))?;
    Ok(profile_import_export(result.imported))
}

#[tauri::command]
fn set_profile_api_key(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
    api_key: String,
) -> Result<Profile, String> {
    if api_key.trim().is_empty() {
        return Err("API key value cannot be empty.".into());
    }
    let mut profiles = load_profiles(&app, &state)?;
    let profile = profiles
        .iter_mut()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| format!("Profile {profile_id} was not found."))?;
    store_profile_api_key_local(&profile_id, &api_key)?;
    profile.credential_stored = true;
    profile.updated_at = timestamp_label();
    let updated = profile.clone();
    save_profiles(&app, &state, &profiles)?;
    Ok(updated)
}

#[tauri::command]
fn get_profile_api_key(
    app: AppHandle,
    state: State<'_, AppState>,
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
        api_key = migrate_cc_switch_api_key_for_profile(&app, &state, &profile)?;
        if api_key.is_some() {
            for item in &mut profiles {
                if item.id == profile_id {
                    item.credential_stored = true;
                }
            }
            save_profiles(&app, &state, &profiles)?;
        }
    }
    Ok(ProfileApiKeyResult {
        profile_id,
        exists: api_key.is_some(),
        api_key,
    })
}

#[tauri::command]
fn delete_profile_api_key(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<Profile, String> {
    delete_profile_api_key_local(&profile_id)?;
    let mut profiles = load_profiles(&app, &state)?;
    let profile = profiles
        .iter_mut()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| format!("Profile {profile_id} was not found."))?;
    profile.credential_stored = false;
    profile.updated_at = timestamp_label();
    let updated = profile.clone();
    save_profiles(&app, &state, &profiles)?;
    Ok(updated)
}

#[tauri::command]
fn preview_profile_apply(
    app: AppHandle,
    state: State<'_, AppState>,
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

#[tauri::command]
async fn apply_profile(
    app: AppHandle,
    profile_id: String,
    host_ids: Vec<String>,
    timeout_ms: Option<u64>,
) -> Result<ProfileApplyBatchResult, String> {
    run_blocking_command("apply_profile", move || {
        let state = app.state::<AppState>();
        let profile = find_profile(&app, &state, &profile_id)?;
        let rendered_toml = render_profile_toml(&profile)?;
        let timeout = ssh::normalize_timeout_ms(timeout_ms.or(Some(30_000)));
        let result =
            apply_profile_to_hosts(&app, &state, &profile, &rendered_toml, host_ids, timeout);
        Ok(result)
    })
    .await?
}

#[tauri::command]
fn detect_cc_switch_profiles(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CcSwitchDetection, String> {
    let detected = detect_cc_switch_profiles_inner(&app, &state)?;
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

#[tauri::command]
fn import_cc_switch_profiles(
    app: AppHandle,
    state: State<'_, AppState>,
    replace: Option<bool>,
) -> Result<ProfileImportExport, String> {
    let detected = detect_cc_switch_profiles_inner(&app, &state)?;
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
    let result = import_profiles_inner(&app, &state, profiles, replace.unwrap_or(false))?;
    for profile in &result.imported {
        if let Some(api_key) = credential_by_key.get(&cc_switch_profile_import_key(profile)) {
            store_profile_api_key_local(&profile.id, api_key)?;
        }
    }
    let mut imported = result.imported;
    refresh_credential_flags(&mut imported);
    let mut profiles = load_profiles(&app, &state)?;
    refresh_credential_flags(&mut profiles);
    save_profiles(&app, &state, &profiles)?;
    Ok(profile_import_export(imported))
}

fn migrate_cc_switch_api_key_for_profile(
    app: &AppHandle,
    state: &AppState,
    profile: &Profile,
) -> Result<Option<String>, String> {
    let import_key = cc_switch_profile_import_key(profile);
    let detected = detect_cc_switch_profiles_inner(app, state)?;
    let api_key = detected
        .into_iter()
        .find(|item| cc_switch_profile_import_key(&item.profile) == import_key)
        .and_then(|item| item.api_key);
    if let Some(api_key) = api_key {
        store_profile_api_key_local(&profile.id, &api_key)?;
        Ok(Some(api_key))
    } else {
        Ok(None)
    }
}

#[tauri::command]
fn list_local_skills(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<SkillPack>, String> {
    load_skills(&app, &state)
}

#[tauri::command]
fn list_skill_packs(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<SkillPack>, String> {
    load_skills(&app, &state)
}

#[tauri::command]
fn import_local_skill(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<SkillImportResult, String> {
    import_skills_from_path(&app, &state, PathBuf::from(path), "local", None)
}

#[tauri::command]
fn update_library_skill_about(
    app: AppHandle,
    state: State<'_, AppState>,
    skill_id: String,
    about: String,
) -> Result<Vec<SkillPack>, String> {
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
}

#[tauri::command]
fn get_skill_inventory_status(app: AppHandle) -> Result<SkillInventoryStatus, String> {
    load_skill_inventory_status(&app)
}

#[tauri::command]
async fn detect_installed_skills(
    app: AppHandle,
    include_hosts: Option<bool>,
    timeout_ms: Option<u64>,
) -> Result<SkillDetectionResult, String> {
    run_blocking_command("detect_installed_skills", move || {
        let state = app.state::<AppState>();
        run_detect_installed_skills(&app, &state, include_hosts.unwrap_or(false), timeout_ms)
    })
    .await?
}

#[tauri::command]
async fn download_github_skill(
    app: AppHandle,
    repo_url: String,
    timeout_ms: Option<u64>,
) -> Result<SkillImportResult, String> {
    run_blocking_command("download_github_skill", move || {
        let state = app.state::<AppState>();
        download_and_import_github_skill(&app, &state, repo_url, timeout_ms)
    })
    .await?
}

#[tauri::command]
async fn get_skill_targets(
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

#[tauri::command]
async fn install_skill_targets(
    app: AppHandle,
    skill_id: String,
    targets: Vec<SkillTargetRequest>,
    timeout_ms: Option<u64>,
) -> Result<SkillTargetOperationResult, String> {
    run_blocking_command("install_skill_targets", move || {
        let state = app.state::<AppState>();
        run_install_skill_targets(&app, &state, skill_id, targets, timeout_ms)
    })
    .await?
}

#[tauri::command]
async fn uninstall_skill_targets(
    app: AppHandle,
    skill_id: String,
    targets: Vec<SkillTargetRequest>,
    timeout_ms: Option<u64>,
) -> Result<SkillTargetOperationResult, String> {
    run_blocking_command("uninstall_skill_targets", move || {
        let state = app.state::<AppState>();
        run_uninstall_skill_targets(&app, &state, skill_id, targets, timeout_ms)
    })
    .await?
}

#[tauri::command]
async fn delete_library_skill(
    app: AppHandle,
    skill_id: String,
    uninstall_first: bool,
    timeout_ms: Option<u64>,
) -> Result<SkillTargetOperationResult, String> {
    run_blocking_command("delete_library_skill", move || {
        let state = app.state::<AppState>();
        run_delete_library_skill(&app, &state, skill_id, uninstall_first, timeout_ms)
    })
    .await?
}

#[tauri::command]
async fn download_installed_skill(
    app: AppHandle,
    request: InstalledSkillRequest,
    timeout_ms: Option<u64>,
) -> Result<InstalledSkillDownloadResult, String> {
    run_blocking_command("download_installed_skill", move || {
        let state = app.state::<AppState>();
        run_download_installed_skill(&app, &state, request, timeout_ms)
    })
    .await?
}

#[tauri::command]
async fn uninstall_installed_skill(
    app: AppHandle,
    request: InstalledSkillRequest,
    timeout_ms: Option<u64>,
) -> Result<SkillTargetOperationResult, String> {
    run_blocking_command("uninstall_installed_skill", move || {
        let state = app.state::<AppState>();
        run_uninstall_installed_skill(&app, &state, request, timeout_ms)
    })
    .await?
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
    if ok {
        if let Some(handle) = app {
            save_current_hosts(handle, state)?;
        }
    }
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

fn delete_task(
    task_id: &str,
    host_id: &str,
    host_name: &str,
    action: &str,
    summary: &str,
    ok: bool,
    command: Option<String>,
) -> TaskRun {
    TaskRun {
        id: task_id.to_string(),
        host_id: host_id.to_string(),
        host_name: host_name.to_string(),
        action: action.to_string(),
        status: if ok {
            TaskStatus::Success
        } else {
            TaskStatus::Failed
        },
        started_at: "now".into(),
        ended_at: Some("now".into()),
        summary: summary.to_string(),
        logs: vec![TaskLog {
            id: format!("{task_id}-log-1"),
            task_run_id: task_id.to_string(),
            level: if ok {
                TaskLogLevel::Info
            } else {
                TaskLogLevel::Error
            },
            timestamp: "now".into(),
            message: summary.to_string(),
            command,
            stdout: if ok { Some("ok".into()) } else { None },
            stderr: if ok { None } else { Some(summary.to_string()) },
            exit_code: Some(if ok { 0 } else { 1 }),
            duration_ms: Some(1),
            timed_out: Some(false),
        }],
    }
}

fn run_remote_probe(
    app: &AppHandle,
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
        };
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
    );
    record_task(state, task.clone());

    RemoteProbeResult {
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
    }
}

fn run_remote_manage_codex(
    app: &AppHandle,
    state: &AppState,
    host_alias: String,
    action: RemoteCodexAction,
    timeout_ms: Option<u64>,
    request_id: Option<String>,
) -> RemoteCodexMaintenanceResult {
    let timeout = ssh::normalize_timeout_ms(timeout_ms.or(Some(120_000)));
    let alias_result = ssh::validate_ssh_alias(&host_alias);
    let alias = alias_result
        .clone()
        .unwrap_or_else(|_| host_alias.trim().to_string());
    let task_id = format!("task-codex-{}", timestamp_millis());
    let host_name = host_name_for_alias(state, &alias);
    let host_id = host_id_for_alias(state, &alias);
    let action_label = remote_codex_action_label(&action);
    let progress = CodexProgressContext {
        app,
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
        record_task(state, task.clone());
        return RemoteCodexMaintenanceResult {
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
        };
    }
    update_host_check(state, &alias, true, check_output.duration_ms);

    let mut next_log_index = 2;
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
        record_task(state, task.clone());
        return RemoteCodexMaintenanceResult {
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
        };
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
        record_task(state, task.clone());
        return RemoteCodexMaintenanceResult {
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
        };
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
    record_task(state, task.clone());

    RemoteCodexMaintenanceResult {
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
    }
}

fn run_codex_step(
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

fn command_step_message(label: &str, output: &ssh::SshCommandOutput, timeout: u64) -> String {
    if output.success() {
        format!("{label} completed.")
    } else if output.timed_out {
        format!("{label} timed out after {timeout} ms.")
    } else {
        format!("{label} failed: {}", command_detail(output))
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_remote_codex_progress(
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
        detail,
        stdout,
        stderr,
        exit_code,
        duration_ms,
        timed_out,
    };
    let _ = progress.app.emit("remote-codex-progress", payload);
}

fn emit_remote_codex_stream_event(
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

fn emit_remote_codex_progress_for_output(
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

fn first_output_line(value: &str) -> Option<String> {
    value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

fn push_command_step_log(
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

fn run_local_upload_codex_fallback(
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
        download_codex_native_package_locally(&platform, &target, timeout, progress);
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
        let _ = fs::remove_dir_all(&package.temp_dir);
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
    let _ = fs::remove_dir_all(&package.temp_dir);
    install_output
}

fn download_codex_native_package_locally(
    platform: &str,
    target: &str,
    timeout: u64,
    progress: Option<&CodexProgressContext<'_>>,
) -> (Option<LocalCodexNativePackage>, ssh::SshCommandOutput) {
    let temp_dir = env::temp_dir().join(format!("codexhub-codex-native-{}", timestamp_millis()));
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
        let _ = fs::remove_dir_all(&temp_dir);
        return (None, metadata_output);
    }

    let metadata = match fs::read_to_string(&metadata_path) {
        Ok(value) => value,
        Err(error) => {
            let _ = fs::remove_dir_all(&temp_dir);
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
            let _ = fs::remove_dir_all(&temp_dir);
            return (
                None,
                failed_command_output("parse local @openai/codex metadata".into(), error),
            );
        }
    };
    if !is_safe_codex_package_version(&version) {
        let _ = fs::remove_dir_all(&temp_dir);
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
        let _ = fs::remove_dir_all(&temp_dir);
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

fn run_refresh_latest_codex_version(
    app: &AppHandle,
    force: bool,
    timeout_ms: Option<u64>,
) -> LatestCodexVersion {
    let cache_path = latest_codex_version_path(app);
    let cached = read_latest_codex_version_cache(&cache_path);
    let now = Local::now().fixed_offset();
    if !force {
        if let Some(cache) = cached.as_ref() {
            if latest_codex_cache_is_fresh(cache, now) {
                return LatestCodexVersion {
                    error: None,
                    ..cache.clone()
                };
            }
        }
    }

    let timeout = ssh::normalize_timeout_ms(timeout_ms.or(Some(30_000)));
    let fetched = fetch_latest_codex_version(timeout);
    let should_write = fetched.is_ok();
    let latest = latest_codex_result_from_fetch(fetched, cached, now);
    if should_write {
        if let Err(error) = write_latest_codex_version_cache(&cache_path, &latest) {
            return LatestCodexVersion {
                error: Some(error),
                ..latest
            };
        }
    }
    latest
}

fn run_get_local_codex_status() -> LocalCodexStatus {
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

fn local_codex_install_hint(platform: platform::RuntimePlatform) -> String {
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

fn latest_codex_result_from_fetch(
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

fn latest_codex_version_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .unwrap_or_else(|_| PathBuf::from(".codexhub"))
        .join("codex-latest.json")
}

fn read_latest_codex_version_cache(path: &Path) -> Option<LatestCodexVersion> {
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<LatestCodexVersion>(&content).ok())
}

fn write_latest_codex_version_cache(
    path: &Path,
    latest: &LatestCodexVersion,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let content = serde_json::to_string_pretty(latest).map_err(|error| error.to_string())?;
    fs::write(path, content).map_err(|error| error.to_string())
}

fn latest_codex_cache_is_fresh(cache: &LatestCodexVersion, now: DateTime<FixedOffset>) -> bool {
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

fn latest_codex_refresh_boundary(now: DateTime<FixedOffset>) -> DateTime<FixedOffset> {
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

fn fetch_latest_codex_version(timeout: u64) -> Result<String, String> {
    let temp_dir = env::temp_dir().join(format!("codexhub-codex-latest-{}", timestamp_millis()));
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
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(command_detail(&output));
    }
    let metadata = match fs::read_to_string(&metadata_path) {
        Ok(value) => value,
        Err(error) => {
            let _ = fs::remove_dir_all(&temp_dir);
            return Err(format!("Could not read downloaded npm metadata: {error}"));
        }
    };
    let latest = parse_npm_latest_metadata(&metadata);
    let _ = fs::remove_dir_all(&temp_dir);
    latest
}

fn parse_npm_latest_metadata(metadata: &str) -> Result<String, String> {
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

fn local_curl_download(
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

fn parse_npmmirror_native_metadata(
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

fn is_safe_codex_package_version(version: &str) -> bool {
    !version.is_empty()
        && version
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '+'))
}

fn codex_install_uploaded_package_script(
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

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn output_trimmed(output: &ssh::SshCommandOutput) -> Option<String> {
    output
        .success()
        .then(|| output.stdout.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn marker_value(stdout: &str, marker: &str) -> Option<String> {
    let prefix = format!("{marker}=");
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn remote_codex_action_label(action: &RemoteCodexAction) -> &'static str {
    match action {
        RemoteCodexAction::CheckVersion => "Check Codex version",
        RemoteCodexAction::Install => "Install Codex",
        RemoteCodexAction::Update => "Update Codex",
        RemoteCodexAction::Uninstall => "Uninstall Codex",
    }
}

fn codex_maintenance_task(
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
        started_at: "now".into(),
        ended_at: Some("now".into()),
        summary: summary.to_string(),
        logs,
    }
}

fn apply_profile_to_hosts(
    app: &AppHandle,
    state: &AppState,
    profile: &Profile,
    rendered_toml: &str,
    host_ids: Vec<String>,
    timeout: u64,
) -> ProfileApplyBatchResult {
    let hosts = resolve_apply_hosts(state, &host_ids);
    if hosts.is_empty() {
        let task_id = format!("task-profile-{}", timestamp_millis());
        let task = TaskRun {
            id: task_id.clone(),
            host_id: "no-host".into(),
            host_name: "No host selected".into(),
            action: "Apply profile".into(),
            status: TaskStatus::Failed,
            started_at: "now".into(),
            ended_at: Some("now".into()),
            summary: "No matching hosts were selected for profile apply.".into(),
            logs: vec![basic_log(
                &task_id,
                1,
                TaskLogLevel::Error,
                "No matching hosts were selected for profile apply.",
            )],
        };
        record_task(state, task.clone());
        return ProfileApplyBatchResult {
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
            profiles: profile_apply_profiles_snapshot(app, state),
            hosts: profile_apply_hosts_snapshot(state),
        };
    }

    let results: Vec<ProfileApplyHostResult> = hosts
        .into_iter()
        .map(|host| {
            let task =
                apply_profile_to_host(app, state, profile, rendered_toml, host.clone(), timeout);
            profile_apply_result_from_task(&host, task)
        })
        .collect();
    let tasks = results
        .iter()
        .filter_map(|result| result.task.clone())
        .collect::<Vec<_>>();
    ProfileApplyBatchResult {
        profile_id: profile.id.clone(),
        ok: results.iter().all(|result| result.status != "failed"),
        results,
        tasks,
        profiles: profile_apply_profiles_snapshot(app, state),
        hosts: profile_apply_hosts_snapshot(state),
    }
}

fn resolve_apply_hosts(state: &AppState, host_ids: &[String]) -> Vec<Host> {
    let hosts = state.hosts.lock().expect("hosts mutex poisoned").clone();
    let requested: BTreeSet<String> = host_ids.iter().cloned().collect();
    hosts
        .into_iter()
        .filter(|host| requested.contains(&host.id) || requested.contains(&host.host_alias))
        .collect()
}

fn profile_import_export(profiles: Vec<Profile>) -> ProfileImportExport {
    ProfileImportExport {
        schema_version: 1,
        exported_at: timestamp_label(),
        profiles,
    }
}

fn profile_apply_targets(hosts: &[Host], profile_id: &str) -> Vec<ProfileApplyTargetFile> {
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

fn profile_apply_preview_result(host: &Host, profile_id: &str) -> ProfileApplyHostResult {
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

fn profile_apply_result_from_task(host: &Host, task: TaskRun) -> ProfileApplyHostResult {
    let status = match &task.status {
        TaskStatus::Success if task.summary.contains("already matched") => "no-change",
        TaskStatus::Success => "success",
        TaskStatus::Failed => "failed",
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

fn profile_apply_backup_path_from_task(task: &TaskRun) -> Option<String> {
    task.logs.iter().find_map(|log| {
        log.stdout
            .as_deref()
            .and_then(|stdout| marker_value(stdout, "CODEXHUB_PROFILE_BACKUP"))
            .filter(|value| !value.is_empty())
    })
}

fn configure_profile_remote_api_key(
    app: &AppHandle,
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
        api_key = migrate_cc_switch_api_key_for_profile(app, state, profile).unwrap_or(None);
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

fn is_valid_env_var_name(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

// Writes the selected profile key only to CodexHub-managed remote env files;
// command logs expose paths and change markers, never the key value.
fn remote_profile_api_key_script(env_var: &str, api_key: &str) -> String {
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

fn check_profile_api_env(
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

fn profile_api_env_label(profile: &Profile) -> String {
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
    timeout: u64,
) -> TaskRun {
    let task_id = format!("task-profile-{}", timestamp_millis());
    let mut logs = Vec::new();
    let mut next_log = 1;
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
        record_task(state, task.clone());
        return task;
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
        record_task(state, task.clone());
        return task;
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
                app,
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
        let summary = if ok {
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
        if ok {
            update_host_profile_apply(app, state, &host.id, &alias, profile, api_key_env_present);
        }
        let task = profile_apply_task(
            &task_id,
            &host,
            if ok && remote_env_configured != Some(false) && api_key_env_present != Some(false) {
                TaskStatus::Success
            } else {
                TaskStatus::Failed
            },
            &summary,
            logs,
        );
        record_task(state, task.clone());
        return task;
    }

    let local_path = match write_profile_temp_file(app, &task_id, rendered_toml) {
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
            record_task(state, task.clone());
            return task;
        }
    };
    let remote_tmp = format!("/tmp/codexhub-profile-{task_id}.toml");
    let upload_output = ssh::upload_file(&alias, &local_path, &remote_tmp, timeout)
        .unwrap_or_else(|error| failed_command_output(format!("scp {remote_tmp}"), error));
    let _ = fs::remove_file(&local_path);
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
        record_task(state, task.clone());
        return task;
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
            app,
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

    let summary = if commit_ok {
        update_host_profile_apply(app, state, &host.id, &alias, profile, api_key_env_present);
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
        if commit_ok && remote_env_configured != Some(false) && api_key_env_present != Some(false) {
            TaskStatus::Success
        } else {
            TaskStatus::Failed
        },
        &summary,
        logs,
    );
    record_task(state, task.clone());
    task
}

fn profile_apply_task(
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
        started_at: "now".into(),
        ended_at: Some("now".into()),
        summary: summary.to_string(),
        logs,
    }
}

fn write_profile_temp_file(
    app: &AppHandle,
    task_id: &str,
    rendered_toml: &str,
) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_cache_dir()
        .unwrap_or_else(|_| env::temp_dir())
        .join("profile-apply");
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join(format!("{task_id}.toml"));
    let mut file = fs::File::create(&path).map_err(|error| error.to_string())?;
    file.write_all(rendered_toml.as_bytes())
        .map_err(|error| error.to_string())?;
    Ok(path)
}

fn profile_apply_commit_script(
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

fn profile_apply_metadata_script(metadata_json: &str) -> String {
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

fn safe_reconnect_shell_fragment() -> &'static str {
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
fn safe_reconnect_decision_from_ps(ps_output: &str) -> SafeReconnectDecision {
    let pids = safe_reconnect_candidate_pids(ps_output);
    match pids.as_slice() {
        [pid] => SafeReconnectDecision::Terminate(*pid),
        [] => SafeReconnectDecision::Manual("no-safe-process-match".into()),
        _ => SafeReconnectDecision::Manual("ambiguous-process-match".into()),
    }
}

#[allow(dead_code)]
fn safe_reconnect_candidate_pids(ps_output: &str) -> Vec<u32> {
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
fn is_safe_reconnect_process(comm: &str, args: &str) -> bool {
    let comm_ok = matches!(comm, "codex" | "codex-app-server" | "codex-remote-control");
    let args_lower = args.to_ascii_lowercase();
    comm_ok
        && args_lower.contains("codex")
        && (args_lower.contains("app-server") || args_lower.contains("remote-control"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn update_host_profile_apply(
    app: &AppHandle,
    state: &AppState,
    host_id: &str,
    alias: &str,
    profile: &Profile,
    api_key_env_present: Option<bool>,
) {
    {
        let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");
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
    }
    if let Err(error) = sync_profile_host_links(app, state, &profile.id, host_id, alias) {
        eprintln!("Failed to persist profile host link: {error}");
    }
}

fn sync_profile_host_links(
    app: &AppHandle,
    state: &AppState,
    profile_id: &str,
    host_id: &str,
    alias: &str,
) -> Result<(), String> {
    let mut profiles = load_profiles(app, state)?;
    sync_profile_host_ids(&mut profiles, profile_id, host_id, alias);
    save_profiles(app, state, &profiles)
}

fn clear_profile_host_links(
    app: &AppHandle,
    state: &AppState,
    host_id: &str,
    alias: &str,
) -> Result<(), String> {
    let mut profiles = load_profiles(app, state)?;
    clear_profile_host_ids(&mut profiles, host_id, alias);
    save_profiles(app, state, &profiles)
}

fn sync_profile_host_ids(profiles: &mut [Profile], profile_id: &str, host_id: &str, alias: &str) {
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

fn clear_profile_host_ids(profiles: &mut [Profile], host_id: &str, alias: &str) {
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

fn normalize_host_link_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn profile_apply_profiles_snapshot(app: &AppHandle, state: &AppState) -> Vec<Profile> {
    load_profiles(app, state).unwrap_or_else(|_| {
        state
            .profiles
            .lock()
            .expect("profiles mutex poisoned")
            .clone()
    })
}

fn profile_apply_hosts_snapshot(state: &AppState) -> Vec<Host> {
    let _ = merge_discovered_hosts(state);
    state.hosts.lock().expect("hosts mutex poisoned").clone()
}

fn reconcile_hosts_with_profile_links(state: &AppState, profiles: &[Profile]) {
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
) {
    let mut probed_host_id = None;
    {
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
    }
    if let Some(host_id) = probed_host_id {
        let result = if let Some(profile_id) = api_config_match.profile_id.as_deref() {
            sync_profile_host_links(app, state, profile_id, &host_id, alias)
        } else {
            clear_profile_host_links(app, state, &host_id, alias)
        };
        if let Err(error) = result {
            eprintln!("Failed to persist probed profile host link: {error}");
        }
    }
    if let Err(error) = save_current_hosts(app, state) {
        eprintln!("Failed to persist probed host state: {error}");
    }
}

fn update_host_codex_status(
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

fn stdout_optional_yes(output: Option<&ssh::SshCommandOutput>) -> Option<bool> {
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

fn path_has_local_bin(path: Option<&str>) -> bool {
    path.unwrap_or_default()
        .split(':')
        .any(|segment| segment == "~/.local/bin" || segment.ends_with("/.local/bin"))
}

fn classify_remote_api_config(
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
    if let Ok(detected) = detect_cc_switch_profiles_inner(app, state) {
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

fn normalize_base_url_key(value: &str) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_profile(provider: &str) -> Profile {
        Profile {
            id: "profile-1".into(),
            name: "Profile One".into(),
            description: "Test profile".into(),
            model: "gpt-5-codex".into(),
            provider: provider.into(),
            base_url: if provider == "openai" {
                Some("https://proxy.example/v1".into())
            } else {
                Some("https://models.example/v1".into())
            },
            api_key_env_var: Some("CODEXHUB_TEST_API_KEY".into()),
            model_reasoning_effort: Some("medium".into()),
            plan_mode_reasoning_effort: Some("high".into()),
            fast_mode: true,
            service_tier: Some("auto".into()),
            approval_policy: "on-request".into(),
            sandbox_mode: "workspace-write".into(),
            extra_toml: String::new(),
            created_at: "1".into(),
            updated_at: "1".into(),
            source: "test".into(),
            credential_stored: true,
            host_ids: vec!["host-1".into()],
        }
    }

    fn test_host(alias: &str) -> Host {
        Host {
            id: format!("host-{alias}"),
            name: format!("Host {alias}"),
            host_alias: alias.into(),
            source: "test".into(),
            address: "127.0.0.1".into(),
            port: 22,
            username: "codex".into(),
            auth_method: AuthMethod::SshKey,
            status: HostStatus::Unknown,
            os: String::new(),
            arch: String::new(),
            shell: String::new(),
            path: None,
            path_has_local_bin: None,
            codex_command_available: None,
            codex_installed: false,
            codex_version: String::new(),
            config_exists: None,
            api_config_name: None,
            api_config_source: None,
            api_key_env_var: None,
            api_key_env_present: None,
            skills_exists: None,
            skills_count: None,
            profile_id: None,
            skill_pack_ids: Vec::new(),
            tags: Vec::new(),
            last_seen: "never".into(),
            latency_ms: None,
        }
    }

    #[test]
    fn stable_updater_status_is_pending_without_build_time_config() {
        let status = app_update_status_for_channel("stable", "0.2.0".into(), None, None);
        assert!(matches!(status.state, AppUpdateState::PendingConfiguration));
        assert_eq!(status.current_version, "0.2.0");
        assert!(!status.configured);
        assert!(!status.feed_configured);
        assert!(!status.signing_configured);
        assert!(status.message.contains(STABLE_UPDATE_ENDPOINT_ENV));
        assert!(status.message.contains(STABLE_UPDATER_PUBKEY_ENV));
    }

    #[test]
    fn up_to_date_update_status_reports_current_version_as_latest() {
        let config = StableUpdaterConfig {
            endpoint: Some("https://example.invalid/latest.json".into()),
            pubkey: Some("public-key".into()),
        };
        let status = app_update_status(
            "stable",
            "0.2.0",
            AppUpdateState::UpToDate,
            &config,
            None,
            Some("2026-07-03 15:05:35".into()),
            "CodexHub stable is up to date.".into(),
        );

        assert_eq!(status.latest_version.as_deref(), Some("0.2.0"));
        assert_eq!(status.checked_at.as_deref(), Some("2026-07-03 15:05:35"));
    }

    #[test]
    fn github_release_api_url_supports_latest_and_tagged_feeds() {
        let latest = Url::parse(
            "https://github.com/example-owner/CodexHub/releases/latest/download/latest.json",
        )
        .unwrap();
        let tagged = Url::parse(
            "https://github.com/example-owner/CodexHub/releases/download/v0.2.7/latest.json",
        )
        .unwrap();
        let asset_api =
            Url::parse("https://api.github.com/repos/example-owner/CodexHub/releases/assets/123")
                .unwrap();

        assert_eq!(
            github_release_api_url(&latest).as_deref(),
            Some("https://api.github.com/repos/example-owner/CodexHub/releases/latest")
        );
        assert_eq!(
            github_release_api_url(&tagged).as_deref(),
            Some("https://api.github.com/repos/example-owner/CodexHub/releases/tags/v0.2.7")
        );
        assert!(is_github_release_asset_api_endpoint(&asset_api));
    }

    #[test]
    fn dev_updater_status_is_disabled() {
        let status = app_update_status_for_channel("dev", "0.2.0".into(), None, None);
        assert!(matches!(status.state, AppUpdateState::Disabled));
        assert!(!status.configured);
        assert!(status
            .message
            .contains("Dev channel auto-updates are disabled"));
    }

    #[test]
    fn updater_pubkey_normalization_returns_tauri_pub_file_value() {
        let pubkey = "RWS19HRXxKw1q5/L9ZWqd5uQUpzxp8rDovvj1gMDY7gvZqhaBWrhAeVv";
        let pub_file =
            format!("untrusted comment: minisign public key: AB35ACC45774F4B5\n{pubkey}\n");
        let encoded_pub_file = general_purpose::STANDARD.encode(pub_file.as_bytes());

        assert_eq!(
            normalize_updater_pubkey(pubkey).as_deref(),
            Some(encoded_pub_file.as_str())
        );
        assert_eq!(
            normalize_updater_pubkey(&pub_file).as_deref(),
            Some(encoded_pub_file.as_str())
        );
        assert_eq!(
            normalize_updater_pubkey(&encoded_pub_file).as_deref(),
            Some(encoded_pub_file.as_str())
        );
    }

    #[test]
    fn legacy_app_settings_default_close_button_behavior_to_ask() {
        let settings: AppSettings = serde_json::from_str(
            r#"{
                "theme": "system",
                "fontPreset": "english",
                "platformAppearance": "auto",
                "setupGuideDismissed": true
            }"#,
        )
        .expect("legacy app settings deserialize");

        assert!(matches!(
            settings.close_button_behavior,
            CloseButtonBehavior::Ask
        ));
        assert!(matches!(
            settings.network_proxy_mode,
            NetworkProxyMode::Auto
        ));
        assert!(settings.network_proxy_url.is_empty());
        assert!(settings.resource_monitor_host_order.is_empty());
        assert_eq!(
            serde_json::to_string(&CloseButtonBehavior::MinimizeToTray)
                .expect("serialize behavior"),
            "\"minimize-to-tray\""
        );
    }

    #[test]
    fn network_proxy_detection_redacts_manual_credentials() {
        let settings = AppSettings {
            network_proxy_mode: NetworkProxyMode::Manual,
            network_proxy_url: "http://user:secret@127.0.0.1:9".into(),
            ..Default::default()
        };
        let status = detect_network_proxy_status(&settings);
        let manual = status
            .candidates
            .iter()
            .find(|candidate| candidate.source == "manual")
            .expect("manual candidate");
        let url = manual.url.as_deref().expect("manual URL");

        assert!(url.contains("redacted"));
        assert!(!url.contains("secret"));
        assert_eq!(
            normalize_proxy_url("7890")
                .expect("port proxy")
                .to_string(),
            "http://127.0.0.1:7890/"
        );
    }

    fn empty_state() -> AppState {
        AppState {
            hosts: Mutex::new(Vec::new()),
            profiles: Mutex::new(Vec::new()),
            skill_packs: Mutex::new(Vec::new()),
            tasks: Mutex::new(Vec::new()),
        }
    }

    #[test]
    fn profile_render_uses_builtin_openai_provider_without_custom_provider_table() {
        let toml = render_profile_toml(&test_profile("openai")).expect("render profile");

        assert!(toml.contains("model = \"gpt-5-codex\""));
        assert!(toml.contains("model_provider = \"openai\""));
        assert!(toml.contains("openai_base_url = \"https://proxy.example/v1\""));
        assert!(toml.contains("[features]"));
        assert!(toml.contains("fast_mode = true"));
        assert!(!toml.contains("[model_providers.openai]"));
    }

    #[test]
    fn profile_render_writes_custom_provider_table() {
        let toml = render_profile_toml(&test_profile("zhipu")).expect("render profile");

        assert!(toml.contains("model_provider = \"zhipu\""));
        assert!(toml.contains("[model_providers.zhipu]"));
        assert!(toml.contains("name = \"zhipu\""));
        assert!(toml.contains("base_url = \"https://models.example/v1\""));
        assert!(toml.contains("env_key = \"CODEXHUB_TEST_API_KEY\""));
    }

    #[test]
    fn profile_render_preserves_release_safe_settings_without_secret_values() {
        let mut profile = test_profile("openai");
        profile.service_tier = Some("flex".into());
        profile.approval_policy = "never".into();
        profile.sandbox_mode = "workspace-write".into();
        profile.extra_toml = "[history]\npersistence = \"save-all\"\n".into();

        let toml = render_profile_toml(&profile).expect("render profile");

        assert!(toml.contains("model_reasoning_effort = \"medium\""));
        assert!(toml.contains("plan_mode_reasoning_effort = \"high\""));
        assert!(toml.contains("service_tier = \"flex\""));
        assert!(toml.contains("approval_policy = \"never\""));
        assert!(toml.contains("sandbox_mode = \"workspace-write\""));
        assert!(toml.contains("[history]"));
        assert!(toml.contains("persistence = \"save-all\""));
        assert!(!toml.contains("credentialStored"));
        assert!(!toml.contains("api_key ="));
        assert!(!toml.contains("sk-"));
    }

    #[test]
    fn profile_extra_toml_rejects_structured_conflicts_and_merges_other_values() {
        let mut profile = test_profile("openai");
        profile.extra_toml =
            "[features]\nexperimental_resume = true\n[history]\npersistence = \"save-all\"\n"
                .into();
        let toml = render_profile_toml(&profile).expect("merge non-conflicting extra TOML");
        assert!(toml.contains("experimental_resume = true"));
        assert!(toml.contains("[history]"));

        profile.extra_toml = "model = \"other\"\n".into();
        assert!(render_profile_toml(&profile)
            .expect_err("model conflict")
            .contains("structured key `model`"));

        profile.extra_toml = "[features]\nfast_mode = false\n".into();
        assert!(render_profile_toml(&profile)
            .expect_err("features conflict")
            .contains("features.fast_mode"));

        profile.extra_toml = "[provider]\napi_key = \"not-for-disk\"\n".into();
        assert!(render_profile_toml(&profile)
            .expect_err("secret key")
            .contains("secret-like key `provider.api_key`"));
    }

    #[test]
    fn profile_export_and_render_do_not_include_key_material() {
        let profile = test_profile("zhipu");
        let rendered = render_profile_toml(&profile).expect("render profile");
        let exported = serde_json::to_string(&profile).expect("serialize profile");

        assert!(!rendered.contains("credential"));
        assert!(!rendered.contains("sk-"));
        assert!(!exported.contains("sk-"));
        assert!(!exported.contains("apiKeyValue"));
    }

    #[test]
    fn task_recorder_prepends_and_keeps_logs_redacted() {
        let state = empty_state();
        let fake_key = format!("{}{}", "sk-", "live12345678901234567890");
        let output = ssh::SshCommandOutput {
            command: "ssh lab echo ok".into(),
            stdout: ssh::redact_sensitive(&format!("token={fake_key}\nok")),
            stderr: ssh::redact_sensitive("password=super-secret-value"),
            exit_code: Some(0),
            duration_ms: 12,
            timed_out: false,
        };
        let older = skill_task(
            "task-old",
            "local",
            "Local machine",
            "Install skill",
            TaskStatus::Success,
            "Installed skill.",
            vec![command_log(
                "task-old",
                0,
                TaskLogLevel::Info,
                "install",
                &output,
            )],
        );
        let newer = skill_task(
            "task-new",
            "host-lab",
            "Host lab",
            "Apply profile",
            TaskStatus::Failed,
            "Profile apply failed.",
            vec![basic_log(
                "task-new",
                0,
                TaskLogLevel::Error,
                "remote config rejected",
            )],
        );

        record_task(&state, older);
        record_task(&state, newer);
        let tasks = state.tasks.lock().expect("tasks mutex poisoned").clone();
        let serialized = serde_json::to_string(&tasks).expect("serialize tasks");

        assert_eq!(
            tasks
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            vec!["task-new", "task-old"]
        );
        assert_eq!(
            serde_json::to_string(&tasks[0].status).expect("serialize status"),
            "\"failed\""
        );
        assert!(serialized.contains("token=[redacted]"));
        assert!(serialized.contains("password=[redacted]"));
        assert!(!serialized.contains(&fake_key));
        assert!(!serialized.contains("super-secret-value"));
    }

    #[test]
    fn profile_and_local_skill_tasks_use_expected_public_shapes() {
        let host = test_host("lab");
        let profile_task = profile_apply_task(
            "task-profile-1",
            &host,
            TaskStatus::Success,
            "Applied profile.",
            vec![basic_log("task-profile-1", 0, TaskLogLevel::Info, "done")],
        );
        let skill_task = local_skill_task("Install skill", "Installed local skill.", true);

        assert_eq!(profile_task.action, "Apply profile");
        assert_eq!(profile_task.host_id, "host-lab");
        assert_eq!(
            serde_json::to_string(&profile_task.status).expect("serialize profile status"),
            "\"success\""
        );
        assert_eq!(skill_task.host_id, "local");
        assert_eq!(skill_task.host_name, "Local machine");
        assert_eq!(skill_task.action, "Install skill");
    }

    #[test]
    fn cc_switch_sqlite_profiles_read_codex_providers_without_secrets() {
        let dir = env::temp_dir().join(format!("codexhub-cc-switch-{}", timestamp_millis()));
        fs::create_dir_all(&dir).expect("create temp cc-switch dir");
        let db_path = dir.join("cc-switch.db");
        fs::write(
            dir.join("settings.json"),
            r#"{"currentProviderCodex":"codex-current"}"#,
        )
        .expect("write settings");

        {
            let connection = rusqlite::Connection::open(&db_path).expect("open sqlite fixture");
            connection
                .execute_batch(
                    r#"
                    CREATE TABLE providers (
                        id TEXT NOT NULL,
                        app_type TEXT NOT NULL,
                        name TEXT NOT NULL,
                        settings_config TEXT NOT NULL,
                        website_url TEXT,
                        category TEXT,
                        created_at INTEGER,
                        sort_index INTEGER,
                        notes TEXT,
                        icon TEXT,
                        icon_color TEXT,
                        meta TEXT NOT NULL DEFAULT '{}',
                        is_current BOOLEAN NOT NULL DEFAULT 0,
                        in_failover_queue BOOLEAN NOT NULL DEFAULT 0,
                        PRIMARY KEY (id, app_type)
                    );
                    CREATE TABLE provider_endpoints (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        provider_id TEXT NOT NULL,
                        app_type TEXT NOT NULL,
                        url TEXT NOT NULL,
                        added_at INTEGER
                    );
                    "#,
                )
                .expect("create cc-switch schema");
            let current_config = "model = \"gpt-5.5\"\nmodel_provider = \"custom\"\nmodel_reasoning_effort = \"xhigh\"\n[features]\nfast_mode = true\n[model_providers.custom]\nname = \"custom\"\n";
            let other_config = "model = \"gpt-5-codex\"\nmodel_provider = \"custom\"\n[model_providers.custom]\nbase_url = \"https://config.example/v1\"\nenv_key = \"REMOTE_API_KEY\"\n";
            let current_settings = serde_json::json!({
                "auth": { "OPENAI_API_KEY": "sk-test-secret", "auth_mode": "api_key" },
                "config": current_config
            })
            .to_string();
            let other_settings = serde_json::json!({ "config": other_config }).to_string();
            let claude_settings =
                serde_json::json!({ "config": "model = \"claude\"\n" }).to_string();
            connection
                .execute(
                    "INSERT INTO providers (id, app_type, name, settings_config, website_url, category, is_current) VALUES (?1, 'codex', ?2, ?3, NULL, 'custom', 0)",
                    rusqlite::params!["codex-current", "Current Codex", current_settings],
                )
                .expect("insert current codex");
            connection
                .execute(
                    "INSERT INTO providers (id, app_type, name, settings_config, website_url, category, is_current) VALUES (?1, 'codex', ?2, ?3, NULL, 'custom', 0)",
                    rusqlite::params!["codex-other", "Other Codex", other_settings],
                )
                .expect("insert other codex");
            connection
                .execute(
                    "INSERT INTO providers (id, app_type, name, settings_config, website_url, category, is_current) VALUES (?1, 'claude', ?2, ?3, NULL, 'custom', 0)",
                    rusqlite::params!["claude-provider", "Claude", claude_settings],
                )
                .expect("insert non-codex");
            connection
                .execute(
                    "INSERT INTO provider_endpoints (provider_id, app_type, url, added_at) VALUES (?1, 'codex', ?2, 1)",
                    rusqlite::params!["codex-current", "https://endpoint.example/v1"],
                )
                .expect("insert endpoint");
        }

        let profiles = parse_cc_switch_sqlite_profiles(&db_path).expect("parse sqlite profiles");
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0].profile.name, "Current Codex");
        assert_eq!(
            profiles[0].profile.base_url.as_deref(),
            Some("https://endpoint.example/v1")
        );
        assert_eq!(profiles[0].profile.provider, "custom");
        assert_eq!(
            profiles[0].profile.model_reasoning_effort.as_deref(),
            Some("xhigh")
        );
        assert!(profiles[0].profile.fast_mode);
        assert_eq!(profiles[0].api_key.as_deref(), Some("sk-test-secret"));
        assert_eq!(
            profiles[1].profile.api_key_env_var.as_deref(),
            Some("REMOTE_API_KEY")
        );
        let serialized = serde_json::to_string(&profiles[0].profile).expect("serialize profile");
        assert!(!serialized.contains("sk-test-secret"));
        assert!(profiles
            .iter()
            .all(|record| !record.profile.credential_stored));

        let _ = fs::remove_file(&db_path);
        let _ = fs::remove_file(dir.join("settings.json"));
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn keyring_missing_entry_message_is_not_a_hard_failure() {
        assert!(is_missing_credential_error(
            "No matching entry found in secure storage"
        ));
    }

    #[test]
    fn cc_switch_raw_db_recovery_extracts_config_without_auth() {
        let settings = serde_json::json!({
            "auth": { "api_key": "sk-raw-secret" },
            "config": "model = \"gpt-5.5\"\nmodel_provider = \"custom\"\n[model_providers.custom]\nbase_url = \"https://raw.example/v1\"\n"
        })
        .to_string();
        let content = format!(
            "noise 891d8cb1-69b8-4cac-9368-4944b1ec1735codexRaw Provider{settings}https://fallback.example/v1custom"
        );
        let profiles = parse_cc_switch_raw_db_profiles(&content, Path::new("cc-switch.db"));

        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].profile.name, "Raw Provider");
        assert_eq!(
            profiles[0].profile.base_url.as_deref(),
            Some("https://raw.example/v1")
        );
        assert_eq!(
            profiles[0].profile.api_key_env_var.as_deref(),
            Some("OPENAI_API_KEY")
        );
        assert_eq!(profiles[0].api_key.as_deref(), Some("sk-raw-secret"));
        let serialized = serde_json::to_string(&profiles[0].profile).expect("serialize profile");
        assert!(!serialized.contains("sk-raw-secret"));
    }

    #[test]
    fn profile_apply_script_checks_no_change_before_backup() {
        let script = profile_apply_commit_script(
            "/tmp/codexhub-profile-test.toml",
            "abc123",
            42,
            "{\"profileId\":\"profile-1\"}",
            "12345",
        );
        let cmp_index = script
            .find("cmp -s \"$config\" \"$staged\"")
            .expect("cmp guard");
        let backup_index = script
            .find("cp -p \"$config\" \"$backup\"")
            .expect("backup command");

        assert!(cmp_index < backup_index);
        assert!(script.contains("CODEXHUB_PROFILE_BACKUP"));
        assert!(script.contains("CODEXHUB_PROFILE_VALIDATION"));
    }

    #[test]
    fn profile_apply_metadata_contains_expected_identity() {
        let metadata = AppliedProfileMetadata {
            profile_id: "profile-1".into(),
            profile_name: "Profile One".into(),
            applied_at: "12345".into(),
            codexhub_version: "0.1.0".into(),
        };
        let json = serde_json::to_string(&metadata).expect("serialize metadata");

        assert!(json.contains("\"profileId\":\"profile-1\""));
        assert!(json.contains("\"profileName\":\"Profile One\""));
        assert!(json.contains("\"codexhubVersion\":\"0.1.0\""));
    }

    #[test]
    fn npm_latest_metadata_parser_extracts_dist_tag_latest() {
        let metadata = r#"{
          "dist-tags": { "latest": "0.142.2", "beta": "0.143.0-beta.1" }
        }"#;

        let latest = parse_npm_latest_metadata(metadata).expect("parse npm latest");

        assert_eq!(latest, "0.142.2");
    }

    #[test]
    fn npm_latest_metadata_parser_rejects_html_missing_and_unsafe_values() {
        assert!(parse_npm_latest_metadata("<html>login</html>").is_err());
        assert!(parse_npm_latest_metadata(r#"{"dist-tags":{}}"#).is_err());
        assert!(parse_npm_latest_metadata(r#"{"dist-tags":{"latest":"0.1.0;rm -rf /"}}"#).is_err());
    }

    #[test]
    fn latest_codex_cache_refreshes_after_daily_four_am_boundary() {
        let now = DateTime::parse_from_rfc3339("2026-06-28T05:00:00+08:00").expect("now");
        let fresh = LatestCodexVersion {
            version: Some("0.142.2".into()),
            checked_at: Some("2026-06-28T04:01:00+08:00".into()),
            source: CODEX_LATEST_SOURCE.into(),
            error: None,
        };
        let stale = LatestCodexVersion {
            checked_at: Some("2026-06-28T03:59:00+08:00".into()),
            ..fresh.clone()
        };

        assert!(latest_codex_cache_is_fresh(&fresh, now));
        assert!(!latest_codex_cache_is_fresh(&stale, now));
    }

    #[test]
    fn latest_codex_cache_uses_previous_day_boundary_before_four_am() {
        let now = DateTime::parse_from_rfc3339("2026-06-28T03:00:00+08:00").expect("now");
        let fresh = LatestCodexVersion {
            version: Some("0.142.2".into()),
            checked_at: Some("2026-06-27T04:01:00+08:00".into()),
            source: CODEX_LATEST_SOURCE.into(),
            error: None,
        };
        let stale = LatestCodexVersion {
            checked_at: Some("2026-06-27T03:59:00+08:00".into()),
            ..fresh.clone()
        };

        assert!(latest_codex_cache_is_fresh(&fresh, now));
        assert!(!latest_codex_cache_is_fresh(&stale, now));
    }

    #[test]
    fn latest_codex_cache_requires_version_and_checked_at() {
        let now = DateTime::parse_from_rfc3339("2026-06-28T05:00:00+08:00").expect("now");
        let missing_version = LatestCodexVersion {
            version: None,
            checked_at: Some("2026-06-28T04:01:00+08:00".into()),
            source: CODEX_LATEST_SOURCE.into(),
            error: None,
        };
        let missing_checked_at = LatestCodexVersion {
            version: Some("0.142.2".into()),
            checked_at: None,
            source: CODEX_LATEST_SOURCE.into(),
            error: None,
        };

        assert!(!latest_codex_cache_is_fresh(&missing_version, now));
        assert!(!latest_codex_cache_is_fresh(&missing_checked_at, now));
    }

    #[test]
    fn latest_codex_fetch_failure_returns_cached_version_or_error_only_result() {
        let now = DateTime::parse_from_rfc3339("2026-06-28T05:00:00+08:00").expect("now");
        let cache = LatestCodexVersion {
            version: Some("0.142.2".into()),
            checked_at: Some("2026-06-28T04:01:00+08:00".into()),
            source: CODEX_LATEST_SOURCE.into(),
            error: None,
        };

        let stale = latest_codex_result_from_fetch(Err("offline".into()), Some(cache.clone()), now);
        let empty = latest_codex_result_from_fetch(Err("offline".into()), None, now);

        assert_eq!(stale.version, cache.version);
        assert_eq!(stale.checked_at, cache.checked_at);
        assert_eq!(stale.error.as_deref(), Some("offline"));
        assert_eq!(empty.version, None);
        assert_eq!(empty.checked_at, None);
        assert_eq!(empty.error.as_deref(), Some("offline"));
    }

    #[test]
    fn skill_metadata_parser_reads_frontmatter_and_falls_back_to_directory_name() {
        let content =
            "---\nname: \"Example Skill\"\ndescription: Run example workflow\nversion: '0.4.2'\n---\nBody";
        let parsed =
            parse_skill_metadata(content, Path::new("example-skill")).expect("parse skill");

        assert_eq!(parsed.name, "Example Skill");
        assert_eq!(parsed.description.as_deref(), Some("Run example workflow"));
        assert_eq!(parsed.version.as_deref(), Some("0.4.2"));

        let fallback = parse_skill_metadata("# Instructions", Path::new("draft-helper"))
            .expect("fallback skill");
        assert_eq!(fallback.name, "draft-helper");
        let description_only = parse_skill_metadata(
            "---\ndescription: Description only\n---\nBody",
            Path::new("helper"),
        )
        .expect("description-only skill");
        assert_eq!(description_only.name, "helper");
        assert_eq!(
            description_only.description.as_deref(),
            Some("Description only")
        );
        assert!(parse_skill_metadata("", Path::new("empty")).is_err());
    }

    #[test]
    fn skill_ids_and_remote_names_reject_unsafe_values() {
        assert_eq!(
            safe_skill_id("Example Skill++").expect("slug"),
            "example-skill"
        );
        assert_eq!(
            safe_skill_id("owner/repo").expect("github slug"),
            "owner-repo"
        );
        assert!(safe_skill_id("!!!").is_err());

        assert_eq!(
            validate_remote_skill_dir_name("Paper_Review-1.2").expect("remote name"),
            "Paper_Review-1.2"
        );
        assert!(validate_remote_skill_dir_name("../secret").is_err());
        assert!(validate_remote_skill_dir_name("paper review").is_err());
        assert!(validate_remote_skill_dir_name(".").is_err());
    }

    #[test]
    fn skill_candidate_scan_uses_root_or_immediate_children() {
        let root = env::temp_dir().join(format!("codexhub-skill-scan-{}", timestamp_millis()));
        let child_a = root.join("example-skill");
        let child_b = root.join("no-skill");
        let nested = root.join("nested").join("deep-skill");
        fs::create_dir_all(&child_a).expect("create child skill");
        fs::create_dir_all(&child_b).expect("create child without skill");
        fs::create_dir_all(&nested).expect("create nested skill");
        fs::write(child_a.join("SKILL.md"), "# Paper").expect("write child skill");
        fs::write(nested.join("SKILL.md"), "# Deep").expect("write nested skill");

        let candidates = skill_candidate_dirs(&root).expect("scan children");
        assert_eq!(candidates, vec![child_a.clone()]);

        fs::write(root.join("SKILL.md"), "# Root").expect("write root skill");
        let root_candidates = skill_candidate_dirs(&root).expect("scan root");
        assert_eq!(root_candidates, vec![root.clone()]);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn installed_skill_scan_includes_second_level_system_skills() {
        let root = env::temp_dir().join(format!(
            "codexhub-installed-skill-scan-{}",
            timestamp_millis()
        ));
        let local = root.join("pdf");
        let system = root.join(".system").join("imagegen");
        let too_deep = root.join("nested").join("deep").join("ignored");
        fs::create_dir_all(&local).expect("create local skill");
        fs::create_dir_all(&system).expect("create system skill");
        fs::create_dir_all(&too_deep).expect("create deep skill");
        fs::write(local.join("SKILL.md"), "# PDF").expect("write local skill");
        fs::write(system.join("SKILL.md"), "# Image").expect("write system skill");
        fs::write(too_deep.join("SKILL.md"), "# Deep").expect("write deep skill");

        let candidates = installed_skill_candidate_dirs(&root).expect("scan installed skills");

        assert!(candidates.contains(&local));
        assert!(candidates.contains(&system));
        assert!(!candidates.contains(&too_deep));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn github_download_url_parser_is_strict_and_supports_tree_paths() {
        assert!(is_allowed_github_repo_url(
            "https://github.com/owner/example-skill"
        ));
        assert!(is_allowed_github_repo_url(
            "https://github.com/owner/example-skill.git"
        ));
        assert!(is_allowed_github_repo_url(
            "https://github.com/openai/skills/tree/main/skills/.curated/winui-app"
        ));
        assert!(!is_allowed_github_repo_url(
            "git@github.com:owner/example-skill.git"
        ));
        assert!(!is_allowed_github_repo_url("https://github.com/owner"));
        assert!(!is_allowed_github_repo_url(
            "https://github.com/owner/example/extra"
        ));
        assert!(!is_allowed_github_repo_url(
            "https://github.com/openai/skills/tree/main/../secret"
        ));
        assert!(!is_allowed_github_repo_url(
            "https://github.com/openai/skills/tree/main/skills//bad"
        ));
        let repo_url = parse_github_skill_url("https://github.com/owner/example-skill.git")
            .expect("parse repo url");
        assert_eq!(repo_url.owner, "owner");
        assert_eq!(repo_url.repo, "example-skill");
        let tree_url = parse_github_skill_url(
            "https://github.com/openai/skills/tree/main/skills/.curated/winui-app",
        )
        .expect("parse tree url");
        assert_eq!(tree_url.owner, "openai");
        assert_eq!(tree_url.repo, "skills");
        assert_eq!(tree_url.clone_url, "https://github.com/openai/skills.git");
        assert_eq!(tree_url.branch.as_deref(), Some("main"));
        assert_eq!(
            tree_url.skill_subpath.as_deref(),
            Some(Path::new("skills/.curated/winui-app"))
        );
    }

    #[test]
    fn ensure_child_path_allows_children_and_rejects_siblings() {
        let root = env::temp_dir().join(format!("codexhub-child-path-{}", timestamp_millis()));
        let child = root.join("managed").join("example-skill");
        let sibling = root
            .parent()
            .expect("temp root parent")
            .join(format!("codexhub-sibling-{}", timestamp_millis()));
        fs::create_dir_all(&child).expect("create child");
        fs::create_dir_all(&sibling).expect("create sibling");

        assert!(ensure_child_path(&root, &child).is_ok());
        assert!(ensure_child_path(&root, &sibling).is_err());

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&sibling);
    }

    #[test]
    fn remote_project_skill_paths_require_absolute_or_home_roots() {
        let (home_expr, home_display) =
            remote_skill_root(&RemoteSkillScope::Project, Some("~/work/repo"))
                .expect("home project path");
        assert_eq!(home_expr, "$HOME/'work/repo'/.codex/skills");
        assert_eq!(home_display, "~/work/repo/.codex/skills");

        let (absolute_expr, absolute_display) =
            remote_skill_root(&RemoteSkillScope::Project, Some("/srv/repo"))
                .expect("absolute project path");
        assert_eq!(absolute_expr, "'/srv/repo'/.codex/skills");
        assert_eq!(absolute_display, "/srv/repo/.codex/skills");

        assert!(remote_skill_root(&RemoteSkillScope::Project, Some("relative/repo")).is_err());
        assert!(remote_skill_root(&RemoteSkillScope::Project, Some("~/")).is_err());
        assert!(remote_skill_root(&RemoteSkillScope::Project, Some("/srv/repo\nbad")).is_err());
    }

    #[test]
    fn remote_skill_list_parser_extracts_validity_and_paths() {
        let stdout = "CODEXHUB_SKILL_ROOT=/home/test/.codex/skills\n\
CODEXHUB_REMOTE_SKILL\texample-skill\tyes\tvalid\t/home/test/.codex/skills/example-skill\tRun example workflow\n\
CODEXHUB_REMOTE_SKILL\tdraft-helper\tno\tmissing-skill-md\t/home/test/.codex/skills/draft-helper\t\n";

        let skills = parse_remote_skill_list(stdout);

        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].name, "example-skill");
        assert_eq!(skills[0].description, "Run example workflow");
        assert!(skills[0].has_skill_md);
        assert_eq!(skills[1].status, "missing-skill-md");
        assert!(!skills[1].has_skill_md);
    }

    #[test]
    fn remote_skill_list_parser_accepts_space_delimited_output() {
        let stdout = "CODEXHUB_SKILL_ROOT=/home/jy/.codex/skills\n\
CODEXHUB_REMOTE_SKILL imagegen yes valid /home/jy/.codex/skills/.system/imagegen Generate or edit raster images\n\
CODEXHUB_REMOTE_SKILL openai-docs yes valid /home/jy/.codex/skills/.system/openai-docs\n\
CODEXHUB_SKILL_ROOT=/home/jy/.codex/superpowers/skills\n\
CODEXHUB_REMOTE_SKILL brainstorming yes valid /home/jy/.codex/superpowers/skills/brainstorming\n\
CODEXHUB_SKILL_COUNT=3\n";

        let skills = parse_remote_skill_list(stdout);

        assert_eq!(skills.len(), 3);
        assert_eq!(skills[0].name, "imagegen");
        assert_eq!(skills[0].description, "Generate or edit raster images");
        assert_eq!(
            skills[2].path,
            "/home/jy/.codex/superpowers/skills/brainstorming"
        );
        assert!(skills.iter().all(|skill| skill.has_skill_md));
    }

    #[test]
    fn remote_skill_list_parser_deduplicates_paths() {
        let stdout = "CODEXHUB_REMOTE_SKILL\timagegen\tyes\tvalid\t/home/test/.codex/skills/.system/imagegen\n\
CODEXHUB_REMOTE_SKILL\timagegen\tyes\tvalid\t/home/test/.codex/skills/.system/imagegen\n";

        let skills = parse_remote_skill_list(stdout);

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "imagegen");
    }

    #[test]
    fn remote_skill_list_script_scans_hidden_second_level_skills() {
        let script = remote_skill_list_script();

        assert!(script.contains("\"$root\"/.[!.]*"));
        assert!(script.contains("\"$root\"/..?*"));
        assert!(script.contains("\"$dir\"/.[!.]*"));
        assert!(script.contains("\"$dir\"/..?*"));
        assert!(script.contains("$HOME/.codex/superpowers/skills"));
        assert!(script.contains("scan_child_dir"));
        assert!(script.contains("scan_root \"$HOME/.codex/skills\""));
        assert!(script.contains("scan_root \"$HOME/.codex/superpowers/skills\""));
        assert!(script.contains("emit_skill_dir \"$nested\""));
        assert!(script.contains("extract_skill_description"));
        assert!(script.contains("description=$(extract_skill_description \"$dir\")"));
        assert!(script.contains("CODEXHUB_REMOTE_SKILL\\t%s\\t%s\\t%s\\t%s\\t%s"));
        assert!(script.contains("scan_find_fallback"));
        assert!(script.contains("find \"$root\" -mindepth 1 -maxdepth 3 -type f -name SKILL.md"));
        assert!(!script.contains("roots=\""));
    }

    #[test]
    fn remote_skill_count_script_matches_hidden_second_level_scan() {
        let script = remote_skill_count_script();

        assert!(script.contains("\"$root\"/.[!.]*"));
        assert!(script.contains("\"$root\"/..?*"));
        assert!(script.contains("\"$dir\"/.[!.]*"));
        assert!(script.contains("\"$dir\"/..?*"));
        assert!(script.contains("$HOME/.codex/superpowers/skills"));
        assert!(script.contains("scan_root \"$HOME/.codex/skills\""));
        assert!(script.contains("scan_root \"$HOME/.codex/superpowers/skills\""));
        assert!(script.contains("count_skill_dir \"$nested\""));
        assert!(!script.contains("roots=\""));
        assert!(!script.contains("find \"$HOME/.codex/skills\" -mindepth 1 -maxdepth 1"));
    }

    #[test]
    fn remote_skill_install_scripts_encode_conflict_policies_and_safety_guards() {
        let backup = remote_skill_install_script(
            "/tmp/skill.tgz",
            "$HOME/.codex/skills",
            "example-skill",
            &SkillConflictPolicy::Backup,
            "12345",
        );
        assert!(backup.contains("policy='backup'"));
        assert!(backup.contains("tar is required on the remote host"));
        assert!(backup.contains("grep -Eq '(^|/)\\.\\.(/|$)|^/'"));
        assert!(backup.contains("mv \"$target\" \"$backup\""));
        assert!(backup.contains("CODEXHUB_SKILL_BACKUP"));
        assert!(!backup.contains("sudo "));

        let skip = remote_skill_install_script(
            "/tmp/skill.tgz",
            "$HOME/.codex/skills",
            "example-skill",
            &SkillConflictPolicy::Skip,
            "12345",
        );
        assert!(skip.contains("policy='skip'"));
        assert!(skip.contains("skipped=yes"));

        let overwrite = remote_skill_install_script(
            "/tmp/skill.tgz",
            "$HOME/.codex/skills",
            "example-skill",
            &SkillConflictPolicy::Overwrite,
            "12345",
        );
        assert!(overwrite.contains("policy='overwrite'"));
        assert!(overwrite.contains("rm -rf \"$target\""));
    }

    #[test]
    fn remote_skill_delete_script_hard_deletes_after_directory_check() {
        let script = remote_skill_delete_script("$HOME/.codex/skills", "example-skill", "12345");

        assert!(script.contains("rm -rf \"$target\""));
        assert!(script.contains("CODEXHUB_SKILL_COUNT"));
        assert!(!script.contains("codexhub.deleted.$timestamp"));
        assert!(!script.contains("mv \"$target\" \"$backup\""));
        assert!(!script.contains("sudo "));
    }

    #[test]
    fn installed_skill_download_script_packages_exact_cached_path() {
        let script = remote_installed_skill_archive_script(
            "/home/me/.codex/superpowers/skills/.system/example-skill",
            "/tmp/codexhub-skill-download-test.tgz",
        );

        assert!(
            script.contains("target='/home/me/.codex/superpowers/skills/.system/example-skill'")
        );
        assert!(script.contains("tar -czf \"$archive\" -C \"$parent\" \"$base\""));
        assert!(script.contains("CODEXHUB_SKILL_ARCHIVE"));
        assert!(!script.contains("sudo "));
    }

    #[test]
    fn installed_skill_delete_script_hard_deletes_exact_cached_path() {
        let script = remote_installed_skill_delete_script("/home/me/.codex/skills/example-skill");

        assert!(script.contains("target='/home/me/.codex/skills/example-skill'"));
        assert!(script.contains("rm -rf \"$target\""));
        assert!(script.contains("CODEXHUB_SKILL_COUNT"));
        assert!(!script.contains("codexhub.deleted"));
        assert!(!script.contains("sudo "));
    }

    #[test]
    fn profile_apply_host_link_moves_between_profiles_and_dedupes_alias() {
        let mut profiles = vec![
            Profile {
                id: "old-profile".into(),
                name: "Old".into(),
                host_ids: vec!["lab-alias".into(), "other-host".into()],
                ..test_profile("openai")
            },
            Profile {
                id: "new-profile".into(),
                name: "New".into(),
                host_ids: vec!["host-42".into()],
                ..test_profile("openai")
            },
        ];

        sync_profile_host_ids(&mut profiles, "new-profile", "host-42", "lab-alias");
        sync_profile_host_ids(&mut profiles, "new-profile", "host-42", "LAB-ALIAS");

        assert_eq!(profiles[0].host_ids, vec!["other-host"]);
        assert_eq!(profiles[1].host_ids, vec!["host-42"]);
    }

    #[test]
    fn probe_unknown_config_clears_profile_host_link() {
        let mut profiles = vec![
            Profile {
                id: "profile-1".into(),
                name: "Known".into(),
                host_ids: vec!["host-42".into(), "lab-alias".into(), "other-host".into()],
                ..test_profile("openai")
            },
            Profile {
                id: "profile-2".into(),
                name: "Other".into(),
                host_ids: vec!["LAB-ALIAS".into()],
                ..test_profile("openai")
            },
        ];

        clear_profile_host_ids(&mut profiles, "host-42", "lab-alias");

        assert_eq!(profiles[0].host_ids, vec!["other-host"]);
        assert!(profiles[1].host_ids.is_empty());
    }

    #[test]
    fn safe_reconnect_matching_is_single_process_only() {
        let one = "123 codex /home/me/.local/bin/codex app-server --port 1234\n999 bash bash\n";
        assert_eq!(
            safe_reconnect_decision_from_ps(one),
            SafeReconnectDecision::Terminate(123)
        );

        let ambiguous = "123 codex codex app-server\n124 codex codex remote-control\n";
        assert_eq!(
            safe_reconnect_decision_from_ps(ambiguous),
            SafeReconnectDecision::Manual("ambiguous-process-match".into())
        );

        let unsafe_match = "222 node node app-server\n333 codex codex login\n";
        assert_eq!(
            safe_reconnect_decision_from_ps(unsafe_match),
            SafeReconnectDecision::Manual("no-safe-process-match".into())
        );
    }

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

    #[test]
    fn remote_codex_action_serializes_as_kebab_case() {
        assert_eq!(
            serde_json::to_string(&RemoteCodexAction::CheckVersion).expect("serialize"),
            "\"check-version\""
        );
        assert_eq!(
            serde_json::to_string(&RemoteCodexAction::Install).expect("serialize"),
            "\"install\""
        );
        assert_eq!(
            serde_json::from_str::<RemoteCodexAction>("\"update\"").expect("deserialize"),
            RemoteCodexAction::Update
        );
        assert_eq!(
            serde_json::to_string(&RemoteCodexAction::Uninstall).expect("serialize"),
            "\"uninstall\""
        );
    }

    #[test]
    fn remote_codex_progress_event_serializes_camel_case() {
        let event = RemoteCodexProgressEvent {
            request_id: "req-1".into(),
            host_alias: "lab".into(),
            action: RemoteCodexAction::Install,
            step: "Install Codex".into(),
            status: "stdout".into(),
            message: "downloading".into(),
            detail: Some("detail".into()),
            stdout: Some("line".into()),
            stderr: None,
            exit_code: Some(0),
            duration_ms: Some(42),
            timed_out: Some(false),
        };
        let json = serde_json::to_string(&event).expect("serialize progress");

        assert!(json.contains("\"requestId\":\"req-1\""));
        assert!(json.contains("\"hostAlias\":\"lab\""));
        assert!(json.contains("\"exitCode\":0"));
        assert!(json.contains("\"durationMs\":42"));
    }

    #[test]
    fn codex_install_script_uses_user_install_dir_and_mirror_fallback() {
        assert!(CODEX_INSTALL_SCRIPT.contains("CODEX_INSTALL_DIR=\"$HOME/.local/bin\""));
        assert!(CODEX_INSTALL_SCRIPT.contains("CODEX_HOME=\"$HOME/.codex\""));
        assert!(CODEX_INSTALL_SCRIPT.contains("CODEX_NON_INTERACTIVE=1"));
        assert!(CODEX_INSTALL_SCRIPT.contains("https://chatgpt.com/codex/install.sh"));
        assert!(CODEX_INSTALL_SCRIPT.contains("registry=https://registry.npmmirror.com"));
        assert!(CODEX_INSTALL_SCRIPT.contains("https://registry.npmmirror.com/@openai/codex"));
        assert!(CODEX_INSTALL_SCRIPT.contains("CODEXHUB_INSTALL_METHOD=npm-mirror-native"));
        assert!(
            CODEX_INSTALL_SCRIPT.contains("CODEXHUB_INSTALL_METHOD=npm-mirror-native-insecure-tls")
        );
        assert!(CODEX_INSTALL_SCRIPT
            .contains("CodexHub will not disable TLS verification for the official installer"));
        assert!(CODEX_INSTALL_SCRIPT.contains("Insecure TLS fallback is limited to npmmirror URLs"));
        assert!(
            CODEX_INSTALL_SCRIPT.contains("npmmirror metadata response was HTML instead of JSON")
        );
        assert!(CODEX_INSTALL_SCRIPT.contains("not a readable gzip tarball"));
        assert!(CODEX_INSTALL_SCRIPT.contains("archive contains unsafe paths"));
        assert!(CODEX_INSTALL_SCRIPT.contains("curl -k -fsSL"));
        assert!(CODEX_INSTALL_SCRIPT.contains("wget --no-check-certificate"));
        assert!(CODEX_INSTALL_SCRIPT.contains("vendor/$target"));
        assert!(CODEX_INSTALL_SCRIPT.contains("ln -sfn \"$release_dir/bin/codex\""));
        assert!(CODEX_INSTALL_SCRIPT.contains("command -v npm"));
        assert!(!CODEX_INSTALL_SCRIPT.contains("sudo"));
        assert!(!CODEX_INSTALL_SCRIPT.contains("chown"));
        assert!(!CODEX_INSTALL_SCRIPT.contains("/usr/local/bin"));
    }

    #[test]
    fn local_npmmirror_metadata_parser_detects_captive_portal_html() {
        let error = parse_npmmirror_native_metadata(
            r#"<html><body>Authentication is required. https://net2.zju.edu.cn/index_85.html</body></html>"#,
            "linux-x64",
        )
        .expect_err("captive portal should be rejected");

        assert!(error.contains("HTML instead of JSON"));
        assert!(error.contains("captive portal"));
    }

    #[test]
    fn local_npmmirror_metadata_parser_extracts_platform_tarball() {
        let metadata = r#"{
          "dist-tags": { "latest": "0.142.2" },
          "versions": {
            "0.142.2-linux-x64": {
              "dist": {
                "tarball": "https://registry.npmmirror.com/@openai/codex/-/codex-0.142.2-linux-x64.tgz"
              }
            }
          }
        }"#;

        let (version, tarball) =
            parse_npmmirror_native_metadata(metadata, "linux-x64").expect("parse metadata");

        assert_eq!(version, "0.142.2");
        assert_eq!(
            tarball,
            "https://registry.npmmirror.com/@openai/codex/-/codex-0.142.2-linux-x64.tgz"
        );
    }

    #[test]
    fn uploaded_codex_package_script_uses_user_install_dir_without_wrapper() {
        let script = codex_install_uploaded_package_script(
            "/tmp/codexhub-codex-1-0.142.2.tgz",
            "0.142.2",
            "x86_64-unknown-linux-musl",
        );

        assert!(script.contains("CODEX_INSTALL_DIR=\"$HOME/.local/bin\""));
        assert!(script.contains("CODEX_HOME=\"$HOME/.codex\""));
        assert!(script.contains("CODEXHUB_INSTALL_METHOD=npm-mirror-native-local-upload"));
        assert!(script.contains("ln -sfn \"$release_dir/bin/codex\" \"$CODEX_INSTALL_DIR/codex\""));
        assert!(script.contains("vendor/$target"));
        assert!(script.contains("archive contains unsafe paths"));
        assert!(!script.contains("sudo"));
        assert!(!script.contains("/usr/local/bin"));
        assert!(!script.contains("wrapper"));
    }

    #[test]
    fn codex_path_repair_script_is_managed_idempotent_and_backed_up() {
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("mkdir -p \"$local_bin\""));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("# >>> CodexHub managed PATH"));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("# <<< CodexHub managed PATH"));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("changed=no"));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("changed=yes"));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("repair_path_file \"$shell_config\""));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("repair_path_file \"$HOME/.profile\""));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("repair_path_file \"$HOME/.bash_profile\""));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("repair_path_file \"$HOME/.zprofile\""));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("cp -p \"$target\" \"$backup_path\""));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("grep -F \"$path_line\""));
        assert!(CODEX_PATH_REPAIR_SCRIPT.contains("CODEXHUB_PATH_CHANGED=%s"));
        assert!(!CODEX_PATH_REPAIR_SCRIPT.contains("sudo"));

        let backup_index = CODEX_PATH_REPAIR_SCRIPT
            .find("cp -p \"$target\" \"$backup_path\"")
            .expect("backup command");
        let append_index = CODEX_PATH_REPAIR_SCRIPT
            .find(">>\"$target\"")
            .expect("append command");
        assert!(backup_index < append_index);
    }

    #[test]
    fn remote_profile_api_key_script_writes_managed_env_and_launcher() {
        let script = remote_profile_api_key_script("OPENAI_API_KEY", "sk-test'value");

        assert!(script.contains("env_file=\"$env_dir/env\""));
        assert!(script.contains("printf 'export %s=%s\\n' \"$env_name\" \"$env_value\""));
        assert!(script.contains("env_value='"));
        assert!(script.contains("\"'\""));
        assert!(!script.contains("env_value='sk-test'value'"));
        assert!(script.contains("chmod 600 \"$env_file\""));
        assert!(script.contains("repair_source_file \"$HOME/.profile\""));
        assert!(script.contains("repair_source_file \"$HOME/.bash_profile\""));
        assert!(script.contains("repair_source_file \"$HOME/.zprofile\""));
        assert!(script.contains("CodexHub managed launcher"));
        assert!(script.contains("target_file=\"$HOME/.codex-hub/codex-target\""));
        assert!(script.contains("exec \"$target\" \"$@\""));
        assert!(script.contains("CODEXHUB_REMOTE_ENV_CHANGED=%s"));
        assert!(script.contains("CODEXHUB_CODEX_LAUNCHER_CHANGED=%s"));
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            setup_window_chrome(app.handle())?;
            setup_app_tray(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            handle_window_close_request(window, event);
        })
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            app_health,
            get_app_update_status,
            check_stable_update,
            install_stable_update,
            get_settings,
            save_settings,
            detect_network_proxy,
            choose_close_button_behavior,
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
            sample_host_resources,
            remote_manage_codex,
            refresh_latest_codex_version,
            get_local_codex_status,
            list_profiles,
            create_profile,
            update_profile,
            delete_profile,
            duplicate_profile,
            import_profiles,
            set_profile_api_key,
            get_profile_api_key,
            delete_profile_api_key,
            preview_profile_apply,
            apply_profile,
            detect_cc_switch_profiles,
            import_cc_switch_profiles,
            list_local_skills,
            import_local_skill,
            update_library_skill_about,
            get_skill_inventory_status,
            detect_installed_skills,
            download_github_skill,
            get_skill_targets,
            install_skill_targets,
            uninstall_skill_targets,
            delete_library_skill,
            download_installed_skill,
            uninstall_installed_skill,
            list_tasks,
            list_skill_packs
        ])
        .run(tauri::generate_context!())
        .expect("error while running CodexHub");
}

fn setup_window_chrome(app: &AppHandle) -> tauri::Result<()> {
    #[cfg(windows)]
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        window.set_decorations(false)?;
    }

    Ok(())
}

fn setup_app_tray(app: &AppHandle) -> tauri::Result<()> {
    let app_name = app_display_name(app);
    let menu = MenuBuilder::new(app)
        .text(TRAY_MENU_SHOW_ID, format!("Show {app_name}"))
        .separator()
        .text(TRAY_MENU_QUIT_ID, format!("Quit {app_name}"))
        .build()?;
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip(&app_name)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            TRAY_MENU_SHOW_ID => show_main_window(app),
            TRAY_MENU_QUIT_ID => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }

    tray.build(app)?;
    Ok(())
}

fn handle_window_close_request(window: &Window, event: &WindowEvent) {
    let WindowEvent::CloseRequested { api, .. } = event else {
        return;
    };
    if window.label() != MAIN_WINDOW_LABEL {
        return;
    }

    api.prevent_close();
    let app = window.app_handle();
    match read_settings(app).close_button_behavior {
        CloseButtonBehavior::Ask => {
            let _ = app.emit(CLOSE_BUTTON_BEHAVIOR_REQUESTED_EVENT, ());
        }
        CloseButtonBehavior::Exit => app.exit(0),
        CloseButtonBehavior::MinimizeToTray => {
            let _ = window.hide();
        }
    }
}

fn app_display_name(app: &AppHandle) -> String {
    app.get_webview_window(MAIN_WINDOW_LABEL)
        .and_then(|window| window.title().ok())
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| "CodexHub".into())
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn hide_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        let _ = window.hide();
    }
}

fn timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn skill_inventory_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .unwrap_or_else(|_| PathBuf::from(".codexhub"))
        .join("skills-inventory.json")
}

fn local_codex_skills_root() -> PathBuf {
    env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .or_else(|| home_dir().map(|path| path.join(".codex")))
        .unwrap_or_else(|| PathBuf::from(".codex"))
        .join("skills")
}

fn load_skill_inventory_status(app: &AppHandle) -> Result<SkillInventoryStatus, String> {
    let path = skill_inventory_path(app);
    let mut status = if path.exists() {
        let content = fs::read_to_string(&path)
            .map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
        serde_json::from_str::<SkillInventoryStatus>(&content)
            .map_err(|error| format!("Failed to parse {}: {error}", path.display()))?
    } else {
        SkillInventoryStatus {
            first_host_scan_completed: false,
            local_skill_root: String::new(),
            local_skills: Vec::new(),
            host_inventories: Vec::new(),
        }
    };
    status.local_skill_root = local_codex_skills_root().to_string_lossy().into_owned();
    Ok(status)
}

fn save_skill_inventory_status(
    app: &AppHandle,
    status: &SkillInventoryStatus,
) -> Result<(), String> {
    let path = skill_inventory_path(app);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let content = serde_json::to_string_pretty(status).map_err(|error| error.to_string())?;
    fs::write(&path, content)
        .map_err(|error| format!("Failed to write {}: {error}", path.display()))
}

fn apply_skill_inventory_to_hosts(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let status = load_skill_inventory_status(app)?;
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");
    for host in hosts.iter_mut() {
        if let Some(inventory) = status
            .host_inventories
            .iter()
            .find(|item| item.host_alias.eq_ignore_ascii_case(&host.host_alias))
        {
            let count = if inventory.ok {
                inventory.skills.len().min(u16::MAX as usize) as u16
            } else {
                0
            };
            host.skills_exists = Some(inventory.ok && count > 0);
            host.skills_count = Some(count);
            if inventory.ok {
                host.status = HostStatus::Online;
            }
        }
    }
    Ok(())
}

fn normalize_skill_pack(skill: &mut SkillPack) {
    if skill.source_type == "git" {
        skill.source_type = "github".into();
    }
    if skill.added_at.trim().is_empty() {
        skill.added_at = date_label();
    }
    if skill.updated_at.trim().is_empty() {
        skill.updated_at = timestamp_label();
    }
    if skill.about.trim().is_empty() {
        skill.about = skill.description.clone();
    }
    skill
        .applications
        .retain(|application| !application.target_type.trim().is_empty());
}

fn merge_imported_skill(skills: &mut Vec<SkillPack>, mut skill: SkillPack) -> SkillPack {
    if let Some(existing) = skills.iter().find(|item| item.id == skill.id) {
        skill.added_at = if existing.added_at.trim().is_empty() {
            date_label()
        } else {
            existing.added_at.clone()
        };
        if skill.about.trim().is_empty() {
            skill.about = existing.about.clone();
        }
        skill.applications = existing.applications.clone();
    }
    normalize_skill_pack(&mut skill);
    skills.retain(|item| item.id != skill.id);
    skills.push(skill.clone());
    skill
}

fn import_skills_from_path(
    app: &AppHandle,
    state: &AppState,
    path: PathBuf,
    source_type: &str,
    source_override: Option<String>,
) -> Result<SkillImportResult, String> {
    let path = path
        .canonicalize()
        .map_err(|error| format!("Could not resolve skill path: {error}"))?;
    if !path.is_dir() {
        return Err(format!("{} is not a directory.", path.display()));
    }

    let candidate_dirs = skill_candidate_dirs(&path)?;
    if candidate_dirs.is_empty() {
        return Err(format!(
            "{} does not contain a SKILL.md file in the root or immediate child directories.",
            path.display()
        ));
    }

    let mut skills = load_skills(app, state)?;
    let mut imported = Vec::new();
    let mut skipped = Vec::new();
    for candidate in candidate_dirs {
        match import_single_skill(app, &candidate, source_type, source_override.as_deref()) {
            Ok(skill) => {
                imported.push(merge_imported_skill(&mut skills, skill));
            }
            Err(error) => skipped.push(format!("{}: {error}", candidate.display())),
        }
    }
    save_skills(app, state, &skills)?;

    let message = if imported.is_empty() {
        format!("No skills imported; {} candidates skipped.", skipped.len())
    } else {
        format!("Imported {} skill(s).", imported.len())
    };
    Ok(SkillImportResult {
        imported,
        skipped,
        message,
    })
}

fn skill_candidate_dirs(path: &Path) -> Result<Vec<PathBuf>, String> {
    if path.join("SKILL.md").is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    let mut candidates = Vec::new();
    for entry in fs::read_dir(path).map_err(|error| format!("Failed to read directory: {error}"))? {
        let entry = entry.map_err(|error| format!("Failed to read directory entry: {error}"))?;
        let child = entry.path();
        if child.is_dir() && child.join("SKILL.md").is_file() {
            candidates.push(child);
        }
    }
    candidates.sort();
    Ok(candidates)
}

fn import_single_skill(
    app: &AppHandle,
    source_dir: &Path,
    source_type: &str,
    source_override: Option<&str>,
) -> Result<SkillPack, String> {
    let skill_md = source_dir.join("SKILL.md");
    let content = fs::read_to_string(&skill_md)
        .map_err(|error| format!("Failed to read {}: {error}", skill_md.display()))?;
    let metadata = parse_skill_metadata(&content, source_dir)?;
    let id = safe_skill_id(&metadata.name)?;
    let managed_root = managed_skills_dir(app);
    fs::create_dir_all(&managed_root).map_err(|error| error.to_string())?;
    let managed_path = managed_root.join(&id);
    if managed_path.exists() {
        fs::remove_dir_all(&managed_path).map_err(|error| {
            format!(
                "Failed to replace existing managed skill {}: {error}",
                managed_path.display()
            )
        })?;
    }
    copy_skill_dir(source_dir, &managed_path)?;
    let description = metadata.description.unwrap_or_default();
    Ok(SkillPack {
        id: id.clone(),
        name: metadata.name,
        version: metadata.version.unwrap_or_default(),
        description: description.clone(),
        about: description,
        source_type: source_type.into(),
        source: source_override
            .map(str::to_string)
            .unwrap_or_else(|| source_dir.to_string_lossy().into_owned()),
        original_path: Some(source_dir.to_string_lossy().into_owned()),
        managed_path: managed_path.to_string_lossy().into_owned(),
        has_skill_md: true,
        skill_count: 1,
        enabled: true,
        added_at: date_label(),
        updated_at: timestamp_label(),
        applications: Vec::new(),
    })
}

struct ParsedSkillMetadata {
    name: String,
    description: Option<String>,
    version: Option<String>,
}

fn parse_skill_metadata(content: &str, source_dir: &Path) -> Result<ParsedSkillMetadata, String> {
    if content.trim().is_empty() {
        return Err("SKILL.md is empty.".into());
    }
    let mut name = None;
    let mut description = None;
    let mut version = None;
    if let Some(frontmatter) = frontmatter_block(content) {
        for line in frontmatter.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let value = unquote_frontmatter_value(value.trim());
            match key.trim() {
                "name" => name = Some(value),
                "description" => description = Some(value),
                "version" => version = Some(value),
                _ => {}
            }
        }
    }
    let fallback_name = source_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("skill")
        .to_string();
    Ok(ParsedSkillMetadata {
        name: name
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(fallback_name),
        description: description.filter(|value| !value.trim().is_empty()),
        version: version.filter(|value| !value.trim().is_empty()),
    })
}

fn frontmatter_block(content: &str) -> Option<&str> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let rest = content
        .strip_prefix("---\n")
        .or_else(|| content.strip_prefix("---\r\n"))?;
    let delimiter = rest.find("\n---").or_else(|| rest.find("\r\n---"))?;
    Some(&rest[..delimiter])
}

fn unquote_frontmatter_value(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

fn safe_skill_id(name: &str) -> Result<String, String> {
    let slug = name
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        return Err("Skill name must contain at least one ASCII letter or number.".into());
    }
    if slug == "." || slug == ".." {
        return Err("Skill name resolved to an unsafe path.".into());
    }
    Ok(slug)
}

fn validate_remote_skill_dir_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Skill name is required.".into());
    }
    if name == "." || name == ".." {
        return Err("Skill name resolved to an unsafe path.".into());
    }
    if name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        Ok(name.to_string())
    } else {
        Err(
            "Skill name may only contain ASCII letters, numbers, dots, hyphens, and underscores."
                .into(),
        )
    }
}

fn copy_skill_dir(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source)
        .map_err(|error| format!("Failed to read {}: {error}", source.display()))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let source_path = entry.path();
        let file_name = entry.file_name();
        if file_name.to_string_lossy() == ".git" {
            continue;
        }
        let destination_path = destination.join(file_name);
        let metadata = fs::symlink_metadata(&source_path).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            copy_skill_dir(&source_path, &destination_path)?;
        } else if metadata.is_file() {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            fs::copy(&source_path, &destination_path).map_err(|error| {
                format!(
                    "Failed to copy {} to {}: {error}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn run_detect_installed_skills(
    app: &AppHandle,
    state: &AppState,
    include_hosts: bool,
    timeout_ms: Option<u64>,
) -> Result<SkillDetectionResult, String> {
    let local_root = local_codex_skills_root();
    let mut skills = load_skills(app, state)?;
    for skill in &mut skills {
        skill
            .applications
            .retain(|application| application.target_type != "local");
    }

    let mut imported_count = 0usize;
    let mut skipped = Vec::new();
    let mut local_inventory_skills = Vec::new();
    if local_root.is_dir() {
        for candidate in installed_skill_candidate_dirs(&local_root)? {
            match import_single_skill(app, &candidate, "local", None) {
                Ok(detected) => {
                    let application = local_skill_application(&candidate, detected.has_skill_md);
                    let detected_id = detected.id.clone();
                    local_inventory_skills.push(RemoteSkill {
                        name: detected_id.clone(),
                        path: candidate.to_string_lossy().into_owned(),
                        has_skill_md: detected.has_skill_md,
                        status: if detected.has_skill_md {
                            "valid".into()
                        } else {
                            "invalid".into()
                        },
                        description: detected.description.clone(),
                    });
                    if let Some(existing) = skills.iter().find(|item| item.id == detected_id) {
                        let mut merged = detected;
                        merged.source_type = existing.source_type.clone();
                        merged.source = existing.source.clone();
                        merged.original_path = existing.original_path.clone();
                        merged.about = if existing.about.trim().is_empty() {
                            merged.about
                        } else {
                            existing.about.clone()
                        };
                        merge_imported_skill(&mut skills, merged);
                    } else {
                        imported_count += 1;
                        merge_imported_skill(&mut skills, detected);
                    }
                    set_skill_application(&mut skills, &detected_id, application);
                }
                Err(error) => skipped.push(format!("{}: {error}", candidate.display())),
            }
        }
    }

    let mut tasks = Vec::new();
    let mut status = load_skill_inventory_status(app)?;
    status.local_skill_root = local_root.to_string_lossy().into_owned();
    local_inventory_skills.sort_by_key(|skill| skill.name.to_ascii_lowercase());
    status.local_skills = local_inventory_skills;
    if include_hosts {
        let hosts = state.hosts.lock().expect("hosts mutex poisoned").clone();
        for host in hosts {
            let result = run_remote_skill_list(state, host.host_alias.clone(), timeout_ms)?;
            let ok = matches!(result.task.status, TaskStatus::Success);
            let previous_inventory = status
                .host_inventories
                .iter()
                .find(|item| item.host_alias.eq_ignore_ascii_case(&result.host_alias))
                .cloned();
            let mut next_inventory = HostSkillInventory {
                host_alias: result.host_alias.clone(),
                scanned_at: timestamp_label(),
                ok,
                message: result.task.summary.clone(),
                skills: result.skills.clone(),
            };
            if ok && result.skills.is_empty() {
                if let Some(previous) = previous_inventory
                    .filter(|inventory| inventory.ok && !inventory.skills.is_empty())
                {
                    let previous_count = previous.skills.len().min(u16::MAX as usize) as u16;
                    update_host_skills(state, &result.host_alias, true, previous_count);
                    next_inventory.message = format!(
                        "Latest scan returned no skills; kept previous cached {} skill(s). {}",
                        previous.skills.len(),
                        result.task.summary
                    );
                    next_inventory.skills = previous.skills;
                }
            }
            upsert_host_inventory(&mut status, next_inventory);
            tasks.push(result.task);
        }
        status.first_host_scan_completed = true;
        refresh_host_applications_from_inventory(&mut skills, &status);
    }

    save_skills(app, state, &skills)?;
    save_skill_inventory_status(app, &status)?;
    let message = if include_hosts {
        format!(
            "Detected local skills and scanned {} host(s). Imported {} new local skill(s); {} skipped.",
            tasks.len(),
            imported_count,
            skipped.len()
        )
    } else {
        format!(
            "Detected local Codex skills. Imported {} new local skill(s); {} skipped.",
            imported_count,
            skipped.len()
        )
    };
    Ok(SkillDetectionResult {
        skills,
        status,
        tasks,
        message,
    })
}

fn installed_skill_candidate_dirs(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut candidates = Vec::new();
    if root.join("SKILL.md").is_file() {
        candidates.push(root.to_path_buf());
    }
    if root.is_dir() {
        for entry in fs::read_dir(root)
            .map_err(|error| format!("Failed to read {}: {error}", root.display()))?
        {
            let entry = entry.map_err(|error| format!("Failed to read skill entry: {error}"))?;
            let child = entry.path();
            if !child.is_dir() {
                continue;
            }
            if child.join("SKILL.md").is_file() {
                candidates.push(child);
            } else {
                for nested in fs::read_dir(&child)
                    .map_err(|error| format!("Failed to read {}: {error}", child.display()))?
                {
                    let nested = nested
                        .map_err(|error| format!("Failed to read nested skill entry: {error}"))?;
                    let nested_path = nested.path();
                    if nested_path.is_dir() && nested_path.join("SKILL.md").is_file() {
                        candidates.push(nested_path);
                    }
                }
            }
        }
    }
    candidates.sort();
    candidates.dedup();
    Ok(candidates)
}

fn set_skill_application(skills: &mut [SkillPack], skill_id: &str, application: SkillApplication) {
    if let Some(skill) = skills.iter_mut().find(|skill| skill.id == skill_id) {
        skill.applications.retain(|current| {
            current.target_type != application.target_type
                || current.host_alias != application.host_alias
        });
        skill.applications.push(application);
        skill.updated_at = timestamp_label();
    }
}

fn remove_skill_application(
    skills: &mut [SkillPack],
    skill_id: &str,
    request: &SkillTargetRequest,
) {
    if let Some(skill) = skills.iter_mut().find(|skill| skill.id == skill_id) {
        skill.applications.retain(|current| {
            current.target_type != request.target_type || current.host_alias != request.host_alias
        });
        skill.updated_at = timestamp_label();
    }
}

fn local_skill_application(path: &Path, has_skill_md: bool) -> SkillApplication {
    SkillApplication {
        target_type: "local".into(),
        label: "local".into(),
        host_alias: None,
        path: path.to_string_lossy().into_owned(),
        detected_at: timestamp_label(),
        has_skill_md,
    }
}

fn host_skill_application(alias: &str, path: &str, has_skill_md: bool) -> SkillApplication {
    SkillApplication {
        target_type: "host".into(),
        label: alias.to_string(),
        host_alias: Some(alias.to_string()),
        path: path.to_string(),
        detected_at: timestamp_label(),
        has_skill_md,
    }
}

fn refresh_host_applications_from_inventory(
    skills: &mut [SkillPack],
    status: &SkillInventoryStatus,
) {
    let scanned_aliases = status
        .host_inventories
        .iter()
        .filter(|inventory| inventory.ok)
        .map(|inventory| inventory.host_alias.clone())
        .collect::<BTreeSet<_>>();
    for skill in skills.iter_mut() {
        skill.applications.retain(|application| {
            application.target_type != "host"
                || application
                    .host_alias
                    .as_ref()
                    .map(|alias| !scanned_aliases.contains(alias))
                    .unwrap_or(true)
        });
    }
    let mut additions = Vec::new();
    for inventory in status
        .host_inventories
        .iter()
        .filter(|inventory| inventory.ok)
    {
        for skill in skills.iter() {
            if let Some(remote) = inventory
                .skills
                .iter()
                .find(|remote| skill_matches_remote(skill, remote))
            {
                additions.push((
                    skill.id.clone(),
                    host_skill_application(
                        &inventory.host_alias,
                        &remote.path,
                        remote.has_skill_md,
                    ),
                ));
            }
        }
    }
    for (skill_id, application) in additions {
        set_skill_application(skills, &skill_id, application);
    }
}

fn skill_matches_remote(skill: &SkillPack, remote: &RemoteSkill) -> bool {
    remote.name.eq_ignore_ascii_case(&skill.id) || remote.name.eq_ignore_ascii_case(&skill.name)
}

fn upsert_host_inventory(status: &mut SkillInventoryStatus, inventory: HostSkillInventory) {
    status
        .host_inventories
        .retain(|item| !item.host_alias.eq_ignore_ascii_case(&inventory.host_alias));
    status.host_inventories.push(inventory);
    status
        .host_inventories
        .sort_by_key(|item| item.host_alias.to_ascii_lowercase());
}

fn update_host_inventory_skill(
    app: &AppHandle,
    alias: &str,
    skill_name: &str,
    path: &str,
    installed: bool,
    description: Option<&str>,
) -> Result<(), String> {
    let mut status = load_skill_inventory_status(app)?;
    if let Some(inventory) = status
        .host_inventories
        .iter_mut()
        .find(|item| item.host_alias.eq_ignore_ascii_case(alias))
    {
        inventory.scanned_at = timestamp_label();
        inventory.ok = true;
        inventory
            .skills
            .retain(|skill| !skill.name.eq_ignore_ascii_case(skill_name));
        if installed {
            inventory.skills.push(RemoteSkill {
                name: skill_name.to_string(),
                path: path.to_string(),
                has_skill_md: true,
                status: "valid".into(),
                description: description.unwrap_or_default().to_string(),
            });
        }
        inventory
            .skills
            .sort_by_key(|skill| skill.name.to_ascii_lowercase());
    } else if installed {
        status.host_inventories.push(HostSkillInventory {
            host_alias: alias.to_string(),
            scanned_at: timestamp_label(),
            ok: true,
            message: "Updated from skill operation.".into(),
            skills: vec![RemoteSkill {
                name: skill_name.to_string(),
                path: path.to_string(),
                has_skill_md: true,
                status: "valid".into(),
                description: description.unwrap_or_default().to_string(),
            }],
        });
    }
    save_skill_inventory_status(app, &status)
}

fn update_local_inventory_skill(
    app: &AppHandle,
    skill_name: &str,
    path: &str,
    installed: bool,
    description: Option<&str>,
) -> Result<(), String> {
    let mut status = load_skill_inventory_status(app)?;
    status.local_skill_root = local_codex_skills_root().to_string_lossy().into_owned();
    status
        .local_skills
        .retain(|skill| !skill.name.eq_ignore_ascii_case(skill_name));
    if installed {
        status.local_skills.push(RemoteSkill {
            name: skill_name.to_string(),
            path: path.to_string(),
            has_skill_md: true,
            status: "valid".into(),
            description: description.unwrap_or_default().to_string(),
        });
    }
    status
        .local_skills
        .sort_by_key(|skill| skill.name.to_ascii_lowercase());
    save_skill_inventory_status(app, &status)
}

fn resolve_installed_skill_request(
    app: &AppHandle,
    request: InstalledSkillRequest,
) -> Result<InstalledSkillRequest, String> {
    let skill_name = validate_remote_skill_dir_name(&request.skill_name)?;
    let requested_path = request.path.trim();
    if requested_path.is_empty() {
        return Err("Installed skill path is required.".into());
    }
    let status = load_skill_inventory_status(app)?;
    match request.target_type.as_str() {
        "local" => {
            let Some(installed) = status.local_skills.iter().find(|skill| {
                skill.has_skill_md
                    && skill.name.eq_ignore_ascii_case(&skill_name)
                    && skill.path == requested_path
            }) else {
                return Err(format!(
                    "Installed skill {skill_name} was not found in the local cached inventory."
                ));
            };
            Ok(InstalledSkillRequest {
                target_type: "local".into(),
                host_alias: None,
                skill_name: installed.name.clone(),
                path: installed.path.clone(),
            })
        }
        "host" => {
            let alias = request
                .host_alias
                .as_deref()
                .ok_or_else(|| "Host alias is required.".to_string())
                .and_then(ssh::validate_ssh_alias)?;
            let Some(inventory) = status
                .host_inventories
                .iter()
                .find(|item| item.host_alias.eq_ignore_ascii_case(&alias) && item.ok)
            else {
                return Err(format!(
                    "Host {alias} does not have a usable cached skill inventory."
                ));
            };
            let Some(installed) = inventory.skills.iter().find(|skill| {
                skill.has_skill_md
                    && skill.name.eq_ignore_ascii_case(&skill_name)
                    && skill.path == requested_path
            }) else {
                return Err(format!(
                    "Installed skill {skill_name} was not found in the cached inventory for {alias}."
                ));
            };
            validate_cached_remote_skill_path(&installed.path)?;
            Ok(InstalledSkillRequest {
                target_type: "host".into(),
                host_alias: Some(alias),
                skill_name: installed.name.clone(),
                path: installed.path.clone(),
            })
        }
        _ => Err("Installed skill target type must be local or host.".into()),
    }
}

fn validate_cached_remote_skill_path(path: &str) -> Result<(), String> {
    if path.trim().is_empty() || path.contains(char::is_control) {
        return Err("Cached remote skill path is empty or contains control characters.".into());
    }
    if path.split('/').any(|part| part == "..") {
        return Err("Cached remote skill path contains an unsafe parent segment.".into());
    }
    if !path.contains("/.codex/skills/") && !path.contains("/.codex/superpowers/skills/") {
        return Err("Cached remote skill path is outside known Codex skill roots.".into());
    }
    Ok(())
}

fn skill_matches_installed_request(skill: &SkillPack, request: &InstalledSkillRequest) -> bool {
    skill.id.eq_ignore_ascii_case(&request.skill_name)
        || skill.name.eq_ignore_ascii_case(&request.skill_name)
}

fn remove_installed_skill_application(skills: &mut [SkillPack], request: &InstalledSkillRequest) {
    for skill in skills {
        if !skill_matches_installed_request(skill, request) {
            continue;
        }
        skill.applications.retain(|application| {
            application.target_type != request.target_type
                || application.host_alias != request.host_alias
        });
    }
}

fn download_and_import_github_skill(
    app: &AppHandle,
    state: &AppState,
    repo_url: String,
    timeout_ms: Option<u64>,
) -> Result<SkillImportResult, String> {
    if !is_allowed_github_repo_url(&repo_url) {
        return Err("Only https://github.com/... skill repositories are supported in v1.".into());
    }
    let parsed = parse_github_skill_url(&repo_url).expect("validated GitHub skill URL");
    let timeout = ssh::normalize_timeout_ms(timeout_ms.or(Some(120_000)));
    let clone_root = skill_clone_cache_dir(app);
    fs::create_dir_all(&clone_root).map_err(|error| error.to_string())?;
    let clone_dir = clone_root.join(format!(
        "{}-{}",
        safe_skill_id(&parsed.source_url).unwrap_or_else(|_| "github-skill".into()),
        timestamp_millis()
    ));
    let mut args = vec!["clone".into(), "--depth".into(), "1".into()];
    if let Some(branch) = &parsed.branch {
        args.push("--branch".into());
        args.push(branch.clone());
    }
    args.push(parsed.clone_url.clone());
    args.push(clone_dir.to_string_lossy().to_string());
    let command = if let Some(branch) = &parsed.branch {
        format!(
            "git clone --depth 1 --branch {branch} {} {}",
            parsed.clone_url,
            path_string(&clone_dir)
        )
    } else {
        format!(
            "git clone --depth 1 {} {}",
            parsed.clone_url,
            path_string(&clone_dir)
        )
    };
    let output = ssh::run_local_process("git", &args, &command, timeout).unwrap_or_else(|error| {
        failed_command_output(command, format!("Could not start git: {error}"))
    });
    if !output.success() {
        let _ = fs::remove_dir_all(&clone_dir);
        return Err(command_detail(&output));
    }
    let import_path = parsed
        .skill_subpath
        .as_ref()
        .map(|subpath| clone_dir.join(subpath))
        .unwrap_or_else(|| clone_dir.clone());
    if !import_path.exists() {
        let _ = fs::remove_dir_all(&clone_dir);
        return Err(format!(
            "GitHub skill path {} was not found after cloning.",
            parsed.display_path()
        ));
    }
    ensure_child_path(&clone_dir, &import_path)?;
    let result = import_skills_from_path(
        app,
        state,
        import_path,
        "github",
        Some(parsed.source_url.clone()),
    );
    if result.is_err() {
        let _ = fs::remove_dir_all(&clone_dir);
    }
    result
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GithubSkillUrl {
    owner: String,
    repo: String,
    clone_url: String,
    branch: Option<String>,
    skill_subpath: Option<PathBuf>,
    source_url: String,
}

impl GithubSkillUrl {
    fn display_path(&self) -> String {
        self.skill_subpath
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.repo.clone())
    }
}

fn is_allowed_github_repo_url(url: &str) -> bool {
    parse_github_skill_url(url).is_some()
}

fn parse_github_skill_url(url: &str) -> Option<GithubSkillUrl> {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.contains(char::is_whitespace) {
        return None;
    }
    if trimmed.contains("..")
        || trimmed.contains('\\')
        || trimmed.contains('"')
        || trimmed.contains('\'')
        || trimmed.contains(char::is_control)
    {
        return None;
    }
    let path = trimmed.strip_prefix("https://github.com/")?;
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.iter().any(|part| part.is_empty()) {
        return None;
    }
    if parts.len() != 2 && !(parts.len() >= 5 && parts[2] == "tree") {
        return None;
    }
    let owner = parts[0].to_string();
    let repo_part = parts[1].to_string();
    let repo = repo_part
        .strip_suffix(".git")
        .unwrap_or(&repo_part)
        .to_string();
    if owner.is_empty()
        || repo.is_empty()
        || !is_safe_github_segment(&owner)
        || !is_safe_github_segment(&repo)
    {
        return None;
    }
    if repo_part.ends_with(".git") && parts.len() != 2 {
        return None;
    }
    let clone_url = format!("https://github.com/{owner}/{repo}.git");
    if parts.len() == 2 {
        return Some(GithubSkillUrl {
            owner,
            repo,
            clone_url,
            branch: None,
            skill_subpath: None,
            source_url: trimmed.to_string(),
        });
    }
    let branch = parts[3].to_string();
    if branch.is_empty() || !is_safe_github_tree_segment(&branch) {
        return None;
    }
    let subpath_parts = parts[4..].to_vec();
    if subpath_parts.is_empty()
        || subpath_parts
            .iter()
            .any(|part| !is_safe_github_tree_segment(part) || *part == "." || *part == "..")
    {
        return None;
    }
    let mut skill_subpath = PathBuf::new();
    for part in subpath_parts {
        skill_subpath.push(part);
    }
    Some(GithubSkillUrl {
        owner,
        repo,
        clone_url,
        branch: Some(branch),
        skill_subpath: Some(skill_subpath),
        source_url: trimmed.to_string(),
    })
}

fn is_safe_github_segment(value: &str) -> bool {
    value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

fn is_safe_github_tree_segment(value: &str) -> bool {
    !value.is_empty()
        && !value.contains("..")
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '@' | '%'))
}

fn run_get_skill_targets(
    app: &AppHandle,
    state: &AppState,
    skill_id: String,
    _timeout_ms: Option<u64>,
) -> Result<SkillTargetsResult, String> {
    let skill = find_skill(app, state, &skill_id)?;
    let status = load_skill_inventory_status(app)?;
    let mut targets = Vec::new();
    let local_path =
        local_skill_installed_path(&skill).unwrap_or_else(|| local_skill_target_path(&skill.id));
    let local_cached = status
        .local_skills
        .iter()
        .find(|installed| skill_matches_remote(&skill, installed));
    let local_installed = local_cached.is_some()
        || skill
            .applications
            .iter()
            .any(|application| application.target_type == "local");
    let local_display_path = local_cached
        .map(|installed| installed.path.clone())
        .unwrap_or_else(|| local_path.to_string_lossy().into_owned());
    targets.push(SkillTarget {
        target_type: "local".into(),
        label: "local".into(),
        host_alias: None,
        path: local_display_path,
        installed: local_installed,
        can_install: !local_installed
            && PathBuf::from(&skill.managed_path)
                .join("SKILL.md")
                .is_file(),
        can_uninstall: local_installed,
        status: if local_installed {
            "installed"
        } else {
            "available"
        }
        .into(),
        message: if local_installed {
            "Skill is installed on the local Codex root.".into()
        } else {
            "Skill can be installed to the local Codex root.".into()
        },
    });

    let hosts = state.hosts.lock().expect("hosts mutex poisoned").clone();
    for host in hosts {
        let inventory = status
            .host_inventories
            .iter()
            .find(|item| item.host_alias.eq_ignore_ascii_case(&host.host_alias));
        let cached_skill = inventory.and_then(|inventory| {
            inventory
                .skills
                .iter()
                .find(|installed| skill_matches_remote(&skill, installed))
        });
        let cache_ok = inventory.map(|item| item.ok).unwrap_or(false);
        let installed = cache_ok && cached_skill.is_some();
        let target_path = cached_skill
            .map(|installed| installed.path.clone())
            .unwrap_or_else(|| format!("~/.codex/skills/{}", skill.id));
        targets.push(SkillTarget {
            target_type: "host".into(),
            label: host.host_alias.clone(),
            host_alias: Some(host.host_alias.clone()),
            path: target_path,
            installed,
            can_install: cache_ok && !installed,
            can_uninstall: installed,
            status: if cache_ok {
                if installed {
                    "installed"
                } else {
                    "available"
                }
            } else {
                "cached-unavailable"
            }
            .into(),
            message: if cache_ok {
                if installed {
                    "Cached: skill is installed on this host.".into()
                } else {
                    "Cached: skill can be installed to this host.".into()
                }
            } else {
                inventory
                    .map(|item| item.message.clone())
                    .filter(|message| !message.trim().is_empty())
                    .unwrap_or_else(|| "Run Detect to refresh this host skill cache.".into())
            },
        });
    }

    Ok(SkillTargetsResult {
        skill_id: skill.id,
        skill_name: skill.name,
        targets,
        tasks: Vec::new(),
        message: "Loaded cached skill targets.".into(),
    })
}

fn run_download_installed_skill(
    app: &AppHandle,
    state: &AppState,
    request: InstalledSkillRequest,
    timeout_ms: Option<u64>,
) -> Result<InstalledSkillDownloadResult, String> {
    let request = resolve_installed_skill_request(app, request)?;
    let mut tasks = Vec::new();
    let import_result = if request.target_type == "local" {
        import_skills_from_path(
            app,
            state,
            PathBuf::from(&request.path),
            "local",
            Some(request.path.clone()),
        )?
    } else {
        let (extract_dir, task) =
            download_remote_installed_skill(app, state, &request, timeout_ms)?;
        tasks.push(task);
        let alias = request.host_alias.clone().unwrap_or_default();
        import_skills_from_path(
            app,
            state,
            extract_dir,
            "host",
            Some(format!("{alias}:{}", request.path)),
        )?
    };
    let skills = load_skills(app, state)?;
    let status = load_skill_inventory_status(app)?;
    let message = if import_result.imported.is_empty() {
        import_result.message.clone()
    } else {
        format!(
            "Downloaded {} to the local skill library.",
            request.skill_name
        )
    };
    Ok(InstalledSkillDownloadResult {
        imported: import_result.imported,
        skipped: import_result.skipped,
        skills,
        status,
        tasks,
        message,
    })
}

fn download_remote_installed_skill(
    app: &AppHandle,
    state: &AppState,
    request: &InstalledSkillRequest,
    timeout_ms: Option<u64>,
) -> Result<(PathBuf, TaskRun), String> {
    let alias = request
        .host_alias
        .as_deref()
        .ok_or_else(|| "Host alias is required.".to_string())
        .and_then(ssh::validate_ssh_alias)?;
    let timeout = ssh::normalize_timeout_ms(timeout_ms.or(Some(120_000)));
    let task_id = format!("task-skill-download-{}", timestamp_millis());
    let host_id = host_id_for_alias(state, &alias);
    let host_name = host_name_for_alias(state, &alias);
    let cache_root = skill_clone_cache_dir(app).join("installed-downloads");
    fs::create_dir_all(&cache_root).map_err(|error| error.to_string())?;
    let remote_archive = format!("/tmp/codexhub-skill-download-{task_id}.tgz");
    let local_archive = cache_root.join(format!("{}.tgz", task_id));
    let extract_dir = cache_root.join(format!("{task_id}-extract"));
    let script = remote_installed_skill_archive_script(&request.path, &remote_archive);
    let package_output = ssh::run_ssh_script(&alias, &script, timeout).unwrap_or_else(|error| {
        failed_command_output(
            format!("ssh {alias} package installed skill {}", request.skill_name),
            error,
        )
    });
    let mut logs = vec![command_log(
        &task_id,
        1,
        if package_output.success() {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        if package_output.success() {
            "Packaged installed remote skill."
        } else {
            "Failed to package installed remote skill."
        },
        &package_output,
    )];
    if !package_output.success() {
        let task = skill_task(
            &task_id,
            &host_id,
            &host_name,
            "Download installed skill",
            TaskStatus::Failed,
            &format!(
                "{} could not be packaged on {alias}: {}",
                request.skill_name,
                command_detail(&package_output)
            ),
            logs,
        );
        record_task(state, task.clone());
        return Err(task.summary);
    }

    let download_output = ssh::download_file(&alias, &remote_archive, &local_archive, timeout)
        .unwrap_or_else(|error| {
            failed_command_output(format!("scp {alias}:{remote_archive}"), error)
        });
    logs.push(command_log(
        &task_id,
        2,
        if download_output.success() {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        if download_output.success() {
            "Downloaded installed skill archive."
        } else {
            "Failed to download installed skill archive."
        },
        &download_output,
    ));
    let _ = ssh::run_ssh_script(
        &alias,
        &format!("rm -f {}", shell_single_quote(&remote_archive)),
        timeout,
    );
    if !download_output.success() {
        let task = skill_task(
            &task_id,
            &host_id,
            &host_name,
            "Download installed skill",
            TaskStatus::Failed,
            &format!(
                "{} could not be downloaded from {alias}: {}",
                request.skill_name,
                command_detail(&download_output)
            ),
            logs,
        );
        record_task(state, task.clone());
        return Err(task.summary);
    }

    fs::create_dir_all(&extract_dir).map_err(|error| error.to_string())?;
    if let Err(error) = extract_skill_archive(&local_archive, &extract_dir) {
        logs.push(basic_log(
            &task_id,
            3,
            TaskLogLevel::Error,
            &format!("Failed to extract installed skill archive: {error}"),
        ));
        let task = skill_task(
            &task_id,
            &host_id,
            &host_name,
            "Download installed skill",
            TaskStatus::Failed,
            &format!("{} archive could not be extracted.", request.skill_name),
            logs,
        );
        record_task(state, task.clone());
        return Err(task.summary);
    }
    logs.push(basic_log(
        &task_id,
        3,
        TaskLogLevel::Info,
        "Extracted installed skill archive into local cache.",
    ));
    let task = skill_task(
        &task_id,
        &host_id,
        &host_name,
        "Download installed skill",
        TaskStatus::Success,
        &format!("{} downloaded from {alias}.", request.skill_name),
        logs,
    );
    record_task(state, task.clone());
    Ok((extract_dir, task))
}

fn extract_skill_archive(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let file = fs::File::open(archive_path)
        .map_err(|error| format!("Failed to open {}: {error}", archive_path.display()))?;
    let decoder = GzDecoder::new(file);
    let mut archive = TarArchive::new(decoder);
    for entry in archive
        .entries()
        .map_err(|error| format!("Failed to read archive entries: {error}"))?
    {
        let mut entry = entry.map_err(|error| format!("Failed to read archive entry: {error}"))?;
        let path = entry
            .path()
            .map_err(|error| format!("Failed to read archive path: {error}"))?
            .to_path_buf();
        if path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err("Downloaded skill archive contains unsafe paths.".into());
        }
        entry
            .unpack_in(destination)
            .map_err(|error| format!("Failed to extract archive entry: {error}"))?;
    }
    Ok(())
}

fn run_uninstall_installed_skill(
    app: &AppHandle,
    state: &AppState,
    request: InstalledSkillRequest,
    timeout_ms: Option<u64>,
) -> Result<SkillTargetOperationResult, String> {
    let request = resolve_installed_skill_request(app, request)?;
    let mut skills = load_skills(app, state)?;
    let (item, task) = if request.target_type == "local" {
        uninstall_installed_local_skill(&request)?
    } else {
        let result = run_remote_installed_skill_delete(state, &request, timeout_ms)?;
        (
            SkillTargetOperationItem {
                target_type: "host".into(),
                label: result.host_alias.clone(),
                host_alias: Some(result.host_alias),
                ok: result.ok,
                message: result.message,
                task: Some(result.task.clone()),
            },
            result.task,
        )
    };
    if request.target_type == "local" {
        record_task(state, task.clone());
    }
    if item.ok {
        remove_installed_skill_application(&mut skills, &request);
        if request.target_type == "local" {
            let _ = update_local_inventory_skill(app, &request.skill_name, "", false, None);
        } else if let Some(alias) = &request.host_alias {
            let _ = update_host_inventory_skill(
                app,
                alias,
                &request.skill_name,
                &request.path,
                false,
                None,
            );
        }
    }
    save_skills(app, state, &skills)?;
    let ok = item.ok;
    let message = if ok {
        "uninstall-success".into()
    } else {
        "uninstall-partial-failure".into()
    };
    Ok(SkillTargetOperationResult {
        ok,
        skills,
        tasks: vec![task],
        results: vec![item],
        message,
    })
}

fn uninstall_installed_local_skill(
    request: &InstalledSkillRequest,
) -> Result<(SkillTargetOperationItem, TaskRun), String> {
    let root = local_codex_skills_root();
    let target = PathBuf::from(&request.path);
    let existed = target.exists();
    if existed {
        ensure_child_path(&root, &target)?;
    } else if let Some(parent) = target.parent() {
        ensure_child_path(&root, parent)?;
    } else {
        return Err("Installed local skill path has no parent directory.".into());
    }
    let ok = if !existed {
        true
    } else if target.is_dir() {
        fs::remove_dir_all(&target).map_err(|error| {
            format!(
                "Failed to remove local installed skill {}: {error}",
                target.display()
            )
        })?;
        true
    } else {
        false
    };
    let message = if ok {
        if existed {
            format!("{} removed from {}.", request.skill_name, target.display())
        } else {
            format!(
                "{} was not present at {}.",
                request.skill_name,
                target.display()
            )
        }
    } else {
        format!(
            "Local installed skill target is not a directory: {}.",
            target.display()
        )
    };
    let task = local_skill_task("Uninstall installed skill", &message, ok);
    Ok((
        SkillTargetOperationItem {
            target_type: "local".into(),
            label: "local".into(),
            host_alias: None,
            ok,
            message,
            task: Some(task.clone()),
        },
        task,
    ))
}

fn run_install_skill_targets(
    app: &AppHandle,
    state: &AppState,
    skill_id: String,
    targets: Vec<SkillTargetRequest>,
    timeout_ms: Option<u64>,
) -> Result<SkillTargetOperationResult, String> {
    let skill = find_skill(app, state, &skill_id)?;
    let mut skills = load_skills(app, state)?;
    let mut tasks = Vec::new();
    let mut results = Vec::new();
    for request in targets {
        if request.target_type == "local" {
            let (item, application, task) = install_local_skill(&skill)?;
            if item.ok {
                let installed_path = application.path.clone();
                set_skill_application(&mut skills, &skill.id, application);
                let _ = update_local_inventory_skill(
                    app,
                    &skill.id,
                    &installed_path,
                    true,
                    Some(&skill.description),
                );
            }
            if let Some(task) = task.clone() {
                record_task(state, task.clone());
                tasks.push(task);
            }
            results.push(item);
        } else if request.target_type == "host" {
            let Some(alias) = request.host_alias.clone() else {
                results.push(failed_target_item(
                    "host",
                    "unknown",
                    None,
                    "Missing host alias.",
                ));
                continue;
            };
            let result = run_remote_skill_install(
                app,
                state,
                alias.clone(),
                skill.id.clone(),
                RemoteSkillScope::User,
                None,
                SkillConflictPolicy::Skip,
                timeout_ms,
            )?;
            let ok = result.ok && !result.skipped;
            if ok {
                set_skill_application(
                    &mut skills,
                    &skill.id,
                    host_skill_application(&result.host_alias, &result.target_path, true),
                );
                let _ = update_host_inventory_skill(
                    app,
                    &result.host_alias,
                    &skill.id,
                    &result.target_path,
                    true,
                    Some(&skill.description),
                );
            }
            tasks.push(result.task.clone());
            results.push(SkillTargetOperationItem {
                target_type: "host".into(),
                label: result.host_alias.clone(),
                host_alias: Some(result.host_alias),
                ok,
                message: result.message,
                task: Some(result.task),
            });
        }
    }
    save_skills(app, state, &skills)?;
    let ok = results.iter().all(|result| result.ok);
    let message = if ok {
        "install-success".to_string()
    } else {
        "install-partial-failure".to_string()
    };
    Ok(SkillTargetOperationResult {
        ok,
        skills,
        tasks,
        results,
        message,
    })
}

fn run_uninstall_skill_targets(
    app: &AppHandle,
    state: &AppState,
    skill_id: String,
    targets: Vec<SkillTargetRequest>,
    timeout_ms: Option<u64>,
) -> Result<SkillTargetOperationResult, String> {
    let skill = find_skill(app, state, &skill_id)?;
    let mut skills = load_skills(app, state)?;
    let mut tasks = Vec::new();
    let mut results = Vec::new();
    for request in targets {
        if request.target_type == "local" {
            let (item, task) = uninstall_local_skill(&skill)?;
            if item.ok {
                remove_skill_application(&mut skills, &skill.id, &request);
                let _ = update_local_inventory_skill(app, &skill.id, "", false, None);
            }
            if let Some(task) = task.clone() {
                record_task(state, task.clone());
                tasks.push(task);
            }
            results.push(item);
        } else if request.target_type == "host" {
            let Some(alias) = request.host_alias.clone() else {
                results.push(failed_target_item(
                    "host",
                    "unknown",
                    None,
                    "Missing host alias.",
                ));
                continue;
            };
            let result = run_remote_skill_delete(
                state,
                alias,
                skill.id.clone(),
                RemoteSkillScope::User,
                None,
                skill.id.clone(),
                timeout_ms,
            )?;
            if result.ok {
                remove_skill_application(&mut skills, &skill.id, &request);
                let _ = update_host_inventory_skill(
                    app,
                    &result.host_alias,
                    &skill.id,
                    &result.target_path,
                    false,
                    None,
                );
            }
            tasks.push(result.task.clone());
            results.push(SkillTargetOperationItem {
                target_type: "host".into(),
                label: result.host_alias.clone(),
                host_alias: Some(result.host_alias),
                ok: result.ok,
                message: result.message,
                task: Some(result.task),
            });
        }
    }
    save_skills(app, state, &skills)?;
    let ok = results.iter().all(|result| result.ok);
    let message = if ok {
        "uninstall-success".to_string()
    } else {
        "uninstall-partial-failure".to_string()
    };
    Ok(SkillTargetOperationResult {
        ok,
        skills,
        tasks,
        results,
        message,
    })
}

fn run_delete_library_skill(
    app: &AppHandle,
    state: &AppState,
    skill_id: String,
    uninstall_first: bool,
    timeout_ms: Option<u64>,
) -> Result<SkillTargetOperationResult, String> {
    let skill = find_skill(app, state, &skill_id)?;
    let mut tasks = Vec::new();
    let mut results = Vec::new();
    if uninstall_first {
        let targets = skill
            .applications
            .iter()
            .map(|application| SkillTargetRequest {
                target_type: application.target_type.clone(),
                host_alias: application.host_alias.clone(),
            })
            .collect::<Vec<_>>();
        if !targets.is_empty() {
            let uninstall =
                run_uninstall_skill_targets(app, state, skill.id.clone(), targets, timeout_ms)?;
            tasks.extend(uninstall.tasks);
            results.extend(uninstall.results);
            if !uninstall.ok {
                return Ok(SkillTargetOperationResult {
                    ok: false,
                    skills: uninstall.skills,
                    tasks,
                    results,
                    message: "Delete cancelled because one or more uninstall operations failed."
                        .into(),
                });
            }
        }
    }

    delete_managed_skill_dir(app, &skill)?;
    let mut skills = load_skills(app, state)?;
    skills.retain(|item| item.id != skill.id);
    save_skills(app, state, &skills)?;
    let message = format!("Removed {} from the local skill library.", skill.name);
    Ok(SkillTargetOperationResult {
        ok: true,
        skills,
        tasks,
        results,
        message,
    })
}

fn local_skill_target_path(skill_id: &str) -> PathBuf {
    local_codex_skills_root().join(skill_id)
}

fn local_skill_installed_path(skill: &SkillPack) -> Option<PathBuf> {
    skill
        .applications
        .iter()
        .find(|application| application.target_type == "local")
        .map(|application| PathBuf::from(&application.path))
        .filter(|path| path.join("SKILL.md").is_file())
        .or_else(|| {
            let target = local_skill_target_path(&skill.id);
            target.join("SKILL.md").is_file().then_some(target)
        })
}

fn install_local_skill(
    skill: &SkillPack,
) -> Result<(SkillTargetOperationItem, SkillApplication, Option<TaskRun>), String> {
    let source = PathBuf::from(&skill.managed_path);
    if !source.join("SKILL.md").is_file() {
        return Err(format!(
            "Managed skill {} no longer contains SKILL.md.",
            skill.name
        ));
    }
    let root = local_codex_skills_root();
    fs::create_dir_all(&root)
        .map_err(|error| format!("Failed to create {}: {error}", root.display()))?;
    let target = root.join(&skill.id);
    if target.exists() {
        let message = format!("{} already exists at {}.", skill.name, target.display());
        let task = local_skill_task("Install skill", &message, false);
        return Ok((
            SkillTargetOperationItem {
                target_type: "local".into(),
                label: "local".into(),
                host_alias: None,
                ok: false,
                message,
                task: Some(task.clone()),
            },
            local_skill_application(&target, target.join("SKILL.md").is_file()),
            Some(task),
        ));
    }
    copy_skill_dir(&source, &target)?;
    let message = format!("Installed {} to {}.", skill.name, target.display());
    let task = local_skill_task("Install skill", &message, true);
    Ok((
        SkillTargetOperationItem {
            target_type: "local".into(),
            label: "local".into(),
            host_alias: None,
            ok: true,
            message,
            task: Some(task.clone()),
        },
        local_skill_application(&target, true),
        Some(task),
    ))
}

fn uninstall_local_skill(
    skill: &SkillPack,
) -> Result<(SkillTargetOperationItem, Option<TaskRun>), String> {
    let Some(target) = local_skill_installed_path(skill) else {
        let message = format!("{} is not installed in the local Codex root.", skill.name);
        let task = local_skill_task("Uninstall skill", &message, true);
        return Ok((
            SkillTargetOperationItem {
                target_type: "local".into(),
                label: "local".into(),
                host_alias: None,
                ok: true,
                message,
                task: Some(task.clone()),
            },
            Some(task),
        ));
    };
    let root = local_codex_skills_root();
    ensure_child_path(&root, &target)?;
    let backup_root = root.join(".codexhub-backups");
    fs::create_dir_all(&backup_root)
        .map_err(|error| format!("Failed to create {}: {error}", backup_root.display()))?;
    let mut backup = backup_root.join(format!("{}.deleted.{}", skill.id, timestamp_label()));
    if backup.exists() {
        backup = backup_root.join(format!(
            "{}.deleted.{}.{}",
            skill.id,
            timestamp_label(),
            timestamp_millis()
        ));
    }
    fs::rename(&target, &backup).map_err(|error| {
        format!(
            "Failed to move {} to {}: {error}",
            target.display(),
            backup.display()
        )
    })?;
    let message = format!("Moved local {} to backup {}.", skill.name, backup.display());
    let task = local_skill_task("Uninstall skill", &message, true);
    Ok((
        SkillTargetOperationItem {
            target_type: "local".into(),
            label: "local".into(),
            host_alias: None,
            ok: true,
            message,
            task: Some(task.clone()),
        },
        Some(task),
    ))
}

fn delete_managed_skill_dir(app: &AppHandle, skill: &SkillPack) -> Result<(), String> {
    let managed_root = managed_skills_dir(app);
    let managed_path = PathBuf::from(&skill.managed_path);
    if managed_path.exists() {
        ensure_child_path(&managed_root, &managed_path)?;
        fs::remove_dir_all(&managed_path).map_err(|error| {
            format!(
                "Failed to remove managed skill {}: {error}",
                managed_path.display()
            )
        })?;
    }
    Ok(())
}

fn ensure_child_path(root: &Path, child: &Path) -> Result<(), String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("Could not resolve {}: {error}", root.display()))?;
    let child = child
        .canonicalize()
        .map_err(|error| format!("Could not resolve {}: {error}", child.display()))?;
    if child.starts_with(&root) {
        Ok(())
    } else {
        Err(format!(
            "Refusing to modify {} because it is outside {}.",
            child.display(),
            root.display()
        ))
    }
}

fn local_skill_task(action: &str, summary: &str, ok: bool) -> TaskRun {
    skill_task(
        &format!("task-local-skill-{}", timestamp_millis()),
        "local",
        "Local machine",
        action,
        if ok {
            TaskStatus::Success
        } else {
            TaskStatus::Failed
        },
        summary,
        Vec::new(),
    )
}

fn failed_target_item(
    target_type: &str,
    label: &str,
    host_alias: Option<String>,
    message: &str,
) -> SkillTargetOperationItem {
    SkillTargetOperationItem {
        target_type: target_type.into(),
        label: label.into(),
        host_alias,
        ok: false,
        message: message.into(),
        task: None,
    }
}

fn find_skill(app: &AppHandle, state: &AppState, skill_id: &str) -> Result<SkillPack, String> {
    load_skills(app, state)?
        .into_iter()
        .find(|skill| skill.id == skill_id)
        .ok_or_else(|| format!("Skill {skill_id} was not found."))
}

fn write_skill_archive(
    app: &AppHandle,
    skill: &SkillPack,
    task_id: &str,
) -> Result<PathBuf, String> {
    let source = PathBuf::from(&skill.managed_path);
    if !source.join("SKILL.md").is_file() {
        return Err(format!(
            "Managed skill {} no longer contains SKILL.md.",
            skill.name
        ));
    }
    let dir = app
        .path()
        .app_cache_dir()
        .unwrap_or_else(|_| env::temp_dir())
        .join("skill-upload");
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join(format!("{task_id}-{}.tgz", skill.id));
    let file = fs::File::create(&path).map_err(|error| error.to_string())?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut tar = TarBuilder::new(encoder);
    tar.append_dir_all("skill", &source)
        .map_err(|error| format!("Failed to archive skill {}: {error}", skill.name))?;
    tar.finish()
        .map_err(|error| format!("Failed to finish skill archive: {error}"))?;
    Ok(path)
}

fn remote_skill_root(
    scope: &RemoteSkillScope,
    project_path: Option<&str>,
) -> Result<(String, String), String> {
    match scope {
        RemoteSkillScope::User => Ok(("$HOME/.codex/skills".into(), "~/.codex/skills".into())),
        RemoteSkillScope::Project => {
            let project_path = project_path
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    "Project path is required for project-level skill install.".to_string()
                })?;
            if project_path.contains('\n') || project_path.contains('\r') {
                return Err("Project path cannot contain line breaks.".into());
            }
            if let Some(suffix) = project_path.strip_prefix("~/") {
                if suffix.is_empty() {
                    return Err("Project path must include a directory after ~/.".into());
                }
                Ok((
                    format!("$HOME/{}/.codex/skills", shell_single_quote(suffix)),
                    format!("{project_path}/.codex/skills"),
                ))
            } else if project_path.starts_with('/') {
                Ok((
                    format!("{}/.codex/skills", shell_single_quote(project_path)),
                    format!("{project_path}/.codex/skills"),
                ))
            } else {
                Err("Project path must start with / or ~/.".into())
            }
        }
    }
}

fn remote_skill_target_display(
    scope: &RemoteSkillScope,
    project_path: Option<&str>,
    skill_name: &str,
) -> Result<String, String> {
    let (_, root) = remote_skill_root(scope, project_path)?;
    Ok(format!("{root}/{skill_name}"))
}

fn remote_skill_list_script() -> &'static str {
    r#"count=0
extract_skill_description() {
  file="$1/SKILL.md"
  [ -f "$file" ] || return
  awk '
    NR == 1 && $0 == "---" { in_frontmatter=1; next }
    in_frontmatter && $0 == "---" { exit }
    in_frontmatter {
      line=$0
      sub(/\r$/, "", line)
      if (line ~ /^[[:space:]]*description[[:space:]]*:/) {
        sub(/^[[:space:]]*description[[:space:]]*:[[:space:]]*/, "", line)
        gsub(/^["'\''"]|["'\''"]$/, "", line)
        print line
        exit
      }
    }
  ' "$file" | tr '\t\r\n' '   ' | sed 's/  */ /g' | cut -c 1-500
}
emit_skill_dir() {
  dir=$1
  [ -d "$dir" ] || return
  name=${dir##*/}
  description=
  if [ -f "$dir/SKILL.md" ]; then
    status=valid
    has=yes
    description=$(extract_skill_description "$dir")
  else
    status=missing-skill-md
    has=no
  fi
  printf 'CODEXHUB_REMOTE_SKILL\t%s\t%s\t%s\t%s\t%s\n' "$name" "$has" "$status" "$dir" "$description"
  count=$((count + 1))
}
scan_child_dir() {
  dir=$1
  [ -d "$dir" ] || return
  if [ -f "$dir/SKILL.md" ]; then
    emit_skill_dir "$dir"
    return
  fi
  before=$count
  for nested in "$dir"/* "$dir"/.[!.]* "$dir"/..?*; do
    [ -d "$nested" ] || continue
    [ -f "$nested/SKILL.md" ] || continue
    emit_skill_dir "$nested"
  done
  if [ "$count" = "$before" ]; then
    emit_skill_dir "$dir"
  fi
}
scan_root() {
  root=$1
  printf 'CODEXHUB_SKILL_ROOT=%s\n' "$root"
  [ -d "$root" ] || return
  if [ -f "$root/SKILL.md" ]; then
    emit_skill_dir "$root"
  else
    for dir in "$root"/* "$root"/.[!.]* "$root"/..?*; do
      scan_child_dir "$dir"
    done
  fi
}
scan_find_fallback() {
  root=$1
  [ -d "$root" ] || return
  command -v find >/dev/null 2>&1 || return
  find "$root" -mindepth 1 -maxdepth 3 -type f -name SKILL.md 2>/dev/null | while IFS= read -r skill_md; do
    dir=${skill_md%/SKILL.md}
    [ -d "$dir" ] || continue
    emit_skill_dir "$dir"
  done
}
scan_root "$HOME/.codex/skills"
scan_root "$HOME/.codex/superpowers/skills"
if [ "$count" = 0 ]; then
  scan_find_fallback "$HOME/.codex/skills"
  scan_find_fallback "$HOME/.codex/superpowers/skills"
fi
printf 'CODEXHUB_SKILL_COUNT=%s\n' "$count"
"#
}

fn run_remote_skill_list(
    state: &AppState,
    host_alias: String,
    timeout_ms: Option<u64>,
) -> Result<RemoteSkillListResult, String> {
    let timeout = ssh::normalize_timeout_ms(timeout_ms.or(Some(30_000)));
    let alias = ssh::validate_ssh_alias(&host_alias)?;
    let task_id = format!("task-skill-list-{}", timestamp_millis());
    let host_id = host_id_for_alias(state, &alias);
    let host_name = host_name_for_alias(state, &alias);
    let check_output = ssh::run_ssh_echo_ok(&alias, timeout)
        .unwrap_or_else(|error| failed_command_output(format!("ssh {alias} echo ok"), error));
    let check_ok = check_output.success() && check_output.stdout.trim() == "ok";
    let mut logs = vec![command_log(
        &task_id,
        1,
        if check_ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        &ssh_check_message(&alias, &check_output, check_ok, timeout),
        &check_output,
    )];
    if !check_ok {
        let task = skill_task(
            &task_id,
            &host_id,
            &host_name,
            "List remote skills",
            TaskStatus::Failed,
            "Remote skill list skipped because SSH check failed.",
            logs,
        );
        record_task(state, task.clone());
        return Ok(RemoteSkillListResult {
            host_alias: alias,
            root_path: "~/.codex/skills; ~/.codex/superpowers/skills".into(),
            count: 0,
            valid_count: 0,
            invalid_count: 0,
            skills: Vec::new(),
            task,
        });
    }

    let script = remote_skill_list_script();
    let output = ssh::run_ssh_script(&alias, script, timeout).unwrap_or_else(|error| {
        failed_command_output(format!("ssh {alias} list remote skills"), error)
    });
    let ok = output.success();
    let skills = if ok {
        parse_remote_skill_list(&output.stdout)
    } else {
        Vec::new()
    };
    let stdout_line_count = output.stdout.lines().count();
    let remote_marker_count = output
        .stdout
        .lines()
        .filter(|line| line.split_whitespace().next() == Some("CODEXHUB_REMOTE_SKILL"))
        .count();
    let stderr_summary = output
        .stderr
        .trim()
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    let list_message = if ok {
        if stderr_summary.is_empty() {
            format!(
                "Listed remote Codex skill roots (~/.codex/skills, ~/.codex/superpowers/skills): stdout {stdout_line_count} line(s), markers {remote_marker_count}, parsed {}.",
                skills.len()
            )
        } else {
            format!(
                "Listed remote Codex skill roots (~/.codex/skills, ~/.codex/superpowers/skills): stdout {stdout_line_count} line(s), markers {remote_marker_count}, parsed {}, stderr: {stderr_summary}",
                skills.len()
            )
        }
    } else {
        "Failed to list remote skills.".to_string()
    };
    logs.push(command_log(
        &task_id,
        2,
        if ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        &list_message,
        &output,
    ));
    let count = skills.len().min(u16::MAX as usize) as u16;
    let valid_count = skills
        .iter()
        .filter(|skill| skill.has_skill_md)
        .count()
        .min(u16::MAX as usize) as u16;
    let invalid_count = count.saturating_sub(valid_count);
    update_host_skills(state, &alias, ok, count);
    let summary = if ok {
        format!(
            "Remote skill list completed for {alias}: {count} skill(s), {valid_count} valid across Codex skill roots."
        )
    } else {
        format!(
            "Remote skill list failed for {alias}: {}",
            command_detail(&output)
        )
    };
    let task = skill_task(
        &task_id,
        &host_id,
        &host_name,
        "List remote skills",
        if ok {
            TaskStatus::Success
        } else {
            TaskStatus::Failed
        },
        &summary,
        logs,
    );
    record_task(state, task.clone());
    Ok(RemoteSkillListResult {
        host_alias: alias,
        root_path: "~/.codex/skills; ~/.codex/superpowers/skills".into(),
        count,
        valid_count,
        invalid_count,
        skills,
        task,
    })
}

fn parse_remote_skill_list(stdout: &str) -> Vec<RemoteSkill> {
    let mut seen_paths = BTreeSet::new();
    stdout
        .lines()
        .filter_map(|line| {
            let parsed = if line.starts_with("CODEXHUB_REMOTE_SKILL\t") {
                let parts = line.splitn(6, '\t').collect::<Vec<_>>();
                let (marker, name, has_skill_md, status, path, description) = match parts.as_slice()
                {
                    [marker, name, has_skill_md, status, path, description] => {
                        (*marker, *name, *has_skill_md, *status, *path, *description)
                    }
                    [marker, name, has_skill_md, status, path] => {
                        (*marker, *name, *has_skill_md, *status, *path, "")
                    }
                    _ => return None,
                };
                if marker != "CODEXHUB_REMOTE_SKILL" {
                    return None;
                }
                (
                    name.to_string(),
                    has_skill_md == "yes",
                    status.to_string(),
                    path.to_string(),
                    description.trim().to_string(),
                )
            } else {
                let parts = line.split_whitespace().collect::<Vec<_>>();
                if parts.len() < 5 {
                    return None;
                }
                let marker = parts[0];
                if marker != "CODEXHUB_REMOTE_SKILL" {
                    return None;
                }
                // Task-log redaction collapses tab-delimited marker rows to spaces,
                // so keep the first five structural fields and treat the rest as
                // the optional SKILL.md description.
                (
                    parts[1].to_string(),
                    parts[2] == "yes",
                    parts[3].to_string(),
                    parts[4].to_string(),
                    parts[5..].join(" "),
                )
            };
            let (name, has_skill_md, status, path, description) = parsed;
            if !seen_paths.insert(path.to_ascii_lowercase()) {
                return None;
            }
            Some(RemoteSkill {
                name,
                has_skill_md,
                status,
                path,
                description,
            })
        })
        .collect()
}

fn remote_installed_skill_archive_script(remote_path: &str, archive_path: &str) -> String {
    format!(
        r#"set -u
target={remote_path}
archive={archive_path}
if [ ! -d "$target" ]; then
  printf 'Installed skill target is missing or is not a directory: %s\n' "$target" >&2
  exit 2
fi
if [ ! -f "$target/SKILL.md" ]; then
  printf 'Installed skill target does not contain SKILL.md: %s\n' "$target" >&2
  exit 3
fi
if ! command -v tar >/dev/null 2>&1; then
  printf 'tar is required on the remote host for skill download.\n' >&2
  exit 4
fi
mkdir -p "${{archive%/*}}"
rm -f "$archive"
parent=${{target%/*}}
base=${{target##*/}}
if [ -z "$parent" ] || [ "$parent" = "$target" ] || [ -z "$base" ]; then
  printf 'Installed skill target path is not usable: %s\n' "$target" >&2
  exit 5
fi
tar -czf "$archive" -C "$parent" "$base"
printf 'CODEXHUB_SKILL_ARCHIVE=%s\n' "$archive"
"#,
        remote_path = shell_single_quote(remote_path),
        archive_path = shell_single_quote(archive_path)
    )
}

fn run_remote_skill_install(
    app: &AppHandle,
    state: &AppState,
    host_alias: String,
    skill_id: String,
    scope: RemoteSkillScope,
    project_path: Option<String>,
    conflict_policy: SkillConflictPolicy,
    timeout_ms: Option<u64>,
) -> Result<RemoteSkillInstallResult, String> {
    let skill = find_skill(app, state, &skill_id)?;
    let timeout = ssh::normalize_timeout_ms(timeout_ms.or(Some(120_000)));
    let alias = ssh::validate_ssh_alias(&host_alias)?;
    let target_display = remote_skill_target_display(&scope, project_path.as_deref(), &skill.id)?;
    let (root_expr, _) = remote_skill_root(&scope, project_path.as_deref())?;
    let task_id = format!("task-skill-install-{}", timestamp_millis());
    let host_id = host_id_for_alias(state, &alias);
    let host_name = host_name_for_alias(state, &alias);
    let mut logs = Vec::new();
    let mut next_log = 1;
    let check_output = ssh::run_ssh_echo_ok(&alias, timeout)
        .unwrap_or_else(|error| failed_command_output(format!("ssh {alias} echo ok"), error));
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
        let summary = "Skill install skipped because SSH check failed.";
        let task = skill_task(
            &task_id,
            &host_id,
            &host_name,
            "Install skill",
            TaskStatus::Failed,
            summary,
            logs,
        );
        record_task(state, task.clone());
        return Ok(RemoteSkillInstallResult {
            host_alias: alias,
            ok: false,
            skill_id: skill.id,
            skill_name: skill.name,
            scope,
            target_path: target_display,
            backup_path: None,
            skipped: false,
            message: summary.into(),
            task,
        });
    }

    let local_archive = match write_skill_archive(app, &skill, &task_id) {
        Ok(path) => path,
        Err(error) => {
            let output = failed_command_output("create local skill archive".into(), error);
            logs.push(command_log(
                &task_id,
                next_log,
                TaskLogLevel::Error,
                "Could not create local skill archive.",
                &output,
            ));
            let task = skill_task(
                &task_id,
                &host_id,
                &host_name,
                "Install skill",
                TaskStatus::Failed,
                "Skill install failed before upload.",
                logs,
            );
            record_task(state, task.clone());
            return Ok(RemoteSkillInstallResult {
                host_alias: alias,
                ok: false,
                skill_id: skill.id,
                skill_name: skill.name,
                scope,
                target_path: target_display,
                backup_path: None,
                skipped: false,
                message: task.summary.clone(),
                task,
            });
        }
    };
    let remote_archive = format!("/tmp/codexhub-skill-{task_id}.tgz");
    let upload_output = ssh::upload_file(&alias, &local_archive, &remote_archive, timeout)
        .unwrap_or_else(|error| failed_command_output(format!("scp {remote_archive}"), error));
    let _ = fs::remove_file(&local_archive);
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
            "Uploaded skill archive to remote staging path."
        } else {
            "Failed to upload skill archive."
        },
        &upload_output,
    ));
    next_log += 1;
    if !upload_ok {
        let task = skill_task(
            &task_id,
            &host_id,
            &host_name,
            "Install skill",
            TaskStatus::Failed,
            "Skill install failed during upload; remote skills were not changed.",
            logs,
        );
        record_task(state, task.clone());
        return Ok(RemoteSkillInstallResult {
            host_alias: alias,
            ok: false,
            skill_id: skill.id,
            skill_name: skill.name,
            scope,
            target_path: target_display,
            backup_path: None,
            skipped: false,
            message: task.summary.clone(),
            task,
        });
    }

    let script = remote_skill_install_script(
        &remote_archive,
        &root_expr,
        &skill.id,
        &conflict_policy,
        &timestamp_label(),
    );
    let output = ssh::run_ssh_script(&alias, &script, timeout).unwrap_or_else(|error| {
        failed_command_output(format!("ssh {alias} install skill {}", skill.id), error)
    });
    let ok = output.success();
    let skipped = marker_value(&output.stdout, "CODEXHUB_SKILL_SKIPPED").as_deref() == Some("yes");
    let backup_path = marker_value(&output.stdout, "CODEXHUB_SKILL_BACKUP");
    logs.push(command_log(
        &task_id,
        next_log,
        if ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        if ok {
            "Validated and installed remote skill."
        } else {
            "Failed to validate or install remote skill."
        },
        &output,
    ));
    let status = if ok {
        TaskStatus::Success
    } else {
        TaskStatus::Failed
    };
    let summary = if ok && skipped {
        format!(
            "{} already exists on {}; install skipped.",
            skill.name, alias
        )
    } else if ok {
        let count = marker_value(&output.stdout, "CODEXHUB_SKILL_COUNT")
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or_else(|| remote_count_after_skill_install(state, &alias));
        update_host_skills(state, &alias, true, count);
        match backup_path.as_deref() {
            Some(path) => format!(
                "{} installed to {} with backup {}.",
                skill.name, target_display, path
            ),
            None => format!("{} installed to {}.", skill.name, target_display),
        }
    } else {
        format!(
            "{} could not be installed to {}; see task logs.",
            skill.name, target_display
        )
    };
    let task = skill_task(
        &task_id,
        &host_id,
        &host_name,
        "Install skill",
        status,
        &summary,
        logs,
    );
    record_task(state, task.clone());
    Ok(RemoteSkillInstallResult {
        host_alias: alias,
        ok,
        skill_id: skill.id,
        skill_name: skill.name,
        scope,
        target_path: target_display,
        backup_path,
        skipped,
        message: summary,
        task,
    })
}

fn remote_count_after_skill_install(state: &AppState, alias: &str) -> u16 {
    state
        .hosts
        .lock()
        .expect("hosts mutex poisoned")
        .iter()
        .find(|host| host.host_alias.eq_ignore_ascii_case(alias))
        .and_then(|host| host.skills_count)
        .unwrap_or(0)
        .saturating_add(1)
}

fn run_remote_installed_skill_delete(
    state: &AppState,
    request: &InstalledSkillRequest,
    timeout_ms: Option<u64>,
) -> Result<RemoteSkillDeleteResult, String> {
    let alias = request
        .host_alias
        .as_deref()
        .ok_or_else(|| "Host alias is required.".to_string())
        .and_then(ssh::validate_ssh_alias)?;
    validate_cached_remote_skill_path(&request.path)?;
    let timeout = ssh::normalize_timeout_ms(timeout_ms.or(Some(30_000)));
    let task_id = format!("task-skill-delete-{}", timestamp_millis());
    let host_id = host_id_for_alias(state, &alias);
    let host_name = host_name_for_alias(state, &alias);
    let script = remote_installed_skill_delete_script(&request.path);
    let output = ssh::run_ssh_script(&alias, &script, timeout).unwrap_or_else(|error| {
        failed_command_output(
            format!("ssh {alias} delete installed skill {}", request.skill_name),
            error,
        )
    });
    let ok = output.success();
    let logs = vec![command_log(
        &task_id,
        1,
        if ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        if ok {
            "Permanently removed remote installed skill directory."
        } else {
            "Failed to permanently remove remote installed skill directory."
        },
        &output,
    )];
    let summary = if ok {
        let count = marker_value(&output.stdout, "CODEXHUB_SKILL_COUNT")
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or_else(|| remote_count_after_skill_delete(state, &alias));
        update_host_skills(state, &alias, true, count);
        format!("{} removed from {}.", request.skill_name, alias)
    } else {
        format!(
            "{} could not be removed from {}: {}",
            request.skill_name,
            alias,
            command_detail(&output)
        )
    };
    let task = skill_task(
        &task_id,
        &host_id,
        &host_name,
        "Uninstall installed skill",
        if ok {
            TaskStatus::Success
        } else {
            TaskStatus::Failed
        },
        &summary,
        logs,
    );
    record_task(state, task.clone());
    Ok(RemoteSkillDeleteResult {
        host_alias: alias,
        ok,
        skill_name: request.skill_name.clone(),
        target_path: request.path.clone(),
        backup_path: None,
        message: summary,
        task,
    })
}

fn run_remote_skill_delete(
    state: &AppState,
    host_alias: String,
    skill_name: String,
    scope: RemoteSkillScope,
    project_path: Option<String>,
    confirm_name: String,
    timeout_ms: Option<u64>,
) -> Result<RemoteSkillDeleteResult, String> {
    let skill_name = validate_remote_skill_dir_name(&skill_name)?;
    if confirm_name.trim() != skill_name {
        return Err(format!("Confirmation must exactly match {skill_name}."));
    }
    let timeout = ssh::normalize_timeout_ms(timeout_ms.or(Some(30_000)));
    let alias = ssh::validate_ssh_alias(&host_alias)?;
    let target_display = remote_skill_target_display(&scope, project_path.as_deref(), &skill_name)?;
    let (root_expr, _) = remote_skill_root(&scope, project_path.as_deref())?;
    let task_id = format!("task-skill-delete-{}", timestamp_millis());
    let host_id = host_id_for_alias(state, &alias);
    let host_name = host_name_for_alias(state, &alias);
    let script = remote_skill_delete_script(&root_expr, &skill_name, &timestamp_label());
    let output = ssh::run_ssh_script(&alias, &script, timeout).unwrap_or_else(|error| {
        failed_command_output(format!("ssh {alias} delete skill {skill_name}"), error)
    });
    let ok = output.success();
    let backup_path = marker_value(&output.stdout, "CODEXHUB_SKILL_BACKUP");
    let logs = vec![command_log(
        &task_id,
        1,
        if ok {
            TaskLogLevel::Info
        } else {
            TaskLogLevel::Error
        },
        if ok {
            "Permanently removed remote skill directory."
        } else {
            "Failed to delete remote skill."
        },
        &output,
    )];
    let summary = if ok {
        let count = marker_value(&output.stdout, "CODEXHUB_SKILL_COUNT")
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or_else(|| remote_count_after_skill_delete(state, &alias));
        update_host_skills(state, &alias, true, count);
        match backup_path.as_deref() {
            Some(path) => format!("{skill_name} removed from {alias}; backup at {path}."),
            None if output.stdout.contains("Skill was not present.") => {
                format!("{skill_name} was not present on {alias}.")
            }
            None => format!("{skill_name} permanently removed from {alias}."),
        }
    } else {
        format!(
            "{skill_name} could not be removed from {alias}: {}",
            command_detail(&output)
        )
    };
    let task = skill_task(
        &task_id,
        &host_id,
        &host_name,
        "Delete skill",
        if ok {
            TaskStatus::Success
        } else {
            TaskStatus::Failed
        },
        &summary,
        logs,
    );
    record_task(state, task.clone());
    Ok(RemoteSkillDeleteResult {
        host_alias: alias,
        ok,
        skill_name,
        target_path: target_display,
        backup_path,
        message: summary,
        task,
    })
}

fn remote_count_after_skill_delete(state: &AppState, alias: &str) -> u16 {
    state
        .hosts
        .lock()
        .expect("hosts mutex poisoned")
        .iter()
        .find(|host| host.host_alias.eq_ignore_ascii_case(alias))
        .and_then(|host| host.skills_count)
        .unwrap_or(1)
        .saturating_sub(1)
}

fn remote_skill_install_script(
    archive_path: &str,
    root_expr: &str,
    skill_name: &str,
    policy: &SkillConflictPolicy,
    timestamp: &str,
) -> String {
    let policy = match policy {
        SkillConflictPolicy::Backup => "backup",
        SkillConflictPolicy::Skip => "skip",
        SkillConflictPolicy::Overwrite => "overwrite",
    };
    format!(
        r#"set -u
archive={archive_path}
root={root_expr}
skill_name={skill_name}
policy={policy}
timestamp={timestamp}
target="$root/$skill_name"
backup="$root/$skill_name.codexhub.bak.$timestamp"
extract_dir="${{TMPDIR:-/tmp}}/codexhub-skill-extract.$$"
stage="$root/.codexhub-stage-$skill_name-$timestamp.$$"
cleanup() {{
  rm -rf "$extract_dir" "$stage"
  rm -f "$archive"
}}
trap cleanup EXIT HUP INT TERM
if [ ! -s "$archive" ]; then
  printf 'Uploaded skill archive is missing or empty: %s\n' "$archive" >&2
  exit 2
fi
if ! command -v tar >/dev/null 2>&1; then
  printf 'tar is required on the remote host for skill install.\n' >&2
  exit 3
fi
if ! tar -tzf "$archive" >/dev/null 2>&1; then
  printf 'Uploaded skill archive is not a readable gzip tarball.\n' >&2
  exit 4
fi
if tar -tzf "$archive" | grep -Eq '(^|/)\.\.(/|$)|^/'; then
  printf 'Uploaded skill archive contains unsafe paths.\n' >&2
  exit 5
fi
rm -rf "$extract_dir" "$stage"
mkdir -p "$extract_dir" "$stage" "$root"
tar -xzf "$archive" -C "$extract_dir"
source_dir="$extract_dir/skill"
if [ ! -f "$source_dir/SKILL.md" ]; then
  printf 'Uploaded skill does not contain SKILL.md at archive root.\n' >&2
  exit 6
fi
cp -R "$source_dir/." "$stage/"
if [ ! -f "$stage/SKILL.md" ]; then
  printf 'Staged skill does not contain SKILL.md after copy.\n' >&2
  exit 7
fi
backup_path=""
skipped=no
if [ -e "$target" ]; then
  case "$policy" in
    skip)
      skipped=yes
      rm -rf "$stage"
      ;;
    backup)
      if [ -e "$backup" ]; then
        backup="$backup.$$"
      fi
      mv "$target" "$backup"
      backup_path="$backup"
      mv "$stage" "$target"
      ;;
    overwrite)
      rm -rf "$target"
      mv "$stage" "$target"
      ;;
    *)
      printf 'Unknown conflict policy: %s\n' "$policy" >&2
      exit 8
      ;;
  esac
else
  mv "$stage" "$target"
fi
printf 'CODEXHUB_SKILL_TARGET=%s\n' "$target"
printf 'CODEXHUB_SKILL_BACKUP=%s\n' "$backup_path"
printf 'CODEXHUB_SKILL_SKIPPED=%s\n' "$skipped"
count=0
for dir in "$root"/*; do
  [ -d "$dir" ] || continue
  count=$((count + 1))
done
printf 'CODEXHUB_SKILL_COUNT=%s\n' "$count"
"#,
        archive_path = shell_single_quote(archive_path),
        root_expr = root_expr,
        skill_name = shell_single_quote(skill_name),
        policy = shell_single_quote(policy),
        timestamp = shell_single_quote(timestamp)
    )
}

fn remote_skill_delete_script(root_expr: &str, skill_name: &str, timestamp: &str) -> String {
    format!(
        r#"set -u
root={root_expr}
skill_name={skill_name}
timestamp={timestamp}
target="$root/$skill_name"
if [ ! -e "$target" ]; then
  printf 'CODEXHUB_SKILL_TARGET=%s\n' "$target"
  printf 'CODEXHUB_SKILL_BACKUP=\n'
  printf 'Skill was not present.\n'
  exit 0
fi
if [ ! -d "$target" ]; then
  printf 'Remote skill target exists but is not a directory: %s\n' "$target" >&2
  exit 2
fi
rm -rf "$target"
printf 'CODEXHUB_SKILL_TARGET=%s\n' "$target"
printf 'CODEXHUB_SKILL_BACKUP=\n'
count=0
for dir in "$root"/*; do
  [ -d "$dir" ] || continue
  count=$((count + 1))
done
printf 'CODEXHUB_SKILL_COUNT=%s\n' "$count"
"#,
        root_expr = root_expr,
        skill_name = shell_single_quote(skill_name),
        timestamp = shell_single_quote(timestamp)
    )
}

fn remote_installed_skill_delete_script(remote_path: &str) -> String {
    format!(
        r#"set -u
target={remote_path}
if [ ! -e "$target" ]; then
  printf 'CODEXHUB_SKILL_TARGET=%s\n' "$target"
  printf 'Skill was not present.\n'
  exit 0
fi
if [ ! -d "$target" ]; then
  printf 'Remote skill target exists but is not a directory: %s\n' "$target" >&2
  exit 2
fi
rm -rf "$target"
printf 'CODEXHUB_SKILL_TARGET=%s\n' "$target"
count=0
for root in "$HOME/.codex/skills" "$HOME/.codex/superpowers/skills"; do
  [ -d "$root" ] || continue
  for dir in "$root"/* "$root"/.[!.]* "$root"/..?*; do
    [ -d "$dir" ] || continue
    count=$((count + 1))
  done
done
printf 'CODEXHUB_SKILL_COUNT=%s\n' "$count"
"#,
        remote_path = shell_single_quote(remote_path)
    )
}

fn skill_task(
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
        started_at: "now".into(),
        ended_at: Some("now".into()),
        summary: summary.to_string(),
        logs,
    }
}

fn update_host_skills(state: &AppState, alias: &str, exists: bool, count: u16) {
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");
    if let Some(host) = hosts
        .iter_mut()
        .find(|host| host.host_alias.eq_ignore_ascii_case(alias))
    {
        host.status = HostStatus::Online;
        host.skills_exists = Some(exists);
        host.skills_count = Some(count);
        host.last_seen = "just now".into();
    }
}

fn find_profile(app: &AppHandle, state: &AppState, profile_id: &str) -> Result<Profile, String> {
    load_profiles(app, state)?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| format!("Profile {profile_id} was not found."))
}

fn profile_from_draft(draft: ProfileDraft) -> Result<Profile, String> {
    let now = timestamp_label();
    let name = normalize_required_text("Profile name", &draft.name)?;
    let model = normalize_required_text("Model", &draft.model)?;
    Ok(Profile {
        id: format!("{}-{}", slugify(&name), timestamp_millis()),
        name,
        description: draft.description.unwrap_or_default(),
        model,
        provider: normalize_optional_text(draft.provider).unwrap_or_else(|| "openai".into()),
        base_url: normalize_optional_text(draft.base_url),
        api_key_env_var: normalize_optional_text(draft.api_key_env_var),
        model_reasoning_effort: normalize_optional_text(draft.model_reasoning_effort),
        plan_mode_reasoning_effort: normalize_optional_text(draft.plan_mode_reasoning_effort),
        fast_mode: draft.fast_mode.unwrap_or(false),
        service_tier: normalize_optional_text(draft.service_tier),
        approval_policy: normalize_optional_text(draft.approval_policy)
            .unwrap_or_else(|| "on-request".into()),
        sandbox_mode: normalize_optional_text(draft.sandbox_mode)
            .unwrap_or_else(|| "workspace-write".into()),
        extra_toml: draft.extra_toml.unwrap_or_default(),
        created_at: now.clone(),
        updated_at: now,
        source: normalize_optional_text(draft.source).unwrap_or_else(|| "manual".into()),
        credential_stored: false,
        host_ids: draft.host_ids.unwrap_or_default(),
    })
}

fn apply_profile_patch(profile: &mut Profile, patch: ProfilePatch) -> Result<(), String> {
    if let Some(name) = patch.name {
        profile.name = normalize_required_text("Profile name", &name)?;
    }
    if let Some(description) = patch.description {
        profile.description = description;
    }
    if let Some(model) = patch.model {
        profile.model = normalize_required_text("Model", &model)?;
    }
    if let Some(provider) = patch.provider {
        profile.provider = normalize_required_text("Provider", &provider)?;
    }
    if patch.base_url.is_some() {
        profile.base_url = normalize_optional_text(patch.base_url);
    }
    if patch.api_key_env_var.is_some() {
        profile.api_key_env_var = normalize_optional_text(patch.api_key_env_var);
    }
    if patch.model_reasoning_effort.is_some() {
        profile.model_reasoning_effort = normalize_optional_text(patch.model_reasoning_effort);
    }
    if patch.plan_mode_reasoning_effort.is_some() {
        profile.plan_mode_reasoning_effort =
            normalize_optional_text(patch.plan_mode_reasoning_effort);
    }
    if let Some(fast_mode) = patch.fast_mode {
        profile.fast_mode = fast_mode;
    }
    if patch.service_tier.is_some() {
        profile.service_tier = normalize_optional_text(patch.service_tier);
    }
    if let Some(approval_policy) = patch.approval_policy {
        profile.approval_policy = normalize_required_text("Approval policy", &approval_policy)?;
    }
    if let Some(sandbox_mode) = patch.sandbox_mode {
        profile.sandbox_mode = normalize_required_text("Sandbox mode", &sandbox_mode)?;
    }
    if let Some(extra_toml) = patch.extra_toml {
        profile.extra_toml = extra_toml;
    }
    if let Some(source) = patch.source {
        profile.source = normalize_required_text("Source", &source)?;
    }
    if let Some(credential_stored) = patch.credential_stored {
        profile.credential_stored = credential_stored && profile_api_key_exists(&profile.id);
    }
    if let Some(host_ids) = patch.host_ids {
        profile.host_ids = host_ids;
    }
    Ok(())
}

fn validate_profile(profile: &Profile) -> Result<(), String> {
    normalize_required_text("Profile id", &profile.id)?;
    normalize_required_text("Profile name", &profile.name)?;
    normalize_required_text("Model", &profile.model)?;
    normalize_required_text("Provider", &profile.provider)?;
    normalize_required_text("Approval policy", &profile.approval_policy)?;
    normalize_required_text("Sandbox mode", &profile.sandbox_mode)?;
    validate_extra_toml(profile)?;
    let rendered = serde_json::to_string(profile).map_err(|error| error.to_string())?;
    if contains_key_material(&rendered) {
        return Err("Profile contains data that looks like API key material.".into());
    }
    Ok(())
}

fn ensure_unique_profile_id(profile: &mut Profile, profiles: &[Profile]) {
    if !profiles.iter().any(|item| item.id == profile.id) {
        return;
    }
    let base = profile.id.clone();
    let mut index = 2;
    while profiles
        .iter()
        .any(|item| item.id == format!("{base}-{index}"))
    {
        index += 1;
    }
    profile.id = format!("{base}-{index}");
}

fn import_profiles_inner(
    app: &AppHandle,
    state: &AppState,
    incoming: Vec<Profile>,
    replace: bool,
) -> Result<ProfileImportResult, String> {
    let mut profiles = if replace {
        Vec::new()
    } else {
        load_profiles(app, state)?
    };
    let mut imported = Vec::new();
    let mut skipped = Vec::new();

    for mut profile in incoming {
        profile.credential_stored = profile_api_key_exists(&profile.id);
        profile.updated_at = timestamp_label();
        if profile.created_at.trim().is_empty() {
            profile.created_at = profile.updated_at.clone();
        }
        match validate_profile(&profile) {
            Ok(()) => {
                profiles.retain(|item| item.id != profile.id);
                if profile.source == "cc-switch" {
                    let incoming_key = cc_switch_profile_import_key(&profile);
                    profiles.retain(|item| {
                        item.source != "cc-switch"
                            || cc_switch_profile_import_key(item) != incoming_key
                    });
                }
                profiles.push(profile.clone());
                imported.push(profile);
            }
            Err(error) => skipped.push(format!("{}: {error}", profile.id)),
        }
    }

    save_profiles(app, state, &profiles)?;
    Ok(ProfileImportResult { imported, skipped })
}

fn refresh_credential_flags(profiles: &mut [Profile]) {
    for profile in profiles {
        profile.credential_stored = profile_api_key_exists(&profile.id);
    }
}

fn clear_profile_from_hosts(state: &AppState, profile_id: &str) {
    let mut hosts = state.hosts.lock().expect("hosts mutex poisoned");
    for host in hosts.iter_mut() {
        if host.profile_id.as_deref() == Some(profile_id) {
            host.profile_id = None;
        }
    }
}

fn render_profile_toml(profile: &Profile) -> Result<String, String> {
    validate_profile(profile)?;
    let mut root = TomlMap::new();
    insert_toml_string(&mut root, "model", &profile.model);
    insert_toml_string(&mut root, "model_provider", &profile.provider);
    insert_toml_string(&mut root, "approval_policy", &profile.approval_policy);
    insert_toml_string(&mut root, "sandbox_mode", &profile.sandbox_mode);
    insert_toml_optional_string(
        &mut root,
        "model_reasoning_effort",
        profile.model_reasoning_effort.as_deref(),
    );
    insert_toml_optional_string(
        &mut root,
        "plan_mode_reasoning_effort",
        profile.plan_mode_reasoning_effort.as_deref(),
    );
    insert_toml_optional_string(&mut root, "service_tier", profile.service_tier.as_deref());

    if profile.provider.eq_ignore_ascii_case("openai") {
        insert_toml_optional_string(&mut root, "openai_base_url", profile.base_url.as_deref());
    } else {
        let provider_key = sanitize_toml_key(&profile.provider)?;
        let mut provider = TomlMap::new();
        insert_toml_string(&mut provider, "name", &profile.provider);
        insert_toml_optional_string(&mut provider, "base_url", profile.base_url.as_deref());
        insert_toml_optional_string(&mut provider, "env_key", profile.api_key_env_var.as_deref());
        let mut provider_tables = TomlMap::new();
        provider_tables.insert(provider_key, TomlValue::Table(provider));
        root.insert("model_providers".into(), TomlValue::Table(provider_tables));
    }

    let mut features = TomlMap::new();
    features.insert("fast_mode".into(), TomlValue::Boolean(profile.fast_mode));
    root.insert("features".into(), TomlValue::Table(features));

    let extra = parse_extra_toml(profile)?;
    merge_toml_table(&mut root, extra)?;
    toml::to_string_pretty(&TomlValue::Table(root)).map_err(|error| error.to_string())
}

fn parse_extra_toml(profile: &Profile) -> Result<TomlMap<String, TomlValue>, String> {
    let trimmed = profile.extra_toml.trim();
    if trimmed.is_empty() {
        return Ok(TomlMap::new());
    }
    let value = trimmed
        .parse::<TomlValue>()
        .map_err(|error| format!("extraToml is not valid TOML: {error}"))?;
    let TomlValue::Table(table) = value else {
        return Err("extraToml must be a TOML table.".into());
    };
    reject_extra_toml_conflicts(profile, &table)?;
    reject_extra_toml_secret_keys(&table, "")?;
    Ok(table)
}

fn validate_extra_toml(profile: &Profile) -> Result<(), String> {
    parse_extra_toml(profile).map(|_| ())
}

fn reject_extra_toml_conflicts(
    profile: &Profile,
    table: &TomlMap<String, TomlValue>,
) -> Result<(), String> {
    let top_level_conflicts = [
        "model",
        "model_provider",
        "approval_policy",
        "sandbox_mode",
        "model_reasoning_effort",
        "plan_mode_reasoning_effort",
        "service_tier",
        "openai_base_url",
    ];
    for key in top_level_conflicts {
        if table.contains_key(key) {
            return Err(format!("extraToml cannot override structured key `{key}`."));
        }
    }
    if let Some(TomlValue::Table(features)) = table.get("features") {
        if features.contains_key("fast_mode") {
            return Err("extraToml cannot override structured key `features.fast_mode`.".into());
        }
    }
    if let Some(TomlValue::Table(model_providers)) = table.get("model_providers") {
        let provider_key = sanitize_toml_key(&profile.provider)?;
        if model_providers.contains_key(&provider_key) {
            return Err(format!(
                "extraToml cannot override structured key `model_providers.{provider_key}`."
            ));
        }
        if model_providers.contains_key("openai") {
            return Err("extraToml cannot define `model_providers.openai`; OpenAI uses the built-in provider.".into());
        }
    }
    Ok(())
}

fn merge_toml_table(
    target: &mut TomlMap<String, TomlValue>,
    source: TomlMap<String, TomlValue>,
) -> Result<(), String> {
    for (key, value) in source {
        match (target.get_mut(&key), value) {
            (Some(TomlValue::Table(target_table)), TomlValue::Table(source_table)) => {
                merge_toml_table(target_table, source_table)?;
            }
            (Some(_), _) => {
                return Err(format!("extraToml conflicts with structured key `{key}`."));
            }
            (None, value) => {
                target.insert(key, value);
            }
        }
    }
    Ok(())
}

fn reject_extra_toml_secret_keys(
    table: &TomlMap<String, TomlValue>,
    prefix: &str,
) -> Result<(), String> {
    for (key, value) in table {
        let path = if prefix.is_empty() {
            key.to_string()
        } else {
            format!("{prefix}.{key}")
        };
        let key_lower = key.to_ascii_lowercase();
        if matches!(
            key_lower.as_str(),
            "api_key" | "apikey" | "token" | "password" | "secret"
        ) {
            return Err(format!(
                "extraToml cannot include secret-like key `{path}`; store local credentials with set_profile_api_key or reference remote environment variables with env_key."
            ));
        }
        if let TomlValue::Table(child) = value {
            reject_extra_toml_secret_keys(child, &path)?;
        }
    }
    Ok(())
}

fn insert_toml_string(table: &mut TomlMap<String, TomlValue>, key: &str, value: &str) {
    table.insert(key.into(), TomlValue::String(value.to_string()));
}

fn insert_toml_optional_string(
    table: &mut TomlMap<String, TomlValue>,
    key: &str,
    value: Option<&str>,
) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        table.insert(key.into(), TomlValue::String(value.to_string()));
    }
}

fn profile_preview_warnings(profile: &Profile) -> Vec<String> {
    let mut warnings = Vec::new();
    if profile.credential_stored {
        warnings.push("Local credential is stored but will not be rendered or uploaded.".into());
    }
    if !profile.provider.eq_ignore_ascii_case("openai") && profile.api_key_env_var.is_none() {
        warnings.push(
            "Custom provider has no env_key; remote authentication must be handled separately."
                .into(),
        );
    }
    warnings
}

fn store_profile_api_key_local(profile_id: &str, api_key: &str) -> Result<(), String> {
    profile_key_entry(profile_id)?
        .set_password(api_key)
        .map_err(|error| format!("Failed to store profile API key in OS credential store: {error}"))
}

fn load_profile_api_key_local(profile_id: &str) -> Result<Option<String>, String> {
    match profile_key_entry(profile_id)?.get_password() {
        Ok(api_key) => Ok(Some(api_key)),
        Err(error) if is_missing_credential_error(&error.to_string()) => Ok(None),
        Err(error) => Err(format!(
            "Failed to read profile API key from OS credential store: {error}"
        )),
    }
}

fn delete_profile_api_key_local(profile_id: &str) -> Result<(), String> {
    match profile_key_entry(profile_id)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(error) if is_missing_credential_error(&error.to_string()) => Ok(()),
        Err(error) => Err(format!(
            "Failed to delete profile API key from OS credential store: {error}"
        )),
    }
}

fn profile_api_key_exists(profile_id: &str) -> bool {
    profile_key_entry(profile_id)
        .and_then(|entry| {
            entry
                .get_password()
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
        .is_ok()
}

fn is_missing_credential_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("no entry")
        || lower.contains("not found")
        || lower.contains("no matching entry")
        || lower.contains("could not find")
}

fn profile_key_entry(profile_id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new("CodexHub", &format!("profile:{profile_id}:api_key"))
        .map_err(|error| format!("OS credential store is unavailable: {error}"))
}

fn detect_cc_switch_profiles_inner(
    app: &AppHandle,
    _state: &AppState,
) -> Result<Vec<DetectedCcSwitchProfile>, String> {
    let mut detected = Vec::new();
    let mut seen = BTreeSet::new();
    for path in cc_switch_candidate_paths(app) {
        if !path.exists() {
            continue;
        }
        let profiles = match path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .as_deref()
        {
            Some("db") | Some("sqlite") | Some("sqlite3") => parse_cc_switch_db_profiles(&path)?,
            _ => {
                let content = match fs::read_to_string(&path) {
                    Ok(content) => content,
                    Err(_) => continue,
                };
                parse_cc_switch_profiles(&content, &path)?
            }
        };
        push_detected_cc_switch_profiles(&mut detected, &mut seen, &path, profiles);
    }
    Ok(detected)
}

fn cc_switch_candidate_paths(app: &AppHandle) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(config_dir) = app.path().app_config_dir() {
        paths.push(config_dir.join("cc-switch").join("profiles.json"));
        paths.push(config_dir.join("cc-switch").join("config.json"));
    }
    if let Some(home) = home_dir() {
        paths.push(home.join(".cc-switch").join("profiles.json"));
        paths.push(home.join(".cc-switch").join("config.json"));
        paths.push(home.join(".cc-switch").join("settings.json"));
        paths.push(home.join(".cc-switch").join("cc-switch.db"));
        paths.push(home.join(".cc-switch").join("cc-switch.sqlite"));
        paths.push(home.join(".config").join("cc-switch").join("profiles.json"));
        paths.push(home.join(".config").join("cc-switch").join("config.json"));
        paths.push(home.join(".config").join("cc-switch").join("settings.json"));
        paths.push(home.join(".config").join("cc-switch").join("cc-switch.db"));
    }
    if let Some(appdata) = env::var_os("APPDATA").map(PathBuf::from) {
        paths.push(appdata.join("com.ccswitch.desktop").join("profiles.json"));
        paths.push(appdata.join("com.ccswitch.desktop").join("config.json"));
        paths.push(appdata.join("com.ccswitch.desktop").join("settings.json"));
        paths.push(appdata.join("cc-switch").join("profiles.json"));
        paths.push(appdata.join("cc-switch").join("config.json"));
    }
    if let Some(localappdata) = env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        paths.push(
            localappdata
                .join("com.ccswitch.desktop")
                .join("cc-switch.db"),
        );
        paths.push(localappdata.join("cc-switch").join("cc-switch.db"));
    }
    paths
}

fn parse_cc_switch_profiles(
    content: &str,
    path: &Path,
) -> Result<Vec<CcSwitchProfileRecord>, String> {
    let json = match serde_json::from_str::<serde_json::Value>(content) {
        Ok(value) => value,
        Err(_) => return Ok(Vec::new()),
    };
    let mut entries = Vec::new();
    collect_cc_switch_profile_entries(&json, &mut entries);
    let mut profiles = Vec::new();
    for (index, item) in entries.iter().enumerate() {
        let name = item
            .get("name")
            .or_else(|| item.get("title"))
            .or_else(|| item.get("label"))
            .and_then(|value| value.as_str())
            .unwrap_or("Imported CC Switch Profile");
        let settings = item
            .get("settings")
            .or_else(|| item.get("settingsConfig"))
            .or_else(|| item.get("settings_config"));
        let config = item
            .get("config")
            .or_else(|| settings.and_then(|value| value.get("config")))
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let mut from_config = cc_switch_profile_from_config(
            &format!("{}-{}", slugify(name), index + 1),
            name,
            config,
            None,
            path,
        );
        let config_api_key = cc_switch_api_key_from_config(config);
        let model = item
            .get("model")
            .or_else(|| settings.and_then(|value| value.get("model")))
            .and_then(|value| value.as_str())
            .or_else(|| {
                from_config
                    .as_ref()
                    .map(|record| record.profile.model.as_str())
            })
            .unwrap_or("gpt-5-codex");
        let provider = item
            .get("provider")
            .or_else(|| item.get("modelProvider"))
            .or_else(|| item.get("model_provider"))
            .or_else(|| settings.and_then(|value| value.get("provider")))
            .and_then(|value| value.as_str())
            .or_else(|| {
                from_config
                    .as_ref()
                    .map(|record| record.profile.provider.as_str())
            })
            .unwrap_or("openai");
        let now = timestamp_label();
        let profile = Profile {
            id: format!("cc-switch-{}-{}", slugify(name), index + 1),
            name: name.to_string(),
            description: format!("Imported from {}", path.display()),
            model: model.to_string(),
            provider: provider.to_string(),
            base_url: item
                .get("base_url")
                .or_else(|| item.get("baseUrl"))
                .or_else(|| item.get("url"))
                .or_else(|| item.get("websiteUrl"))
                .or_else(|| item.get("website_url"))
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .or_else(|| {
                    from_config
                        .as_mut()
                        .and_then(|record| record.profile.base_url.take())
                }),
            api_key_env_var: item
                .get("api_key_env_var")
                .or_else(|| item.get("apiKeyEnvVar"))
                .or_else(|| item.get("env_key"))
                .or_else(|| settings.and_then(|value| value.get("env_key")))
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .or_else(|| {
                    from_config
                        .as_mut()
                        .and_then(|record| record.profile.api_key_env_var.take())
                })
                .or_else(|| Some("OPENAI_API_KEY".into())),
            model_reasoning_effort: item
                .get("model_reasoning_effort")
                .or_else(|| item.get("modelReasoningEffort"))
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .or_else(|| {
                    from_config
                        .as_mut()
                        .and_then(|record| record.profile.model_reasoning_effort.take())
                }),
            plan_mode_reasoning_effort: item
                .get("plan_mode_reasoning_effort")
                .or_else(|| item.get("planModeReasoningEffort"))
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .or_else(|| {
                    from_config
                        .as_mut()
                        .and_then(|record| record.profile.plan_mode_reasoning_effort.take())
                }),
            fast_mode: item
                .get("fast_mode")
                .or_else(|| item.get("fastMode"))
                .and_then(|value| value.as_bool())
                .unwrap_or_else(|| {
                    from_config
                        .as_ref()
                        .map(|record| record.profile.fast_mode)
                        .unwrap_or(false)
                }),
            service_tier: item
                .get("service_tier")
                .or_else(|| item.get("serviceTier"))
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .or_else(|| {
                    from_config
                        .as_mut()
                        .and_then(|record| record.profile.service_tier.take())
                }),
            approval_policy: item
                .get("approval_policy")
                .or_else(|| item.get("approvalPolicy"))
                .and_then(|value| value.as_str())
                .unwrap_or("on-request")
                .to_string(),
            sandbox_mode: item
                .get("sandbox_mode")
                .or_else(|| item.get("sandboxMode"))
                .and_then(|value| value.as_str())
                .unwrap_or("workspace-write")
                .to_string(),
            extra_toml: String::new(),
            created_at: now.clone(),
            updated_at: now,
            source: "cc-switch".into(),
            credential_stored: false,
            host_ids: Vec::new(),
        };
        if validate_profile(&profile).is_ok() {
            profiles.push(CcSwitchProfileRecord {
                api_key: cc_switch_api_key_from_value(item)
                    .or_else(|| settings.and_then(cc_switch_api_key_from_value))
                    .or(config_api_key)
                    .or_else(|| from_config.and_then(|record| record.api_key)),
                profile,
            });
        }
    }
    Ok(profiles)
}

fn collect_cc_switch_profile_entries<'a>(
    value: &'a serde_json::Value,
    entries: &mut Vec<&'a serde_json::Value>,
) {
    if let Some(array) = value.as_array() {
        for item in array {
            collect_cc_switch_profile_entries(item, entries);
        }
        return;
    }
    let Some(object) = value.as_object() else {
        return;
    };
    let app_type = object
        .get("app_type")
        .or_else(|| object.get("appType"))
        .or_else(|| object.get("app"))
        .and_then(|value| value.as_str());
    let has_profile_shape = object.contains_key("model")
        || object.contains_key("provider")
        || object.contains_key("modelProvider")
        || object.contains_key("model_provider")
        || object.contains_key("base_url")
        || object.contains_key("baseUrl")
        || object.contains_key("settings_config")
        || object.contains_key("settingsConfig")
        || object.contains_key("config");
    if has_profile_shape && app_type.map(|value| value == "codex").unwrap_or(true) {
        entries.push(value);
    }
    for key in ["profiles", "providers", "items", "data"] {
        if let Some(child) = object.get(key) {
            collect_cc_switch_profile_entries(child, entries);
        }
    }
}

fn parse_cc_switch_db_profiles(path: &Path) -> Result<Vec<CcSwitchProfileRecord>, String> {
    let sqlite_profiles = parse_cc_switch_sqlite_profiles(path)?;
    if !sqlite_profiles.is_empty() {
        return Ok(sqlite_profiles);
    }
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(Vec::new()),
    };
    let text = String::from_utf8_lossy(&bytes);
    Ok(parse_cc_switch_raw_db_profiles(&text, path))
}

fn parse_cc_switch_sqlite_profiles(path: &Path) -> Result<Vec<CcSwitchProfileRecord>, String> {
    let flags =
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let connection = match rusqlite::Connection::open_with_flags(path, flags) {
        Ok(connection) => connection,
        Err(_) => return Ok(Vec::new()),
    };
    if !sqlite_table_exists(&connection, "providers")? {
        return Ok(Vec::new());
    }

    let current_provider = read_cc_switch_current_provider(path);
    let mut statement = match connection.prepare(
        "SELECT id, name, settings_config, website_url, is_current \
         FROM providers WHERE app_type = 'codex'",
    ) {
        Ok(statement) => statement,
        Err(_) => return Ok(Vec::new()),
    };

    let rows = statement
        .query_map([], |row| {
            Ok(CcSwitchSqliteProvider {
                id: row.get::<_, String>(0)?,
                name: row.get::<_, String>(1)?,
                settings_config: row.get::<_, String>(2)?,
                website_url: row.get::<_, Option<String>>(3).ok().flatten(),
                is_current: row.get::<_, bool>(4).unwrap_or(false),
            })
        })
        .map_err(|error| format!("Failed to query cc-switch providers: {error}"))?;

    let mut profiles = Vec::new();
    for row in rows {
        let row = match row {
            Ok(row) => row,
            Err(_) => continue,
        };
        let settings = match serde_json::from_str::<serde_json::Value>(&row.settings_config) {
            Ok(settings) => settings,
            Err(_) => continue,
        };
        let config = settings
            .get("config")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let endpoint = cc_switch_provider_endpoint(&connection, &row.id)
            .unwrap_or(None)
            .or(row.website_url.clone());
        if let Some(mut record) =
            cc_switch_profile_from_config(&row.id, &row.name, config, endpoint.as_deref(), path)
        {
            record.api_key = cc_switch_api_key_from_value(&settings)
                .or_else(|| cc_switch_api_key_from_config(config))
                .or(record.api_key);
            let is_current = row.is_current || current_provider.as_deref() == Some(row.id.as_str());
            profiles.push((is_current, record));
        }
    }

    profiles.sort_by_key(|(is_current, record)| {
        (!*is_current, record.profile.name.to_ascii_lowercase())
    });
    Ok(dedupe_cc_switch_profiles(
        profiles.into_iter().map(|(_, record)| record).collect(),
    ))
}

struct CcSwitchSqliteProvider {
    id: String,
    name: String,
    settings_config: String,
    website_url: Option<String>,
    is_current: bool,
}

fn sqlite_table_exists(
    connection: &rusqlite::Connection,
    table_name: &str,
) -> Result<bool, String> {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            [table_name],
            |row| row.get(0),
        )
        .map_err(|error| format!("Failed to inspect cc-switch database schema: {error}"))?;
    Ok(count > 0)
}

fn cc_switch_provider_endpoint(
    connection: &rusqlite::Connection,
    provider_id: &str,
) -> Result<Option<String>, String> {
    if !sqlite_table_exists(connection, "provider_endpoints")? {
        return Ok(None);
    }
    match connection.query_row(
        "SELECT url FROM provider_endpoints \
         WHERE provider_id = ?1 AND app_type = 'codex' \
         ORDER BY id ASC LIMIT 1",
        [provider_id],
        |row| row.get::<_, String>(0),
    ) {
        Ok(url) => Ok(Some(url)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(_) => Ok(None),
    }
}

fn read_cc_switch_current_provider(db_path: &Path) -> Option<String> {
    let settings_path = db_path.parent()?.join("settings.json");
    let content = fs::read_to_string(settings_path).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&content).ok()?;
    value
        .get("currentProviderCodex")
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

fn parse_cc_switch_raw_db_profiles(content: &str, path: &Path) -> Vec<CcSwitchProfileRecord> {
    let mut profiles = Vec::new();
    let mut seen_json_starts = BTreeSet::new();
    for marker in ["{\"auth\"", "{\"env\"", "{\"config\""] {
        let mut offset = 0;
        while let Some(relative) = content[offset..].find(marker) {
            let json_start = offset + relative;
            offset = json_start + marker.len();
            if !seen_json_starts.insert(json_start) {
                continue;
            }
            let Some((json, json_end)) = extract_json_object(content, json_start) else {
                continue;
            };
            let Ok(settings) = serde_json::from_str::<serde_json::Value>(json) else {
                continue;
            };
            let Some((record_id, name)) = cc_switch_raw_record_identity(content, json_start) else {
                continue;
            };
            let config = settings
                .get("config")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let fallback_url = extract_url_after(content, json_end);
            if let Some(mut record) = cc_switch_profile_from_config(
                &record_id,
                &name,
                config,
                fallback_url.as_deref(),
                path,
            ) {
                record.api_key = cc_switch_api_key_from_value(&settings)
                    .or_else(|| cc_switch_api_key_from_config(config))
                    .or(record.api_key);
                profiles.push(record);
            }
        }
    }
    dedupe_cc_switch_profiles(profiles)
}

fn extract_json_object(content: &str, start: usize) -> Option<(&str, usize)> {
    if !content[start..].starts_with('{') {
        return None;
    }
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, ch) in content[start..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let end = start + offset + ch.len_utf8();
                    return Some((&content[start..end], end));
                }
            }
            _ => {}
        }
    }
    None
}

fn cc_switch_raw_record_identity(content: &str, json_start: usize) -> Option<(String, String)> {
    let prefix_start = json_start.saturating_sub(260);
    let prefix = &content[prefix_start..json_start];
    let marker = prefix.rfind("codex")?;
    let name = clean_cc_switch_raw_field(&prefix[marker + "codex".len()..]);
    if name.is_empty() || name.contains("session") {
        return None;
    }
    let before_marker = &prefix[..marker];
    let id = last_uuid(before_marker).or_else(|| last_cc_switch_ascii_token(before_marker))?;
    if id.contains("session") {
        return None;
    }
    Some((id, name))
}

fn clean_cc_switch_raw_field(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|ch| {
            if ch.is_control() || ch == '\u{fffd}' {
                ' '
            } else {
                ch
            }
        })
        .collect();
    cleaned
        .trim_matches(|ch: char| {
            !(ch.is_alphanumeric() || matches!(ch, ' ' | '-' | '_' | '.' | '(' | ')'))
        })
        .trim()
        .to_string()
}

fn last_uuid(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    if bytes.len() < 36 {
        return None;
    }
    for start in (0..=bytes.len() - 36).rev() {
        let candidate = &value[start..start + 36];
        if is_uuid_like(candidate) {
            return Some(candidate.to_string());
        }
    }
    None
}

fn is_uuid_like(value: &str) -> bool {
    value.len() == 36
        && value.char_indices().all(|(index, ch)| {
            matches!(index, 8 | 13 | 18 | 23) && ch == '-'
                || !matches!(index, 8 | 13 | 18 | 23) && ch.is_ascii_hexdigit()
        })
}

fn last_cc_switch_ascii_token(value: &str) -> Option<String> {
    let trimmed = value.trim_end_matches(|ch: char| {
        !(ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    });
    let start = trimmed
        .rfind(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.')))
        .map(|index| index + 1)
        .unwrap_or(0);
    let token = trimmed[start..].trim();
    if matches!(token, "default" | "codex-official") || token.ends_with("-official") {
        Some(token.to_string())
    } else {
        None
    }
}

fn extract_url_after(content: &str, start: usize) -> Option<String> {
    let end = (start + 420).min(content.len());
    let window = &content[start..end];
    let http = window.find("http://");
    let https = window.find("https://");
    let offset = match (http, https) {
        (Some(http), Some(https)) => http.min(https),
        (Some(http), None) => http,
        (None, Some(https)) => https,
        (None, None) => return None,
    };
    let url = window[offset..]
        .chars()
        .take_while(|ch| {
            !ch.is_whitespace()
                && !ch.is_control()
                && !matches!(ch, '"' | '\'' | '<' | '>' | '{' | '}' | '[' | ']')
        })
        .collect::<String>()
        .trim_end_matches(|ch| matches!(ch, ',' | ';' | ')' | '('))
        .to_string();
    if url.starts_with("http://") || url.starts_with("https://") {
        Some(url)
    } else {
        None
    }
}

fn cc_switch_profile_from_config(
    record_id: &str,
    name: &str,
    config: &str,
    fallback_url: Option<&str>,
    path: &Path,
) -> Option<CcSwitchProfileRecord> {
    let parsed = config.parse::<TomlValue>().ok();
    let root = parsed.as_ref().and_then(TomlValue::as_table);
    let model = root
        .and_then(|table| toml_string(table, "model"))
        .or_else(|| toml_line_string(config, "model"))
        .unwrap_or_else(|| "gpt-5-codex".into());
    let provider = root
        .and_then(|table| toml_string(table, "model_provider"))
        .or_else(|| toml_line_string(config, "model_provider"))
        .unwrap_or_else(|| "openai".into());
    let provider_table = root
        .and_then(|table| table.get("model_providers"))
        .and_then(TomlValue::as_table)
        .and_then(|providers| providers.get(&provider))
        .and_then(TomlValue::as_table);
    let base_url = provider_table
        .and_then(|table| toml_string(table, "base_url"))
        .or_else(|| root.and_then(|table| toml_string(table, "openai_base_url")))
        .or_else(|| toml_line_string(config, "base_url"))
        .or_else(|| fallback_url.map(str::to_string));
    let api_key_env_var = provider_table
        .and_then(|table| toml_string(table, "env_key"))
        .or_else(|| toml_line_string(config, "env_key"))
        .or_else(|| Some("OPENAI_API_KEY".into()));
    let api_key = cc_switch_api_key_from_config(config);
    let model_reasoning_effort = root
        .and_then(|table| toml_string(table, "model_reasoning_effort"))
        .or_else(|| toml_line_string(config, "model_reasoning_effort"));
    let plan_mode_reasoning_effort = root
        .and_then(|table| toml_string(table, "plan_mode_reasoning_effort"))
        .or_else(|| toml_line_string(config, "plan_mode_reasoning_effort"));
    let service_tier = root
        .and_then(|table| toml_string(table, "service_tier"))
        .or_else(|| toml_line_string(config, "service_tier"));
    let fast_mode = root
        .and_then(|table| table.get("features"))
        .and_then(TomlValue::as_table)
        .and_then(|table| table.get("fast_mode"))
        .and_then(TomlValue::as_bool)
        .unwrap_or(false);

    if config.trim().is_empty() && base_url.is_none() && record_id != "codex-official" {
        return None;
    }

    let now = timestamp_label();
    let profile = Profile {
        id: format!("cc-switch-{}", slugify(record_id)),
        name: name.to_string(),
        description: format!("Imported from {}", path.display()),
        model,
        provider,
        base_url,
        api_key_env_var,
        model_reasoning_effort,
        plan_mode_reasoning_effort,
        fast_mode,
        service_tier,
        approval_policy: "on-request".into(),
        sandbox_mode: "workspace-write".into(),
        extra_toml: String::new(),
        created_at: now.clone(),
        updated_at: now,
        source: "cc-switch".into(),
        credential_stored: false,
        host_ids: Vec::new(),
    };
    validate_profile(&profile)
        .ok()
        .map(|_| CcSwitchProfileRecord { profile, api_key })
}

fn cc_switch_api_key_from_value(value: &serde_json::Value) -> Option<String> {
    let direct_candidates = [
        value
            .get("auth")
            .and_then(|auth| auth.get("api_key"))
            .and_then(|item| item.as_str()),
        value
            .get("auth")
            .and_then(|auth| auth.get("apiKey"))
            .and_then(|item| item.as_str()),
        value.get("api_key").and_then(|item| item.as_str()),
        value.get("apiKey").and_then(|item| item.as_str()),
    ];
    direct_candidates
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|item| !item.is_empty())
        .map(str::to_string)
        .or_else(|| {
            value
                .get("auth")
                .and_then(serde_json::Value::as_object)
                .and_then(|auth| {
                    auth.iter()
                        .filter(|(key, _)| cc_switch_auth_key_may_hold_api_key(key))
                        .filter_map(|(_, value)| value.as_str())
                        .map(str::trim)
                        .find(|item| !item.is_empty())
                        .map(str::to_string)
                })
        })
}

fn cc_switch_auth_key_may_hold_api_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    normalized.contains("apikey") || normalized.ends_with("token")
}

fn cc_switch_api_key_from_config(config: &str) -> Option<String> {
    let parsed = config.parse::<TomlValue>().ok();
    let root = parsed.as_ref().and_then(TomlValue::as_table);
    let provider = root
        .and_then(|table| toml_string(table, "model_provider"))
        .or_else(|| toml_line_string(config, "model_provider"))
        .unwrap_or_else(|| "openai".into());
    let provider_table = root
        .and_then(|table| table.get("model_providers"))
        .and_then(TomlValue::as_table)
        .and_then(|providers| providers.get(&provider))
        .and_then(TomlValue::as_table);
    provider_table
        .and_then(|table| toml_string(table, "api_key"))
        .or_else(|| root.and_then(|table| toml_string(table, "api_key")))
        .or_else(|| toml_line_string(config, "api_key"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn toml_string(table: &TomlMap<String, TomlValue>, key: &str) -> Option<String> {
    table
        .get(key)
        .and_then(TomlValue::as_str)
        .map(str::to_string)
}

fn toml_line_string(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || !line.starts_with(key) {
            continue;
        }
        let Some((left, right)) = line.split_once('=') else {
            continue;
        };
        if left.trim() != key {
            continue;
        }
        let value = right.trim();
        if let Some(stripped) = value
            .strip_prefix('"')
            .and_then(|item| item.strip_suffix('"'))
        {
            return Some(stripped.replace("\\\"", "\""));
        }
    }
    None
}

fn push_detected_cc_switch_profiles(
    detected: &mut Vec<DetectedCcSwitchProfile>,
    seen: &mut BTreeSet<String>,
    path: &Path,
    profiles: Vec<CcSwitchProfileRecord>,
) {
    for record in profiles {
        let key = cc_switch_profile_import_key(&record.profile);
        if seen.insert(key) {
            detected.push(DetectedCcSwitchProfile {
                source_path: path.to_string_lossy().into_owned(),
                profile: record.profile,
                api_key: record.api_key,
            });
        }
    }
}

fn dedupe_cc_switch_profiles(profiles: Vec<CcSwitchProfileRecord>) -> Vec<CcSwitchProfileRecord> {
    let mut seen = BTreeSet::new();
    profiles
        .into_iter()
        .filter(|record| seen.insert(cc_switch_profile_import_key(&record.profile)))
        .collect()
}

fn cc_switch_profile_import_key(profile: &Profile) -> String {
    format!(
        "{}|{}|{}|{}",
        cc_switch_profile_key_part(&profile.name),
        cc_switch_profile_key_part(&profile.provider),
        cc_switch_profile_key_part(&profile.model),
        cc_switch_profile_base_url_key(profile.base_url.as_deref())
    )
}

fn cc_switch_profile_key_part(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn cc_switch_profile_base_url_key(value: Option<&str>) -> String {
    value
        .unwrap_or_default()
        .trim()
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn contains_key_material(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    lower.contains("sk-")
        || lower.contains("password=")
        || lower.contains("token=")
        || lower.contains("-----begin ")
}

fn normalize_required_text(label: &str, value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{label} is required."))
    } else {
        Ok(value.to_string())
    }
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn slugify(value: &str) -> String {
    let slug = value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "profile".into()
    } else {
        slug
    }
}

fn sanitize_toml_key(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("Provider is required.".into());
    }
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
    {
        Ok(value.to_string())
    } else {
        Err("Provider may only contain ASCII letters, numbers, hyphens, and underscores.".into())
    }
}

fn timestamp_label() -> String {
    timestamp_millis().to_string()
}

fn date_label() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn default_true() -> bool {
    true
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(PathBuf::from))
}

fn mock_hosts() -> Vec<Host> {
    Vec::new()
}

fn mock_profiles() -> Vec<Profile> {
    Vec::new()
}

fn mock_skill_packs() -> Vec<SkillPack> {
    Vec::new()
}

fn mock_tasks() -> Vec<TaskRun> {
    Vec::new()
}
