import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type {
  CcSwitchDetection,
  HostDraft,
  HostOperationProgressEvent,
  HostPatch,
  HostResourceProgressEvent,
  InstalledSkillRequest,
  ProfileDraft,
  ProfileImportExport,
  ProfileApplyOptions,
  ProfilePatch,
  RemoteCodexAction,
  RemoteCodexBatchResult,
  RemoteCodexMaintenanceResult,
  RemoteProbeBatchItemCompletedEvent,
  SkillDetectionResult,
  SkillImportResult,
  SkillTargetOperationResult,
  SkillTargetRequest,
  SkillTargetsResult,
  SshBootstrapProgressEvent,
  SshBootstrapResult,
  SshConfigDeleteResult,
  SshHostDraft,
  TaskRun,
  RemoteProbeBatchResult
} from "../models";
import type { AppSettings, CloseButtonBehavior, SettingsSaveResult } from "../settings";
import type {
  AppUpdateStatusDto,
  CcSwitchDetectionDto,
  ConnectionTestDto,
  HealthDto,
  HostDto,
  HostResourceBatchResultDto,
  LatestCodexVersionDto,
  LocalCodexStatusDto,
  NetworkProxyStatusDto,
  ProfileApiKeyResultDto,
  ProfileApplyBatchResultDto,
  ProfileDto,
  ProfileImportExportDto,
  RemoteCodexMaintenanceResultDto,
  RemoteProbeResultDto,
  SkillDetectionResultDto,
  SkillImportResultDto,
  SkillInventoryStatusDto,
  SkillPackDto,
  SkillTargetOperationResultDto,
  SkillTargetsResultDto,
  SshBootstrapResultDto,
  SshCheckResultDto,
  SshConfigDeleteResultDto,
  SshConfigHostDto,
  SshConfigWriteResultDto,
  SshKeyGenerationResultDto,
  SshStatusDto,
  StorageHealth,
  StorageMigrationPlan,
  StorageRestorePlan,
  ProfileApplyPreviewDto,
  TaskEvent,
  TaskPage,
  TaskQuery
} from "../generated/rust-contracts";
import { normalizeSettings, saveDesktopSettingsCache } from "../settings";
import type { CodexHubApi } from "./contracts";
import type { TauriCommand } from "./commands";
import { assertTauriRuntime, requireHostAlias, requiredInvoke } from "./invoke";
import {
  normalizeHost,
  normalizeProfile,
  normalizeProfileApplyOptions,
  normalizeProfileApplyPreview,
  normalizeProfileApplyResult
} from "./normalize";

async function runWithHostOperationProgress<T>(
  command: TauriCommand,
  requestId: string | undefined,
  onProgress: ((event: HostOperationProgressEvent) => void) | undefined,
  invoke: () => Promise<T>
) {
  let unlisten: UnlistenFn | null = null;
  assertTauriRuntime(command);
  if (requestId && onProgress) {
    unlisten = await listen<HostOperationProgressEvent>("host-operation-progress", (event) => {
      if (event.payload.requestId === requestId) onProgress(event.payload);
    });
  }

  try {
    return await invoke();
  } finally {
    unlisten?.();
  }
}

async function runWithHostResourceProgress<T>(
  requestId: string | undefined,
  onProgress: ((event: HostResourceProgressEvent) => void) | undefined,
  invoke: () => Promise<T>
) {
  let unlisten: UnlistenFn | null = null;
  assertTauriRuntime("sample_host_resources");
  if (requestId && onProgress) {
    unlisten = await listen<HostResourceProgressEvent>("host-resource-progress", (event) => {
      if (event.payload.requestId === requestId) onProgress(event.payload);
    });
  }

  try {
    return await invoke();
  } finally {
    unlisten?.();
  }
}

