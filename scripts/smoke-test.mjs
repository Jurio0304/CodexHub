import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
const requiredFiles = [
  "README.md",
  "LICENSE",
  "SECURITY.md",
  "docs/research.md",
  "docs/architecture.md",
  "docs/desktop-command-boundaries.md",
  "docs/feedback-error-handling.md",
  "docs/storage-migrations.md",
  "docs/mvp-scope.md",
  "docs/known-limitations.md",
  "docs/linux-support.md",
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
  "src/ui/ModalFrame.tsx",
  "src/ui/AlertModalFrame.tsx",
  "src/ui/ConfirmDialog.tsx",
  "src/ui/AppErrorBoundary.tsx",
  "src/ui/feedback.tsx",
  "src/ui/OperationProgress.tsx",
  "src/api.ts",
  "src/api/contracts.ts",
  "src/api/desktop.ts",
  "src/api/fallbacks.ts",
  "src/api/index.ts",
  "src/api/invoke.ts",
  "src/api/mock.ts",
  "src/api/normalize.ts",
  "src/api/desktop.test.ts",
  "src/api/normalize.test.ts",
  "src/ui/ProfileApplyFlow.test.tsx",
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
  "scripts/create-linux-updater-feed.mjs",
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
  "src-tauri/src/app_runtime.rs",
  "src-tauri/src/backend_tests.rs",
  "src-tauri/src/contracts.rs",
  "src-tauri/src/domain.rs",
  "src-tauri/src/jobs.rs",
  "src-tauri/src/commands/host_ssh.rs",
  "src-tauri/src/commands/profiles.rs",
  "src-tauri/src/commands/settings.rs",
  "src-tauri/src/commands/skills.rs",
  "src-tauri/src/commands/storage.rs",
  "src-tauri/src/commands/tasks.rs",
  "src-tauri/src/commands/updater.rs",
  "src-tauri/src/adapters/credentials.rs",
  "src-tauri/src/adapters/events.rs",
  "src-tauri/src/services/codex_runtime.rs",
  "src-tauri/src/services/profile_links.rs",
  "src-tauri/src/services/host_operations.rs",
  "src-tauri/src/services/host_use_cases.rs",
  "src-tauri/src/services/profile_catalog.rs",
  "src-tauri/src/services/profile_operations.rs",
  "src-tauri/src/services/profile_use_cases.rs",
  "src-tauri/src/services/skill_operations.rs",
  "src-tauri/src/services/skill_use_cases.rs",
  "src-tauri/src/services/storage_operations.rs",
  "src-tauri/src/services/updater_operations.rs",
  "src-tauri/src/hosts.rs",
  "src-tauri/src/profiles.rs",
  "src-tauri/src/resource_monitor.rs",
  "src-tauri/src/settings.rs",
  "src-tauri/src/skills.rs",
  "src-tauri/src/tasks.rs",
  "src-tauri/src/updater.rs",
  "src-tauri/capabilities/default.json",
  ".github/workflows/ci.yml",
  ".github/workflows/build-macos-release.yml",
  ".github/workflows/build-linux-release.yml",
  ".github/workflows/build-windows-release.yml"
];

const read = (file) => fs.readFileSync(path.join(root, file), "utf8").replace(/\r\n?/g, "\n");
const removed = (...parts) => parts.join("");
const fail = (message) => {
  console.error(`SMOKE FAIL: ${message}`);
  process.exit(1);
};

for (const file of requiredFiles) {
  if (!fs.existsSync(path.join(root, file))) fail(`missing ${file}`);
}

const packageJson = JSON.parse(read("package.json"));
if (packageJson.version !== "0.4.7") fail("package version should be 0.4.7");
for (const script of ["tauri", "dev", "dev:web", "dev:mock", "build", "build:tauri", "build:tauri:dev", "build:linux:release", "build:linux:updater", "build:macos:release", "build:macos:updater", "build:installer:nsis", "build:installer:nsis:updater", "build:installer:nsis:dev", "build:installer:msi", "build:installer:msi:dev", "release:portable", "release:portable:dev", "release:updater-feed", "release:linux-updater-feed", "release:macos-updater-feed", "validate:release", "validate:release:dev", "audit:public", "smoke", "smoke:mock", "test"]) {
  if (!packageJson.scripts?.[script]) fail(`missing package script ${script}`);
}
if (packageJson.scripts.build !== "pnpm build:tauri") fail("default build should use build:tauri");
if (!packageJson.scripts.dev.includes("--config src-tauri/tauri.dev.conf.json")) fail("default dev should use the dev channel Tauri config");
if (!packageJson.scripts["build:tauri"].includes("--no-bundle --ci")) fail("build:tauri should skip installer bundling in CI");
if (!packageJson.scripts["build:tauri:dev"].includes("--config src-tauri/tauri.dev.conf.json")) fail("dev Tauri build should use the dev channel config");
if (!packageJson.scripts["build:linux:release"].includes("--bundles deb")) fail("Linux release build should create deb bundles");
if (packageJson.scripts["build:linux:release"].includes("appimage")) fail("Linux release build should not create AppImage bundles");
if (!packageJson.scripts["build:linux:updater"].includes("create-updater-tauri-config.mjs")) fail("Linux updater deb build should inject the updater Tauri config");
if (!packageJson.scripts["build:linux:updater"].includes("src-tauri/tauri.updater.local.json")) fail("Linux updater deb build should use the generated local updater artifact config");
if (!packageJson.scripts["build:linux:updater"].includes("--bundles deb")) fail("Linux updater build should create signed deb bundles");
if (!packageJson.scripts["build:installer:nsis:updater"].includes("create-updater-tauri-config.mjs")) fail("Windows updater NSIS build should inject the updater Tauri config");
if (!packageJson.scripts["build:installer:nsis:updater"].includes("src-tauri/tauri.updater.local.json")) fail("Windows updater NSIS build should use the generated local updater artifact config");
if (!packageJson.scripts["build:macos:updater"].includes("create-updater-tauri-config.mjs")) fail("macOS updater build should inject the updater Tauri config");
if (!packageJson.scripts["build:macos:updater"].includes("--bundles app,dmg")) fail("macOS updater build should create app and dmg bundles");
if (!packageJson.scripts["release:updater-feed"].includes("create-windows-updater-feed.mjs")) fail("release:updater-feed should generate the Windows updater feed");
if (!packageJson.scripts["release:linux-updater-feed"].includes("create-linux-updater-feed.mjs")) fail("release:linux-updater-feed should merge the Linux updater feed");
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
if (tauriConfig.version !== "0.4.7") fail("stable Tauri version should be 0.4.7");
if (tauriConfig.app?.windows?.[0]?.title !== "CodexHub") fail("stable window title should be CodexHub");
if (devTauriConfig.productName !== "CodexHub Dev") fail("dev productName should be CodexHub Dev");
if (devTauriConfig.identifier !== "dev.codexhub.desktop") fail("dev identifier should be dev.codexhub.desktop");
if (devTauriConfig.version !== "0.4.7") fail("dev Tauri version should be 0.4.7");
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
for (const permission of [
  "core:window:allow-close",
  "core:window:allow-hide",
  "core:window:allow-minimize",
  "core:window:allow-start-dragging",
  "core:window:allow-title",
  "core:window:allow-toggle-maximize"
]) {
  if (!defaultCapability.permissions?.includes(permission)) fail(`missing custom titlebar window permission: ${permission}`);
}
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
const feedbackDocs = read("docs/feedback-error-handling.md");
const mvp = read("docs/mvp-scope.md");
const limitations = read("docs/known-limitations.md");
const linuxSupport = read("docs/linux-support.md");
const macosSupport = read("docs/macos-support.md");
const readme = read("README.md");
const publicScope = read("docs/public-scope.md");
const releaseChecklist = read("docs/release-checklist.md");
const stableUpdater = read("docs/stable-updater.md");
const security = read("SECURITY.md");
const zhReadme = read("docs/zh-CN/README.md");

