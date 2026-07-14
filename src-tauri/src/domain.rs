use crate::*;

pub(crate) const CODEX_NPM_REGISTRY_URL: &str = "https://registry.npmjs.org/@openai/codex";
pub(crate) const CODEX_LATEST_SOURCE: &str = "npm";
/// Bounds SSH fan-out so batch actions remain responsive without flooding a host fleet.
pub(crate) const HOST_OPERATION_MAX_CONCURRENCY: usize = 6;
pub(crate) const CODEX_LATEST_REFRESH_HOUR: u32 = 4;
pub(crate) const STABLE_UPDATE_ENDPOINT_ENV: &str = "CODEXHUB_STABLE_UPDATE_ENDPOINT";
pub(crate) const STABLE_UPDATER_PUBKEY_ENV: &str = "CODEXHUB_STABLE_UPDATER_PUBKEY";
pub(crate) const STABLE_IDENTIFIER: &str = "app.codexhub.desktop";
pub(crate) const DEV_IDENTIFIER: &str = "dev.codexhub.desktop";
pub(crate) const APP_UPDATE_CHECK_TIMEOUT_SECS: u64 = 30;
pub(crate) const GITHUB_API_ACCEPT: &str = "application/vnd.github+json";
pub(crate) const OCTET_STREAM_ACCEPT: &str = "application/octet-stream";
pub(crate) const MAIN_WINDOW_LABEL: &str = "main";
pub(crate) const CLOSE_BUTTON_BEHAVIOR_REQUESTED_EVENT: &str = "close-button-behavior-requested";
pub(crate) const TRAY_ID: &str = "codexhub-main-tray";
pub(crate) const TRAY_MENU_SHOW_ID: &str = "show_codexhub";
pub(crate) const TRAY_MENU_QUIT_ID: &str = "quit_codexhub";

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "HealthDto")]
pub(crate) struct Health {
    pub(crate) app: &'static str,
    pub(crate) mode: &'static str,
    pub(crate) remote_wrapper_required: bool,
}

#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(rename = "AuthMethodDto")]
pub(crate) enum AuthMethod {
    SshKey,
    Password,
    Agent,
}

#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "HostStatusDto")]
pub(crate) enum HostStatus {
    Online,
    Offline,
    Unknown,
    Testing,
}

#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "HostDto")]
pub(crate) struct Host {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) host_alias: String,
    pub(crate) source: String,
    pub(crate) address: String,
    pub(crate) port: u16,
    pub(crate) username: String,
    pub(crate) auth_method: AuthMethod,
    pub(crate) status: HostStatus,
    pub(crate) os: String,
    pub(crate) arch: String,
    pub(crate) shell: String,
    pub(crate) path: Option<String>,
    pub(crate) path_has_local_bin: Option<bool>,
    pub(crate) codex_command_available: Option<bool>,
    pub(crate) codex_installed: bool,
    pub(crate) codex_version: String,
    pub(crate) config_exists: Option<bool>,
    pub(crate) api_config_name: Option<String>,
    pub(crate) api_config_source: Option<String>,
    pub(crate) api_key_env_var: Option<String>,
    pub(crate) api_key_env_present: Option<bool>,
    pub(crate) skills_exists: Option<bool>,
    pub(crate) skills_count: Option<u16>,
    pub(crate) profile_id: Option<String>,
    pub(crate) skill_pack_ids: Vec<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) last_seen: String,
    #[ts(type = "number | null")]
    pub(crate) latency_ms: Option<u64>,
}

#[derive(Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "HostDraftDto")]
pub(crate) struct HostDraft {
    pub(crate) name: String,
    pub(crate) address: String,
    pub(crate) port: u16,
    pub(crate) username: String,
    pub(crate) auth_method: AuthMethod,
    pub(crate) tags: Vec<String>,
}

