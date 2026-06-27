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
  [mvp, "Mandatory remote Codex wrapper"],
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
  "list_profiles",
  "apply_profile",
  "list_tasks"
]) {
  if (!rustLib.includes(command)) fail(`missing ${command} Tauri command`);
}
for (const asyncCommand of ["async fn ssh_check", "async fn remote_probe_codex", "async fn remote_manage_codex"]) {
  if (!rustLib.includes(asyncCommand)) fail(`long remote command must stay async: ${asyncCommand}`);
}
if (!rustLib.includes("spawn_blocking(command)")) fail("long remote commands should run through the blocking worker pool");

const app = read("src/App.tsx");
for (const label of ["Dashboard", "Hosts", "Profiles", "Skills", "Tasks", "Settings", "Server Matrix", "Font", "SSH Hosts", "Host IP", "Codex版本", "Test all", "一键测试", "测试延迟", "stdout", "stderr", "Install Codex", "Update Codex", "新增 SSH Host", "连接进程", "BootstrapProgressLog"]) {
  if (!app.includes(label)) fail(`missing UI label: ${label}`);
}
if (!app.includes("function ProfilesView()")) fail("Profiles page should be intentionally empty until config editing is implemented");
if (app.includes("profileCodexBar")) fail("Profiles page should not expose the all-host remote Codex list");
if (!app.includes("CodexOperationModal")) fail("Install/update should show a compact Codex operation progress modal");
if (!app.includes("codexOperationModal")) fail("Codex operation modal state should be wired in App");
if (!app.includes("RemoteCodexProgressEvent")) fail("Codex operation modal should consume real progress events");
if (app.includes('event.status === "success" ? "success"') || app.includes('event.status === "failed" ? "failed"')) fail("Codex operation modal status should only change from the final result or catch path");
if (!app.includes("logRowsRef") || !app.includes("logRows.scrollTop = logRows.scrollHeight") || !app.includes("ref={logRowsRef}")) fail("Codex operation log should auto-scroll to the latest row");
if (app.includes('<div className="eyebrow">{copy.codexOperation.title}</div>') || app.includes("{operation.hostName} · {operation.hostAlias}")) fail("Codex operation header should only keep the title and status badge");
if (app.includes("profileCard")) fail("Profiles page should use a compact table list instead of profile cards");
if (app.includes("onApplyProfile")) fail("Profiles page should not show the old profile apply card/list actions");
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

const models = read("src/models.ts");
for (const token of ["SshBootstrapProgressEvent", "RemoteCodexProgressEvent", "RemoteCodexMaintenanceResult", "check-version", "password_login", "verify_alias_login"]) {
  if (!models.includes(token)) fail(`missing bootstrap model token: ${token}`);
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
for (const token of ["sshHostsTable", "table-layout: auto", "flex-wrap: nowrap", ".sshHostsCodexCol"]) {
  if (!styles.includes(token)) fail(`missing SSH Hosts responsive table style token: ${token}`);
}
const codexLogRowsStyle = styles.match(/\.codexOperationLogRows\s*\{[^}]*\}/)?.[0] ?? "";
if (!codexLogRowsStyle.includes("border: 1px solid var(--border)") || !codexLogRowsStyle.includes("background: var(--surface-muted)")) fail("Codex operation logs should render inside one unified panel");
const codexLogCodeStyle = styles.match(/\.codexOperationLogRow code\s*\{[^}]*\}/)?.[0] ?? "";
if (!codexLogCodeStyle.includes("overflow-wrap: anywhere") || !codexLogCodeStyle.includes("white-space: pre-wrap")) fail("Codex operation log detail should wrap long output");
if (codexLogCodeStyle.includes("text-overflow") || codexLogCodeStyle.includes("white-space: nowrap")) fail("Codex operation log detail should not be ellipsized or forced onto one line");

console.log("SMOKE PASS: CodexHub docs and Tauri skeleton are present.");