async function runWithRemoteProbeBatchEvents<T>(
  requestId: string | undefined,
  onProgress: ((event: HostOperationProgressEvent) => void) | undefined,
  onItemCompleted: ((event: RemoteProbeBatchItemCompletedEvent) => void) | undefined,
  invoke: () => Promise<T>
) {
  let unlistenProgress: UnlistenFn | null = null;
  let unlistenCompleted: UnlistenFn | null = null;
  assertTauriRuntime("batch_remote_probe_codex");
  try {
    if (requestId && onProgress) {
      unlistenProgress = await listen<HostOperationProgressEvent>("host-operation-progress", (event) => {
        if (event.payload.requestId === requestId) onProgress(event.payload);
      });
    }
    if (requestId && onItemCompleted) {
      unlistenCompleted = await listen<RemoteProbeBatchItemCompletedEvent>("remote-probe-batch-item-completed", (event) => {
        if (event.payload.requestId === requestId) onItemCompleted(event.payload);
      });
    }
    return await invoke();
  } finally {
    unlistenCompleted?.();
    unlistenProgress?.();
  }
}

export const desktopApi: CodexHubApi = {
  getHealth: () => requiredInvoke<HealthDto>("app_health"),
  getAppUpdateStatus: () => requiredInvoke<AppUpdateStatusDto>("get_app_update_status"),
  checkStableUpdate: () => requiredInvoke<AppUpdateStatusDto>("check_stable_update"),
  installStableUpdate: () => requiredInvoke<AppUpdateStatusDto>("install_stable_update"),
  detectNetworkProxy: () => requiredInvoke<NetworkProxyStatusDto>("detect_network_proxy"),
  getSettings: () =>
    requiredInvoke<AppSettings>("get_settings").then((settings) => {
      const normalized = normalizeSettings(settings);
      saveDesktopSettingsCache(normalized);
      return normalized;
    }),
  saveSettings: (settings: AppSettings) => {
    const normalized = normalizeSettings(settings);
    return requiredInvoke<SettingsSaveResult>("save_settings", { settings: normalized }).then((saved) => {
      const nextSettings = normalizeSettings(saved.settings);
      saveDesktopSettingsCache(nextSettings);
      return { ...saved, settings: nextSettings };
    });
  },
  chooseCloseButtonBehavior: (behavior: Exclude<CloseButtonBehavior, "ask">) => {
    return requiredInvoke<SettingsSaveResult>("choose_close_button_behavior", { behavior }).then((saved) => {
      const nextSettings = normalizeSettings(saved.settings);
      saveDesktopSettingsCache(nextSettings);
      return { ...saved, settings: nextSettings };
    });
  },
  onCloseButtonBehaviorRequested: async (handler: () => void): Promise<UnlistenFn> => {
    assertTauriRuntime("choose_close_button_behavior");
    return listen("close-button-behavior-requested", () => handler());
  },
  getSshStatus: () => requiredInvoke<SshStatusDto>("get_ssh_status"),
  generateEd25519Key: () => requiredInvoke<SshKeyGenerationResultDto>("generate_ed25519_key"),
  listSshConfigHosts: () => requiredInvoke<SshConfigHostDto[]>("list_ssh_config_hosts"),
  upsertSshConfigHost: (draft: SshHostDraft) => requiredInvoke<SshConfigWriteResultDto>("upsert_ssh_config_host", { draft }),
  deleteSshConfigHost: (alias: string): Promise<SshConfigDeleteResult> =>
    requiredInvoke<SshConfigDeleteResultDto>("delete_ssh_config_host", { alias: requireHostAlias("delete_ssh_config_host", alias) }),
  listHosts: () => requiredInvoke<HostDto[]>("list_hosts").then((hosts) => hosts.map(normalizeHost)),
  refreshDiscoveredHosts: () => requiredInvoke<HostDto[]>("refresh_discovered_hosts").then((hosts) => hosts.map(normalizeHost)),
  refreshLatestCodexVersion: (force = false, timeoutMs = 30000) =>
    requiredInvoke<LatestCodexVersionDto>("refresh_latest_codex_version", { force, timeoutMs }),
  getLocalCodexStatus: () => requiredInvoke<LocalCodexStatusDto>("get_local_codex_status"),
  addHost: (draft: HostDraft) => requiredInvoke("add_host", { draft }),
  updateHost: (id: string, patch: HostPatch) => requiredInvoke("update_host", { id, patch }),
  deleteHost: (id: string) => requiredInvoke("delete_host", { id }),
  testSshConnection: (id: string) => requiredInvoke<ConnectionTestDto>("test_ssh_connection", { id }),
  sshCheck: (hostAlias: string, timeoutMs = 10000) =>
    requiredInvoke<SshCheckResultDto>("ssh_check", { hostAlias: requireHostAlias("ssh_check", hostAlias), timeoutMs }),
  connectSshHost: async (
    draft: SshHostDraft,
    password: string,
    requestId: string,
    onProgress?: (event: SshBootstrapProgressEvent) => void,
    timeoutMs = 10000
  ): Promise<SshBootstrapResult> => {
    const alias = requireHostAlias("bootstrap_ssh_host", draft.alias);
    let unlisten: UnlistenFn | null = null;
    assertTauriRuntime("bootstrap_ssh_host");
    if (onProgress) {
      unlisten = await listen<SshBootstrapProgressEvent>("ssh-bootstrap-progress", (event) => {
        if (event.payload.requestId === requestId) onProgress(event.payload);
      });
    }

    try {
      return await requiredInvoke<SshBootstrapResultDto>("bootstrap_ssh_host", {
        draft: { ...draft, alias },
        password,
        timeoutMs,
        requestId
      });
    } finally {
      unlisten?.();
    }
  },
  bootstrapSshHost: (draft: SshHostDraft, password: string, timeoutMs = 10000) =>
    requiredInvoke<SshBootstrapResultDto>("bootstrap_ssh_host", {
      draft: { ...draft, alias: requireHostAlias("bootstrap_ssh_host", draft.alias) },
      password,
      timeoutMs
    }),
  bootstrapExistingSshHost: (hostAlias: string, password: string, timeoutMs = 10000) =>
    requiredInvoke<SshBootstrapResultDto>("bootstrap_existing_ssh_host", {
      hostAlias: requireHostAlias("bootstrap_existing_ssh_host", hostAlias),
      password,
      timeoutMs
    }),
  remoteProbeCodex: async (
    hostAlias: string,
    timeoutMs = 10000,
    requestId?: string,
    onProgress?: (event: HostOperationProgressEvent) => void
  ) => runWithHostOperationProgress(
    "remote_probe_codex",
    requestId,
    onProgress,
    () => requiredInvoke<RemoteProbeResultDto>("remote_probe_codex", {
      hostAlias: requireHostAlias("remote_probe_codex", hostAlias),
      timeoutMs,
      requestId
    })
  ),
  batchRemoteProbeCodex: async (
    hostAliases: string[],
    timeoutMs = 10000,
    requestId?: string,
    onProgress?: (event: HostOperationProgressEvent) => void,
    onItemCompleted?: (event: RemoteProbeBatchItemCompletedEvent) => void
  ) => runWithRemoteProbeBatchEvents(
    requestId,
    onProgress,
    onItemCompleted,
    () => requiredInvoke<RemoteProbeBatchResult>("batch_remote_probe_codex", {
      hostAliases: hostAliases.map((alias) => requireHostAlias("batch_remote_probe_codex", alias)),
      timeoutMs,
      requestId
    })
  ),
  sampleHostResources: (
    hostAliases: string[],
    timeoutMs = 10000,
    recordTask = true,
    requestId?: string,
    onProgress?: (event: HostResourceProgressEvent) => void
  ) => runWithHostResourceProgress(
    requestId,
    onProgress,
    () => requiredInvoke<HostResourceBatchResultDto>("sample_host_resources", {
      hostAliases: hostAliases.map((alias) => requireHostAlias("sample_host_resources", alias)),
      timeoutMs,
      recordTask,
      requestId
    })
  ),
  remoteManageCodex: async (
    hostAlias: string,
    action: RemoteCodexAction,
    timeoutMs = 120000,
    requestId?: string,
    onProgress?: (event: HostOperationProgressEvent) => void
  ): Promise<RemoteCodexMaintenanceResult> => {
    const alias = requireHostAlias("remote_manage_codex", hostAlias);
    return runWithHostOperationProgress(
      "remote_manage_codex",
      requestId,
      onProgress,
      () => requiredInvoke<RemoteCodexMaintenanceResultDto>("remote_manage_codex", {
        hostAlias: alias,
        action,
        timeoutMs,
        requestId
      })
    );
  },
  batchRemoteUpdateCodex: async (
    hostAliases: string[],
    timeoutMs = 120000,
    requestId?: string,
    onProgress?: (event: HostOperationProgressEvent) => void
  ) => runWithHostOperationProgress(
    "batch_remote_update_codex",
    requestId,
    onProgress,
    () => requiredInvoke<RemoteCodexBatchResult>("batch_remote_update_codex", {
      hostAliases: hostAliases.map((alias) => requireHostAlias("batch_remote_update_codex", alias)),
      timeoutMs,
      requestId
    })
  ),
  listProfiles: () => requiredInvoke<ProfileDto[]>("list_profiles").then((profiles) => profiles.map(normalizeProfile)),
  createProfile: (draft: ProfileDraft) =>
    requiredInvoke<ProfileDto>("create_profile", { draft }).then(normalizeProfile),
  updateProfile: (id: string, patch: ProfilePatch) =>
    requiredInvoke<ProfileDto>("update_profile", { id, patch }).then(normalizeProfile),
  deleteProfile: (id: string) => requiredInvoke("delete_profile", { id }),
  duplicateProfile: (id: string) => requiredInvoke<ProfileDto>("duplicate_profile", { id }).then(normalizeProfile),
  importProfiles: (bundle: ProfileImportExport) =>
    requiredInvoke<ProfileImportExportDto>("import_profiles", { bundle }).then((result) => ({
      ...result,
      profiles: result.profiles.map(normalizeProfile)
    })),
  setProfileApiKey: (profileId: string, apiKey: string) =>
    requiredInvoke<ProfileDto>("set_profile_api_key", { profileId, apiKey }).then(normalizeProfile),
  getProfileApiKey: (profileId: string) => requiredInvoke<ProfileApiKeyResultDto>("get_profile_api_key", { profileId }),
  deleteProfileApiKey: (profileId: string) =>
    requiredInvoke<ProfileDto>("delete_profile_api_key", { profileId }).then(normalizeProfile),
  previewProfileApply: (profileId: string, hostIds: string[]) =>
    requiredInvoke<ProfileApplyPreviewDto>("preview_profile_apply", { profileId, hostIds })
      .then(normalizeProfileApplyPreview),
  applyProfile: (profileId: string, hostIds: string[], options: ProfileApplyOptions) =>
    requiredInvoke<ProfileApplyBatchResultDto>("apply_profile", {
      profileId,
      hostIds,
      options: normalizeProfileApplyOptions(options)
    }).then(normalizeProfileApplyResult),
  detectCcSwitchProfiles: () =>
    requiredInvoke<CcSwitchDetectionDto>("detect_cc_switch_profiles").then((detection) => ({
      ...detection,
      importExport: { ...detection.importExport, profiles: detection.importExport.profiles.map(normalizeProfile) }
    })),
  importCcSwitchProfiles: (_detection: CcSwitchDetection) =>
    requiredInvoke<ProfileImportExportDto>("import_cc_switch_profiles", { replace: false }).then((result) => ({
      ...result,
      profiles: result.profiles.map(normalizeProfile)
    })),
  listSkillPacks: () => requiredInvoke<SkillPackDto[]>("list_local_skills"),
  getSkillInventoryStatus: () => requiredInvoke<SkillInventoryStatusDto>("get_skill_inventory_status"),
  detectInstalledSkills: (includeHosts: boolean, timeoutMs = 120000): Promise<SkillDetectionResult> =>
    requiredInvoke<SkillDetectionResultDto>("detect_installed_skills", { includeHosts, timeoutMs }),
  importLocalSkill: (path: string): Promise<SkillImportResult> => requiredInvoke<SkillImportResultDto>("import_local_skill", { path }),
  downloadGithubSkill: (repoUrl: string, timeoutMs = 120000): Promise<SkillImportResult> =>
    requiredInvoke<SkillImportResultDto>("download_github_skill", { repoUrl, timeoutMs }),
  getSkillTargets: (skillId: string, timeoutMs = 30000): Promise<SkillTargetsResult> =>
    requiredInvoke<SkillTargetsResultDto>("get_skill_targets", { skillId, timeoutMs }),
  installSkillTargets: (skillId: string, targets: SkillTargetRequest[], timeoutMs = 120000): Promise<SkillTargetOperationResult> =>
    requiredInvoke<SkillTargetOperationResultDto>("install_skill_targets", { skillId, targets: validateSkillTargets("install_skill_targets", targets), timeoutMs }),
  uninstallSkillTargets: (skillId: string, targets: SkillTargetRequest[], timeoutMs = 120000): Promise<SkillTargetOperationResult> =>
    requiredInvoke<SkillTargetOperationResultDto>("uninstall_skill_targets", { skillId, targets: validateSkillTargets("uninstall_skill_targets", targets), timeoutMs }),
  deleteLibrarySkill: (skillId: string, uninstallFirst: boolean, timeoutMs = 120000): Promise<SkillTargetOperationResult> =>
    requiredInvoke<SkillTargetOperationResultDto>("delete_library_skill", { skillId, uninstallFirst, timeoutMs }),
  downloadInstalledSkill: (request: InstalledSkillRequest, timeoutMs = 120000) =>
    requiredInvoke("download_installed_skill", { request: validateInstalledSkillRequest("download_installed_skill", request), timeoutMs }),
  uninstallInstalledSkill: (request: InstalledSkillRequest, timeoutMs = 120000) =>
    requiredInvoke<SkillTargetOperationResultDto>("uninstall_installed_skill", { request: validateInstalledSkillRequest("uninstall_installed_skill", request), timeoutMs }),
  updateLibrarySkillAbout: (skillId: string, about: string) =>
    requiredInvoke<SkillPackDto[]>("update_library_skill_about", { skillId, about }),
  listTasks: () => requiredInvoke<TaskRun[]>("list_tasks"),
  queryTasks: (query?: TaskQuery) => requiredInvoke<TaskPage>("query_tasks", { query: query ?? null }),
  getTask: (taskId: string) => requiredInvoke<TaskRun | null>("get_task", { taskId }),
  acknowledgeTask: (taskId: string) => requiredInvoke<boolean>("acknowledge_task", { taskId }),
  clearTaskHistory: () => requiredInvoke<number>("clear_task_history"),
  recordFrontendError: (message: string) => requiredInvoke<TaskRun>("record_frontend_error", { message }),
  onTaskUpdated: async (handler: (event: TaskEvent) => void): Promise<UnlistenFn> => {
    assertTauriRuntime("query_tasks");
    return listen<TaskEvent>("task-updated", (event) => handler(event.payload));
  },
  getStorageHealth: () => requiredInvoke<StorageHealth[]>("get_storage_health"),
  previewStorageMigration: (store: string) =>
    requiredInvoke<StorageMigrationPlan>("preview_storage_migration", { store }),
  applyStorageMigration: (plan: StorageMigrationPlan) =>
    requiredInvoke<StorageHealth>("apply_storage_migration", { plan }),
  previewStorageRestore: (store: string) =>
    requiredInvoke<StorageRestorePlan>("preview_storage_restore", { store }),
  restoreStorageBackup: (plan: StorageRestorePlan) =>
    requiredInvoke<StorageHealth>("restore_storage_backup", { plan })
};

function validateSkillTargets(command: "install_skill_targets" | "uninstall_skill_targets", targets: SkillTargetRequest[]) {
  return targets.map((target) =>
    target.targetType === "host"
      ? { ...target, hostAlias: requireHostAlias(command, target.hostAlias) }
      : target
  );
}

function validateInstalledSkillRequest(
  command: "download_installed_skill" | "uninstall_installed_skill",
  request: InstalledSkillRequest
) {
  return request.targetType === "host"
    ? { ...request, hostAlias: requireHostAlias(command, request.hostAlias) }
    : request;
}
