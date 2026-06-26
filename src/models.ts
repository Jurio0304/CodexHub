export type Health = {
  app: string;
  mode: string;
  remoteWrapperRequired: boolean;
};

export type HostStatus = "online" | "offline" | "unknown" | "testing";
export type AuthMethod = "ssh-key" | "password" | "agent";

export type Host = {
  id: string;
  name: string;
  hostAlias: string;
  source: "managed" | "unmanaged-readonly" | "mock" | "manual" | string;
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
  skillsExists: boolean | null;
  skillsCount: number | null;
  profileId: string | null;
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
  approvalPolicy: string;
  sandboxMode: string;
  updatedAt: string;
  hostIds: string[];
};

export type SkillPack = {
  id: string;
  name: string;
  version: string;
  description: string;
  source: string;
  skillCount: number;
  enabled: boolean;
  updatedAt: string;
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
  os: string;
  arch: string;
  shell: string;
  path: string | null;
  pathHasLocalBin: boolean;
  codexInstalled: boolean;
  codexPath: string | null;
  codexVersion: string;
  configExists: boolean;
  skillsExists: boolean;
  skillsCount: number;
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
  source: "managed" | "unmanaged-readonly" | "mock" | string;
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
