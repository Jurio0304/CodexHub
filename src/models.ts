import type {
  AppUpdateStateDto,
  AppUpdateStatusDto,
  ConnectionTestDto,
  CpuSnapshotDto,
  DeleteOperationResultDto,
  GpuProcessSnapshotDto,
  GpuSnapshotDto,
  GpuVendorDto,
  HealthDto,
  HostResourceBatchResultDto,
  HostResourceProgressEventDto,
  HostOperationKindDto,
  HostOperationProgressEventDto,
  HostResourceSnapshotDto,
  HostResourceSshStatusDto,
  HostResourceStatusDto,
  LatestCodexVersionDto,
  LocalCodexStatusDto,
  MemorySnapshotDto,
  NetworkProxyCandidateDto,
  NetworkProxyStatusDto,
  RemoteCodexActionDto,
  RemoteCodexBatchItemDto,
  RemoteCodexBatchResultDto,
  RemoteCodexMaintenanceResultDto,
  RemoteCodexProgressEventDto,
  RemoteProbeResultDto,
  RemoteProbeBatchItemDto,
  RemoteProbeBatchItemCompletedEventDto,
  RemoteProbeBatchResultDto,
  SshBootstrapResultDto,
  SshCheckResultDto,
  SshConfigDeleteResultDto,
  SshConfigHostDto,
  SshConfigWriteResultDto,
  SshHostDraftDto,
  SshKeyGenerationResultDto,
  SshKeyInfoDto,
  SshStatusDto,
  TaskLog,
  TaskLogLevel,
  TaskRun,
  TaskStep,
  TaskStepStatus,
  TaskStatus
} from "./generated/rust-contracts";

export type Health = HealthDto;

export type AppReleaseChannel = "stable" | "dev" | string;

export type AppUpdateState = AppUpdateStateDto | "checking";
export type AppUpdateStatus = Omit<AppUpdateStatusDto, "channel" | "state"> & {
  channel: AppReleaseChannel;
  state: AppUpdateState;
};

export type NetworkProxyCandidate = NetworkProxyCandidateDto;
export type NetworkProxyStatus = NetworkProxyStatusDto;
export type LatestCodexVersion = LatestCodexVersionDto;

export type RuntimePlatform = "windows" | "macos" | "linux";

export type LocalCodexStatus = LocalCodexStatusDto;

export type HostStatus = "online" | "offline" | "unknown" | "testing";
export type AuthMethod = "ssh-key" | "password" | "agent";

export type Host = {
  id: string;
  name: string;
  hostAlias: string;
  source: "managed" | "local" | "mock" | "manual" | string;
  address: string;
  port: number;
  username: string;
  authMethod: AuthMethod;
  status: HostStatus;
  os: string;
  arch: string;
  shell: string;
  path: string | null;
  pathHasLocalBin: boolean | null;
  codexCommandAvailable: boolean | null;
  codexInstalled: boolean;
  codexVersion: string;
  configExists: boolean | null;
  apiConfigName?: string | null;
  apiConfigSource?: string | null;
  apiKeyEnvVar?: string | null;
  apiKeyEnvPresent?: boolean | null;
  skillsExists: boolean | null;
  skillsCount: number | null;
  profileId: string | null;
  profileAppliedAt?: string | null;
  profileAppliedSource?: string | null;
  skillPackIds: string[];
  tags: string[];
  lastSeen: string;
  latencyMs: number | null;
};

export type HostDraft = {
  name: string;
  address: string;
  port: number;
  username: string;
  authMethod: AuthMethod;
  tags: string[];
};

export type HostPatch = Partial<Pick<Host, "name" | "address" | "port" | "username" | "authMethod" | "status" | "profileId" | "tags">>;

