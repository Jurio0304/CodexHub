import type {
  CcSwitchDetection,
  Host,
  HostDraft,
  HostOperationProgressEvent,
  HostPatch,
  HostResourceBatchResult,
  HostResourceProgressEvent,
  InstalledSkillDownloadResult,
  InstalledSkillRequest,
  Profile,
  ProfileApiKeyResult,
  ProfileApplyBatchResult,
  ProfileApplyOptions,
  ProfileApplyPreview,
  ProfileDraft,
  ProfileImportExport,
  ProfilePatch,
  RemoteCodexAction,
  RemoteCodexBatchResult,
  RemoteCodexMaintenanceResult,
  RemoteProbeBatchResult,
  RemoteProbeBatchItemCompletedEvent,
  RemoteProbeResult,
  SkillDetectionResult,
  SkillInventoryStatus,
  SkillImportResult,
  SkillPack,
  SkillTargetOperationResult,
  SkillTargetRequest,
  SkillTargetsResult,
  SshBootstrapProgressEvent,
  SshBootstrapResult,
  SshCheckResult,
  SshConfigDeleteResult,
  SshHostDraft,
  TaskLog,
  TaskRun,
  TaskStep,
  TaskStepStatus
} from "../models";
import { getCodexSkillsPath, getPlatform } from "../platform";
import type { AppSettings, CloseButtonBehavior } from "../settings";
import { loadMockSettings, normalizeSettings, saveMockSettings } from "../settings";
import type { CodexHubApi } from "./contracts";
import type { TaskEvent } from "../generated/rust-contracts";
import { normalizeProfile, normalizeProfileApplyOptions, normalizeProfileApplyResult } from "./normalize";
import {
  fallbackAppUpdateStatus,
  fallbackConnection,
  fallbackHealth,
  fallbackHosts,
  fallbackLatestCodexVersion,
  fallbackLocalCodexStatus,
  fallbackNetworkProxyStatus,
  fallbackProfiles,
  fallbackSkillPacks,
  fallbackSshConfigHosts,
  fallbackSshStatus,
  fallbackTasks
} from "./fallbacks";
const clone = <T,>(value: T): T => JSON.parse(JSON.stringify(value)) as T;

let mockProfiles = clone(fallbackProfiles);
let mockHosts = clone(fallbackHosts);
let mockProfileCredentialIds = new Set<string>();
let mockSkillPacks = clone(fallbackSkillPacks);
let mockTasks = clone(fallbackTasks);
const mockAcknowledgedTaskIds = new Set<string>();
const mockTaskHandlers = new Set<(event: TaskEvent) => void>();
let mockSkillInventoryStatus: SkillInventoryStatus = {
  firstHostScanCompleted: false,
  localSkillRoot: getCodexSkillsPath({ platform: getPlatform() }),
  localSkills: [],
  hostInventories: []
};

function localSkillPath(skillId: string) {
  const platform = getPlatform();
  const root = getCodexSkillsPath({ platform });
  return platform === "windows" ? `${root}\\${skillId}` : `${root}/${skillId}`;
}

function mockTaskRun(hostId: string, hostName: string, action: string, summary: string, ok = true, command?: string): TaskRun {
  const taskId = `mock-task-${Date.now()}-${Math.random().toString(36).slice(2)}`;
  const timestamp = new Date().toISOString();
  return {
    id: taskId,
    hostId,
    hostName,
    action,
    status: ok ? "success" : "failed",
    startedAt: timestamp,
    endedAt: timestamp,
    summary,
    steps: [],
    logs: [
      {
        id: `${taskId}-log-1`,
        taskRunId: taskId,
        level: ok ? "info" : "error",
        timestamp,
        message: summary,
        command,
        stdout: ok ? "ok" : "",
        stderr: ok ? "" : summary,
        exitCode: ok ? 0 : 1,
        durationMs: 1,
        timedOut: false
      }
    ]
  };
}

function recordMockTask(task: TaskRun) {
  mockTasks = retainLatestMockTaskHistory([task, ...mockTasks.filter((item) => item.id !== task.id)]);
  const event: TaskEvent = {
    taskId: task.id,
    status: task.status,
    summary: task.summary,
    updatedAt: new Date().toISOString()
  };
  for (const handler of mockTaskHandlers) handler(clone(event));
}

function retainLatestMockTaskHistory(tasks: TaskRun[]) {
  const retained = [...tasks];
  for (let index = retained.length - 1; retained.length > 100 && index >= 0; index -= 1) {
    if (retained[index].status === "queued" || retained[index].status === "running") continue;
    retained.splice(index, 1);
  }
  return retained;
}

function nowStamp() {
  return new Date().toISOString().slice(0, 16).replace("T", " ");
}

function todayStamp() {
  return new Date().toISOString().slice(0, 10);
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
    mockHosts.find((item) => item.hostAlias === alias || item.id === alias) ?? {
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
      codexCommandAvailable: null,
      codexInstalled: false,
      codexVersion: "pending",
      configExists: null,
      apiConfigName: null,
      apiConfigSource: null,
      apiKeyEnvVar: null,
      apiKeyEnvPresent: null,
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
      profileName: profileId,
      renderedToml: "",
      targetFiles: [],
      hostResults: [],
      warnings: []
    };
  }
  const targetHosts = mockHosts.filter((host) => hostIds.includes(host.id) || hostIds.includes(host.hostAlias));
  const renderedToml = renderProfileToml(profile);
  return {
    profileId: profile.id,
    profileName: profile.name,
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
      message: host.profileId === profile.id ? "Preview expects no changes." : "Preview expects backup then replace.",
      reload: {
        mode: "app-services",
        status: "not-requested",
        targetedCount: 0,
        stoppedCount: 0,
        preservedCliCount: 0,
        replacementObserved: false,
        message: "Remote Codex reload is selected only after confirmation."
      }
    })),
    warnings: []
  };
}

function mockReloadResult(options: ProfileApplyOptions) {
  if (options.remoteCodexReloadMode === "none") {
    return {
      mode: "none" as const,
      status: "not-requested" as const,
      targetedCount: 0,
      stoppedCount: 0,
      preservedCliCount: 0,
      replacementObserved: false,
      message: "Configuration applied without reloading remote Codex processes."
    };
  }
  const allCodex = options.remoteCodexReloadMode === "all-codex";
  return {
    mode: options.remoteCodexReloadMode,
    status: "reconnected" as const,
    targetedCount: allCodex ? 2 : 1,
    stoppedCount: allCodex ? 2 : 1,
    preservedCliCount: allCodex ? 0 : 1,
    replacementObserved: true,
    message: allCodex
      ? "All confirmed remote Codex processes stopped and the App service reconnected."
      : "Remote Codex App service reconnected; interactive CLI sessions were preserved."
  };
}

