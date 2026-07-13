import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
const read = (relativePath) => fs.readFileSync(path.join(root, relativePath), "utf8");
const fail = (message) => {
  console.error(`API BOUNDARY FAIL: ${message}`);
  process.exit(1);
};

const commandsSource = read("src/api/commands.ts");
const desktopSource = read("src/api/desktop.ts");
const apiContractsSource = read("src/api/contracts.ts");
const mockSource = read("src/api/mock.ts");
const invokeSource = read("src/api/invoke.ts");
const settingsSource = read("src/settings.ts");
const appSource = read("src/App.tsx");
const rustSettingsSource = read("src-tauri/src/settings.rs");
const boundaryDoc = read("docs/desktop-command-boundaries.md");
const rustHandlerSource = read("src-tauri/src/app_runtime.rs");
const rustSource = [
  "src-tauri/src/lib.rs",
  "src-tauri/src/app_runtime.rs",
  "src-tauri/src/services/host_use_cases.rs",
  "src-tauri/src/services/profile_use_cases.rs",
  "src-tauri/src/services/skill_use_cases.rs",
  "src-tauri/src/services/updater_operations.rs"
].map(read).join("\n");
const packageJson = JSON.parse(read("package.json"));
const tauriConfig = JSON.parse(read("src-tauri/tauri.conf.json"));

const policyCommands = [...commandsSource.matchAll(/^  ([a-z][a-z0-9_]+): \{/gm)].map((match) => match[1]);
const handlerBody = rustHandlerSource.match(/invoke_handler\(tauri::generate_handler!\[([\s\S]*?)\]\)/)?.[1] ?? "";
const rustCommands = handlerBody
  .split(",")
  .map((command) => command.trim())
  .filter(Boolean);
const desktopCommands = [...desktopSource.matchAll(/(?:requiredInvoke(?:<[^>]+>)?|assertTauriRuntime)\("([a-z][a-z0-9_]+)"/g)].map(
  (match) => match[1]
);

const sorted = (items) => [...new Set(items)].sort();
if (JSON.stringify(sorted(policyCommands)) !== JSON.stringify(sorted(rustCommands))) {
  fail("commandPolicies and Rust generate_handler! are not identical");
}
if (!/get_profile_api_key:\s*\{[^}]*sensitiveOutput:\s*true/.test(commandsSource)) {
  fail("get_profile_api_key must be declared as sensitive output");
}
for (const command of desktopCommands) {
  if (!policyCommands.includes(command)) fail(`desktop command is missing policy: ${command}`);
}
for (const command of ["batch_remote_probe_codex", "batch_remote_update_codex"]) {
  if (!policyCommands.includes(command)) fail(`batch host operation is missing command policy: ${command}`);
  if (!rustCommands.includes(command)) fail(`batch host operation is missing Rust handler: ${command}`);
  if (!desktopCommands.includes(command)) fail(`batch host operation must use the required desktop invoke path: ${command}`);
}
for (const token of [
  "HostOperationProgressHandler",
  "batchRemoteProbeCodex",
  "batchRemoteUpdateCodex",
  'listen<HostOperationProgressEvent>("host-operation-progress"',
  "event.payload.requestId === requestId"
]) {
  if (!`${apiContractsSource}\n${desktopSource}`.includes(token)) {
    fail(`structured host-operation API boundary is missing: ${token}`);
  }
}
for (const token of ["batchRemoteProbeCodex:", "batchRemoteUpdateCodex:", "runMockConcurrencyPool", "concurrency = 6"]) {
  if (!mockSource.includes(token)) fail(`Mock batch API boundary is missing: ${token}`);
}
if (desktopSource.includes('"remote-codex-progress"')) {
  fail("desktop API must consume the unified host-operation-progress event");
}
for (const forbidden of ["safeInvoke", "mockApi", "./fallbacks", "fallbackHealth", "fallbackSshStatus"]) {
  if (desktopSource.includes(forbidden)) fail(`desktop API contains forbidden fallback token: ${forbidden}`);
}
for (const required of ["DesktopCommandError", "assertTauriRuntime(command)", "redactSensitiveText", "isTauri()"] ) {
  if (!invokeSource.includes(required)) fail(`invoke boundary is missing: ${required}`);
}
for (const required of ["requireHostAlias", "get_profile_api_key", "test_connection_host_alias"]) {
  if (!`${invokeSource}\n${desktopSource}\n${rustSource}`.includes(required)) fail(`backend hardening is missing: ${required}`);
}
for (const forbidden of ["Mock SSH handshake", '"Mock Host"', "get_profile_credential_status", "ProfileCredentialStatus"]) {
  if (`${rustSource}\n${desktopSource}`.includes(forbidden)) fail(`desktop backend contains forbidden Mock/stale token: ${forbidden}`);
}
for (const command of policyCommands) {
  if (!boundaryDoc.includes(`\`${command}\``)) fail(`desktop command boundary doc is missing: ${command}`);
}
if (!packageJson.scripts["dev:web:mock"]?.includes("--mode mock")) fail("dev:web:mock must explicitly select mock mode");
if (!packageJson.scripts["dev:web:desktop"]?.includes("--mode desktop")) fail("dev:web:desktop must explicitly select desktop mode");
if (!packageJson.scripts["build:web:mock"]?.includes("--mode mock")) fail("build:web:mock must explicitly select mock mode");
if (!packageJson.scripts["build:web:desktop"]?.includes("--mode desktop")) fail("build:web:desktop must explicitly select desktop mode");
if (tauriConfig.build?.beforeDevCommand !== "pnpm dev:web:desktop") fail("Tauri dev must use desktop frontend mode");
if (tauriConfig.build?.beforeBuildCommand !== "pnpm build:web:desktop") fail("Tauri build must use desktop frontend mode");
for (const required of ["desktopSettingsCacheKey", "mockSettingsStorageKey", "legacySettingsStorageKey"]) {
  if (!settingsSource.includes(required)) fail(`settings storage boundary is missing: ${required}`);
}
for (const required of ["settingsSaveError", "pendingSettings", "retrySettingsSave", "await api.saveSettings"]) {
  if (!appSource.includes(required)) fail(`settings transaction UI is missing: ${required}`);
}
for (const required of ["SettingsSaveResult", "storage::save_document", "changed: false", "backup_path"]) {
  if (!rustSettingsSource.includes(required)) fail(`Rust settings transaction is missing: ${required}`);
}

console.log(`API BOUNDARY PASS: ${policyCommands.length} Tauri commands use explicit desktop/mock boundaries.`);
