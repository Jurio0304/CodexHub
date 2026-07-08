import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";

const root = process.cwd();
const packageJson = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
const version = packageJson.version;
const repo = process.env.GITHUB_REPOSITORY || process.env.CODEXHUB_RELEASE_REPOSITORY;
const releaseTag = process.env.CODEXHUB_RELEASE_TAG || process.env.GITHUB_REF_NAME || `v${version}`;
const normalizedTag = releaseTag.startsWith("v") ? releaseTag : `v${releaseTag}`;
const linuxArch = process.env.CODEXHUB_LINUX_ARCH || "amd64";
const platformKey = process.env.CODEXHUB_LINUX_PLATFORM || "linux-x86_64";
const bundleDir = path.join(root, "src-tauri", "target", "release", "bundle");
const outputDir = path.join(root, "dist-release", `v${version}`, "linux-updater");
const existingDir = path.join(root, "dist-release", "existing-release");
const existingLatestPath = process.env.CODEXHUB_EXISTING_FEED_PATH || path.join(existingDir, "latest.json");
const existingChecksumPath = process.env.CODEXHUB_EXISTING_CHECKSUM_PATH || path.join(existingDir, "SHA256SUMS.txt");
const latestPath = path.join(outputDir, "latest.json");
const checksumPath = path.join(outputDir, "SHA256SUMS.txt");

const fail = (message) => {
  console.error(`LINUX UPDATER FEED FAIL: ${message}`);
  process.exit(1);
};

const findFiles = (directory, predicate) => {
  if (!fs.existsSync(directory)) return [];
  const entries = fs.readdirSync(directory, { withFileTypes: true });
  return entries.flatMap((entry) => {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) return findFiles(entryPath, predicate);
    return predicate(entryPath) ? [entryPath] : [];
  });
};

const singleFile = (files, label) => {
  if (files.length === 0) fail(`missing ${label}`);
  if (files.length > 1) fail(`expected one ${label}, found: ${files.map((file) => path.relative(root, file)).join(", ")}`);
  return files[0];
};

if (!repo || !/^[^/\s]+\/[^/\s]+$/.test(repo)) {
  fail("set GITHUB_REPOSITORY or CODEXHUB_RELEASE_REPOSITORY to <owner>/<repo>");
}
if (!fs.existsSync(existingLatestPath)) fail(`missing existing release feed to merge: ${path.relative(root, existingLatestPath)}`);

const appImagePath = singleFile(
  findFiles(path.join(bundleDir, "appimage"), (filePath) => filePath.endsWith(".AppImage")),
  "Linux AppImage"
);
const signaturePath = `${appImagePath}.sig`;
if (!fs.existsSync(signaturePath)) fail(`missing Linux AppImage signature: ${path.relative(root, signaturePath)}`);

const debCandidates = findFiles(path.join(bundleDir, "deb"), (filePath) => filePath.endsWith(".deb"));
const debPath = singleFile(
  debCandidates.filter((filePath) => path.basename(filePath).includes(linuxArch)),
  `Linux ${linuxArch} deb`
);

const existingFeed = JSON.parse(fs.readFileSync(existingLatestPath, "utf8"));
if (existingFeed.version !== version) {
  fail(`existing release feed version ${existingFeed.version} does not match package version ${version}`);
}
const signature = fs.readFileSync(signaturePath, "utf8").trim();
if (!signature) fail(`empty updater signature: ${path.relative(root, signaturePath)}`);

fs.mkdirSync(outputDir, { recursive: true });
const appImageName = `CodexHub_${version}_${linuxArch}.AppImage`;
const debName = `CodexHub_${version}_${linuxArch}.deb`;
fs.copyFileSync(appImagePath, path.join(outputDir, appImageName));
fs.copyFileSync(debPath, path.join(outputDir, debName));

const updaterUrl = `https://github.com/${repo}/releases/download/${normalizedTag}/${appImageName}`;
const stableNotes = `CodexHub ${version} stable update.`;
const existingNotes = typeof existingFeed.notes === "string" ? existingFeed.notes.trim() : "";
const feed = {
  ...existingFeed,
  version,
  notes: existingNotes || stableNotes,
  pub_date: new Date().toISOString(),
  platforms: {
    ...(existingFeed.platforms ?? {}),
    [platformKey]: {
      signature,
      url: updaterUrl
    }
  }
};

fs.writeFileSync(latestPath, `${JSON.stringify(feed, null, 2)}\n`, "utf8");

const generatedNames = new Set([appImageName, debName, "latest.json"]);
const existingChecksumLines = fs.existsSync(existingChecksumPath)
  ? fs
      .readFileSync(existingChecksumPath, "utf8")
      .split(/\r?\n/)
      .map((line) => line.trimEnd())
      .filter(Boolean)
      .filter((line) => {
        const match = line.match(/^[a-f0-9]{64}\s+\*?(.+)$/i);
        return match ? !generatedNames.has(match[1]) : true;
      })
  : [];
const checksumEntries = [appImageName, debName, "latest.json"].map((fileName) => {
  const filePath = path.join(outputDir, fileName);
  const digest = crypto.createHash("sha256").update(fs.readFileSync(filePath)).digest("hex");
  return `${digest}  ${fileName}`;
});
fs.writeFileSync(checksumPath, `${[...existingChecksumLines, ...checksumEntries].join("\n")}\n`, "utf8");

console.log(`Linux updater feed: ${path.relative(root, latestPath)}`);
console.log(`Linux AppImage artifact: ${path.relative(root, path.join(outputDir, appImageName))}`);
console.log(`Linux deb artifact: ${path.relative(root, path.join(outputDir, debName))}`);
console.log(`Linux updater checksums: ${path.relative(root, checksumPath)}`);
