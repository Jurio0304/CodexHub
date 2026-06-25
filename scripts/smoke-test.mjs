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
  "add_host",
  "update_host",
  "delete_host",
  "test_ssh_connection",
  "list_profiles",
  "apply_profile",
  "list_tasks"
]) {
  if (!rustLib.includes(command)) fail(`missing ${command} Tauri command`);
}

const app = read("src/App.tsx");
for (const label of ["Dashboard", "Hosts", "Profiles", "Skills", "Tasks", "Settings", "Server Matrix", "Font"]) {
  if (!app.includes(label)) fail(`missing UI label: ${label}`);
}

const settings = read("src/settings.ts");
for (const fontPreset of ["System Default", "Chinese Optimized", "English Optimized", "Cross Platform"]) {
  if (!settings.includes(fontPreset)) fail(`missing font preset: ${fontPreset}`);
}

const styles = read("src/styles.css");
for (const token of ["--font-ui", "--font-mono", "font-family: var(--font-ui)", "font-family: var(--font-mono)"]) {
  if (!styles.includes(token)) fail(`missing font token: ${token}`);
}

console.log("SMOKE PASS: CodexHub docs and Tauri skeleton are present.");