#[derive(Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "HostPatchDto")]
pub(crate) struct HostPatch {
    #[ts(optional)]
    pub(crate) name: Option<String>,
    #[ts(optional)]
    pub(crate) address: Option<String>,
    #[ts(optional)]
    pub(crate) port: Option<u16>,
    #[ts(optional)]
    pub(crate) username: Option<String>,
    #[ts(optional)]
    pub(crate) auth_method: Option<AuthMethod>,
    #[ts(optional)]
    pub(crate) status: Option<HostStatus>,
    #[ts(optional)]
    pub(crate) profile_id: Option<String>,
    #[ts(optional)]
    pub(crate) tags: Option<Vec<String>>,
}

#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "ProfileDto")]
pub(crate) struct Profile {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) model: String,
    pub(crate) provider: String,
    pub(crate) base_url: Option<String>,
    pub(crate) api_key_env_var: Option<String>,
    pub(crate) model_reasoning_effort: Option<String>,
    pub(crate) plan_mode_reasoning_effort: Option<String>,
    pub(crate) fast_mode: bool,
    pub(crate) service_tier: Option<String>,
    pub(crate) approval_policy: String,
    pub(crate) sandbox_mode: String,
    pub(crate) extra_toml: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) source: String,
    pub(crate) credential_stored: bool,
    pub(crate) host_ids: Vec<String>,
}

#[derive(Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "ProfileDraftDto")]
pub(crate) struct ProfileDraft {
    pub(crate) name: String,
    #[ts(optional)]
    pub(crate) description: Option<String>,
    pub(crate) model: String,
    #[ts(optional)]
    pub(crate) provider: Option<String>,
    #[ts(optional)]
    pub(crate) base_url: Option<String>,
    #[ts(optional)]
    pub(crate) api_key_env_var: Option<String>,
    #[ts(optional)]
    pub(crate) model_reasoning_effort: Option<String>,
    #[ts(optional)]
    pub(crate) plan_mode_reasoning_effort: Option<String>,
    #[ts(optional)]
    pub(crate) fast_mode: Option<bool>,
    #[ts(optional)]
    pub(crate) service_tier: Option<String>,
    #[ts(optional)]
    pub(crate) approval_policy: Option<String>,
    #[ts(optional)]
    pub(crate) sandbox_mode: Option<String>,
    #[ts(optional)]
    pub(crate) extra_toml: Option<String>,
    #[ts(optional)]
    pub(crate) source: Option<String>,
    #[ts(optional)]
    pub(crate) host_ids: Option<Vec<String>>,
}

#[derive(Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "ProfilePatchDto")]
pub(crate) struct ProfilePatch {
    #[ts(optional)]
    pub(crate) name: Option<String>,
    #[ts(optional)]
    pub(crate) description: Option<String>,
    #[ts(optional)]
    pub(crate) model: Option<String>,
    #[ts(optional)]
    pub(crate) provider: Option<String>,
    #[ts(optional)]
    pub(crate) base_url: Option<String>,
    #[ts(optional)]
    pub(crate) api_key_env_var: Option<String>,
    #[ts(optional)]
    pub(crate) model_reasoning_effort: Option<String>,
    #[ts(optional)]
    pub(crate) plan_mode_reasoning_effort: Option<String>,
    #[ts(optional)]
    pub(crate) fast_mode: Option<bool>,
    #[ts(optional)]
    pub(crate) service_tier: Option<String>,
    #[ts(optional)]
    pub(crate) approval_policy: Option<String>,
    #[ts(optional)]
    pub(crate) sandbox_mode: Option<String>,
    #[ts(optional)]
    pub(crate) extra_toml: Option<String>,
    #[ts(optional)]
    pub(crate) source: Option<String>,
    #[ts(optional)]
    pub(crate) credential_stored: Option<bool>,
    #[ts(optional)]
    pub(crate) host_ids: Option<Vec<String>>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "ProfileApplyPreviewDto")]
pub(crate) struct ProfileApplyPreview {
    pub(crate) profile_id: String,
    pub(crate) profile_name: String,
    pub(crate) rendered_toml: String,
    pub(crate) target_files: Vec<ProfileApplyTargetFile>,
    pub(crate) host_results: Vec<ProfileApplyHostResult>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "ProfileApplyTargetFileDto")]