const requiredText = [
  [readme, "CodexHub is a desktop control console"],
  [zhReadme, "通用桌面控制台，支持 Windows、macOS 和 Linux"],
  [readme, "latest stable build"],
  [readme, "CodexHub_0.4.7_aarch64.dmg"],
  [readme, "CodexHub_0.4.7_amd64.deb"],
  [readme, "CodexHub_0.4.7_arm64.deb"],
  [readme, "update checks fail"],
  [zhReadme, "检查更新失败"],
  [readme, "Settings > Codex > Connections"],
  [readme, "strictly matched Codex processes owned by the current remote SSH user"],
  [readme, "executable selected through `~/.codex/packages/standalone/current`"],
  [readme, "strictly marked managed releases"],
  [zhReadme, "身份已严格确认的远端 Codex 进程"],
  [zhReadme, "托管 target 继续跟随 `standalone/current`"],
  [zhReadme, "版本低于新版本的 `releases/<entry>`"],
  [architecture, "Remote Codex Reload Boundary"],
  [architecture, "Managed Runtime Coordination And Release Cleanup"],
  [architecture, "target, launcher, and login-shell `codex --version` values agree"],
  [architecture, "/proc/<pid>/status"],
  [architecture, "never raw process command lines"],
  [mvp, "per-apply confirmation for remote Codex process activation"],
  [mvp, "serialize CodexHub runtime writers with a current-UID/PID/starttime lock"],
  [limitations, "there is no standalone Host reload button"],
  [limitations, "Staged Update backups keep consuming the same disk space until the user explicitly deletes them after inspection"],
  [architecture, "deletion-backups/update-<UTC>-<PID>"],
  [mvp, "automatic permanent deletion of staged Update backups"],
  [limitations, "codex-original.<timestamp>.<pid>"],
  [limitations, ".codexhub-managed-capture"],
  [limitations, "GNU `mv -T -n`"],
  [limitations, "shared current-user writer lock"],
  [limitations, "raw SSH stdout/stderr and process command lines are discarded"],
  [readme, "Windows tray / macOS menu bar / Linux tray status icon"],
  [readme, "MIT"],
  [zhReadme, "Windows 托盘 / macOS 菜单栏 / Linux 托盘状态图标"],
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
  [stableUpdater, "linux-x86_64"],
  [stableUpdater, "linux-aarch64"],
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
  [architecture, "Task schema v4"],
  [architecture, "host-operation-progress"],
  [architecture, "fixed six-host sliding pool"],
  [architecture, "two disclosure levels"],
  [architecture, "hostOperationLogPopups"],
  [feedbackDocs, "does not cancel, pause, or discard"],
  [feedbackDocs, "Tasks page remains the authoritative retained history"],
  [mvp, "Mandatory remote Codex wrapper"],
  [mvp, "Window 5: profile/API config"],
  [mvp, "Window 6: single-card local skill library"],
  [mvp, "selected host's `~/.codex-hub/env`"],
  [mvp, "fixed six-host sliding concurrency pool"],
  [mvp, "Writing local credential-store key names or API key values into remote Codex config"],
  [limitations, "Profiles /"],
  [limitations, "Linux desktop support targets Ubuntu/Debian x86_64 and arm64"],
  [limitations, "direct GitHub repository URLs and GitHub"],
  [limitations, "CodexHub writes the value only to the selected host's `~/.codex-hub/env`"],
  [limitations, "CodexHub never reads or writes local ChatGPT/Codex App private files"],
  [limitations, "fixed maximum of six concurrent hosts"],
  [security, "CodexHub-managed remote `~/.codex-hub/env`"],
  [macosSupport, "Menu bar/status item restore behavior"],
  [macosSupport, "Real Mac Validation Status"],
  [macosSupport, "real-device validation baseline was completed"],
  [macosSupport, "APPLE_SIGNING_IDENTITY=-"],
  [macosSupport, "~/.ssh/config"],
  [linuxSupport, "Ubuntu/Debian x86_64"],
  [linuxSupport, "Ubuntu/Debian arm64"],
  [linuxSupport, "CodexHub_<version>_amd64.deb"],
  [linuxSupport, "CodexHub_<version>_arm64.deb"],
  [linuxSupport, "macOS-style appearance"]
];

for (const [content, phrase] of requiredText) {
  if (!content.includes(phrase)) fail(`missing required phrase: ${phrase}`);
}

