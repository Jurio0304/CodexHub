import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";

const root = process.cwd();
const packageJson = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
const version = packageJson.version;
const repo = process.env.GITHUB_REPOSITORY || process.env.CODEXHUB_RELEASE_REPOSITORY;
const releaseTag = process.env.CODEXHUB_RELEASE_TAG || process.env.GITHUB_REF_NAME || `v${version}`;
const normalizedTag = releaseTag.startsWith("v") ? releaseTag : `v${releaseTag}`;
const outputDir = path.join(root, "dist-release", `v${version}`, "linux-updater");
const existingDir = path.join(root, "dist-release", "existing-release");
const existingLatestPath = process.env.CODEXHUB_EXISTING_FEED_PATH || path.join(existingDir, "latest.json");
const existingChecksumPath = process.env.CODEXHUB_EXISTING_CHECKSUM_PATH || path.join(existingDir, "SHA256SUMS.txt");
const artifactDir =
  process.env.CODEXHUB_LINUX_DEB_DIR ||
  path.join(root, "dist-release", `v${version}`, "linux-deb");
const bundleDebDir = path.join(root, "src-tauri", "target", "release", "bundle", "deb");
const latestPath = path.join(outputDir, "latest.json");
const checksumPath = path.join(outputDir, "SHA256SUMS.txt");

const linuxTargets = [
  { debArch: "amd64", platformKey: "linux-x86_64" },
  { debArch: "arm64", platformKey: "linux-aarch64" }
];

const fail = (message) => {
  console.error(`LINUX UPDATER FEED FAIL: ${message}`);
  process.exit(1);
};

const firstExistingPath = (...candidates) => candidates.find((candidate) => fs.existsSync(candidate));
const stripBom = (value) => value.replace(/^\uFEFF/, "");

if (!repo || !/^[^/\s]+\/[^/\s]+$/.test(repo)) {
  fail("set GITHUB_REPOSITORY or CODEXHUB_RELEASE_REPOSITORY to <owner>/<repo>");
}
if (!fs.existsSync(existingLatestPath)) {
  fail(`missing existing release feed to merge: ${path.relative(root, existingLatestPath)}`);
}

const existingFeed = JSON.parse(stripBom(fs.readFileSync(existingLatestPath, "utf8")));
if (existingFeed.version !== version) {
  fail(`existing release feed version ${existingFeed.version} does not match package version ${version}`);
}

fs.mkdirSync(outputDir, { recursive: true });

const platformEntries = {};
const debNames = [];
for (const target of linuxTargets) {
  const debName = `CodexHub_${version}_${target.debArch}.deb`;
  const debPath = firstExistingPath(path.join(artifactDir, debName), path.join(bundleDebDir, debName));
  if (!debPath) fail(`missing Linux deb asset: ${debName}`);

  const signaturePath = `${debPath}.sig`;
  if (!fs.existsSync(signaturePath)) fail(`missing Linux deb updater signature: ${path.relative(root, signaturePath)}`);
  const signature = fs.readFileSync(signaturePath, "utf8").trim();
  if (!signature) fail(`empty Linux deb updater signature: ${path.relative(root, signaturePath)}`);

  fs.copyFileSync(debPath, path.join(outputDir, debName));
  debNames.push(debName);
  platformEntries[target.platformKey] = {
    signature,
    url: `https://github.com/${repo}/releases/download/${normalizedTag}/${debName}`
  };
}

const stableNotes = `CodexHub ${version} stable update.`;
const existingNotes = typeof existingFeed.notes === "string" ? existingFeed.notes.trim() : "";
const feed = {
  ...existingFeed,
  version,
  notes: existingNotes || stableNotes,
  pub_date: new Date().toISOString(),
  platforms: {
    ...(existingFeed.platforms ?? {}),
    ...platformEntries
  }
};

fs.writeFileSync(latestPath, `${JSON.stringify(feed, null, 2)}\n`, "utf8");

const generatedNames = new Set([...debNames, "latest.json"]);
const existingChecksumLines = fs.existsSync(existingChecksumPath)
  ? fs
      .readFileSync(existingChecksumPath, "utf8")
      .replace(/^\uFEFF/, "")
      .split(/\r?\n/)
      .map((line) => line.trimEnd())
      .filter(Boolean)
      .filter((line) => {
        const match = line.match(/^[a-f0-9]{64}\s+\*?(.+)$/i);
        return match ? !generatedNames.has(match[1]) : true;
      })
  : [];
const checksumEntries = [...debNames, "latest.json"].map((fileName) => {
  const filePath = path.join(outputDir, fileName);
  const digest = crypto.createHash("sha256").update(fs.readFileSync(filePath)).digest("hex");
  return `${digest}  ${fileName}`;
});
fs.writeFileSync(checksumPath, `${[...existingChecksumLines, ...checksumEntries].join("\n")}\n`, "utf8");

console.log(`Linux updater feed: ${path.relative(root, latestPath)}`);
for (const debName of debNames) {
  console.log(`Linux updater artifact: ${path.relative(root, path.join(outputDir, debName))}`);
}
console.log(`Linux updater checksums: ${path.relative(root, checksumPath)}`);
