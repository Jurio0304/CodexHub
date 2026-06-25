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
  address: string;
  port: number;
  username: string;
  authMethod: AuthMethod;
  status: HostStatus;
  os: string;
  codexVersion: string;
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
