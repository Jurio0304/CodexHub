import type { UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppUpdateStatus,
  CcSwitchDetection,
  ConnectionTest,
  DeleteOperationResult,
  Health,
  Host,
  HostDraft,
  HostOperationProgressEvent,
  HostPatch,
  HostResourceBatchResult,
  HostResourceProgressEvent,
  InstalledSkillDownloadResult,
  InstalledSkillRequest,
  LatestCodexVersion,
  LocalCodexStatus,
  NetworkProxyStatus,
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
  RemoteProbeResult,
  RemoteProbeBatchItemCompletedEvent,
  RemoteProbeBatchResult,
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
  SshConfigHost,
  SshConfigWriteResult,
  SshHostDraft,
  SshKeyGenerationResult,
  SshStatus,
  TaskRun
} from "../models";
import type { AppSettings, CloseButtonBehavior, SettingsSaveResult } from "../settings";
import type {
  StorageHealth,
  StorageMigrationPlan,
  StorageRestorePlan,
  TaskEvent,
  TaskPage,
  TaskQuery
} from "../generated/rust-contracts";

export type SshBootstrapProgressHandler = (event: SshBootstrapProgressEvent) => void;
export type HostOperationProgressHandler = (event: HostOperationProgressEvent) => void;
export type HostResourceProgressHandler = (event: HostResourceProgressEvent) => void;
export type RemoteProbeBatchItemCompletedHandler = (event: RemoteProbeBatchItemCompletedEvent) => void;
export type RemoteCodexProgressHandler = HostOperationProgressHandler;
export type TaskUpdatedHandler = (event: TaskEvent) => void;

