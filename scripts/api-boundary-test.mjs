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
const invokeSource = read("src/api/invoke.ts");
const rustSource = read("src-tauri/src/lib.rs");
const packageJson = JSON.parse(read("package.json"));
const tauriConfig = JSON.parse(read("src-tauri/tauri.conf.json"));

const policyCommands = [...commandsSource.matchAll(/^  ([a-z][a-z0-9_]+): \{/gm)].map((match) => match[1]);
const handlerBody = rustSource.match(/invoke_handler\(tauri::generate_handler!\[([\s\S]*?)\]\)/)?.[1] ?? "";
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
if (policyCommands.length !== 54) fail(`expected 54 command policies, received ${policyCommands.length}`);
for (const command of desktopCommands) {
  if (!policyCommands.includes(command)) fail(`desktop command is missing policy: ${command}`);
}
for (const forbidden of ["safeInvoke", "mockApi", "./fallbacks", "fallbackHealth", "fallbackSshStatus"]) {
  if (desktopSource.includes(forbidden)) fail(`desktop API contains forbidden fallback token: ${forbidden}`);
}
for (const required of ["DesktopCommandError", "assertTauriRuntime(command)", "redactSensitiveText", "isTauri()"] ) {
  if (!invokeSource.includes(required)) fail(`invoke boundary is missing: ${required}`);
}
if (!packageJson.scripts["dev:web:mock"]?.includes("--mode mock")) fail("dev:web:mock must explicitly select mock mode");
if (!packageJson.scripts["dev:web:desktop"]?.includes("--mode desktop")) fail("dev:web:desktop must explicitly select desktop mode");
if (!packageJson.scripts["build:web:mock"]?.includes("--mode mock")) fail("build:web:mock must explicitly select mock mode");
if (!packageJson.scripts["build:web:desktop"]?.includes("--mode desktop")) fail("build:web:desktop must explicitly select desktop mode");
if (tauriConfig.build?.beforeDevCommand !== "pnpm dev:web:desktop") fail("Tauri dev must use desktop frontend mode");
if (tauriConfig.build?.beforeBuildCommand !== "pnpm build:web:desktop") fail("Tauri build must use desktop frontend mode");

console.log(`API BOUNDARY PASS: ${policyCommands.length} Tauri commands use explicit desktop/mock boundaries.`);
