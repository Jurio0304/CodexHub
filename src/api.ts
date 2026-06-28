import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type {
  ConnectionTest,
  Health,
  Host,
  HostDraft,
  HostPatch,
  LatestCodexVersion,
  CcSwitchDetection,
  Profile,
  ProfileApplyBatchResult,
  ProfileApplyPreview,
  ProfileDraft,
  ProfileImportExport,
  ProfilePatch,
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

export const fallbackHosts: Host[] = [];

export const fallbackLatestCodexVersion: LatestCodexVersion = {
  version: "0.32.0",
  checkedAt: "mock",
  source: "npm",
  error: null
};

export const fallbackProfiles: Profile[] = [];

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

export const fallbackTasks: TaskRun[] = [];

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

export const fallbackSshConfigHosts: SshConfigHost[] = [];

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

let mockProfiles = clone(fallbackProfiles);

function nowStamp() {
  return new Date().toISOString().slice(0, 16).replace("T", " ");
}

function slugifyProfileName(name: string) {
  const slug = name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug || `profile-${Date.now()}`;
}

function uniqueProfileId(name: string) {
  const base = slugifyProfileName(name);
  let candidate = base;
  let index = 2;
  while (mockProfiles.some((profile) => profile.id === candidate)) {
    candidate = `${base}-${index}`;
    index += 1;
  }
  return candidate;
}

function normalizeProfile(profile: Profile): Profile {
  return {
    ...profile,
    provider: profile.provider || "openai",
    baseUrl: profile.baseUrl || "https://api.openai.com/v1",
    apiKeyEnvVar: profile.apiKeyEnvVar || "OPENAI_API_KEY",
    modelReasoningEffort: profile.modelReasoningEffort || "medium",
    planModeReasoningEffort: profile.planModeReasoningEffort || "high",
    serviceTier: profile.serviceTier || "auto",
    extraToml: profile.extraToml || "",
    createdAt: profile.createdAt || profile.updatedAt || nowStamp(),
    updatedAt: profile.updatedAt || nowStamp(),
    source: profile.source || "imported",
    credentialStored: Boolean(profile.credentialStored),
    hostIds: profile.hostIds ?? []
  };
}

function createMockProfile(draft: ProfileDraft): Profile {
  const timestamp = nowStamp();
  return {
    id: uniqueProfileId(draft.name),
    ...draft,
    createdAt: timestamp,
    updatedAt: timestamp,
    source: "managed",
    credentialStored: false
  };
}

function normalizeProfileApplyResult(result: ProfileApplyBatchResult): ProfileApplyBatchResult {
  return {
    ...result,
    profiles: (result.profiles ?? []).map(normalizeProfile),
    hosts: result.hosts ?? []
  };
}

function renderProfileToml(profile: Profile) {
  const lines = [
    `model = "${escapeToml(profile.model)}"`,
    `model_provider = "${escapeToml(profile.provider)}"`,
    `approval_policy = "${escapeToml(profile.approvalPolicy)}"`,
    `sandbox_mode = "${escapeToml(profile.sandboxMode)}"`,
    `model_reasoning_effort = "${escapeToml(profile.modelReasoningEffort)}"`,
    `plan_mode_reasoning_effort = "${escapeToml(profile.planModeReasoningEffort)}"`,
    `service_tier = "${escapeToml(profile.serviceTier)}"`,
    "",
    "[features]",
    `fast_mode = ${profile.fastMode ? "true" : "false"}`
  ];
  if (profile.provider === "openai") {
    if (profile.baseUrl && profile.baseUrl !== "https://api.openai.com/v1") {
      lines.splice(2, 0, `openai_base_url = "${escapeToml(profile.baseUrl)}"`);
    }
  } else {
    lines.push(
      "",
      `[model_providers.${profile.provider}]`,
      `name = "${escapeToml(profile.provider)}"`,
      `base_url = "${escapeToml(profile.baseUrl)}"`,
      `env_key = "${escapeToml(profile.apiKeyEnvVar)}"`
    );
  }
  if (profile.extraToml.trim()) {
    lines.push("", "# Extra TOML", profile.extraToml.trim());
  }
  return `${lines.join("\n")}\n`;
}

function escapeToml(value: string) {
  return value.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}

function fallbackHostForAlias(hostAlias: string): Host {
  const alias = hostAlias || "mock-host";
  return (
    fallbackHosts.find((item) => item.hostAlias === alias || item.id === alias) ?? {
      id: `mock-${slugifyProfileName(alias)}`,
      name: alias,
      hostAlias: alias,
      source: "mock",
      address: alias,
      port: 22,
      username: "codex",
      authMethod: "ssh-key",
      status: "unknown",
      os: "Unknown",
      arch: "Unknown",
      shell: "Unknown",
      path: null,
      pathHasLocalBin: null,
      codexInstalled: false,
      codexVersion: "pending",
      configExists: null,
      apiConfigName: null,
      apiConfigSource: null,
      skillsExists: null,
      skillsCount: null,
      profileId: null,
      skillPackIds: [],
      tags: [],
      lastSeen: "not tested",
      latencyMs: null
    }
  );
}

function mockPreviewProfileApply(profileId: string, hostIds: string[]): ProfileApplyPreview {
  const profile = mockProfiles.find((item) => item.id === profileId);
  if (!profile) {
    return {
      profileId,
      renderedToml: "",
      targetFiles: [],
      hostResults: []
    };
  }
  const targetHosts = fallbackHosts.filter((host) => hostIds.includes(host.id) || hostIds.includes(host.hostAlias));
  const renderedToml = renderProfileToml(profile);
  return {
    profileId: profile.id,
    renderedToml,
    targetFiles: targetHosts.map((host) => ({
      hostId: host.id,
      hostName: host.name,
      hostAlias: host.hostAlias,
      path: "~/.codex/config.toml",
      backupExpected: host.configExists !== false && host.profileId !== profile.id,
      noChangeExpected: host.profileId === profile.id
    })),
    hostResults: targetHosts.map((host) => ({
      hostId: host.id,
      hostName: host.name,
      hostAlias: host.hostAlias,
      status: "pending",
      targetPath: "~/.codex/config.toml",
      backupPath: host.configExists === false || host.profileId === profile.id ? null : "~/.codex/config.toml.codexhub.bak.mock",
      message: host.profileId === profile.id ? "Preview expects no changes." : "Preview expects backup then replace."
    }))
  };
}

function mockApplyProfile(profileId: string, hostIds: string[]): ProfileApplyBatchResult {
  const profile = mockProfiles.find((item) => item.id === profileId);
  if (!profile) {
    return {
      profileId,
      ok: false,
      results: [],
      tasks: [],
      profiles: clone(mockProfiles).map(normalizeProfile),
      hosts: clone(fallbackHosts)
    };
  }
  const targetHosts = fallbackHosts.filter((host) => hostIds.includes(host.id) || hostIds.includes(host.hostAlias));
  const tasks = targetHosts.map((host): TaskRun => {
    const noChange = host.profileId === profile.id;
    return {
      id: `mock-apply-${host.id}-${Date.now()}`,
      hostId: host.id,
      hostName: host.name,
      action: "Apply profile",
      status: "success",
      startedAt: "now",
      endedAt: "now",
      summary: noChange
        ? `${profile.name} already matches ${host.name}; no remote backup needed.`
        : `${profile.name} rendered to ~/.codex/config.toml with mock backup.`,
      logs: [
        {
          id: `mock-apply-log-${host.id}-${Date.now()}`,
          taskRunId: "mock-apply",
          level: "info",
          timestamp: "now",
          message: noChange ? "Remote config matched rendered TOML." : "Mock remote backup and replace completed.",
          command: `ssh ${host.hostAlias} apply-profile ${profile.id}`,
          stdout: noChange ? "no changes" : "config.toml updated",
          stderr: "",
          exitCode: 0,
          durationMs: 36,
          timedOut: false
        }
      ]
    };
  });
  const successfulHostIds = new Set(targetHosts.map((host) => host.id));
  const nextProfiles = mockProfiles.map((item) =>
    item.id === profileId
      ? normalizeProfile({ ...item, hostIds: Array.from(new Set([...item.hostIds, ...successfulHostIds])), updatedAt: nowStamp() })
      : normalizeProfile({ ...item, hostIds: item.hostIds.filter((hostId) => !successfulHostIds.has(hostId)) })
  );
  mockProfiles = nextProfiles;
  const nextHosts = fallbackHosts.map((host) =>
    successfulHostIds.has(host.id)
      ? { ...host, profileId, apiConfigName: profile.name, apiConfigSource: "profile", configExists: true, lastSeen: "just now" }
      : host
  );
  return {
    profileId: profile.id,
    ok: true,
    tasks,
    results: targetHosts.map((host, index) => ({
      hostId: host.id,
      hostName: host.name,
      hostAlias: host.hostAlias,
      status: host.profileId === profile.id ? "no-change" : "success",
      targetPath: "~/.codex/config.toml",
      backupPath: host.profileId === profile.id ? null : "~/.codex/config.toml.codexhub.bak.mock",
      message: tasks[index].summary,
      task: tasks[index]
    })),
    profiles: clone(nextProfiles).map(normalizeProfile),
    hosts: clone(nextHosts)
  };
}

function mockCcSwitchDetection(): CcSwitchDetection {
  const timestamp = nowStamp();
  const profile: Profile = {
    id: uniqueProfileId("cc-switch-import"),
    name: "cc-switch Import",
    description: "Imported from a detected cc-switch configuration.",
    model: "claude-3-5-sonnet-latest",
    provider: "anthropic",
    baseUrl: "https://api.anthropic.com",
    apiKeyEnvVar: "ANTHROPIC_API_KEY",
    modelReasoningEffort: "medium",
    planModeReasoningEffort: "high",
    fastMode: false,
    serviceTier: "auto",
    approvalPolicy: "on-request",
    sandboxMode: "workspace-write",
    extraToml: "",
    createdAt: timestamp,
    updatedAt: timestamp,
    source: "cc-switch",
    credentialStored: false,
    hostIds: []
  };
  return {
    detected: true,
    sourcePath: "~/.cc-switch/config.json",
    message: "Mock cc-switch profile detected.",
    importExport: {
      schemaVersion: 1,
      exportedAt: timestamp,
      profiles: [profile]
    }
  };
}

function mockSshCheck(hostAlias: string): SshCheckResult {
  const host = fallbackHostForAlias(hostAlias);
  const ok = false;
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
  const host = fallbackHostForAlias(hostAlias);
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
    latencyMs: host.latencyMs,
    os: host.os,
    arch: host.arch,
    shell: host.shell,
    path: host.path,
    pathHasLocalBin: Boolean(host.pathHasLocalBin),
    codexInstalled: host.codexInstalled,
    codexPath: host.codexInstalled ? "/usr/local/bin/codex" : null,
    codexVersion: host.codexVersion,
    configExists: Boolean(host.configExists),
    apiConfigName: host.configExists ? host.apiConfigName ?? "Unknown config" : "No config",
    apiConfigSource: host.configExists ? host.apiConfigSource ?? "unknown" : "none",
    skillsExists: Boolean(host.skillsExists),
    skillsCount: host.skillsCount ?? 0,
    task
  };
}