export type CodexHubApi = {
  getHealth: () => Promise<Health>;
  getAppUpdateStatus: () => Promise<AppUpdateStatus>;
  checkStableUpdate: () => Promise<AppUpdateStatus>;
  installStableUpdate: () => Promise<AppUpdateStatus>;
  detectNetworkProxy: () => Promise<NetworkProxyStatus>;
  getSettings: () => Promise<AppSettings>;
  saveSettings: (settings: AppSettings) => Promise<SettingsSaveResult>;
  chooseCloseButtonBehavior: (behavior: Exclude<CloseButtonBehavior, "ask">) => Promise<SettingsSaveResult>;
  onCloseButtonBehaviorRequested: (handler: () => void) => Promise<UnlistenFn>;
  getSshStatus: () => Promise<SshStatus>;
  generateEd25519Key: () => Promise<SshKeyGenerationResult>;
  listSshConfigHosts: () => Promise<SshConfigHost[]>;
  upsertSshConfigHost: (draft: SshHostDraft) => Promise<SshConfigWriteResult>;
  deleteSshConfigHost: (alias: string) => Promise<SshConfigDeleteResult>;
  listHosts: () => Promise<Host[]>;
  refreshDiscoveredHosts: () => Promise<Host[]>;
  refreshLatestCodexVersion: (force?: boolean, timeoutMs?: number) => Promise<LatestCodexVersion>;
  getLocalCodexStatus: () => Promise<LocalCodexStatus>;
  addHost: (draft: HostDraft) => Promise<Host>;
  updateHost: (id: string, patch: HostPatch) => Promise<Host>;
  deleteHost: (id: string) => Promise<boolean>;
  testSshConnection: (id: string) => Promise<ConnectionTest>;
  sshCheck: (hostAlias: string, timeoutMs?: number) => Promise<SshCheckResult>;
  connectSshHost: (
    draft: SshHostDraft,
    password: string,
    requestId: string,
    onProgress?: SshBootstrapProgressHandler,
    timeoutMs?: number
  ) => Promise<SshBootstrapResult>;
  bootstrapSshHost: (draft: SshHostDraft, password: string, timeoutMs?: number) => Promise<SshBootstrapResult>;
  bootstrapExistingSshHost: (hostAlias: string, password: string, timeoutMs?: number) => Promise<SshBootstrapResult>;
  remoteProbeCodex: (
    hostAlias: string,
    timeoutMs?: number,
    requestId?: string,
    onProgress?: HostOperationProgressHandler
  ) => Promise<RemoteProbeResult>;
  batchRemoteProbeCodex: (
    hostAliases: string[],
    timeoutMs?: number,
    requestId?: string,
    onProgress?: HostOperationProgressHandler,
    onItemCompleted?: RemoteProbeBatchItemCompletedHandler
  ) => Promise<RemoteProbeBatchResult>;
  sampleHostResources: (
    hostAliases: string[],
    timeoutMs?: number,
    recordTask?: boolean,
    requestId?: string,
    onProgress?: HostResourceProgressHandler
  ) => Promise<HostResourceBatchResult>;
  remoteManageCodex: (
    hostAlias: string,
    action: RemoteCodexAction,
    timeoutMs?: number,
    requestId?: string,
    onProgress?: HostOperationProgressHandler
  ) => Promise<RemoteCodexMaintenanceResult>;
  batchRemoteUpdateCodex: (
    hostAliases: string[],
    timeoutMs?: number,
    requestId?: string,
    onProgress?: HostOperationProgressHandler
  ) => Promise<RemoteCodexBatchResult>;
  listProfiles: () => Promise<Profile[]>;
  createProfile: (draft: ProfileDraft) => Promise<Profile>;
  updateProfile: (id: string, patch: ProfilePatch) => Promise<Profile>;
  deleteProfile: (id: string) => Promise<DeleteOperationResult>;
  duplicateProfile: (id: string) => Promise<Profile>;
  importProfiles: (bundle: ProfileImportExport) => Promise<ProfileImportExport>;
  setProfileApiKey: (profileId: string, apiKey: string) => Promise<Profile>;
  getProfileApiKey: (profileId: string) => Promise<ProfileApiKeyResult>;
  deleteProfileApiKey: (profileId: string) => Promise<Profile>;
  previewProfileApply: (profileId: string, hostIds: string[]) => Promise<ProfileApplyPreview>;
  applyProfile: (profileId: string, hostIds: string[], options: ProfileApplyOptions) => Promise<ProfileApplyBatchResult>;
  detectCcSwitchProfiles: () => Promise<CcSwitchDetection>;
  importCcSwitchProfiles: (detection: CcSwitchDetection) => Promise<ProfileImportExport>;
  listSkillPacks: () => Promise<SkillPack[]>;
  getSkillInventoryStatus: () => Promise<SkillInventoryStatus>;
  detectInstalledSkills: (includeHosts: boolean, timeoutMs?: number) => Promise<SkillDetectionResult>;
  importLocalSkill: (path: string) => Promise<SkillImportResult>;
  downloadGithubSkill: (repoUrl: string, timeoutMs?: number) => Promise<SkillImportResult>;
  getSkillTargets: (skillId: string, timeoutMs?: number) => Promise<SkillTargetsResult>;
  installSkillTargets: (
    skillId: string,
    targets: SkillTargetRequest[],
    timeoutMs?: number
  ) => Promise<SkillTargetOperationResult>;
  uninstallSkillTargets: (
    skillId: string,
    targets: SkillTargetRequest[],
    timeoutMs?: number
  ) => Promise<SkillTargetOperationResult>;
  deleteLibrarySkill: (skillId: string, uninstallFirst: boolean, timeoutMs?: number) => Promise<SkillTargetOperationResult>;
  downloadInstalledSkill: (request: InstalledSkillRequest, timeoutMs?: number) => Promise<InstalledSkillDownloadResult>;
  uninstallInstalledSkill: (request: InstalledSkillRequest, timeoutMs?: number) => Promise<SkillTargetOperationResult>;
  updateLibrarySkillAbout: (skillId: string, about: string) => Promise<SkillPack[]>;
  listTasks: () => Promise<TaskRun[]>;
  queryTasks: (query?: TaskQuery) => Promise<TaskPage>;
  getTask: (taskId: string) => Promise<TaskRun | null>;
  acknowledgeTask: (taskId: string) => Promise<boolean>;
  clearTaskHistory: () => Promise<number>;
  recordFrontendError: (message: string) => Promise<TaskRun>;
  onTaskUpdated: (handler: TaskUpdatedHandler) => Promise<UnlistenFn>;
  getStorageHealth: () => Promise<StorageHealth[]>;
  previewStorageMigration: (store: string) => Promise<StorageMigrationPlan>;
  applyStorageMigration: (plan: StorageMigrationPlan) => Promise<StorageHealth>;
  previewStorageRestore: (store: string) => Promise<StorageRestorePlan>;
  restoreStorageBackup: (plan: StorageRestorePlan) => Promise<StorageHealth>;
};