pub(crate) struct ProfileApplyTargetFile {
    pub(crate) host_id: String,
    pub(crate) host_name: String,
    pub(crate) host_alias: String,
    pub(crate) path: String,
    pub(crate) backup_expected: bool,
    pub(crate) no_change_expected: bool,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "ProfileApplyHostResultDto")]
pub(crate) struct ProfileApplyHostResult {
    pub(crate) host_id: String,
    pub(crate) host_name: String,
    pub(crate) host_alias: String,
    pub(crate) status: String,
    pub(crate) target_path: String,
    pub(crate) backup_path: Option<String>,
    pub(crate) message: String,
    pub(crate) task: Option<TaskRun>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "ProfileApplyBatchResultDto")]
pub(crate) struct ProfileApplyBatchResult {
    pub(crate) profile_id: String,
    pub(crate) ok: bool,
    pub(crate) results: Vec<ProfileApplyHostResult>,
    pub(crate) tasks: Vec<TaskRun>,
    pub(crate) profiles: Vec<Profile>,
    pub(crate) hosts: Vec<Host>,
}

#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "ProfileImportExportDto")]
pub(crate) struct ProfileImportExport {
    pub(crate) schema_version: u16,
    pub(crate) exported_at: String,
    pub(crate) profiles: Vec<Profile>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "ProfileApiKeyResultDto")]
pub(crate) struct ProfileApiKeyResult {
    pub(crate) profile_id: String,
    pub(crate) exists: bool,
    pub(crate) api_key: Option<String>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "ProfileImportResultDto")]
pub(crate) struct ProfileImportResult {
    pub(crate) imported: Vec<Profile>,
    pub(crate) skipped: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DetectedCcSwitchProfile {
    pub(crate) source_path: String,
    pub(crate) profile: Profile,
    #[serde(skip_serializing)]
    pub(crate) api_key: Option<String>,
}

#[derive(Clone)]
pub(crate) struct CcSwitchProfileRecord {
    pub(crate) profile: Profile,
    pub(crate) api_key: Option<String>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "CcSwitchDetectionDto")]
pub(crate) struct CcSwitchDetection {
    pub(crate) detected: bool,
    pub(crate) source_path: Option<String>,
    pub(crate) message: String,
    pub(crate) import_export: ProfileImportExport,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppliedProfileMetadata {
    pub(crate) profile_id: String,
    pub(crate) profile_name: String,
    pub(crate) applied_at: String,
    pub(crate) codexhub_version: String,
}

#[derive(Clone)]
pub(crate) struct RemoteApiConfigMatch {
    pub(crate) name: String,
    pub(crate) source: String,
    pub(crate) profile_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum SafeReconnectDecision {
    Terminate(u32),
    Manual(String),
}

#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "SkillPackDto")]
pub(crate) struct SkillPack {
    pub(crate) id: String,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) version: String,
    #[serde(default)]
    pub(crate) description: String,
    #[serde(default)]
    pub(crate) about: String,
    pub(crate) source_type: String,
    #[serde(default)]
    pub(crate) source: String,
    #[serde(default)]
    pub(crate) original_path: Option<String>,
    #[serde(default)]
    pub(crate) managed_path: String,
    #[serde(default)]
    pub(crate) has_skill_md: bool,
    #[serde(default)]
    pub(crate) skill_count: u16,
    #[serde(default = "default_true")]
    pub(crate) enabled: bool,
    #[serde(default)]
    pub(crate) added_at: String,
    #[serde(default)]
    pub(crate) updated_at: String,
    #[serde(default)]
    pub(crate) applications: Vec<SkillApplication>,
}

#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "SkillApplicationDto")]
pub(crate) struct SkillApplication {
    pub(crate) target_type: String,
    pub(crate) label: String,
    pub(crate) host_alias: Option<String>,
    pub(crate) path: String,
    pub(crate) detected_at: String,
    pub(crate) has_skill_md: bool,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "SkillImportResultDto")]