function mockRemoteManageCodex(hostAlias: string, action: RemoteCodexAction): RemoteCodexMaintenanceResult {
  const host = fallbackHostForAlias(hostAlias);
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
  const host = fallbackHostForAlias(hostAlias);
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
  refreshLatestCodexVersion: (force = false, timeoutMs = 30000) =>
    safeInvoke<LatestCodexVersion>("refresh_latest_codex_version", { force, timeoutMs }, () => ({
      ...fallbackLatestCodexVersion,
      checkedAt: new Date().toISOString()
    })),
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
      apiConfigName: null,
      apiConfigSource: null,
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
      ...fallbackHostForAlias(id),
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
  listProfiles: () => safeInvoke<Profile[]>("list_profiles", undefined, () => clone(mockProfiles)).then((profiles) => profiles.map(normalizeProfile)),
  createProfile: (draft: ProfileDraft) =>
    safeInvoke<Profile>("create_profile", { draft }, () => {
      const profile = createMockProfile(draft);
      mockProfiles = [...mockProfiles, profile];
      return clone(profile);
    }).then(normalizeProfile),
  updateProfile: (id: string, patch: ProfilePatch) =>
    safeInvoke<Profile>("update_profile", { id, patch }, () => {
      const current = mockProfiles.find((profile) => profile.id === id);
      if (!current) throw new Error(`Profile ${id} was not found.`);
      const next = normalizeProfile({ ...current, ...patch, updatedAt: nowStamp() });
      mockProfiles = mockProfiles.map((profile) => (profile.id === id ? next : profile));
      return clone(next);
    }).then(normalizeProfile),
  deleteProfile: (id: string) =>
    safeInvoke<boolean>("delete_profile", { id }, () => {
      mockProfiles = mockProfiles.filter((profile) => profile.id !== id);
      return true;
    }),
  duplicateProfile: (id: string) =>
    safeInvoke<Profile>("duplicate_profile", { id }, () => {
      const source = mockProfiles.find((profile) => profile.id === id);
      if (!source) throw new Error(`Profile ${id} was not found.`);
      const duplicate = normalizeProfile({
        ...source,
        id: uniqueProfileId(`${source.name} copy`),
        name: `${source.name} Copy`,
        createdAt: nowStamp(),
        updatedAt: nowStamp(),
        source: "managed",
        credentialStored: false
      });
      mockProfiles = [...mockProfiles, duplicate];
      return clone(duplicate);
    }).then(normalizeProfile),
  importProfiles: (bundle: ProfileImportExport) =>
    safeInvoke<ProfileImportExport>("import_profiles", { bundle }, () => {
      const imported = bundle.profiles.map((profile) =>
        normalizeProfile({
          ...profile,
          id: mockProfiles.some((item) => item.id === profile.id) ? uniqueProfileId(profile.name) : profile.id,
          updatedAt: nowStamp(),
          source: profile.source || "imported",
          credentialStored: false
        })
      );
      mockProfiles = [...mockProfiles, ...imported];
      return { schemaVersion: bundle.schemaVersion || 1, exportedAt: nowStamp(), profiles: clone(imported) };
    }).then((result) => ({ ...result, profiles: result.profiles.map(normalizeProfile) })),
  exportProfiles: (profileIds?: string[]) =>
    safeInvoke<ProfileImportExport>("export_profiles", { profileIds }, () => {
      const selected = profileIds?.length ? mockProfiles.filter((profile) => profileIds.includes(profile.id)) : mockProfiles;
      return { schemaVersion: 1, exportedAt: nowStamp(), profiles: clone(selected) };
    }).then((result) => ({ ...result, profiles: result.profiles.map(normalizeProfile) })),
  setProfileApiKey: async (profileId: string, apiKey: string) => {
    if (hasTauriRuntime()) {
      return requiredInvoke<Profile>("set_profile_api_key", { profileId, apiKey }).then(normalizeProfile);
    }
    const current = mockProfiles.find((item) => item.id === profileId);
    if (!current) throw new Error(`Profile ${profileId} was not found.`);
    const profile = normalizeProfile({
      ...current,
      credentialStored: Boolean(apiKey),
      updatedAt: nowStamp()
    });
    mockProfiles = mockProfiles.map((item) => (item.id === profileId ? profile : item));
    return clone(profile);
  },
  deleteProfileApiKey: async (profileId: string) => {
    if (hasTauriRuntime()) {
      return requiredInvoke<Profile>("delete_profile_api_key", { profileId }).then(normalizeProfile);
    }
    const current = mockProfiles.find((item) => item.id === profileId);
    if (!current) throw new Error(`Profile ${profileId} was not found.`);
    const profile = normalizeProfile({
      ...current,
      credentialStored: false,
      updatedAt: nowStamp()
    });
    mockProfiles = mockProfiles.map((item) => (item.id === profileId ? profile : item));
    return clone(profile);
  },
  previewProfileApply: (profileId: string, hostIds: string[]) =>
    safeInvoke<ProfileApplyPreview>("preview_profile_apply", { profileId, hostIds }, () => mockPreviewProfileApply(profileId, hostIds)),
  applyProfile: async (profileId: string, hostIds: string[]) => {
    if (hasTauriRuntime()) {
      return requiredInvoke<ProfileApplyBatchResult>("apply_profile", { profileId, hostIds }).then(normalizeProfileApplyResult);
    }
    return normalizeProfileApplyResult(mockApplyProfile(profileId, hostIds));
  },
  detectCcSwitchProfiles: () =>
    safeInvoke<CcSwitchDetection>("detect_cc_switch_profiles", undefined, () => mockCcSwitchDetection()).then((detection) => ({
      ...detection,
      importExport: { ...detection.importExport, profiles: detection.importExport.profiles.map(normalizeProfile) }
    })),
  importCcSwitchProfiles: (detection: CcSwitchDetection) =>
    safeInvoke<ProfileImportExport>("import_cc_switch_profiles", { detection }, () => {
      const imported = detection.importExport.profiles.map((profile) =>
        normalizeProfile({
          ...profile,
          id: mockProfiles.some((item) => item.id === profile.id) ? uniqueProfileId(profile.name) : profile.id,
          source: "cc-switch",
          credentialStored: false,
          updatedAt: nowStamp()
        })
      );
      mockProfiles = [...mockProfiles, ...imported];
      return { schemaVersion: 1, exportedAt: nowStamp(), profiles: clone(imported) };
    }).then((result) => ({ ...result, profiles: result.profiles.map(normalizeProfile) })),
  listSkillPacks: () => safeInvoke<SkillPack[]>("list_skill_packs", undefined, () => clone(fallbackSkillPacks)),
  listTasks: () => safeInvoke<TaskRun[]>("list_tasks", undefined, () => clone(fallbackTasks))
};
