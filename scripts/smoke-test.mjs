import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
const requiredFiles = [
  "README.md",
  "docs/research.md",
  "docs/architecture.md",
  "docs/mvp-scope.md",
  "docs/known-limitations.md",
  "package.json",
  "vite.config.mjs",
  "index.html",
  "src/main.tsx",
  "src/App.tsx",
  "src/api.ts",
  "src/models.ts",
  "src/settings.ts",
  "src/styles.css",
  "figs/app-logo.png",
  "src-tauri/Cargo.toml",
  "src-tauri/tauri.conf.json",
  "src-tauri/icons/32x32.png",
  "src-tauri/icons/64x64.png",
  "src-tauri/icons/128x128.png",
  "src-tauri/icons/128x128@2x.png",
  "src-tauri/icons/icon.png",
  "src-tauri/icons/icon.ico",
  "src-tauri/src/main.rs",
  "src-tauri/src/lib.rs",
  "src-tauri/capabilities/default.json"
];

const read = (file) => fs.readFileSync(path.join(root, file), "utf8");
const removed = (...parts) => parts.join("");
const fail = (message) => {
  console.error(`SMOKE FAIL: ${message}`);
  process.exit(1);
};

for (const file of requiredFiles) {
  if (!fs.existsSync(path.join(root, file))) fail(`missing ${file}`);
}

const packageJson = JSON.parse(read("package.json"));
for (const script of ["tauri", "dev", "dev:web", "dev:mock", "smoke"]) {
  if (!packageJson.scripts?.[script]) fail(`missing package script ${script}`);
}
if (!packageJson.dependencies?.["@tauri-apps/plugin-dialog"]) fail("missing Tauri dialog plugin dependency");

const tauriConfig = JSON.parse(read("src-tauri/tauri.conf.json"));
const defaultCapability = JSON.parse(read("src-tauri/capabilities/default.json"));
if (!JSON.stringify(defaultCapability).includes("dialog:default")) fail("missing dialog capability permission");
const requiredBundleIcons = [
  "icons/32x32.png",
  "icons/64x64.png",
  "icons/128x128.png",
  "icons/128x128@2x.png",
  "icons/icon.png",
  "icons/icon.ico"
];
for (const icon of requiredBundleIcons) {
  if (!tauriConfig.bundle?.icon?.includes(icon)) fail(`Tauri bundle should use ${icon}`);
}

const readBinary = (file) => fs.readFileSync(path.join(root, file));
const pngSize = (file) => {
  const bytes = readBinary(file);
  if (
    bytes.length < 24 ||
    bytes[0] !== 0x89 ||
    bytes.toString("ascii", 1, 4) !== "PNG" ||
    bytes.toString("ascii", 12, 16) !== "IHDR"
  ) {
    fail(`${file} should be a PNG image`);
  }
  return [bytes.readUInt32BE(16), bytes.readUInt32BE(20)];
};
for (const [file, expected] of [
  ["src-tauri/icons/32x32.png", 32],
  ["src-tauri/icons/64x64.png", 64],
  ["src-tauri/icons/128x128.png", 128],
  ["src-tauri/icons/128x128@2x.png", 256],
  ["src-tauri/icons/icon.png", 512]
]) {
  const [width, height] = pngSize(file);
  if (width !== expected || height !== expected) fail(`${file} should be ${expected}x${expected}, got ${width}x${height}`);
}

const icoFrames = (file) => {
  const bytes = readBinary(file);
  if (bytes.length < 6) fail(`${file} is too small to be a valid ICO`);
  const reserved = bytes.readUInt16LE(0);
  const type = bytes.readUInt16LE(2);
  const count = bytes.readUInt16LE(4);
  if (reserved !== 0 || type !== 1 || count < 1) fail(`${file} should be a valid icon ICO`);
  const frames = [];
  for (let index = 0; index < count; index += 1) {
    const offset = 6 + index * 16;
    if (offset + 16 > bytes.length) fail(`${file} has a truncated ICO directory`);
    const width = bytes[offset] || 256;
    const height = bytes[offset + 1] || 256;
    const bitDepth = bytes.readUInt16LE(offset + 6);
    const byteLength = bytes.readUInt32LE(offset + 8);
    const imageOffset = bytes.readUInt32LE(offset + 12);
    if (imageOffset + byteLength > bytes.length) fail(`${file} has a truncated ${width}x${height} frame`);
    frames.push({ width, height, bitDepth });
  }
  return frames;
};
const iconFrames = icoFrames("src-tauri/icons/icon.ico");
for (const size of [16, 24, 32, 48, 64, 128, 256]) {
  if (!iconFrames.some((frame) => frame.width === size && frame.height === size && frame.bitDepth === 32)) {
    fail(`Tauri icon.ico should include a ${size}x${size} 32-bit frame`);
  }
}

const research = read("docs/research.md");
const architecture = read("docs/architecture.md");
const mvp = read("docs/mvp-scope.md");
const limitations = read("docs/known-limitations.md");

const requiredText = [
  [research, "No public, stable API was found"],
  [research, "~/.codex/config.toml"],
  [research, "~/.codex/skills/"],
  [architecture, "MVP does not require a remote Codex wrapper"],
  [architecture, "directly manages remote Codex files"],
  [architecture, "env_key"],
  [architecture, "apiKeyEnvVar"],
  [architecture, "credentialStored"],
  [architecture, "download_github_skill"],
  [architecture, "update_library_skill_about"],
  [architecture, "tree/<branch>/<skill-path>"],
  [architecture, "install_skill_targets"],
  [architecture, "Skill Management"],
  [mvp, "Mandatory remote Codex wrapper"],
  [mvp, "Window 5: profile/API config"],
  [mvp, "Window 6: single-card local skill library"],
  [mvp, "Writing local credential-store key names or API key values into remote Codex config"],
  [limitations, "Profiles /"],
  [limitations, "direct GitHub repository URLs and GitHub"],
  [limitations, "The optional stored local credential key is never written to remote"],
  [limitations, "CodexHub must not write Codex App private state"]
];

