import { invoke } from "@tauri-apps/api/core";
import type {
  ConnectionTest,
  Health,
  Host,
  HostDraft,
  HostPatch,
  Profile,
  SkillPack,
  SshConfigHost,
  SshConfigWriteResult,
  SshHostDraft,
  SshKeyGenerationResult,
  SshStatus,
  TaskRun
} from "./models";
import type { AppSettings } from "./settings";
import { loadLocalSettings, normalizeSettings, saveLocalSettings } from "./settings";

export const fallbackHealth: Health = {
  app: "CodexHub",
  mode: "web-mock",
  remoteWrapperRequired: false
};

export const fallbackHosts: Host[] = [
  {
    id: "mac-studio-lab",
    name: "Mac Studio Lab",
    address: "10.0.8.12",
    port: 22,
    username: "jurio",
    authMethod: "ssh-key",
    status: "online",
    os: "macOS 15.5",
    codexVersion: "0.32.0",
    profileId: "research-default",
    skillPackIds: ["paper-review", "tauri-builder"],
    tags: ["local", "gpu"],
    lastSeen: "2 min ago",
    latencyMs: 18
  },
  {
    id: "win-workstation",
    name: "Windows Workstation",
    address: "192.168.31.42",
    port: 22,
    username: "pc",
    authMethod: "agent",
    status: "unknown",
    os: "Windows 11 Pro",
    codexVersion: "pending",
    profileId: "safe-editing",
    skillPackIds: ["tauri-builder"],
    tags: ["desktop", "primary"],
    lastSeen: "not tested",
    latencyMs: null
  },
  {
    id: "linux-runner",
    name: "Linux Runner",
    address: "172.20.4.8",
    port: 2222,
    username: "codex",
    authMethod: "ssh-key",
    status: "offline",
    os: "Ubuntu 24.04 LTS",
    codexVersion: "0.31.1",
    profileId: null,
    skillPackIds: ["paper-review"],
    tags: ["remote", "ci"],
    lastSeen: "yesterday",
    latencyMs: null
  }
];

export const fallbackProfiles: Profile[] = [
  {
    id: "research-default",
    name: "Research Default",
    description: "Balanced model and approval policy for literature review, repo browsing, and report drafting.",
    model: "gpt-5-codex",
    approvalPolicy: "on-request",
    sandboxMode: "workspace-write",
    updatedAt: "2026-06-24 22:10",
    hostIds: ["mac-studio-lab"]
  },
  {
    id: "safe-editing",
    name: "Safe Editing",
    description: "Conservative profile for protected repos: narrow write scope, explicit publish steps, and no private-state writes.",
    model: "gpt-5-codex",
    approvalPolicy: "on-failure",
    sandboxMode: "workspace-write",
    updatedAt: "2026-06-23 18:35",
    hostIds: ["win-workstation"]
  },
  {
    id: "diagnostics",
    name: "Diagnostics",
    description: "Read-mostly profile for host checks, logs, and environment inspection before any remediation.",
    model: "gpt-5-mini",
    approvalPolicy: "never",
    sandboxMode: "read-only",
    updatedAt: "2026-06-21 09:42",
    hostIds: []
  }
];

export const fallbackSkillPacks: SkillPack[] = [
  {
    id: "paper-review",
    name: "Paper Review",
    version: "0.4.1",
    description: "Summarize papers, extract claims, and prepare structured reading notes.",
    source: "~/.codex/skills/paper-review",
    skillCount: 5,
    enabled: true,
    updatedAt: "2026-06-24"
  },
  {
    id: "tauri-builder",
    name: "Tauri Builder",
    version: "0.2.0",
    description: "Scaffold, test, and package Tauri desktop features with React and Rust boundaries.",
    source: "./skills/tauri-builder",
    skillCount: 3,
    enabled: true,
    updatedAt: "2026-06-20"
  },
  {
    id: "windows-diagnostics",
    name: "Windows Diagnostics",
    version: "0.1.5",
    description: "Collect reproducible PowerShell checks for network, shell, and toolchain issues.",
    source: "./skills/windows-diagnostics",
    skillCount: 4,
    enabled: false,
    updatedAt: "2026-06-18"
  }
];

export const fallbackTasks: TaskRun[] = [
  {
    id: "task-1042",
    hostId: "mac-studio-lab",
    hostName: "Mac Studio Lab",
    action: "Apply profile",
    status: "success",
    startedAt: "2026-06-25 09:14",
    endedAt: "2026-06-25 09:15",
    summary: "Research Default rendered to ~/.codex/config.toml with backup codexhub-1042.toml.",
    logs: [
      {
        id: "log-1042-1",
        taskRunId: "task-1042",
        level: "info",
        timestamp: "09:14:10",
        message: "Opened SFTP session and created remote backup."
      },
      {
        id: "log-1042-2",
        taskRunId: "task-1042",
        level: "info",
        timestamp: "09:14:41",
        message: "Rendered profile preview matched expected TOML sections."
      }
    ]
  },
  {
    id: "task-1039",
    hostId: "linux-runner",
    hostName: "Linux Runner",
    action: "Test SSH connection",
    status: "failed",
    startedAt: "2026-06-24 22:02",
    endedAt: "2026-06-24 22:02",
    summary: "Connection timed out. Check VPN route or host firewall before applying profiles.",
    logs: [
      {
        id: "log-1039-1",
        taskRunId: "task-1039",
        level: "warn",
        timestamp: "22:02:18",
        message: "Mock check marks linux-runner offline for UI validation."
      }
    ]
  },
  {
    id: "task-1035",
    hostId: "win-workstation",
    hostName: "Windows Workstation",
    action: "Sync skill pack",
    status: "queued",
    startedAt: "2026-06-24 18:25",
    endedAt: null,
    summary: "Queued Paper Review skill pack for the next available SSH session.",
    logs: [
      {
        id: "log-1035-1",
        taskRunId: "task-1035",
        level: "info",
        timestamp: "18:25:00",
        message: "Task created from mock backend reservation."
      }
    ]
  }
];