if (/\bWindows-first\b/i.test(readme)) {
  fail("README should describe CodexHub as a Windows, macOS, and Linux desktop console, not Windows-first");
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
for (const token of ["PRIVATE KEY", "sk-[A-Za-z0-9_-]{20,}", "release-artifacts", "personal repository or user identifier", "local home directory", "(?:windows|macos|linux)-updater", "PUBLIC AUDIT PASS"]) {
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

const rustBackend = [
  "src-tauri/src/lib.rs",
  "src-tauri/src/app_runtime.rs",
  "src-tauri/src/contracts.rs",
  "src-tauri/src/domain.rs",
  "src-tauri/src/jobs.rs",
  "src-tauri/src/hosts.rs",
  "src-tauri/src/profiles.rs",
  "src-tauri/src/resource_monitor.rs",
  "src-tauri/src/settings.rs",
  "src-tauri/src/skills.rs",
  "src-tauri/src/tasks.rs",
  "src-tauri/src/updater.rs",
  "src-tauri/src/commands/settings.rs",
  "src-tauri/src/commands/host_ssh.rs",
  "src-tauri/src/commands/profiles.rs",
  "src-tauri/src/commands/skills.rs",
  "src-tauri/src/commands/storage.rs",
  "src-tauri/src/commands/tasks.rs",
  "src-tauri/src/commands/updater.rs",
  "src-tauri/src/adapters/credentials.rs",
  "src-tauri/src/adapters/events.rs",
  "src-tauri/src/services/codex_runtime.rs",
  "src-tauri/src/services/profile_links.rs",
  "src-tauri/src/services/host_operations.rs",
  "src-tauri/src/services/host_use_cases.rs",
  "src-tauri/src/services/profile_catalog.rs",
  "src-tauri/src/services/profile_operations.rs",
  "src-tauri/src/services/profile_use_cases.rs",
  "src-tauri/src/services/skill_operations.rs",
  "src-tauri/src/services/skill_use_cases.rs",
  "src-tauri/src/services/storage_operations.rs",
  "src-tauri/src/services/updater_operations.rs",
  "src-tauri/src/storage/json_store.rs",
  "src-tauri/src/storage/task_store.rs",
  "src-tauri/src/storage/transaction.rs"
].map(read).join("\n");
const rustHostUseCases = read("src-tauri/src/services/host_use_cases.rs");
const rustHostOperations = read("src-tauri/src/services/host_operations.rs");
const rustSkillOperations = read("src-tauri/src/services/skill_operations.rs");
const rustProfileOperations = read("src-tauri/src/services/profile_operations.rs");
const rustCodexRuntime = read("src-tauri/src/services/codex_runtime.rs");
const sshRs = read("src-tauri/src/ssh.rs");
const rustPlatform = read("src-tauri/src/platform.rs");
const tsPlatform = read("src/platform.ts");
const ciWorkflow = read(".github/workflows/ci.yml");
const macosWorkflow = read(".github/workflows/build-macos-release.yml");
const linuxWorkflow = read(".github/workflows/build-linux-release.yml");
const windowsWorkflow = read(".github/workflows/build-windows-release.yml");
const updaterConfigScript = read("scripts/create-updater-tauri-config.mjs");
const windowsUpdaterFeedScript = read("scripts/create-windows-updater-feed.mjs");
const linuxUpdaterFeedScript = read("scripts/create-linux-updater-feed.mjs");
const macosUpdaterFeedScript = read("scripts/create-macos-updater-feed.mjs");
for (const token of ["sidebar_completion_indicators", "sidebar_completion_indicators: true", "#[serde(default = \"default_true\")]"]) {
  if (!rustBackend.includes(token)) fail(`missing sidebar completion settings Rust token: ${token}`);
}
if (!/#\[serde\(default = "default_true"\)\]\s*pub\(crate\) host_operation_log_popups: bool/u.test(rustBackend)) {
  fail("host-operation log pop-up setting must default to enabled when legacy settings omit it");
}
if (!rustBackend.includes("host_operation_log_popups: true")) {
  fail("default Rust settings must enable host-operation log pop-ups");
}
for (const token of [
  "CODEX_NATIVE_PLATFORM_SCRIPT",
  "npm-mirror-native-local-upload",
  "parse_npmmirror_native_metadata",
  "run_ssh_script_streaming",
  "validate_local_codex_native_package",
  "validate_codex_native_archive_listing",
  "package/vendor/{target}/bin/codex",
  "return validation_output.unwrap_or(download_output)"
]) {
  if (!rustBackend.includes(token)) fail(`missing local upload Codex fallback token: ${token}`);
}
for (const token of [
  "CURRENT_TASK_SCHEMA_VERSION: i64 = 4",
  "CREATE TABLE IF NOT EXISTS task_steps",
  "ALTER TABLE task_logs ADD COLUMN step_id TEXT",
  "backup_before_schema_upgrade",
  "VACUUM INTO",
  "TaskStepStatus",
  "pub(crate) steps: Vec<TaskStep>",
  "pub(crate) step_id: Option<String>"
]) {
  if (!rustBackend.includes(token)) fail(`missing task-step schema v4 token: ${token}`);
}
for (const token of [
  "pub(crate) fn task_log_id",
  "Task payload invariant violation",
  "validate_task_payload",
  "fail_running_task",
  "is_payload_invariant_error"
]) {
  if (!rustBackend.includes(token)) fail(`missing durable task-log identity guard: ${token}`);
}
if (rustBackend.includes('id: format!("{task_id}-log')) {
  fail("Rust task-log constructors must use the shared task_log_id allocator");
}
for (const token of [
  "HostOperationProgressEvent",
  "host-operation-progress",
  "persist_step_update",
  "pub(crate) const HOST_OPERATION_MAX_CONCURRENCY: usize = 6;",
  "run_official_codex_installer",
  "run_remote_native_mirror_install",
  "run_remote_npm_mirror_install",
  "run_local_upload_codex_fallback",
  "CODEX_OFFICIAL_INSTALL_SCRIPT",
  "CODEX_REMOTE_NATIVE_MIRROR_SCRIPT",
  "CODEX_REMOTE_NPM_MIRROR_SCRIPT"
]) {
  if (!rustBackend.includes(token)) fail(`missing structured host-operation token: ${token}`);
}
for (const token of [
  "std::collections::VecDeque",
  "worker_count = item_count.min(HOST_OPERATION_MAX_CONCURRENCY)",
  ".pop_front()",
  "[index] = Some(output)"
]) {
  if (!rustBackend.includes(token)) fail(`batch host operations must use the six-worker sliding pool: ${token}`);
}
const parallelLatestHelper = rustHostOperations.match(
  /fn run_with_parallel_latest[\s\S]*?std::thread::scope\(\|scope\| \{[\s\S]*?let latest = scope\.spawn\(latest\);[\s\S]*?let batch_result = batch\(\);[\s\S]*?latest[\s\S]*?\.join\(\)/
);
if (!parallelLatestHelper) {
  fail("batch host tests must start exactly one latest-version worker before running the host pool");
}
if (!/run_with_parallel_latest\(\s*\|\| run_refresh_latest_codex_version\(state, true, timeout_ms\),\s*\|\| run_probe_batch_items/.test(rustHostOperations)) {
  fail("batch host tests must run the single latest-version query in parallel with the bounded host pool");
}
for (const stepId of [
  "ssh-check",
  "system",
  "codex",
  "api",
  "skills",
  "preparation",
  "official-installer",
  "remote-native-mirror",
  "remote-npm-mirror",
  "local-upload",
  "final-verification",
  "uninstall"
]) {
  if (!rustBackend.includes(`"${stepId}"`)) fail(`missing stable host-operation stepId: ${stepId}`);
}
const codexStreamEmitter = rustHostOperations.match(
  /pub\(crate\) fn emit_remote_codex_stream_event[\s\S]*?pub\(crate\) fn emit_remote_codex_progress_for_output/
)?.[0] ?? "";
if (!codexStreamEmitter.includes("ProcessStreamKind::Heartbeat")) {
  fail("remote Codex streaming must retain transient heartbeat events");
}
for (const forbidden of ["persist_step_update", "append_message", "task_store.upsert"]) {
  if (codexStreamEmitter.includes(forbidden)) fail(`heartbeat/stream events must not be persisted: ${forbidden}`);
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
  "batch_remote_probe_codex",
  "sample_host_resources",
  "remote_manage_codex",
  "batch_remote_update_codex",
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
  if (!rustBackend.includes(command)) fail(`missing ${command} Tauri command`);
}
for (const token of ["AppUpdateStatus", "AppUpdateState", "CODEXHUB_STABLE_UPDATE_ENDPOINT", "CODEXHUB_STABLE_UPDATER_PUBKEY", "tauri_plugin_updater::Builder::new().build()", "UpdaterExt", "stable_updater_configured", "normalize_updater_pubkey", "extract_minisign_public_key"]) {
  if (!rustBackend.includes(token)) fail(`missing stable updater backend token: ${token}`);
}
if (!cargoToml.includes('tauri-plugin-updater = { version = "2", default-features = false, features = ["native-tls", "zip"] }')) {
  fail("stable updater must use native TLS so release checks can use the OS trust store");
}
if (!cargoToml.includes('reqwest = { version = "0.13", default-features = false, features = ["json", "native-tls"] }')) {
  fail("stable updater GitHub feed resolver must use reqwest with native TLS");
}
if (!cargoToml.includes('keyring = { version = "3", features = ["apple-native", "windows-native", "linux-native-sync-persistent"] }')) {
  fail("keyring must keep Windows/macOS backends and enable Linux persistent credential storage");
}
for (const token of ["stable_update_endpoints", "resolve_github_latest_json_asset_endpoint", "github_release_api_url", "OCTET_STREAM_ACCEPT", "api.github.com/repos", "stable_update_network_routes", "LOCAL_PROXY_PORTS", "NetworkProxyMode", "detect_network_proxy_status", "builder.proxy(proxy)"]) {
  if (!rustBackend.includes(token)) fail(`missing GitHub updater feed fallback token: ${token}`);
}
for (const token of ["mod resource_monitor", "sample_host_resources", "resource_monitor::sample_host_resources_with_progress", "HostResourceProgressEvent", "host-resource-progress", "RESOURCE_SAMPLE_CONCURRENCY", "query-compute-apps", "CH_GPU_PROCESS", "etimes"]) {
  if (!rustBackend.includes(token)) fail(`missing resource monitor backend token: ${token}`);
}
for (const token of ["app_update_check_task", "app_update_install_task", "app_update_state_label", "record_task(&state, app_update_check_task(running, &status, &attempts))", "Install app update", "Check app update"]) {
  if (!rustBackend.includes(token)) fail(`missing stable updater task token: ${token}`);
}
for (const token of ["install_stable_update", "download_and_install", "AppUpdateState::Installing", "channel != \"stable\"", "stable_updater_configured(&config)", "updater_error_message"]) {
  if (!rustBackend.includes(token)) fail(`missing gated stable updater install token: ${token}`);
}
if (rustBackend.includes("export_profiles")) fail("Profiles export command should be removed");
for (const removedCommand of [
  removed("search", "_online", "_skills"),
  removed("clone", "_skill", "_repo"),
  removed("list", "_remote", "_skills"),
  removed("preview", "_remote", "_skill", "_install"),
  removed("install", "_remote", "_skill", "_batch"),
  removed("delete", "_remote", "_skill")
]) {
  if (rustBackend.includes(removedCommand)) fail(`removed public Skills command should not remain: ${removedCommand}`);
}
const listHostsMatch = rustHostUseCases.match(/fn execute_list_hosts[\s\S]*?\n}\r?\n\r?\npub\(crate\) fn execute_refresh_discovered_hosts/);
if (!listHostsMatch) fail("could not locate list_hosts function boundary");
if (listHostsMatch[0].includes("merge_discovered_hosts")) fail("list_hosts must not auto-import local SSH config");
for (const asyncCommand of [
  "async fn get_ssh_status",
  "async fn list_ssh_config_hosts",
  "async fn ssh_check",
  "async fn remote_probe_codex",
  "async fn batch_remote_probe_codex",
  "async fn remote_manage_codex",
  "async fn batch_remote_update_codex",
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
  if (!rustBackend.includes(asyncCommand)) fail(`long remote command must stay async: ${asyncCommand}`);
}
if (!rustBackend.includes("spawn_blocking(command)")) fail("long remote commands should run through the blocking worker pool");
for (const token of [
  "tauri_plugin_dialog::init()",
  "SkillImportResult",
  "SkillInventoryStatus",
  "SkillDetectionResult",
  "SkillTargetsResult",
  "SkillTargetOperationResult",
  "RemoteSkillListResult",
  "managed_skills_dir",
  'storage::load_cache_document(&state.paths, "skills-inventory.json")',
  'storage::save_cache_document(&state.paths, "skills-inventory.json", status)',
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
  if (!rustBackend.includes(token)) fail(`missing Window 6 Skills backend token: ${token}`);
}
for (const token of ["Latest scan returned no skills; kept previous cached", "previous_inventory", "previous.skills"]) {
  if (!rustBackend.includes(token)) fail(`missing installed skill inventory empty-scan guard: ${token}`);
}
if (rustBackend.includes('find "$HOME/.codex/skills" -mindepth 1 -maxdepth 1')) {
  fail("remote skill probes must not count only first-level ~/.codex/skills directories");
}
const getSkillTargetsMatch = rustSkillOperations.match(/fn run_get_skill_targets[\s\S]*?\n}\r?\n\r?\npub\(crate\) fn run_install_skill_targets/);
if (!getSkillTargetsMatch) fail("could not locate run_get_skill_targets function boundary");
for (const liveProbeToken of ["run_remote_skill_install_preview", "run_remote_skill_list"]) {
  if (getSkillTargetsMatch[0].includes(liveProbeToken)) {
    fail(`get_skill_targets must use cached inventory instead of live remote probing: ${liveProbeToken}`);
  }
}
for (const token of ["LatestCodexVersion", "parse_npm_latest_metadata", "latest_codex_cache_is_fresh", "CODEX_LATEST_REFRESH_HOUR", "https://registry.npmjs.org/@openai/codex", "codex-latest.json"]) {
  if (!rustBackend.includes(token)) fail(`missing latest Codex version backend token: ${token}`);
}
for (const token of ["hosts.json", "load_hosts", "save_hosts", "save_current_hosts"]) {
  if (!rustBackend.includes(token)) fail(`missing host persistence token: ${token}`);
}
for (const token of ["setup_guide_dismissed", "#[serde(default)]"]) {
  if (!rustBackend.includes(token)) fail(`missing setup guide settings backend token: ${token}`);
}
for (const token of ["platform_appearance", "PlatformAppearance", "default_platform_appearance"]) {
  if (!rustBackend.includes(token)) fail(`missing platform appearance backend token: ${token}`);
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
  if (!rustBackend.includes(token)) fail(`missing close-to-tray backend token: ${token}`);
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
  "name: CI",
  "push:",
  "pull_request:",
  "branches:",
  "- master",
  "runs-on: ubuntu-22.04",
  "pnpm smoke",
  "pnpm smoke:mock",
  "pnpm typecheck",
  "pnpm build:web",
  "cargo test --manifest-path src-tauri/Cargo.toml",
  "concurrency:"
]) {
  if (!ciWorkflow.includes(token)) fail(`missing CI workflow token: ${token}`);
}
for (const forbiddenCiToken of [
  "pnpm build:linux:release",
  "pnpm build:macos:release",
  "pnpm build:installer:nsis:updater",
  "gh release upload",
  "upload_to_release"
]) {
  if (ciWorkflow.includes(forbiddenCiToken)) fail(`CI workflow must not package or upload release assets: ${forbiddenCiToken}`);
}
for (const [name, workflow] of [
  ["macOS", macosWorkflow],
  ["Linux", linuxWorkflow],
  ["Windows", windowsWorkflow]
]) {
  if (workflow.includes("\n  push:")) fail(`${name} release workflow must be manual-only and not run on push`);
  if (workflow.includes("\n  pull_request:")) fail(`${name} release workflow must be manual-only and not run on pull_request`);
  if (!workflow.includes("workflow_dispatch:")) fail(`${name} release workflow must keep manual workflow_dispatch`);
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
  "CodexHub_${VERSION}_aarch64.dmg#CodexHub_${VERSION}_aarch64.dmg",
  "#$(basename \"$updaterArchive\")",
  "latest.json#latest.json",
  "SHA256SUMS.txt#SHA256SUMS.txt"
]) {
  if (!macosWorkflow.includes(token)) fail(`missing macOS asset filename label token: ${token}`);
}
for (const forbiddenWorkflowToken of [
  "macOS Apple Silicon DMG unsigned",
  "macOS Apple Silicon updater archive unsigned",
  "stable updater feed",
  "SHA256 checksums"
]) {
  if (macosWorkflow.includes(forbiddenWorkflowToken)) fail(`macOS release asset label must match the file name: ${forbiddenWorkflowToken}`);
}
for (const token of [
  "runs-on: ${{ matrix.runner }}",
  "ubuntu-22.04-arm",
  "deb_arch: amd64",
  "deb_arch: arm64",
  "libwebkit2gtk-4.1-dev",
  "libayatana-appindicator3-dev",
  "libxdo-dev",
  "libkeyutils-dev",
  "actions/upload-artifact@v4",
  "actions/download-artifact@v4",
  "pnpm typecheck",
  "pnpm build:web",
  "cargo test --manifest-path src-tauri/Cargo.toml",
  "pnpm build:linux:release",
  "pnpm build:linux:updater",
  "pnpm release:linux-updater-feed",
  "src-tauri/target/release/bundle/deb/*.deb.sig",
  "upload_to_release == 'true'",
  "CODEXHUB_STABLE_UPDATER_PUBKEY: ${{ vars.CODEXHUB_STABLE_UPDATER_PUBKEY }}",
  "TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}",
  "gh release upload",
  "CodexHub_${VERSION}_amd64.deb#CodexHub_${VERSION}_amd64.deb",
  "CodexHub_${VERSION}_arm64.deb#CodexHub_${VERSION}_arm64.deb",
  "latest.json#latest.json",
  "SHA256SUMS.txt#SHA256SUMS.txt"
]) {
  if (!linuxWorkflow.includes(token)) fail(`missing Linux workflow token: ${token}`);
}
for (const forbiddenLinuxWorkflowToken of [
  "appimage",
  "_amd64.AppImage#",
  ".deb.sig#"
]) {
  if (linuxWorkflow.includes(forbiddenLinuxWorkflowToken)) fail(`Linux updater workflow must not include token: ${forbiddenLinuxWorkflowToken}`);
}
for (const forbiddenWorkflowToken of ["softprops/action-gh-release", "gh release create", "tauri-action@v0"]) {
  if (linuxWorkflow.includes(forbiddenWorkflowToken)) fail(`Linux release workflow must not create a GitHub Release: ${forbiddenWorkflowToken}`);
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
  "CodexHub_$($env:VERSION)_x64-setup.exe#CodexHub_$($env:VERSION)_x64-setup.exe",
  "latest.json#latest.json",
  "SHA256SUMS.txt#SHA256SUMS.txt"
]) {
  if (!windowsWorkflow.includes(token)) fail(`missing Windows asset filename label token: ${token}`);
}
for (const forbiddenWorkflowToken of [
  "Windows x64 Setup",
  "stable updater feed",
  "SHA256 checksums"
]) {
  if (windowsWorkflow.includes(forbiddenWorkflowToken)) fail(`Windows release asset label must match the file name: ${forbiddenWorkflowToken}`);
}
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
  "linux-x86_64",
  "linux-aarch64",
  "CodexHub_${version}_${target.debArch}.deb",
  "const signaturePath = `${debPath}.sig`",
  "existing release feed",
  "SHA256SUMS.txt",
  "CODEXHUB_RELEASE_TAG",
  "https://github.com/${repo}/releases/download/${normalizedTag}/${debName}",
  "...platformEntries"
]) {
  if (!linuxUpdaterFeedScript.includes(token)) fail(`missing Linux updater feed script token: ${token}`);
}
for (const forbiddenLinuxFeedToken of ["AppImage"]) {
  if (linuxUpdaterFeedScript.includes(forbiddenLinuxFeedToken)) fail(`Linux updater feed script must not include token: ${forbiddenLinuxFeedToken}`);
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
  "apply_batch_with_metadata",
  "prepare_profiles_import",
  "find_cc_switch_api_key_for_profile",
  "is_missing_credential_error"
]) {
  if (!rustBackend.includes(token)) fail(`missing cc-switch adapter token: ${token}`);
}
for (const token of ["profiles: Vec<Profile>", "hosts: Vec<Host>", "services::profile_links::save", "sync_profile_host_ids", "clear_profile_host_ids", "reconcile_hosts_with_profile_links", "RemoteApiConfigMatch"]) {
  if (!rustBackend.includes(token)) fail(`missing profile apply refreshed-state token: ${token}`);
}
const reconcileHostsMatch = rustProfileOperations.match(/fn reconcile_hosts_with_profile_links[\s\S]*?\n}\r?\n\r?\npub\(crate\) fn record_task/);
if (!reconcileHostsMatch) fail("missing reconcile_hosts_with_profile_links function body");
if (reconcileHostsMatch[0].includes("host.config_exists = Some(true)") || reconcileHostsMatch[0].includes("host.api_config_name = Some(profile.name.clone())")) {
  fail("profile host-link reconcile must not promote local links into confirmed remote API config facts");
}
for (const token of ["api_config_name", "api_config_source", "classify_remote_api_config", "normalize_base_url_key", "api_probe_group_script", "CODEXHUB_API_BASE_URL"]) {
  if (!rustBackend.includes(token)) fail(`missing remote API config probe token: ${token}`);
}
for (const token of [
  "codex_command_available",
  "CODEX_COMMAND_AVAILABLE_SCRIPT",
  "codex_probe_group_script",
  "check codex command in current shell",
  "check codex command in login shell",
  "api_probe_group_script",
  "CODEXHUB_API_ENV_PRESENT",
  "api_key_env_present",
  "check_profile_api_env",
  "configure_profile_remote_api_key",
  "remote_profile_api_key_script",
  "$HOME/.codex-hub/env",
  "CodexHub managed launcher",
  "CODEXHUB_REMOTE_ENV_CHANGED",
  "CODEXHUB_RUNTIME_LAUNCHER_CHANGED",
  "reconcile_after_successful_remote_env_write",
  "shell_single_quote(&shell_single_quote(api_key))"
]) {
  if (!rustBackend.includes(token)) fail(`missing remote readiness probe token: ${token}`);
}
for (const token of [
  "RemoteCodexReloadMode",
  "RemoteCodexReloadStatus",
  "RemoteCodexReloadResult",
  "ProfileApplyOptions",
  "ProfileApplyOutcome",
  "remote_codex_reload_script",
  "parse_remote_codex_reload_result",
  "remote_codex_reload_log_message",
  '"profile-apply"',
  '"remote-codex-reload"',
  '"runtime-reconcile"',
  '"release-cleanup"'
]) {
  if (!rustBackend.includes(token)) fail(`missing remote Codex reload contract token: ${token}`);
}
const profileApplyHostFlow = rustProfileOperations.match(
  /fn apply_profile_to_host[\s\S]*?\n}\r?\n\r?\nfn finish_failed_profile_apply/
)?.[0] ?? "";
if (!profileApplyHostFlow) fail("missing single-host profile apply flow");
const profileApplyOrder = [
  "let remote_env_configured",
  "let api_key_env_present",
  "let local_persist_error",
  "let reload = if remote_apply_ready"
].map((token) => profileApplyHostFlow.indexOf(token));
if (profileApplyOrder.some((index) => index < 0) || profileApplyOrder.some((index, position) => position > 0 && index <= profileApplyOrder[position - 1])) {
  fail("profile apply must verify env, persist confirmed state, then reload remote Codex processes");
}
const commitScriptSource = rustProfileOperations.match(
  /pub\(crate\) fn profile_apply_commit_script[\s\S]*?\n}\r?\n\r?\npub\(crate\) fn profile_apply_metadata_script/
)?.[0] ?? "";
const metadataScriptSource = rustProfileOperations.match(
  /pub\(crate\) fn profile_apply_metadata_script[\s\S]*?\n}\r?\n\r?\n\/\/ Reloads only identities/
)?.[0] ?? "";
for (const [name, source] of [["commit", commitScriptSource], ["metadata", metadataScriptSource]]) {
  if (!source) fail(`missing profile ${name} script source`);
  for (const forbidden of ["safe_reconnect", "CODEXHUB_RELOAD", "kill -TERM"]) {
    if (source.includes(forbidden)) fail(`profile ${name} script must not embed process reload logic: ${forbidden}`);
  }
}
const reloadScriptSource = rustProfileOperations.match(
  /pub\(crate\) fn remote_codex_reload_script[\s\S]*?\n}\r?\n\r?\npub\(crate\) fn parse_remote_codex_reload_result/
)?.[0] ?? "";
if (!reloadScriptSource) fail("missing isolated remote Codex reload script source");
for (const token of [
  "for proc_dir in /proc/[0-9]*",
  "proc_uid=$(awk '/^Uid:/",
  "proc_start=$(sed 's/^[^)]*) //'",
  "codex:codex:app-server",
  "codex:codex:remote-control",
  "preserved_cli=$((preserved_cli + 1))",
  "[ \"$proc_start\" = \"$expected_start\" ]",
  "kill -TERM \"$pid\"",
  "[ \"$elapsed\" -lt 5 ]",
  "[ \"$elapsed\" -lt 15 ]",
  "manual-required old-process-still-running",
  "manual-required unverified-process",
  "reconnected replacement-observed"
]) {
  if (!reloadScriptSource.includes(token)) fail(`remote reload safety path is missing: ${token}`);
}
for (const forbidden of [
  "pkill",
  "killall",
  "SIGKILL",
  "kill -KILL",
  "kill -9",
  "kill -- -",
  "kill -TERM -",
  "kill -TERM 0",
  "[[",
  "]]",
  "<(",
  ">(",
  "declare -a",
  "function "
]) {
  if (reloadScriptSource.includes(forbidden)) fail(`remote reload script contains forbidden shell/process token: ${forbidden}`);
}
for (const line of reloadScriptSource.split(/\r?\n/u).filter((line) => line.includes("printf"))) {
  if (/proc_(?:argv|arg1|arg2|comm)/u.test(line)) fail("remote reload protocol must not print a process command line");
}
const reloadLoggingSource = rustProfileOperations.match(
  /pub\(crate\) fn reload_remote_codex_processes[\s\S]*?\n}\r?\n\r?\npub\(crate\) fn remote_codex_reload_log_message/
)?.[0] ?? "";
if (!reloadLoggingSource.includes("logs.push(basic_log(")) fail("remote reload logs must persist only a structured local summary");
if (reloadLoggingSource.includes("command_log(")) fail("remote reload logs must not persist raw SSH command/stdout/stderr");
const writerLockStart = rustCodexRuntime.indexOf("pub(crate) const REMOTE_CODEX_RUNTIME_WRITER_LOCK_PRELUDE");
const runtimeScriptStart = rustCodexRuntime.indexOf("pub(crate) const REMOTE_CODEX_RUNTIME_RECONCILE_SCRIPT");
const cleanupScriptStart = rustCodexRuntime.indexOf("pub(crate) const REMOTE_CODEX_RELEASE_CLEANUP_SCRIPT");
const cleanupFunctionStart = rustCodexRuntime.indexOf("pub(crate) fn remote_codex_release_cleanup_script");
if (writerLockStart < 0 || runtimeScriptStart <= writerLockStart || cleanupScriptStart <= runtimeScriptStart || cleanupFunctionStart <= cleanupScriptStart) {
  fail("missing isolated managed-runtime reconcile/cleanup scripts");
}
const writerLockSource = rustCodexRuntime.slice(writerLockStart, runtimeScriptStart);
const runtimeReconcileScriptSource = rustCodexRuntime.slice(runtimeScriptStart, cleanupScriptStart);
const releaseCleanupScriptSource = rustCodexRuntime.slice(cleanupScriptStart, cleanupFunctionStart);
for (const token of [
  'codexhub_runtime_lock_path="$codexhub_runtime_lock_root/.codexhub-runtime-cleanup.lock"',
  "codexhub_runtime_process_identity()",
  'ln "$codexhub_runtime_lock_candidate" "$codexhub_runtime_lock_path"',
  "codexhub_runtime_capture_locked_state()",
  "codexhub_locked_runtime_floor",
  "codexhub_runtime_verify_post_mutation_floor()",
  'mv -T -n "$codexhub_runtime_lock_path" "$stale_lock"',
  "trap 'exit 143' TERM"
]) {
  if (!writerLockSource.includes(token)) fail(`managed runtime writer lock path is missing: ${token}`);
}
for (const token of [
  "select_verified_current_binary()",
  "for current_relative in bin/codex codex; do",
  '[ "$current_match_count" -eq 1 ] || return 1',
  'standalone_current="$current_link/$saved_current_relative"',
  'normalized_target_file_value="$standalone_current"',
  'mark_verified_release "$legacy_release_dir" "$legacy_release_version"',
  'capture_name="codex-original.$timestamp.$$"',
  'capture_marker_suffix=".codexhub-managed-capture"',
  'verify_managed_capture "$capture_path" "$capture_name"',
  "restore_exact_current_floor()",
  "trap runtime_signal_failure HUP INT TERM",
  'source_moved=yes\n  if ! mv "$launcher" "$capture_path"',
  'target_changed=yes\n  if ! mv "$target_tmp" "$target_file"',
  'launcher_changed=yes\n  if ! mv "$launcher_tmp" "$launcher"',
  'launcher_version=$(normalized_version_for_path "$launcher"',
  "login_version=$(login_shell_version",
  'target_version=$(normalized_version_for_path "$candidate"',
  "runtime-version-mismatch",
  "selected-target-would-downgrade",
  "runtime-version-below-operation-start"
]) {
  if (!runtimeReconcileScriptSource.includes(token)) fail(`managed runtime anti-regression path is missing: ${token}`);
}
for (const token of [
  'marker_name=".codexhub-managed-release"',
  'cleanup_policy=${codexhub_cleanup_policy:-managed-only}',
  "CODEXHUB_CLEANUP_ADOPTED=%s",
  'capture_marker_suffix=".codexhub-managed-capture"',
  'for candidate in "$release_root"/*',
  'verify_release_identity "$candidate"',
  'managed_marker_valid "$candidate" "$candidate_version"',
  'version_is_strictly_lower "$candidate_version" "$cleanup_verified_version"',
  'adopt_verified_release "$candidate" "$candidate_real" "$candidate_name" "$candidate_version"',
  'ln "$marker_tmp" "$marker"',
  '"$candidate_real" = "$protected_current"',
  '"$candidate_real" = "$protected_target"',
  "scan_current_uid_executables",
  'release_in_use_now "$candidate_real"',
  'quarantine="$release_root/.codexhub-quarantine.$candidate_name.$$"',
  'mv -T -n "$candidate" "$quarantine"',
  'release_in_use_now "$isolated_real"',
  'rm -rf "$quarantine"',
  'for candidate in "$hub_dir"/codex-original.*',
  'verify_capture_candidate "$candidate"',
  '"$candidate_real" = "$protected_capture"',
  'capture_in_use_now "$isolated_real"',
  'rm -f "$capture_marker"',
  'cleanup_lock="$cleanup_lock_root/.codexhub-runtime-cleanup.lock"',
  "verify_owned_cleanup_lock",
  "proc_starttime_after",
  "proc_exe_after",
  "trap 'trap - EXIT; cleanup_work_dir; exit 143' TERM"
]) {
  if (!releaseCleanupScriptSource.includes(token)) fail(`managed release cleanup safety path is missing: ${token}`);
}
for (const forbidden of [
  "find ",
  "pkill",
  "killall",
  "SIGKILL",
  'mv "$candidate" "$quarantine"',
  'find "$release_root"',
  'rm -rf "$release_root"',
  'rm -rf "$candidate"'
]) {
  if (releaseCleanupScriptSource.includes(forbidden)) fail(`managed release cleanup contains forbidden broad action: ${forbidden}`);
}
for (const token of [
  "CodexReleaseCleanupPolicy::ManagedOnly",
  "CodexReleaseCleanupPolicy::VerifiedOlderThan",
  "remote_codex_release_cleanup_script_with_policy"
]) {
  if (!rustCodexRuntime.includes(token)) fail(`missing release cleanup policy contract: ${token}`);
}
if (!rustHostOperations.includes("action == RemoteCodexAction::Update") ||
    !rustHostOperations.includes("CodexReleaseCleanupPolicy::VerifiedOlderThan")) {
  fail("Update must select verified-old-release adoption only after final verification");
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
  if (rustBackend.includes(token)) fail(`default mock data should not include ${token}`);
}

