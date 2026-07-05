import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
const requiredFiles = [
  "README.md",
  "LICENSE",
  "SECURITY.md",
  "docs/research.md",
  "docs/architecture.md",
  "docs/mvp-scope.md",
  "docs/known-limitations.md",
  "docs/macos-support.md",
  "docs/public-scope.md",
  "docs/release-checklist.md",
  "docs/release-channels.md",
  "docs/stable-updater.md",
  "docs/zh-CN/README.md",
  "package.json",
  "vite.config.mjs",
  "index.html",
  "src/main.tsx",
  "src/App.tsx",
  "src/api.ts",
  "src/models.ts",
  "src/platform.ts",
  "src/settings.ts",
  "src/styles.css",
  "scripts/mock-dev.mjs",
  "scripts/mock-smoke-test.mjs",
  "scripts/audit-public-scope.mjs",
  "scripts/validate-release.ps1",
  "scripts/package-portable.ps1",
  "scripts/check-release-exe.ps1",
  "scripts/create-updater-tauri-config.mjs",
  "scripts/create-windows-updater-feed.mjs",
  "scripts/create-macos-updater-feed.mjs",
  "figs/app-logo.png",
  "src-tauri/Cargo.toml",
  "src-tauri/tauri.conf.json",
  "src-tauri/tauri.dev.conf.json",
  "src-tauri/tauri.updater.conf.json",
  "src-tauri/icons/32x32.png",
  "src-tauri/icons/64x64.png",
  "src-tauri/icons/128x128.png",
  "src-tauri/icons/128x128@2x.png",
  "src-tauri/icons/icon.png",
  "src-tauri/icons/icon.ico",
  "src-tauri/src/main.rs",
  "src-tauri/src/lib.rs",
  "src-tauri/capabilities/default.json",
  ".github/workflows/build-macos-release.yml",
  ".github/workflows/build-windows-release.yml"
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
if (packageJson.version !== "0.2.8") fail("package version should be 0.2.8");
for (const script of ["tauri", "dev", "dev:web", "dev:mock", "build", "build:tauri", "build:tauri:dev", "build:macos:release", "build:macos:updater", "build:installer:nsis", "build:installer:nsis:updater", "build:installer:nsis:dev", "build:installer:msi", "build:installer:msi:dev", "release:portable", "release:portable:dev", "release:updater-feed", "release:macos-updater-feed", "validate:release", "validate:release:dev", "audit:public", "smoke", "smoke:mock", "test"]) {
  if (!packageJson.scripts?.[script]) fail(`missing package script ${script}`);
}
if (packageJson.scripts.build !== "pnpm build:tauri") fail("default build should use build:tauri");
if (!packageJson.scripts.dev.includes("--config src-tauri/tauri.dev.conf.json")) fail("default dev should use the dev channel Tauri config");
if (!packageJson.scripts["build:tauri"].includes("--no-bundle --ci")) fail("build:tauri should skip installer bundling in CI");
if (!packageJson.scripts["build:tauri:dev"].includes("--config src-tauri/tauri.dev.conf.json")) fail("dev Tauri build should use the dev channel config");
if (!packageJson.scripts["build:installer:nsis:updater"].includes("create-updater-tauri-config.mjs")) fail("Windows updater NSIS build should inject the updater Tauri config");
if (!packageJson.scripts["build:installer:nsis:updater"].includes("src-tauri/tauri.updater.local.json")) fail("Windows updater NSIS build should use the generated local updater artifact config");
if (!packageJson.scripts["build:macos:updater"].includes("create-updater-tauri-config.mjs")) fail("macOS updater build should inject the updater Tauri config");
if (!packageJson.scripts["build:macos:updater"].includes("--bundles app,dmg")) fail("macOS updater build should create app and dmg bundles");
if (!packageJson.scripts["release:updater-feed"].includes("create-windows-updater-feed.mjs")) fail("release:updater-feed should generate the Windows updater feed");
if (!packageJson.scripts["release:macos-updater-feed"].includes("create-macos-updater-feed.mjs")) fail("release:macos-updater-feed should merge the macOS updater feed");
if (!packageJson.scripts["release:portable"].includes("package-portable.ps1")) fail("release:portable should call package-portable.ps1");
if (!packageJson.scripts["release:portable:dev"].includes("-Channel dev")) fail("dev portable release should pass -Channel dev");
if (!packageJson.scripts["validate:release"].includes("validate-release.ps1")) fail("validate:release should call validate-release.ps1");
if (!packageJson.scripts["validate:release:dev"].includes("-Channel dev") || !packageJson.scripts["validate:release:dev"].includes("-NoLive")) fail("dev release validation should stay local and dev-channel only");
if (!packageJson.dependencies?.["@tauri-apps/plugin-dialog"]) fail("missing Tauri dialog plugin dependency");

const tauriConfig = JSON.parse(read("src-tauri/tauri.conf.json"));
const devTauriConfig = JSON.parse(read("src-tauri/tauri.dev.conf.json"));
const updaterTauriConfig = JSON.parse(read("src-tauri/tauri.updater.conf.json"));
if (tauriConfig.productName !== "CodexHub") fail("stable productName should be CodexHub");
if (tauriConfig.identifier !== "app.codexhub.desktop") fail("stable identifier should be app.codexhub.desktop");
if (tauriConfig.version !== "0.2.8") fail("stable Tauri version should be 0.2.8");
if (tauriConfig.app?.windows?.[0]?.title !== "CodexHub") fail("stable window title should be CodexHub");
if (devTauriConfig.productName !== "CodexHub Dev") fail("dev productName should be CodexHub Dev");
if (devTauriConfig.identifier !== "dev.codexhub.desktop") fail("dev identifier should be dev.codexhub.desktop");
if (devTauriConfig.version !== "0.2.8") fail("dev Tauri version should be 0.2.8");
if (devTauriConfig.app?.windows?.[0]?.title !== "CodexHub Dev") fail("dev window title should be CodexHub Dev");
if (tauriConfig.identifier === devTauriConfig.identifier) fail("stable and dev identifiers must differ for app data isolation");
if (tauriConfig.identifier?.endsWith(".app")) fail("Tauri identifier should not end with .app");
if (devTauriConfig.identifier?.endsWith(".app")) fail("Dev Tauri identifier should not end with .app");
if (tauriConfig.plugins?.updater?.pubkey !== "") {
  fail("stable updater plugin needs an empty pubkey placeholder so startup config deserializes before build-time config is injected");
}
if (updaterTauriConfig.bundle?.createUpdaterArtifacts !== true) fail("Windows updater build config should create Tauri updater artifacts");
if (devTauriConfig.bundle?.createUpdaterArtifacts) fail("dev channel must not create updater artifacts");
if (tauriConfig.bundle?.targets === "all") fail("Tauri bundle targets must not default to all installers");
if (Array.isArray(tauriConfig.bundle?.targets) && tauriConfig.bundle.targets.includes("msi")) {
  fail("MSI bundling should be an explicit command, not a default target");
}
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
const macosSupport = read("docs/macos-support.md");
const readme = read("README.md");
const publicScope = read("docs/public-scope.md");
const releaseChecklist = read("docs/release-checklist.md");
const stableUpdater = read("docs/stable-updater.md");
const security = read("SECURITY.md");
const zhReadme = read("docs/zh-CN/README.md");

const requiredText = [
  [readme, "CodexHub is a desktop control console"],
  [zhReadme, "通用桌面控制台，支持 Windows 和 macOS"],
  [readme, "latest stable build"],
  [readme, "CodexHub_0.2.8_aarch64.dmg"],
  [readme, "update checks fail"],
  [zhReadme, "检查更新失败"],
  [readme, "Settings > Codex > Connections"],
  [readme, "Windows tray / macOS menu bar status icon"],
  [readme, "MIT"],
  [zhReadme, "Windows 托盘 / macOS 菜单栏状态图标"],
  [publicScope, "source-only"],
  [publicScope, "pnpm audit:public"],
  [releaseChecklist, "Live SSH Acceptance"],
  [releaseChecklist, "CodexHub Dev"],
  [releaseChecklist, "app.codexhub.desktop"],
  [releaseChecklist, "dev.codexhub.desktop"],
  [releaseChecklist, "validate-release.ps1 -Channel stable -UserTested"],
  [releaseChecklist, "CODEXHUB_STABLE_UPDATE_ENDPOINT"],
  [releaseChecklist, "Settings > Codex > Connections"],
  [stableUpdater, "tauri-plugin-updater"],
  [stableUpdater, "CODEXHUB_STABLE_UPDATER_PUBKEY"],
  [stableUpdater, "Dev And Portable Boundaries"],
  [security, "CodexHub does not write Codex App private state"],
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
  [architecture, "app_config_dir()"],
  [architecture, "app_cache_dir()"],
  [architecture, "bundle identifier"],
  [architecture, "app.codexhub.desktop"],
  [architecture, "dev.codexhub.desktop"],
  [architecture, "Desktop Lifecycle"],
  [architecture, "closeButtonBehavior"],
  [architecture, "close-button-behavior-requested"],
  [architecture, "Stable Updater Foundation"],
  [mvp, "Mandatory remote Codex wrapper"],
  [mvp, "Window 5: profile/API config"],
  [mvp, "Window 6: single-card local skill library"],
  [mvp, "selected host's `~/.codex-hub/env`"],
  [mvp, "Writing local credential-store key names or API key values into remote Codex config"],
  [limitations, "Profiles /"],
  [limitations, "direct GitHub repository URLs and GitHub"],
  [limitations, "CodexHub writes the value only to the selected host's `~/.codex-hub/env`"],
  [limitations, "CodexHub must not write Codex App private state"],
  [security, "CodexHub-managed remote `~/.codex-hub/env`"],
  [limitations, "Menu bar/status item restore"],
  [macosSupport, "Requires real macOS test"],
  [macosSupport, "APPLE_SIGNING_IDENTITY=-"],
  [macosSupport, "~/.ssh/config"]
];

for (const [content, phrase] of requiredText) {
  if (!content.includes(phrase)) fail(`missing required phrase: ${phrase}`);
}

if (/\bWindows-first\b/i.test(readme)) {
  fail("README should describe CodexHub as a Windows and macOS desktop console, not Windows-first");
}

for (const staleDocToken of [
  "export_profiles(profile_ids)",
  "list_ssh_hosts()",
  "generate_ssh_host_block",
  "append_ssh_host_block_with_backup",
  "render_profile_config",
  "remote_restore_backup"
]) {
  if (architecture.includes(staleDocToken) || readme.includes(staleDocToken)) {
    fail(`public docs should not advertise stale command: ${staleDocToken}`);
  }
}

const portableScript = read("scripts/package-portable.ps1");
for (const token of ["CodexHub-v$Version-windows-x64-portable", "CodexHub-Dev-v$Version-windows-x64-portable", "ValidateSet(\"stable\", \"dev\")", "release-artifacts", "Compress-Archive", "SHA256SUMS.txt", "pnpm audit:public"]) {
  if (!portableScript.includes(token)) fail(`missing portable packaging token: ${token}`);
}
for (const internalDoc of ["docs\\release-checklist.md", "docs\\release-channels.md"]) {
  if (portableScript.includes(internalDoc)) fail(`portable package should not include internal release doc: ${internalDoc}`);
}
const releaseValidationScript = read("scripts/validate-release.ps1");
for (const token of ["ValidateSet(\"dev\", \"stable\")", "Stable validation requires -UserTested", "Stable validation cannot use -SkipTauriBuild", "Public leak audit", "Live SSH acceptance", "Summary", "Artifacts:", "Manual test items:"]) {
  if (!releaseValidationScript.includes(token)) fail(`missing release validation token: ${token}`);
}
const publicAuditScript = read("scripts/audit-public-scope.mjs");
for (const token of ["PRIVATE KEY", "sk-[A-Za-z0-9_-]{20,}", "release-artifacts", "personal repository or user identifier", "local home directory", "PUBLIC AUDIT PASS"]) {
  if (!publicAuditScript.includes(token)) fail(`missing public audit token: ${token}`);
}
const exeCheckScript = read("scripts/check-release-exe.ps1");
for (const token of ["APPDATA", "LOCALAPPDATA", "USERPROFILE", "WindowStyle", "Release exe startup check passed"]) {
  if (!exeCheckScript.includes(token)) fail(`missing release exe check token: ${token}`);
}

const cargoToml = read("src-tauri/Cargo.toml");
for (const token of ["tauri-plugin-updater", "url = \"2\"", "base64 = \"0.22\""]) {
  if (!cargoToml.includes(token)) fail(`missing updater Cargo dependency token: ${token}`);
}
if (!cargoToml.includes("features = [\"tray-icon\"]")) fail("Tauri dependency should enable the tray-icon feature");

const rustLib = read("src-tauri/src/lib.rs");
const sshRs = read("src-tauri/src/ssh.rs");
const rustPlatform = read("src-tauri/src/platform.rs");
const tsPlatform = read("src/platform.ts");
const macosWorkflow = read(".github/workflows/build-macos-release.yml");
const windowsWorkflow = read(".github/workflows/build-windows-release.yml");
const updaterConfigScript = read("scripts/create-updater-tauri-config.mjs");
const windowsUpdaterFeedScript = read("scripts/create-windows-updater-feed.mjs");
const macosUpdaterFeedScript = read("scripts/create-macos-updater-feed.mjs");
for (const token of ["sidebar_completion_indicators", "sidebar_completion_indicators: true", "#[serde(default = \"default_true\")]"]) {
  if (!rustLib.includes(token)) fail(`missing sidebar completion settings Rust token: ${token}`);
}
for (const token of ["CODEX_NATIVE_PLATFORM_SCRIPT", "npm-mirror-native-local-upload", "parse_npmmirror_native_metadata", "remote-codex-progress", "RemoteCodexProgressEvent", "run_ssh_script_streaming"]) {
  if (!rustLib.includes(token)) fail(`missing local upload Codex fallback token: ${token}`);
}
for (const command of [
  "app_health",
  "get_app_update_status",
  "check_stable_update",
  "install_stable_update",
  "detect_network_proxy",
  "get_settings",
  "save_settings",
  "choose_close_button_behavior",
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
  "get_local_codex_status",
  "list_profiles",
  "create_profile",
  "update_profile",
  "delete_profile",
  "duplicate_profile",
  "import_profiles",
  "set_profile_api_key",
  "get_profile_api_key",
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
for (const token of ["AppUpdateStatus", "AppUpdateState", "CODEXHUB_STABLE_UPDATE_ENDPOINT", "CODEXHUB_STABLE_UPDATER_PUBKEY", "tauri_plugin_updater::Builder::new().build()", "UpdaterExt", "stable_updater_configured", "normalize_updater_pubkey", "extract_minisign_public_key"]) {
  if (!rustLib.includes(token)) fail(`missing stable updater backend token: ${token}`);
}
if (!cargoToml.includes('tauri-plugin-updater = { version = "2", default-features = false, features = ["native-tls", "zip"] }')) {
  fail("stable updater must use native TLS so release checks can use the OS trust store");
}
if (!cargoToml.includes('reqwest = { version = "0.13", default-features = false, features = ["json", "native-tls"] }')) {
  fail("stable updater GitHub feed resolver must use reqwest with native TLS");
}
for (const token of ["stable_update_endpoints", "resolve_github_latest_json_asset_endpoint", "github_release_api_url", "OCTET_STREAM_ACCEPT", "api.github.com/repos", "stable_update_network_routes", "LOCAL_PROXY_PORTS", "NetworkProxyMode", "detect_network_proxy_status", "builder.proxy(proxy)"]) {
  if (!rustLib.includes(token)) fail(`missing GitHub updater feed fallback token: ${token}`);
}
for (const token of ["app_update_check_task", "app_update_install_task", "app_update_state_label", "record_task(&state, app_update_check_task(&status, &attempts))", "Install app update", "Check app update"]) {
  if (!rustLib.includes(token)) fail(`missing stable updater task token: ${token}`);
}
for (const token of ["install_stable_update", "download_and_install", "AppUpdateState::Installing", "channel != \"stable\"", "stable_updater_configured(&config)", "updater_error_message"]) {
  if (!rustLib.includes(token)) fail(`missing gated stable updater install token: ${token}`);
}
if (rustLib.includes("export_profiles")) fail("Profiles export command should be removed");
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
  "async fn download_installed_skill",
  "async fn get_skill_targets",
  "async fn install_skill_targets",
  "async fn uninstall_installed_skill",
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
  "extract_skill_description",
  "scan_find_fallback",
  "CODEXHUB_REMOTE_SKILL\\t%s\\t%s\\t%s\\t%s\\t%s",
  "remote_skill_install_script",
  "remote_skill_delete_script",
  "remote_installed_skill_archive_script",
  "remote_installed_skill_delete_script",
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
for (const token of ["Latest scan returned no skills; kept previous cached", "previous_inventory", "previous.skills"]) {
  if (!rustLib.includes(token)) fail(`missing installed skill inventory empty-scan guard: ${token}`);
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
for (const token of ["platform_appearance", "PlatformAppearance", "default_platform_appearance"]) {
  if (!rustLib.includes(token)) fail(`missing platform appearance backend token: ${token}`);
}
for (const token of [
  "CloseButtonBehavior",
  "close_button_behavior",
  "default_close_button_behavior",
  "CLOSE_BUTTON_BEHAVIOR_REQUESTED_EVENT",
  "close-button-behavior-requested",
  "TrayIconBuilder",
  "TRAY_MENU_SHOW_ID",
  "TRAY_MENU_QUIT_ID",
  "TrayIconEvent::Click",
  "WindowEvent::CloseRequested",
  "api.prevent_close()",
  "show_main_window",
  "hide_main_window"
]) {
  if (!rustLib.includes(token)) fail(`missing close-to-tray backend token: ${token}`);
}
for (const token of [
  "RuntimePlatform",
  "get_ssh_config_path",
  "get_default_ssh_key_path",
  "get_codex_config_path",
  "get_codex_skills_path",
  "/opt/homebrew/bin/codex",
  "/usr/local/bin/codex",
  ".local/bin/codex"
]) {
  if (!rustPlatform.includes(token)) fail(`missing Rust platform adapter token: ${token}`);
}
for (const token of [
  "getPlatform",
  "isWindows",
  "isMacOS",
  "isLinux",
  "getHomeDir",
  "getSshDir",
  "getSshConfigPath",
  "getDefaultSshKeyPath",
  "getCodexConfigPath",
  "getCodexSkillsPath",
  "detectCodexBinaryPath",
  "/opt/homebrew/bin/codex"
]) {
  if (!tsPlatform.includes(token)) fail(`missing TS platform adapter token: ${token}`);
}
for (const token of [
  "runs-on: macos-14",
  "actions/upload-artifact@v4",
  "pnpm typecheck",
  "pnpm build:web",
  "pnpm build:macos:release",
  "pnpm build:macos:updater",
  "pnpm release:macos-updater-feed",
  "CODEXHUB_STABLE_UPDATER_PUBKEY: ${{ vars.CODEXHUB_STABLE_UPDATER_PUBKEY }}",
  "TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}",
  "upload_to_release == 'true'",
  "gh release upload",
  ".app.tar.gz",
  "codexhub-macos-v${{ steps.meta.outputs.version }}-unsigned-release",
  "APPLE_SIGNING_IDENTITY: \"-\""
]) {
  if (!macosWorkflow.includes(token)) fail(`missing macOS workflow token: ${token}`);
}
for (const forbiddenWorkflowToken of ["softprops/action-gh-release", "gh release create", "tauri-action@v0"]) {
  if (macosWorkflow.includes(forbiddenWorkflowToken)) fail(`macOS release workflow must not create a GitHub Release: ${forbiddenWorkflowToken}`);
}
for (const token of [
  "runs-on: windows-2022",
  "CODEXHUB_STABLE_UPDATE_ENDPOINT: ${{ vars.CODEXHUB_STABLE_UPDATE_ENDPOINT }}",
  "CODEXHUB_STABLE_UPDATER_PUBKEY: ${{ vars.CODEXHUB_STABLE_UPDATER_PUBKEY }}",
  "TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}",
  "pnpm build:installer:nsis:updater",
  "pnpm release:updater-feed",
  "latest.json",
  "SHA256SUMS.txt",
  "upload_to_release == 'true'",
  "gh release upload"
]) {
  if (!windowsWorkflow.includes(token)) fail(`missing Windows updater workflow token: ${token}`);
}
if (windowsWorkflow.includes(".exe.sig#")) fail("Windows GitHub Release upload must not publish standalone updater signature assets");
for (const token of [
  "CODEXHUB_STABLE_UPDATER_PUBKEY",
  "tauri.updater.local.json",
  "normalizePubkey",
  "extractMinisignPublicKey",
  "config.plugins",
  "pubkey"
]) {
  if (!updaterConfigScript.includes(token)) fail(`missing updater Tauri config script token: ${token}`);
}
for (const token of [
  "CodexHub_${version}_x64-setup.exe",
  "const signaturePath = `${updaterPath}.sig`",
  "SHA256SUMS.txt",
  "createHash(\"sha256\")",
  "windows-x86_64",
  "CODEXHUB_RELEASE_TAG",
  "https://github.com/${repo}/releases/download/${normalizedTag}/${updaterName}",
  "latest.json"
]) {
  if (!windowsUpdaterFeedScript.includes(token)) fail(`missing Windows updater feed script token: ${token}`);
}
for (const token of [
  "CodexHub_${version}_${macArch}.dmg",
  "darwin-aarch64",
  ".app.tar.gz",
  "existing release feed",
  "SHA256SUMS.txt",
  "CODEXHUB_RELEASE_TAG",
  "https://github.com/${repo}/releases/download/${normalizedTag}/${tarName}",
  "[platformKey]"
]) {
  if (!macosUpdaterFeedScript.includes(token)) fail(`missing macOS updater feed script token: ${token}`);
}
for (const token of ["CREATE_NO_WINDOW", "process_command", "creation_flags(CREATE_NO_WINDOW)"]) {
  if (!sshRs.includes(token)) fail(`missing hidden Windows child-process token: ${token}`);
}
for (const token of [
  "parse_cc_switch_sqlite_profiles",
  "provider_endpoints",
  "cc-switch.db",
  "currentProviderCodex",
  "credential_stored: false",
  "CcSwitchProfileRecord",
  "#[serde(skip_serializing)]",
  "cc_switch_api_key_from_value",
  "cc_switch_profile_import_key",
  "cc_switch_auth_key_may_hold_api_key",
  "cc_switch_profile_base_url_key",
  "credential_by_key",
  "store_profile_api_key_local",
  "migrate_cc_switch_api_key_for_profile",
  "is_missing_credential_error"
]) {
  if (!rustLib.includes(token)) fail(`missing cc-switch adapter token: ${token}`);
}
for (const token of ["profiles: Vec<Profile>", "hosts: Vec<Host>", "sync_profile_host_links", "sync_profile_host_ids", "clear_profile_host_links", "reconcile_hosts_with_profile_links", "RemoteApiConfigMatch"]) {
  if (!rustLib.includes(token)) fail(`missing profile apply refreshed-state token: ${token}`);
}
const reconcileHostsMatch = rustLib.match(/fn reconcile_hosts_with_profile_links[\s\S]*?\n}\r?\n\r?\nfn record_task/);
if (!reconcileHostsMatch) fail("missing reconcile_hosts_with_profile_links function body");
if (reconcileHostsMatch[0].includes("host.config_exists = Some(true)") || reconcileHostsMatch[0].includes("host.api_config_name = Some(profile.name.clone())")) {
  fail("profile host-link reconcile must not promote local links into confirmed remote API config facts");
}
for (const token of ["api_config_name", "api_config_source", "classify_remote_api_config", "normalize_base_url_key", "read ~/.codex/config.toml base URL"]) {
  if (!rustLib.includes(token)) fail(`missing remote API config probe token: ${token}`);
}
for (const token of [
  "codex_command_available",
  "CODEX_COMMAND_AVAILABLE_SCRIPT",
  "check codex command in PATH",
  "REMOTE_API_ENV_PRESENT_SCRIPT",
  "check remote API env",
  "api_key_env_present",
  "check_profile_api_env",
  "configure_profile_remote_api_key",
  "remote_profile_api_key_script",
  "$HOME/.codex-hub/env",
  "CodexHub managed launcher",
  "CODEXHUB_REMOTE_ENV_CHANGED",
  "CODEXHUB_CODEX_LAUNCHER_CHANGED",
  "shell_single_quote(&shell_single_quote(api_key))"
]) {
  if (!rustLib.includes(token)) fail(`missing remote readiness probe token: ${token}`);
}
const remoteReadinessApp = read("src/App.tsx");
const remoteReadinessModels = read("src/models.ts");
for (const token of ["codexCommandAvailable", "apiKeyEnvPresent", "codexPathMissing", "apiEnvMissing"]) {
  if (!remoteReadinessApp.includes(token) && !remoteReadinessModels.includes(token)) fail(`missing remote readiness UI/type token: ${token}`);
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
for (const label of ["Home", "主页", "Hosts", "Profiles", "Skills", "Tasks", "✅ Tasks", "✅ 任务", "Settings", "Host Matrix", "主机矩阵", "Font", "Sidebar visual hints", "侧边栏视觉提示", "Host list", "主机列表", "Local config", "本地配置", "🎨 Appearance", "🎨 外观", "🔑 Local keys", "🔑 本地密钥", "🧭 Version info", "🧭 版本信息", "⚙️ Other", "⚙️ 其他", "Program close button behavior", "程序关闭按钮行为", "Host IP", "Codex版本", "Test all", "一键测试", "Update outdated", "一键更新", "Details", "详情", "Logs", "日志", "Copied!", "复制成功！", "Add Server", "添加服务器", "来源", "System", "系统", "Codex", "API config", "API 配置", "Test latency", "测试延迟", "stdout", "stderr", "Install Codex", "Update Codex", "新增 SSH Host", "连接进程", "BootstrapProgressLog", "Ask next time", "Exit app", "Minimize to tray", "关闭按钮", "下次询问", "退出程序", "最小化到托盘"]) {
  if (!app.includes(label)) fail(`missing UI label: ${label}`);
}
for (const token of [
  "closeButtonPromptOpen",
  "CloseButtonBehaviorPromptModal",
  "api.onCloseButtonBehaviorRequested",
  "api.chooseCloseButtonBehavior",
  "handleChooseCloseButtonBehavior",
  "onCloseButtonBehaviorChange",
  "copy.settings.closeButton",
  "copy.settings.closeButtonOptions",
  "copy.closeButtonPrompt"
]) {
  if (!app.includes(token)) fail(`missing close-button UI token: ${token}`);
}
for (const token of ["appUpdateStatus", "appUpdateFailureTask", "appUpdateChecking", "appUpdateInstalling", "copy.settings.appUpdates", "copy.settings.dailyUpdateCheck", "copy.settings.softwareName", "copy.settings.installedAt", "copy.settings.updatedAt", "copy.settings.checkStableUpdate", "copy.settings.installStableUpdate", "copy.settings.checkFailed", "copy.settings.updateCheckFailureHint", "copy.settings.pendingConfiguration", 'className="sshHostsTable versionInfoTable"', "appUpdateStatus.softwareName", "appUpdateStatus.installedAt ?? copy.settings.unknown", "appVersionTone(appUpdateStatus.currentVersion, appUpdateStatus.latestVersion)", "appUpdateLatestVersionLabel(appUpdateStatus, copy)", "appLatestVersionTone(appUpdateStatus)", "title={appUpdateStatus.message}", "appUpdateStatus.checkedAt ?? copy.settings.notChecked", "latestAppUpdateTask", "latestAppInstallTask", "createLocalAppUpdateTask", "footer={("]) {
  if (!app.includes(token)) fail(`missing stable updater Settings UI token: ${token}`);
}
for (const token of ["NetworkProxyManualModal", "networkProxyManualOpen", "handleNetworkProxyChoice", "copy.settings.networkProxy", "copy.settings.networkProxyOptions", "copy.settings.networkProxyManualTitle", "copy.settings.networkProxyPort", "copy.settings.networkProxySave", "onNetworkProxyModeChange", "onNetworkProxyManualRequest", "networkProxyControl"]) {
  if (!app.includes(token)) fail(`missing network proxy Settings UI token: ${token}`);
}
if (app.includes("networkProxyInline")) fail("network proxy Settings UI should not expose an inline proxy input");
for (const token of ["APP_UPDATE_DAILY_CHECK_HOUR = 4", "nextDailyAppUpdateCheckAt", "runStableUpdateCheck(\"daily\")", "scheduleNextAppUpdateCheck", "appUpdateStatusRef", "appUpdateBusyRef"]) {
  if (!app.includes(token)) fail(`missing daily stable updater check token: ${token}`);
}
for (const token of ["function appVersionTone", "function appUpdateLatestVersionLabel", "function appLatestVersionTone", "isCodexVersionBehind(current, latest) ? \"red\" : \"green\"", "status.state === \"error\" && status.checkedAt", "status.state === \"pending-configuration\""]) {
  if (!app.includes(token)) fail(`missing app version badge tone token: ${token}`);
}
for (const label of ["Version info", "版本信息", "Software", "软件名", "Installed at", "安装时间", "Updated at", "更新时间", "Check failed", "检查失败", "Pending setup", "待配置"]) {
  if (!app.includes(label)) fail(`missing version info UI label: ${label}`);
}
for (const token of ["canInstallStableUpdate", "appUpdateStatus.state === \"available\"", "onInstallStableUpdate", "api.installStableUpdate", "showSidebarStableUpdateButton", "canInstallSidebarStableUpdate", "sidebarUpdateButton", "copy.settings.sidebarInstallStableUpdate"]) {
  if (!app.includes(token)) fail(`stable updater install UI must stay gated by available update: ${token}`);
}
if (!app.includes('const canCheckStableUpdate = appUpdateStatus.channel === "stable" && !appUpdateBusy')) {
  fail("Stable update Check button should remain clickable even when feed/signing configuration is pending");
}
if (app.includes('const canCheckStableUpdate = appUpdateStatus.channel === "stable" && appUpdateStatus.configured')) {
  fail("Stable update Check button must not be disabled solely because updater feed/signing is pending");
}
for (const token of ['icon: "🏠"', 'icon: "🖥️"', 'icon: "🧾"', 'icon: "🧩"', 'icon: "✅"', 'icon: "⚙️"', 'className="navIcon"', "metricPrimary", "metricSecondary", "appliedProfileCount", "new Set(hosts.map((host) => host.profileId)", "successfulTaskCount", "matrixHeader", "matrixEmptyIcon", "onAddServer", "onTestAllSshHosts"]) {
  if (!app.includes(token)) fail(`missing dashboard home polish token: ${token}`);
}
for (const token of [
  "SectionCompletionTone",
  "sectionCompletionSignals",
  "sidebarCompletionIndicatorsRef",
  "runSectionOperation",
  "markSectionCompletionSignal",
  "clearSectionCompletionSignal(item.id)",
  "className=\"navCompletionDot\"",
  "data-tone={completionTone}",
  "onPointerDownCapture={handleContentInteraction}",
  "onScrollCapture={handleContentInteraction}",
  "onWheelCapture={handleContentInteraction}",
  "copy.settings.sidebarCompletionIndicators",
  "className=\"pillToggle\"",
  "role=\"switch\"",
  "onSidebarCompletionIndicatorsChange"
]) {
  if (!app.includes(token)) fail(`missing sidebar completion indicator UI token: ${token}`);
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
  "api.downloadInstalledSkill",
  "api.getSkillTargets",
  "api.installSkillTargets",
  "api.uninstallInstalledSkill",
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
  "InstalledSkillPreviewModal",
  "InstalledSkillOperationModal",
  "SkillInstalledConfirmModal",
  "downloadInstalledSkill",
  "uninstallInstalledSkill",
  "downloadInstalledStarted",
  "uninstallInstalledStarted",
  "operationWaiting",
  "description: skill.description?.trim() ?? \"\"",
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
if (!app.includes("CodexUninstallConfirmModal") || !app.includes("uninstallCodexConfirmBody") || !app.includes('runRemoteCodexAction(target.hostAlias, "uninstall")')) fail("Remote Codex uninstall should require an explicit confirmation modal before execution");
if (app.includes('event.status === "success" ? "success"') || app.includes('event.status === "failed" ? "failed"')) fail("Codex operation modal status should only change from the final result or catch path");
if (!app.includes("logRowsRef") || !app.includes("logRows.scrollTop = logRows.scrollHeight") || !app.includes("ref={logRowsRef}")) fail("Codex operation log should auto-scroll to the latest row");
if (app.includes("<code>{compactProgressLogDetail") || app.includes("<code>{compactTaskLogDetail") || app.includes("{log.detail ? <code>")) fail("Codex operation modal should show compact log summaries without console-style detail rows");
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
  "api.setProfileApiKey",
  "api.getProfileApiKey",
  "api.previewProfileApply",
  "api.applyProfile",
  "api.detectCcSwitchProfiles",
  "api.importCcSwitchProfiles",
  "apiKeyEnvVar",
  "credentialStored"
]) {
  if (!app.includes(token)) fail(`missing Profiles UI/API token: ${token}`);
}
for (const token of ["api.exportProfiles", "copy.profiles.export"]) {
  if (app.includes(token)) fail(`Profiles export UI/API token should be removed: ${token}`);
}
for (const token of ["Store key", "Delete key", "存储 key", "删除 key", "未存储凭据", "No stored credential", "api.deleteProfileApiKey"]) {
  if (app.includes(token)) fail(`removed credential editor token should not appear in App UI: ${token}`);
}
for (const token of ["SimpleDeleteConfirmModal", "deleteHostAlias", "deleteProfileId", "credentialVisible", "CredentialVisibilityIcon", "viewBox=\"0 0 24 24\"", "showApiKey", "hideApiKey", "onGetProfileApiKey", "handleDetectLocalSshHosts"]) {
  if (!app.includes(token)) fail(`missing delete confirmation or API key editor token: ${token}`);
}
for (const token of ["const nextProfiles = await api.listProfiles();", "passwordInputWrap profileCredentialInputWrap"]) {
  if (!app.includes(token)) fail(`missing profile credential refresh/visibility token: ${token}`);
}
for (const token of ["disabled={!password}", "disabled={credentialLoading || (!canLoadStoredCredential && !credentialInput.trim())"]) {
  if (app.includes(token)) fail(`credential visibility button must stay visible and enabled: ${token}`);
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
  'apiKeyEnvVar: "Env key"',
  'apiKeyEnvVar: "环境变量"',
  "apiKeyStoredPlaceholder",
  "Credential stored",
  "Third-party import",
  "第三方导入",
  "Local storage",
  "本地存储",
  "Rendered TOML",
  "backup"
]) {
  if (!app.includes(token)) fail(`missing compact Profiles UI label: ${token}`);
}
for (const token of ["ProfileEditModal", "ProfileHostSelectModal", "ProfileApplyPreviewModal", "ProfileApplyOperationModal", "ProfileModelCombobox", "ProfileStorageBadge", "profileLibraryActions", "ccSwitchActionButton", "profileCcSwitchStatus", "profileRowActions", "profileApplyTable", "profileHostSelectModal", "profileFastModeSegment"]) {
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
  "selectedApplyHostIds",
  "profileApplyEligibleHostIds",
  "profileApplyRunningHostIdSet",
  "copy.profiles.alreadyApplied",
  "hostSourceLabel(copy, host)",
  "copy.profiles.applyColumn",
  "copy.profiles.selectHosts",
  "copy.profiles.selectAll",
  "copy.profiles.apiConfig",
  "copy.profiles.noApiConfig",
  "copy.profiles.unknownApiConfig",
  "copy.profiles.applyOperationTitle",
  "copy.profiles.applyOperationStarted",
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
  ["delete", "copy.profiles.delete"]
];
for (const [action, token] of profileRowActions) {
  if (!app.includes(token)) fail(`missing profile row ${action} action token: ${token}`);
}
if (!app.includes('activeSection === "profiles"') || !app.includes("copy.profiles.newApiConfig") || !app.includes("newProfileRequest")) {
  fail("Profiles page header should include a top-level New API config action");
}
if (!app.includes("lastNewProfileRequestRef") || !app.includes("newProfileRequest === lastNewProfileRequestRef.current")) {
  fail("Profiles new-profile request should not replay when entering the Profiles page");
}
for (const token of [
  'namePlaceholder: "Custom config name"',
  'namePlaceholder: "自定义配置名"',
  "modelPlaceholder: DEFAULT_PROFILE_MODEL",
  "providerPlaceholder: DEFAULT_PROFILE_PROVIDER",
  "baseUrlPlaceholder: DEFAULT_PROFILE_BASE_URL",
  "placeholder={copy.profiles.namePlaceholder}",
  "placeholder={copy.profiles.modelPlaceholder}",
  "placeholder={copy.profiles.providerPlaceholder}",
  "placeholder={copy.profiles.baseUrlPlaceholder}",
  "profileDraftWithCreateDefaults"
]) {
  if (!app.includes(token)) fail(`missing Profiles new-profile placeholder token: ${token}`);
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
if (!app.includes('className="profilesStack"') || !app.includes("profileLibraryActions") || !app.includes("profileTable") || !app.includes("profileRowActions") || !app.includes("profileApplyPanel") || !app.includes("profileApplyTable") || !app.includes("profileApplyOperationModal")) {
  fail("Profiles page should use compact stack, library actions, row actions, table, apply panel, and operation-log tokens");
}
const profileApplyTableStart = app.indexOf('className="sshHostsTable profileApplyTable"');
const profileApplyTableEnd = profileApplyTableStart >= 0 ? app.indexOf("</table>", profileApplyTableStart) : -1;
const profileApplyTableBlock = profileApplyTableStart >= 0 && profileApplyTableEnd >= 0 ? app.slice(profileApplyTableStart, profileApplyTableEnd) : "";
if (!profileApplyTableBlock) fail("missing profile apply table block");
if (profileApplyTableBlock.includes('type="checkbox"') || profileApplyTableBlock.includes("data-selected")) {
  fail("Profile apply table should not use checkbox or selected-row state for applied config");
}
for (const token of ["const alreadyApplied = selectedProfile ? profileMatchesConfirmedHostApiConfig(selectedProfile, host) : false", "disabled={!selectedProfile || alreadyApplied || profileApplyRunningHostIdSet.has(host.id)}"]) {
  if (!profileApplyTableBlock.includes(token)) fail(`missing profile apply applied-state token: ${token}`);
}
for (const token of ["setSelectedHostIds([])", "const alreadyApplied = profileMatchesConfirmedHostApiConfig(profile, host)", "const disabled = alreadyApplied || applying", "disabled={disabled}", "setSelectedHostIds(eligibleHostIds)"]) {
  if (!app.includes(token)) fail(`missing profile host picker eligibility token: ${token}`);
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
if (!app.includes('onManageCodex(sshHost.alias, "install")') || !app.includes('onManageCodex(sshHost.alias, "update")') || !app.includes('onManageCodex(sshHost.alias, "uninstall")')) fail("SSH Hosts table should expose remote Codex install/update/uninstall actions");
if (!rustLib.includes('remove_path "$CODEX_HOME"') || !rustLib.includes('remove_path "$hub_dir"') || rustLib.includes("codexhub.uninstall.bak")) fail("Remote Codex uninstall should directly delete Codex config/env paths without backups");
if (!app.includes('installCodex: "安装"') || !app.includes('updateCodex: "更新"') || !app.includes('uninstallCodex: "卸载"')) fail("SSH Hosts Codex buttons should use short install/update/uninstall labels");
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
if (app.includes(`placeholder="${removed("10", ".39", ".2", ".30")}"`) || app.includes('placeholder="jy"')) fail("SSH Host modal placeholders must not contain personal host details");
if (app.includes("window.setTimeout(onClose")) fail("SSH Host modal should stay open after successful connection");
if (!app.includes('placeholder="127.0.0.1"') || !app.includes('placeholder="Username"')) fail("SSH Host modal should use generic placeholders");
if (!app.includes("id_ed25519 detected") || app.includes("value={hasIdentityFile ? defaultIdentityFile")) fail("SSH Host modal must not display full IdentityFile paths");
if (app.includes("<p>输入一次远端密码") || app.includes("<span>{message}</span>")) fail("SSH Host modal should not show intro or bottom helper copy");
for (const token of ["TaskLogModal", "taskLogDetailModal", "taskLogFlowRow", "taskLogMetaGrid", "taskLogStreamGrid", "taskDetailsCol", "copy.tasks.details", "copy.tasks.logs", "open={task.status === \"failed\" || log.level === \"error\"}"]) {
  if (!app.includes(token)) fail(`missing task-history log modal token: ${token}`);
}
for (const token of ["logPanel", "publicKeyBox", "commandGrid", "commands.map((command)"]) {
  if (app.includes(token)) fail(`Tasks/Settings simplification should remove old token: ${token}`);
}
for (const token of ["copyPublicKeyButton", "data-success={publicKeyCopied}", "copy.settings.copyPublicKeySuccess", "onCopyPublicKey: (publicKey: string) => Promise<boolean>"]) {
  if (!app.includes(token)) fail(`missing simplified SSH settings copy token: ${token}`);
}

const api = read("src/api.ts");
for (const token of ["fallbackAppUpdateStatus", "getAppUpdateStatus", "checkStableUpdate", "installStableUpdate", "detectNetworkProxy", "get_app_update_status", "check_stable_update", "install_stable_update", "detect_network_proxy"]) {
  if (!api.includes(token)) fail(`missing stable updater API token: ${token}`);
}
if (!api.includes('checkStableUpdate: () => requiredInvoke<AppUpdateStatus>("check_stable_update")')) {
  fail("Stable update check should expose backend/IPC errors instead of falling back to mock status");
}
for (const token of ["chooseCloseButtonBehavior", "choose_close_button_behavior", "onCloseButtonBehaviorRequested", "close-button-behavior-requested", "requiredInvoke<AppSettings>"]) {
  if (!api.includes(token)) fail(`missing close-button API token: ${token}`);
}
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
  "setProfileApiKey",
  "getProfileApiKey",
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
if (api.includes("exportProfiles")) fail("Profiles export API should be removed");
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
  "downloadInstalledSkill",
  "download_installed_skill",
  "getSkillTargets",
  "get_skill_targets",
  "installSkillTargets",
  "install_skill_targets",
  "uninstallInstalledSkill",
  "uninstall_installed_skill",
  "uninstallSkillTargets",
  "uninstall_skill_targets",
  "deleteLibrarySkill",
  "delete_library_skill",
  "mockDetectInstalledSkills",
  "mockDownloadInstalledSkill",
  "mockUninstallInstalledSkill",
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
for (const token of ["AppUpdateStatus", "AppUpdateState", "pending-configuration", "up-to-date", "installing", "feedConfigured", "signingConfigured"]) {
  if (!models.includes(token)) fail(`missing stable updater model token: ${token}`);
}
for (const token of ["SshBootstrapProgressEvent", "RemoteCodexProgressEvent", "RemoteCodexMaintenanceResult", "check-version", "password_login", "verify_alias_login"]) {
  if (!models.includes(token)) fail(`missing bootstrap model token: ${token}`);
}
for (const token of ["apiKeyEnvVar", "credentialStored", "ProfileApiKeyResult", "ProfileApplyPreview", "ProfileApplyBatchResult", "ProfileApplyHostResult"]) {
  if (!models.includes(token)) fail(`missing Profile/API model token: ${token}`);
}
for (const token of ["SshConfigDeleteResult", "DeleteOperationResult", "task: TaskRun"]) {
  if (!models.includes(token)) fail(`missing delete operation model token: ${token}`);
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
  "InstalledSkillRequest",
  "InstalledSkillDownloadResult",
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
for (const token of ["setupGuideDismissed", "setupGuideDismissed: false", "platformAppearance", "platformAppearance: \"auto\"", "networkProxyMode", "networkProxyMode: \"auto\"", "networkProxyUrl", "sidebarCompletionIndicators", "sidebarCompletionIndicators: true", "candidate.sidebarCompletionIndicators !== false", "resolvePlatformAppearance", "applyPlatformAppearance"]) {
  if (!settings.includes(token)) fail(`missing settings token: ${token}`);
}
for (const token of [
  "CloseButtonBehavior",
  "closeButtonBehavior",
  "closeButtonBehavior: \"ask\"",
  "closeButtonBehaviorValues",
  "\"minimize-to-tray\""
]) {
  if (!settings.includes(token)) fail(`missing close-button settings token: ${token}`);
}
for (const oldFontPreset of ["System Default", "Chinese Optimized", "English Optimized", "Cross Platform"]) {
  if (settings.includes(oldFontPreset)) fail(`old font preset should not remain: ${oldFontPreset}`);
}

const styles = read("src/styles.css");
for (const token of ["appUpdatePanel", "appUpdateSchedule", "sidebarUpdateButton", "versionInfoTable", "taskLogModalHint", "networkProxyControl", "table-layout: fixed", "white-space: normal", "overflow-wrap: anywhere"]) {
  if (!styles.includes(token)) fail(`missing stable updater Settings style token: ${token}`);
}
const versionInfoTableStyle = styles.match(/\.versionInfoTable\s*\{[^}]*\}/)?.[0] ?? "";
if (!versionInfoTableStyle.includes("min-width: 0")) fail("Version info table should shrink inside the Settings card");
const versionInfoCellStyle = styles.match(/\.versionInfoTable th,\s*\.versionInfoTable td\s*\{[^}]*\}/)?.[0] ?? "";
if (!versionInfoCellStyle.includes("white-space: normal") || !versionInfoCellStyle.includes("overflow-wrap: anywhere")) {
  fail("Version info table cells should wrap internally instead of forcing horizontal overflow");
}
for (const token of ["--font-ui", "--font-mono", "--app-content-max: 1220px", "--content-max: var(--app-content-max)", "font-family: var(--font-ui)", "font-family: var(--font-mono)"]) {
  if (!styles.includes(token)) fail(`missing font token: ${token}`);
}
for (const token of ['aria-label={copy.settings.font}', 'aria-label={copy.settings.platformAppearance}', 'data-options="2"', "onFontPresetChange(preset)", "onPlatformAppearanceChange(choice)"]) {
  if (!app.includes(token)) fail(`missing segmented font setting token: ${token}`);
}
for (const removedToken of ["localCodexStatus", "localCodexBusy", "onRefreshLocalCodex", "copy.settings.localCodexCli", "localCodexCli", "localCodexDetected", "localCodexMissing", "codexSearchPaths", "codexInstallHint", "codexCliDetails"]) {
  if (app.includes(removedToken) || styles.includes(removedToken)) fail(`Local Codex CLI Settings card should be removed: ${removedToken}`);
}
if (app.includes("<select value={settings.fontPreset}")) fail("font setting should use the same segmented module style as theme");
if (!styles.includes('.segmentedControl[data-options="2"]')) fail("missing two-option segmented control style");
if (!app.includes('className="settingsRows dividedSettingsRows appearanceRows"')) fail("missing divided appearance settings row group");
const appearanceDividerCount = (app.match(/data-divider="true"/g) ?? []).length;
if (appearanceDividerCount < 3) fail("settings cards should keep theme, sidebar visual hint, and close behavior dividers");
if (!/className="settingControlRow" data-divider="true">\s*<span>{copy\.settings\.theme}<\/span>/.test(app)) {
  fail("appearance card should place the first divider above the theme row");
}
if (/className="settingControlRow" data-divider="true">\s*<span>{copy\.settings\.platformAppearance}<\/span>/.test(app)) {
  fail("appearance card should not place the first divider above the platform row");
}
if (!/className="settingControlRow" data-divider="true">\s*<span>{copy\.settings\.sidebarCompletionIndicators}<\/span>/.test(app)) {
  fail("appearance card should place a divider above the sidebar visual hints row");
}
if (!/className="settingControlRow" data-divider="true">\s*<span>{copy\.settings\.closeButtonBehavior}<\/span>/.test(app)) {
  fail("other settings card should place a divider above the close button behavior row");
}
if (/className="settingControlRow" data-divider="true">\s*<span>{copy\.settings\.networkProxy}<\/span>/.test(app)) {
  fail("other settings card should not place a divider above the network proxy row");
}
for (const token of [
  ".dividedSettingsRows",
  '.dividedSettingsRows .settingControlRow[data-divider="true"]::before',
  "min-height: 60px",
  "padding: 8px 0",
  ".dividedSettingsRows .segmentedControl",
  "height: 44px",
  ".dividedSettingsRows .segmentedControl button"
]) {
  if (!styles.includes(token)) fail(`missing appearance row alignment style token: ${token}`);
}
for (const token of ["navIcon", "navLabel", "navCompletionDot", "navCompletionDot[data-tone=\"error\"]", "pillToggle", "pillToggleThumb", "translateX(22px)", "metricPrimary", "metricSecondary", "matrixHeader", "matrixEmptyState", "matrixEmptyIcon", ".hostMeta .badge"]) {
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
for (const token of ["profilesStack", "profileLibraryActions", "ccSwitchActionButton", "profileCcSwitchStatus", "profileTable", "profileRowActions", "profileApplyPanel", "profileApplyTable", "profileApplyOperationModal", "profileHostSelectModal", "profileHostSelectList", "profileHostSelectStatus", "profileModelCombobox", "profileModelOptions", "profileModelOption", "profileFastModeSegment", "profileFastModeOption"]) {
  if (!styles.includes(token)) fail(`missing compact Profiles style token: ${token}`);
}
for (const token of ["simpleDeleteModal", "credentialVisibilityButton", "credentialEyeIcon", ".sshHostModal.ProfileEditModal .fieldGroup", ".sshHostModal:not(.ProfileEditModal) .fieldGroup input", "::-ms-reveal", ".passwordInputWrap .credentialVisibilityButton"]) {
  if (!styles.includes(token)) fail(`missing delete confirmation or credential editor style token: ${token}`);
}
for (const token of [".credentialEyeIcon::before", ".credentialEyeIcon::after"]) {
  if (styles.includes(token)) fail(`credential visibility icon must be real SVG, not CSS-drawn pseudo icon: ${token}`);
}
for (const token of ["min-width: 0", ".skillsTable td:nth-child(5)", ".skillRowActions", "flex-wrap: wrap"]) {
  if (!styles.includes(token)) fail(`missing responsive Skills table style token: ${token}`);
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