const fallbackConnection: ConnectionTest = {
  ok: true,
  latencyMs: 24,
  message: "Mock SSH handshake completed."
};

export const fallbackSshStatus: SshStatus = {
  sshDir: "%USERPROFILE%\\.ssh",
  configPath: "%USERPROFILE%\\.ssh\\config",
  sshKeygenAvailable: false,
  preferredIdentityFile: "%USERPROFILE%\\.ssh\\id_ed25519",
  ed25519: {
    keyType: "ed25519",
    privatePath: "%USERPROFILE%\\.ssh\\id_ed25519",
    publicPath: "%USERPROFILE%\\.ssh\\id_ed25519.pub",
    privateExists: false,
    publicExists: false,
    publicKey: null
  },
  rsa: {
    keyType: "rsa",
    privatePath: "%USERPROFILE%\\.ssh\\id_rsa",
    publicPath: "%USERPROFILE%\\.ssh\\id_rsa.pub",
    privateExists: false,
    publicExists: false,
    publicKey: null
  }
};

async function safeInvoke<T>(command: string, args: Record<string, unknown> | undefined, fallback: T | (() => T)): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch {
    return typeof fallback === "function" ? (fallback as () => T)() : fallback;
  }
}

async function requiredInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw new Error(formatInvokeError(error));
  }
}

function formatInvokeError(error: unknown) {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "The Tauri desktop backend is required for this operation.";
}

const clone = <T,>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

export const api = {
  getHealth: () => safeInvoke<Health>("app_health", undefined, fallbackHealth),
  getSettings: () =>
    safeInvoke<AppSettings>("get_settings", undefined, () => loadLocalSettings()).then((settings) => {
      const normalized = normalizeSettings(settings);
      saveLocalSettings(normalized);
      return normalized;
    }),
  saveSettings: (settings: AppSettings) => {
    const normalized = normalizeSettings(settings);
    saveLocalSettings(normalized);
    return safeInvoke<AppSettings>("save_settings", { settings: normalized }, normalized).then((saved) => {
      const nextSettings = normalizeSettings(saved);
      saveLocalSettings(nextSettings);
      return nextSettings;
    });
  },
  getSshStatus: () => safeInvoke<SshStatus>("get_ssh_status", undefined, () => clone(fallbackSshStatus)),
  generateEd25519Key: () => requiredInvoke<SshKeyGenerationResult>("generate_ed25519_key"),
  listSshConfigHosts: () => safeInvoke<SshConfigHost[]>("list_ssh_config_hosts", undefined, []),
  upsertSshConfigHost: (draft: SshHostDraft) => requiredInvoke<SshConfigWriteResult>("upsert_ssh_config_host", { draft }),
  deleteSshConfigHost: (alias: string) => requiredInvoke<SshConfigWriteResult>("delete_ssh_config_host", { alias }),
  listHosts: () => safeInvoke<Host[]>("list_hosts", undefined, () => clone(fallbackHosts)),
  addHost: (draft: HostDraft) =>
    safeInvoke<Host>("add_host", { draft }, () => ({
      id: `mock-host-${Date.now()}`,
      name: draft.name,
      address: draft.address,
      port: draft.port,
      username: draft.username,
      authMethod: draft.authMethod,
      status: "unknown",
      os: "Unknown",
      codexVersion: "pending",
      profileId: null,
      skillPackIds: [],
      tags: draft.tags,
      lastSeen: "just added",
      latencyMs: null
    })),
  updateHost: (id: string, patch: HostPatch) =>
    safeInvoke<Host>("update_host", { id, patch }, () => ({
      ...(fallbackHosts.find((host) => host.id === id) ?? fallbackHosts[0]),
      ...patch
    })),
  deleteHost: (id: string) => safeInvoke<boolean>("delete_host", { id }, true),
  testSshConnection: (id: string) => safeInvoke<ConnectionTest>("test_ssh_connection", { id }, fallbackConnection),
  listProfiles: () => safeInvoke<Profile[]>("list_profiles", undefined, () => clone(fallbackProfiles)),
  listSkillPacks: () => safeInvoke<SkillPack[]>("list_skill_packs", undefined, () => clone(fallbackSkillPacks)),
  applyProfile: (profileId: string, hostIds: string[]) =>
    safeInvoke<TaskRun>("apply_profile", { profileId, hostIds }, () => {
      const host = fallbackHosts.find((item) => item.id === hostIds[0]) ?? fallbackHosts[0];
      const profile = fallbackProfiles.find((item) => item.id === profileId) ?? fallbackProfiles[0];

      return {
        id: `mock-task-${Date.now()}`,
        hostId: host.id,
        hostName: host.name,
        action: "Apply profile",
        status: "success",
        startedAt: "now",
        endedAt: "now",
        summary: `${profile.name} applied to ${host.name} through mock command.`,
        logs: [
          {
            id: `mock-log-${Date.now()}`,
            taskRunId: "mock-task",
            level: "info",
            timestamp: "now",
            message: "Frontend fallback completed without a Tauri runtime."
          }
        ]
      };
    }),
  listTasks: () => safeInvoke<TaskRun[]>("list_tasks", undefined, () => clone(fallbackTasks))
};
