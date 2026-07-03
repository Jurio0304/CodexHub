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

const decodeBase64Strict = (value) => {
  const normalized = value.replace(/\s+/g, "");
  if (!normalized || !/^[A-Za-z0-9+/]+={0,2}$/.test(normalized)) return null;
  try {
    const bytes = Buffer.from(normalized, "base64");
    const roundTrip = bytes.toString("base64").replace(/=+$/, "");
    if (roundTrip !== normalized.replace(/=+$/, "")) return null;
    return { bytes, normalized };
  } catch {
    return null;
  }
};

const publicKeyLineInfo = (value) => {
  const decoded = decodeBase64Strict(value);
  if (!decoded || decoded.bytes.length !== 42) return null;
  const [algorithm0, algorithm1] = decoded.bytes;
  if (algorithm0 !== 0x45 || (algorithm1 !== 0x64 && algorithm1 !== 0x44)) return null;
  const keyId = Array.from(decoded.bytes.slice(2, 10))
    .reverse()
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("")
    .toUpperCase();
  return { keyLine: decoded.normalized, keyId };
};

const extractMinisignPublicKeyLine = (value) =>
  value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find((line) => line && publicKeyLineInfo(line));

const normalizeMinisignPubFileText = (value) => {
  const keyLine = extractMinisignPublicKeyLine(value);
  if (!keyLine) return null;
  const comment =
    value
      .split(/\r?\n/)
      .map((line) => line.trim())
      .find((line) => line.includes("minisign public key")) ??
    `untrusted comment: minisign public key: ${publicKeyLineInfo(keyLine)?.keyId}`;
  return `${comment}\n${keyLine}\n`;
};

const encodePubFileText = (value) => Buffer.from(value, "utf8").toString("base64");

const normalizePubkey = (value) => {
  if (!value) return "";
  const trimmed = value.trim();
  const decoded = decodeBase64Strict(trimmed);
  if (decoded) {
    const decodedText = decoded.bytes.toString("utf8");
    if (decodedText.includes("minisign public key")) {
      const pubFileText = normalizeMinisignPubFileText(decodedText);
      return pubFileText ? encodePubFileText(pubFileText) : "";
    }
    const keyLine = publicKeyLineInfo(trimmed);
    if (keyLine) {
      return encodePubFileText(`untrusted comment: minisign public key: ${keyLine.keyId}\n${keyLine.keyLine}\n`);
    }
  }
  if (trimmed.includes("minisign public key") || trimmed.includes("\n")) {
    const pubFileText = normalizeMinisignPubFileText(trimmed);
    return pubFileText ? encodePubFileText(pubFileText) : "";
  }
  return "";
};

if (!rawPubkey) {
  fail("CODEXHUB_STABLE_UPDATER_PUBKEY is required for signed updater artifact builds.");
}
const pubkey = normalizePubkey(rawPubkey);
const decodedPubkey = decodeBase64Strict(pubkey);
if (!decodedPubkey || !normalizeMinisignPubFileText(decodedPubkey.bytes.toString("utf8"))) {
  fail("CODEXHUB_STABLE_UPDATER_PUBKEY must be a Tauri `.key.pub` value, raw minisign `.pub` text, or bare minisign public key line.");
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