export type Profile = {
  id: string;
  name: string;
  description: string;
  model: string;
  provider: string;
  baseUrl: string;
  apiKeyEnvVar: string;
  modelReasoningEffort: string;
  planModeReasoningEffort: string;
  fastMode: boolean;
  serviceTier: string;
  approvalPolicy: string;
  sandboxMode: string;
  extraToml: string;
  createdAt: string;
  updatedAt: string;
  source: "managed" | "imported" | "cc-switch" | "mock" | string;
  credentialStored: boolean;
  hostIds: string[];
};

export type ProfileDraft = {
  name: string;
  description: string;
  model: string;
  provider: string;
  baseUrl: string;
  apiKeyEnvVar: string;
  modelReasoningEffort: string;
  planModeReasoningEffort: string;
  fastMode: boolean;
  serviceTier: string;
  approvalPolicy: string;
  sandboxMode: string;
  extraToml: string;
  hostIds: string[];
};

export type ProfilePatch = Partial<ProfileDraft>;

export type ProfileImportExport = {
  schemaVersion: number;
  exportedAt: string;
  profiles: Profile[];
};

export type ProfileApiKeyResult = {
  profileId: string;
  exists: boolean;
  apiKey: string | null;
};

export type ProfileApplyHostResult = {
  hostId: string;
  hostName: string;
  hostAlias: string;
  status: "pending" | "success" | "failed" | "no-change";
  targetPath: string;
  backupPath: string | null;
  message: string;
  task?: TaskRun;
};

export type ProfileApplyPreview = {
  profileId: string;
  profileName: string;
  renderedToml: string;
  targetFiles: Array<{
    hostId: string;
    hostName: string;
    hostAlias: string;
    path: string;
    backupExpected: boolean;
    noChangeExpected: boolean;
  }>;
  hostResults: ProfileApplyHostResult[];
  warnings: string[];
};

export type ProfileApplyBatchResult = {
  profileId: string;
  ok: boolean;
  results: ProfileApplyHostResult[];
  tasks: TaskRun[];
  profiles: Profile[];
  hosts: Host[];
};

export type CcSwitchDetection = {
  detected: boolean;
  sourcePath: string | null;
  message: string;
  importExport: ProfileImportExport;
};

export type SkillPack = {
  id: string;
  name: string;
  version: string;
  description: string;
  about: string;
  sourceType: "local" | "github" | string;
  source: string;
  originalPath: string | null;
  managedPath: string;
  hasSkillMd: boolean;
  skillCount: number;
  enabled: boolean;
  addedAt: string;
  updatedAt: string;
  applications: SkillApplication[];
};

export type SkillApplication = {
  targetType: "local" | "host" | string;
  label: string;
  hostAlias: string | null;
  path: string;
  detectedAt: string;
  hasSkillMd: boolean;
};

export type SkillImportResult = {
  imported: SkillPack[];
  skipped: string[];
  message: string;
};

export type HostSkillInventory = {
  hostAlias: string;
  scannedAt: string;
  ok: boolean;
  message: string;
  skills: RemoteSkill[];
};

export type SkillInventoryStatus = {
  firstHostScanCompleted: boolean;
  localSkillRoot: string;
  localSkills: RemoteSkill[];
  hostInventories: HostSkillInventory[];
};

export type SkillDetectionResult = {
  skills: SkillPack[];
  status: SkillInventoryStatus;
  tasks: TaskRun[];
  message: string;
};

export type RemoteSkill = {
  name: string;
  path: string;
  hasSkillMd: boolean;
  status: string;
  description?: string;
};

export type RemoteSkillListResult = {
  hostAlias: string;
  rootPath: string;
  count: number;
  validCount: number;
  invalidCount: number;
  skills: RemoteSkill[];
  task: TaskRun;
};

export type SkillTargetRequest = {
  targetType: "local" | "host" | string;
  hostAlias?: string | null;
};

export type SkillTarget = {
  targetType: "local" | "host" | string;
  label: string;
  hostAlias: string | null;
  path: string;
  installed: boolean;
  canInstall: boolean;
  canUninstall: boolean;
  status: string;
  message: string;
};

