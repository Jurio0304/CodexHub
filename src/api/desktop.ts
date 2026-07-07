import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppUpdateStatus,
  CcSwitchDetection,
  HostDraft,
  HostPatch,
  HostResourceBatchResult,
  InstalledSkillRequest,
  LatestCodexVersion,
  LocalCodexStatus,
  NetworkProxyStatus,
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
  SkillDetectionResult,
  SkillInventoryStatus,
  SkillImportResult,
  SkillPack,
  SkillTargetOperationResult,
  SkillTargetRequest,
  SkillTargetsResult,
  SshBootstrapProgressEvent,
  SshBootstrapResult,
  SshConfigDeleteResult,
  SshHostDraft,
  TaskRun
} from "../models";
import type { AppSettings, CloseButtonBehavior } from "../settings";
import { loadLocalSettings, normalizeSettings, saveLocalSettings } from "../settings";
import type { CodexHubApi } from "./contracts";
import {
  fallbackAppUpdateStatus,
  fallbackHealth,
  fallbackNetworkProxyStatus,
  fallbackSshConfigHosts,
  fallbackSshStatus
} from "./fallbacks";
import { hasTauriRuntime, requiredInvoke, safeInvoke } from "./invoke";
import { mockApi, normalizeProfile, normalizeProfileApplyResult } from "./mock";

