import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";

const root = process.cwd();
const packageJson = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
const version = packageJson.version;
const repo = process.env.GITHUB_REPOSITORY || process.env.CODEXHUB_RELEASE_REPOSITORY;
const releaseTag = process.env.CODEXHUB_RELEASE_TAG || process.env.GITHUB_REF_NAME || `v${version}`;
const normalizedTag = releaseTag.startsWith("v") ? releaseTag : `v${releaseTag}`;
const bundleDir = path.join(root, "src-tauri", "target", "release", "bundle", "nsis");
const updaterName = `CodexHub_${version}_x64-setup.exe`;
const updaterPath = path.join(bundleDir, updaterName);
const signaturePath = `${updaterPath}.sig`;
const outputDir = path.join(root, "dist-release", `v${version}`, "windows-updater");
const latestPath = path.join(outputDir, "latest.json");
const checksumPath = path.join(outputDir, "SHA256SUMS.txt");

const fail = (message) => {
  console.error(`WINDOWS UPDATER FEED FAIL: ${message}`);
  process.exit(1);
};

for (const file of [updaterPath, signaturePath]) {
  if (!fs.existsSync(file)) fail(`missing expected release file: ${path.relative(root, file)}`);
}
if (!repo || !/^[^/\s]+\/[^/\s]+$/.test(repo)) {
  fail("set GITHUB_REPOSITORY or CODEXHUB_RELEASE_REPOSITORY to <owner>/<repo>");
}

const signature = fs.readFileSync(signaturePath, "utf8").trim();
if (!signature) fail(`empty updater signature: ${path.relative(root, signaturePath)}`);

fs.mkdirSync(outputDir, { recursive: true });
fs.copyFileSync(updaterPath, path.join(outputDir, path.basename(updaterPath)));

const updaterUrl = `https://github.com/${repo}/releases/download/${normalizedTag}/${updaterName}`;
const feed = {
  version,
  notes: `CodexHub ${version} stable update.`,
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature,
      url: updaterUrl
    }
  }
};

fs.writeFileSync(latestPath, `${JSON.stringify(feed, null, 2)}\n`, "utf8");
const checksumEntries = [updaterName, "latest.json"].map((fileName) => {
  const filePath = path.join(outputDir, fileName);
  const digest = crypto.createHash("sha256").update(fs.readFileSync(filePath)).digest("hex");
  return `${digest}  ${fileName}`;
});
fs.writeFileSync(checksumPath, `${checksumEntries.join("\n")}\n`, "utf8");
console.log(`Windows updater feed: ${path.relative(root, latestPath)}`);
console.log(`Windows updater artifact: ${path.relative(root, path.join(outputDir, updaterName))}`);
console.log(`Windows updater checksums: ${path.relative(root, checksumPath)}`);
