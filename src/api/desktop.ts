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
import type { AppSettings, CloseButtonBehavior, SettingsSaveResult } from "../settings";
import { normalizeSettings, saveDesktopSettingsCache } from "../settings";
import type { CodexHubApi } from "./contracts";
import { assertTauriRuntime, requireHostAlias, requiredInvoke } from "./invoke";
import { normalizeProfile, normalizeProfileApplyResult } from "./normalize";

export const desktopApi: CodexHubApi = {
  getHealth: () => requiredInvoke("app_health"),
  getAppUpdateStatus: () => requiredInvoke("get_app_update_status"),
  checkStableUpdate: () => requiredInvoke<AppUpdateStatus>("check_stable_update"),
  installStableUpdate: () => requiredInvoke<AppUpdateStatus>("install_stable_update"),
  detectNetworkProxy: () => requiredInvoke<NetworkProxyStatus>("detect_network_proxy"),
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
  getSshStatus: () => requiredInvoke("get_ssh_status"),
  generateEd25519Key: () => requiredInvoke("generate_ed25519_key"),
  listSshConfigHosts: () => requiredInvoke("list_ssh_config_hosts"),
  upsertSshConfigHost: (draft: SshHostDraft) => requiredInvoke("upsert_ssh_config_host", { draft }),
  deleteSshConfigHost: (alias: string): Promise<SshConfigDeleteResult> =>
    requiredInvoke("delete_ssh_config_host", { alias: requireHostAlias("delete_ssh_config_host", alias) }),
  listHosts: () => requiredInvoke("list_hosts"),
  refreshDiscoveredHosts: () => requiredInvoke("refresh_discovered_hosts"),
  refreshLatestCodexVersion: (force = false, timeoutMs = 30000) =>
    requiredInvoke<LatestCodexVersion>("refresh_latest_codex_version", { force, timeoutMs }),
  getLocalCodexStatus: () => requiredInvoke<LocalCodexStatus>("get_local_codex_status"),
  addHost: (draft: HostDraft) => requiredInvoke("add_host", { draft }),
  updateHost: (id: string, patch: HostPatch) => requiredInvoke("update_host", { id, patch }),
  deleteHost: (id: string) => requiredInvoke("delete_host", { id }),
  testSshConnection: (id: string) => requiredInvoke("test_ssh_connection", { id }),
  sshCheck: (hostAlias: string, timeoutMs = 10000) =>
    requiredInvoke("ssh_check", { hostAlias: requireHostAlias("ssh_check", hostAlias), timeoutMs }),
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
      return await requiredInvoke("bootstrap_ssh_host", {
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
    requiredInvoke("bootstrap_ssh_host", {
      draft: { ...draft, alias: requireHostAlias("bootstrap_ssh_host", draft.alias) },
      password,
      timeoutMs
    }),
  bootstrapExistingSshHost: (hostAlias: string, password: string, timeoutMs = 10000) =>
    requiredInvoke("bootstrap_existing_ssh_host", {
      hostAlias: requireHostAlias("bootstrap_existing_ssh_host", hostAlias),
      password,
      timeoutMs
    }),
  remoteProbeCodex: (hostAlias: string, timeoutMs = 10000) =>
    requiredInvoke<RemoteProbeResult>("remote_probe_codex", {
      hostAlias: requireHostAlias("remote_probe_codex", hostAlias),
      timeoutMs
    }),
  sampleHostResources: (hostAliases: string[], timeoutMs = 8000) =>
    requiredInvoke<HostResourceBatchResult>("sample_host_resources", {
      hostAliases: hostAliases.map((alias) => requireHostAlias("sample_host_resources", alias)),
      timeoutMs
    }),
  remoteManageCodex: async (
    hostAlias: string,
    action: RemoteCodexAction,
    timeoutMs = 120000,
    requestId?: string,
    onProgress?: (event: RemoteCodexProgressEvent) => void
  ): Promise<RemoteCodexMaintenanceResult> => {
    const alias = requireHostAlias("remote_manage_codex", hostAlias);
    let unlisten: UnlistenFn | null = null;
    assertTauriRuntime("remote_manage_codex");
    if (requestId && onProgress) {
      unlisten = await listen<RemoteCodexProgressEvent>("remote-codex-progress", (event) => {
        if (event.payload.requestId === requestId) onProgress(event.payload);
      });
    }

    try {
      return await requiredInvoke("remote_manage_codex", {
        hostAlias: alias,
        action,
        timeoutMs,
        requestId
      });
    } finally {
      unlisten?.();
    }
  },
  listProfiles: () => requiredInvoke<Profile[]>("list_profiles").then((profiles) => profiles.map(normalizeProfile)),
  createProfile: (draft: ProfileDraft) =>
    requiredInvoke<Profile>("create_profile", { draft }).then(normalizeProfile),
  updateProfile: (id: string, patch: ProfilePatch) =>
    requiredInvoke<Profile>("update_profile", { id, patch }).then(normalizeProfile),
  deleteProfile: (id: string) => requiredInvoke("delete_profile", { id }),
  duplicateProfile: (id: string) => requiredInvoke<Profile>("duplicate_profile", { id }).then(normalizeProfile),
  importProfiles: (bundle: ProfileImportExport) =>
    requiredInvoke<ProfileImportExport>("import_profiles", { bundle }).then((result) => ({
      ...result,
      profiles: result.profiles.map(normalizeProfile)
    })),
  setProfileApiKey: (profileId: string, apiKey: string) =>
    requiredInvoke<Profile>("set_profile_api_key", { profileId, apiKey }).then(normalizeProfile),
  getProfileCredentialStatus: (profileId: string) => requiredInvoke("get_profile_credential_status", { profileId }),
  deleteProfileApiKey: (profileId: string) =>
    requiredInvoke<Profile>("delete_profile_api_key", { profileId }).then(normalizeProfile),
  previewProfileApply: (profileId: string, hostIds: string[]) =>
    requiredInvoke<ProfileApplyPreview>("preview_profile_apply", { profileId, hostIds }),
  applyProfile: (profileId: string, hostIds: string[]) =>
    requiredInvoke<ProfileApplyBatchResult>("apply_profile", { profileId, hostIds }).then(normalizeProfileApplyResult),
  detectCcSwitchProfiles: () =>
    requiredInvoke<CcSwitchDetection>("detect_cc_switch_profiles").then((detection) => ({
      ...detection,
      importExport: { ...detection.importExport, profiles: detection.importExport.profiles.map(normalizeProfile) }
    })),
  importCcSwitchProfiles: (detection: CcSwitchDetection) =>
    requiredInvoke<ProfileImportExport>("import_cc_switch_profiles", { detection }).then((result) => ({
      ...result,
      profiles: result.profiles.map(normalizeProfile)
    })),
  listSkillPacks: () => requiredInvoke<SkillPack[]>("list_local_skills"),
  getSkillInventoryStatus: () => requiredInvoke<SkillInventoryStatus>("get_skill_inventory_status"),
  detectInstalledSkills: (includeHosts: boolean, timeoutMs = 120000): Promise<SkillDetectionResult> =>
    requiredInvoke("detect_installed_skills", { includeHosts, timeoutMs }),
  importLocalSkill: (path: string): Promise<SkillImportResult> => requiredInvoke("import_local_skill", { path }),
  downloadGithubSkill: (repoUrl: string, timeoutMs = 120000): Promise<SkillImportResult> =>
    requiredInvoke("download_github_skill", { repoUrl, timeoutMs }),
  getSkillTargets: (skillId: string, timeoutMs = 30000): Promise<SkillTargetsResult> =>
    requiredInvoke("get_skill_targets", { skillId, timeoutMs }),
  installSkillTargets: (skillId: string, targets: SkillTargetRequest[], timeoutMs = 120000): Promise<SkillTargetOperationResult> =>
    requiredInvoke("install_skill_targets", { skillId, targets: validateSkillTargets("install_skill_targets", targets), timeoutMs }),
  uninstallSkillTargets: (skillId: string, targets: SkillTargetRequest[], timeoutMs = 120000): Promise<SkillTargetOperationResult> =>
    requiredInvoke("uninstall_skill_targets", { skillId, targets: validateSkillTargets("uninstall_skill_targets", targets), timeoutMs }),
  deleteLibrarySkill: (skillId: string, uninstallFirst: boolean, timeoutMs = 120000): Promise<SkillTargetOperationResult> =>
    requiredInvoke("delete_library_skill", { skillId, uninstallFirst, timeoutMs }),
  downloadInstalledSkill: (request: InstalledSkillRequest, timeoutMs = 120000) =>
    requiredInvoke("download_installed_skill", { request: validateInstalledSkillRequest("download_installed_skill", request), timeoutMs }),
  uninstallInstalledSkill: (request: InstalledSkillRequest, timeoutMs = 120000) =>
    requiredInvoke("uninstall_installed_skill", { request: validateInstalledSkillRequest("uninstall_installed_skill", request), timeoutMs }),
  updateLibrarySkillAbout: (skillId: string, about: string) =>
    requiredInvoke<SkillPack[]>("update_library_skill_about", { skillId, about }),
  listTasks: () => requiredInvoke<TaskRun[]>("list_tasks")
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