for (const [content, phrase] of requiredText) {
  if (!content.includes(phrase)) fail(`missing required phrase: ${phrase}`);
}

const rustLib = read("src-tauri/src/lib.rs");
const sshRs = read("src-tauri/src/ssh.rs");
for (const token of ["CODEX_NATIVE_PLATFORM_SCRIPT", "npm-mirror-native-local-upload", "parse_npmmirror_native_metadata", "remote-codex-progress", "RemoteCodexProgressEvent", "run_ssh_script_streaming"]) {
  if (!rustLib.includes(token)) fail(`missing local upload Codex fallback token: ${token}`);
}
for (const command of [
  "app_health",
  "get_settings",
  "save_settings",
  "get_ssh_status",
  "generate_ed25519_key",
  "list_ssh_config_hosts",
  "upsert_ssh_config_host",
  "delete_ssh_config_host",
  "list_hosts",
  "refresh_discovered_hosts",
  "add_host",
  "update_host",
  "delete_host",
  "test_ssh_connection",
  "ssh_check",
  "bootstrap_ssh_host",
  "bootstrap_existing_ssh_host",
  "remote_probe_codex",
  "remote_manage_codex",
  "refresh_latest_codex_version",
  "list_profiles",
  "create_profile",
  "update_profile",
  "delete_profile",
  "duplicate_profile",
  "import_profiles",
  "export_profiles",
  "set_profile_api_key",
  "delete_profile_api_key",
  "preview_profile_apply",
  "apply_profile",
  "detect_cc_switch_profiles",
  "import_cc_switch_profiles",
  "list_local_skills",
  "import_local_skill",
  "update_library_skill_about",
  "get_skill_inventory_status",
  "detect_installed_skills",
  "download_github_skill",
  "get_skill_targets",
  "install_skill_targets",
  "uninstall_skill_targets",
  "delete_library_skill",
  "list_tasks"
]) {
  if (!rustLib.includes(command)) fail(`missing ${command} Tauri command`);
}
for (const removedCommand of [
  removed("search", "_online", "_skills"),
  removed("clone", "_skill", "_repo"),
  removed("list", "_remote", "_skills"),
  removed("preview", "_remote", "_skill", "_install"),
  removed("install", "_remote", "_skill", "_batch"),
  removed("delete", "_remote", "_skill")
]) {
  if (rustLib.includes(removedCommand)) fail(`removed public Skills command should not remain: ${removedCommand}`);
}
const listHostsMatch = rustLib.match(/fn list_hosts[\s\S]*?\n}\r?\n\r?\n#\[tauri::command\]\r?\nfn refresh_discovered_hosts/);
if (!listHostsMatch) fail("could not locate list_hosts function boundary");
if (listHostsMatch[0].includes("merge_discovered_hosts")) fail("list_hosts must not auto-import local SSH config");
for (const asyncCommand of [
  "async fn get_ssh_status",
  "async fn list_ssh_config_hosts",
  "async fn ssh_check",
  "async fn remote_probe_codex",
  "async fn remote_manage_codex",
  "async fn refresh_latest_codex_version",
  "async fn detect_installed_skills",
  "async fn download_github_skill",
  "async fn get_skill_targets",
  "async fn install_skill_targets",
  "async fn uninstall_skill_targets",
  "async fn delete_library_skill"
]) {
  if (!rustLib.includes(asyncCommand)) fail(`long remote command must stay async: ${asyncCommand}`);
}
if (!rustLib.includes("spawn_blocking(command)")) fail("long remote commands should run through the blocking worker pool");
for (const token of [
  "tauri_plugin_dialog::init()",
  "SkillImportResult",
  "SkillInventoryStatus",
  "SkillDetectionResult",
  "SkillTargetsResult",
  "SkillTargetOperationResult",
  "RemoteSkillListResult",
  "managed_skills_dir",
  "skill_inventory_path",
  "local_skills",
  "update_local_inventory_skill",
  "apply_skill_inventory_to_hosts",
  "Loaded cached skill targets.",
  "installed_skill_candidate_dirs",
  "skill_candidate_dirs",
  "parse_skill_metadata",
  "is_allowed_github_repo_url",
  "parse_github_skill_url",
  "write_skill_archive",
  "remote_skill_count_script",
  "remote_skill_list_script",
  "remote_skill_install_script",
  "remote_skill_delete_script",
  "validate_remote_skill_dir_name",
  "$HOME/.codex/superpowers/skills",
  '"$root"/.[!.]*',
  '"$root"/..?*',
  '"$dir"/.[!.]*',
  '"$dir"/..?*',
  "CODEXHUB_SKILL_BACKUP",
  "CODEXHUB_SKILL_SKIPPED",
  "tar is required on the remote host"
]) {
  if (!rustLib.includes(token)) fail(`missing Window 6 Skills backend token: ${token}`);
}
if (rustLib.includes('find "$HOME/.codex/skills" -mindepth 1 -maxdepth 1')) {
  fail("remote skill probes must not count only first-level ~/.codex/skills directories");
}
const getSkillTargetsMatch = rustLib.match(/fn run_get_skill_targets[\s\S]*?\n}\r?\n\r?\nfn run_install_skill_targets/);
if (!getSkillTargetsMatch) fail("could not locate run_get_skill_targets function boundary");
for (const liveProbeToken of ["run_remote_skill_install_preview", "run_remote_skill_list"]) {
  if (getSkillTargetsMatch[0].includes(liveProbeToken)) {
    fail(`get_skill_targets must use cached inventory instead of live remote probing: ${liveProbeToken}`);
  }
}
for (const token of ["LatestCodexVersion", "parse_npm_latest_metadata", "latest_codex_cache_is_fresh", "CODEX_LATEST_REFRESH_HOUR", "https://registry.npmjs.org/@openai/codex", "codex-latest.json"]) {
  if (!rustLib.includes(token)) fail(`missing latest Codex version backend token: ${token}`);
}
for (const token of ["hosts.json", "load_hosts", "save_hosts", "save_current_hosts"]) {
  if (!rustLib.includes(token)) fail(`missing host persistence token: ${token}`);
}
for (const token of ["setup_guide_dismissed", "#[serde(default)]"]) {
  if (!rustLib.includes(token)) fail(`missing setup guide settings backend token: ${token}`);
}
for (const token of ["CREATE_NO_WINDOW", "process_command", "creation_flags(CREATE_NO_WINDOW)"]) {
  if (!sshRs.includes(token)) fail(`missing hidden Windows child-process token: ${token}`);
}
for (const token of ["parse_cc_switch_sqlite_profiles", "provider_endpoints", "cc-switch.db", "currentProviderCodex", "credential_stored: false"]) {
  if (!rustLib.includes(token)) fail(`missing cc-switch adapter token: ${token}`);
}
for (const token of ["profiles: Vec<Profile>", "hosts: Vec<Host>", "sync_profile_host_links", "sync_profile_host_ids", "clear_profile_host_links", "reconcile_hosts_with_profile_links", "RemoteApiConfigMatch"]) {
  if (!rustLib.includes(token)) fail(`missing profile apply refreshed-state token: ${token}`);
}
if (rustLib.includes("host.config_exists = Some(true);\n            host.api_config_name = Some(profile.name.clone());")) {
  fail("profile host-link reconcile must not promote local links into confirmed remote API config facts");
}
for (const token of ["api_config_name", "api_config_source", "classify_remote_api_config", "normalize_base_url_key", "read ~/.codex/config.toml base URL"]) {
  if (!rustLib.includes(token)) fail(`missing remote API config probe token: ${token}`);
}
for (const token of [
  "Research Default",
  "Safe Editing",
  "Research Default Copy",
  "Mac Studio Lab",
  "Windows Workstation",
  "Linux Runner",
  'name: "Diagnostics"',
  'name: "Diagnostics".into()'
]) {
  if (rustLib.includes(token)) fail(`default mock data should not include ${token}`);
}

