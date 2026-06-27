import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type {
  ConnectionTest,
  Health,
  Host,
  HostDraft,
  HostPatch,
  Profile,
  RemoteCodexAction,
  RemoteCodexMaintenanceResult,
  RemoteCodexProgressEvent,
  RemoteProbeResult,
  SkillPack,
  SshBootstrapProgressEvent,
  SshBootstrapResult,
  SshCheckResult,
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
    hostAlias: "mac-studio-lab",
    source: "mock",
    address: "10.0.8.12",
    port: 22,
    username: "jurio",
    authMethod: "ssh-key",
    status: "online",
    os: "macOS 15.5",
    arch: "arm64",
    shell: "/bin/zsh",
    path: "/Users/jurio/.local/bin:/usr/local/bin:/usr/bin",
    pathHasLocalBin: true,
    codexInstalled: true,
    codexVersion: "0.32.0",
    configExists: true,
    skillsExists: true,
    skillsCount: 5,
    profileId: "research-default",
    skillPackIds: ["paper-review", "tauri-builder"],
    tags: ["local", "gpu"],
    lastSeen: "2 min ago",
    latencyMs: 18
  },
  {
    id: "win-workstation",
    name: "Windows Workstation",
    hostAlias: "win-workstation",
    source: "mock",
    address: "192.168.31.42",
    port: 22,
    username: "pc",
    authMethod: "agent",
    status: "unknown",
    os: "Windows 11 Pro",
    arch: "x86_64",
    shell: "Unknown",
    path: null,
    pathHasLocalBin: null,
    codexInstalled: false,
    codexVersion: "pending",
    configExists: null,
    skillsExists: null,
    skillsCount: null,
    profileId: "safe-editing",
    skillPackIds: ["tauri-builder"],
    tags: ["desktop", "primary"],
    lastSeen: "not tested",
    latencyMs: null
  },
  {
    id: "linux-runner",
    name: "Linux Runner",
    hostAlias: "linux-runner",
    source: "mock",
    address: "172.20.4.8",
    port: 2222,
    username: "codex",
    authMethod: "ssh-key",
    status: "offline",
    os: "Ubuntu 24.04 LTS",
    arch: "x86_64",
    shell: "/bin/bash",
    path: "/home/codex/.local/bin:/usr/local/bin:/usr/bin",
    pathHasLocalBin: true,
    codexInstalled: true,
    codexVersion: "0.31.1",
    configExists: false,
    skillsExists: true,
    skillsCount: 1,
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

export const fallbackSshConfigHosts: SshConfigHost[] = [
  {
    alias: "mac-studio-lab",
    hostName: "10.0.8.12",
    port: 22,
    user: "jurio",
    identityFile: "~/.ssh/id_ed25519",
    managed: false,
    source: "unmanaged-readonly"
  },
  {
    alias: "linux-runner",
    hostName: "172.20.4.8",
    port: 2222,
    user: "codex",
    identityFile: "~/.ssh/id_ed25519",
    managed: true,
    source: "managed"
  }
];

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

function hasTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function formatInvokeError(error: unknown) {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "The Tauri desktop backend is required for this operation.";
}

const clone = <T,>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

function mockSshCheck(hostAlias: string): SshCheckResult {
  const host = fallbackHosts.find((item) => item.hostAlias === hostAlias || item.id === hostAlias) ?? fallbackHosts[0];
  const ok = host.id !== "linux-runner";
  const task: TaskRun = {
    id: `mock-ssh-${Date.now()}`,
    hostId: host.id,
    hostName: host.name,
    action: "Test SSH connection",
    status: ok ? "success" : "failed",
    startedAt: "now",
    endedAt: "now",
    summary: ok ? `Mock SSH connection to ${host.hostAlias} returned ok.` : `Mock SSH connection to ${host.hostAlias} timed out.`,
    logs: [
      {
        id: `mock-ssh-log-${Date.now()}`,
        taskRunId: "mock-ssh",
        level: ok ? "info" : "error",
        timestamp: "now",
        message: ok ? "ssh echo ok completed." : "ssh echo ok timed out.",
        command: `ssh ${host.hostAlias} echo ok`,
        stdout: ok ? "ok" : "",
        stderr: ok ? "" : "mock timeout",
        exitCode: ok ? 0 : null,
        durationMs: ok ? 24 : 10000,
        timedOut: !ok
      }
    ]
  };
  return {
    hostAlias: host.hostAlias,
    ok,
    latencyMs: ok ? 24 : null,
    message: task.summary,
    task
  };
}

function mockSshBootstrapHost(draft: SshHostDraft): SshBootstrapResult {
  const hostAlias = draft.alias || draft.hostName;
  const message = `Mock SSH bootstrap for ${hostAlias} completed; key login returned ok.`;
  const task: TaskRun = {
    id: `mock-bootstrap-${Date.now()}`,
    hostId: `ssh-${hostAlias}`,
    hostName: hostAlias,
    action: "Bootstrap SSH key",
    status: "success",
    startedAt: "now",
    endedAt: "now",
    summary: message,
    logs: [
      {
        id: `mock-bootstrap-log-${Date.now()}`,
        taskRunId: "mock-bootstrap",
        level: "info",
        timestamp: "now",
        message: "Mock password setup installed the public key.",
        command: `ssh password bootstrap ${draft.user}@${draft.hostName}:${draft.port} authorized_keys`,
        stdout: "authorized_keys updated",
        stderr: "",
        exitCode: 0,
        durationMs: 42,
        timedOut: false
      }
    ]
  };
  return {
    hostAlias,
    ok: true,
    latencyMs: 24,
    message,
    generatedKey: false,
    privateKeyPath: draft.identityFile,
    publicKeyPath: `${draft.identityFile}.pub`,
    writeResult: {
      changed: true,
      action: "added",
      configPath: "%USERPROFILE%\\.ssh\\config",
      backupPath: null,
      host: { ...draft, managed: true, source: "managed" },
      message: `Mock saved Host ${draft.alias}.`
    },
    task
  };
}

const bootstrapStepOrder: Array<{ step: SshBootstrapProgressEvent["step"]; message: string; detail: string }> = [
  { step: "password_login", message: "Password login succeeded.", detail: "mock password authentication succeeded" },
  { step: "install_public_key", message: "Public key installed.", detail: "authorized_keys updated" },
  { step: "set_permissions", message: "Remote SSH permissions set.", detail: "permissions set" },
  { step: "verify_alias_login", message: "Alias login returned ok.", detail: "ok" }
];

const delay = (ms: number) => new Promise((resolve) => window.setTimeout(resolve, ms));

async function mockSshBootstrapHostWithProgress(
  draft: SshHostDraft,
  requestId: string,
  onProgress?: (event: SshBootstrapProgressEvent) => void
): Promise<SshBootstrapResult> {
  const hostAlias = draft.alias || draft.hostName;
  for (const [index, item] of bootstrapStepOrder.entries()) {
    onProgress?.({
      requestId,
      hostAlias,
      step: item.step,
      status: "running",
      message: item.message.replace("succeeded", "running"),
      detail: null,
      stdout: null,
      stderr: null,
      exitCode: null,
      durationMs: null,
      timedOut: null
    });
    await delay(140);
    onProgress?.({
      requestId,
      hostAlias,
      step: item.step,
      status: "success",
      message: item.message,
      detail: item.detail,
      stdout: index === bootstrapStepOrder.length - 1 ? "ok" : item.detail,
      stderr: "",
      exitCode: 0,
      durationMs: 30 + index * 8,
      timedOut: false
    });
  }
  return mockSshBootstrapHost(draft);
}

function mockRemoteProbe(hostAlias: string): RemoteProbeResult {
  const host = fallbackHosts.find((item) => item.hostAlias === hostAlias || item.id === hostAlias) ?? fallbackHosts[0];
  const task: TaskRun = {
    id: `mock-probe-${Date.now()}`,
    hostId: host.id,
    hostName: host.name,
    action: "Probe remote system",
    status: "success",
    startedAt: "now",
    endedAt: "now",
    summary: `Mock probe completed for ${host.hostAlias}.`,
    logs: [
      {
        id: `mock-probe-log-${Date.now()}`,
        taskRunId: "mock-probe",
        level: "info",
        timestamp: "now",
        message: "uname -s completed.",
        command: `ssh ${host.hostAlias} sh -lc "uname -s"`,
        stdout: host.os,
        stderr: "",
        exitCode: 0,
        durationMs: 18,
        timedOut: false
      },
      {
        id: `mock-probe-log-${Date.now()}-path`,
        taskRunId: "mock-probe",
        level: "info",
        timestamp: "now",
        message: "echo $PATH completed.",
        command: `ssh ${host.hostAlias} sh -lc "printf '%s\\n' \\"$PATH\\""`,
        stdout: host.path ?? "",
        stderr: "",
        exitCode: 0,
        durationMs: 12,
        timedOut: false
      }
    ]
  };
  return {
    hostAlias: host.hostAlias,
    sshStatus: "online",
    os: host.os,
    arch: host.arch,
    shell: host.shell,
    path: host.path,
    pathHasLocalBin: Boolean(host.pathHasLocalBin),
    codexInstalled: host.codexInstalled,
    codexPath: host.codexInstalled ? "/usr/local/bin/codex" : null,
    codexVersion: host.codexVersion,
    configExists: Boolean(host.configExists),
    skillsExists: Boolean(host.skillsExists),
    skillsCount: host.skillsCount ?? 0,
    task
  };
}

function mockRemoteManageCodex(hostAlias: string, action: RemoteCodexAction): RemoteCodexMaintenanceResult {
  const host = fallbackHosts.find((item) => item.hostAlias === hostAlias || item.id === hostAlias) ?? fallbackHosts[0];
  const actionLabel =
    action === "check-version" ? "Check Codex version" : action === "install" ? "Install Codex" : "Update Codex";
  const nextVersion = host.codexInstalled && action === "check-version" ? host.codexVersion : "codex-cli 0.32.0";
  const message = `Mock ${actionLabel.toLowerCase()} completed for ${host.hostAlias}: ${nextVersion}.`;
  const task: TaskRun = {
    id: `mock-codex-${Date.now()}`,
    hostId: host.id,
    hostName: host.name,
    action: actionLabel,
    status: "success",
    startedAt: "now",
    endedAt: "now",
    summary: message,
    logs: [
      {
        id: `mock-codex-log-${Date.now()}`,
        taskRunId: "mock-codex",
        level: "info",
        timestamp: "now",
        message: "Mock remote Codex maintenance command completed.",
        command: `ssh ${host.hostAlias} codex maintenance ${action}`,
        stdout: nextVersion,
        stderr: "",
        exitCode: 0,
        durationMs: 48,
        timedOut: false
      }
    ]
  };
  return {
    hostAlias: host.hostAlias,
    ok: true,
    action,
    beforeVersion: host.codexInstalled ? host.codexVersion : null,
    afterVersion: nextVersion,
    codexPath: "$HOME/.local/bin/codex",
    installMethod: action === "check-version" ? null : "mock",
    pathChanged: action !== "check-version" && !host.pathHasLocalBin,
    shellConfigPath: action === "check-version" ? null : "$HOME/.bashrc",
    backupPath: action === "check-version" ? null : "$HOME/.bashrc.codexhub.bak.mock",
    message,
    task
  };
}

async function mockRemoteManageCodexWithProgress(
  hostAlias: string,
  action: RemoteCodexAction,
  requestId?: string,
  onProgress?: (event: RemoteCodexProgressEvent) => void
): Promise<RemoteCodexMaintenanceResult> {
  const host = fallbackHosts.find((item) => item.hostAlias === hostAlias || item.id === hostAlias) ?? fallbackHosts[0];
  const emit = (step: string, status: RemoteCodexProgressEvent["status"], message: string, line?: string) => {
    if (!requestId || !onProgress) return;
    onProgress({
      requestId,
      hostAlias: host.hostAlias,
      action,
      step,
      status,
      message,
      detail: step,
      stdout: status === "stdout" ? line ?? message : null,
      stderr: status === "stderr" ? line ?? message : null,
      exitCode: status === "success" ? 0 : null,
      durationMs: 24,
      timedOut: false
    });
  };

  emit("ssh-check", "running", `Checking SSH connection to ${host.hostAlias}.`);
  await delay(80);
  emit("ssh-check", "success", `SSH connection to ${host.hostAlias} returned ok.`);
  emit(action === "install" ? "Install Codex" : "Update Codex", "running", "Starting remote Codex maintenance.");
  await delay(100);
  emit(action === "install" ? "Install Codex" : "Update Codex", "stdout", "Downloading Codex package.", "Downloading Codex package.");
  await delay(100);
  emit("codex --version after maintenance", "stdout", "codex-cli 0.32.0", "codex-cli 0.32.0");
  await delay(60);
  const result = mockRemoteManageCodex(hostAlias, action);
  emit("summary", result.ok ? "success" : "failed", result.message);
  return result;
}

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
  listSshConfigHosts: () => safeInvoke<SshConfigHost[]>("list_ssh_config_hosts", undefined, () => clone(fallbackSshConfigHosts)),
  upsertSshConfigHost: (draft: SshHostDraft) => requiredInvoke<SshConfigWriteResult>("upsert_ssh_config_host", { draft }),
  deleteSshConfigHost: (alias: string) => requiredInvoke<SshConfigWriteResult>("delete_ssh_config_host", { alias }),
  listHosts: () => safeInvoke<Host[]>("list_hosts", undefined, () => clone(fallbackHosts)),
  refreshDiscoveredHosts: () => safeInvoke<Host[]>("refresh_discovered_hosts", undefined, () => clone(fallbackHosts)),
  addHost: (draft: HostDraft) =>
    safeInvoke<Host>("add_host", { draft }, () => ({
      id: `mock-host-${Date.now()}`,
      name: draft.name,
      hostAlias: draft.address,
      source: "manual",
      address: draft.address,
      port: draft.port,
      username: draft.username,
      authMethod: draft.authMethod,
      status: "unknown",
      os: "Unknown",
      arch: "Unknown",
      shell: "Unknown",
      path: null,
      pathHasLocalBin: null,
      codexInstalled: false,
      codexVersion: "pending",
      configExists: null,
      skillsExists: null,
      skillsCount: null,
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
  sshCheck: (hostAlias: string, timeoutMs = 10000) =>
    safeInvoke<SshCheckResult>("ssh_check", { hostAlias, timeoutMs }, () => mockSshCheck(hostAlias)),
  connectSshHost: async (
    draft: SshHostDraft,
    password: string,
    requestId: string,
    onProgress?: (event: SshBootstrapProgressEvent) => void,
    timeoutMs = 10000
  ) => {
    if (!hasTauriRuntime()) {
      return mockSshBootstrapHostWithProgress(draft, requestId, onProgress);
    }

    let unlisten: UnlistenFn | null = null;
    if (onProgress) {
      unlisten = await listen<SshBootstrapProgressEvent>("ssh-bootstrap-progress", (event) => {
        if (event.payload.requestId === requestId) onProgress(event.payload);
      });
    }

    try {
      return await requiredInvoke<SshBootstrapResult>("bootstrap_ssh_host", {
        draft,
        password,
        timeoutMs,
        requestId
      });
    } finally {
      unlisten?.();
    }
  },
  bootstrapSshHost: (draft: SshHostDraft, password: string, timeoutMs = 10000) =>
    safeInvoke<SshBootstrapResult>("bootstrap_ssh_host", { draft, password, timeoutMs }, () => mockSshBootstrapHost(draft)),
  bootstrapExistingSshHost: (hostAlias: string, password: string, timeoutMs = 10000) =>
    safeInvoke<SshBootstrapResult>("bootstrap_existing_ssh_host", { hostAlias, password, timeoutMs }, () =>
      mockSshBootstrapHost({
        alias: hostAlias,
        hostName: hostAlias,
        port: 22,
        user: "codex",
        identityFile: "%USERPROFILE%\\.ssh\\id_ed25519"
      })
    ),
  remoteProbeCodex: (hostAlias: string, timeoutMs = 10000) =>
    safeInvoke<RemoteProbeResult>("remote_probe_codex", { hostAlias, timeoutMs }, () => mockRemoteProbe(hostAlias)),
  remoteManageCodex: async (
    hostAlias: string,
    action: RemoteCodexAction,
    timeoutMs = 120000,
    requestId?: string,
    onProgress?: (event: RemoteCodexProgressEvent) => void
  ) => {
    if (!hasTauriRuntime()) {
      return mockRemoteManageCodexWithProgress(hostAlias, action, requestId, onProgress);
    }

    let unlisten: UnlistenFn | null = null;
    if (requestId && onProgress) {
      unlisten = await listen<RemoteCodexProgressEvent>("remote-codex-progress", (event) => {
        if (event.payload.requestId === requestId) onProgress(event.payload);
      });
    }

    try {
      return await requiredInvoke<RemoteCodexMaintenanceResult>("remote_manage_codex", {
        hostAlias,
        action,
        timeoutMs,
        requestId
      });
    } finally {
      unlisten?.();
    }
  },
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
