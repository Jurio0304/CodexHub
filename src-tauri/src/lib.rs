mod ssh;

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager, State};

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
    address: String,
    port: u16,
    username: String,
    auth_method: AuthMethod,
    status: HostStatus,
    os: String,
    codex_version: String,
    profile_id: Option<String>,
    skill_pack_ids: Vec<String>,
    tags: Vec<String>,
    last_seen: String,
    latency_ms: Option<u16>,
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
    latency_ms: Option<u16>,
    message: String,
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
    state.hosts.lock().expect("hosts mutex poisoned").clone()
}

#[tauri::command]
fn add_host(state: State<'_, AppState>, draft: HostDraft) -> Host {
    let host = Host {
        id: format!("host-{}", timestamp_millis()),
        name: draft.name,
        address: draft.address,
        port: draft.port,
        username: draft.username,
        auth_method: draft.auth_method,
        status: HostStatus::Unknown,
        os: "Unknown".into(),
        codex_version: "pending".into(),
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
        address: patch.address.unwrap_or_else(|| "127.0.0.1".into()),
        port: patch.port.unwrap_or(22),
        username: patch.username.unwrap_or_else(|| "codex".into()),
        auth_method: patch.auth_method.unwrap_or(AuthMethod::SshKey),
        status: patch.status.unwrap_or(HostStatus::Unknown),
        os: "Unknown".into(),
        codex_version: "pending".into(),
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
        host.status = if ok { HostStatus::Online } else { HostStatus::Offline };
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
    let first_host_id = host_ids.first().cloned().unwrap_or_else(|| "no-host".into());
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
        summary: format!("{} applied to {} through mock backend.", profile_name, host_name),
        logs: vec![
            TaskLog {
                id: format!("{}-log-1", task_id),
                task_run_id: task_id.clone(),
                level: TaskLogLevel::Info,
                timestamp: "now".into(),
                message: "Reserved apply_profile command accepted host selection.".into(),
            },
            TaskLog {
                id: format!("{}-log-2", task_id),
                task_run_id: task_id.clone(),
                level: TaskLogLevel::Info,
                timestamp: "now".into(),
                message: "No remote files were changed; this is mock data only.".into(),
            },
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
            add_host,
            update_host,
            delete_host,
            test_ssh_connection,
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
            address: "10.0.8.12".into(),
            port: 22,
            username: "jurio".into(),
            auth_method: AuthMethod::SshKey,
            status: HostStatus::Online,
            os: "macOS 15.5".into(),
            codex_version: "0.32.0".into(),
            profile_id: Some("research-default".into()),
            skill_pack_ids: vec!["paper-review".into(), "tauri-builder".into()],
            tags: vec!["local".into(), "gpu".into()],
            last_seen: "2 min ago".into(),
            latency_ms: Some(18),
        },
        Host {
            id: "win-workstation".into(),
            name: "Windows Workstation".into(),
            address: "192.168.31.42".into(),
            port: 22,
            username: "pc".into(),
            auth_method: AuthMethod::Agent,
            status: HostStatus::Unknown,
            os: "Windows 11 Pro".into(),
            codex_version: "pending".into(),
            profile_id: Some("safe-editing".into()),
            skill_pack_ids: vec!["tauri-builder".into()],
            tags: vec!["desktop".into(), "primary".into()],
            last_seen: "not tested".into(),
            latency_ms: None,
        },
        Host {
            id: "linux-runner".into(),
            name: "Linux Runner".into(),
            address: "172.20.4.8".into(),
            port: 2222,
            username: "codex".into(),
            auth_method: AuthMethod::SshKey,
            status: HostStatus::Offline,
            os: "Ubuntu 24.04 LTS".into(),
            codex_version: "0.31.1".into(),
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
            description: "Summarize papers, extract claims, and prepare structured reading notes.".into(),
            source: "~/.codex/skills/paper-review".into(),
            skill_count: 5,
            enabled: true,
            updated_at: "2026-06-24".into(),
        },
        SkillPack {
            id: "tauri-builder".into(),
            name: "Tauri Builder".into(),
            version: "0.2.0".into(),
            description: "Scaffold, test, and package Tauri desktop features with React and Rust boundaries.".into(),
            source: "./skills/tauri-builder".into(),
            skill_count: 3,
            enabled: true,
            updated_at: "2026-06-20".into(),
        },
        SkillPack {
            id: "windows-diagnostics".into(),
            name: "Windows Diagnostics".into(),
            version: "0.1.5".into(),
            description: "Collect reproducible PowerShell checks for network, shell, and toolchain issues.".into(),
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
            summary: "Research Default rendered to ~/.codex/config.toml with backup codexhub-1042.toml.".into(),
            logs: vec![
                TaskLog {
                    id: "log-1042-1".into(),
                    task_run_id: "task-1042".into(),
                    level: TaskLogLevel::Info,
                    timestamp: "09:14:10".into(),
                    message: "Opened SFTP session and created remote backup.".into(),
                },
                TaskLog {
                    id: "log-1042-2".into(),
                    task_run_id: "task-1042".into(),
                    level: TaskLogLevel::Info,
                    timestamp: "09:14:41".into(),
                    message: "Rendered profile preview matched expected TOML sections.".into(),
                },
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
            summary: "Connection timed out. Check VPN route or host firewall before applying profiles.".into(),
            logs: vec![
                TaskLog {
                    id: "log-1039-1".into(),
                    task_run_id: "task-1039".into(),
                    level: TaskLogLevel::Warn,
                    timestamp: "22:02:18".into(),
                    message: "Mock check marks linux-runner offline for UI validation.".into(),
                },
                TaskLog {
                    id: "log-1039-2".into(),
                    task_run_id: "task-1039".into(),
                    level: TaskLogLevel::Error,
                    timestamp: "22:02:20".into(),
                    message: "Connection timeout is simulated; no SSH socket was opened.".into(),
                },
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
            logs: vec![TaskLog {
                id: "log-1035-1".into(),
                task_run_id: "task-1035".into(),
                level: TaskLogLevel::Info,
                timestamp: "18:25:00".into(),
                message: "Task created from mock backend reservation.".into(),
            }],
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
            logs: vec![TaskLog {
                id: "log-1031-1".into(),
                task_run_id: "task-1031".into(),
                level: TaskLogLevel::Info,
                timestamp: "18:10:12".into(),
                message: "Mock worker is holding this run in progress for UI coverage.".into(),
            }],
        },
    ]
}
