export type Health = {
  app: string;
  mode: string;
  remoteWrapperRequired: boolean;
};

export type LatestCodexVersion = {
  version: string | null;
  checkedAt: string | null;
  source: "npm" | string;
  error: string | null;
};

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
  codexInstalled: boolean;
  codexVersion: string;
  configExists: boolean | null;
  apiConfigName?: string | null;
  apiConfigSource?: string | null;
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

export type TaskStatus = "queued" | "running" | "success" | "failed";
export type TaskLogLevel = "info" | "warn" | "error";

export type TaskLog = {
  id: string;
  taskRunId: string;
  level: TaskLogLevel;
  timestamp: string;
  message: string;
  command?: string;
  stdout?: string;
  stderr?: string;
  exitCode?: number | null;
  durationMs?: number;
  timedOut?: boolean;
};

export type TaskRun = {
  id: string;
  hostId: string;
  hostName: string;
  action: string;
  status: TaskStatus;
  startedAt: string;
  endedAt: string | null;
  summary: string;
  logs: TaskLog[];
};

export type ConnectionTest = {
  ok: boolean;
  latencyMs: number | null;
  message: string;
};

export type SshCheckResult = {
  hostAlias: string;
  ok: boolean;
  latencyMs: number | null;
  message: string;
  task: TaskRun;
};

export type SshBootstrapResult = {
  hostAlias: string;
  ok: boolean;
  latencyMs: number | null;
  message: string;
  generatedKey: boolean;
  privateKeyPath: string;
  publicKeyPath: string;
  writeResult: SshConfigWriteResult;
  task: TaskRun;
};

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

export type RemoteProbeResult = {
  hostAlias: string;
  sshStatus: HostStatus;
  latencyMs: number | null;
  os: string;
  arch: string;
  shell: string;
  path: string | null;
  pathHasLocalBin: boolean;
  codexInstalled: boolean;
  codexPath: string | null;
  codexVersion: string;
  configExists: boolean;
  apiConfigName: string;
  apiConfigSource: string;
  skillsExists: boolean;
  skillsCount: number;
  task: TaskRun;
};

export type RemoteCodexAction = "check-version" | "install" | "update";

export type RemoteCodexProgressEvent = {
  requestId: string;
  hostAlias: string;
  action: RemoteCodexAction;
  step: string;
  status: "running" | "stdout" | "stderr" | "heartbeat" | "success" | "failed" | string;
  message: string;
  detail: string | null;
  stdout: string | null;
  stderr: string | null;
  exitCode: number | null;
  durationMs: number | null;
  timedOut: boolean | null;
};

export type RemoteCodexMaintenanceResult = {
  hostAlias: string;
  ok: boolean;
  action: RemoteCodexAction;
  beforeVersion: string | null;
  afterVersion: string | null;
  codexPath: string | null;
  installMethod: string | null;
  pathChanged: boolean;
  shellConfigPath: string | null;
  backupPath: string | null;
  message: string;
  task: TaskRun;
};

export type SshKeyInfo = {
  keyType: "ed25519" | "rsa" | string;
  privatePath: string;
  publicPath: string;
  privateExists: boolean;
  publicExists: boolean;
  publicKey: string | null;
};

export type SshStatus = {
  sshDir: string;
  configPath: string;
  sshKeygenAvailable: boolean;
  preferredIdentityFile: string;
  ed25519: SshKeyInfo;
  rsa: SshKeyInfo;
};

export type SshHostDraft = {
  alias: string;
  hostName: string;
  port: number;
  user: string;
  identityFile: string;
};

export type SshConfigHost = SshHostDraft & {
  managed: boolean;
  source: "managed" | "local" | "mock" | string;
};

export type SshConfigWriteResult = {
  changed: boolean;
  action: "added" | "updated" | "deleted" | "unchanged" | string;
  configPath: string;
  backupPath: string | null;
  host: SshConfigHost | null;
  message: string;
};

export type SshKeyGenerationResult = {
  privatePath: string;
  publicPath: string;
  status: SshStatus;
  message: string;
};