const app = read("src/App.tsx");
for (const label of ["Home", "主页", "Hosts", "Profiles", "Skills", "Tasks", "✅ Tasks", "✅ 任务", "Settings", "Host Matrix", "主机矩阵", "Font", "Host list", "主机列表", "Local config", "本地配置", "Local keys", "本地密钥", "Host IP", "Codex版本", "Test all", "一键测试", "Update outdated", "一键更新", "Details", "详情", "Logs", "日志", "Copied!", "复制成功！", "Add Server", "添加服务器", "来源", "System", "系统", "Codex", "API config", "API 配置", "Test latency", "测试延迟", "stdout", "stderr", "Install Codex", "Update Codex", "新增 SSH Host", "连接进程", "BootstrapProgressLog"]) {
  if (!app.includes(label)) fail(`missing UI label: ${label}`);
}
for (const token of ['icon: "🏠"', 'icon: "🖥️"', 'icon: "🧾"', 'icon: "🧩"', 'icon: "✅"', 'icon: "⚙️"', 'className="navIcon"', "metricPrimary", "metricSecondary", "appliedProfileCount", "new Set(hosts.map((host) => host.profileId)", "successfulTaskCount", "matrixHeader", "matrixEmptyIcon", "onAddServer", "onTestAllSshHosts"]) {
  if (!app.includes(token)) fail(`missing dashboard home polish token: ${token}`);
}
for (const token of [
  "SetupGuideModal",
  "setupGuideOpen",
  "setupGuideBusy",
  "setupGuideStep",
  "handleSetupGuideLanguageNext",
  "handleImportLocalSshConfig",
  "setupGuideDismissed",
  "🧭 Setup Guide",
  "🧭 配置向导",
  "Choose Language",
  "Step 1: Please choose your preferred language.",
  "第1步：请选择偏好语言",
  "Next",
  "Nothing here yet...",
  '<div className="matrixEmptyIcon" aria-hidden="true">🖥️</div>',
  "Detecting local config...",
  "正在检测本地配置...",
  "Import local config",
  "导入本地配置",
  "未检测到本地存在可用的SSH配置，可使用CodexHub手动添加",
  "Detect local config",
  "检测本地配置",
  "EmptyListState",
  'hosts: "🖥️"',
  'profiles: "🧾"',
  'skills: "🧩"',
  'tasks: "✅"',
  "emptyLists",
  "emptyListState",
  "copy.emptyLists.hosts",
  "onOpenSetupGuide"
]) {
  if (!app.includes(token)) fail(`missing setup guide or empty-state token: ${token}`);
}
for (const token of ['new URL("../figs/app-logo.png", import.meta.url).href', '<img className="appIcon" src={appLogoUrl} alt="" aria-hidden="true" />']) {
  if (!app.includes(token)) fail(`missing app logo UI token: ${token}`);
}
for (const token of ["copy.hosts.source", "copy.dashboard.system", "copy.hosts.codex", "copy.hosts.configExists", "copy.hosts.latency", "copy.hosts.skills"]) {
  if (!app.includes(token)) fail(`missing Host Matrix field token: ${token}`);
}
for (const token of [
  '@tauri-apps/plugin-dialog',
  "open({ directory: true",
  "function SkillsView(",
  "api.importLocalSkill",
  "api.getSkillInventoryStatus",
  "api.detectInstalledSkills",
  "api.downloadGithubSkill",
  "api.getSkillTargets",
  "api.installSkillTargets",
  "api.uninstallSkillTargets",
  "api.deleteLibrarySkill",
  "className=\"skillsStack\"",
  "skillLibraryActions",
  "skillApplicationTags",
  "skillRowActions",
  "installedLibrary",
  "inventoryStatus",
  "buildInstalledSkillRows",
  'sourceTone: "green"',
  "unknownSkillCount",
  "installedSkillTagStyle",
  "installedSkillsTable",
  "installedSkillTag",
  "SkillFirstScanModal",
  "SkillDownloadModal",
  "SkillPreviewModal",
  "const about = skillDescription || skill.about?.trim() || copy.skills.aboutFallback",
  "className=\"taskLogModalMeta skillPreviewMeta\"",
  "SkillTargetsModal",
  "onRefreshSkillLibrary",
  "copy.skills.refresh",
  "copy.skills.refreshed",
  'selectAll: "Select all"',
  'selectAll: "全选"',
  'setSelectedTargetKeys([])',
  '"install-success"',
  '"install-partial-failure"',
  '"uninstall-success"',
  '"uninstall-partial-failure"',
  "copy.skills.installSuccess",
  "copy.skills.installPartialFailure",
  "copy.skills.uninstallSuccess",
  "copy.skills.uninstallPartialFailure",
  'skill.sourceType === "github" ? "blue" : "gray"',
  'mode === "install" ? target.message : target.path || target.message',
  "SkillDeleteModal",
  "copy.skills.firstScanTitle",
  "void handleDetect(true).catch",
  "handleRefresh",
  'importDirectory: "Import"',
  'importDirectory: "导入"',
  "copy.skills.githubUrl",
  "List remote skills",
  "Preview skill install",
  "Install skill",
  "Delete skill"
]) {
  if (!app.includes(token)) fail(`missing Window 6 Skills UI token: ${token}`);
}
if (app.includes("void handleDetect(false).catch")) fail("manual skill detection must refresh host inventories, not local-only cache");
for (const removedToken of [
  removed("api.search", "Online", "Skills"),
  removed("api.clone", "SkillRepo"),
  removed("api.list", "RemoteSkills"),
  removed("api.preview", "RemoteSkillInstall"),
  removed("api.install", "RemoteSkillBatch"),
  removed("api.delete", "RemoteSkill"),
  removed("skill", "SearchBar"),
  removed("skill", "RemotePanel"),
  removed("copy.skills.remote", "Install"),
  "copy.skills[policy]"
]) {
  if (app.includes(removedToken)) fail(`removed Skills UI token should not remain: ${removedToken}`);
}
for (const removedInstallToken of [
  "Installed skill on {} of {} selected target(s).",
  "Installed skill on ",
  "Uninstalled skill from {} of {} selected target(s).",
  "Uninstalled skill from "
]) {
  if (rustLib.includes(removedInstallToken)) fail(`old skill operation success message should not remain: ${removedInstallToken}`);
}
for (const token of [
  'label: "Dashboard"',
  'label: "仪表盘"',
  'description: "Overview"',
  'description: "SSH targets"',
  'description: "总览"',
  'description: "SSH 目标"',
  "item.description",
  "backendContract",
  "Backend contract",
  "后端约定",
  "batchOperations",
  "Batch operations",
  "批量操作",
  "recentTasks",
  "Recent tasks",
  "最近任务",
  "BatchOperationsPlaceholder",
  "RecentTasks",
  "desktopMvp",
  "Desktop MVP",
  "桌面 MVP",
  "brandSubtle"
]) {
  if (app.includes(token)) fail(`dashboard home polish should remove old token: ${token}`);
}
if (!app.includes("CodexOperationModal")) fail("Install/update should show a compact Codex operation progress modal");
if (!app.includes("codexOperationModal")) fail("Codex operation modal state should be wired in App");
if (!app.includes("RemoteCodexProgressEvent")) fail("Codex operation modal should consume real progress events");
if (app.includes('event.status === "success" ? "success"') || app.includes('event.status === "failed" ? "failed"')) fail("Codex operation modal status should only change from the final result or catch path");
if (!app.includes("logRowsRef") || !app.includes("logRows.scrollTop = logRows.scrollHeight") || !app.includes("ref={logRowsRef}")) fail("Codex operation log should auto-scroll to the latest row");
if (app.includes('<div className="eyebrow">{copy.codexOperation.title}</div>') || app.includes("{operation.hostName} · {operation.hostAlias}")) fail("Codex operation header should only keep the title and status badge");
if (app.includes("profileCard")) fail("Profiles page should use a compact table list instead of profile cards");
if (!app.includes("function ProfilesView(")) fail("Profiles page should be implemented");
for (const token of [
  "api.listProfiles",
  "api.createProfile",
  "api.updateProfile",
  "api.deleteProfile",
  "api.duplicateProfile",
  "api.importProfiles",
  "api.exportProfiles",
  "api.setProfileApiKey",
  "api.deleteProfileApiKey",
  "api.previewProfileApply",
  "api.applyProfile",
  "api.detectCcSwitchProfiles",
  "api.importCcSwitchProfiles",
  "apiKeyEnvVar",
  "credentialStored"
]) {
  if (!app.includes(token)) fail(`missing Profiles UI/API token: ${token}`);
}
for (const token of [
  'library: "Local config"',
  'library: "本地配置"',
  'hosts: "Hosts"',
  'hosts: "主机"',
  '"Apply configuration"',
  '"应用配置"',
  'applySelected: "Apply selected"',
  'applySelected: "应用所选"',
  'applyOne: "Apply"',
  'applyOne: "应用"',
  'selectAll: "Select all"',
  'selectAll: "一键全选"',
  'selectHosts: "Select hosts"',
  'selectHosts: "选择主机"',
  'apiConfig: "API config"',
  'apiConfig: "API配置"',
  'noApiConfig: "No config"',
  'noApiConfig: "无配置"',
  'unknownApiConfig: "Unknown config"',
  'unknownApiConfig: "未知配置"',
  'ccSwitchChecking: "Checking cc-switch..."',
  'ccSwitchChecking: "正在检测 cc-switch..."',
  'importDetected: "导入检测配置"',
  'apiKeyEnvVar: "API key"',
  "Credential stored",
  "Third-party import",
  "第三方导入",
  "Local storage",
  "本地存储",
  "Store key",
  "Rendered TOML",
  "backup"
]) {
  if (!app.includes(token)) fail(`missing compact Profiles UI label: ${token}`);
}
for (const token of ["ProfileEditModal", "ProfileHostSelectModal", "ProfileApplyPreviewModal", "ProfileModelCombobox", "ProfileStorageBadge", "profileLibraryActions", "ccSwitchActionButton", "profileCcSwitchStatus", "profileRowActions", "profileApplyTable", "profileHostSelectModal", "profileFastModeSegment"]) {
  if (!app.includes(token)) fail(`missing Profiles modal/action token: ${token}`);
}
for (const token of [
  'const CODEX_MODEL_OPTIONS = ["gpt-5.5", "gpt-5.4", "gpt-5.4-mini", "gpt-5.3-codex", "gpt-5.2", "gpt-5-codex"]',
  'const REASONING_EFFORT_OPTIONS = ["low", "medium", "high", "xhigh"]',
  "CODEX_MODEL_OPTIONS.filter",
  'role="combobox"',
  'role="radiogroup"',
  "appliedHostCountByProfileId",
  "appliedHostKeys",
  "profileMatchesConfirmedHostApiConfig",
  "profile.hostIds",
  "result.profiles.length > 0",
  "result.hosts.length > 0",
  "const [refreshedHosts, refreshedProfiles] = await Promise.all([api.listHosts(), api.listProfiles()])",
  "selectedAppliedHostIds",
  "hostSourceLabel(copy, host)",
  "copy.profiles.applyColumn",
  "copy.profiles.selectHosts",
  "copy.profiles.selectAll",
  "copy.profiles.apiConfig",
  "copy.profiles.noApiConfig",
  "copy.profiles.unknownApiConfig",
  "copy.profiles.applySuccess",
  'profile.source === "cc-switch"',
  "copy.profiles.thirdPartyImport",
  "copy.profiles.localStorageLabel",
  "canImportCcSwitchDetection ? \"primaryButton\" : \"secondaryButton\"",
  "canImportCcSwitchDetection ? handleImportDetected() : handleDetectCcSwitch()"
]) {
  if (!app.includes(token)) fail(`missing Profiles requested fix token: ${token}`);
}
for (const token of [
  "HostApiConfigBadge",
  "hostApiConfigChecked",
  "copy.profiles.unknownApiConfig",
  "isHostCodexTested(host) && (host.configExists !== null || Boolean(host.apiConfigSource || host.apiConfigName))",
  'source === "profile" && host.profileId',
  "result.apiConfigName",
  "result.apiConfigSource",
  'configExists: "API config"',
  'configExists: "API 配置"'
]) {
  if (!app.includes(token)) fail(`missing host API config UI token: ${token}`);
}
if (app.includes("return host.configExists !== null || Boolean(host.apiConfigSource || host.apiConfigName);")) {
  fail("Host API config badge must not trust stored apiConfigName/profileId before host testing");
}
for (const token of ["hostIdentityByKey", "profileHostKeys.has", "host.profileId === selectedProfile.id ||"]) {
  if (app.includes(token)) fail(`Profiles host counts must use confirmed API config matches, not local host links: ${token}`);
}
for (const token of [
  "HostDetailValueBadge",
  "hostStatusTone",
  "latencyTone",
  "booleanTone",
  "hostCodexInstalledStatus",
  "dashboardHostSkillCount",
  "dashboardSkillCountTone",
  "archTone",
  "hostCodexStatus(copy, host, undefined, hosts, latestCodexVersion)",
  "<HostApiConfigBadge copy={copy} host={host}"
]) {
  if (!app.includes(token)) fail(`missing unified host-detail badge token: ${token}`);
}
for (const token of [
  "latestCodexVersion",
  "copy.hosts.latestCodexVersion",
  "sshHostsLatestVersionCol",
  "latestCodexStatus",
  "formatCodexVersionLabel",
  "parseCodexVersion",
  "isCodexVersionBehind",
  "codexVersionTone",
  "latencyTone(host?.latencyMs, hosts)",
  "api.refreshLatestCodexVersion(true)",
  "api.refreshLatestCodexVersion(false)",
  "next.setHours(4, 0, 0, 0)"
]) {
  if (!app.includes(token)) fail(`missing latest/relative Codex UI token: ${token}`);
}
for (const token of ["value < 100", "value < 300", "return { label: host.codexVersion || copy.profiles.notChecked, tone: \"green\" }", "const updateDisabled = Boolean(busy) || !codexTested || !host?.codexInstalled;"]) {
  if (app.includes(token)) fail(`Host latency/version UI should not keep old absolute or always-update logic: ${token}`);
}
for (const token of [
  "<dd>{host ? copy.status.host[host.status] : copy.hosts.unknown}</dd>",
  "<dd>{host?.os ?? copy.hosts.unknown}</dd>",
  "<dd>{host?.arch ?? copy.hosts.unknown}</dd>",
  "<dd>{host?.shell ?? copy.hosts.unknown}</dd>",
  "<dd>{formatLatency(host?.latencyMs, copy)}</dd>",
  "<dd>{host ? formatBoolean(host.codexInstalled, copy) : copy.hosts.unknown}</dd>",
  "<dd>{host?.codexVersion ?? copy.hosts.unknown}</dd>",
  "<dd>{host?.skillsCount ?? copy.hosts.unknown}</dd>"
]) {
  if (app.includes(token)) fail(`Host details should render badge values instead of raw dd text: ${token}`);
}
for (const token of ['configExists: "Config exists"', 'configExists: "Config 存在"', "formatNullableBoolean(host.configExists"]) {
  if (app.includes(token)) fail(`Host details should show API config labels, not old config-exists boolean: ${token}`);
}
for (const token of ["API key env var", "API key 环境变量", 'className="checkboxRow"', "profile.hostIds.length}</td>"]) {
  if (app.includes(token)) fail(`Profiles UI should not keep old profile editor/count token: ${token}`);
}
if (app.includes("脳")) fail("Modal close buttons must not contain mojibake text");
const profileRowActions = [
  ["edit", "copy.profiles.edit"],
  ["duplicate", "copy.profiles.duplicate"],
  ["export", "copy.profiles.export"],
  ["delete", "copy.profiles.delete"]
];
for (const [action, token] of profileRowActions) {
  if (!app.includes(token)) fail(`missing profile row ${action} action token: ${token}`);
}
if (!app.includes("copy.profiles.newProfile") && !app.includes("copy.profiles.create")) {
  fail("Profiles library actions should include create/new profile");
}
for (const [action, token] of [
  ["import", "copy.profiles.import"],
  ["detect cc-switch", "copy.profiles.detectCcSwitch"],
  ["import detected", "copy.profiles.importDetected"]
]) {
  if (!app.includes(token)) fail(`missing profile library ${action} action token: ${token}`);
}
for (const [action, token] of [
  ["preview", "copy.profiles.previewApply"],
  ["apply selected", "copy.profiles.applySelected"],
  ["per-row apply", "copy.profiles.applyOne"],
  ["select all", "copy.profiles.selectAll"],
  ["select hosts", "copy.profiles.selectHosts"]
]) {
  if (!app.includes(token)) fail(`missing profile apply ${action} action token: ${token}`);
}
if (!app.includes('className="profilesStack"') || !app.includes("profileLibraryActions") || !app.includes("profileTable") || !app.includes("profileRowActions") || !app.includes("profileApplyPanel") || !app.includes("profileApplyTable") || !app.includes("profileHostSelectCell")) {
  fail("Profiles page should use compact stack, library actions, row actions, table, apply panel, and host-table tokens");
}
for (const token of [
  "<th>{copy.profiles.approval}</th>",
  "<th>{copy.profiles.sandbox}</th>",
  "<th>{copy.profiles.updated}</th>",
  "<td>{profile.approvalPolicy}</td>",
  "<td>{profile.sandboxMode}</td>",
  "<td>{profile.updatedAt}</td>",
  'updateDraft("approvalPolicy"',
  'updateDraft("sandboxMode"',
  "copy.profiles.approval",
  "copy.profiles.sandbox",
  "copy.profiles.updated"
]) {
  if (app.includes(token)) fail(`Profiles UI should stay minimal and not render: ${token}`);
}
if (!app.includes('onManageCodex(sshHost.alias, "install")') || !app.includes('onManageCodex(sshHost.alias, "update")')) fail("SSH Hosts table should expose remote Codex install/update actions");
if (!app.includes('installCodex: "安装"') || !app.includes('updateCodex: "更新"')) fail("SSH Hosts Codex buttons should use short install/update labels");
for (const token of ["onUpdateOutdatedCodexHosts", "handleUpdateOutdatedCodexHosts", "Promise.allSettled", "outdatedCodexAliases", "copy.hosts.updateOutdatedCodex"]) {
  if (!app.includes(token)) fail(`missing one-click outdated Codex update token: ${token}`);
}
if (!app.includes('className="sshHostsTable"') || !app.includes("sshHostsActionsCol") || !app.includes("sshHostsCodexCol")) fail("SSH Hosts table should use the compact responsive table layout");
if (app.includes('<td className="tableActions')) fail("SSH Hosts action cells must remain table cells; put flex on an inner button group");
if (!app.includes('className="tableActions sshHostsActionGroup"')) fail("SSH Hosts action buttons should be wrapped in an inner flex group");
if (app.includes("HostDetailsPanel copy={copy} host={selectedHost} hostBusy")) fail("Host details should not own remote Codex maintenance actions");
if (app.includes("onAddHost={")) fail("Dashboard or non-Hosts Add Server handler should not remain");
if (app.includes("<th>{copy.hosts.identityFile}</th>")) fail("SSH Hosts table should not show IdentityFile as a column");
if (app.includes("onProbeHost")) fail("Probe button path should be removed from the UI");
if (app.includes("existingHosts")) fail("Existing app hosts card should be removed");
if (app.includes("<strong>{health.mode}</strong>")) fail("Sidebar footer should show Backend mode, not raw backend value");
if (app.includes("{health.mode}</Badge>")) fail("UI should not render raw backend mode badges");
if (app.includes("PATH 含 ~/.local/bin")) fail("Host details should show test latency instead of PATH local-bin status");
if (!app.includes("if (!result.ok)")) fail("SSH bootstrap modal must not treat failed results as success");
if (!app.includes('result.writeResult.action === "rolled_back"')) fail("failed new SSH bootstrap should refresh only after managed-block rollback");
if (app.includes('placeholder="10.39.2.30"') || app.includes('placeholder="jy"')) fail("SSH Host modal placeholders must not contain personal host details");
if (app.includes("window.setTimeout(onClose")) fail("SSH Host modal should stay open after successful connection");
if (!app.includes('placeholder="127.0.0.1"') || !app.includes('placeholder="Username"')) fail("SSH Host modal should use generic placeholders");
if (!app.includes("id_ed25519 detected") || app.includes("value={hasIdentityFile ? defaultIdentityFile")) fail("SSH Host modal must not display full IdentityFile paths");
if (app.includes("<p>输入一次远端密码") || app.includes("<span>{message}</span>")) fail("SSH Host modal should not show intro or bottom helper copy");
for (const token of ["TaskLogModal", "taskLogModal", "taskDetailsCol", "copy.tasks.details", "copy.tasks.logs"]) {
  if (!app.includes(token)) fail(`missing task-history log modal token: ${token}`);
}
for (const token of ["logPanel", "publicKeyBox", "commandGrid", "commands.map((command)"]) {
  if (app.includes(token)) fail(`Tasks/Settings simplification should remove old token: ${token}`);
}
for (const token of ["copyPublicKeyButton", "data-success={publicKeyCopied}", "copy.settings.copyPublicKeySuccess", "onCopyPublicKey: (publicKey: string) => Promise<boolean>"]) {
  if (!app.includes(token)) fail(`missing simplified SSH settings copy token: ${token}`);
}

