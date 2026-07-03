import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
const sourcePath = path.join(root, "src-tauri", "tauri.updater.conf.json");
const outputPath = path.join(root, "src-tauri", "tauri.updater.local.json");
const rawPubkey = (process.env.CODEXHUB_STABLE_UPDATER_PUBKEY ?? "").trim();

const fail = (message) => {
  console.error(`UPDATER CONFIG FAIL: ${message}`);
  process.exit(1);
};

const extractMinisignPublicKey = (value) =>
  value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find(
      (line) =>
        line &&
        !line.startsWith("untrusted comment") &&
        !line.startsWith("trusted comment") &&
        !line.includes("minisign public key")
    );

const normalizePubkey = (value) => {
  if (!value) return "";
  const decodedPubkey = Buffer.from(value, "base64").toString("utf8");
  if (decodedPubkey.includes("minisign public key")) {
    return extractMinisignPublicKey(decodedPubkey) ?? "";
  }
  if (value.includes("minisign public key") || value.includes("\n")) {
    return extractMinisignPublicKey(value) ?? "";
  }
  return value;
};

if (!rawPubkey) {
  fail("CODEXHUB_STABLE_UPDATER_PUBKEY is required for signed updater artifact builds.");
}
const pubkey = normalizePubkey(rawPubkey);
if (!/^[A-Za-z0-9+/=]{40,120}$/.test(pubkey)) {
  fail("CODEXHUB_STABLE_UPDATER_PUBKEY must be a Tauri updater public key or a generated `.pub` file value.");
}
if (!fs.existsSync(sourcePath)) {
  fail(`missing source updater config: ${path.relative(root, sourcePath)}`);
}

const config = JSON.parse(fs.readFileSync(sourcePath, "utf8"));
config.plugins = {
  ...(config.plugins ?? {}),
  updater: {
    ...(config.plugins?.updater ?? {}),
    pubkey
  }
};

fs.writeFileSync(outputPath, `${JSON.stringify(config, null, 2)}\n`, "utf8");
console.log(`Updater Tauri config: ${path.relative(root, outputPath)}`);
