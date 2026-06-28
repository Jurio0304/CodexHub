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
  "src-tauri/Cargo.toml",
  "src-tauri/tauri.conf.json",
  "src-tauri/icons/icon.ico",
  "src-tauri/src/main.rs",
  "src-tauri/src/lib.rs",
  "src-tauri/capabilities/default.json"
];

const read = (file) => fs.readFileSync(path.join(root, file), "utf8");
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

JSON.parse(read("src-tauri/tauri.conf.json"));
JSON.parse(read("src-tauri/capabilities/default.json"));

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
  [mvp, "Mandatory remote Codex wrapper"],
  [mvp, "Window 5: profile/API config"],
  [mvp, "Writing local credential-store key names or API key values into remote Codex config"],
  [limitations, "Profiles /"],
  [limitations, "The optional stored local credential key is never written to remote"],
  [limitations, "CodexHub must not write Codex App private state"]
];

for (const [content, phrase] of requiredText) {
  if (!content.includes(phrase)) fail(`missing required phrase: ${phrase}`);
}

const rustLib = read("src-tauri/src/lib.rs");
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
  "list_tasks"
]) {
  if (!rustLib.includes(command)) fail(`missing ${command} Tauri command`);
}
for (const asyncCommand of ["async fn ssh_check", "async fn remote_probe_codex", "async fn remote_manage_codex", "async fn refresh_latest_codex_version"]) {
  if (!rustLib.includes(asyncCommand)) fail(`long remote command must stay async: ${asyncCommand}`);
}
if (!rustLib.includes("spawn_blocking(command)")) fail("long remote commands should run through the blocking worker pool");
for (const token of ["LatestCodexVersion", "parse_npm_latest_metadata", "latest_codex_cache_is_fresh", "CODEX_LATEST_REFRESH_HOUR", "https://registry.npmjs.org/@openai/codex", "codex-latest.json"]) {
  if (!rustLib.includes(token)) fail(`missing latest Codex version backend token: ${token}`);
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
for (const label of ["Dashboard", "Hosts", "Profiles", "Skills", "Tasks", "Settings", "Server Matrix", "Font", "SSH Hosts", "Host IP", "Codex版本", "Test all", "一键测试", "测试延迟", "stdout", "stderr", "Install Codex", "Update Codex", "新增 SSH Host", "连接进程", "BootstrapProgressLog"]) {
  if (!app.includes(label)) fail(`missing UI label: ${label}`);
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
  'library: "Profile library"',
  'library: "配置库"',
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
  "skillCountTone",
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
for (const oldFontPreset of ["System Default", "Chinese Optimized", "English Optimized", "Cross Platform"]) {
  if (settings.includes(oldFontPreset)) fail(`old font preset should not remain: ${oldFontPreset}`);
}

const styles = read("src/styles.css");
for (const token of ["--font-ui", "--font-mono", "font-family: var(--font-ui)", "font-family: var(--font-mono)"]) {
  if (!styles.includes(token)) fail(`missing font token: ${token}`);
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

console.log("SMOKE PASS: CodexHub docs and Tauri skeleton are present.");