const api = read("src/api.ts");
for (const token of ["connectSshHost", "ssh-bootstrap-progress", "remote-codex-progress", "mockSshBootstrapHostWithProgress", "mockRemoteManageCodexWithProgress", "remoteManageCodex"]) {
  if (!api.includes(token)) fail(`missing bootstrap API token: ${token}`);
}
for (const token of [
  "Research Default",
  "Safe Editing",
  "Research Default Copy",
  "Mac Studio Lab",
  "Windows Workstation",
  "Linux Runner",
  'name: "Diagnostics"'
]) {
  if (api.includes(token)) fail(`web fallback mock data should not include ${token}`);
}
for (const token of [
  "listProfiles",
  "createProfile",
  "updateProfile",
  "deleteProfile",
  "duplicateProfile",
  "importProfiles",
  "exportProfiles",
  "setProfileApiKey",
  "deleteProfileApiKey",
  "previewProfileApply",
  "applyProfile",
  "detectCcSwitchProfiles",
  "importCcSwitchProfiles",
  "refreshLatestCodexVersion",
  "fallbackLatestCodexVersion",
  "env_key",
  "apiKeyEnvVar",
  "credentialStored"
]) {
  if (!api.includes(token)) fail(`missing Profile/API config token: ${token}`);
}
for (const token of [
  "listSkillPacks",
  "list_local_skills",
  "importLocalSkill",
  "import_local_skill",
  "updateLibrarySkillAbout",
  "update_library_skill_about",
  "getSkillInventoryStatus",
  "get_skill_inventory_status",
  "detectInstalledSkills",
  "detect_installed_skills",
  "downloadGithubSkill",
  "download_github_skill",
  "getSkillTargets",
  "get_skill_targets",
  "installSkillTargets",
  "install_skill_targets",
  "uninstallSkillTargets",
  "uninstall_skill_targets",
  "deleteLibrarySkill",
  "delete_library_skill",
  "mockDetectInstalledSkills",
  "mockUpdateLibrarySkillAbout",
  "mockSkillTargetOperation",
  "mockDeleteLibrarySkill",
  "uninstall-success"
]) {
  if (!api.includes(token)) fail(`missing Window 6 Skills API token: ${token}`);
}
for (const removedToken of [
  removed("search", "Online", "Skills"),
  removed("search", "_online", "_skills"),
  removed("clone", "SkillRepo"),
  removed("clone", "_skill", "_repo"),
  removed("list", "RemoteSkills"),
  removed("preview", "RemoteSkillInstall"),
  removed("install", "RemoteSkillBatch"),
  removed("delete", "RemoteSkill"),
  removed("mock", "RemoteSkillList"),
  removed("mock", "RemoteSkillInstallBatch"),
  removed("mock", "RemoteSkillDelete")
]) {
  if (api.includes(removedToken)) fail(`removed Skills API token should not remain: ${removedToken}`);
}