pub(crate) struct SkillImportResult {
    pub(crate) imported: Vec<SkillPack>,
    pub(crate) skipped: Vec<String>,
    pub(crate) message: String,
}

#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "RemoteSkillDto")]
pub(crate) struct RemoteSkill {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) has_skill_md: bool,
    pub(crate) status: String,
    #[serde(default)]
    pub(crate) description: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteSkillListResult {
    pub(crate) host_alias: String,
    pub(crate) root_path: String,
    pub(crate) count: u16,
    pub(crate) valid_count: u16,
    pub(crate) invalid_count: u16,
    pub(crate) skills: Vec<RemoteSkill>,
    pub(crate) task: TaskRun,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RemoteSkillScope {
    User,
    Project,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SkillConflictPolicy {
    Backup,
    Skip,
    Overwrite,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteSkillInstallResult {
    pub(crate) host_alias: String,
    pub(crate) ok: bool,
    pub(crate) skill_id: String,
    pub(crate) skill_name: String,
    pub(crate) scope: RemoteSkillScope,
    pub(crate) target_path: String,
    pub(crate) backup_path: Option<String>,
    pub(crate) skipped: bool,
    pub(crate) message: String,
    pub(crate) task: TaskRun,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteSkillDeleteResult {
    pub(crate) host_alias: String,
    pub(crate) ok: bool,
    pub(crate) skill_name: String,
    pub(crate) target_path: String,
    pub(crate) backup_path: Option<String>,
    pub(crate) message: String,
    pub(crate) task: TaskRun,
}

#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "SkillInventoryStatusDto")]
pub(crate) struct SkillInventoryStatus {
    #[serde(default)]
    pub(crate) first_host_scan_completed: bool,
    #[serde(default)]
    pub(crate) local_skill_root: String,
    #[serde(default)]
    pub(crate) local_skills: Vec<RemoteSkill>,
    #[serde(default)]
    pub(crate) host_inventories: Vec<HostSkillInventory>,
}

#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "HostSkillInventoryDto")]
pub(crate) struct HostSkillInventory {
    pub(crate) host_alias: String,
    pub(crate) scanned_at: String,
    pub(crate) ok: bool,
    pub(crate) message: String,
    pub(crate) skills: Vec<RemoteSkill>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "SkillDetectionResultDto")]
pub(crate) struct SkillDetectionResult {
    pub(crate) skills: Vec<SkillPack>,
    pub(crate) status: SkillInventoryStatus,
    pub(crate) tasks: Vec<TaskRun>,
    pub(crate) message: String,
}

#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "SkillTargetRequestDto")]
pub(crate) struct SkillTargetRequest {
    pub(crate) target_type: String,
    #[ts(optional)]
    pub(crate) host_alias: Option<String>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "SkillTargetDto")]
pub(crate) struct SkillTarget {
    pub(crate) target_type: String,
    pub(crate) label: String,
    pub(crate) host_alias: Option<String>,
    pub(crate) path: String,
    pub(crate) installed: bool,
    pub(crate) can_install: bool,
    pub(crate) can_uninstall: bool,
    pub(crate) status: String,
    pub(crate) message: String,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "SkillTargetsResultDto")]
pub(crate) struct SkillTargetsResult {
    pub(crate) skill_id: String,
    pub(crate) skill_name: String,
    pub(crate) targets: Vec<SkillTarget>,
    pub(crate) tasks: Vec<TaskRun>,
    pub(crate) message: String,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "SkillTargetOperationItemDto")]
pub(crate) struct SkillTargetOperationItem {
    pub(crate) target_type: String,
    pub(crate) label: String,
    pub(crate) host_alias: Option<String>,
    pub(crate) ok: bool,
    pub(crate) message: String,
    pub(crate) task: Option<TaskRun>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "SkillTargetOperationResultDto")]
pub(crate) struct SkillTargetOperationResult {
    pub(crate) ok: bool,
    pub(crate) skills: Vec<SkillPack>,
    pub(crate) tasks: Vec<TaskRun>,
    pub(crate) results: Vec<SkillTargetOperationItem>,
    pub(crate) message: String,
}