const app = read("src/App.tsx");
const mockApiSource = read("src/api/mock.ts");
const modalFrameSource = read("src/ui/ModalFrame.tsx");
const operationProgressSource = read("src/ui/OperationProgress.tsx");
const alertModalFrameSource = read("src/ui/AlertModalFrame.tsx");
for (const token of ["MonitorView", "MonitorHostCard", "MonitorHostStatusIndicator", "CircularStatusIndicator", "HostStatusIndicator", "resolveMonitorHostIndicatorState", "resolveHostStatusIndicatorState", "applyRemoteProbeResultToHosts", "applyRemoteProbeBatchResultsToHosts", "resourcePendingHostAliases", "monitorBentoGrid", "ResizeObserver", "resourceMonitorAutoRefresh", "resourceMonitorRefreshSeconds", "resourceMonitorHostOrder", "monitorAutoRefreshControl", "pillToggle", "monitorDragHandle", "monitorSegmentedMeter", "aggregateGpuProcessUsers", "elapsedSeconds", "copy.monitor.refreshNow", "copy.monitor.autoRefresh", "copy.monitor.gpuProcesses", "sampleHostResources", "mergeHostResourceSnapshot", "resource-monitor-${Date.now()}", "监控", "onPointerDown", "previewMonitorHostOrder", "monitorDragGhost", "data-placeholder", "requestAnimationFrame", "stopMonitorAutoScroll", "MonitorMeterTone", "host.hostAlias", "formatCpuLoadSummary", "pendingReorderTimerRef", "monitorCpuPercent", "summarizeGpuMemory", 'aria-label={label}', 'role="img"', 'stroke="currentColor"']) {
  if (!app.includes(token)) fail(`missing resource monitor UI token: ${token}`);
}
if (!/<th className="sshHostsAliasCol">[\s\S]*?<th className="sshHostsOnlineCol">[\s\S]*?<th className="sshHostsSourceCol">/.test(app)) {
  fail("Host list columns should keep Alias -> Online -> Source order");
}
if ((app.match(/<HostStatusIndicator/g) ?? []).length < 3) fail("Host list, matrix, and details should share the circular Host status indicator");
const hostDetailsSource = app.slice(app.indexOf("function HostDetailsPanel"), app.indexOf("function profileApplyEligibleHostIds"));
if (!hostDetailsSource.includes("copy.hosts.source") || !hostDetailsSource.includes("hostSourceLabel(copy, host)")) fail("Host details should show Source");
if (hostDetailsSource.includes("copy.hosts.shell")) fail("Host details should not show the Shell row");
for (const token of ["persist_host_check", "save_hosts_state", "The offline Host status failed to persist"]) {
  if (!rustBackend.includes(token)) fail(`missing persisted Host connectivity token: ${token}`);
}
if (app.includes("resourceStatusTone")) fail("resource monitor host headers should use icon indicators instead of text badges");
if (app.includes("copy.monitor.noGpuProcesses")) fail("resource monitor should hide empty GPU process text in host cards");
for (const token of ["MonitorHostRow", "monitorTable"]) {
  if (app.includes(token)) fail(`resource monitor should use Bento cards instead of table UI: ${token}`);
}
for (const label of ["Home", "主页", "Hosts", "Profiles", "Skills", "Tasks", "任务", "Settings", "Host Matrix", "主机矩阵", "Font", "Sidebar visual hints", "侧边栏视觉提示", "Host list", "主机列表", "Local config", "本地配置", "Appearance", "外观", "Local keys", "本地密钥", "Version info", "版本信息", "Other", "其他", "Program close button behavior", "程序关闭按钮行为", "Host IP", "Codex版本", "Test all", "一键测试", "Update outdated", "一键更新", "Details", "详情", "Logs", "日志", "Copied!", "复制成功！", "Add Server", "添加服务器", "来源", "System", "系统", "Codex", "API config", "API 配置", "Test latency", "测试延迟", "stdout", "stderr", "Install Codex", "Update Codex", "新增 SSH Host", "连接进程", "BootstrapProgressLog", "Ask next time", "Exit app", "Minimize to tray", "关闭按钮", "下次询问", "退出程序", "最小化到托盘"]) {
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
for (const token of ["appUpdateStatus", "appUpdateFailureTask", "appUpdateChecking", "appUpdateInstalling", "copy.settings.appUpdates", "copy.settings.dailyUpdateCheck", "copy.settings.softwareName", "copy.settings.installedAt", "copy.settings.updatedAt", "copy.settings.checkStableUpdate", "copy.settings.installStableUpdate", "copy.settings.checkFailed", "copy.settings.updateCheckFailureHint", "copy.settings.pendingConfiguration", 'className="sshHostsTable versionInfoTable"', "appUpdateStatus.softwareName", "appUpdateStatus.installedAt ?? copy.settings.unknown", "appVersionTone(appUpdateStatus.currentVersion, appUpdateStatus.latestVersion)", "appUpdateLatestVersionLabel(appUpdateStatus, copy)", "appLatestVersionTone(appUpdateStatus)", "title={appUpdateStatus.message}", "appUpdateStatus.checkedAt ?? copy.settings.notChecked", "latestAppUpdateTask", "latestAppInstallTask", "footer={("]) {
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
for (const token of ["type NavIconId = SectionId", "type PlatformIconId", "type TitleBarAction", "function AppTitleBar(", "startDragging()", 'data-action="minimize"', 'data-action="maximize"', 'data-action="close"', "function PlatformIcon(", "function ModalFrame(", "function ModalHeader(", "function ModalActions(", "function NavIcon(", 'className="navIcon"', 'className="navGlyph"', "<NavIcon id={item.id}", "function CommandBar(", "function CommandGroup(", "metricPrimary", "metricSecondary", "appliedProfileCount", "new Set(hosts.map((host) => host.profileId)", "successfulTaskCount", "matrixHeader", "matrixEmptyIcon", "onAddServer", "onTestAllSshHosts"]) {
  if (!`${app}\n${modalFrameSource}`.includes(token)) fail(`missing dashboard home polish token: ${token}`);
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
  "handleSetupGuidePreferencesNext",
  "handleImportLocalSshConfig",
  "setupGuideDismissed",
  "Setup Guide",
  "配置向导",
  "Preferences",
  "偏好设置",
  "setupGuidePreferences",
  "setupGuidePreferenceRow",
  "preferencesCopy.settings.themeOptions",
  "preferencesCopy.settings.platformOptions",
  "Choose Language",
  "Step 1: Please choose your preferred language.",
  "第1步：请选择偏好语言",
  "Next",
  "Nothing here yet...",
  '<div className="matrixEmptyIcon" aria-hidden="true"><NavIcon id="hosts" /></div>',
  "Detecting local config...",
  "正在检测本地配置...",
  "Import local config",
  "导入本地配置",
  "未检测到本地存在可用的SSH配置，可使用CodexHub手动添加",
  "Detect local config",
  "检测本地配置",
  "EmptyListState",
  "const iconId =",
  '<NavIcon id={iconId} />',
  "emptyLists",
  "emptyListState",
  "copy.emptyLists.hosts",
  "onOpenSetupGuide"
]) {
  if (!app.includes(token)) fail(`missing setup guide or empty-state token: ${token}`);
}
for (const token of ['new URL("../src-tauri/icons/128x128.png", import.meta.url).href', '<img className="appIcon" src={appLogoUrl} alt="" aria-hidden="true" />']) {
  if (!app.includes(token)) fail(`missing app logo UI token: ${token}`);
}
if (app.includes('new URL("../figs/app-logo.png", import.meta.url).href')) fail("runtime sidebar logo should not bundle figs/app-logo.png");
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
  if (rustBackend.includes(removedInstallToken)) fail(`old skill operation success message should not remain: ${removedInstallToken}`);
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
if (!app.includes("OperationProgressModal")) fail("Host operations should use the shared step progress modal");
if (!app.includes("HostOperationProgressEvent")) fail("Host operations should consume structured progress events");
const managedMaintenanceStepIds = '["preparation", "official-installer", "remote-native-mirror", "remote-npm-mirror", "local-upload", "runtime-reconcile", "final-verification", "release-cleanup"]';
for (const action of ["codex-install", "codex-update"]) {
  if (!app.includes(`${JSON.stringify(action)}: ${managedMaintenanceStepIds}`)) {
    fail(`${action} pending progress must use the stable runtime reconcile/verification/cleanup order`);
  }
}
if (!mockApiSource.includes(`const installStepIds = ${managedMaintenanceStepIds}`)) {
  fail("mock Codex maintenance must use the stable runtime reconcile/verification/cleanup order");
}
if (!app.includes("CodexUninstallConfirmModal") || !app.includes("uninstallCodexConfirmBody") || !app.includes('runRemoteCodexAction(target.hostAlias, "uninstall")')) fail("Remote Codex uninstall should require an explicit confirmation modal before execution");
for (const token of [
  "OperationProgressModal",
  "OperationProgressPanel",
  "OperationStepCard",
  "OperationStatusIcon",
  "useState(false)",
  "aria-expanded={expanded}",
  "{expanded ? (",
  "logsForStep(selectedHost.logs, step.stepId)",
  'role="tablist"',
  'role="tab"',
  "aria-selected={selected}",
  "hosts.some((host) => host.hostAlias === selectedHostAlias)",
  "copy.latestLog",
  "copy.logLevel[log.level]",
  '<details className="taskLogFlowRow operationStepLogEntry"',
  '<summary className="codexOperationLogRow"',
  'className="taskLogFlowDetails"',
  "log.command",
  "log.exitCode",
  "log.durationMs",
  "log.stdout",
  "log.stderr"
]) {
  if (!operationProgressSource.includes(token)) fail(`missing shared operation progress behavior: ${token}`);
}
if (!/\{expanded \? \([\s\S]*?<details className="taskLogFlowRow operationStepLogEntry"[\s\S]*?<summary className="codexOperationLogRow"[\s\S]*?\{log\.message\}[\s\S]*?<div className="taskLogFlowDetails">/u.test(operationProgressSource)) {
  fail("operation step cards must reveal concise log rows before each row reveals full command/output details");
}
if (operationProgressSource.includes("operationStepChevron")) {
  fail("operation step cards must not render the removed right-side chevron");
}
if (operationProgressSource.includes("useState(step.status") || operationProgressSource.includes('step.status === "failed" && expanded')) {
  fail("failed operation steps must stay collapsed until the user expands them");
}
for (const token of [
  "onDisableLogPopups",
  "copy.disableLogPopups",
  "operationProgressHeaderActions",
  "operationProgressDisableButton"
]) {
  if (!operationProgressSource.includes(token)) fail(`missing operation log pop-up header control: ${token}`);
}
for (const token of [
  "showProgressModal && settings.hostOperationLogPopups",
  'action === "install" || action === "update" || action === "uninstall"',
  "settings.hostOperationLogPopups;",
  "setDisableOperationLogPopupsOpen(true)",
  "persistSettings({ ...settings, hostOperationLogPopups: false })",
  "if (!saved) return;",
  "setCodexOperationModal(null)",
  'defaultPlacement: codexOperationModal ? "global" : "detail"'
]) {
  if (!app.includes(token)) fail(`missing host-operation log pop-up preference behavior: ${token}`);
}
const batchProgressPreferenceCount = (app.match(/const showProgressModal = settings\.hostOperationLogPopups;/gu) ?? []).length;
if (batchProgressPreferenceCount < 2) {
  fail("batch host test and batch Codex update must both honor the log pop-up preference");
}
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
for (const token of ["SimpleDeleteConfirmModal", "deleteHostAlias", "deleteProfileId", "onGetProfileApiKey", "handleDetectLocalSshHosts"]) {
  if (!app.includes(token)) fail(`missing delete confirmation or credential reveal token: ${token}`);
}
for (const token of ["const nextProfiles = await api.listProfiles();", "passwordInputWrap profileCredentialInputWrap", 'type={credentialVisible ? "text" : "password"}']) {
  if (!app.includes(token)) fail(`missing profile credential reveal or masked input token: ${token}`);
}
for (const token of ["credentialVisible", "passwordVisible", "CredentialVisibilityIcon", "showApiKey", "hideApiKey", "showPassword", "hidePassword", "if (!result.apiKey)"]) {
  if (!app.includes(token)) fail(`secret visibility/readback token is missing: ${token}`);
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
  "Config TOML",
  "backup"
]) {
  if (!app.includes(token)) fail(`missing compact Profiles UI label: ${token}`);
}
for (const token of ["ProfileEditModal", "ProfileHostSelectModal", "ProfileApplyPreviewModal", "ProfileApplyConfirmModal", "ProfileApplyOperationModal", "ProfileModelCombobox", "ProfileStorageBadge", "profileLibraryActions", "ccSwitchActionButton", "profileCcSwitchStatus", "profileRowActions", "profileApplyTable", "profileHostSelectModal", "profileFastModeSegment"]) {
  if (!app.includes(token)) fail(`missing Profiles modal/action token: ${token}`);
}
for (const token of [
  'useState<RemoteCodexReloadMode>("app-services")',
  'setReloadMode("app-services")',
  'mode: "none"',
  'mode: "app-services"',
  'mode: "all-codex"',
  'name="remote-codex-reload-mode"',
  "allCodexSelected && !allCodexAcknowledged",
  "onConfirm({ remoteCodexReloadMode: reloadMode })",
  "onNext={requestProfileApply}",
  "onNext={() => handleApply()}",
  "handleApply([host.id])",
  'status: "running"',
  'result.outcome === "manual-reconnect"',
  "copy.profiles.manualReconnectGuide",
  "copy.feedback.viewTask",
  "onOpenTask(taskId)"
]) {
  if (!app.includes(token)) fail(`missing profile apply confirmation/result behavior: ${token}`);
}
if ((app.match(/onRunProfileApply\(/gu) ?? []).length !== 1) {
  fail("all profile apply entry points must converge on the single post-confirmation runner");
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
  "setLatestCodexVersion(result.latestCodexVersion)",
  "api.refreshLatestCodexVersion(true)",
  "api.refreshLatestCodexVersion(false)",
  "next.setHours(4, 0, 0, 0)"
]) {
  if (!app.includes(token)) fail(`missing latest/relative Codex UI token: ${token}`);
}
for (const token of [
  "api.batchRemoteProbeCodex(",
  "applyCompletedItem(event.item)",
  "applyHostOperationProgressConnectivityToHosts",
  "api.batchRemoteUpdateCodex(uniqueAliases, 120000, requestId",
  "applyHostOperationProgressEvent(current, event)",
  "aggregateOperationStatus(current.hosts)",
  'status: hostIndex < 6 ? "running" : "pending"',
  "copy.hosts.testedAllResult",
  "copy.hosts.testedAllFailed",
  "copy.hosts.updatedOutdatedCodex"
]) {
  if (!app.includes(token)) fail(`missing backend-batched host-operation UI token: ${token}`);
}
if (app.includes("TaskDrawer")) fail("global Task drawer should be removed while the Tasks page remains available");
if (!app.includes('apiMode === "mock" ? copy.common.mockMode : copy.common.backendMode')) {
  fail("the pre-drawer backend-mode footer should remain explicit in desktop and Mock modes");
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
for (const token of ["disabled={!selectedProfile || profileApplyRunningHostIdSet.has(host.id)}", "handleApply([host.id])"]) {
  if (!profileApplyTableBlock.includes(token)) fail(`missing profile re-apply token: ${token}`);
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
if (!rustBackend.includes('remove_path "$CODEX_HOME"') || !rustBackend.includes('remove_path "$hub_dir"') || rustBackend.includes("codexhub.uninstall.bak")) fail("Remote Codex uninstall should directly delete Codex config/env paths without backups");
if (!app.includes('installCodex: "安装"') || !app.includes('updateCodex: "更新"') || !app.includes('uninstallCodex: "卸载"')) fail("SSH Hosts Codex buttons should use short install/update/uninstall labels");
for (const token of ["onUpdateOutdatedCodexHosts", "handleUpdateOutdatedCodexHosts", "api.batchRemoteUpdateCodex", "outdatedCodexAliases", "copy.hosts.updateOutdatedCodex", "copy.codexOperation.batchUpdateStarted"]) {
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
for (const token of ["TaskLogModal", "taskLogDetailModal", "taskDetailsCol", "copy.tasks.details", "copy.tasks.logs"]) {
  if (!app.includes(token)) fail(`missing task-history log modal token: ${token}`);
}
for (const token of [
  "taskOperationHost(task, copy)",
  'stepId = "legacy-history"',
  'stepId: "unassigned-diagnostics"',
  "<OperationProgressPanel",
  "operationStepPresentation(copy, step)"
]) {
  if (!app.includes(token)) fail(`Tasks history must reuse step-card progress UI: ${token}`);
}
if (app.includes('open={task.status === "failed" && log.level === "error"}')) {
  fail("failed task details must not expand automatically");
}
for (const token of ["MAX_VISIBLE_TASKS = 100", "onClearTaskHistory", "api.clearTaskHistory()", "copy.tasks.clearHistory", "copy.tasks.historyCleared", "clearHistoryOpen"]) {
  if (!app.includes(token)) fail(`missing task-history retention or page clear token: ${token}`);
}
if (app.includes("taskLogClearButton") || app.includes("onClearLogs")) {
  fail("task detail dialog should not own task-history clearing");
}
for (const token of ["MAX_TASK_HISTORY: usize = 100", "TaskHistoryArchive", "task_recycle_tombstones", "trash::delete", "manual-clear", "retention", "system recycle bin"]) {
  if (!rustBackend.includes(token)) fail(`missing system task-history recycle token: ${token}`);
}
if (rustBackend.includes("task_log_trash")) {
  fail("task records must use system recycle-bin archives instead of task_log_trash");
}
if (app.includes("<em>{log.timestamp}</em>")) fail("Tasks log summary rows should show message content without timestamp labels");
for (const token of ["logPanel", "publicKeyBox", "commandGrid", "commands.map((command)"]) {
  if (app.includes(token)) fail(`Tasks/Settings simplification should remove old token: ${token}`);
}
for (const token of ["copyPublicKeyButton", "data-success={publicKeyCopied}", "copy.settings.copyPublicKeySuccess", "onCopyPublicKey: (publicKey: string) => Promise<boolean>"]) {
  if (!app.includes(token)) fail(`missing simplified SSH settings copy token: ${token}`);
}

const api = [
  "src/api.ts",
  "src/api/contracts.ts",
  "src/api/desktop.ts",
  "src/api/fallbacks.ts",
  "src/api/index.ts",
  "src/api/invoke.ts",
  "src/api/mock.ts"
].map(read).join("\n");
for (const token of ["fallbackAppUpdateStatus", "getAppUpdateStatus", "checkStableUpdate", "installStableUpdate", "detectNetworkProxy", "get_app_update_status", "check_stable_update", "install_stable_update", "detect_network_proxy"]) {
  if (!api.includes(token)) fail(`missing stable updater API token: ${token}`);
}
if (!api.includes('checkStableUpdate: () => requiredInvoke<AppUpdateStatusDto>("check_stable_update")')) {
  fail("Stable update check should expose backend/IPC errors instead of falling back to mock status");
}
for (const token of ["chooseCloseButtonBehavior", "choose_close_button_behavior", "onCloseButtonBehaviorRequested", "close-button-behavior-requested", "requiredInvoke<AppSettings>"]) {
  if (!api.includes(token)) fail(`missing close-button API token: ${token}`);
}
for (const token of ["connectSshHost", "ssh-bootstrap-progress", "host-operation-progress", "remote-probe-batch-item-completed", "mockSshBootstrapHostWithProgress", "mockRemoteManageCodexWithProgress", "remoteManageCodex", "batchRemoteProbeCodex", "batchRemoteUpdateCodex", "onItemCompleted"]) {
  if (!api.includes(token)) fail(`missing bootstrap API token: ${token}`);
}
for (const token of ["sampleHostResources", "sample_host_resources", "HostResourceBatchResult", "HostResourceProgressEvent", "host-resource-progress", "mockSampleHostResources", "mockSampleHostResourcesWithProgress", "recordTask?: boolean", "recordTask = true", "sshStatus", "timedOut", "gpuUuid", "processes"]) {
  if (!api.includes(token)) fail(`missing resource monitor API token: ${token}`);
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
for (const token of [
  "normalizeProfileApplyOptions",
  "normalizeProfileApplyResult",
  "options: normalizeProfileApplyOptions(options)",
  "mockApplyProfile(profileId, hostIds, normalizeProfileApplyOptions(options))"
]) {
  if (!api.includes(token)) fail(`missing profile apply reload API boundary token: ${token}`);
}
for (const forbidden of ["remoteReloadCodex", "remote_reload_codex", "reloadRemoteCodexHost"]) {
  if (`${api}\n${rustBackend}`.includes(forbidden)) {
    fail(`remote Codex reload must not expose a standalone Host action: ${forbidden}`);
  }
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

const models = [read("src/models.ts"), read("src/generated/rust-contracts.ts")].join("\n");
for (const token of ["AppUpdateStatus", "AppUpdateState", "pending-configuration", "up-to-date", "installing", "feedConfigured", "signingConfigured"]) {
  if (!models.includes(token)) fail(`missing stable updater model token: ${token}`);
}
for (const token of ["SshBootstrapProgressEvent", "HostOperationProgressEvent", "TaskStep", "TaskStepStatus", "RemoteCodexMaintenanceResult", "RemoteProbeBatchResult", "RemoteCodexBatchResult", "check-version", "password_login", "verify_alias_login"]) {
  if (!models.includes(token)) fail(`missing bootstrap model token: ${token}`);
}
for (const token of [
  "apiKeyEnvVar",
  "credentialStored",
  "ProfileApiKeyResult",
  "ProfileApplyPreview",
  "ProfileApplyBatchResult",
  "ProfileApplyHostResult",
  "ProfileApplyOptions",
  "ProfileApplyOutcome",
  "RemoteCodexReloadMode",
  "RemoteCodexReloadStatus",
  "RemoteCodexReloadResult",
  "remoteCodexReloadMode",
  "targetedCount",
  "preservedCliCount",
  "replacementObserved",
  "manual-reconnect"
]) {
  if (!models.includes(token)) fail(`missing Profile/API model token: ${token}`);
}
for (const token of ["HostResourceSnapshot", "GpuSnapshot", "GpuProcessSnapshot", "HostResourceBatchResult", "CpuSnapshot", "MemorySnapshot", "elapsedSeconds"]) {
  if (!models.includes(token)) fail(`missing resource monitor model token: ${token}`);
}
for (const token of ["SshConfigDeleteResult", "DeleteOperationResult", "task: TaskRun"]) {
  if (!models.includes(token)) fail(`missing delete operation model token: ${token}`);
}
for (const token of ["LatestCodexVersionDto", "version: string | null", "checkedAt: string | null", "source: string"]) {
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
for (const [name, content] of [["src/App.tsx", app], ["src/api modules", api], ["src/models.ts", models]]) {
  for (const token of forbiddenApiKeyTokens) {
    if (content.includes(token)) fail(`${name} must not render or store direct API key token: ${token}`);
  }
}

const settings = read("src/settings.ts");
for (const fontPreset of ["English", "简体中文", "zh-cn"]) {
  if (!settings.includes(fontPreset)) fail(`missing font preset: ${fontPreset}`);
}
for (const token of ["setupGuideDismissed", "setupGuideDismissed: false", "platformAppearance", "platformAppearance: \"auto\"", "networkProxyMode", "networkProxyMode: \"auto\"", "networkProxyUrl", "sidebarCompletionIndicators", "sidebarCompletionIndicators: true", "candidate.sidebarCompletionIndicators !== false", "hostOperationLogPopups", "hostOperationLogPopups: true", "candidate.hostOperationLogPopups !== false", "resolvePlatformAppearance", "applyPlatformAppearance"]) {
  if (!settings.includes(token)) fail(`missing settings token: ${token}`);
}
if (!settings.includes('isWindows(platform) ? "windows" : "macos"')) {
  fail("Platform auto appearance should resolve Linux to the macOS-style UI");
}
for (const token of ["resourceMonitorAutoRefresh", "resourceMonitorAutoRefresh: true", "resourceMonitorHostOrder", "resourceMonitorHostOrder: []", "resourceMonitorRefreshSeconds", "resourceMonitorRefreshSeconds: 60", "normalizeResourceMonitorRefreshSeconds"]) {
  if (!settings.includes(token)) fail(`missing resource monitor settings token: ${token}`);
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
if (!alertModalFrameSource.includes("portalModalContent")) fail("AlertDialog content should use the centered portal content class");
for (const token of [".modalFrame.portalModalContent", "position: fixed", "transform: translate(-50%, -50%)", "z-index: 51"]) {
  if (!styles.includes(token)) fail(`missing centered AlertDialog portal style: ${token}`);
}
const feedback = read("src/ui/feedback.tsx");
for (const token of ["duration={5000}", "dismissForInteraction", 'data-tone={item.tone}', "feedbackToastActions", 'FeedbackPlacement = "detail" | "global"', 'data-placement={viewportPlacement}', "defaultPlacement?: FeedbackPlacement", "configurationRef.current.defaultPlacement"]) {
  if (!feedback.includes(token)) fail(`missing transient feedback token: ${token}`);
}
if (feedback.includes("<Toast.Close") || feedback.includes("persistentFeedbackRegion")) {
  fail("feedback should have no close icon or persistent error region");
}
if (styles.includes("taskDrawer") || styles.includes("persistentFeedback")) {
  fail("removed Task drawer and persistent feedback styles should not remain");
}
for (const token of ['.feedbackToast[data-tone="info"]', '.feedbackToast[data-tone="success"]', '.feedbackToast[data-tone="warning"]', '.feedbackToast[data-tone="error"]', "animation: feedback-enter 1000ms", "animation: feedback-exit 1000ms", "filter: blur(8px)", "translate3d(0, 10px, 0)", "box-shadow: 0 18px 44px", "left: calc(250px + (100vw - 250px) / 2)", '.feedbackToastViewport[data-placement="global"]']) {
  if (!styles.includes(token)) fail(`missing elevated semantic feedback style: ${token}`);
}
for (const token of ['setInfoNotice(copy.notices.addHost, "global")', 'setInfoNotice(copy.notices.newApiConfig, "global")', 'refreshResourceMonitor("initial")', 'refreshResourceMonitor("manual")', 'refreshResourceMonitor("auto")', 'const recordTask = shouldRecordResourceSample(trigger)', 'const showFeedback = recordTask', 'RESOURCE_MONITOR_SAMPLE_TIMEOUT_MS,\n        recordTask', 'placement: "global", tone: "info"']) {
  if (!app.includes(token)) fail(`missing scoped information or monitor feedback token: ${token}`);
}
for (const token of ["record_task.unwrap_or(true)", "if should_record_task", "if !should_record_task", "run_resource_sample"]) {
  if (!rustBackend.includes(token)) fail(`missing automatic resource-sample task policy token: ${token}`);
}
for (const token of ["task_log_summary", "for snapshot in result.snapshots()", "HostResourceStatus::Ok => TaskLogLevel::Info", "HostResourceStatus::Partial => TaskLogLevel::Warn", "HostResourceStatus::Failed => TaskLogLevel::Error"]) {
  if (!rustBackend.includes(token)) fail(`missing per-host resource task log token: ${token}`);
}
for (const token of ["mockResourceSampleTask", "mockResourceSnapshotLogMessage", 'snapshot.status === "ok" ? "info"', 'partial > 0 || failed > 0 ? "warn" : "info"']) {
  if (!api.includes(token)) fail(`missing Mock per-host resource task log token: ${token}`);
}
for (const token of ["normalize_health_check_timeout_ms", "MAX_HEALTH_CHECK_TIMEOUT_MS", "CH_SSH_CONNECTED=1", "HostResourceSshStatus::Offline", "RemoteProbeBatchItemCompletedEvent"]) {
  if (!`${rustBackend}\n${sshRs}`.includes(token)) fail(`missing ten-second SSH health/status token: ${token}`);
}
for (const token of [".storageHealthBanner", "width: min(100%, var(--app-content-max))", "margin: 0 auto 16px"]) {
  if (!styles.includes(token)) fail(`storage health banner should align with the visible detail width: ${token}`);
}
for (const token of ["storagePlanCard", "storagePlanCode", "grid-template-columns: repeat(2, minmax(0, 1fr))", 'font-family: "Consolas"']) {
  if (!app.includes(token) && !styles.includes(token)) fail(`storage migration confirmation should use card and code-snippet styling: ${token}`);
}
for (const token of ["monitorBentoGrid", "monitorHostCard", "circularStatusIndicator", "sshHostsOnlineCol", "monitorSummaryTile", "monitorGpuBlock", "monitorProcessRow", "monitorAutoRefreshControl", "monitorDragHandle", "monitorSegmentedMeter", "monitorDragGhost", "--monitor-gap", "grid-auto-rows: 2px", "data-placeholder", 'data-tone="memory"', 'data-tone="cpu"', 'data-tone="gpu"', 'data-status="refreshing"', "width: 24px", "min-width: 64px", "background: var(--green)", "background: var(--red)", "background: var(--blue)"]) {
  if (!styles.includes(token)) fail(`missing resource monitor Bento style token: ${token}`);
}
if (styles.includes("monitorBentoTile")) fail("resource monitor should use simplified summary tiles instead of old Bento tile styles");
for (const token of [".monitorTable", "min-width: 980px"]) {
  if (styles.includes(token)) fail(`resource monitor table style should be removed: ${token}`);
}
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
const sidebarVisualHintRow = app.indexOf("<span>{copy.settings.sidebarCompletionIndicators}</span>");
const logPopupHintRow = app.indexOf("<span>{copy.settings.hostOperationLogPopups}</span>");
if (sidebarVisualHintRow < 0 || logPopupHintRow <= sidebarVisualHintRow) {
  fail("appearance settings must place the log pop-up pill directly after sidebar visual hints");
}
for (const token of [
  "onHostOperationLogPopupsChange",
  "data-enabled={settings.hostOperationLogPopups}",
  "aria-checked={settings.hostOperationLogPopups}",
  "onHostOperationLogPopupsChange(!settings.hostOperationLogPopups)"
]) {
  if (!app.includes(token)) fail(`missing host-operation log pop-up Settings pill behavior: ${token}`);
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
  "min-height: 38px",
  ".dividedSettingsRows .segmentedControl button"
]) {
  if (!styles.includes(token)) fail(`missing appearance row alignment style token: ${token}`);
}
for (const token of ["titleDragRegion", "captionGlyph::before", "appTitleBar", "captionButton", "modalHeader", "modalTitleIcon", "emojiIcon", "navIcon", "navGlyph", "navLabel", "navCompletionDot", "navCompletionDot[data-tone=\"error\"]", "commandBar", "commandGroup", "pillToggle", "pillToggleThumb", "translateX(22px)", "metricPrimary", "metricSecondary", "matrixHeader", "matrixEmptyState", "matrixEmptyIcon", ".hostMeta .badge"]) {
  if (!styles.includes(token)) fail(`missing dashboard home polish style token: ${token}`);
}
for (const token of ["setupGuideModal", "setupGuideLanguage", "setupGuideLanguageOption", "setupGuideHostList", "setupGuideHostHeader", "setupGuideActions", "emptyListState", "emptyListIcon", "emptyListActions"]) {
  if (!styles.includes(token)) fail(`missing setup guide or empty-state style token: ${token}`);
}
for (const token of ["matrixEmptyIcon::before", "matrixEmptyIcon::after", "matrixEmptyIcon span"]) {
  if (styles.includes(token)) fail(`matrix empty state should use shared NavIcon instead of CSS line icon: ${token}`);
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
for (const token of ["simpleDeleteModal", ".sshHostModal.ProfileEditModal .fieldGroup", ".sshHostModal:not(.ProfileEditModal) .fieldGroup input", "::-ms-reveal", ".passwordInputWrap"]) {
  if (!styles.includes(token)) fail(`missing delete confirmation or credential editor style token: ${token}`);
}
for (const token of ["credentialVisibilityButton", "credentialEyeIcon"]) {
  if (!styles.includes(token)) fail(`secret visibility style is missing: ${token}`);
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
for (const token of [
  ".operationProgressModal",
  ".operationHostSelector",
  ".operationHostTab",
  ".operationStepList",
  ".operationStepCard",
  ".operationStepSummary",
  ".operationStatusIcon",
  ".operationStepDetails",
  '.operationStatusIcon[data-status="running"]',
  '.operationStatusIcon[data-status="success"]',
  '.operationStatusIcon[data-status="failed"]'
]) {
  if (!styles.includes(token)) fail(`missing shared operation progress style: ${token}`);
}
const operationHostSelectorStyle = styles.match(/\.operationHostSelector\s*\{[^}]*\}/u)?.[0] ?? "";
for (const token of ["display: flex", "flex-wrap: wrap"]) {
  if (!operationHostSelectorStyle.includes(token)) fail(`batch host selector must wrap all host pills: ${token}`);
}
if (styles.includes("operationStepChevron")) {
  fail("removed operation step chevron styles must not remain");
}
const operationHeaderCloseStyle = styles.match(/\.operationProgressHeaderActions \.modalCloseButton\s*\{[^}]*\}/u)?.[0] ?? "";
if (!operationHeaderCloseStyle.includes("position: static")) {
  fail("operation progress close button must stay inset inside the header action group");
}
for (const token of ["taskLogModal", "taskLogModalMeta", "taskDetailsCol", "taskTableWrap", "tasksTable", "copyPublicKeyButton", 'data-success="true"', "max-width: var(--app-content-max)"]) {
  if (!styles.includes(token)) fail(`missing simplified UI style token: ${token}`);
}

console.log("SMOKE PASS: CodexHub docs and Tauri skeleton are present.");
