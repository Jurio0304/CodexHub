export type RuntimePlatform = "windows" | "macos" | "linux";

export type PlatformPathOptions = {
  platform?: RuntimePlatform;
  homeDir?: string;
  detectedCodexPath?: string | null;
};

function normalizedNavigatorPlatform() {
  if (typeof navigator === "undefined") return "";
  const userAgentDataPlatform = (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData?.platform;
  return `${userAgentDataPlatform ?? navigator.platform ?? navigator.userAgent ?? ""}`.toLowerCase();
}

export function getPlatform(platformHint = normalizedNavigatorPlatform()): RuntimePlatform {
  const hint = platformHint.toLowerCase();
  if (hint.includes("mac")) return "macos";
  if (hint.includes("win")) return "windows";
  return "linux";
}

export function isWindows(platform: RuntimePlatform = getPlatform()) {
  return platform === "windows";
}

export function isMacOS(platform: RuntimePlatform = getPlatform()) {
  return platform === "macos";
}

export function isLinux(platform: RuntimePlatform = getPlatform()) {
  return platform === "linux";
}

export function getHomeDir(options: PlatformPathOptions = {}) {
  const platform = options.platform ?? getPlatform();
  if (options.homeDir) return trimTrailingSlash(options.homeDir);
  return platform === "windows" ? "%USERPROFILE%" : "~";
}

export function getSshDir(options: PlatformPathOptions = {}) {
  return joinPlatformPath(getHomeDir(options), ".ssh", options.platform ?? getPlatform());
}

export function getSshConfigPath(options: PlatformPathOptions = {}) {
  return joinPlatformPath(getSshDir(options), "config", options.platform ?? getPlatform());
}

export function getDefaultSshKeyPath(options: PlatformPathOptions = {}) {
  return joinPlatformPath(getSshDir(options), "id_ed25519", options.platform ?? getPlatform());
}

export function getCodexConfigPath(options: PlatformPathOptions = {}) {
  return joinPlatformPath(getHomeDir(options), ".codex/config.toml", options.platform ?? getPlatform());
}

export function getCodexSkillsPath(options: PlatformPathOptions = {}) {
  return joinPlatformPath(getHomeDir(options), ".codex/skills", options.platform ?? getPlatform());
}

export function getCodexBinaryCandidates(options: PlatformPathOptions = {}) {
  const platform = options.platform ?? getPlatform();
  const homeDir = getHomeDir({ ...options, platform });
  if (platform === "macos") {
    return ["/opt/homebrew/bin/codex", "/usr/local/bin/codex", joinPlatformPath(homeDir, ".local/bin/codex", platform), "which codex"];
  }
  if (platform === "windows") {
    return [
      joinPlatformPath(homeDir, ".local/bin/codex.exe", platform),
      joinPlatformPath(homeDir, "AppData/Roaming/npm/codex.cmd", platform),
      "where codex"
    ];
  }
  return [
    joinPlatformPath(homeDir, ".local/bin/codex", platform),
    joinPlatformPath(homeDir, ".npm-global/bin/codex", platform),
    "which codex"
  ];
}

export function detectCodexBinaryPath(options: PlatformPathOptions = {}) {
  if (options.detectedCodexPath) return options.detectedCodexPath;
  return getCodexBinaryCandidates(options)[0];
}

function joinPlatformPath(base: string, suffix: string, platform: RuntimePlatform) {
  const normalizedBase = trimTrailingSlash(base);
  const separator = platform === "windows" ? "\\" : "/";
  return `${normalizedBase}${separator}${suffix.replace(/[\\/]+/g, separator)}`;
}

function trimTrailingSlash(value: string) {
  return value.replace(/[\\/]+$/, "");
}