#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "InstalledSkillRequestDto")]
pub(crate) struct InstalledSkillRequest {
    pub(crate) target_type: String,
    #[ts(optional)]
    pub(crate) host_alias: Option<String>,
    pub(crate) skill_name: String,
    pub(crate) path: String,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "InstalledSkillDownloadResultDto")]
pub(crate) struct InstalledSkillDownloadResult {
    pub(crate) imported: Vec<SkillPack>,
    pub(crate) skipped: Vec<String>,
    pub(crate) skills: Vec<SkillPack>,
    pub(crate) status: SkillInventoryStatus,
    pub(crate) tasks: Vec<TaskRun>,
    pub(crate) message: String,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "ConnectionTestDto")]
pub(crate) struct ConnectionTest {
    pub(crate) ok: bool,
    #[ts(type = "number | null")]
    pub(crate) latency_ms: Option<u64>,
    pub(crate) message: String,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "SshCheckResultDto")]
pub(crate) struct SshCheckResult {
    pub(crate) host_alias: String,
    pub(crate) ok: bool,
    #[ts(type = "number | null")]
    pub(crate) latency_ms: Option<u64>,
    pub(crate) message: String,
    pub(crate) task: TaskRun,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "SshBootstrapResultDto")]
pub(crate) struct SshBootstrapResult {
    pub(crate) host_alias: String,
    pub(crate) ok: bool,
    #[ts(type = "number | null")]
    pub(crate) latency_ms: Option<u64>,
    pub(crate) message: String,
    pub(crate) generated_key: bool,
    pub(crate) private_key_path: String,
    pub(crate) public_key_path: String,
    pub(crate) write_result: ssh::SshConfigWriteResult,
    pub(crate) task: TaskRun,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "SshConfigDeleteResultDto")]
pub(crate) struct SshConfigDeleteResult {
    #[serde(flatten)]
    pub(crate) write_result: ssh::SshConfigWriteResult,
    pub(crate) task: TaskRun,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "DeleteOperationResultDto")]
pub(crate) struct DeleteOperationResult {
    pub(crate) ok: bool,
    pub(crate) deleted: bool,
    pub(crate) message: String,
    pub(crate) task: TaskRun,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "SshBootstrapProgressEventDto")]
pub(crate) struct SshBootstrapProgressEvent {
    pub(crate) request_id: String,
    pub(crate) host_alias: String,
    pub(crate) step: String,
    pub(crate) status: String,
    pub(crate) message: String,
    pub(crate) detail: Option<String>,
    pub(crate) stdout: Option<String>,
    pub(crate) stderr: Option<String>,
    pub(crate) exit_code: Option<i32>,
    #[ts(type = "number | null")]
    pub(crate) duration_ms: Option<u64>,
    pub(crate) timed_out: Option<bool>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "RemoteProbeResultDto")]
pub(crate) struct RemoteProbeResult {
    pub(crate) host_alias: String,
    pub(crate) ssh_status: HostStatus,
    #[ts(type = "number | null")]
    pub(crate) latency_ms: Option<u64>,
    pub(crate) os: String,
    pub(crate) arch: String,
    pub(crate) shell: String,
    pub(crate) path: Option<String>,
    pub(crate) path_has_local_bin: bool,
    pub(crate) codex_command_available: bool,
    pub(crate) codex_installed: bool,
    pub(crate) codex_path: Option<String>,
    pub(crate) codex_version: String,
    pub(crate) config_exists: bool,
    pub(crate) api_config_name: String,
    pub(crate) api_config_source: String,
    pub(crate) api_key_env_var: Option<String>,
    pub(crate) api_key_env_present: Option<bool>,
    pub(crate) skills_exists: bool,
    pub(crate) skills_count: u16,
    pub(crate) task: TaskRun,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "LatestCodexVersionDto")]
