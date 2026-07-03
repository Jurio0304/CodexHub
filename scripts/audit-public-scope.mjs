import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const root = process.cwd();
const textExtensions = new Set([
  ".css",
  ".html",
  ".json",
  ".lock",
  ".md",
  ".mjs",
  ".ps1",
  ".rs",
  ".toml",
  ".ts",
  ".tsx",
  ".txt",
  ".yaml",
  ".yml"
]);
const binaryExtensions = new Set([".exe", ".ico", ".png", ".zip"]);

const forbiddenPathPatterns = [
  /(^|[\\/])\.env($|[\\/\.])/i,
  /(^|[\\/])(?:id_ed25519|id_rsa)(?:\.pub)?$/i,
  /(^|[\\/])known_hosts$/i,
  /(^|[\\/])ssh_config$/i,
  /(^|[\\/])(?:hosts|profiles|tasks|settings|skill-inventory|codex-latest)\.json$/i,
  /\.(?:db|sqlite|sqlite3|pem|key)$/i,
  /(^|[\\/])src-tauri[\\/]target[\\/]/i,
  /(^|[\\/])dist[\\/]/i
];

const contentChecks = [
  {
    name: "private key material",
    pattern: /-----BEGIN (?:RSA |DSA |EC |OPENSSH |PRIVATE )?PRIVATE KEY-----/i,
    allow: (file, line) =>
      file.replaceAll("\\", "/") === "src-tauri/src/ssh.rs" &&
      line.includes("token=sk-test123 password=hunter2")
  },
  {
    name: "OpenAI-like API key",
    pattern: /\bsk-[A-Za-z0-9_-]{20,}\b/,
    allow: () => false
  },
  {
    name: "recorded private host",
    pattern: /\b(?:10\.39\.|10\.214\.|jy@10\.)[0-9.]*\b/i,
    allow: () => false
  }
];

const fail = (message) => {
  console.error(`PUBLIC AUDIT FAIL: ${message}`);
  process.exitCode = 1;
};

const escapeRegExp = (value) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

const auditScriptPath = "scripts/audit-public-scope.mjs";

const gitFiles = () => {
  const result = spawnSync("git", ["ls-files", "-z", "--cached", "--others", "--exclude-standard"], {
    cwd: root,
    encoding: "utf8"
  });
  if (result.status !== 0) {
    throw new Error(result.stderr.trim() || "git ls-files failed");
  }
  return result.stdout.split("\0").filter(Boolean);
};

const artifactFiles = () => {
  const roots = ["release-artifacts", "dist-release"]
    .map((item) => path.join(root, item))
    .filter((item) => fs.existsSync(item));
  const files = [];
  const walk = (directory) => {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const absolute = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        walk(absolute);
      } else if (entry.isFile()) {
        files.push(path.relative(root, absolute));
      }
    }
  };
  for (const directory of roots) walk(directory);
  return files;
};

const uniqueFiles = [...new Set([...gitFiles(), ...artifactFiles()])].sort();

const personalChecks = [
  {
    name: "personal repository or user identifier",
    pattern: /\bjurio(?:0304)?\b/i
  },
  {
    name: "personal Windows profile path",
    pattern: /[A-Z]:[\\/]Users[\\/](?:PC|[^\\/\s]*jurio[^\\/\s]*)\b/i
  },
  {
    name: "personal workspace path",
    pattern: /[A-Z]:[\\/][^\r\n]*\bjurio\b[^\r\n]*/i
  }
];

const localHome = os.homedir();
if (localHome) {
  personalChecks.push({
    name: "local home directory",
    pattern: new RegExp(escapeRegExp(localHome), "i")
  });
}

const localMachine = process.env.COMPUTERNAME;
if (localMachine && localMachine.length >= 3) {
  personalChecks.push({
    name: "local machine name",
    pattern: new RegExp(`\\b${escapeRegExp(localMachine)}\\b`, "i")
  });
}

const isUpdaterFeedReleaseUrl = (file, line) =>
  /^dist-release\/v[^/]+\/windows-updater\/latest\.json$/i.test(file) &&
  /https:\/\/github\.com\/[^/\s"]+\/[^/\s"]+\/releases\/download\//i.test(line);

for (const file of uniqueFiles) {
  const normalized = file.replaceAll("\\", "/");
  for (const pattern of forbiddenPathPatterns) {
    if (pattern.test(normalized)) {
      fail(`forbidden public path: ${file}`);
    }
  }

  const absolute = path.join(root, file);
  if (!fs.existsSync(absolute) || !fs.statSync(absolute).isFile()) continue;

  const extension = path.extname(file).toLowerCase();
  if (binaryExtensions.has(extension)) continue;
  if (!textExtensions.has(extension)) continue;
  if (fs.statSync(absolute).size > 2_000_000) continue;

  const content = fs.readFileSync(absolute, "utf8");
  const lines = content.split(/\r?\n/);
  lines.forEach((line, index) => {
    for (const check of contentChecks) {
      if (check.pattern.test(line) && !check.allow(normalized, line)) {
        fail(`${check.name} in ${file}:${index + 1}`);
      }
    }
    for (const check of personalChecks) {
      if (check.pattern.test(line) && normalized !== auditScriptPath && !isUpdaterFeedReleaseUrl(normalized, line)) {
        fail(`${check.name} in ${file}:${index + 1}`);
      }
    }
  });
}

if (process.exitCode) {
  process.exit(process.exitCode);
}

console.log(`PUBLIC AUDIT PASS: checked ${uniqueFiles.length} source and release-scope file(s).`);
