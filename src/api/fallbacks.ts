import type {
  AppUpdateStatus,
  ConnectionTest,
  Health,
  Host,
  LatestCodexVersion,
  LocalCodexStatus,
  NetworkProxyStatus,
  Profile,
  SkillPack,
  SshConfigHost,
  SshStatus,
  TaskRun
} from "../models";
import {
  detectCodexBinaryPath,
  getCodexBinaryCandidates,
  getDefaultSshKeyPath,
  getPlatform,
  getSshConfigPath,
  getSshDir
} from "../platform";

export const fallbackHealth: Health = {
  app: "CodexHub",
  mode: "web-mock",
  remoteWrapperRequired: false
};

export const fallbackAppUpdateStatus: AppUpdateStatus = {
  softwareName: "CodexHub Dev",
  channel: "dev",
  currentVersion: "0.2.3",
  installedAt: null,
  state: "disabled",
  configured: false,
  feedConfigured: false,
  signingConfigured: false,
  latestVersion: null,
  checkedAt: null,
  message: "Dev channel auto-updates are disabled. Use local builds, preview packages, or test artifacts."
};

export const fallbackHosts: Host[] = [];

export const fallbackLatestCodexVersion: LatestCodexVersion = {
  version: "0.32.0",
  checkedAt: "mock",
  source: "npm",
  error: null
};

export const fallbackNetworkProxyStatus: NetworkProxyStatus = {
  mode: "auto",
  proxyUrl: null,
  source: null,
  message: "The desktop backend is required to detect local proxy ports.",
  candidates: []
};

export function fallbackLocalCodexStatus(): LocalCodexStatus {
  const platform = getPlatform();
  return {
    platform,
    detected: false,
    path: null,
    version: null,
    searchPaths: getCodexBinaryCandidates({ platform }),
    installHint:
      platform === "macos"
        ? "Install Codex CLI with the official OpenAI/Codex installer, then ensure /opt/homebrew/bin, /usr/local/bin, or ~/.local/bin is on PATH."
        : `Expected Codex CLI near ${detectCodexBinaryPath({ platform })}; install it with the official OpenAI/Codex installer, then refresh.`
  };
}

export const fallbackProfiles: Profile[] = [];

export const fallbackSkillPacks: SkillPack[] = [];

export const fallbackTasks: TaskRun[] = [];

export const fallbackConnection: ConnectionTest = {
  ok: true,
  latencyMs: 24,
  message: "Mock SSH handshake completed."
};

export function fallbackSshStatus(): SshStatus {
  const platform = getPlatform();
  const sshDir = getSshDir({ platform });
  const ed25519Path = getDefaultSshKeyPath({ platform });
  const rsaPath = platform === "windows" ? `${sshDir}\\id_rsa` : `${sshDir}/id_rsa`;
  return {
    sshDir,
    configPath: getSshConfigPath({ platform }),
    sshKeygenAvailable: false,
    preferredIdentityFile: ed25519Path,
    ed25519: {
      keyType: "ed25519",
      privatePath: ed25519Path,
      publicPath: `${ed25519Path}.pub`,
      privateExists: false,
      publicExists: false,
      publicKey: null
    },
    rsa: {
      keyType: "rsa",
      privatePath: rsaPath,
      publicPath: `${rsaPath}.pub`,
      privateExists: false,
      publicExists: false,
      publicKey: null
    }
  };
}

export const fallbackSshConfigHosts: SshConfigHost[] = [];