pub(crate) struct LatestCodexVersion {
    pub(crate) version: Option<String>,
    pub(crate) checked_at: Option<String>,
    pub(crate) source: String,
    pub(crate) error: Option<String>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "LocalCodexStatusDto")]
pub(crate) struct LocalCodexStatus {
    pub(crate) platform: platform::RuntimePlatform,
    pub(crate) detected: bool,
    pub(crate) path: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) search_paths: Vec<String>,
    pub(crate) install_hint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(rename = "RemoteCodexActionDto")]
pub(crate) enum RemoteCodexAction {
    CheckVersion,
    Install,
    Update,
    Uninstall,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(rename = "HostOperationKindDto")]
pub(crate) enum HostOperationKind {
    HostTest,
    CodexInstall,
    CodexUpdate,
    CodexUninstall,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "HostOperationProgressEventDto")]
pub(crate) struct HostOperationProgressEvent {
    pub(crate) request_id: String,
    pub(crate) task_id: String,
    pub(crate) host_alias: String,
    pub(crate) operation: HostOperationKind,
    pub(crate) step: crate::tasks::TaskStep,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) log: Option<crate::tasks::TaskLog>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "RemoteCodexMaintenanceResultDto")]
pub(crate) struct RemoteCodexMaintenanceResult {
    pub(crate) host_alias: String,
    pub(crate) ok: bool,
    pub(crate) action: RemoteCodexAction,
    pub(crate) before_version: Option<String>,
    pub(crate) after_version: Option<String>,
    pub(crate) codex_path: Option<String>,
    pub(crate) codex_command_available: bool,
    pub(crate) install_method: Option<String>,
    pub(crate) path_changed: bool,
    pub(crate) shell_config_path: Option<String>,
    pub(crate) backup_path: Option<String>,
    pub(crate) message: String,
    pub(crate) task: TaskRun,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "RemoteProbeBatchItemDto")]
pub(crate) struct RemoteProbeBatchItem {
    pub(crate) host_alias: String,
    pub(crate) ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) result: Option<RemoteProbeResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) error: Option<String>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "RemoteProbeBatchItemCompletedEventDto")]
pub(crate) struct RemoteProbeBatchItemCompletedEvent {
    pub(crate) request_id: String,
    pub(crate) item: RemoteProbeBatchItem,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "RemoteProbeBatchResultDto")]
pub(crate) struct RemoteProbeBatchResult {
    pub(crate) request_id: String,
    pub(crate) latest_codex_version: LatestCodexVersion,
    pub(crate) results: Vec<RemoteProbeBatchItem>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "RemoteCodexBatchItemDto")]
pub(crate) struct RemoteCodexBatchItem {
    pub(crate) host_alias: String,
    pub(crate) ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) result: Option<RemoteCodexMaintenanceResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) error: Option<String>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "RemoteCodexBatchResultDto")]
pub(crate) struct RemoteCodexBatchResult {
    pub(crate) request_id: String,
    pub(crate) action: RemoteCodexAction,
    pub(crate) results: Vec<RemoteCodexBatchItem>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "RemoteCodexProgressEventDto")]
pub(crate) struct RemoteCodexProgressEvent {
    pub(crate) request_id: String,
    pub(crate) host_alias: String,
    pub(crate) action: RemoteCodexAction,
    pub(crate) step: String,
    pub(crate) status: String,
    pub(crate) message: String,
    pub(crate) detail: Option<String>,
    pub(crate) stdout: Option<String>,
    pub(crate) stderr: Option<String>,
    pub(crate) exit_code: Option<i32>,
    #[ts(type = "number | null")]
    pub(crate) duration_ms: Option<u64>,
    pub(crate) timed_out: Option<bool>,
}

pub(crate) struct CodexProgressContext<'a> {
    pub(crate) app: &'a AppHandle,
    pub(crate) request_id: Option<&'a str>,
    pub(crate) host_alias: &'a str,
    pub(crate) action: &'a RemoteCodexAction,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "NetworkProxyCandidateDto")]