function mockApplyProfile(profileId: string, hostIds: string[], options: ProfileApplyOptions): ProfileApplyBatchResult {
  const profile = mockProfiles.find((item) => item.id === profileId);
  if (!profile) {
    return {
      profileId,
      ok: false,
      outcome: "failed",
      results: [],
      tasks: [],
      profiles: clone(mockProfiles).map(normalizeProfile),
      hosts: clone(mockHosts)
    };
  }
  const targetHosts = mockHosts.filter((host) => hostIds.includes(host.id) || hostIds.includes(host.hostAlias));
  const tasks = targetHosts.map((host): TaskRun => {
    const noChange = host.profileId === profile.id;
    const taskId = `mock-apply-${host.id}-${Date.now()}`;
    const reload = mockReloadResult(options);
    return {
      id: taskId,
      hostId: host.id,
      hostName: host.name,
      action: "Apply profile",
      status: "success",
      startedAt: "now",
      endedAt: "now",
      summary: noChange
        ? `${profile.name} already matches ${host.name}; no remote backup needed.`
        : `${profile.name} rendered to ~/.codex/config.toml with mock backup.`,
      steps: [
        {
          taskRunId: taskId,
          stepId: "profile-apply",
          sequence: 0,
          status: "success",
          summary: noChange ? "Remote profile files already matched." : "Remote profile files were applied and verified.",
          startedAt: "now",
          endedAt: "now"
        },
        {
          taskRunId: taskId,
          stepId: "remote-codex-reload",
          sequence: 1,
          status: reload.status === "not-requested" ? "skipped" : "success",
          summary: reload.message,
          startedAt: reload.status === "not-requested" ? null : "now",
          endedAt: "now"
        }
      ],
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
  const nextHosts = mockHosts.map((host) =>
    successfulHostIds.has(host.id)
      ? { ...host, profileId, apiConfigName: profile.name, apiConfigSource: "profile", configExists: true, lastSeen: "just now" }
      : host
  );
  mockHosts = nextHosts;
  return {
    profileId: profile.id,
    ok: true,
    outcome: "success",
    tasks,
    results: targetHosts.map((host, index) => ({
      hostId: host.id,
      hostName: host.name,
      hostAlias: host.hostAlias,
      status: host.profileId === profile.id ? "no-change" : "success",
      targetPath: "~/.codex/config.toml",
      backupPath: host.profileId === profile.id ? null : "~/.codex/config.toml.codexhub.bak.mock",
      message: tasks[index].summary,
      reload: mockReloadResult(options),
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
    credentialStored: true,
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

function ccSwitchProfileKey(profile: Pick<Profile, "name" | "provider" | "model" | "baseUrl">) {
  return [
    profile.name.trim().toLowerCase(),
    profile.provider.trim().toLowerCase(),
    profile.model.trim().toLowerCase(),
    profile.baseUrl.trim().replace(/\/+$/, "").toLowerCase()
  ].join("|");
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
    steps: [],
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
        exitCode: ok ? 0 : undefined,
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
    steps: [],
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
      configPath: fallbackSshStatus().configPath,
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

function mockOperationLog(
  taskRunId: string,
  stepId: string,
  level: TaskLog["level"],
  message: string,
  command: string,
  stdout = "",
  stderr = "",
  exitCode = 0
): TaskLog {
  return {
    id: `${taskRunId}-${stepId}-${Math.random().toString(36).slice(2)}`,
    taskRunId,
    stepId,
    level,
    timestamp: new Date().toISOString(),
    message,
    command,
    stdout,
    stderr,
    exitCode,
    durationMs: 24,
    timedOut: false
  };
}

function mockOperationEmitter(
  requestId: string | undefined,
  taskId: string,
  hostAlias: string,
  operation: HostOperationProgressEvent["operation"],
  onProgress?: (event: HostOperationProgressEvent) => void
) {
  return (step: TaskStep, log?: TaskLog) => {
    if (!requestId || !onProgress) return;
    onProgress({ requestId, taskId, hostAlias, operation, step, log });
  };
}

async function runMockConcurrencyPool<T, R>(
  values: T[],
  worker: (value: T, index: number) => Promise<R>,
  concurrency = 6
) {
  const results = new Array<R>(values.length);
  let nextIndex = 0;
  // 滑动并发池在一个任务完成后立即领取下一项，同时保持结果输入顺序。
  const runners = Array.from({ length: Math.min(concurrency, values.length) }, async () => {
    while (nextIndex < values.length) {
      const index = nextIndex;
      nextIndex += 1;
      results[index] = await worker(values[index], index);
    }
  });
  await Promise.all(runners);
  return results;
}

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

function buildMockRemoteProbe(hostAlias: string): RemoteProbeResult {
  const host = fallbackHostForAlias(hostAlias);
  const taskId = `mock-probe-${Date.now()}-${Math.random().toString(36).slice(2)}`;
  const timestamp = new Date().toISOString();
  const probeTimedOut = hostAlias.toLowerCase().includes("timeout");
  const probeFailed = probeTimedOut || hostAlias.toLowerCase().includes("fail-probe") || hostAlias.toLowerCase().includes("offline");
  const failedProbeLog = {
    ...mockOperationLog(taskId, "ssh-check", "error", "Mock SSH connection failed.", `ssh ${host.hostAlias} echo ok`, "", probeTimedOut ? "mock timeout" : "mock connection refused", probeTimedOut ? undefined : 255),
    exitCode: probeTimedOut ? undefined : 255,
    durationMs: probeTimedOut ? 10000 : 24,
    timedOut: probeTimedOut
  };
  const task: TaskRun = {
    id: taskId,
    hostId: host.id,
    hostName: host.name,
    action: "Probe remote system",
    status: probeFailed ? "failed" : "success",
    startedAt: timestamp,
    endedAt: timestamp,
    summary: probeFailed ? `Mock SSH probe failed for ${host.hostAlias}.` : `Mock probe completed for ${host.hostAlias}.`,
    steps: ["ssh-check", "system", "codex", "api", "skills"].map((stepId, sequence) => ({
      taskRunId: taskId,
      stepId,
      sequence,
      status: probeFailed ? (sequence === 0 ? "failed" as const : "skipped" as const) : "success" as const,
      summary: probeFailed ? (sequence === 0 ? "Mock SSH connection failed." : "Skipped because SSH was unavailable.") : `Mock ${stepId} probe completed.`,
      startedAt: sequence === 0 || !probeFailed ? timestamp : null,
      endedAt: timestamp
    })),
    logs: probeFailed ? [failedProbeLog] : [
      {
        id: `mock-probe-log-${Date.now()}`,
        taskRunId: taskId,
        stepId: "system",
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
        taskRunId: taskId,
        stepId: "system",
        level: "info",
        timestamp: "now",
        message: "echo $PATH completed.",
        command: `ssh ${host.hostAlias} sh -lc "printf '%s\\n' \\"$PATH\\""`,
        stdout: host.path ?? "",
        stderr: "",
        exitCode: 0,
        durationMs: 12,
        timedOut: false
      },
      mockOperationLog(taskId, "ssh-check", "info", "SSH connection returned ok.", `ssh ${host.hostAlias} echo ok`, "ok"),
      mockOperationLog(taskId, "codex", "info", "Codex status probe completed.", `ssh ${host.hostAlias} codex-probe`, host.codexVersion),
      mockOperationLog(taskId, "api", "info", "API configuration probe completed.", `ssh ${host.hostAlias} api-config-probe`, host.configExists ? "configured" : "not configured"),
      mockOperationLog(taskId, "skills", "info", "Skills inventory probe completed.", `ssh ${host.hostAlias} skills-probe`, String(host.skillsCount ?? 0))
    ]
  };
  return {
    hostAlias: host.hostAlias,
    sshStatus: probeFailed ? "offline" : "online",
    latencyMs: probeFailed ? null : host.latencyMs,
    os: probeFailed ? "Unknown" : host.os,
    arch: probeFailed ? "Unknown" : host.arch,
    shell: probeFailed ? "Unknown" : host.shell,
    path: probeFailed ? null : host.path,
    pathHasLocalBin: !probeFailed && Boolean(host.pathHasLocalBin),
    codexCommandAvailable: !probeFailed && (host.codexCommandAvailable ?? Boolean(host.pathHasLocalBin && host.codexInstalled)),
    codexInstalled: !probeFailed && host.codexInstalled,
    codexPath: !probeFailed && host.codexInstalled ? "/usr/local/bin/codex" : null,
    codexVersion: probeFailed ? "unknown" : host.codexVersion,
    configExists: !probeFailed && Boolean(host.configExists),
    apiConfigName: !probeFailed && host.configExists ? host.apiConfigName ?? "Unknown config" : "No config",
    apiConfigSource: !probeFailed && host.configExists ? host.apiConfigSource ?? "unknown" : "none",
    apiKeyEnvVar: probeFailed ? null : host.apiKeyEnvVar ?? (host.configExists ? "OPENAI_API_KEY" : null),
    apiKeyEnvPresent: probeFailed ? null : host.apiKeyEnvPresent ?? (host.configExists ? true : null),
    skillsExists: !probeFailed && Boolean(host.skillsExists),
    skillsCount: probeFailed ? 0 : host.skillsCount ?? 0,
    task
  };
}

async function mockRemoteProbeWithProgress(
  hostAlias: string,
  requestId?: string,
  onProgress?: (event: HostOperationProgressEvent) => void
) {
  const result = buildMockRemoteProbe(hostAlias);
  const emit = mockOperationEmitter(requestId, result.task.id, result.hostAlias, "host-test", onProgress);
  const [sshStep, ...probeSteps] = result.task.steps;
  emit({ ...sshStep, status: "running", endedAt: null });
  await delay(35);
  emit(sshStep, result.task.logs.find((log) => log.stepId === sshStep.stepId));
  if (sshStep.status === "success") {
    // 四组只读探测先同时进入运行态，再独立完成。
    for (const step of probeSteps) emit({ ...step, status: "running", endedAt: null });
    await Promise.all(probeSteps.map(async (step, index) => {
      await delay(45 + index * 12);
      emit(step, result.task.logs.find((log) => log.stepId === step.stepId));
    }));
  } else {
    for (const step of probeSteps) emit(step);
  }
  recordMockTask(result.task);
  return clone(result);
}

function mockHostResourceSnapshot(hostAlias: string, index: number): HostResourceBatchResult["snapshots"][number] {
  const host = fallbackHostForAlias(hostAlias);
  if (hostAlias.toLowerCase().includes("timeout")) {
    return {
      hostAlias: host.hostAlias,
      status: "failed",
      sshStatus: "offline",
      timedOut: true,
      sampledAt: new Date().toISOString(),
      latencyMs: null,
      error: "Resource sampling timed out after 10000 ms.",
      cpu: null,
      memory: null,
      gpuTool: "none",
      gpus: []
    };
  }
  const profile = index % 3;
  const nvidia = profile !== 1;
  const gpuCount = profile === 2 ? 4 : nvidia ? 2 : 1;
  const nvidiaGpus = Array.from({ length: gpuCount }, (_, gpuIndex) => {
    const uuid = `GPU-mock-${index}-${gpuIndex}`;
    const busy = profile === 2 && gpuIndex < 2;
    return {
      vendor: "nvidia" as const,
      index: String(gpuIndex),
      uuid,
      name: profile === 2 ? "NVIDIA A800 80GB PCIe" : "NVIDIA RTX 4090",
      status: "ok" as const,
      utilizationPercent: busy ? 78 - gpuIndex * 12 : 6 + gpuIndex * 8,
      memoryUsedBytes: (busy ? 42_000 - gpuIndex * 8_000 : 1_024 + gpuIndex * 512) * 1024 ** 2,
      memoryTotalBytes: (profile === 2 ? 81_920 : 24_576) * 1024 ** 2,
      temperatureC: busy ? 68 - gpuIndex : 42 + gpuIndex,
      powerWatts: busy ? 252.4 - gpuIndex * 18 : 62.5 + gpuIndex * 12,
      driverVersion: "550.54",
      processes: busy
        ? [
            {
              gpuUuid: uuid,
              pid: 4100 + index * 10 + gpuIndex,
              name: "python",
              usedMemoryBytes: (18_432 - gpuIndex * 2_048) * 1024 ** 2,
              user: "amax",
              elapsedSeconds: 3_600 + index * 300 + gpuIndex * 90,
              command: `python train_${index}_${gpuIndex}.py --batch-size 8 --precision bf16`
            },
            {
              gpuUuid: uuid,
              pid: 5100 + index * 10 + gpuIndex,
              name: "python",
              usedMemoryBytes: (2_048 + gpuIndex * 512) * 1024 ** 2,
              user: gpuIndex === 0 ? "jy" : "codex",
              elapsedSeconds: 940 + gpuIndex * 180,
              command: "python serve.py --port 8000"
            }
          ]
        : []
    };
  });
  return {
    hostAlias: host.hostAlias,
    status: profile === 1 ? "partial" : "ok",
    sshStatus: "online",
    timedOut: false,
    sampledAt: new Date().toISOString(),
    latencyMs: 35 + index * 18,
    error: null,
    cpu: {
      usagePercent: nvidia ? 42.7 : 18.3,
      load1: nvidia ? 1.46 : 0.38,
      load5: nvidia ? 1.08 : 0.42,
      load15: nvidia ? 0.91 : 0.39,
      cores: nvidia ? 32 : 16,
      model: nvidia ? "AMD EPYC mock node" : "Intel Xeon mock node"
    },
    memory: {
      totalBytes: nvidia ? 256 * 1024 ** 3 : 128 * 1024 ** 3,
      availableBytes: nvidia ? 114 * 1024 ** 3 : 92 * 1024 ** 3,
      usedPercent: nvidia ? 55.5 : 28.1
    },
    gpuTool: nvidia ? "nvidia-smi" : "lspci",
    gpus: nvidia
      ? nvidiaGpus
      : [{
          vendor: "intel",
          index: null,
          uuid: null,
          name: "Intel display controller detected by lspci",
          status: "detected",
          utilizationPercent: null,
          memoryUsedBytes: null,
          memoryTotalBytes: null,
          temperatureC: null,
          powerWatts: null,
          driverVersion: null,
          processes: []
        }]
  };
}

function mockResourceSnapshotLogMessage(snapshot: HostResourceBatchResult["snapshots"][number]) {
  const duration = snapshot.latencyMs === null ? "" : ` in ${snapshot.latencyMs} ms`;
  if (snapshot.status === "ok") {
    return `Host ${snapshot.hostAlias}: resource sampling succeeded${duration}; ${snapshot.gpus.length} GPU(s) detected.`;
  }
  if (snapshot.status === "partial") {
    return `Host ${snapshot.hostAlias}: resource sampling completed with partial data${duration}; ${snapshot.gpus.length} GPU(s) detected.`;
  }
  const detail = (snapshot.error ?? "Unknown resource sampling error.").replace(/\s+/g, " ").trim().slice(0, 240);
  return `Host ${snapshot.hostAlias}: resource sampling failed: ${detail}`;
}

function mockResourceSampleTask(result: HostResourceBatchResult) {
  const partial = result.snapshots.filter((snapshot) => snapshot.status === "partial").length;
  const failed = result.snapshots.filter((snapshot) => snapshot.status === "failed").length;
  const total = result.snapshots.length;
  const summary = `Sampled ${total} host(s): ${partial} partial, ${failed} failed.`;
  const task = mockTaskRun(
    "resource-monitor",
    `${total} host(s)`,
    "Sample host resources",
    summary,
    partial === 0 && failed === 0
  );
  task.logs = result.snapshots.map((snapshot, index) => ({
    id: `${task.id}-log-${index + 1}`,
    taskRunId: task.id,
    level: snapshot.status === "ok" ? "info" : snapshot.status === "partial" ? "warn" : "error",
    timestamp: task.startedAt,
    message: mockResourceSnapshotLogMessage(snapshot),
    durationMs: snapshot.latencyMs ?? (snapshot.timedOut ? 10000 : undefined),
    timedOut: snapshot.timedOut
  }));
  task.logs.push({
    id: `${task.id}-log-${task.logs.length + 1}`,
    taskRunId: task.id,
    level: partial > 0 || failed > 0 ? "warn" : "info",
    timestamp: task.startedAt,
    message: summary
  });
  return task;
}

function mockSampleHostResources(hostAliases: string[], recordTask = true): HostResourceBatchResult {
  const aliases = hostAliases.length > 0 ? hostAliases : ["mock-gpu-01", "mock-cpu-01", "mock-gpu-02"];
  const result: HostResourceBatchResult = {
    checkedAt: new Date().toISOString(),
    snapshots: aliases.map(mockHostResourceSnapshot)
  };
  if (recordTask) {
    recordMockTask(mockResourceSampleTask(result));
  }
  return result;
}

async function mockSampleHostResourcesWithProgress(
  hostAliases: string[],
  recordTask: boolean,
  requestId?: string,
  onProgress?: (event: HostResourceProgressEvent) => void
) {
  const result = mockSampleHostResources(hostAliases, recordTask);
  if (requestId && onProgress) {
    for (const snapshot of result.snapshots) {
      await delay(15);
      onProgress({ requestId, snapshot: clone(snapshot) });
    }
  }
  return result;
}

function mockRemoteManageCodex(hostAlias: string, action: RemoteCodexAction): RemoteCodexMaintenanceResult {
  const host = fallbackHostForAlias(hostAlias);
  const maintenanceFailed = action !== "check-version" && hostAlias.toLowerCase().includes("fail");
  const actionLabel =
    action === "check-version"
      ? "Check Codex version"
      : action === "install"
        ? "Install Codex"
        : action === "update"
          ? "Update Codex"
          : "Uninstall Codex";
  const beforeVersion = host.codexInstalled
    ? host.codexVersion
    : action === "uninstall"
      ? "codex-cli 0.31.0"
      : null;
  const nextVersion = action === "uninstall"
    ? (maintenanceFailed ? beforeVersion : null)
    : maintenanceFailed
      ? (action === "update" && host.codexInstalled ? host.codexVersion : null)
      : host.codexInstalled && action === "check-version"
        ? host.codexVersion
        : "codex-cli 0.32.0";
  const versionLabel = nextVersion ?? "not installed";
  const message = `Mock ${actionLabel.toLowerCase()} completed for ${host.hostAlias}: ${versionLabel}.`;
  const taskId = `mock-codex-${Date.now()}-${Math.random().toString(36).slice(2)}`;
  const timestamp = new Date().toISOString();
  const installStepIds = ["preparation", "official-installer", "remote-native-mirror", "remote-npm-mirror", "local-upload", "runtime-reconcile", "final-verification", "release-cleanup"];
  const uninstallStepIds = ["preparation", "uninstall", "final-verification"];
  const stepIds = action === "uninstall" ? uninstallStepIds : action === "check-version" ? ["codex"] : installStepIds;
  const statusForStep = (stepId: string): TaskStepStatus => {
    if (action === "uninstall" && maintenanceFailed) {
      return stepId === "preparation" ? "success" : "failed";
    }
    if (action === "install" || action === "update") {
      if (maintenanceFailed && stepId !== "preparation") return "failed";
      if (stepId === "official-installer" || stepId === "remote-native-mirror") return "failed";
      if (stepId === "local-upload") return "skipped";
    }
    return "success";
  };
  const steps = stepIds.map((stepId, sequence): TaskStep => ({
    taskRunId: taskId,
    stepId,
    sequence,
    status: statusForStep(stepId),
    summary: statusForStep(stepId) === "failed"
      ? action === "uninstall"
        ? stepId === "uninstall"
          ? "Mock uninstall command failed; Codex remains installed."
          : "Mock final verification found Codex still installed."
        : `Mock ${stepId} was unavailable; continuing to the next method.`
      : statusForStep(stepId) === "skipped"
        ? `Mock ${stepId} was not required after an earlier method succeeded.`
        : action === "update" && stepId === "release-cleanup"
          ? "Mock older runtimes moved to staged update backup mock-update-backup."
        : `Mock ${stepId} completed.`,
    startedAt: statusForStep(stepId) === "skipped" ? null : timestamp,
    endedAt: timestamp
  }));
  const logs = steps.flatMap((step) => step.status === "skipped" ? [] : [mockOperationLog(
    taskId,
    step.stepId,
    step.status === "failed" ? "error" : "info",
    step.summary,
    `ssh ${host.hostAlias} codex-maintenance ${action} ${step.stepId}`,
    step.status === "success" ? (step.stepId === "final-verification" ? versionLabel : "ok") : "",
    step.status === "failed"
      ? action === "uninstall" ? "mock uninstall failed; Codex is still present" : "mock method unavailable"
      : "",
    step.status === "failed" ? 1 : 0
  )]);
  const task: TaskRun = {
    id: taskId,
    hostId: host.id,
    hostName: host.name,
    action: actionLabel,
    status: maintenanceFailed ? "failed" : "success",
    startedAt: timestamp,
    endedAt: timestamp,
    summary: maintenanceFailed ? `Mock ${actionLabel.toLowerCase()} failed for ${host.hostAlias}.` : message,
    steps,
    logs
  };
  return {
    hostAlias: host.hostAlias,
    ok: !maintenanceFailed,
    action,
    beforeVersion,
    afterVersion: nextVersion,
    codexPath: (action === "uninstall" && !maintenanceFailed) || (maintenanceFailed && action === "install") ? null : "$HOME/.local/bin/codex",
    codexCommandAvailable: !(action === "uninstall" && !maintenanceFailed) && !(maintenanceFailed && action === "install"),
    installMethod: action === "check-version" || action === "uninstall" || maintenanceFailed ? null : "remote-npm-mirror",
    pathChanged: !maintenanceFailed && action !== "check-version" && action !== "uninstall" && !host.pathHasLocalBin,
    shellConfigPath: action === "check-version" || action === "uninstall" ? null : "$HOME/.bashrc",
    backupPath:
      action === "check-version"
        ? null
        : action === "uninstall"
          ? null
          : "$HOME/.bashrc.codexhub.bak.mock",
    message: maintenanceFailed ? `Mock ${actionLabel.toLowerCase()} failed for ${host.hostAlias}.` : message,
    task
  };
}

async function mockRemoteManageCodexWithProgress(
  hostAlias: string,
  action: RemoteCodexAction,
  requestId?: string,
  onProgress?: (event: HostOperationProgressEvent) => void
): Promise<RemoteCodexMaintenanceResult> {
  const result = mockRemoteManageCodex(hostAlias, action);
  if (action !== "check-version") {
    const operation = action === "install" ? "codex-install" : action === "update" ? "codex-update" : "codex-uninstall";
    const emit = mockOperationEmitter(requestId, result.task.id, result.hostAlias, operation, onProgress);
    for (const step of result.task.steps) {
      if (step.status === "skipped") {
        emit(step);
        continue;
      }
      emit({ ...step, status: "running", endedAt: null });
      await delay(45);
      emit(step, result.task.logs.find((log) => log.stepId === step.stepId));
    }
  }
  recordMockTask(result.task);
  return clone(result);
}

async function mockBatchRemoteProbeCodex(
  hostAliases: string[],
  requestId: string,
  onProgress?: (event: HostOperationProgressEvent) => void,
  onItemCompleted?: (event: RemoteProbeBatchItemCompletedEvent) => void
): Promise<RemoteProbeBatchResult> {
  const results = await runMockConcurrencyPool(hostAliases, async (hostAlias): Promise<RemoteProbeBatchResult["results"][number]> => {
    let item: RemoteProbeBatchResult["results"][number];
    try {
      const result = await mockRemoteProbeWithProgress(hostAlias, requestId, onProgress);
      item = { hostAlias, ok: result.task.status === "success", result };
    } catch (error) {
      item = { hostAlias, ok: false, error: error instanceof Error ? error.message : String(error) };
    }
    // 单台任务落定后立即通知 UI；并发池最终仍按输入顺序汇总。
    onItemCompleted?.({ requestId, item: clone(item) });
    return item;
  });
  return {
    requestId,
    latestCodexVersion: { ...fallbackLatestCodexVersion, checkedAt: new Date().toISOString() },
    results
  };
}

async function mockBatchRemoteUpdateCodex(
  hostAliases: string[],
  requestId: string,
  onProgress?: (event: HostOperationProgressEvent) => void
): Promise<RemoteCodexBatchResult> {
  const results = await runMockConcurrencyPool(hostAliases, async (hostAlias): Promise<RemoteCodexBatchResult["results"][number]> => {
    try {
      const result = await mockRemoteManageCodexWithProgress(hostAlias, "update", requestId, onProgress);
      return { hostAlias, ok: result.ok, result };
    } catch (error) {
      return { hostAlias, ok: false, error: error instanceof Error ? error.message : String(error) };
    }
  });
  return { requestId, action: "update", results };
}

function mockImportSkill(path: string): SkillImportResult {
  const name = path.split(/[\\/]/).filter(Boolean).pop() || `skill-${Date.now()}`;
  const id = slugifyProfileName(name);
  const skill: SkillPack = {
    id,
    name,
    version: "mock",
    description: `Imported from ${path}.`,
    about: `Imported from ${path}.`,
    sourceType: "local",
    source: path,
    originalPath: path,
    managedPath: `%APPDATA%\\CodexHub\\skills\\${id}`,
    hasSkillMd: true,
    skillCount: 1,
    enabled: true,
    addedAt: todayStamp(),
    updatedAt: nowStamp(),
    applications: []
  };
  mockSkillPacks = [...mockSkillPacks.filter((item) => item.id !== skill.id), skill];
  return { imported: [skill], skipped: [], message: `Mock imported ${name}.` };
}

function mockDownloadGithubSkill(repoUrl: string): SkillImportResult {
  const name = repoUrl.split("/").pop()?.replace(/\.git$/, "") || "github-skill";
  const id = slugifyProfileName(name);
  const skill: SkillPack = {
    id,
    name,
    version: "github",
    description: `Mock cloned from ${repoUrl}.`,
    about: `Mock GitHub About for ${repoUrl}.`,
    sourceType: "github",
    source: repoUrl,
    originalPath: repoUrl,
    managedPath: `%APPDATA%\\CodexHub\\skills\\${id}`,
    hasSkillMd: true,
    skillCount: 1,
    enabled: true,
    addedAt: todayStamp(),
    updatedAt: nowStamp(),
    applications: []
  };
  mockSkillPacks = [...mockSkillPacks.filter((item) => item.id !== skill.id), skill];
  return { imported: [skill], skipped: [], message: `Mock downloaded ${name}.` };
}

function mockDetectInstalledSkills(includeHosts: boolean): SkillDetectionResult {
  const detected = mockSkillPacks[0] ?? mockImportSkill(localSkillPath("example-skill")).imported[0];
  mockSkillPacks = mockSkillPacks.map((skill) =>
    skill.id === detected.id
      ? {
          ...skill,
          applications: [
            ...skill.applications.filter((application) => application.targetType !== "local"),
            {
              targetType: "local",
              label: "local",
              hostAlias: null,
              path: localSkillPath(skill.id),
              detectedAt: nowStamp(),
              hasSkillMd: true
            }
          ]
        }
      : skill
  );
  mockSkillInventoryStatus = {
    ...mockSkillInventoryStatus,
    firstHostScanCompleted: includeHosts ? true : mockSkillInventoryStatus.firstHostScanCompleted,
    localSkills: mockSkillPacks
      .filter((skill) => skill.applications.some((application) => application.targetType === "local"))
      .map((skill) => ({
        name: skill.id,
        path: localSkillPath(skill.id),
        hasSkillMd: true,
        status: "valid",
        description: skill.description || skill.about || ""
      }))
  };
  return {
    skills: clone(mockSkillPacks),
    status: clone(mockSkillInventoryStatus),
    tasks: [],
    message: includeHosts ? "Mock detected local and host skills." : "Mock detected local skills."
  };
}

function mockGetSkillTargets(skillId: string): SkillTargetsResult {
  const skill = mockSkillPacks.find((item) => item.id === skillId) ?? mockSkillPacks[0];
  const localInstalled = Boolean(skill?.applications.some((application) => application.targetType === "local"));
  return {
    skillId: skill?.id ?? skillId,
    skillName: skill?.name ?? skillId,
    targets: [
      {
        targetType: "local",
        label: "local",
        hostAlias: null,
        path: localSkillPath(skill?.id ?? skillId),
        installed: localInstalled,
        canInstall: !localInstalled,
        canUninstall: localInstalled,
        status: localInstalled ? "installed" : "available",
        message: localInstalled ? "Mock local skill is installed." : "Mock local install target is available."
      }
    ],
    tasks: [],
    message: "Mock checked skill targets."
  };
}

function mockSkillTargetOperation(skillId: string, targets: SkillTargetRequest[], action: "install" | "uninstall"): SkillTargetOperationResult {
  const skill = mockSkillPacks.find((item) => item.id === skillId);
  if (!skill) throw new Error(`Skill ${skillId} was not found.`);
  const results = targets.map((target) => {
    const label = target.targetType === "local" ? "local" : target.hostAlias ?? "unknown";
    return {
      targetType: target.targetType,
      label,
      hostAlias: target.hostAlias ?? null,
      ok: true,
      message: `Mock ${action}ed ${skill.name} on ${label}.`,
      task: null
    };
  });
  mockSkillPacks = mockSkillPacks.map((item) => {
    if (item.id !== skill.id) return item;
    if (action === "install") {
      const applications = targets.map((target) => ({
        targetType: target.targetType,
        label: target.targetType === "local" ? "local" : target.hostAlias ?? "unknown",
        hostAlias: target.hostAlias ?? null,
        path: target.targetType === "local" ? localSkillPath(skill.id) : `~/.codex/skills/${skill.id}`,
        detectedAt: nowStamp(),
        hasSkillMd: true
      }));
      return {
        ...item,
        applications: [
          ...item.applications.filter(
            (application) => !applications.some((next) => next.targetType === application.targetType && next.hostAlias === application.hostAlias)
          ),
          ...applications
        ]
      };
    }
    return {
      ...item,
      applications: item.applications.filter(
        (application) => !targets.some((target) => target.targetType === application.targetType && (target.hostAlias ?? null) === application.hostAlias)
      )
    };
  });
  const timestamp = nowStamp();
  const localTargets = targets.filter((target) => target.targetType === "local");
  const hostTargets = targets.filter((target) => target.targetType === "host" && target.hostAlias);
  if (localTargets.length > 0) {
    mockSkillInventoryStatus = {
      ...mockSkillInventoryStatus,
      localSkills: action === "install"
        ? [
            ...mockSkillInventoryStatus.localSkills.filter((item) => item.name !== skill.id),
            {
              name: skill.id,
              path: localSkillPath(skill.id),
              hasSkillMd: true,
              status: "valid",
              description: skill.description || skill.about || ""
            }
          ].sort((left, right) => left.name.localeCompare(right.name))
        : mockSkillInventoryStatus.localSkills.filter((item) => item.name !== skill.id)
    };
  }
  if (hostTargets.length > 0) {
    const nextInventories = mockSkillInventoryStatus.hostInventories.slice();
    for (const target of hostTargets) {
      const hostAlias = target.hostAlias ?? "unknown";
      const existing = nextInventories.find((inventory) => inventory.hostAlias === hostAlias);
      const base = existing ?? {
        hostAlias,
        scannedAt: timestamp,
        ok: true,
        message: "Updated from mock skill operation.",
        skills: []
      };
      base.scannedAt = timestamp;
      base.ok = true;
      base.skills = base.skills.filter((item) => item.name !== skill.id);
      if (action === "install") {
        base.skills.push({
          name: skill.id,
          path: `~/.codex/skills/${skill.id}`,
          hasSkillMd: true,
          status: "valid",
          description: skill.description || skill.about || ""
        });
        base.skills.sort((left, right) => left.name.localeCompare(right.name));
      }
      if (!existing) nextInventories.push(base);
    }
    mockSkillInventoryStatus = {
      ...mockSkillInventoryStatus,
      hostInventories: nextInventories.sort((left, right) => left.hostAlias.localeCompare(right.hostAlias))
    };
  }
  return {
    ok: true,
    skills: clone(mockSkillPacks),
    tasks: [],
    results,
    message: action === "install" ? "install-success" : "uninstall-success"
  };
}

function mockDeleteLibrarySkill(skillId: string): SkillTargetOperationResult {
  const skill = mockSkillPacks.find((item) => item.id === skillId);
  mockSkillPacks = mockSkillPacks.filter((item) => item.id !== skillId);
  return {
    ok: true,
    skills: clone(mockSkillPacks),
    tasks: [],
    results: [],
    message: `Mock removed ${skill?.name ?? skillId} from the local skill library.`
  };
}

function mockDownloadInstalledSkill(request: InstalledSkillRequest): InstalledSkillDownloadResult {
  const id = slugifyProfileName(request.skillName);
  const skill: SkillPack = {
    id,
    name: request.skillName,
    version: "mock",
    description: `Mock imported from ${request.path}.`,
    about: `Mock imported from ${request.path}.`,
    sourceType: request.targetType === "host" ? "host" : "local",
    source: request.path,
    originalPath: request.path,
    managedPath: `%APPDATA%\\CodexHub\\skills\\${id}`,
    hasSkillMd: true,
    skillCount: 1,
    enabled: true,
    addedAt: todayStamp(),
    updatedAt: nowStamp(),
    applications: []
  };
  mockSkillPacks = [...mockSkillPacks.filter((item) => item.id !== skill.id), skill];
  return {
    imported: [skill],
    skipped: [],
    skills: clone(mockSkillPacks),
    status: clone(mockSkillInventoryStatus),
    tasks: [],
    message: `Mock downloaded ${request.skillName} to the local skill library.`
  };
}

function mockUninstallInstalledSkill(request: InstalledSkillRequest): SkillTargetOperationResult {
  const id = slugifyProfileName(request.skillName);
  const label = request.targetType === "local" ? "local" : request.hostAlias ?? "unknown";
  mockSkillPacks = mockSkillPacks.map((skill) => ({
    ...skill,
    applications: skill.applications.filter(
      (application) =>
        !(
          (skill.id === id || skill.name.toLowerCase() === request.skillName.toLowerCase()) &&
          application.targetType === request.targetType &&
          (application.hostAlias ?? null) === (request.hostAlias ?? null)
        )
    )
  }));
  if (request.targetType === "local") {
    mockSkillInventoryStatus = {
      ...mockSkillInventoryStatus,
      localSkills: mockSkillInventoryStatus.localSkills.filter(
        (skill) => !(skill.name.toLowerCase() === request.skillName.toLowerCase() && skill.path === request.path)
      )
    };
  } else if (request.hostAlias) {
    mockSkillInventoryStatus = {
      ...mockSkillInventoryStatus,
      hostInventories: mockSkillInventoryStatus.hostInventories.map((inventory) =>
        inventory.hostAlias === request.hostAlias
          ? {
              ...inventory,
              skills: inventory.skills.filter(
                (skill) => !(skill.name.toLowerCase() === request.skillName.toLowerCase() && skill.path === request.path)
              )
            }
          : inventory
      )
    };
  }
  return {
    ok: true,
    skills: clone(mockSkillPacks),
    tasks: [],
    results: [
      {
        targetType: request.targetType,
        label,
        hostAlias: request.hostAlias ?? null,
        ok: true,
        message: `Mock uninstalled ${request.skillName} from ${label}.`,
        task: null
      }
    ],
    message: "uninstall-success"
  };
}

function mockUpdateLibrarySkillAbout(skillId: string, about: string): SkillPack[] {
  mockSkillPacks = mockSkillPacks.map((skill) =>
    skill.id === skillId ? { ...skill, about, description: about.trim(), updatedAt: nowStamp() } : skill
  );
  return clone(mockSkillPacks);
}

export const mockApi: CodexHubApi = {
  getHealth: async () => fallbackHealth,
  getAppUpdateStatus: async () => fallbackAppUpdateStatus,
  checkStableUpdate: async () => fallbackAppUpdateStatus,
  installStableUpdate: async () => fallbackAppUpdateStatus,
  detectNetworkProxy: async () => fallbackNetworkProxyStatus,
  getSettings: async () => {
    const settings = normalizeSettings(loadMockSettings());
    saveMockSettings(settings);
    return settings;
  },
  saveSettings: async (settings: AppSettings) => {
    const current = normalizeSettings(loadMockSettings());
    const normalized = normalizeSettings(settings);
    saveMockSettings(normalized);
    return { settings: normalized, changed: JSON.stringify(current) !== JSON.stringify(normalized), backupPath: null };
  },
  chooseCloseButtonBehavior: async (behavior: Exclude<CloseButtonBehavior, "ask">) => {
    const current = normalizeSettings(loadMockSettings());
    const normalized = normalizeSettings({ ...current, closeButtonBehavior: behavior });
    saveMockSettings(normalized);
    return { settings: normalized, changed: JSON.stringify(current) !== JSON.stringify(normalized), backupPath: null };
  },
  onCloseButtonBehaviorRequested: async () => () => {},
  getSshStatus: async () => fallbackSshStatus(),
  generateEd25519Key: async () => {
    throw new Error("The Tauri desktop backend is required to generate SSH keys.");
  },
  listSshConfigHosts: async () => clone(fallbackSshConfigHosts),
  upsertSshConfigHost: async (_draft: SshHostDraft) => {
    throw new Error("The Tauri desktop backend is required to write SSH config.");
  },
  deleteSshConfigHost: async (alias: string): Promise<SshConfigDeleteResult> => {
    const task = mockTaskRun(
      `mock-host-${slugifyProfileName(alias)}`,
      alias,
      "Delete SSH Host",
      `Deleted Host ${alias} from the mock SSH config inventory.`,
      true,
      `delete_ssh_config_host ${alias}`
    );
    recordMockTask(task);
    return {
      changed: true,
      action: "deleted",
      configPath: fallbackSshStatus().configPath,
      backupPath: null,
      host: null,
      message: `Deleted Host ${alias} from the mock SSH config inventory.`,
      task
    };
  },
  listHosts: async () => clone(mockHosts),
  refreshDiscoveredHosts: async () => clone(mockHosts),
  refreshLatestCodexVersion: async () => ({
    ...fallbackLatestCodexVersion,
    checkedAt: new Date().toISOString()
  }),
  getLocalCodexStatus: async () => fallbackLocalCodexStatus(),
  addHost: async (draft: HostDraft) => {
    const host: Host = {
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
      codexCommandAvailable: null,
      codexInstalled: false,
      codexVersion: "pending",
      configExists: null,
      apiConfigName: null,
      apiConfigSource: null,
      apiKeyEnvVar: null,
      apiKeyEnvPresent: null,
      skillsExists: null,
      skillsCount: null,
      profileId: null,
      skillPackIds: [],
      tags: draft.tags,
      lastSeen: "just added",
      latencyMs: null
    };
    mockHosts = [...mockHosts, host];
    return clone(host);
  },
  updateHost: async (id: string, patch: HostPatch) => {
    const current = mockHosts.find((host) => host.id === id) ?? fallbackHostForAlias(id);
    const host = { ...current, ...patch };
    mockHosts = mockHosts.some((item) => item.id === id)
      ? mockHosts.map((item) => item.id === id ? host : item)
      : [...mockHosts, host];
    return clone(host);
  },
  deleteHost: async (id: string) => {
    const before = mockHosts.length;
    mockHosts = mockHosts.filter((host) => host.id !== id);
    return mockHosts.length !== before;
  },
  testSshConnection: async () => fallbackConnection,
  sshCheck: async (hostAlias: string) => mockSshCheck(hostAlias),
  connectSshHost: async (
    draft: SshHostDraft,
    _password: string,
    requestId: string,
    onProgress?: (event: SshBootstrapProgressEvent) => void
  ) => mockSshBootstrapHostWithProgress(draft, requestId, onProgress),
  bootstrapSshHost: async (draft: SshHostDraft) => mockSshBootstrapHost(draft),
  bootstrapExistingSshHost: async (hostAlias: string) =>
    mockSshBootstrapHost({
      alias: hostAlias,
      hostName: hostAlias,
      port: 22,
      user: "codex",
      identityFile: fallbackSshStatus().preferredIdentityFile
    }),
  remoteProbeCodex: async (hostAlias: string, _timeoutMs = 10000, requestId?: string, onProgress?: (event: HostOperationProgressEvent) => void) =>
    mockRemoteProbeWithProgress(hostAlias, requestId, onProgress),
  batchRemoteProbeCodex: async (hostAliases: string[], _timeoutMs = 10000, requestId = `mock-batch-probe-${Date.now()}`, onProgress?: (event: HostOperationProgressEvent) => void, onItemCompleted?: (event: RemoteProbeBatchItemCompletedEvent) => void) =>
    mockBatchRemoteProbeCodex(hostAliases, requestId, onProgress, onItemCompleted),
  sampleHostResources: async (
    hostAliases: string[],
    _timeoutMs = 10000,
    recordTask = true,
    requestId?: string,
    onProgress?: (event: HostResourceProgressEvent) => void
  ) => mockSampleHostResourcesWithProgress(hostAliases, recordTask, requestId, onProgress),
  remoteManageCodex: async (
    hostAlias: string,
    action: RemoteCodexAction,
    _timeoutMs = 120000,
    requestId?: string,
    onProgress?: (event: HostOperationProgressEvent) => void
  ) => mockRemoteManageCodexWithProgress(hostAlias, action, requestId, onProgress),
  batchRemoteUpdateCodex: async (hostAliases: string[], _timeoutMs = 120000, requestId = `mock-batch-update-${Date.now()}`, onProgress?: (event: HostOperationProgressEvent) => void) =>
    mockBatchRemoteUpdateCodex(hostAliases, requestId, onProgress),
  listProfiles: async () => clone(mockProfiles).map(normalizeProfile),
  createProfile: async (draft: ProfileDraft) => {
    const profile = createMockProfile(draft);
    mockProfiles = [...mockProfiles, profile];
    return normalizeProfile(clone(profile));
  },
  updateProfile: async (id: string, patch: ProfilePatch) => {
    const current = mockProfiles.find((profile) => profile.id === id);
    if (!current) throw new Error(`Profile ${id} was not found.`);
    const next = normalizeProfile({ ...current, ...patch, updatedAt: nowStamp() });
    mockProfiles = mockProfiles.map((profile) => (profile.id === id ? next : profile));
    return clone(next);
  },
  deleteProfile: async (id: string) => {
    const profile = mockProfiles.find((item) => item.id === id);
    const deleted = Boolean(profile);
    mockProfiles = mockProfiles.filter((item) => item.id !== id);
    mockProfileCredentialIds.delete(id);
    const message = deleted ? `Deleted profile ${profile?.name ?? id}.` : `Profile ${id} was not found.`;
    const task = mockTaskRun(id, profile?.name ?? id, "Delete profile", message, deleted, `delete_profile ${id}`);
    recordMockTask(task);
    return { ok: deleted, deleted, message, task };
  },
  duplicateProfile: async (id: string) => {
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
  },
  importProfiles: async (bundle: ProfileImportExport) => {
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
    return { schemaVersion: bundle.schemaVersion || 1, exportedAt: nowStamp(), profiles: clone(imported).map(normalizeProfile) };
  },
  setProfileApiKey: async (profileId: string, apiKey: string) => {
    const current = mockProfiles.find((item) => item.id === profileId);
    if (!current) throw new Error(`Profile ${profileId} was not found.`);
    const profile = normalizeProfile({
      ...current,
      credentialStored: Boolean(apiKey),
      updatedAt: nowStamp()
    });
    if (apiKey) mockProfileCredentialIds.add(profileId);
    mockProfiles = mockProfiles.map((item) => (item.id === profileId ? profile : item));
    return clone(profile);
  },
  getProfileApiKey: async (profileId: string): Promise<ProfileApiKeyResult> => {
    const current = mockProfiles.find((item) => item.id === profileId);
    if (!current) throw new Error(`Profile ${profileId} was not found.`);
    const exists = mockProfileCredentialIds.has(profileId);
    // Explicit Mock mode never retains the user's real input value.
    return { profileId, exists, apiKey: exists ? "mock-api-key-not-for-real-use" : null };
  },
  deleteProfileApiKey: async (profileId: string) => {
    const current = mockProfiles.find((item) => item.id === profileId);
    if (!current) throw new Error(`Profile ${profileId} was not found.`);
    const profile = normalizeProfile({
      ...current,
      credentialStored: false,
      updatedAt: nowStamp()
    });
    mockProfileCredentialIds.delete(profileId);
    mockProfiles = mockProfiles.map((item) => (item.id === profileId ? profile : item));
    return clone(profile);
  },
  previewProfileApply: async (profileId: string, hostIds: string[]) => mockPreviewProfileApply(profileId, hostIds),
  applyProfile: async (profileId: string, hostIds: string[], options: ProfileApplyOptions) =>
    normalizeProfileApplyResult(mockApplyProfile(profileId, hostIds, normalizeProfileApplyOptions(options))),
  detectCcSwitchProfiles: async () => {
    const detection = mockCcSwitchDetection();
    return {
      ...detection,
      importExport: { ...detection.importExport, profiles: detection.importExport.profiles.map(normalizeProfile) }
    };
  },
  importCcSwitchProfiles: async (detection: CcSwitchDetection) => {
    const imported = detection.importExport.profiles.map((profile) =>
      normalizeProfile({
        ...profile,
        source: "cc-switch",
        credentialStored: true,
        updatedAt: nowStamp()
      })
    );
    for (const profile of imported) {
      mockProfileCredentialIds.add(profile.id);
    }
    const importedKeys = new Set(imported.map(ccSwitchProfileKey));
    mockProfiles = [
      ...mockProfiles.filter((profile) => profile.source !== "cc-switch" || !importedKeys.has(ccSwitchProfileKey(profile))),
      ...imported
    ];
    return { schemaVersion: 1, exportedAt: nowStamp(), profiles: clone(imported).map(normalizeProfile) };
  },
  listSkillPacks: async () => clone(mockSkillPacks),
  getSkillInventoryStatus: async () => clone(mockSkillInventoryStatus),
  detectInstalledSkills: async (includeHosts: boolean) => mockDetectInstalledSkills(includeHosts),
  importLocalSkill: async (path: string) => mockImportSkill(path),
  downloadGithubSkill: async (repoUrl: string) => mockDownloadGithubSkill(repoUrl),
  getSkillTargets: async (skillId: string) => mockGetSkillTargets(skillId),
  installSkillTargets: async (skillId: string, targets: SkillTargetRequest[]) => mockSkillTargetOperation(skillId, targets, "install"),
  uninstallSkillTargets: async (skillId: string, targets: SkillTargetRequest[]) => mockSkillTargetOperation(skillId, targets, "uninstall"),
  deleteLibrarySkill: async (skillId: string, uninstallFirst: boolean) => {
    if (uninstallFirst) {
      const skill = mockSkillPacks.find((item) => item.id === skillId);
      if (skill) {
        mockSkillTargetOperation(
          skillId,
          skill.applications.map((application) => ({ targetType: application.targetType, hostAlias: application.hostAlias })),
          "uninstall"
        );
      }
    }
    return mockDeleteLibrarySkill(skillId);
  },
  downloadInstalledSkill: async (request: InstalledSkillRequest) => mockDownloadInstalledSkill(request),
  uninstallInstalledSkill: async (request: InstalledSkillRequest) => mockUninstallInstalledSkill(request),
  updateLibrarySkillAbout: async (skillId: string, about: string) => mockUpdateLibrarySkillAbout(skillId, about),
  listTasks: async () => clone(mockTasks),
  queryTasks: async (query) => {
    const limit = Math.max(1, Math.min(100, query?.limit ?? 50));
    const cursorIndex = query?.cursor ? mockTasks.findIndex((task) => task.id === query.cursor) : -1;
    const start = query?.cursor ? (cursorIndex >= 0 ? cursorIndex + 1 : mockTasks.length) : 0;
    const pageItems = mockTasks.slice(start, start + limit);
    const items = clone(pageItems);
    return {
      items,
      nextCursor:
        start + pageItems.length < mockTasks.length && pageItems.length > 0
          ? pageItems[pageItems.length - 1].id
          : null,
      unacknowledgedTaskIds: mockTasks
        .filter((task) => (task.status === "failed" || task.status === "interrupted") && !mockAcknowledgedTaskIds.has(task.id))
        .map((task) => task.id)
    };
  },
  getTask: async (taskId: string) => clone(mockTasks.find((task) => task.id === taskId) ?? null),
  acknowledgeTask: async (taskId: string) => {
    if (!mockTasks.some((task) => task.id === taskId)) return false;
    mockAcknowledgedTaskIds.add(taskId);
    return true;
  },
  clearTaskHistory: async () => {
    const removedIds = mockTasks
      .filter((task) => task.status !== "queued" && task.status !== "running")
      .map((task) => task.id);
    const removedIdSet = new Set(removedIds);
    mockTasks = mockTasks.filter((task) => !removedIdSet.has(task.id));
    for (const taskId of removedIds) mockAcknowledgedTaskIds.delete(taskId);
    return removedIds.length;
  },
  recordFrontendError: async (message: string) => {
    const task = mockTaskRun("local-ui", "CodexHub UI", "Frontend error", message, false);
    recordMockTask(task);
    return clone(task);
  },
  onTaskUpdated: async (handler) => {
    mockTaskHandlers.add(handler);
    return () => mockTaskHandlers.delete(handler);
  },
  getStorageHealth: async () => ["settings", "hosts", "profiles", "skills"].map((store) => ({
    store,
    path: `mock://${store}.json`,
    state: "missing" as const,
    schemaVersion: null,
    currentSchemaVersion: 1,
    sourceSha256: null,
    latestBackupPath: null,
    message: "Explicit Mock mode does not use desktop storage."
  })),
  previewStorageMigration: async (store: string) => {
    throw new Error(`Mock storage ${store} does not require migration.`);
  },
  applyStorageMigration: async () => {
    throw new Error("Storage migration requires the Tauri desktop backend.");
  },
  previewStorageRestore: async (store: string) => {
    throw new Error(`Mock storage ${store} has no recovery backup.`);
  },
  restoreStorageBackup: async () => {
    throw new Error("Storage recovery requires the Tauri desktop backend.");
  }
};