export type SkillTargetsResult = {
  skillId: string;
  skillName: string;
  targets: SkillTarget[];
  tasks: TaskRun[];
  message: string;
};

export type SkillTargetOperationItem = {
  targetType: "local" | "host" | string;
  label: string;
  hostAlias: string | null;
  ok: boolean;
  message: string;
  task: TaskRun | null;
};

export type SkillTargetOperationResult = {
  ok: boolean;
  skills: SkillPack[];
  tasks: TaskRun[];
  results: SkillTargetOperationItem[];
  message: string;
};

export type InstalledSkillRequest = {
  targetType: "local" | "host" | string;
  hostAlias?: string | null;
  skillName: string;
  path: string;
};

export type InstalledSkillDownloadResult = {
  imported: SkillPack[];
  skipped: string[];
  skills: SkillPack[];
  status: SkillInventoryStatus;
  tasks: TaskRun[];
  message: string;
};

// Wire task contracts are generated from Rust; UI-only models remain in this file.
export type {
  HostOperationKindDto as HostOperationKind,
  HostOperationProgressEventDto as HostOperationProgressEvent,
  TaskLog,
  TaskLogLevel,
  TaskRun,
  TaskStep,
  TaskStepStatus,
  TaskStatus
};

export type ConnectionTest = ConnectionTestDto;
export type SshCheckResult = SshCheckResultDto;
export type SshBootstrapResult = SshBootstrapResultDto;

export type SshBootstrapStep = "password_login" | "install_public_key" | "set_permissions" | "verify_alias_login";
export type SshBootstrapStepStatus = "pending" | "running" | "success" | "failed";

export type SshBootstrapProgressEvent = {
  requestId: string;
  hostAlias: string;
  step: SshBootstrapStep;
  status: SshBootstrapStepStatus;
  message: string;
  detail: string | null;
  stdout: string | null;
  stderr: string | null;
  exitCode: number | null;
  durationMs: number | null;
  timedOut: boolean | null;
};

export type RemoteProbeResult = RemoteProbeResultDto;

export type RemoteProbeBatchItem = RemoteProbeBatchItemDto;
export type RemoteProbeBatchItemCompletedEvent = RemoteProbeBatchItemCompletedEventDto;
export type RemoteProbeBatchResult = RemoteProbeBatchResultDto;

export type HostResourceStatus = HostResourceStatusDto;
export type HostResourceSshStatus = HostResourceSshStatusDto;
export type GpuVendor = GpuVendorDto;
export type GpuTool = "nvidia-smi" | "rocm-smi" | "lspci" | "none" | string;
export type CpuSnapshot = CpuSnapshotDto;
export type MemorySnapshot = MemorySnapshotDto;
export type GpuProcessSnapshot = GpuProcessSnapshotDto;
export type GpuSnapshot = GpuSnapshotDto;
export type HostResourceSnapshot = HostResourceSnapshotDto;
export type HostResourceProgressEvent = HostResourceProgressEventDto;
export type HostResourceBatchResult = HostResourceBatchResultDto;
export type RemoteCodexAction = RemoteCodexActionDto;
export type RemoteCodexProgressEvent = RemoteCodexProgressEventDto;
export type RemoteCodexMaintenanceResult = RemoteCodexMaintenanceResultDto;

export type RemoteCodexBatchItem = RemoteCodexBatchItemDto;
export type RemoteCodexBatchResult = RemoteCodexBatchResultDto;

export type SshKeyInfo = SshKeyInfoDto;
export type SshStatus = SshStatusDto;
export type SshHostDraft = SshHostDraftDto;
export type SshConfigHost = SshConfigHostDto;
export type SshConfigWriteResult = SshConfigWriteResultDto;
export type SshConfigDeleteResult = SshConfigDeleteResultDto;
export type DeleteOperationResult = DeleteOperationResultDto;
export type SshKeyGenerationResult = SshKeyGenerationResultDto;