pub(crate) struct NetworkProxyCandidate {
    pub(crate) source: String,
    pub(crate) url: Option<String>,
    pub(crate) available: bool,
    pub(crate) message: String,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "NetworkProxyStatusDto")]
pub(crate) struct NetworkProxyStatus {
    pub(crate) mode: NetworkProxyMode,
    pub(crate) proxy_url: Option<String>,
    pub(crate) source: Option<String>,
    pub(crate) message: String,
    pub(crate) candidates: Vec<NetworkProxyCandidate>,
}

pub(crate) struct AppServices {
    pub(crate) paths: AppPaths,
    pub(crate) hosts: Mutex<Vec<Host>>,
    pub(crate) profiles: Mutex<Vec<Profile>>,
    pub(crate) host_profile_write_lock: Mutex<()>,
    pub(crate) skill_packs: Mutex<Vec<SkillPack>>,
    pub(crate) task_store: TaskStore,
    pub(crate) task_storage_error: Mutex<Option<String>>,
    pub(crate) task_event_sink: Option<adapters::TaskEventSink>,
}

pub(crate) struct AppState {
    pub(crate) services: Arc<AppServices>,
}

impl AppState {
    #[cfg(test)]
    pub(crate) fn new(task_store: TaskStore) -> Self {
        Self::new_with_runtime(AppPaths::for_tests(), task_store, None, None)
    }

    pub(crate) fn new_with_runtime(
        paths: AppPaths,
        task_store: TaskStore,
        task_storage_error: Option<String>,
        task_event_sink: Option<adapters::TaskEventSink>,
    ) -> Self {
        Self {
            services: Arc::new(AppServices {
                paths,
                hosts: Mutex::new(empty_hosts()),
                profiles: Mutex::new(empty_profiles()),
                host_profile_write_lock: Mutex::new(()),
                skill_packs: Mutex::new(empty_skill_packs()),
                task_store,
                task_storage_error: Mutex::new(task_storage_error),
                task_event_sink,
            }),
        }
    }
}

impl Deref for AppState {
    type Target = AppServices;

    fn deref(&self) -> &Self::Target {
        &self.services
    }
}

pub(crate) const CODEX_RESOLVER_SCRIPT: &str = r#"best_path=""
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

pub(crate) fn codex_path_probe_script() -> String {
    format!("{CODEX_RESOLVER_SCRIPT}\nprintf '%s\\n' \"$best_path\"")
}

pub(crate) fn codex_version_probe_script() -> String {
    format!("{CODEX_RESOLVER_SCRIPT}\nprintf '%s\\n' \"$best_version\"")
}

pub(crate) const CODEX_COMMAND_AVAILABLE_SCRIPT: &str = r#"if command -v codex >/dev/null 2>&1; then
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

pub(crate) fn remote_skill_count_script() -> &'static str {
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

pub(crate) const CODEX_PATH_REPAIR_SCRIPT: &str = r##"set -u
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

pub(crate) const CODEX_OFFICIAL_INSTALL_SCRIPT: &str = r##"set -eu
export CODEX_INSTALL_DIR="$HOME/.local/bin"
export CODEX_HOME="$HOME/.codex"
export CODEX_NON_INTERACTIVE=1
export PATH="$HOME/.local/bin:$PATH"
mkdir -p "$CODEX_INSTALL_DIR" "$CODEX_HOME"
tmp_dir="${TMPDIR:-/tmp}/codexhub-official-install.$$"
mkdir -p "$tmp_dir"
trap 'rm -rf "$tmp_dir"' EXIT HUP INT TERM

if command -v curl >/dev/null 2>&1; then
  curl -fsSL --connect-timeout 15 --max-time 45 "https://chatgpt.com/codex/install.sh" -o "$tmp_dir/install.sh"
elif command -v wget >/dev/null 2>&1; then
  wget --timeout=45 --tries=1 -qO "$tmp_dir/install.sh" "https://chatgpt.com/codex/install.sh"
else
  printf 'curl or wget is not available for the official Codex installer.\n' >&2
  exit 127