export const api: CodexHubApi = {
  getHealth: () => safeInvoke("app_health", undefined, fallbackHealth),
  getAppUpdateStatus: () => safeInvoke("get_app_update_status", undefined, fallbackAppUpdateStatus),
  checkStableUpdate: () => requiredInvoke<AppUpdateStatus>("check_stable_update"),
  installStableUpdate: () => requiredInvoke<AppUpdateStatus>("install_stable_update"),
  detectNetworkProxy: () => safeInvoke<NetworkProxyStatus>("detect_network_proxy", undefined, fallbackNetworkProxyStatus),
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
  chooseCloseButtonBehavior: (behavior: Exclude<CloseButtonBehavior, "ask">) => {
    const normalized = normalizeSettings({ ...loadLocalSettings(), closeButtonBehavior: behavior });
    if (!hasTauriRuntime()) {
      saveLocalSettings(normalized);
      return Promise.resolve(normalized);
    }
    return requiredInvoke<AppSettings>("choose_close_button_behavior", { behavior }).then((saved) => {
      const nextSettings = normalizeSettings(saved);
      saveLocalSettings(nextSettings);
      return nextSettings;
    });
  },
  onCloseButtonBehaviorRequested: async (handler: () => void): Promise<UnlistenFn> => {
    if (!hasTauriRuntime()) return () => {};
    return listen("close-button-behavior-requested", () => handler());
  },
  getSshStatus: () => safeInvoke("get_ssh_status", undefined, () => fallbackSshStatus()),
  generateEd25519Key: () => requiredInvoke("generate_ed25519_key"),
  listSshConfigHosts: () => safeInvoke("list_ssh_config_hosts", undefined, () => mockApi.listSshConfigHosts()),
  upsertSshConfigHost: (draft: SshHostDraft) => requiredInvoke("upsert_ssh_config_host", { draft }),
  deleteSshConfigHost: async (alias: string): Promise<SshConfigDeleteResult> => {
    if (hasTauriRuntime()) {
      return requiredInvoke("delete_ssh_config_host", { alias });
    }
    return mockApi.deleteSshConfigHost(alias);
  },
  listHosts: () => safeInvoke("list_hosts", undefined, () => mockApi.listHosts()),
  refreshDiscoveredHosts: () => safeInvoke("refresh_discovered_hosts", undefined, () => mockApi.refreshDiscoveredHosts()),
  refreshLatestCodexVersion: (force = false, timeoutMs = 30000) =>
    safeInvoke<LatestCodexVersion>("refresh_latest_codex_version", { force, timeoutMs }, () =>
      mockApi.refreshLatestCodexVersion(force, timeoutMs)
    ),
  getLocalCodexStatus: () => safeInvoke<LocalCodexStatus>("get_local_codex_status", undefined, () => mockApi.getLocalCodexStatus()),
  addHost: (draft: HostDraft) => safeInvoke("add_host", { draft }, () => mockApi.addHost(draft)),
  updateHost: (id: string, patch: HostPatch) => safeInvoke("update_host", { id, patch }, () => mockApi.updateHost(id, patch)),
  deleteHost: (id: string) => safeInvoke("delete_host", { id }, () => mockApi.deleteHost(id)),
  testSshConnection: (id: string) => safeInvoke("test_ssh_connection", { id }, () => mockApi.testSshConnection(id)),
  sshCheck: (hostAlias: string, timeoutMs = 10000) =>
    safeInvoke("ssh_check", { hostAlias, timeoutMs }, () => mockApi.sshCheck(hostAlias, timeoutMs)),
  connectSshHost: async (
    draft: SshHostDraft,
    password: string,
    requestId: string,
    onProgress?: (event: SshBootstrapProgressEvent) => void,
    timeoutMs = 10000
  ): Promise<SshBootstrapResult> => {
    if (!hasTauriRuntime()) {
      return mockApi.connectSshHost(draft, password, requestId, onProgress, timeoutMs);
    }

    let unlisten: UnlistenFn | null = null;
    if (onProgress) {
      unlisten = await listen<SshBootstrapProgressEvent>("ssh-bootstrap-progress", (event) => {
        if (event.payload.requestId === requestId) onProgress(event.payload);
      });
    }

    try {
      return await requiredInvoke("bootstrap_ssh_host", {
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
    safeInvoke("bootstrap_ssh_host", { draft, password, timeoutMs }, () => mockApi.bootstrapSshHost(draft, password, timeoutMs)),
  bootstrapExistingSshHost: (hostAlias: string, password: string, timeoutMs = 10000) =>
    safeInvoke("bootstrap_existing_ssh_host", { hostAlias, password, timeoutMs }, () =>
      mockApi.bootstrapExistingSshHost(hostAlias, password, timeoutMs)
    ),
  remoteProbeCodex: (hostAlias: string, timeoutMs = 10000) =>
    safeInvoke<RemoteProbeResult>("remote_probe_codex", { hostAlias, timeoutMs }, () => mockApi.remoteProbeCodex(hostAlias, timeoutMs)),
  sampleHostResources: (hostAliases: string[], timeoutMs = 8000) =>
    safeInvoke<HostResourceBatchResult>("sample_host_resources", { hostAliases, timeoutMs }, () =>
      mockApi.sampleHostResources(hostAliases, timeoutMs)
    ),
  remoteManageCodex: async (
    hostAlias: string,
    action: RemoteCodexAction,
    timeoutMs = 120000,
    requestId?: string,
    onProgress?: (event: RemoteCodexProgressEvent) => void
  ): Promise<RemoteCodexMaintenanceResult> => {
    if (!hasTauriRuntime()) {
      return mockApi.remoteManageCodex(hostAlias, action, timeoutMs, requestId, onProgress);
    }

    let unlisten: UnlistenFn | null = null;
    if (requestId && onProgress) {
      unlisten = await listen<RemoteCodexProgressEvent>("remote-codex-progress", (event) => {
        if (event.payload.requestId === requestId) onProgress(event.payload);
      });
    }

    try {
      return await requiredInvoke("remote_manage_codex", {
        hostAlias,
        action,
        timeoutMs,
        requestId
      });
    } finally {
      unlisten?.();
    }
  },
  listProfiles: () => safeInvoke<Profile[]>("list_profiles", undefined, () => mockApi.listProfiles()).then((profiles) => profiles.map(normalizeProfile)),
  createProfile: (draft: ProfileDraft) =>
    safeInvoke<Profile>("create_profile", { draft }, () => mockApi.createProfile(draft)).then(normalizeProfile),
  updateProfile: (id: string, patch: ProfilePatch) =>
    safeInvoke<Profile>("update_profile", { id, patch }, () => mockApi.updateProfile(id, patch)).then(normalizeProfile),
  deleteProfile: (id: string) => safeInvoke("delete_profile", { id }, () => mockApi.deleteProfile(id)),
  duplicateProfile: (id: string) => safeInvoke<Profile>("duplicate_profile", { id }, () => mockApi.duplicateProfile(id)).then(normalizeProfile),
  importProfiles: (bundle: ProfileImportExport) =>
    safeInvoke<ProfileImportExport>("import_profiles", { bundle }, () => mockApi.importProfiles(bundle)).then((result) => ({
      ...result,
      profiles: result.profiles.map(normalizeProfile)
    })),
  setProfileApiKey: async (profileId: string, apiKey: string) => {
    if (hasTauriRuntime()) {
      return requiredInvoke<Profile>("set_profile_api_key", { profileId, apiKey }).then(normalizeProfile);
    }
    return mockApi.setProfileApiKey(profileId, apiKey);
  },
  getProfileApiKey: async (profileId: string) => {
    if (hasTauriRuntime()) {
      return requiredInvoke("get_profile_api_key", { profileId });
    }
    return mockApi.getProfileApiKey(profileId);
  },
  deleteProfileApiKey: async (profileId: string) => {
    if (hasTauriRuntime()) {
      return requiredInvoke<Profile>("delete_profile_api_key", { profileId }).then(normalizeProfile);
    }
    return mockApi.deleteProfileApiKey(profileId);
  },
  previewProfileApply: (profileId: string, hostIds: string[]) =>
    safeInvoke<ProfileApplyPreview>("preview_profile_apply", { profileId, hostIds }, () => mockApi.previewProfileApply(profileId, hostIds)),
  applyProfile: async (profileId: string, hostIds: string[]) => {
    if (hasTauriRuntime()) {
      return requiredInvoke<ProfileApplyBatchResult>("apply_profile", { profileId, hostIds }).then(normalizeProfileApplyResult);
    }
    return mockApi.applyProfile(profileId, hostIds);
  },
  detectCcSwitchProfiles: () =>
    safeInvoke<CcSwitchDetection>("detect_cc_switch_profiles", undefined, () => mockApi.detectCcSwitchProfiles()).then((detection) => ({
      ...detection,
      importExport: { ...detection.importExport, profiles: detection.importExport.profiles.map(normalizeProfile) }
    })),
  importCcSwitchProfiles: (detection: CcSwitchDetection) =>
    safeInvoke<ProfileImportExport>("import_cc_switch_profiles", { detection }, () => mockApi.importCcSwitchProfiles(detection)).then((result) => ({
      ...result,
      profiles: result.profiles.map(normalizeProfile)
    })),
  listSkillPacks: () => safeInvoke<SkillPack[]>("list_local_skills", undefined, () => mockApi.listSkillPacks()),
  getSkillInventoryStatus: () =>
    safeInvoke<SkillInventoryStatus>("get_skill_inventory_status", undefined, () => mockApi.getSkillInventoryStatus()),
  detectInstalledSkills: async (includeHosts: boolean, timeoutMs = 120000): Promise<SkillDetectionResult> => {
    if (hasTauriRuntime()) {
      return requiredInvoke("detect_installed_skills", { includeHosts, timeoutMs });
    }
    return mockApi.detectInstalledSkills(includeHosts, timeoutMs);
  },
  importLocalSkill: async (path: string): Promise<SkillImportResult> => {
    if (hasTauriRuntime()) {
      return requiredInvoke("import_local_skill", { path });
    }
    return mockApi.importLocalSkill(path);
  },
  downloadGithubSkill: async (repoUrl: string, timeoutMs = 120000): Promise<SkillImportResult> => {
    if (hasTauriRuntime()) {
      return requiredInvoke("download_github_skill", { repoUrl, timeoutMs });
    }
    return mockApi.downloadGithubSkill(repoUrl, timeoutMs);
  },
  getSkillTargets: async (skillId: string, timeoutMs = 30000): Promise<SkillTargetsResult> => {
    if (hasTauriRuntime()) {
      return requiredInvoke("get_skill_targets", { skillId, timeoutMs });
    }
    return mockApi.getSkillTargets(skillId, timeoutMs);
  },
  installSkillTargets: async (skillId: string, targets: SkillTargetRequest[], timeoutMs = 120000): Promise<SkillTargetOperationResult> => {
    if (hasTauriRuntime()) {
      return requiredInvoke("install_skill_targets", {
        skillId,
        targets,
        timeoutMs
      });
    }
    return mockApi.installSkillTargets(skillId, targets, timeoutMs);
  },
  uninstallSkillTargets: async (skillId: string, targets: SkillTargetRequest[], timeoutMs = 120000): Promise<SkillTargetOperationResult> => {
    if (hasTauriRuntime()) {
      return requiredInvoke("uninstall_skill_targets", {
        skillId,
        targets,
        timeoutMs
      });
    }
    return mockApi.uninstallSkillTargets(skillId, targets, timeoutMs);
  },
  deleteLibrarySkill: async (skillId: string, uninstallFirst: boolean, timeoutMs = 120000): Promise<SkillTargetOperationResult> => {
    if (hasTauriRuntime()) {
      return requiredInvoke("delete_library_skill", { skillId, uninstallFirst, timeoutMs });
    }
    return mockApi.deleteLibrarySkill(skillId, uninstallFirst, timeoutMs);
  },
  downloadInstalledSkill: async (request: InstalledSkillRequest, timeoutMs = 120000) => {
    if (hasTauriRuntime()) {
      return requiredInvoke("download_installed_skill", { request, timeoutMs });
    }
    return mockApi.downloadInstalledSkill(request, timeoutMs);
  },
  uninstallInstalledSkill: async (request: InstalledSkillRequest, timeoutMs = 120000) => {
    if (hasTauriRuntime()) {
      return requiredInvoke("uninstall_installed_skill", { request, timeoutMs });
    }
    return mockApi.uninstallInstalledSkill(request, timeoutMs);
  },
  updateLibrarySkillAbout: async (skillId: string, about: string) => {
    if (hasTauriRuntime()) {
      return requiredInvoke<SkillPack[]>("update_library_skill_about", { skillId, about });
    }
    return mockApi.updateLibrarySkillAbout(skillId, about);
  },
  listTasks: () => safeInvoke<TaskRun[]>("list_tasks", undefined, () => mockApi.listTasks())
};

void fallbackSshConfigHosts;