const models = read("src/models.ts");
for (const token of ["SshBootstrapProgressEvent", "RemoteCodexProgressEvent", "RemoteCodexMaintenanceResult", "check-version", "password_login", "verify_alias_login"]) {
  if (!models.includes(token)) fail(`missing bootstrap model token: ${token}`);
}
for (const token of ["apiKeyEnvVar", "credentialStored", "ProfileApplyPreview", "ProfileApplyBatchResult", "ProfileApplyHostResult"]) {
  if (!models.includes(token)) fail(`missing Profile/API model token: ${token}`);
}
for (const token of ["LatestCodexVersion", "version: string | null", "checkedAt: string | null", 'source: "npm" | string']) {
  if (!models.includes(token)) fail(`missing latest Codex version model token: ${token}`);
}
for (const token of ["profiles: Profile[]", "hosts: Host[]"]) {
  if (!models.includes(token)) fail(`Profile apply result should return refreshed state token: ${token}`);
}
for (const token of ["apiConfigName", "apiConfigSource"]) {
  if (!models.includes(token) || !api.includes(token)) fail(`missing host API config model/API token: ${token}`);
}
for (const token of [
  "SkillImportResult",
  "SkillApplication",
  "SkillInventoryStatus",
  "SkillDetectionResult",
  "SkillTargetRequest",
  "SkillTarget",
  "SkillTargetsResult",
  "SkillTargetOperationResult",
  "RemoteSkillListResult",
  "localSkills",
  "sourceType",
  "about",
  "addedAt",
  "applications",
  "managedPath",
  "hasSkillMd"
]) {
  if (!models.includes(token)) fail(`missing Window 6 Skills model token: ${token}`);
}
for (const removedToken of [
  removed("Online", "SkillCandidate"),
  removed("Online", "SkillSearchResult"),
  removed("Remote", "SkillScope"),
  removed("Skill", "ConflictPolicy"),
  removed("Remote", "SkillInstallPreview"),
  removed("Remote", "SkillBatchInstallResult"),
  removed("Remote", "SkillDeleteResult")
]) {
  if (models.includes(removedToken)) fail(`removed Skills model token should not remain: ${removedToken}`);
}