fi
if command -v timeout >/dev/null 2>&1; then
  timeout 75 sh "$tmp_dir/install.sh"
else
  sh "$tmp_dir/install.sh"
fi
printf 'CODEXHUB_INSTALL_METHOD=official\n'
"##;

pub(crate) const CODEX_REMOTE_NATIVE_MIRROR_SCRIPT: &str = r##"set -u
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

  printf '[CodexHub] Starting %s from %s\n' "$phase_label" "$url"

  if [ "$allow_insecure" = "yes" ] && ! allow_insecure_for_url "$url"; then
    printf 'Insecure TLS fallback is limited to npmmirror URLs; refusing disabled verification for %s\n' "$url" >&2
    allow_insecure=no
  fi

  if command -v curl >/dev/null 2>&1; then
    rm -f "$err_file"
    if curl -fsSL --connect-timeout "$connect_timeout" --max-time "$max_time" "$url" -o "$output" 2>"$err_file"; then
      printf '[CodexHub] Finished %s.\n' "$phase_label"
      return 0
    fi
    last_status=$?
    cat "$err_file" >&2
    if [ "$allow_insecure" = "yes" ] && is_tls_cert_error "$err_file"; then
      printf 'TLS certificate verification failed for %s; retrying npmmirror download with certificate checks disabled.\n' "$url" >&2
      rm -f "$err_file"
      if curl -k -fsSL --connect-timeout "$connect_timeout" --max-time "$max_time" "$url" -o "$output" 2>"$err_file"; then
        insecure_tls_fallback=yes
        printf '[CodexHub] Finished %s with insecure TLS fallback.\n' "$phase_label"
        return 0
      fi
      last_status=$?
      cat "$err_file" >&2
    fi
  fi
  if command -v wget >/dev/null 2>&1; then
    rm -f "$err_file"
    if wget --timeout="$max_time" --tries=1 -qO "$output" "$url" 2>"$err_file"; then
      printf '[CodexHub] Finished %s.\n' "$phase_label"
      return 0
    fi
    last_status=$?
    cat "$err_file" >&2
    if [ "$allow_insecure" = "yes" ] && is_tls_cert_error "$err_file"; then
      printf 'TLS certificate verification failed for %s; retrying npmmirror download with certificate checks disabled.\n' "$url" >&2
      rm -f "$err_file"
      if wget --timeout="$max_time" --tries=1 --no-check-certificate -qO "$output" "$url" 2>"$err_file"; then
        insecure_tls_fallback=yes
        printf '[CodexHub] Finished %s with insecure TLS fallback.\n' "$phase_label"
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
printf 'CODEXHUB_INSTALL_METHOD=failed\n'
exit "$native_status"
"##;

pub(crate) const CODEX_REMOTE_NPM_MIRROR_SCRIPT: &str = r##"set -eu
export CODEX_INSTALL_DIR="$HOME/.local/bin"
export CODEX_HOME="$HOME/.codex"
export CODEX_NON_INTERACTIVE=1
export PATH="$HOME/.local/bin:$PATH"
mkdir -p "$CODEX_INSTALL_DIR" "$CODEX_HOME"
if ! command -v npm >/dev/null 2>&1; then
  printf 'npm is not available for the npmmirror installation method.\n' >&2
  exit 127
fi
npm install -g @openai/codex --prefix "$HOME/.local" --registry=https://registry.npmmirror.com
printf 'CODEXHUB_INSTALL_METHOD=npm-mirror\n'
"##;

pub(crate) const CODEX_UNINSTALL_SCRIPT: &str = r##"set -u
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

pub(crate) const CODEX_NATIVE_PLATFORM_SCRIPT: &str = r#"set -u
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

pub(crate) struct LocalCodexNativePackage {
    pub(crate) version: String,
    pub(crate) target: String,
    pub(crate) tarball_path: PathBuf,
    pub(crate) temp_dir: PathBuf,
}

#[cfg(test)]
impl Default for AppState {
    fn default() -> Self {
        Self::new(TaskStore::in_memory())
    }
}