const forbiddenApiKeyTokens = [
  "apiKeyValue",
  "apiKeyPlaintext",
  "apiKeySecret",
  "storedApiKey",
  "profileApiKeyValue",
  "localStorage.setItem(\"apiKey",
  "localStorage.setItem('apiKey"
];
for (const [name, content] of [["src/App.tsx", app], ["src/api.ts", api], ["src/models.ts", models]]) {
  for (const token of forbiddenApiKeyTokens) {
    if (content.includes(token)) fail(`${name} must not render or store direct API key token: ${token}`);
  }
}

const settings = read("src/settings.ts");
for (const fontPreset of ["English", "简体中文", "zh-cn"]) {
  if (!settings.includes(fontPreset)) fail(`missing font preset: ${fontPreset}`);
}
for (const token of ["setupGuideDismissed", "setupGuideDismissed: false"]) {
  if (!settings.includes(token)) fail(`missing setup guide settings token: ${token}`);
}
for (const oldFontPreset of ["System Default", "Chinese Optimized", "English Optimized", "Cross Platform"]) {
  if (settings.includes(oldFontPreset)) fail(`old font preset should not remain: ${oldFontPreset}`);
}

const styles = read("src/styles.css");
for (const token of ["--font-ui", "--font-mono", "--app-content-max: 1220px", "--content-max: var(--app-content-max)", "font-family: var(--font-ui)", "font-family: var(--font-mono)"]) {
  if (!styles.includes(token)) fail(`missing font token: ${token}`);
}
for (const token of ['aria-label={copy.settings.font}', 'data-options="2"', "onFontPresetChange(preset)"]) {
  if (!app.includes(token)) fail(`missing segmented font setting token: ${token}`);
}
if (app.includes("<select value={settings.fontPreset}")) fail("font setting should use the same segmented module style as theme");
if (!styles.includes('.segmentedControl[data-options="2"]')) fail("missing two-option segmented control style");
for (const token of ["navIcon", "metricPrimary", "metricSecondary", "matrixHeader", "matrixEmptyState", "matrixEmptyIcon", ".hostMeta .badge"]) {
  if (!styles.includes(token)) fail(`missing dashboard home polish style token: ${token}`);
}
for (const token of ["setupGuideModal", "setupGuideLanguage", "setupGuideLanguageOption", "setupGuideHostList", "setupGuideHostHeader", "setupGuideActions", "emptyListState", "emptyListIcon", "emptyListActions"]) {
  if (!styles.includes(token)) fail(`missing setup guide or empty-state style token: ${token}`);
}
for (const token of ["matrixEmptyIcon::before", "matrixEmptyIcon::after", "matrixEmptyIcon span"]) {
  if (styles.includes(token)) fail(`matrix empty state should use emoji instead of CSS line icon: ${token}`);
}
for (const token of ["calloutPanel", "hostCardActions", "tagRow", "skillLine", "taskList", "taskItem", "brandSubtle"]) {
  if (styles.includes(token)) fail(`dashboard home polish should remove old style token: ${token}`);
}
for (const token of ["modalBackdrop", "bootstrapLogCard", "stepIcon.success", "stepIcon.failed"]) {
  if (!styles.includes(token)) fail(`missing SSH modal style token: ${token}`);
}
for (const token of ["sshHostsTable", "table-layout: auto", "flex-wrap: nowrap", ".sshHostsCodexCol", ".sshHostsLatestVersionCol"]) {
  if (!styles.includes(token)) fail(`missing SSH Hosts responsive table style token: ${token}`);
}
for (const token of ["profilesStack", "profileLibraryActions", "ccSwitchActionButton", "profileCcSwitchStatus", "profileTable", "profileRowActions", "profileApplyPanel", "profileApplyTable", "profileHostSelectCell", "profileHostSelectModal", "profileHostSelectList", "profileModelCombobox", "profileModelOptions", "profileModelOption", "profileFastModeSegment", "profileFastModeOption"]) {
  if (!styles.includes(token)) fail(`missing compact Profiles style token: ${token}`);
}
for (const token of [
  "skillsStack",
  "skillsTable",
  "skillLibraryActions",
  "skillApplicationTags",
  "skillRowActions",
  "skillDownloadForm",
  "skillPreviewModal",
  ".taskLogModalMeta.skillPreviewMeta",
  "skillPreviewDetails",
  "skillTargetList",
  "skillTargetRow",
  "skillDeleteActions",
  "skillMessage",
  "installedSkillsTable",
  "installedSkillTags",
  "installedSkillTag",
  "--skill-color"
]) {
  if (!styles.includes(token)) fail(`missing Window 6 Skills style token: ${token}`);
}
for (const removedToken of [
  removed("skill", "SearchBar"),
  removed("skill", "Controls"),
  removed("skill", "Segment"),
  removed("skill", "HostList"),
  removed("skill", "RemotePanel"),
  removed("skill", "RemoteHost"),
  removed("skill", "RemoteList"),
  removed("skill", "RemoteRow"),
  removed("skill", "DeleteRow")
]) {
  if (styles.includes(removedToken)) fail(`removed Skills style token should not remain: ${removedToken}`);
}
const ccSwitchActionButtonStyle = styles.match(/\.ccSwitchActionButton\s*\{[^}]*\}/)?.[0] ?? "";
for (const token of ["flex: 0 0 168px", "width: 168px", "height: 38px", "white-space: nowrap"]) {
  if (!ccSwitchActionButtonStyle.includes(token)) fail(`missing fixed one-line cc-switch action button style token: ${token}`);
}
for (const token of [".profileApplyPanel .tableWrap", "overflow-x: hidden", ".profileApplyTable .sshHostsAliasCol", ".profileApplyTable .miniButton"]) {
  if (!styles.includes(token)) fail(`missing responsive profile apply table token: ${token}`);
}
const detailGridBadgeStyle = styles.match(/\.detailGrid \.badge\s*\{[^}]*\}/)?.[0] ?? "";
for (const token of ["white-space: normal", "overflow-wrap: anywhere", "word-break: break-word"]) {
  if (!detailGridBadgeStyle.includes(token)) fail(`missing host detail badge wrapping style token: ${token}`);
}
const codexLogRowsStyle = styles.match(/\.codexOperationLogRows\s*\{[^}]*\}/)?.[0] ?? "";
if (!codexLogRowsStyle.includes("border: 1px solid var(--border)") || !codexLogRowsStyle.includes("background: var(--surface-muted)")) fail("Codex operation logs should render inside one unified panel");
const codexLogCodeStyle = styles.match(/\.codexOperationLogRow code\s*\{[^}]*\}/)?.[0] ?? "";
if (!codexLogCodeStyle.includes("overflow-wrap: anywhere") || !codexLogCodeStyle.includes("white-space: pre-wrap")) fail("Codex operation log detail should wrap long output");
if (codexLogCodeStyle.includes("text-overflow") || codexLogCodeStyle.includes("white-space: nowrap")) fail("Codex operation log detail should not be ellipsized or forced onto one line");
for (const token of ["taskLogModal", "taskLogModalMeta", "taskDetailsCol", "taskTableWrap", "tasksTable", "copyPublicKeyButton", 'data-success="true"', "max-width: var(--app-content-max)"]) {
  if (!styles.includes(token)) fail(`missing simplified UI style token: ${token}`);
}

console.log("SMOKE PASS: CodexHub docs and Tauri skeleton are present.");
