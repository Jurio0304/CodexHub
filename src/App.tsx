import { createContext, useCallback, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { CSSProperties, FormEvent, MouseEventHandler, PointerEvent as ReactPointerEvent, ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { loadInitialAppData } from "./app/bootstrap";
import { api, apiMode, fallbackAppUpdateStatus, fallbackHealth, parseApiError } from "./api";
import type {
  AppUpdateStatus,
  DeleteOperationResult,
  Health,
  Host,
  HostResourceBatchResult,
  HostResourceSnapshot,
  HostStatus,
  InstalledSkillDownloadResult,
  InstalledSkillRequest,
  LatestCodexVersion,
  Profile,
  ProfileApiKeyResult,
  ProfileApplyBatchResult,
  ProfileApplyPreview,
  ProfileDraft,
  ProfileImportExport,
  ProfilePatch,
  CcSwitchDetection,
  RemoteCodexAction,
  RemoteCodexMaintenanceResult,
  RemoteCodexProgressEvent,
  RemoteSkill,
  SkillDetectionResult,
  SkillInventoryStatus,
  SkillImportResult,
  SkillApplication,
  SkillPack,
  SkillTarget,
  SkillTargetOperationResult,
  SkillTargetRequest,
  SkillTargetsResult,
  SshBootstrapProgressEvent,
  SshBootstrapResult,
  SshBootstrapStep,
  SshBootstrapStepStatus,
  SshConfigDeleteResult,
  SshConfigHost,
  SshHostDraft,
  SshKeyInfo,
  SshStatus,
  TaskRun,
  TaskStatus
} from "./models";
import type { ApiErrorCode, StorageHealth, StorageMigrationPlan, StorageRestorePlan } from "./generated/rust-contracts";
import { getPlatform, isWindows } from "./platform";
import type { RuntimePlatform } from "./platform";
import {
  applyAppSettings,
  fontPresets,
  loadDesktopSettingsCache,
  loadMockSettings,
  normalizeSettings,
  normalizeResourceMonitorRefreshSeconds,
  resolvePlatformAppearance
} from "./settings";
import type { AppSettings, CloseButtonBehavior, FontPreset, NetworkProxyMode, PlatformAppearance, ThemeChoice } from "./settings";
import { ConfirmDialog } from "./ui/ConfirmDialog";
import { AlertModalFrame } from "./ui/AlertModalFrame";
import { useFeedback } from "./ui/feedback";
import type { FeedbackPlacement, FeedbackTone } from "./ui/feedback";
import { ModalFrame } from "./ui/ModalFrame";

type SectionId = "dashboard" | "hosts" | "profiles" | "skills" | "monitor" | "tasks" | "settings";
type NavIconId = SectionId;
type PlatformIconId =
  | SectionId
  | "close"
  | "delete"
  | "download"
  | "install"
  | "key"
  | "language"
  | "network"
  | "preview"
  | "scan"
  | "terminal"
  | "update"
  | "warning";
type TitleBarAction = "minimize" | "maximize" | "close";
type Locale = "en" | "zh";
type HostBusyAction = "test" | "bootstrap" | RemoteCodexAction;
type BadgeTone = "green" | "yellow" | "red" | "blue" | "gray";
type CommandBarAction = {
  id: string;
  label: string;
  kind?: "primary" | "secondary" | "danger";
  disabled?: boolean;
  onClick: () => void;
};
type SetupGuideStep = "preferences" | "ssh";
type SectionCompletionTone = "success" | "error";
type SectionCompletionSignals = Partial<Record<SectionId, SectionCompletionTone>>;
type SectionOperationOptions<T> = {
  classify?: (result: T) => SectionCompletionTone | null;
};
type HostProbeOptions = {
  includeLatestVersion?: boolean;
  notifyCompletion?: boolean;
  refreshCatalog?: boolean;
};
type ResourceRefreshTrigger = "initial" | "manual" | "auto";
const MAX_VISIBLE_TASKS = 100;
type CodexOperationModalStatus = "running" | "success" | "failed";
type CodexOperationModalState = {
  hostAlias: string;
  hostName: string;
  action: RemoteCodexAction;
  status: CodexOperationModalStatus;
  logs: RemoteCodexProgressEvent[];
  task?: TaskRun;
  message?: string;
  error?: string;
};
type CodexUninstallConfirmState = {
  hostAlias: string;
  hostName: string;
};
type MonitorGpuUserUsage = {
  user: string;
  processCount: number;
  elapsedSeconds: number | null;
  usedMemoryBytes: number;
  color: string;
};
type MonitorGpuUserColorMap = ReadonlyMap<string, string>;
type MonitorMeterTone = "memory" | "cpu" | "gpu" | "green" | "yellow" | "red" | "gray";
type MonitorDragState = {
  alias: string;
  offsetX: number;
  offsetY: number;
  pointerId: number;
  previewOrder: string[];
  sourceOrder: string[];
  width: number;
  height: number;
  x: number;
  y: number;
};
type ProfileApplyOperationModalState = {
  requestId: string;
  profileName: string;
  hostNames: string[];
  status: CodexOperationModalStatus;
  logs: Array<{ level: "info" | "warn" | "error"; message: string }>;
  tasks: TaskRun[];
  message?: string;
  error?: string;
};

const CODEX_MODEL_OPTIONS = ["gpt-5.5", "gpt-5.4", "gpt-5.4-mini", "gpt-5.3-codex", "gpt-5.2", "gpt-5-codex"];
const REASONING_EFFORT_OPTIONS = ["low", "medium", "high", "xhigh"];
const DEFAULT_PROFILE_MODEL = "gpt-5-codex";
const DEFAULT_PROFILE_PROVIDER = "openai";
const DEFAULT_PROFILE_BASE_URL = "https://api.openai.com/v1";
const DEFAULT_PROFILE_API_KEY_ENV_VAR = "OPENAI_API_KEY";
const APP_UPDATE_DAILY_CHECK_HOUR = 4;
const RESOURCE_MONITOR_SAMPLE_TIMEOUT_MS = 30_000;
const viteEnv = (import.meta as ImportMeta & { env?: Record<string, string | boolean | undefined> }).env;
const PREVIEW_SIDEBAR_UPDATE_BUTTON =
  viteEnv?.DEV === true && viteEnv?.VITE_CODEXHUB_PREVIEW_UPDATE_BUTTON === "1";
const appLogoUrl = new URL("../src-tauri/icons/128x128.png", import.meta.url).href;
const PlatformAppearanceContext = createContext<RuntimePlatform>("windows");

const platformEmojiIcons = {
  dashboard: "🏠",
  hosts: "🖥️",
  profiles: "🧾",
  skills: "🧩",
  monitor: "📊",
  tasks: "✅",
  settings: "⚙️",
  close: "✕",
  delete: "🗑️",
  download: "⬇️",
  install: "📥",
  key: "🔑",
  language: "🌐",
  network: "🧭",
  preview: "👁️",
  scan: "🔎",
  terminal: "⌘",
  update: "🔄",
  warning: "⚠️"
} satisfies Record<PlatformIconId, string>;

export const uiCopy = {
  en: {
    navItems: [
      { id: "dashboard", label: "Home" },
      { id: "hosts", label: "Hosts" },
      { id: "monitor", label: "Monitor" },
      { id: "profiles", label: "Profiles" },
      { id: "skills", label: "Skills" },
      { id: "tasks", label: "Tasks" },
      { id: "settings", label: "Settings" }
    ] satisfies Array<{ id: SectionId; label: string }>,
    sections: {
      dashboard: {
        title: "Home",
        eyebrow: "Home",
        body: "Mock SSH inventory, profile status, and recent operations for the first CodexHub desktop shell."
      },
      hosts: {
        title: "Hosts",
        eyebrow: "Server inventory",
        body: "Add CodexHub-managed SSH config blocks without disturbing user-owned SSH settings."
      },
      profiles: {
        title: "Profiles",
        eyebrow: "Codex configuration",
        body: "Draft managed profile presets for remote ~/.codex/config.toml files."
      },
      skills: {
        title: "Skills",
        eyebrow: "Skill packs",
        body: "Review skill bundles that will sync to remote ~/.codex/skills/ directories."
      },
      monitor: {
        title: "Monitor",
        eyebrow: "Host resources",
        body: "Sample CPU, memory, and GPU status across remembered hosts without installing a remote agent."
      },
      tasks: {
        title: "Tasks",
        eyebrow: "Task runs",
        body: "Track mock backend commands, generated logs, and pending host operations."
      },
      settings: {
        title: "Settings",
        eyebrow: "Preferences",
        body: "Adjust the shell theme, inspect local SSH key status, and copy public keys."
      }
    } satisfies Record<SectionId, { title: string; eyebrow: string; body: string }>,
    common: {
      addServer: "Add Server",
      backendMode: "Backend mode",
      mockMode: "Mock mode — no desktop/SSH writes",
      backendUnavailableTitle: "Desktop backend unavailable",
      backendUnavailableBody: "CodexHub could not load authoritative desktop data. Start the Tauri desktop app or explicitly use the Web/Mock command.",
      host: "Host",
      justNow: "just now",
      primaryNavigation: "Primary navigation",
      required: "required",
      notRequired: "not required",
      loading: "loading",
      ready: "ready",
      unassigned: "Unassigned",
      locale: "en-US",
      keyValueSeparator: ": "
    },
    feedback: {
      retry: "Retry",
      details: "Details",
      viewTask: "View task",
      dismiss: "Dismiss",
      notifications: "Notifications",
      genericNotice: "Operation completed.",
      genericError: "Operation failed. Review the task details or logs for diagnostics.",
      migrationRequired: "Local data must be migrated before this operation can continue.",
      storageCorrupt: "Local data requires recovery before this operation can continue.",
      partialFailure: "The operation completed partially. Review the task details.",
      errorTitles: {
        "backend-unavailable": "Desktop backend unavailable",
        "invalid-arguments": "Check the requested values",
        "storage-unavailable": "Persistent storage unavailable",
        "storage-corrupt": "Local data needs recovery",
        "migration-required": "Local data migration required",
        "operation-failed": "Operation failed",
        "partial-failure": "Operation completed partially",
        unexpected: "Unexpected error"
      }
    },
    windowControls: {
      close: "Close window",
      maximize: "Maximize window",
      minimize: "Minimize window"
    },
    notices: {
      default: "Local SSH key and config management is ready in the desktop backend.",
      addHost: "Fill in the SSH config form. CodexHub will create or update one managed Host block with a backup first.",
      newApiConfig: "Fill in the profile form to create a reusable API configuration.",
      publicKeyCopied: "Public key copied to clipboard.",
      copyFailed: "Could not copy automatically. Select the public key text and copy it manually.",
      addHostBeforeProfile: "Add a host before applying a profile.",
      mockHostRemoved: (name: string) => `${name} was removed from the mock inventory only.`
    },
    dashboard: {
      summaryLabel: "Home summary",
      online: "Online",
      applied: "Applied",
      enabled: "Enabled",
      success: "Success",
      serverMatrix: "Host Matrix",
      system: "System",
      noHosts: "No hosts yet",
      noHostsBody: "Add the first SSH target to populate the host matrix.",
      noSkillPacks: "No skill packs"
    },
    monitor: {
      refreshNow: "Refresh now",
      refreshing: "Refreshing",
      autoRefresh: "Auto refresh",
      refreshEvery: "Refresh every",
      seconds: "seconds",
      lastUpdated: "Last updated",
      never: "Never",
      hostStatus: "Host resource status",
      noHosts: "No hosts to monitor",
      noHostsBody: "Add or import SSH hosts before opening live resource monitoring.",
      noData: "No sample yet.",
      cpu: "CPU",
      memory: "Memory",
      gpu: "GPU",
      load: "Load",
      cores: "Cores",
      available: "Available",
      latency: "Latency",
      sampledAt: "Sampled",
      status: "Status",
      gpuTool: "GPU tool",
      vram: "VRAM",
      temp: "Temp",
      power: "Power",
      driver: "Driver",
      utilization: "Util",
      gpuProcesses: "GPU processes",
      noGpuProcesses: "No GPU processes",
      processCount: "Processes",
      processCountShort: (count: number) => `${count} proc`,
      processMemory: "Process memory",
      runtime: "Runtime",
      usage: "Usage",
      pid: "PID",
      command: "Command",
      noGpu: "No GPU data",
      detectedOnly: "Detected only",
      statusOnline: "Online",
      statusPartial: "Partial",
      statusFailed: "Failed",
      statusNoSample: "No sample",
      dragHandle: "Drag to reorder host",
      refreshed: (count: number) => `Resource monitor refreshed ${count} hosts.`,
      refreshFailed: "Resource monitor refresh failed."
    },
    setupGuide: {
      title: "Setup Guide",
      preferencesTitle: "Preferences",
      preferencesBody: "Choose the theme, platform style, and UI font before entering CodexHub.",
      languageTitle: "Choose Language",
      languageBody: "Step 1: Please choose your preferred language.",
      languageEnglish: "English",
      languageChinese: "Simplified Chinese",
      next: "Next",
      bodyWithHosts: (count: number) => `${count} local SSH Host entr${count === 1 ? "y" : "ies"} detected. Importing refreshes CodexHub only.`,
      bodyEmpty: "No usable local SSH config was detected. You can add hosts manually in CodexHub.",
      ed25519KeyTitle: "Ed25519 identity key",
      ed25519Ready: "Private key ready",
      ed25519MissingBody: "CodexHub will automatically create an Ed25519 key for SSH connections, even if you skip this setup step.",
      ed25519ReadyBody: "CodexHub will use this existing private key for SSH connections.",
      detectedPath: (path: string) => `Detected from ${path}`,
      importLocalConfig: "Import local config",
      skip: "Skip",
      close: "Close guide",
      detecting: "Detecting local config...",
      importing: "Importing...",
      source: "Source",
      moreHosts: (count: number) => `+${count} more`,
      imported: (count: number) => `${count} local SSH Host entr${count === 1 ? "y" : "ies"} imported into CodexHub.`
    },
    emptyLists: {
      hosts: "Nothing here yet...",
      profiles: "Nothing here yet...",
      profileHosts: "Nothing here yet...",
      skills: "Nothing here yet...",
      skillHosts: "Nothing here yet...",
      tasks: "Nothing here yet..."
    },
    hosts: {
      sshManager: "SSH config manager",
      addServerTitle: "Add server",
      addCodexHubHost: "Add SSH Host",
      writesTo: "Writes to",
      userOwnedPreserved: "User-owned blocks are preserved.",
      localPathsLoaded: "local paths loaded",
      webPreview: "web preview",
      formIntro: "Enter the remote password once. CodexHub logs in, installs your local public key, sets permissions, then tests ssh HostAlias. Passwords are never stored.",
      writing: "Connecting...",
      closeWhileConnectingTitle: "Hide connection progress?",
      closeWhileConnectingBody: "The SSH operation will continue in the background. You can review its durable task record later.",
      closeWhileConnectingConfirm: "Hide",
      keyCreated: "Created the local .ssh/id_ed25519 key.",
      keyMissing: "No local id_ed25519 key was detected. Create one before connecting.",
      connectingProgress: "Connecting. Waiting for all four steps to finish...",
      connectionFailed: "Connection failed",
      connectionSuccess: "Connection succeeded",
      generateKey: "Create",
      identityDetected: "id_ed25519 detected",
      identityMissing: "id_ed25519 not detected",
      progressTitle: "Connection progress",
      progressSubtitle: "Live bootstrap log",
      failureDetails: "Failure details",
      noFailureDetails: "No detailed log was returned.",
      waitingToStart: "Waiting to start",
      bootstrapSteps: {
        password_login: "1. Log in with password",
        install_public_key: "2. Install local public key",
        set_permissions: "3. Set remote permissions",
        verify_alias_login: "4. Test SSH Host alias"
      },
      savedHost: (alias: string) => `Saved Host ${alias}.`,
      editingHost: (alias: string) => `Editing managed Host ${alias}. Submit with the same alias to update it in place.`,
      deleteConfirm: (alias: string) => `Delete Host ${alias} from SSH config?`,
      deletedHost: (alias: string) => `Deleted Host ${alias}.`,
      hostAlias: "Host Alias",
      hostName: "Host IP",
      port: "Port",
      user: "User",
      identityFile: "IdentityFile",
      bootstrapPassword: "One-time password",
      bootstrapPasswordHelp: "Optional. Used once to log in, append your public key to ~/.ssh/authorized_keys, then test key login.",
      showPassword: "Show one-time password",
      hidePassword: "Hide one-time password",
      cancel: "Cancel",
      saving: "Saving...",
      writeSshConfig: "Connect",
      reset: "Reset",
      codexhubManaged: "CodexHub",
      sshHostBlocks: "SSH Host blocks",
      repeatedSaves: "Repeated saves update the same alias instead of appending duplicates.",
      newHost: "New Host",
      noManagedHosts: "No SSH hosts",
      noManagedHostsBody: "Click Add Server in the Hosts header to connect and create the first SSH config entry.",
      detectedSshHosts: "Host list",
      detectedSshHostsBody: "CodexHub lists local SSH config HostAlias entries for unified management.",
      detect: "Detect",
      refreshDetected: "Test all",
      detectLocalConfig: "Detect local config",
      detectedLocalHosts: (count: number) => `${count} local SSH Host entr${count === 1 ? "y" : "ies"} detected.`,
      testingAll: "Testing all...",
      testedAll: "All hosts tested.",
      testedAllResult: (success: number, total: number) => `Tested ${success}/${total} host(s) successfully.`,
      testedAllFailed: (success: number, total: number) => `Host testing completed with failures: ${success}/${total} succeeded.`,
      updateOutdatedCodex: "Update outdated",
      updatingOutdatedCodex: "Updating...",
      updatedOutdatedCodex: (success: number, total: number) => `Updated ${success}/${total} outdated host(s).`,
      noOutdatedCodex: "No outdated Codex hosts.",
      localSource: "Local",
      source: "Source",
      bootstrapping: "Bootstrapping...",
      testing: "Testing...",
      checkVersion: "Check Version",
      checkingVersion: "Checking...",
      installCodex: "Install",
      installingCodex: "Installing...",
      updateCodex: "Update",
      updatingCodex: "Updating...",
      uninstallCodex: "Uninstall",
      uninstallingCodex: "Uninstalling...",
      uninstallCodexConfirmTitle: (alias: string) => `Uninstall Codex from ${alias}`,
      uninstallCodexConfirmBody:
        "This will permanently delete the remote Codex installation, ~/.codex, CodexHub-managed env/API key files, and related Codex config/cache directories. No backup will be created.",
      confirmUninstallCodex: "Uninstall",
      details: "Host details",
      detailsTitle: (alias: string) => `Host · ${alias}`,
      detailsBody: "Connection status and remote Codex readiness from the latest test.",
      sshStatus: "SSH status",
      arch: "Arch",
      shell: "Shell",
      codexInstalled: "Codex installed",
      codexVersion: "Codex version",
      latestCodexVersion: "Latest version",
      latestCodexUnknown: "Not fetched",
      configExists: "API config",
      skillsCount: "Skills count",
      yes: "Yes",
      no: "No",
      unknown: "Unknown",
      mockInventory: "Mock inventory",
      mockInventoryBody: "Inventory rows combine discovered SSH config hosts, CodexHub-managed hosts, and mock rows in web mode.",
      alias: "Alias",
      name: "Name",
      endpoint: "Endpoint",
      status: "Status",
      profile: "Profile",
      skills: "Skills",
      lastSeen: "Last seen",
      actions: "Actions",
      edit: "Edit",
      delete: "Delete",
      test: "Test",
      testSsh: "Test SSH",
      os: "OS",
      codex: "Codex",
      codexPathMissing: "Installed, PATH missing",
      apiEnvMissing: "API env missing",
      apiEnvReady: "API env ready",
      apiEnvUnknown: "API env unknown",
      apiEnvStatusTitle: (envVar: string) => `Remote ${envVar} status`,
      latency: "Test latency"
    },
    profiles: {
      library: "Local config",
      editor: "Editor",
      edit: "Edit",
      preview: "Preview",
      applyConfig: "Apply configuration",
      newProfile: "New",
      newApiConfig: "New API config",
      duplicate: "Duplicate",
      delete: "Delete",
      import: "Import",
      actions: "Actions",
      applyColumn: "Apply",
      selectHosts: "Select hosts",
      selectAll: "Select all",
      apiConfig: "API config",
      noApiConfig: "No config",
      unknownApiConfig: "Unknown config",
      detectCcSwitch: "Detect cc-switch",
      importDetected: "Import detected",
      save: "Save",
      saving: "Saving...",
      create: "Create",
      name: "Name",
      namePlaceholder: "Custom config name",
      description: "Description",
      model: "Model",
      modelPlaceholder: DEFAULT_PROFILE_MODEL,
      provider: "Provider",
      providerPlaceholder: DEFAULT_PROFILE_PROVIDER,
      baseUrl: "Base URL",
      baseUrlPlaceholder: DEFAULT_PROFILE_BASE_URL,
      apiKeyEnvVar: "Env key",
      apiKey: "API key",
      apiKeyPlaceholder: "Paste a new key to store",
      apiKeyStoredPlaceholder: "Saved API key",
      apiKeyLoading: "Loading key...",
      apiKeyMissing: "No stored key was found.",
      showApiKey: "Show API key",
      hideApiKey: "Hide API key",
      credentialStored: "Credential stored",
      thirdPartyImport: "Third-party import",
      localStorageLabel: "Local storage",
      modelReasoningEffort: "Model reasoning",
      planModeReasoningEffort: "Plan reasoning",
      fastMode: "Fast mode",
      serviceTier: "Service tier",
      advanced: "Advanced",
      extraToml: "Extra TOML",
      source: "Source",
      selectedHosts: "Selected hosts",
      targetFiles: "Target files",
      targetHost: "Host",
      targetPath: "Path",
      renderedToml: "Config TOML",
      perHostStatus: "Per-host status",
      previewApply: "Preview",
      applySelected: "Apply selected",
      applyOne: "Apply",
      batchApply: "Batch apply",
      backupExpected: "backup",
      noChangeExpected: "no change",
      noPreview: "Select hosts and preview before applying.",
      noProfiles: "No profiles yet.",
      noProfilesBody: "Create or import a profile to render remote Codex TOML.",
      noHostTargets: "No hosts available.",
      applySuccess: (name: string, count: number) => `${name} applied to ${count} host${count === 1 ? "" : "s"}.`,
      applyOperationTitle: "Apply API config",
      applyOperationStarted: (name: string, count: number) => `Applying ${name} to ${count} selected host${count === 1 ? "" : "s"}.`,
      applyOperationWaiting: "Running remote config scripts...",
      applyOperationSuccess: (name: string, count: number) => `${name} finished on ${count} host${count === 1 ? "" : "s"}.`,
      applyOperationFailed: "One or more hosts failed to apply this API config.",
      alreadyApplied: "Already applied",
      importReady: (count: number) => `${count} profiles imported.`,
      deleteConfirm: (name: string) => `Delete profile ${name}?`,
      ccSwitchFound: (count: number) => `${count} cc-switch profiles detected.`,
      ccSwitchNone: "No cc-switch profiles detected.",
      ccSwitchChecking: "Checking cc-switch...",
      codexPrep: "Remote Codex",
      noHosts: "Add a host before checking remote Codex.",
      notChecked: "Not checked",
      notInstalled: "Not installed",
      operationFailed: "Operation failed",
      hosts: "Hosts",
      applyOnline: "Apply to online hosts"
    },
    codexOperation: {
      title: "Codex maintenance",
      running: "Running",
      success: "Completed",
      failed: "Failed",
      progress: "Progress",
      summary: "Summary",
      latestLog: "Latest log",
      started: "Started remote maintenance.",
      waiting: "Waiting for remote output...",
      installHint: "Install can try official download, mirror fallback, local upload, then version check.",
      updateHint: "Update can try official download, mirror fallback, local upload, then version check.",
      uninstallHint: "Uninstall permanently deletes remote Codex files, config, and CodexHub-managed env/API key files.",
      noLogs: "No task log returned yet.",
      hide: "Hide",
      close: "Close",
      viewTasks: "View Tasks"
    },
    skills: {
      library: "Local skill library",
      installedLibrary: "Installed skill library",
      detect: "Detect",
      detecting: "Detecting...",
      detected: "Detection complete",
      refresh: "Refresh",
      refreshing: "Refreshing...",
      refreshed: "Refreshed from cache",
      importDirectory: "Import",
      download: "Download",
      downloadTitle: "Download skill",
      downloadBody: "Enter a GitHub repository URL.",
      downloadAction: "Download",
      downloaded: "Download complete",
      githubUrl: "GitHub URL:",
      noSkills: "No local skills",
      noSkillsBody: "Use Detect, Import, or Download to populate the local skill library.",
      skill: "Skill",
      source: "Source",
      addedAt: "Added",
      applications: "Applied",
      actions: "Actions",
      unapplied: "Not applied",
      localMachine: "Local",
      sourceLocal: "Local",
      sourceGithub: "GitHub",
      preview: "Preview",
      edit: "Edit",
      save: "Save",
      install: "Install",
      installSuccess: "Installed successfully!",
      installPartialFailure: "Install incomplete.",
      uninstall: "Uninstall",
      uninstallSuccess: "Uninstalled successfully!",
      uninstallPartialFailure: "Uninstall incomplete.",
      delete: "Delete",
      firstScanTitle: "First skill scan",
      firstScanBody: "CodexHub will scan the local Codex skills folder and every configured host. This can take a while.",
      firstScanAction: "Scan local and hosts",
      localOnlyDetect: "Detect local skills",
      selectTargets: "Select targets",
      selectAll: "Select all",
      noTargets: "No available targets",
      installHint: "Installs to the default Codex skills root.",
      uninstallHint: "Only targets where this skill is already installed can be selected.",
      deleteTitle: "Delete skill",
      deleteBody: "Remove this skill from the library. You can uninstall it from all known targets first, or only remove the library entry.",
      uninstallAndDelete: "Uninstall and delete",
      directDelete: "Delete only",
      downloadInstalledTitle: "Download installed skill",
      downloadInstalledBody: (name: string) => `Download ${name} into the local skill library.`,
      downloadInstalledAction: "Download to library",
      downloadInstalledStarted: (name: string) => `Downloading ${name} into the local skill library.`,
      downloadUnavailableLocalExists: "Already in local library",
      uninstallInstalledTitle: "Uninstall installed skill",
      uninstallInstalledBody: (name: string, target: string) =>
        `Uninstall ${name} from ${target}. This permanently deletes the skill directory and cannot be undone.`,
      uninstallInstalledAction: "Uninstall from this target",
      uninstallInstalledStarted: (name: string, target: string) => `Uninstalling ${name} from ${target}.`,
      installedPreviewTarget: "Target",
      installedPreviewStatus: "Status",
      installedPreviewLocalLibrary: "Local library",
      installedPreviewNotDownloaded: "Not downloaded",
      operationWaiting: "Running skill operation scripts...",
      operationDone: "Operation finished.",
      target: "Target",
      path: "Path",
      details: "Details",
      hostIp: "Host IP",
      installedSkills: "Installed skills",
      noInstalledSkills: "No skills",
      status: "Status",
      available: "Available",
      installedTarget: "Installed",
      unavailableTarget: "Unavailable",
      aboutFallback: "No description available.",
      imported: (count: number) => `Imported ${count} skill(s).`,
      installed: (count: number) => `Installed on ${count} target(s).`,
      uninstalled: (count: number) => `Uninstalled from ${count} target(s).`
    },
    tasks: {
      runs: "Runs",
      taskHistory: "Task history",
      body: "Mock TaskRun rows demonstrate the local task model and future worker queue.",
      action: "Action",
      host: "Host",
      status: "Status",
      started: "Started",
      summary: "Summary",
      details: "Details",
      logs: "Logs",
      clearHistory: "Clear all",
      clearHistoryTitle: "Clear task history?",
      clearHistoryBody: "All completed task records and logs will be exported as a JSON archive and moved to the system recycle bin. Running and queued tasks will remain.",
      clearHistoryMockBody: "All completed Mock task records will be removed from memory. Running and queued tasks will remain.",
      clearHistoryConfirm: "Move to recycle bin",
      clearingHistory: "Clearing...",
      historyCleared: (count: number) => `${count} completed task record${count === 1 ? "" : "s"} moved to the system recycle bin.`,
      mockHistoryCleared: (count: number) => `${count} completed Mock task record${count === 1 ? "" : "s"} cleared from memory.`,
      loadMore: "Load older tasks",
      loadingMore: "Loading...",
      taskLog: "TaskLog",
      noTask: "No task",
      noLogs: "No logs yet.",
      command: "Command",
      stdout: "stdout",
      stderr: "stderr",
      exitCode: "Exit",
      duration: "Duration",
      timedOut: "Timed out",
      noOutput: "(no output)",
      minutesAgo: (minutes: number) => `${minutes}m ago`,
      actionLabels: {
        "Generate Ed25519 key": "Generate Ed25519 key",
        "Save SSH Host": "Save SSH Host",
        "Apply profile": "Apply profile",
        "Test SSH connection": "Test SSH connection",
        "Bootstrap SSH key": "Bootstrap SSH key",
        "Sync skill pack": "Sync skill pack",
        "Preview profile": "Preview profile",
        "Probe remote system": "Test remote system",
        "Check Codex version": "Check Codex version",
        "Install Codex": "Install Codex",
        "Update Codex": "Update Codex",
        "Uninstall Codex": "Uninstall Codex",
        "List remote skills": "List remote skills",
        "Preview skill install": "Preview skill install",
        "Install skill": "Install skill",
        "Delete SSH Host": "Delete SSH Host",
        "Delete profile": "Delete profile",
        "Delete skill": "Delete skill",
        "Check app update": "Check app update",
        "Install app update": "Install app update",
        "Refresh latest Codex version": "Refresh latest Codex version",
        "Import cc-switch profiles": "Import cc-switch profiles",
        "Refresh discovered hosts": "Refresh discovered hosts",
        "Add host": "Add host",
        "Update host": "Update host",
        "Delete host": "Delete host",
        "Create profile": "Create profile",
        "Update profile": "Update profile",
        "Duplicate profile": "Duplicate profile",
        "Import profiles": "Import profiles",
        "Store profile credential": "Store profile credential",
        "Migrate profile credential": "Migrate profile credential",
        "Delete profile credential": "Delete profile credential",
        "Import local skill": "Import local skill",
        "Update skill details": "Update skill details",
        "Save settings": "Save settings",
        "Choose close button behavior": "Choose close button behavior",
        "Migrate local storage": "Migrate local storage",
        "Restore local storage": "Restore local storage",
        "Sample host resources": "Sample host resources",
        "Frontend error": "Frontend error"
      },
      unknownAction: "Background task",
      summaryByStatus: {
        queued: (action: string) => `${action} queued.`,
        running: (action: string) => `${action} is running.`,
        success: (action: string) => `${action} completed.`,
        failed: (action: string) => `${action} failed. Review the logs for details.`,
        interrupted: (action: string) => `${action} was interrupted.`
      }
    },
    storage: {
      title: "Local data needs attention",
      attentionSummary: (count: number) => `${count} local data store${count === 1 ? "" : "s"} require attention before related writes can continue.`,
      migrationRequired: "This store uses a readable legacy schema. Writes stay locked until you preview and confirm migration.",
      corrupt: "This store is damaged. CodexHub will not silently use a backup; preview a validated recovery first.",
      recoveryRequired: "A previous storage operation was interrupted. Review the durable task record before retrying or recovering.",
      previewMigration: "Preview migration",
      previewMigrations: "Review migrations",
      previewRestore: "Preview recovery",
      planTitle: "Confirm local data operation",
      storeLabels: {
        settings: "App settings",
        hosts: "Hosts",
        profiles: "Profiles",
        skills: "Skills"
      },
      path: "Data path",
      fingerprint: "Source fingerprint",
      backupLocation: "Backup location",
      applyMigration: "Back up and migrate",
      applyMigrations: "Back up and migrate all",
      restoreBackup: "Back up current data and restore",
      cancel: "Cancel",
      working: "Working...",
      migrationDone: "Local data migration completed.",
      migrationsDone: (count: number) => `${count} local data store${count === 1 ? "" : "s"} migrated successfully.`,
      restoreDone: "Local data recovery completed."
    },
    settings: {
      appearance: "Appearance",
      theme: "Theme",
      platformAppearance: "Platform",
      font: "Font",
      sidebarCompletionIndicators: "Sidebar visual hints",
      runtime: "Runtime",
      backend: "Backend",
      app: "App",
      remoteWrapper: "Remote wrapper",
      sshConfig: "SSH config",
      desktopBackendRequired: "desktop backend required",
      localSsh: "Local keys",
      sshKeyStatus: "SSH key status",
      sshKeyBody: "Private key files are checked by existence only. CodexHub never reads or displays private key content.",
      appUpdates: "Version info",
      dailyUpdateCheck: "Daily check: 04:00",
      closeButton: "Other",
      closeButtonBehavior: "Program close button behavior",
      networkProxy: "Network proxy",
      networkProxyOptions: {
        auto: "Auto",
        direct: "Direct",
        manual: "Manual"
      },
      networkProxyUrl: "Proxy URL",
      networkProxyManualTitle: "Manual proxy",
      networkProxyPort: "Proxy port",
      networkProxyPortPlaceholder: "7890",
      networkProxySave: "Save proxy",
      softwareName: "Software",
      currentVersion: "Current version",
      installedAt: "Installed at",
      latestVersion: "Latest version",
      updatedAt: "Updated at",
      notChecked: "Not checked",
      checkFailed: "Check failed",
      updateCheckFailureHint: "You can review this run later from the Tasks details page.",
      pendingConfiguration: "Pending setup",
      checkStableUpdate: "Check",
      updateChecking: "Checking...",
      installStableUpdate: "Update",
      sidebarInstallStableUpdate: "Update",
      previewUpdateButtonNotice: "Dev preview: this button is shown for visual inspection only.",
      settingsSaved: "Settings saved.",
      settingsSaveFailed: "Settings could not be saved. The last confirmed values are still active.",
      settingsRetry: "Retry save",
      settingsSaving: "Saving settings...",
      updateInstalling: "Installing...",
      refresh: "Refresh",
      generating: "Generating...",
      generateEd25519: "Generate Ed25519",
      publicKey: "Public key",
      readyToCopy: "Ready to copy",
      noPublicKey: "No public key detected",
      copyPublicKey: "Copy Public Key",
      copyPublicKeySuccess: "Copied!",
      publicKeyEmpty: "Generate or add an SSH public key to show it here.",
      commandReservations: "Command reservations",
      commandSurface: "Command surface",
      privateFound: "private found",
      missing: "missing",
      privatePath: "Private path",
      publicPath: "Public path",
      available: "available",
      unknown: "unknown",
      themeOptions: {
        system: "System",
        light: "Light",
        dark: "Dark"
      },
      platformOptions: {
        auto: "Auto",
        windows: "Windows",
        macos: "macOS"
      },
      closeButtonOptions: {
        ask: "Ask next time",
        exit: "Exit app",
        "minimize-to-tray": "Minimize to tray"
      }
    },
    closeButtonPrompt: {
      title: "What should the close button do?",
      body: "Choose once and CodexHub will remember it. You can change this later at the bottom of Settings.",
      exitTitle: "Exit app",
      exitBody: "Close CodexHub completely.",
      minimizeTitle: "Minimize to tray",
      minimizeBody: "Keep CodexHub running in the background and restore it from the tray.",
      cancel: "Keep open"
    },
    status: {
      host: {
        online: "online",
        offline: "offline",
        unknown: "unknown",
        testing: "testing"
      },
      task: {
        queued: "queued",
        running: "running",
        success: "success",
        failed: "failed",
        interrupted: "interrupted"
      },
      log: {
        info: "info",
        warn: "warn",
        error: "error"
      }
    }
  },
  zh: {
    navItems: [
      { id: "dashboard", label: "主页" },
      { id: "hosts", label: "主机" },
      { id: "monitor", label: "监控" },
      { id: "profiles", label: "配置" },
      { id: "skills", label: "技能" },
      { id: "tasks", label: "任务" },
      { id: "settings", label: "设置" }
    ] satisfies Array<{ id: SectionId; label: string }>,
    sections: {
      dashboard: {
        title: "主页",
        eyebrow: "主页",
        body: "用于 CodexHub 桌面壳的 SSH 清单、配置状态和最近操作。"
      },
      hosts: {
        title: "主机",
        eyebrow: "服务器清单",
        body: "添加 CodexHub 管理的 SSH config 块，不影响用户已有 SSH 设置。"
      },
      profiles: {
        title: "配置",
        eyebrow: "Codex 配置",
        body: "为远端 ~/.codex/config.toml 草拟受管理的配置预设。"
      },
      skills: {
        title: "技能",
        eyebrow: "技能包",
        body: "查看未来会同步到远程 ~/.codex/skills/ 目录的技能包。"
      },
      monitor: {
        title: "监控",
        eyebrow: "主机资源",
        body: "通过只读 SSH 同时采样所有主机 CPU、内存和 GPU 状态，无需安装远程 agent。"
      },
      tasks: {
        title: "任务",
        eyebrow: "任务运行",
        body: "跟踪后端命令、生成日志和待处理主机操作。"
      },
      settings: {
        title: "设置",
        eyebrow: "偏好设置",
        body: "调整界面主题，查看本地 SSH 密钥状态，并复制公钥。"
      }
    } satisfies Record<SectionId, { title: string; eyebrow: string; body: string }>,
    common: {
      addServer: "添加服务器",
      backendMode: "后端模式",
      mockMode: "模拟模式——不会执行桌面或 SSH 写入",
      backendUnavailableTitle: "桌面后端不可用",
      backendUnavailableBody: "CodexHub 无法加载权威桌面数据。请启动 Tauri 桌面应用，或明确使用 Web/Mock 命令。",
      host: "主机",
      justNow: "刚刚",
      primaryNavigation: "主导航",
      required: "需要",
      notRequired: "不需要",
      loading: "加载中",
      ready: "就绪",
      unassigned: "未分配",
      locale: "zh-CN",
      keyValueSeparator: "："
    },
    feedback: {
      retry: "重试",
      details: "详情",
      viewTask: "查看任务",
      dismiss: "关闭",
      notifications: "通知",
      genericNotice: "操作已完成。",
      genericError: "操作失败，请在任务详情或日志中查看诊断信息。",
      migrationRequired: "本地数据需要先完成迁移，随后才能继续该操作。",
      storageCorrupt: "本地数据需要先完成恢复，随后才能继续该操作。",
      partialFailure: "操作部分完成，请查看任务详情。",
      errorTitles: {
        "backend-unavailable": "桌面后端不可用",
        "invalid-arguments": "请检查操作参数",
        "storage-unavailable": "持久存储不可用",
        "storage-corrupt": "本地数据需要恢复",
        "migration-required": "本地数据需要迁移",
        "operation-failed": "操作失败",
        "partial-failure": "操作部分完成",
        unexpected: "发生意外错误"
      }
    },
    windowControls: {
      close: "关闭窗口",
      maximize: "最大化窗口",
      minimize: "最小化窗口"
    },
    notices: {
      default: "本地 SSH 密钥和配置管理已在桌面后端就绪。",
      addHost: "请填写 SSH config 表单。CodexHub 会先备份，再创建或更新一个受管理的 Host 块。",
      newApiConfig: "请填写配置表单，以创建可复用的 API 配置。",
      publicKeyCopied: "公钥已复制到剪贴板。",
      copyFailed: "无法自动复制。请选中公钥文本后手动复制。",
      addHostBeforeProfile: "请先添加主机，再应用配置。",
      mockHostRemoved: (name: string) => `${name} 已从 mock 清单中移除。`
    },
    dashboard: {
      summaryLabel: "主页概览",
      online: "在线",
      applied: "应用",
      enabled: "启用",
      success: "成功",
      serverMatrix: "主机矩阵",
      system: "系统",
      noHosts: "还没有主机",
      noHostsBody: "添加第一个 SSH 目标后会填充主机矩阵。",
      noSkillPacks: "无技能包"
    },
    monitor: {
      refreshNow: "立即刷新",
      refreshing: "刷新中",
      autoRefresh: "自动刷新",
      refreshEvery: "刷新间隔",
      seconds: "秒",
      lastUpdated: "上次更新",
      never: "未刷新",
      hostStatus: "主机监控状态",
      noHosts: "暂无可监控主机",
      noHostsBody: "请先添加或导入 SSH 主机，再查看实时资源状态。",
      noData: "尚无采样。",
      cpu: "CPU",
      memory: "内存",
      gpu: "GPU",
      load: "负载",
      cores: "核心",
      available: "可用",
      latency: "延迟",
      sampledAt: "采样时间",
      status: "状态",
      gpuTool: "GPU 工具",
      vram: "显存",
      temp: "温度",
      power: "功耗",
      driver: "驱动",
      utilization: "占用率",
      gpuProcesses: "GPU 进程",
      noGpuProcesses: "无 GPU 进程",
      processCount: "进程数",
      processCountShort: (count: number) => `${count} 进程`,
      processMemory: "进程显存",
      runtime: "运行时",
      usage: "占用量",
      pid: "PID",
      command: "命令",
      noGpu: "无 GPU 数据",
      detectedOnly: "统一内存",
      statusOnline: "在线",
      statusPartial: "部分",
      statusFailed: "失败",
      statusNoSample: "未采样",
      dragHandle: "拖拽调整主机位置",
      refreshed: (count: number) => `已刷新 ${count} 台主机监控状态。`,
      refreshFailed: "监控刷新失败。"
    },
    setupGuide: {
      title: "配置向导",
      preferencesTitle: "偏好设置",
      preferencesBody: "进入 CodexHub 前，先选择主题、平台风格和界面字体。",
      languageTitle: "选择语言",
      languageBody: "第1步：请选择偏好语言",
      languageEnglish: "英文",
      languageChinese: "简体中文",
      next: "下一步",
      bodyWithHosts: (count: number) => `检测到 ${count} 个本地 SSH Host。导入只刷新 CodexHub，不改写 SSH config。`,
      bodyEmpty: "未检测到本地存在可用的SSH配置，可使用CodexHub手动添加",
      ed25519KeyTitle: "Ed25519 身份密钥",
      ed25519Ready: "已有私钥",
      ed25519MissingBody: "CodexHub 会自动创建用于 SSH 连接的 Ed25519 密钥，即使你选择跳过此步骤也会创建。",
      ed25519ReadyBody: "CodexHub 会使用这个已有私钥进行 SSH 连接。",
      detectedPath: (path: string) => `检测位置：${path}`,
      importLocalConfig: "导入本地配置",
      skip: "跳过",
      close: "关闭向导",
      detecting: "正在检测本地配置...",
      importing: "导入中...",
      source: "来源",
      moreHosts: (count: number) => `还有 ${count} 个`,
      imported: (count: number) => `已导入 ${count} 个本地 SSH Host 到 CodexHub。`
    },
    emptyLists: {
      hosts: "这里空空如也...",
      profiles: "这里空空如也...",
      profileHosts: "这里空空如也...",
      skills: "这里空空如也...",
      skillHosts: "这里空空如也...",
      tasks: "这里空空如也..."
    },
    hosts: {
      sshManager: "SSH 连接管理",
      addServerTitle: "添加服务器",
      addCodexHubHost: "新增 SSH Host",
      writesTo: "写入",
      userOwnedPreserved: "用户已有配置块会被保留。",
      localPathsLoaded: "本地路径已加载",
      webPreview: "网页预览",
      formIntro: "输入一次远端密码。CodexHub 会登录远端、安装本地公钥、设置权限，并用 ssh Host 别名测试；密码不会保存。",
      writing: "正在连接...",
      closeWhileConnectingTitle: "隐藏连接进度？",
      closeWhileConnectingBody: "SSH 操作会继续在后台运行，稍后可从持久任务记录查看结果。",
      closeWhileConnectingConfirm: "隐藏",
      keyCreated: "已新建本地 .ssh/id_ed25519。",
      keyMissing: "未检测到本地 id_ed25519，请先新建密钥。",
      connectingProgress: "正在连接，请等待四个步骤完成...",
      connectionFailed: "连接失败",
      connectionSuccess: "成功连接",
      generateKey: "新建",
      identityDetected: "已检测到 id_ed25519",
      identityMissing: "未检测到 id_ed25519",
      progressTitle: "连接进程",
      progressSubtitle: "实时引导日志",
      failureDetails: "详细失败日志",
      noFailureDetails: "未返回详细日志",
      waitingToStart: "等待开始",
      bootstrapSteps: {
        password_login: "1. 密码登录远端",
        install_public_key: "2. 安装本地公钥",
        set_permissions: "3. 设置远端权限",
        verify_alias_login: "4. SSH Host 别名测试"
      },
      savedHost: (alias: string) => `已保存 Host ${alias}。`,
      editingHost: (alias: string) => `正在编辑受管理的 Host ${alias}。用相同别名提交会原地更新。`,
      deleteConfirm: (alias: string) => `确定要从 SSH config 删除 Host ${alias} 吗？`,
      deletedHost: (alias: string) => `已删除 Host ${alias}。`,
      hostAlias: "Host 别名",
      hostName: "Host IP",
      port: "端口",
      user: "用户",
      identityFile: "IdentityFile",
      bootstrapPassword: "一次性密码",
      bootstrapPasswordHelp: "可选。仅用于首次登录，把本地公钥追加到远端 ~/.ssh/authorized_keys，然后测试密钥登录。",
      showPassword: "显示一次性密码",
      hidePassword: "隐藏一次性密码",
      cancel: "取消",
      saving: "保存中...",
      writeSshConfig: "连接",
      reset: "重置",
      codexhubManaged: "CodexHub",
      sshHostBlocks: "SSH Host 块",
      repeatedSaves: "重复保存会更新同一别名，不会追加重复块。",
      newHost: "新建 Host",
      noManagedHosts: "没有 SSH Hosts",
      noManagedHostsBody: "点击主机页右上角“添加服务器”，连接成功后会创建第一个 SSH 配置项。",
      detectedSshHosts: "主机列表",
      detectedSshHostsBody: "CodexHub 列出本地 SSH config 中的 HostAlias，用于统一管理。",
      detect: "检测",
      refreshDetected: "一键测试",
      detectLocalConfig: "检测本地配置",
      detectedLocalHosts: (count: number) => `检测到 ${count} 个本地 SSH Host。`,
      testingAll: "测试中...",
      testedAll: "一键测试完成。",
      testedAllResult: (success: number, total: number) => `一键测试完成：${success}/${total} 台主机成功。`,
      testedAllFailed: (success: number, total: number) => `一键测试完成：${success}/${total} 台主机成功，其余主机失败。`,
      updateOutdatedCodex: "一键更新",
      updatingOutdatedCodex: "更新中...",
      updatedOutdatedCodex: (success: number, total: number) => `已更新 ${success}/${total} 台版本落后的主机。`,
      noOutdatedCodex: "没有版本落后的 Codex 主机。",
      localSource: "本地",
      source: "来源",
      bootstrapping: "连接中...",
      testing: "测试中...",
      checkVersion: "检查版本",
      checkingVersion: "检查中...",
      installCodex: "安装",
      installingCodex: "安装中...",
      updateCodex: "更新",
      updatingCodex: "更新中...",
      uninstallCodex: "卸载",
      uninstallingCodex: "卸载中...",
      uninstallCodexConfirmTitle: (alias: string) => `卸载 ${alias} 上的 Codex`,
      uninstallCodexConfirmBody:
        "这会永久删除远端 Codex 安装、~/.codex、CodexHub 管理的 env/API key 文件，以及相关 Codex 配置/缓存目录。不会创建备份。",
      confirmUninstallCodex: "卸载",
      details: "主机详情",
      detailsTitle: (alias: string) => `主机 · ${alias}`,
      detailsBody: "展示最近一次测试得到的连接状态与远端 Codex 就绪度。",
      sshStatus: "SSH 状态",
      arch: "架构",
      shell: "Shell",
      codexInstalled: "Codex 已安装",
      codexVersion: "Codex版本",
      latestCodexVersion: "最新版本",
      latestCodexUnknown: "未获取",
      configExists: "API 配置",
      skillsCount: "Skills 数量",
      yes: "是",
      no: "否",
      unknown: "未知",
      mockInventory: "Mock 清单",
      mockInventoryBody: "清单行合并自动发现的 SSH config hosts、CodexHub 管理 hosts 和 web 模式 mock rows。",
      alias: "别名",
      name: "名称",
      endpoint: "端点",
      status: "状态",
      profile: "配置",
      skills: "技能",
      lastSeen: "上次在线",
      actions: "操作",
      edit: "编辑",
      delete: "删除",
      test: "测试",
      testSsh: "测试 SSH",
      os: "系统",
      codex: "Codex",
      codexPathMissing: "已安装，PATH 未就绪",
      apiEnvMissing: "API 环境缺失",
      apiEnvReady: "API 环境就绪",
      apiEnvUnknown: "API 环境未知",
      apiEnvStatusTitle: (envVar: string) => `远端 ${envVar} 状态`,
      latency: "测试延迟"
    },
    profiles: {
      library: "本地配置",
      editor: "编辑器",
      edit: "编辑",
      preview: "预览",
      applyConfig: "应用配置",
      newProfile: "新建",
      newApiConfig: "新建API配置",
      duplicate: "复制",
      delete: "删除",
      import: "导入",
      actions: "操作",
      applyColumn: "应用",
      selectHosts: "选择主机",
      selectAll: "一键全选",
      apiConfig: "API配置",
      noApiConfig: "无配置",
      unknownApiConfig: "未知配置",
      detectCcSwitch: "检测 cc-switch",
      importDetected: "导入检测配置",
      save: "保存",
      saving: "保存中...",
      create: "创建",
      name: "名称",
      namePlaceholder: "自定义配置名",
      description: "说明",
      model: "模型",
      modelPlaceholder: DEFAULT_PROFILE_MODEL,
      provider: "Provider",
      providerPlaceholder: DEFAULT_PROFILE_PROVIDER,
      baseUrl: "Base URL",
      baseUrlPlaceholder: DEFAULT_PROFILE_BASE_URL,
      apiKeyEnvVar: "环境变量",
      apiKey: "API key",
      apiKeyPlaceholder: "粘贴新 key 后存储",
      apiKeyStoredPlaceholder: "已保存 API key",
      apiKeyLoading: "正在加载 key...",
      apiKeyMissing: "未找到已保存的 key。",
      showApiKey: "显示 API key",
      hideApiKey: "隐藏 API key",
      credentialStored: "凭据已存储",
      thirdPartyImport: "第三方导入",
      localStorageLabel: "本地存储",
      modelReasoningEffort: "模型推理",
      planModeReasoningEffort: "计划推理",
      fastMode: "快速模式",
      serviceTier: "服务层级",
      advanced: "高级",
      extraToml: "额外 TOML",
      source: "来源",
      selectedHosts: "已选主机",
      targetFiles: "目标文件",
      targetHost: "主机",
      targetPath: "路径",
      renderedToml: "配置 TOML",
      perHostStatus: "单主机状态",
      previewApply: "预览",
      applySelected: "应用所选",
      applyOne: "应用",
      batchApply: "批量应用",
      backupExpected: "备份",
      noChangeExpected: "无变化",
      noPreview: "选择主机并预览后再应用。",
      noProfiles: "暂无配置。",
      noProfilesBody: "新建或导入配置后即可渲染远端 Codex TOML。",
      noHostTargets: "暂无可用主机。",
      applySuccess: (name: string, count: number) => `已将 ${name} 应用到 ${count} 台主机。`,
      applyOperationTitle: "应用 API 配置",
      applyOperationStarted: (name: string, count: number) => `正在将 ${name} 应用到 ${count} 台已选主机。`,
      applyOperationWaiting: "正在执行远端配置脚本...",
      applyOperationSuccess: (name: string, count: number) => `${name} 已在 ${count} 台主机完成。`,
      applyOperationFailed: "部分主机应用 API 配置失败。",
      alreadyApplied: "已应用",
      importReady: (count: number) => `已导入 ${count} 个配置。`,
      deleteConfirm: (name: string) => `删除配置 ${name}？`,
      ccSwitchFound: (count: number) => `检测到 ${count} 个 cc-switch 配置。`,
      ccSwitchNone: "未检测到 cc-switch 配置。",
      ccSwitchChecking: "正在检测 cc-switch...",
      codexPrep: "远端 Codex",
      noHosts: "请先添加主机，再检查远端 Codex。",
      notChecked: "未检查",
      notInstalled: "未安装",
      operationFailed: "操作失败",
      hosts: "主机",
      applyOnline: "应用到在线主机"
    },
    codexOperation: {
      title: "Codex 维护",
      running: "运行中",
      success: "已完成",
      failed: "失败",
      progress: "进程",
      summary: "摘要",
      latestLog: "简要日志",
      started: "已开始远端维护。",
      waiting: "正在等待远端输出...",
      installHint: "安装会尝试官方下载、镜像兜底、本地上传，然后检查版本。",
      updateHint: "更新会尝试官方下载、镜像兜底、本地上传，然后检查版本。",
      uninstallHint: "卸载会永久删除远端 Codex 文件、配置以及 CodexHub 管理的 env/API key 文件。",
      noLogs: "尚未返回任务日志。",
      hide: "隐藏",
      close: "关闭",
      viewTasks: "查看任务"
    },
    skills: {
      library: "本地技能库",
      installedLibrary: "安装技能库",
      detect: "检测",
      detecting: "检测中...",
      detected: "检测完成",
      refresh: "刷新",
      refreshing: "刷新中...",
      refreshed: "已根据缓存刷新",
      importDirectory: "导入",
      download: "下载",
      downloadTitle: "下载 skill",
      downloadBody: "输入 GitHub 仓库 URL。",
      downloadAction: "下载",
      downloaded: "下载完成",
      githubUrl: "GitHub URL：",
      noSkills: "还没有本地技能",
      noSkillsBody: "使用“检测”“导入”或“下载”添加到本地技能库。",
      skill: "技能",
      source: "来源",
      addedAt: "添加日期",
      applications: "应用",
      actions: "操作",
      unapplied: "未应用",
      localMachine: "本机",
      sourceLocal: "本地",
      sourceGithub: "GitHub",
      preview: "预览",
      edit: "编辑",
      save: "保存",
      install: "安装",
      installSuccess: "安装成功！",
      installPartialFailure: "安装未全部成功。",
      uninstall: "卸载",
      uninstallSuccess: "卸载成功！",
      uninstallPartialFailure: "卸载未全部成功。",
      delete: "删除",
      firstScanTitle: "首次扫描",
      firstScanBody: "将扫描本机和所有主机 Codex skills，耗时较久。",
      firstScanAction: "扫描本机和主机",
      localOnlyDetect: "仅检测本机",
      selectTargets: "选择目标",
      selectAll: "全选",
      noTargets: "没有可用目标",
      installHint: "默认安装到 Codex skills 根目录。",
      uninstallHint: "只能选择已安装该 skill 的目标。",
      deleteTitle: "删除 skill",
      deleteBody: "从技能库删除该 skill。可以先从所有已知目标卸载，也可以只删除库记录。",
      uninstallAndDelete: "卸载并删除",
      directDelete: "直接删除",
      downloadInstalledTitle: "下载已安装技能",
      downloadInstalledBody: (name: string) => `将 ${name} 下载到本地技能库。`,
      downloadInstalledAction: "下载到技能库",
      downloadInstalledStarted: (name: string) => `正在将 ${name} 下载到本地技能库。`,
      downloadUnavailableLocalExists: "本地技能库已存在",
      uninstallInstalledTitle: "卸载已安装技能",
      uninstallInstalledBody: (name: string, target: string) =>
        `将从 ${target} 卸载 ${name}。该操作会直接删除技能目录，无法恢复。`,
      uninstallInstalledAction: "从当前目标卸载",
      uninstallInstalledStarted: (name: string, target: string) => `正在从 ${target} 卸载 ${name}。`,
      installedPreviewTarget: "目标",
      installedPreviewStatus: "状态",
      installedPreviewLocalLibrary: "本地技能库",
      installedPreviewNotDownloaded: "未下载",
      operationWaiting: "正在执行技能操作脚本...",
      operationDone: "操作完成。",
      target: "目标",
      path: "路径",
      details: "详细说明",
      hostIp: "Host IP",
      installedSkills: "已安装技能",
      noInstalledSkills: "无技能",
      status: "状态",
      available: "可安装",
      installedTarget: "已安装",
      unavailableTarget: "不可用",
      aboutFallback: "暂无简介。",
      imported: (count: number) => `已导入 ${count} 个技能。`,
      installed: (count: number) => `已安装到 ${count} 个目标。`,
      uninstalled: (count: number) => `已从 ${count} 个目标卸载。`
    },
    tasks: {
      runs: "运行",
      taskHistory: "任务历史",
      body: "TaskRun 行用于展示本地任务模型和工作队列。",
      action: "操作",
      host: "主机",
      status: "状态",
      started: "开始时间",
      summary: "摘要",
      details: "详情",
      logs: "日志",
      clearHistory: "一键清空",
      clearHistoryTitle: "清空全部任务记录？",
      clearHistoryBody: "所有已结束任务及其日志将导出为 JSON 归档并移入系统回收站；正在运行和排队的任务会保留。",
      clearHistoryMockBody: "所有已结束 Mock 任务记录将从内存中清除；正在运行和排队的任务会保留。",
      clearHistoryConfirm: "移入系统回收站",
      clearingHistory: "清空中...",
      historyCleared: (count: number) => `${count} 条已结束任务记录已移入系统回收站。`,
      mockHistoryCleared: (count: number) => `已从内存中清除 ${count} 条 Mock 任务记录。`,
      loadMore: "加载更早任务",
      loadingMore: "加载中...",
      taskLog: "任务日志",
      noTask: "无任务",
      noLogs: "暂无日志。",
      command: "命令",
      stdout: "stdout",
      stderr: "stderr",
      exitCode: "退出码",
      duration: "耗时",
      timedOut: "超时",
      noOutput: "（无输出）",
      minutesAgo: (minutes: number) => `${minutes} 分钟前`,
      actionLabels: {
        "Generate Ed25519 key": "生成 Ed25519 密钥",
        "Save SSH Host": "保存 SSH Host",
        "Apply profile": "应用配置",
        "Test SSH connection": "测试 SSH 连接",
        "Bootstrap SSH key": "配置 SSH 密钥",
        "Sync skill pack": "同步技能包",
        "Preview profile": "预览配置",
        "Probe remote system": "测试远端系统",
        "Check Codex version": "检查 Codex 版本",
        "Install Codex": "安装 Codex",
        "Update Codex": "更新 Codex",
        "Uninstall Codex": "卸载 Codex",
        "List remote skills": "列出远端 Skills",
        "Preview skill install": "预览 Skill 安装",
        "Install skill": "安装 Skill",
        "Delete SSH Host": "删除 SSH Host",
        "Delete profile": "删除配置",
        "Delete skill": "删除 Skill",
        "Check app update": "检查程序版本",
        "Install app update": "安装程序更新",
        "Refresh latest Codex version": "刷新 Codex 最新版本",
        "Import cc-switch profiles": "导入 cc-switch 配置",
        "Refresh discovered hosts": "刷新已发现主机",
        "Add host": "添加主机",
        "Update host": "更新主机",
        "Delete host": "删除主机",
        "Create profile": "创建配置",
        "Update profile": "更新配置",
        "Duplicate profile": "复制配置",
        "Import profiles": "导入配置",
        "Store profile credential": "保存配置凭据",
        "Migrate profile credential": "迁移配置凭据",
        "Delete profile credential": "删除配置凭据",
        "Import local skill": "导入本地 Skill",
        "Update skill details": "更新 Skill 详情",
        "Save settings": "保存设置",
        "Choose close button behavior": "设置关闭按钮行为",
        "Migrate local storage": "迁移本地数据",
        "Restore local storage": "恢复本地数据",
        "Sample host resources": "采样主机资源",
        "Frontend error": "前端错误"
      },
      unknownAction: "后台任务",
      summaryByStatus: {
        queued: (action: string) => `${action}已加入队列。`,
        running: (action: string) => `${action}正在执行。`,
        success: (action: string) => `${action}已完成。`,
        failed: (action: string) => `${action}失败，请查看日志详情。`,
        interrupted: (action: string) => `${action}已中断。`
      }
    },
    storage: {
      title: "本地数据需要处理",
      attentionSummary: (count: number) => `${count} 个本地数据存储需要处理，完成后相关写操作即可恢复。`,
      migrationRequired: "该存储仍使用可读取的旧版 schema。完成预览和确认迁移前，相关写操作保持锁定。",
      corrupt: "该存储已损坏。CodexHub 不会静默使用备份；请先预览通过校验的恢复方案。",
      recoveryRequired: "上一次存储操作被中断。重试或恢复前，请先检查持久任务记录。",
      previewMigration: "预览迁移",
      previewMigrations: "查看迁移方案",
      previewRestore: "预览恢复",
      planTitle: "确认本地数据操作",
      storeLabels: {
        settings: "应用设置",
        hosts: "主机数据",
        profiles: "配置数据",
        skills: "技能数据"
      },
      path: "数据路径",
      fingerprint: "源文件指纹",
      backupLocation: "备份位置",
      applyMigration: "备份并迁移",
      applyMigrations: "备份并迁移全部",
      restoreBackup: "先备份当前数据再恢复",
      cancel: "取消",
      working: "处理中...",
      migrationDone: "本地数据迁移完成。",
      migrationsDone: (count: number) => `${count} 个本地数据存储已完成迁移。`,
      restoreDone: "本地数据恢复完成。"
    },
    settings: {
      appearance: "外观",
      theme: "主题",
      platformAppearance: "平台",
      font: "字体",
      sidebarCompletionIndicators: "侧边栏视觉提示",
      runtime: "运行时",
      backend: "后端",
      app: "应用",
      remoteWrapper: "远程包装器",
      sshConfig: "SSH 配置",
      desktopBackendRequired: "需要桌面后端",
      localSsh: "本地密钥",
      sshKeyStatus: "SSH 密钥状态",
      sshKeyBody: "仅检查私钥文件是否存在。CodexHub 从不读取或显示私钥内容。",
      appUpdates: "版本信息",
      dailyUpdateCheck: "每日 04:00 自动检查",
      closeButton: "其他",
      closeButtonBehavior: "程序关闭按钮行为",
      networkProxy: "网络代理",
      networkProxyOptions: {
        auto: "自动",
        direct: "直连",
        manual: "手动"
      },
      networkProxyUrl: "代理地址",
      networkProxyManualTitle: "手动代理",
      networkProxyPort: "代理端口",
      networkProxyPortPlaceholder: "7890",
      networkProxySave: "保存代理",
      softwareName: "软件名",
      currentVersion: "当前版本",
      installedAt: "安装时间",
      latestVersion: "最新版本",
      updatedAt: "更新时间",
      notChecked: "未检查",
      checkFailed: "检查失败",
      updateCheckFailureHint: "你可以稍后在“任务”详情页回看本次运行的日志。",
      pendingConfiguration: "待配置",
      checkStableUpdate: "检查",
      updateChecking: "检查中...",
      installStableUpdate: "更新",
      sidebarInstallStableUpdate: "更新",
      previewUpdateButtonNotice: "Dev 预览：此按钮仅用于视觉检查。",
      settingsSaved: "设置已保存。",
      settingsSaveFailed: "设置保存失败，当前仍使用上一次确认成功的值。",
      settingsRetry: "重试保存",
      settingsSaving: "正在保存设置...",
      updateInstalling: "安装中...",
      refresh: "刷新",
      generating: "生成中...",
      generateEd25519: "生成 Ed25519",
      publicKey: "公钥",
      readyToCopy: "可复制",
      noPublicKey: "未检测到公钥",
      copyPublicKey: "复制公钥",
      copyPublicKeySuccess: "复制成功！",
      publicKeyEmpty: "生成或添加 SSH 公钥后会显示在这里。",
      commandReservations: "命令预留",
      commandSurface: "命令接口",
      privateFound: "已找到私钥",
      missing: "缺失",
      privatePath: "私钥路径",
      publicPath: "公钥路径",
      available: "可用",
      unknown: "未知",
      themeOptions: {
        system: "系统",
        light: "浅色",
        dark: "深色"
      },
      platformOptions: {
        auto: "自动",
        windows: "Windows",
        macos: "macOS"
      },
      closeButtonOptions: {
        ask: "下次询问",
        exit: "退出程序",
        "minimize-to-tray": "最小化到托盘"
      }
    },
    closeButtonPrompt: {
      title: "关闭按钮要执行什么操作？",
      body: "选择后 CodexHub 会记住你的偏好。之后可以在设置页面底部重新修改。",
      exitTitle: "退出程序",
      exitBody: "完全关闭 CodexHub。",
      minimizeTitle: "最小化到托盘",
      minimizeBody: "让 CodexHub 在后台继续运行，可从托盘恢复窗口。",
      cancel: "保持打开"
    },
    status: {
      host: {
        online: "在线",
        offline: "离线",
        unknown: "未知",
        testing: "测试中"
      },
      task: {
        queued: "排队中",
        running: "运行中",
        success: "成功",
        failed: "失败",
        interrupted: "已中断"
      },
      log: {
        info: "信息",
        warn: "警告",
        error: "错误"
      }
    }
  }
} as const;

type UICopy = (typeof uiCopy)[Locale];

function visibleSshConfigHostsForHosts(configHosts: SshConfigHost[], appHosts: Host[]) {
  const aliases = new Set(appHosts.map((host) => host.hostAlias.toLowerCase()));
  return configHosts.filter((host) => aliases.has(host.alias.toLowerCase()));
}

function nextDailyAppUpdateCheckAt(now = new Date()) {
  const next = new Date(now);
  next.setHours(APP_UPDATE_DAILY_CHECK_HOUR, 0, 0, 0);
  if (next.getTime() <= now.getTime()) next.setDate(next.getDate() + 1);
  return next;
}

function sectionOperationTone(result: unknown): SectionCompletionTone {
  if (result && typeof result === "object") {
    const candidate = result as { ok?: unknown; state?: unknown; status?: unknown; message?: unknown; task?: TaskRun };
    if (candidate.ok === false || candidate.state === "error" || candidate.status === "failed" || candidate.task?.status === "failed") {
      return "error";
    }
    if (typeof candidate.message === "string" && candidate.message.includes("partial-failure")) {
      return "error";
    }
  }
  return "success";
}

function CommandBar({
  ariaLabel,
  children,
  className = ""
}: {
  ariaLabel?: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={`commandBar ${className}`.trim()} role="toolbar" aria-label={ariaLabel}>
      {children}
    </div>
  );
}

function CommandGroup({
  ariaLabel,
  children,
  className = "",
  onClick
}: {
  ariaLabel?: string;
  children: ReactNode;
  className?: string;
  onClick?: MouseEventHandler<HTMLDivElement>;
}) {
  return (
    <div className={`commandGroup ${className}`.trim()} role={ariaLabel ? "toolbar" : "group"} aria-label={ariaLabel} onClick={onClick}>
      {children}
    </div>
  );
}

function CommandBarActions({
  actions,
  ariaLabel,
  className = ""
}: {
  actions: CommandBarAction[];
  ariaLabel?: string;
  className?: string;
}) {
  return (
    <CommandBar ariaLabel={ariaLabel} className={className}>
      {actions.map((action) => (
        <button
          className={action.kind === "primary" ? "primaryButton" : action.kind === "danger" ? "secondaryButton dangerButton" : "secondaryButton"}
          disabled={action.disabled}
          key={action.id}
          type="button"
          onClick={action.onClick}
        >
          {action.label}
        </button>
      ))}
    </CommandBar>
  );
}

function usePlatformAppearance() {
  return useContext(PlatformAppearanceContext);
}

function AppTitleBar({
  copy,
  onCloseRequest
}: {
  copy: UICopy;
  onCloseRequest: () => Promise<void> | void;
}) {
  const [title, setTitle] = useState("CodexHub");

  useEffect(() => {
    let cancelled = false;
    getCurrentWindow().title()
      .then((windowTitle) => {
        if (!cancelled && windowTitle.trim()) setTitle(windowTitle);
      })
      .catch(() => console.debug("Desktop window title is unavailable in this preview."));
    return () => {
      cancelled = true;
    };
  }, []);

  const handleDragMouseDown = (event: React.MouseEvent<HTMLElement>) => {
    if (event.button !== 0) return;
    if (event.detail === 2) {
      event.preventDefault();
      void runAction("maximize");
      return;
    }
    void getCurrentWindow().startDragging().catch(() => {
      console.debug("Desktop window dragging is unavailable in this preview.");
    });
  };

  const runAction = async (action: TitleBarAction) => {
    if (action === "close") {
      await onCloseRequest();
      return;
    }

    try {
      const currentWindow = getCurrentWindow();
      if (action === "minimize") await currentWindow.minimize();
      if (action === "maximize") await currentWindow.toggleMaximize();
    } catch {
      console.debug("Desktop window controls are unavailable in this preview.");
    }
  };

  return (
    <header className="appTitleBar">
      <div className="titleDragRegion" data-tauri-drag-region onMouseDown={handleDragMouseDown}>
        <div className="appTitle" data-tauri-drag-region>
        <img className="titleBarIcon" src={appLogoUrl} alt="" aria-hidden="true" />
        <span data-tauri-drag-region>{title}</span>
        </div>
      </div>
      <div className="captionControls" role="group" aria-label={title}>
        <button className="captionButton" data-action="minimize" type="button" aria-label={copy.windowControls.minimize} onClick={() => void runAction("minimize")}>
          <span className="captionGlyph" aria-hidden="true" />
        </button>
        <button className="captionButton" data-action="maximize" type="button" aria-label={copy.windowControls.maximize} onClick={() => void runAction("maximize")}>
          <span className="captionGlyph" aria-hidden="true" />
        </button>
        <button className="captionButton closeCaptionButton" data-action="close" type="button" aria-label={copy.windowControls.close} onClick={() => void runAction("close")}>
          <span className="captionGlyph" aria-hidden="true" />
        </button>
      </div>
    </header>
  );
}

function useActionErrorReporter(copy: UICopy) {
  const { notify } = useFeedback();
  return useCallback((error: unknown) => {
    const structured = parseApiError(error);
    const code: ApiErrorCode = structured?.code ?? "operation-failed";
    notify({
      title: copy.feedback.errorTitles[code],
      message: localizeFeedbackMessage(formatError(error), copy, "error"),
      taskId: structured?.taskId ?? undefined,
      tone: "error"
    });
  }, [copy.feedback.errorTitles, notify]);
}

function localizeFeedbackMessage(message: string, copy: UICopy, tone: FeedbackTone) {
  if (copy.common.locale !== "zh-CN") return message;
  const normalized = message.toLowerCase();
  if (normalized.includes("storage-migration-required") || normalized.includes("migration-required")) {
    return copy.feedback.migrationRequired;
  }
  if (normalized.includes("storage-corrupt") || normalized.includes("storage-recovery-required")) {
    return copy.feedback.storageCorrupt;
  }
  if (normalized.includes("partial-failure")) return copy.feedback.partialFailure;
  const cjkCount = message.match(/[\u3400-\u9fff]/gu)?.length ?? 0;
  const latinWordCount = message.match(/[a-z]{2,}/giu)?.length ?? 0;
  if (cjkCount >= Math.max(2, latinWordCount)) return message;
  return tone === "error" ? copy.feedback.genericError : copy.feedback.genericNotice;
}

function storageWriteBlockerError(item: StorageHealth) {
  if (item.state === "migration-required") {
    return `storage-migration-required:${item.store}: Preview and confirm the local data migration before writing.`;
  }
  if (item.state === "recovery-required") {
    return `storage-recovery-required:${item.store}: Resolve the interrupted local data operation before writing.`;
  }
  return `storage-corrupt:${item.store}: Preview and confirm recovery before writing.`;
}

function storageStoreIcon(store: string): PlatformIconId {
  if (store === "settings" || store === "hosts" || store === "profiles" || store === "skills") return store;
  return "warning";
}

function storageStoreLabel(store: string, copy: UICopy) {
  const labels = copy.storage.storeLabels as Record<string, string>;
  return labels[store] ?? store;
}

function ModalCloseButton({
  ariaLabel,
  disabled,
  onClick
}: {
  ariaLabel: string;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button className="modalCloseButton" disabled={disabled} type="button" onClick={onClick} aria-label={ariaLabel}>
      <span aria-hidden="true">×</span>
    </button>
  );
}

function ModalHeader({
  badge,
  children,
  className = "",
  description,
  icon,
  title,
  titleId,
  closeAriaLabel,
  closeDisabled,
  onClose
}: {
  badge?: ReactNode;
  children?: ReactNode;
  className?: string;
  description?: ReactNode;
  icon?: PlatformIconId;
  title: ReactNode;
  titleId: string;
  closeAriaLabel?: string;
  closeDisabled?: boolean;
  onClose?: () => void;
}) {
  return (
    <div className={`modalHeader ${className}`.trim()}>
      <div className="modalTitleBlock">
        {icon ? <span className="modalTitleIcon" aria-hidden="true"><PlatformIcon id={icon} /></span> : null}
        <div>
          <h2 id={titleId}>{title}</h2>
          {description ? <p>{description}</p> : null}
          {children}
        </div>
      </div>
      {badge ? <div className="modalHeaderBadge">{badge}</div> : null}
      {onClose ? <ModalCloseButton ariaLabel={closeAriaLabel ?? "Close"} disabled={closeDisabled} onClick={onClose} /> : null}
    </div>
  );
}

function ModalActions({
  children,
  className = "",
  dataHasHosts
}: {
  children: ReactNode;
  className?: string;
  dataHasHosts?: boolean;
}) {
  return <div className={`modalActions ${className}`.trim()} data-has-hosts={dataHasHosts ?? undefined}>{children}</div>;
}

type StorageActionPlan =
  | { kind: "migration"; plans: StorageMigrationPlan[] }
  | { kind: "restore"; plan: StorageRestorePlan };

function StorageHealthCenter({
  copy,
  health,
  onChanged
}: {
  copy: UICopy;
  health: StorageHealth[];
  onChanged: () => Promise<StorageHealth[]>;
}) {
  const { notify } = useFeedback();
  const [plan, setPlan] = useState<StorageActionPlan | null>(null);
  const [busyStore, setBusyStore] = useState<string | null>(null);
  const attention = health.filter((item) => item.state === "migration-required" || item.state === "corrupt" || item.state === "recovery-required");
  const migrations = attention.filter((item) => item.state === "migration-required");
  if (attention.length === 0 && !plan) return null;

  const preview = async (item: StorageHealth) => {
    setBusyStore(item.store);
    try {
      setPlan({ kind: "restore", plan: await api.previewStorageRestore(item.store) });
    } catch (error) {
      notify({ message: localizeFeedbackMessage(formatError(error), copy, "error"), taskId: taskIdForError(error), tone: "error" });
    } finally {
      setBusyStore(null);
    }
  };

  const previewMigrations = async () => {
    setBusyStore("migration-batch");
    try {
      const plans = await Promise.all(migrations.map((item) => api.previewStorageMigration(item.store)));
      setPlan({ kind: "migration", plans });
    } catch (error) {
      notify({ message: localizeFeedbackMessage(formatError(error), copy, "error"), taskId: taskIdForError(error), tone: "error" });
    } finally {
      setBusyStore(null);
    }
  };

  const apply = async () => {
    if (!plan) return;
    setBusyStore(plan.kind === "migration" ? "migration-batch" : plan.plan.store);
    try {
      if (plan.kind === "migration") {
        for (const migrationPlan of plan.plans) await api.applyStorageMigration(migrationPlan);
        notify({ message: copy.storage.migrationsDone(plan.plans.length), tone: "success" });
      } else {
        await api.restoreStorageBackup(plan.plan);
        notify({ message: copy.storage.restoreDone, tone: "success" });
      }
      setPlan(null);
      await onChanged();
    } catch (error) {
      notify({ message: localizeFeedbackMessage(formatError(error), copy, "error"), taskId: taskIdForError(error), tone: "error" });
    } finally {
      setBusyStore(null);
    }
  };

  const planItems = plan?.kind === "migration"
    ? plan.plans.map((item) => ({
        store: item.store,
        path: item.path,
        fingerprint: item.sourceSha256,
        backup: item.backupDirectory,
        schema: `v${item.fromSchemaVersion} → v${item.toSchemaVersion}`
      }))
    : plan ? [{
        store: plan.plan.store,
        path: plan.plan.targetPath,
        fingerprint: plan.plan.backupSha256,
        backup: plan.plan.backupPath,
        schema: null
      }] : [];

  return (
    <>
      {attention.length > 0 ? (
        <section className="storageHealthBanner" role={attention.some((item) => item.state === "corrupt" || item.state === "recovery-required") ? "alert" : "status"}>
          <div className="storageHealthSummary">
            <TitleWithIcon icon="warning" level={2}>{copy.storage.title}</TitleWithIcon>
            <p>{copy.storage.attentionSummary(attention.length)}</p>
          </div>
          <div className="storageHealthCompact">
            <div className="storageHealthStores">
              {attention.map((item) => <Badge key={item.store} tone={item.state === "migration-required" ? "yellow" : "red"}>{item.store}</Badge>)}
            </div>
            <div className="storageHealthActions">
              {migrations.length > 0 ? (
                <button className="secondaryButton" disabled={Boolean(busyStore)} type="button" onClick={() => void previewMigrations()}>
                  {busyStore === "migration-batch" ? copy.storage.working : copy.storage.previewMigrations}
                </button>
              ) : null}
              {attention.filter((item) => item.state === "corrupt").map((item) => (
                <button className="secondaryButton" disabled={Boolean(busyStore)} key={item.store} type="button" onClick={() => void preview(item)}>
                  {busyStore === item.store ? copy.storage.working : `${copy.storage.previewRestore}: ${item.store}`}
                </button>
              ))}
            </div>
          </div>
        </section>
      ) : null}
      {plan ? (
        <div className="modalBackdrop" role="presentation">
          <ModalFrame className="taskLogModal storagePlanModal" titleId="storage-plan-title">
            <ModalHeader
              title={copy.storage.planTitle}
              titleId="storage-plan-title"
              icon="warning"
              closeAriaLabel={copy.storage.cancel}
              closeDisabled={Boolean(busyStore)}
              onClose={() => setPlan(null)}
            />
            <div className="storagePlanDetails" data-count={planItems.length}>
              {planItems.map((item) => (
                <section className="storagePlanCard" key={item.store}>
                  <header className="storagePlanCardHeader">
                    <div className="storagePlanCardTitle">
                      <span className="storagePlanCardIcon" aria-hidden="true"><PlatformIcon id={storageStoreIcon(item.store)} /></span>
                      <div>
                        <strong>{storageStoreLabel(item.store, copy)}</strong>
                        <span>{item.store}</span>
                      </div>
                    </div>
                    {item.schema ? <Badge tone="yellow">{item.schema}</Badge> : null}
                  </header>
                  <div className="storagePlanField">
                    <span>{copy.storage.path}</span>
                    <code className="storagePlanCode">{item.path}</code>
                  </div>
                  <div className="storagePlanField">
                    <span>{copy.storage.fingerprint}</span>
                    <code className="storagePlanCode">{item.fingerprint}</code>
                  </div>
                  <div className="storagePlanField">
                    <span>{copy.storage.backupLocation}</span>
                    <code className="storagePlanCode">{item.backup}</code>
                  </div>
                </section>
              ))}
            </div>
            <ModalActions>
              <button className="secondaryButton" disabled={Boolean(busyStore)} type="button" onClick={() => setPlan(null)}>{copy.storage.cancel}</button>
              <button className="primaryButton" disabled={Boolean(busyStore)} type="button" onClick={() => void apply()}>
                {busyStore ? copy.storage.working : plan.kind === "migration" ? copy.storage.applyMigrations : copy.storage.restoreBackup}
              </button>
            </ModalActions>
          </ModalFrame>
        </div>
      ) : null}
    </>
  );
}

function PlatformIcon({ id }: { id: PlatformIconId }) {
  const platform = usePlatformAppearance();
  if (platform === "macos") {
    return <span className="emojiIcon" aria-hidden="true">{platformEmojiIcons[id]}</span>;
  }
  return <WindowsIcon id={id} />;
}

function NavIcon({ id }: { id: NavIconId }) {
  return <PlatformIcon id={id} />;
}

function TitleWithIcon({
  children,
  icon,
  level
}: {
  children: ReactNode;
  icon: PlatformIconId;
  level: 1 | 2 | 3;
}) {
  const content = (
    <>
      <span className="titleIcon" data-level={level} aria-hidden="true">
        <PlatformIcon id={icon} />
      </span>
      <span className="titleText">{children}</span>
    </>
  );

  if (level === 1) return <h1 className="titleWithIcon" data-level={level}>{content}</h1>;
  if (level === 2) return <h2 className="titleWithIcon" data-level={level}>{content}</h2>;
  return <h3 className="titleWithIcon" data-level={level}>{content}</h3>;
}

function WindowsIcon({ id }: { id: PlatformIconId }) {
  return (
    <svg className="navGlyph" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      {id === "dashboard" ? (
        <>
          <path d="M4 11.2 12 4l8 7.2" />
          <path d="M6.5 10.5V20h11v-9.5" />
          <path d="M10 20v-5h4v5" />
        </>
      ) : null}
      {id === "hosts" ? (
        <>
          <rect x="4" y="5" width="16" height="11" rx="1.6" />
          <path d="M9 20h6" />
          <path d="M12 16v4" />
        </>
      ) : null}
      {id === "profiles" ? (
        <>
          <path d="M7 4.5h7l3 3V20H7z" />
          <path d="M14 4.5v3h3" />
          <path d="M9.5 12h5" />
          <path d="M9.5 15.5H14" />
        </>
      ) : null}
      {id === "skills" ? (
        <>
          <path d="M8.2 4.5h7.6v5.1h3.7v6.2h-3.7v3.7H8.2v-3.7H4.5V9.6h3.7z" />
          <path d="M10 9.6h4" />
          <path d="M10 14.2h4" />
        </>
      ) : null}
      {id === "monitor" ? (
        <>
          <path d="M4.5 18.5h15" />
          <path d="M7 15.5v-4" />
          <path d="M12 15.5V7" />
          <path d="M17 15.5v-6" />
          <path d="m5.5 9.5 3.5 2.8 3.5-4.5 5.8 3.2" />
        </>
      ) : null}
      {id === "tasks" ? (
        <>
          <rect x="5" y="4.5" width="14" height="15" rx="1.8" />
          <path d="m8 9 1.4 1.4L12 7.8" />
          <path d="M13.5 9.5H16" />
          <path d="m8 14.3 1.4 1.4L12 13" />
          <path d="M13.5 14.8H16" />
        </>
      ) : null}
      {id === "settings" ? (
        <>
          <circle cx="12" cy="12" r="2.8" />
          <path d="M12 4.5v2.2" />
          <path d="M12 17.3v2.2" />
          <path d="M4.5 12h2.2" />
          <path d="M17.3 12h2.2" />
          <path d="m6.7 6.7 1.6 1.6" />
          <path d="m15.7 15.7 1.6 1.6" />
          <path d="m17.3 6.7-1.6 1.6" />
          <path d="m8.3 15.7-1.6 1.6" />
        </>
      ) : null}
      {id === "download" || id === "install" ? (
        <>
          <path d="M12 4.5v10" />
          <path d="m8.5 11 3.5 3.5 3.5-3.5" />
          <path d="M5.5 18.5h13" />
        </>
      ) : null}
      {id === "delete" ? (
        <>
          <path d="M5.5 7h13" />
          <path d="M9 7V5h6v2" />
          <path d="M7.5 9.5 8.4 20h7.2l.9-10.5" />
          <path d="M10.5 11.5v5" />
          <path d="M13.5 11.5v5" />
        </>
      ) : null}
      {id === "preview" ? (
        <>
          <path d="M3.5 12c2.1-4 5-6 8.5-6s6.4 2 8.5 6c-2.1 4-5 6-8.5 6s-6.4-2-8.5-6Z" />
          <circle cx="12" cy="12" r="2.6" />
        </>
      ) : null}
      {id === "scan" ? (
        <>
          <circle cx="10.5" cy="10.5" r="5.5" />
          <path d="m15 15 4.5 4.5" />
          <path d="M10.5 7.5v3h3" />
        </>
      ) : null}
      {id === "network" ? (
        <>
          <circle cx="12" cy="12" r="7" />
          <path d="M5 12h14" />
          <path d="M12 5c2 2.2 3 4.5 3 7s-1 4.8-3 7" />
          <path d="M12 5c-2 2.2-3 4.5-3 7s1 4.8 3 7" />
        </>
      ) : null}
      {id === "key" ? (
        <>
          <circle cx="8" cy="12" r="3.2" />
          <path d="M11 12h8" />
          <path d="M16 12v3" />
          <path d="M19 12v2" />
        </>
      ) : null}
      {id === "language" ? (
        <>
          <circle cx="12" cy="12" r="7.2" />
          <path d="M4.8 12h14.4" />
          <path d="M12 4.8c2 2.1 3 4.5 3 7.2s-1 5.1-3 7.2" />
          <path d="M12 4.8c-2 2.1-3 4.5-3 7.2s1 5.1 3 7.2" />
        </>
      ) : null}
      {id === "terminal" ? (
        <>
          <path d="m5 8 4 4-4 4" />
          <path d="M11.5 16h7" />
        </>
      ) : null}
      {id === "update" ? (
        <>
          <path d="M18.5 9A6.8 6.8 0 0 0 6.2 7.2L4.5 9" />
          <path d="M4.5 5.2V9h3.8" />
          <path d="M5.5 15a6.8 6.8 0 0 0 12.3 1.8l1.7-1.8" />
          <path d="M19.5 18.8V15h-3.8" />
        </>
      ) : null}
      {id === "warning" ? (
        <>
          <path d="M12 4.5 20 19H4z" />
          <path d="M12 9.5v4" />
          <path d="M12 16.7h.1" />
        </>
      ) : null}
      {id === "close" ? (
        <>
          <path d="M7 7 17 17" />
          <path d="M17 7 7 17" />
        </>
      ) : null}
    </svg>
  );
}

function App() {
  const { notify, configure: configureFeedback } = useFeedback();
  const [activeSection, setActiveSection] = useState<SectionId>("dashboard");
  const [settings, setSettings] = useState<AppSettings>(() =>
    apiMode === "mock" ? loadMockSettings() : loadDesktopSettingsCache()
  );
  const [health, setHealth] = useState<Health>(fallbackHealth);
  const [appUpdateStatus, setAppUpdateStatus] = useState<AppUpdateStatus>(fallbackAppUpdateStatus);
  const [appUpdateFailureTask, setAppUpdateFailureTask] = useState<TaskRun | null>(null);
  const [appUpdateChecking, setAppUpdateChecking] = useState(false);
  const [appUpdateInstalling, setAppUpdateInstalling] = useState(false);
  const [hosts, setHosts] = useState<Host[]>([]);
  const [profiles, setProfiles] = useState<Profile[]>([]);
  const [skillPacks, setSkillPacks] = useState<SkillPack[]>([]);
  const [skillInventoryStatus, setSkillInventoryStatus] = useState<SkillInventoryStatus>({
    firstHostScanCompleted: false,
    localSkillRoot: "",
    localSkills: [],
    hostInventories: []
  });
  const [tasks, setTasks] = useState<TaskRun[]>([]);
  const [taskNextCursor, setTaskNextCursor] = useState<string | null>(null);
  const [taskLoadingMore, setTaskLoadingMore] = useState(false);
  const [unacknowledgedTaskIds, setUnacknowledgedTaskIds] = useState<Set<string>>(() => new Set());
  const [requestedTaskId, setRequestedTaskId] = useState<string | null>(null);
  const [storageHealth, setStorageHealth] = useState<StorageHealth[]>([]);
  const [sshStatus, setSshStatus] = useState<SshStatus | null>(null);
  const [sshConfigHosts, setSshConfigHosts] = useState<SshConfigHost[]>([]);
  const [latestCodexVersion, setLatestCodexVersion] = useState<LatestCodexVersion | null>(null);
  const [resourceSnapshots, setResourceSnapshots] = useState<HostResourceSnapshot[]>([]);
  const [resourceCheckedAt, setResourceCheckedAt] = useState<string | null>(null);
  const [resourceBusy, setResourceBusy] = useState(false);
  const [resourceError, setResourceError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [bootstrapError, setBootstrapError] = useState<string | null>(null);
  const [settingsSaving, setSettingsSaving] = useState(false);
  const [settingsSaveError, setSettingsSaveError] = useState<string | null>(null);
  const [pendingSettings, setPendingSettings] = useState<AppSettings | null>(null);
  const [sshBusy, setSshBusy] = useState(false);
  const [hostBusy, setHostBusy] = useState<Record<string, HostBusyAction>>({});
  const [hostModalOpen, setHostModalOpen] = useState(false);
  const [newProfileRequest, setNewProfileRequest] = useState(0);
  const [codexOperationModal, setCodexOperationModal] = useState<CodexOperationModalState | null>(null);
  const [codexUninstallConfirm, setCodexUninstallConfirm] = useState<CodexUninstallConfirmState | null>(null);
  const [setupGuideOpen, setSetupGuideOpen] = useState(false);
  const [setupGuideStep, setSetupGuideStep] = useState<SetupGuideStep>("preferences");
  const [setupGuideSshConfigHosts, setSetupGuideSshConfigHosts] = useState<SshConfigHost[]>([]);
  const [setupGuideBusy, setSetupGuideBusy] = useState(false);
  const [closeButtonPromptOpen, setCloseButtonPromptOpen] = useState(false);
  const [closeButtonPromptBusy, setCloseButtonPromptBusy] = useState(false);
  const [networkProxyManualOpen, setNetworkProxyManualOpen] = useState(false);
  const [sectionCompletionSignals, setSectionCompletionSignals] = useState<SectionCompletionSignals>({});
  const sidebarCompletionIndicatorsRef = useRef(settings.sidebarCompletionIndicators);
  const appUpdateStatusRef = useRef(fallbackAppUpdateStatus);
  const appUpdateBusyRef = useRef(false);
  const resourceBusyRef = useRef(false);
  const resourceMonitorVisitedRef = useRef(false);
  const settingsSaveBusyRef = useRef(false);

  const locale: Locale = settings.fontPreset === "zh-cn" ? "zh" : "en";
  const copy = uiCopy[locale];
  const setNotice = useCallback((message: string) => {
    notify({ message: localizeFeedbackMessage(message, copy, "success"), tone: "success" });
  }, [copy, notify]);
  const setInfoNotice = useCallback((message: string, placement: FeedbackPlacement = "detail") => {
    notify({ message: localizeFeedbackMessage(message, copy, "info"), placement, tone: "info" });
  }, [copy, notify]);
  const setErrorNotice = useCallback((message: string, taskId?: string) => {
    notify({ message: localizeFeedbackMessage(message, copy, "error"), taskId, tone: "error" });
  }, [copy, notify]);
  const runtimePlatform = useMemo(() => getPlatform(), []);
  const effectivePlatform = resolvePlatformAppearance(settings.platformAppearance);
  const usesCustomTitleBar = isWindows(runtimePlatform);
  const appUpdateBusy = appUpdateChecking || appUpdateInstalling;
  const previewSidebarStableUpdateButton = PREVIEW_SIDEBAR_UPDATE_BUTTON && appUpdateStatus.channel === "dev";
  const showSidebarStableUpdateButton =
    previewSidebarStableUpdateButton ||
    (appUpdateStatus.channel === "stable" && (appUpdateStatus.state === "available" || appUpdateInstalling));
  const canInstallSidebarStableUpdate =
    previewSidebarStableUpdateButton ||
    (appUpdateStatus.channel === "stable" && appUpdateStatus.configured && appUpdateStatus.state === "available" && !appUpdateBusy);
  useEffect(() => {
    sidebarCompletionIndicatorsRef.current = settings.sidebarCompletionIndicators;
    if (!settings.sidebarCompletionIndicators) setSectionCompletionSignals({});
  }, [settings.sidebarCompletionIndicators]);

  useEffect(() => {
    appUpdateStatusRef.current = appUpdateStatus;
  }, [appUpdateStatus]);

  useEffect(() => {
    appUpdateBusyRef.current = appUpdateChecking || appUpdateInstalling;
  }, [appUpdateChecking, appUpdateInstalling]);

  useEffect(() => {
    resourceBusyRef.current = resourceBusy;
  }, [resourceBusy]);

  const clearSectionCompletionSignal = useCallback((section: SectionId) => {
    setSectionCompletionSignals((current) => {
      if (!current[section]) return current;
      const next = { ...current };
      delete next[section];
      return next;
    });
  }, []);

  const markSectionCompletionSignal = useCallback((section: SectionId, tone: SectionCompletionTone) => {
    if (section === "tasks" || !sidebarCompletionIndicatorsRef.current) return;
    setSectionCompletionSignals((current) => (current[section] === tone ? current : { ...current, [section]: tone }));
  }, []);

  const runSectionOperation = useCallback(async <T,>(
    section: SectionId,
    action: () => Promise<T>,
    options: SectionOperationOptions<T> = {}
  ) => {
    try {
      const result = await action();
      const tone = options.classify ? options.classify(result) : sectionOperationTone(result);
      if (tone) markSectionCompletionSignal(section, tone);
      return result;
    } catch (error) {
      markSectionCompletionSignal(section, "error");
      throw error;
    }
  }, [markSectionCompletionSignal]);

  const handleContentInteraction = useCallback(() => {
    clearSectionCompletionSignal(activeSection);
  }, [activeSection, clearSectionCompletionSignal]);

  const refreshSshState = async () => {
    const [nextSshStatus, allSshConfigHosts, nextHosts] = await Promise.all([
      api.getSshStatus(),
      api.listSshConfigHosts(),
      api.listHosts()
    ]);
    const nextSshConfigHosts = visibleSshConfigHostsForHosts(allSshConfigHosts, nextHosts);
    setSshStatus(nextSshStatus);
    setSshConfigHosts(nextSshConfigHosts);
    setHosts(nextHosts);
    return {
      sshStatus: nextSshStatus,
      sshConfigHosts: nextSshConfigHosts,
      hosts: nextHosts
    };
  };

  const refreshSshDetectionState = async () => {
    const [nextSshStatus, detectedSshConfigHosts] = await Promise.all([
      api.getSshStatus(),
      api.listSshConfigHosts()
    ]);
    setSshStatus(nextSshStatus);
    setSetupGuideSshConfigHosts(detectedSshConfigHosts);
    return {
      sshStatus: nextSshStatus,
      sshConfigHosts: detectedSshConfigHosts
    };
  };

  const refreshResourceMonitor = useCallback(async (trigger: ResourceRefreshTrigger): Promise<HostResourceBatchResult | null> => {
    const showFeedback = trigger !== "auto";
    if (resourceBusyRef.current) return null;
    if (hosts.length === 0) {
      setResourceSnapshots([]);
      setResourceCheckedAt(null);
      if (showFeedback) setInfoNotice(copy.monitor.refreshed(0));
      return null;
    }

    resourceBusyRef.current = true;
    setResourceBusy(true);
    if (showFeedback) setResourceError(null);

    try {
      const result = await api.sampleHostResources(
        hosts.map((host) => host.hostAlias),
        RESOURCE_MONITOR_SAMPLE_TIMEOUT_MS
      );
      setResourceSnapshots(result.snapshots);
      setResourceCheckedAt(result.checkedAt);
      if (showFeedback) setInfoNotice(copy.monitor.refreshed(result.snapshots.length));
      return result;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setResourceError(message);
      if (showFeedback) setErrorNotice(copy.monitor.refreshFailed, taskIdForError(error));
      return null;
    } finally {
      resourceBusyRef.current = false;
      setResourceBusy(false);
    }
  }, [copy.monitor, hosts, setErrorNotice, setInfoNotice]);

  useEffect(() => {
    if (loading || activeSection !== "monitor") return;
    const firstEntry = !resourceMonitorVisitedRef.current;
    resourceMonitorVisitedRef.current = true;
    if (firstEntry) void refreshResourceMonitor("initial");
    else if (settings.resourceMonitorAutoRefresh) void refreshResourceMonitor("auto");
    if (!settings.resourceMonitorAutoRefresh) return;
    const timer = window.setInterval(() => {
      void refreshResourceMonitor("auto");
    }, settings.resourceMonitorRefreshSeconds * 1000);
    return () => window.clearInterval(timer);
  }, [
    activeSection,
    loading,
    refreshResourceMonitor,
    settings.resourceMonitorAutoRefresh,
    settings.resourceMonitorRefreshSeconds
  ]);

  const importLocalSshConfig = async () => {
    const [nextSshStatus, detectedSshConfigHosts, nextHosts] = await Promise.all([
      api.getSshStatus(),
      api.listSshConfigHosts(),
      api.refreshDiscoveredHosts()
    ]);
    setSshStatus(nextSshStatus);
    setSetupGuideSshConfigHosts(detectedSshConfigHosts);
    setSshConfigHosts(detectedSshConfigHosts);
    setHosts(nextHosts);
    return {
      sshStatus: nextSshStatus,
      sshConfigHosts: detectedSshConfigHosts,
      detectedSshConfigHosts,
      hosts: nextHosts
    };
  };

  const handleDetectLocalSshHosts = async () => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const result = await importLocalSshConfig();
      setNotice(copy.hosts.detectedLocalHosts(result.detectedSshConfigHosts.length));
      return result;
    });
  };

  const refreshLatestCodex = async (force = false) => {
    const latest = await api.refreshLatestCodexVersion(force);
    setLatestCodexVersion(latest);
    return latest;
  };

  const refreshTasks = useCallback(async () => {
    const page = await api.queryTasks({ limit: MAX_VISIBLE_TASKS, cursor: null });
    const nextTasks = normalizeTaskRunsForUi(page.items);
    setTasks(nextTasks);
    setTaskNextCursor(page.nextCursor);
    setUnacknowledgedTaskIds(new Set(page.unacknowledgedTaskIds));
    return nextTasks;
  }, []);

  const loadMoreTasks = useCallback(async () => {
    if (!taskNextCursor || taskLoadingMore) return;
    setTaskLoadingMore(true);
    try {
      const page = await api.queryTasks({ limit: MAX_VISIBLE_TASKS, cursor: taskNextCursor });
      const olderTasks = normalizeTaskRunsForUi(page.items);
      setTasks((current) => {
        const currentIds = new Set(current.map((task) => task.id));
        return mergeTaskRunsForUi(current, olderTasks.filter((task) => !currentIds.has(task.id)));
      });
      setTaskNextCursor(page.nextCursor);
      setUnacknowledgedTaskIds(new Set(page.unacknowledgedTaskIds));
    } catch (error) {
      setErrorNotice(formatError(error), taskIdForError(error));
    } finally {
      setTaskLoadingMore(false);
    }
  }, [setErrorNotice, taskLoadingMore, taskNextCursor]);

  const openTaskDetail = useCallback((taskId: string) => {
    setActiveSection("tasks");
    setRequestedTaskId(taskId);
    if (!unacknowledgedTaskIds.has(taskId)) return;
    setUnacknowledgedTaskIds((current) => {
      const next = new Set(current);
      next.delete(taskId);
      return next;
    });
    void api.acknowledgeTask(taskId).then(() => refreshTasks()).catch((error) => {
      setUnacknowledgedTaskIds((current) => new Set(current).add(taskId));
      setErrorNotice(formatError(error), taskId);
    });
  }, [refreshTasks, setErrorNotice, unacknowledgedTaskIds]);

  const clearTaskHistory = useCallback(async () => {
    const cleared = await api.clearTaskHistory();
    await refreshTasks();
    setNotice(apiMode === "mock" ? copy.tasks.mockHistoryCleared(cleared) : copy.tasks.historyCleared(cleared));
    return cleared;
  }, [copy.tasks, refreshTasks, setNotice]);

  useEffect(() => {
    configureFeedback({
      labels: copy.feedback,
      onOpenTask: openTaskDetail
    });
  }, [configureFeedback, copy.feedback, openTaskDetail]);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | null = null;
    void api.onTaskUpdated(() => {
      if (!active) return;
      void refreshTasks().catch((error) => setErrorNotice(formatError(error), taskIdForError(error)));
    }).then((dispose) => {
      if (active) unlisten = dispose;
      else dispose();
    }).catch((error) => {
      if (active) setErrorNotice(formatError(error), taskIdForError(error));
    });
    return () => {
      active = false;
      unlisten?.();
    };
  }, [refreshTasks, setErrorNotice]);

  const refreshStorageHealth = useCallback(async () => {
    const health = await api.getStorageHealth();
    setStorageHealth(health);
    return health;
  }, []);

  useEffect(() => {
    void refreshStorageHealth().catch((error) => setErrorNotice(formatError(error), taskIdForError(error)));
  }, [refreshStorageHealth, setErrorNotice]);

  const runStableUpdateCheck = useCallback(async (mode: "manual" | "daily" = "manual") => {
    const currentStatus = appUpdateStatusRef.current;
    if (mode === "daily" && (currentStatus.channel !== "stable" || appUpdateBusyRef.current)) {
      return currentStatus;
    }
    const showFeedback = mode === "manual";
    setAppUpdateChecking(true);
    setAppUpdateStatus((current) => ({
      ...current,
      state: "checking",
      message: copy.settings.updateChecking
    }));
    try {
      const nextStatus = await api.checkStableUpdate();
      setAppUpdateStatus(nextStatus);
      if (showFeedback) setNotice(nextStatus.message);
      const nextTasks = await refreshTasks();
      if (nextStatus.state === "error") {
        const recordedTask = latestAppUpdateTask(nextTasks);
        if (showFeedback) {
          setErrorNotice(nextStatus.message, recordedTask?.id);
          setAppUpdateFailureTask(recordedTask);
        }
      }
      return nextStatus;
    } catch (error) {
      const message = formatError(error);
      let nextTasks: TaskRun[] = [];
      try {
        nextTasks = await refreshTasks();
      } catch (taskError) {
        setErrorNotice(formatError(taskError), taskIdForError(taskError));
      }
      const task = latestAppUpdateTask(nextTasks);
      const errorStatus = {
        ...currentStatus,
        state: "error" as const,
        message
      };
      if (showFeedback) {
        setErrorNotice(message, task?.id);
        setAppUpdateFailureTask(task);
      }
      setAppUpdateStatus(errorStatus);
      return errorStatus;
    } finally {
      setAppUpdateChecking(false);
    }
  }, [copy.settings.updateChecking, refreshTasks]);

  const handleCheckStableUpdate = async () => {
    return runSectionOperation("settings", () => runStableUpdateCheck("manual"));
  };

  const handleInstallStableUpdate = async () => {
    return runSectionOperation("settings", async () => {
      setAppUpdateInstalling(true);
      setAppUpdateStatus((current) => ({
        ...current,
        state: "installing",
        message: copy.settings.updateInstalling
      }));
      try {
        const nextStatus = await api.installStableUpdate();
        setAppUpdateStatus(nextStatus);
        setNotice(nextStatus.message);
        const nextTasks = await refreshTasks();
        if (nextStatus.state === "error") {
          const recordedTask = latestAppInstallTask(nextTasks);
          setErrorNotice(nextStatus.message, recordedTask?.id);
          setAppUpdateFailureTask(recordedTask);
        }
        return nextStatus;
      } catch (error) {
        const message = formatError(error);
        let nextTasks: TaskRun[] = [];
        try {
          nextTasks = await refreshTasks();
        } catch (taskError) {
          setErrorNotice(formatError(taskError), taskIdForError(taskError));
        }
        const task = latestAppInstallTask(nextTasks);
        const errorStatus = {
          ...appUpdateStatusRef.current,
          state: "error" as const,
          message
        };
        setAppUpdateStatus(errorStatus);
        setErrorNotice(message, task?.id);
        setAppUpdateFailureTask(task);
        return errorStatus;
      } finally {
        setAppUpdateInstalling(false);
      }
    });
  };

  const handleSidebarStableUpdate = () => {
    if (previewSidebarStableUpdateButton) {
      setNotice(copy.settings.previewUpdateButtonNotice);
      return;
    }
    void handleInstallStableUpdate();
  };

  useEffect(() => {
    let mounted = true;

    loadInitialAppData()
      .then(({ settings: nextSettings, health: nextHealth, appUpdateStatus: nextAppUpdateStatus, hosts: nextHosts, profiles: nextProfiles, skillPacks: nextSkillPacks, skillInventoryStatus: nextSkillInventoryStatus, tasks: nextTasks, taskNextCursor: nextTaskCursor, unacknowledgedTaskIds: nextUnacknowledgedTaskIds }) => {
        if (!mounted) return;
        setSettings(nextSettings);
        setHealth(nextHealth);
        setAppUpdateStatus(nextAppUpdateStatus);
        setHosts(nextHosts);
        setProfiles(nextProfiles);
        setSkillPacks(nextSkillPacks);
        setSkillInventoryStatus(nextSkillInventoryStatus);
        setTasks(normalizeTaskRunsForUi(nextTasks));
        setTaskNextCursor(nextTaskCursor);
        setUnacknowledgedTaskIds(new Set(nextUnacknowledgedTaskIds));
        setSetupGuideStep("preferences");
        setSetupGuideOpen(!nextSettings.setupGuideDismissed);
        if (nextSettings.setupGuideDismissed || nextHosts.length > 0) {
          void refreshSshState();
        }
      })
      .catch((error) => {
        if (mounted) setBootstrapError(formatError(error));
      })
      .finally(() => {
        if (mounted) setLoading(false);
      });

    return () => {
      mounted = false;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let timer: number | undefined;
    const scheduleNextAppUpdateCheck = () => {
      const next = nextDailyAppUpdateCheckAt();
      timer = window.setTimeout(() => {
        if (cancelled) return;
        void runStableUpdateCheck("daily").finally(() => {
          if (!cancelled) scheduleNextAppUpdateCheck();
        });
      }, next.getTime() - Date.now());
    };
    scheduleNextAppUpdateCheck();
    return () => {
      cancelled = true;
      if (typeof timer === "number") window.clearTimeout(timer);
    };
  }, [runStableUpdateCheck]);

  useEffect(() => {
    let mounted = true;
    void api.refreshLatestCodexVersion(false).then((latest) => {
      if (mounted) setLatestCodexVersion(latest);
    });
    return () => {
      mounted = false;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let timer: number | undefined;
    const scheduleNextRefresh = () => {
      const now = new Date();
      const next = new Date(now);
      next.setHours(4, 0, 0, 0);
      if (next.getTime() <= now.getTime()) next.setDate(next.getDate() + 1);
      timer = window.setTimeout(() => {
        if (cancelled) return;
        void api.refreshLatestCodexVersion(true).then((latest) => {
          if (!cancelled) setLatestCodexVersion(latest);
          scheduleNextRefresh();
        });
      }, next.getTime() - now.getTime());
    };
    scheduleNextRefresh();
    return () => {
      cancelled = true;
      if (typeof timer === "number") window.clearTimeout(timer);
    };
  }, []);

  useEffect(() => {
    applyAppSettings(settings);
  }, [settings]);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | null = null;
    void api.onCloseButtonBehaviorRequested(() => {
      setCloseButtonPromptOpen(true);
      setCloseButtonPromptBusy(false);
    }).then((dispose) => {
      if (active) {
        unlisten = dispose;
        return;
      }
      dispose();
    });
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  const selectedCopy = copy.sections[activeSection];
  const onlineCount = hosts.filter((host) => host.status === "online").length;
  const appliedProfileCount = useMemo(
    () => new Set(hosts.map((host) => host.profileId).filter((profileId): profileId is string => Boolean(profileId))).size,
    [hosts]
  );
  const successfulTaskCount = tasks.filter((task) => task.status === "success").length;

  const profileById = useMemo(() => new Map(profiles.map((profile) => [profile.id, profile])), [profiles]);

  const handleAddHost = () => {
    setActiveSection("hosts");
    setHostModalOpen(true);
    setInfoNotice(copy.notices.addHost, "global");
  };

  const handleNewProfile = () => {
    setNewProfileRequest((current) => current + 1);
    setInfoNotice(copy.notices.newApiConfig, "global");
  };

  const handleConnectSshHost = async (
    draft: SshHostDraft,
    password: string,
    requestId: string,
    onProgress: (event: SshBootstrapProgressEvent) => void
  ): Promise<SshBootstrapResult> => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const hostAlias = draft.alias || draft.hostName;
      setHostBusy((current) => ({ ...current, [hostAlias]: "bootstrap" }));
      setHosts((current) => current.map((host) => (host.hostAlias === hostAlias ? { ...host, status: "testing" } : host)));

      try {
        const result = await api.connectSshHost(draft, password, requestId, onProgress);
        setTasks((current) => mergeTaskRunsForUi([normalizeTaskRunForUi(result.task)], current));
        if (result.ok) setNotice(result.message);
        else setErrorNotice(result.message, result.task.id);
        if (!result.ok) {
          if (result.writeResult.action === "rolled_back") {
            await refreshSshState();
          }
          setHosts((current) =>
            current.map((host) =>
              host.hostAlias === result.hostAlias
                ? {
                    ...host,
                    status: "offline",
                    latencyMs: null
                  }
                : host
            )
          );
          return result;
        }
        await refreshSshState();
        setHosts((current) =>
          current.map((host) =>
            host.hostAlias === result.hostAlias
              ? {
                  ...host,
                  status: "online",
                  latencyMs: result.latencyMs,
                  lastSeen: copy.common.justNow
                }
              : host
          )
        );
        return result;
      } finally {
        setHostBusy((current) => {
          const next = { ...current };
          delete next[hostAlias];
          return next;
        });
      }
    });
  };

  const handleDeleteSshConfigHost = async (alias: string) => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const result = await api.deleteSshConfigHost(alias);
      await refreshSshState();
      setTasks((current) => mergeTaskRunsForUi([normalizeTaskRunForUi(result.task)], current));
      setNotice(result.backupPath ? `${result.message} Backup: ${result.backupPath}` : result.message);
      return result;
    });
  };

  const handleGenerateEd25519Key = async () => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      setSshBusy(true);
      try {
        const result = await api.generateEd25519Key();
        setSshStatus(result.status);
        setNotice(result.message);
        return result;
      } catch (error) {
        setErrorNotice(formatError(error), taskIdForError(error));
        throw error;
      } finally {
        setSshBusy(false);
      }
    });
  };

  const handleCopyPublicKey = async (publicKey: string) => {
    try {
      await navigator.clipboard.writeText(publicKey);
      setNotice(copy.notices.publicKeyCopied);
      return true;
    } catch {
      notify({ message: copy.notices.copyFailed, tone: "warning" });
      return false;
    }
  };

  const handleTestHost = async (
    idOrAlias: string,
    signalSection: SectionId | null = activeSection,
    options: HostProbeOptions = {}
  ) => {
    const {
      includeLatestVersion = true,
      notifyCompletion = true,
      refreshCatalog = true
    } = options;
    const run = async () => {
      const blockedStore = storageHealth.find((item) =>
        (item.store === "profiles" || item.store === "hosts")
        && item.state !== "current"
        && item.state !== "missing"
      );
      if (blockedStore) {
        throw new Error(storageWriteBlockerError(blockedStore));
      }
      const target = hosts.find((host) => host.id === idOrAlias || host.hostAlias === idOrAlias);
      const hostAlias = target?.hostAlias ?? idOrAlias;
      setHostBusy((current) => ({ ...current, [hostAlias]: "test" }));
      setHosts((current) => current.map((host) => (host.hostAlias === hostAlias ? { ...host, status: "testing" } : host)));

      try {
        const probe = api.remoteProbeCodex(hostAlias);
        const result = includeLatestVersion
          ? (await Promise.all([
              probe,
              refreshLatestCodex(true).catch((error): LatestCodexVersion => {
                const latest = {
                  version: null,
                  checkedAt: null,
                  source: "npm",
                  error: formatError(error)
                };
                setLatestCodexVersion(latest);
                return latest;
              })
            ]))[0]
          : await probe;
        setHosts((current) =>
          current.map((host) =>
            host.hostAlias === result.hostAlias
              ? {
                  ...host,
                  status: result.sshStatus,
                  os: result.os,
                  arch: result.arch,
                  shell: result.shell,
                  path: result.path,
                  pathHasLocalBin: result.pathHasLocalBin,
                  codexCommandAvailable: result.codexCommandAvailable,
                  codexInstalled: result.codexInstalled,
                  codexVersion: result.codexVersion,
                  configExists: result.configExists,
                  apiConfigName: result.apiConfigName,
                  apiConfigSource: result.apiConfigSource,
                  apiKeyEnvVar: result.apiKeyEnvVar,
                  apiKeyEnvPresent: result.apiKeyEnvPresent,
                  skillsExists: result.skillsExists,
                  skillsCount: result.skillsCount,
                  latencyMs: result.latencyMs,
                  lastSeen: result.sshStatus === "online" ? copy.common.justNow : host.lastSeen
                }
              : host
          )
        );
        if (refreshCatalog) {
          const [refreshedHosts, refreshedProfiles] = await Promise.all([api.listHosts(), api.listProfiles()]);
          setHosts(refreshedHosts);
          setProfiles(refreshedProfiles);
        }
        setTasks((current) => mergeTaskRunsForUi([normalizeTaskRunForUi(result.task)], current));
        if (notifyCompletion) {
          const message = `${target?.name ?? hostAlias}: ${localizeTaskSummary(result.task, copy)}`;
          if (result.task.status === "success") setNotice(message);
          else setErrorNotice(message, result.task.id);
        }
        return { ...result, ok: result.sshStatus === "online" && result.task.status === "success" };
      } finally {
        setHostBusy((current) => {
          const next = { ...current };
          delete next[hostAlias];
          return next;
        });
      }
    };

    return signalSection ? runSectionOperation(signalSection, run) : run();
  };

  const handleTestAllSshHosts = async () => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const blockedStore = storageHealth.find((item) =>
        (item.store === "profiles" || item.store === "hosts")
        && item.state !== "current"
        && item.state !== "missing"
      );
      if (blockedStore) {
        throw new Error(storageWriteBlockerError(blockedStore));
      }
      const latestCodexProbe = refreshLatestCodex(true).catch((error): LatestCodexVersion => {
        const latest = {
          version: null,
          checkedAt: null,
          source: "npm",
          error: formatError(error)
        };
        setLatestCodexVersion(latest);
        return latest;
      });
      const [, results] = await Promise.all([
        latestCodexProbe,
        Promise.allSettled(
          sshConfigHosts.map((host) => handleTestHost(host.alias, null, {
            includeLatestVersion: false,
            notifyCompletion: false,
            refreshCatalog: false
          }))
        )
      ]);
      const [refreshedHosts, refreshedProfiles] = await Promise.all([api.listHosts(), api.listProfiles()]);
      setHosts(refreshedHosts);
      setProfiles(refreshedProfiles);
      const successful = results.filter((result) => result.status === "fulfilled" && result.value.ok).length;
      const ok = successful === results.length;
      if (ok) setNotice(copy.hosts.testedAllResult(successful, results.length));
      else setErrorNotice(copy.hosts.testedAllFailed(successful, results.length));
      return { ok };
    });
  };

  const applyRemoteCodexResult = (result: RemoteCodexMaintenanceResult, action: RemoteCodexAction) => {
    const resolvedVersion = action === "uninstall" ? result.afterVersion : result.afterVersion ?? result.beforeVersion;
    const nextVersion = resolvedVersion ?? "not installed";
    const sshCheckFailed = result.message.toLowerCase().includes("ssh check failed");
    setHosts((current) =>
      current.map((host) =>
        host.hostAlias === result.hostAlias
          ? {
              ...host,
              status: sshCheckFailed ? "offline" : "online",
              codexInstalled: Boolean(resolvedVersion),
              codexVersion: nextVersion,
              pathHasLocalBin: action === "check-version" ? host.pathHasLocalBin : result.ok || result.pathChanged ? true : host.pathHasLocalBin,
              codexCommandAvailable: result.codexCommandAvailable,
              lastSeen: sshCheckFailed ? host.lastSeen : copy.common.justNow
            }
          : host
      )
    );
    setTasks((current) => mergeTaskRunsForUi([normalizeTaskRunForUi(result.task)], current));
  };

  const runRemoteCodexAction = async (idOrAlias: string, action: RemoteCodexAction) => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const target = hosts.find((host) => host.id === idOrAlias || host.hostAlias === idOrAlias);
      const hostAlias = target?.hostAlias ?? idOrAlias;
      const hostName = target?.name ?? hostAlias;
      const showProgressModal = action === "install" || action === "update" || action === "uninstall";
      const requestId = showProgressModal ? `codex-${Date.now()}-${Math.random().toString(36).slice(2)}` : undefined;
      setHostBusy((current) => ({ ...current, [hostAlias]: action }));
      setHosts((current) => current.map((host) => (host.hostAlias === hostAlias ? { ...host, status: "testing" } : host)));
      if (showProgressModal) {
        setCodexOperationModal({
          hostAlias,
          hostName,
          action,
          status: "running",
          logs: []
        });
        await waitForNextFrame();
      }

      try {
        const result = await api.remoteManageCodex(hostAlias, action, 120000, requestId, (event) => {
          setCodexOperationModal((current) => {
            if (!current || current.hostAlias !== event.hostAlias || current.action !== event.action) return current;
            return {
              ...current,
              message: event.step === "summary" ? event.message : current.message,
              logs: [...current.logs, event].slice(-80)
            };
          });
        });
        applyRemoteCodexResult(result, action);
        if (result.ok) setNotice(`${hostName}: ${result.message}`);
        else setErrorNotice(`${hostName}: ${result.message}`, result.task.id);
        void refreshLatestCodex(true);
        if (showProgressModal) {
          setCodexOperationModal((current) =>
            current && current.hostAlias === hostAlias && current.action === action
              ? {
                  ...current,
                  status: result.ok ? "success" : "failed",
                  task: result.task,
                  message: result.message
                }
              : current
          );
        }
        return result;
      } catch (error) {
        const errorMessage = formatError(error);
        setErrorNotice(`${hostName}: ${errorMessage}`, taskIdForError(error));
        if (showProgressModal) {
          setCodexOperationModal((current) =>
            current && current.hostAlias === hostAlias && current.action === action
              ? {
                  ...current,
                  status: "failed",
                  error: errorMessage
                }
              : current
          );
        }
        setHosts((current) => current.map((host) => (host.hostAlias === hostAlias ? { ...host, status: "offline" } : host)));
        return { ok: false, message: errorMessage };
      } finally {
        setHostBusy((current) => {
          const next = { ...current };
          delete next[hostAlias];
          return next;
        });
      }
    });
  };

  const handleRemoteCodexAction = async (idOrAlias: string, action: RemoteCodexAction) => {
    const target = hosts.find((host) => host.id === idOrAlias || host.hostAlias === idOrAlias);
    const hostAlias = target?.hostAlias ?? idOrAlias;
    if (action === "uninstall") {
      setCodexUninstallConfirm({
        hostAlias,
        hostName: target?.name ?? hostAlias
      });
      return { ok: true };
    }
    return runRemoteCodexAction(hostAlias, action);
  };

  const handleUpdateOutdatedCodexHosts = async (aliases: string[]) => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const uniqueAliases = Array.from(new Set(aliases.filter(Boolean)));
      if (uniqueAliases.length === 0) {
        setNotice(copy.hosts.noOutdatedCodex);
        return { ok: true };
      }
      setHostBusy((current) => {
        const next = { ...current };
        for (const alias of uniqueAliases) next[alias] = "update";
        return next;
      });
      setHosts((current) => current.map((host) => (uniqueAliases.includes(host.hostAlias) ? { ...host, status: "testing" } : host)));
      await waitForNextFrame();

      try {
        const results = await Promise.allSettled(uniqueAliases.map((alias) => api.remoteManageCodex(alias, "update", 120000)));
        const fulfilled = results.flatMap((result) => (result.status === "fulfilled" ? [result.value] : []));
        const rejectedAliases = results.flatMap((result, index) => (result.status === "rejected" ? [uniqueAliases[index]] : []));
        for (const result of fulfilled) applyRemoteCodexResult(result, "update");
        if (rejectedAliases.length > 0) {
          setHosts((current) => current.map((host) => (rejectedAliases.includes(host.hostAlias) ? { ...host, status: "offline" } : host)));
        }
        const successCount = fulfilled.filter((result) => result.ok).length;
        const message = copy.hosts.updatedOutdatedCodex(successCount, uniqueAliases.length);
        if (successCount === uniqueAliases.length) setNotice(message);
        else setErrorNotice(message);
        void refreshLatestCodex(true);
        return { ok: successCount === uniqueAliases.length };
      } finally {
        setHostBusy((current) => {
          const next = { ...current };
          for (const alias of uniqueAliases) delete next[alias];
          return next;
        });
      }
    });
  };

  const replaceProfile = (profile: Profile) => {
    setProfiles((current) => current.map((item) => (item.id === profile.id ? profile : item)));
  };

  const handleCreateProfile = async (draft: ProfileDraft) => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const profile = await api.createProfile(draft);
      setProfiles((current) => [...current, profile]);
      setNotice(`${profile.name}: ${copy.profiles.create}`);
      return profile;
    });
  };

  const handleUpdateProfile = async (id: string, patch: ProfilePatch) => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const profile = await api.updateProfile(id, patch);
      replaceProfile(profile);
      setNotice(`${profile.name}: ${copy.profiles.save}`);
      return profile;
    });
  };

  const handleDeleteProfile = async (id: string) => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const profile = profiles.find((item) => item.id === id);
      const result = await api.deleteProfile(id);
      setTasks((current) => mergeTaskRunsForUi([normalizeTaskRunForUi(result.task)], current));
      if (!result.deleted) {
        setNotice(result.message);
        return result;
      }
      setProfiles((current) => current.filter((item) => item.id !== id));
      setHosts((current) => current.map((host) => (host.profileId === id ? { ...host, profileId: null, apiConfigName: null, apiConfigSource: null } : host)));
      setNotice(profile ? `${profile.name}: ${copy.profiles.delete}` : copy.profiles.delete);
      return result;
    });
  };

  const handleDuplicateProfile = async (id: string) => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const profile = await api.duplicateProfile(id);
      setProfiles((current) => [...current, profile]);
      setNotice(`${profile.name}: ${copy.profiles.duplicate}`);
      return profile;
    });
  };

  const handleImportProfiles = async (bundle: ProfileImportExport) => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const result = await api.importProfiles(bundle);
      setProfiles((current) => [...current, ...result.profiles]);
      setNotice(copy.profiles.importReady(result.profiles.length));
      return result;
    });
  };

  const refreshSkills = async () => {
    const [nextSkills, nextStatus] = await Promise.all([api.listSkillPacks(), api.getSkillInventoryStatus()]);
    setSkillPacks(nextSkills);
    setSkillInventoryStatus(nextStatus);
    return nextSkills;
  };

  const handleRefreshSkillLibrary = async () => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const [nextSkills, nextStatus, nextHosts] = await Promise.all([
        api.listSkillPacks(),
        api.getSkillInventoryStatus(),
        api.listHosts()
      ]);
      setSkillPacks(nextSkills);
      setSkillInventoryStatus(nextStatus);
      setHosts(nextHosts);
      setNotice(copy.skills.refreshed);
      return { skills: nextSkills, status: nextStatus, hosts: nextHosts };
    });
  };

  const applySkillDetectionResult = async (result: SkillDetectionResult) => {
    setSkillPacks(result.skills);
    setSkillInventoryStatus(result.status);
    if (result.tasks.length > 0) setTasks((current) => mergeTaskRunsForUi(normalizeTaskRunsForUi(result.tasks), current));
    const nextHosts = await api.listHosts();
    setHosts(nextHosts);
    setNotice(result.message);
    return result;
  };

  const applySkillOperationResult = async (result: SkillTargetOperationResult) => {
    setSkillPacks(result.skills);
    if (result.tasks.length > 0) setTasks((current) => mergeTaskRunsForUi(normalizeTaskRunsForUi(result.tasks), current));
    const nextHosts = await api.listHosts();
    setHosts(nextHosts);
    const nextStatus = await api.getSkillInventoryStatus();
    setSkillInventoryStatus(nextStatus);
    const message = result.message === "install-success"
        ? copy.skills.installSuccess
        : result.message === "install-partial-failure"
          ? copy.skills.installPartialFailure
          : result.message === "uninstall-success"
            ? copy.skills.uninstallSuccess
            : result.message === "uninstall-partial-failure"
              ? copy.skills.uninstallPartialFailure
              : result.message || copy.skills.operationDone;
    if (result.ok) setNotice(message);
    else setErrorNotice(message);
    return result;
  };

  const handleDetectInstalledSkills = async (includeHosts: boolean) => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const result = await api.detectInstalledSkills(includeHosts);
      return applySkillDetectionResult(result);
    });
  };

  const handleImportSkillDirectory = async () => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const selected = await open({ directory: true, multiple: false, title: copy.skills.importDirectory });
      const path = Array.isArray(selected) ? selected[0] : selected;
      if (!path) return null;
      const result = await api.importLocalSkill(path);
      await refreshSkills();
      setNotice(result.message || copy.skills.imported(result.imported.length));
      return result;
    }, { classify: (result) => (result ? sectionOperationTone(result) : null) });
  };

  const handleDownloadGithubSkill = async (repoUrl: string) => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const result = await api.downloadGithubSkill(repoUrl);
      await refreshSkills();
      setNotice(result.message || copy.skills.imported(result.imported.length));
      return result;
    });
  };

  const handleGetSkillTargets = async (skillId: string) => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const result = await api.getSkillTargets(skillId);
      if (result.tasks.length > 0) setTasks((current) => mergeTaskRunsForUi(normalizeTaskRunsForUi(result.tasks), current));
      return result;
    });
  };

  const handleInstallSkillTargets = async (skillId: string, targets: SkillTargetRequest[]) => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const result = await api.installSkillTargets(skillId, targets);
      return applySkillOperationResult(result);
    });
  };

  const handleUninstallSkillTargets = async (skillId: string, targets: SkillTargetRequest[]) => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const result = await api.uninstallSkillTargets(skillId, targets);
      return applySkillOperationResult(result);
    });
  };

  const handleDeleteLibrarySkill = async (skillId: string, uninstallFirst: boolean) => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const result = await api.deleteLibrarySkill(skillId, uninstallFirst);
      return applySkillOperationResult(result);
    });
  };

  const applyInstalledSkillDownloadResult = async (result: InstalledSkillDownloadResult) => {
    setSkillPacks(result.skills);
    setSkillInventoryStatus(result.status);
    if (result.tasks.length > 0) setTasks((current) => mergeTaskRunsForUi(normalizeTaskRunsForUi(result.tasks), current));
    const nextHosts = await api.listHosts();
    setHosts(nextHosts);
    setNotice(result.message || copy.skills.downloaded);
    return result;
  };

  const handleDownloadInstalledSkill = async (request: InstalledSkillRequest) => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const result = await api.downloadInstalledSkill(request);
      return applyInstalledSkillDownloadResult(result);
    });
  };

  const handleUninstallInstalledSkill = async (request: InstalledSkillRequest) => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const result = await api.uninstallInstalledSkill(request);
      return applySkillOperationResult(result);
    });
  };

  const handleUpdateLibrarySkillAbout = async (skillId: string, about: string) => {
    const nextSkills = await api.updateLibrarySkillAbout(skillId, about);
    setSkillPacks(nextSkills);
    const updated = nextSkills.find((skill) => skill.id === skillId);
    setNotice(updated ? `${updated.name}: ${copy.skills.save}` : copy.skills.save);
    return updated ?? null;
  };

  const handleSetProfileApiKey = async (profileId: string, apiKey: string) => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const profile = await api.setProfileApiKey(profileId, apiKey);
      replaceProfile(profile);
      setNotice(`${profile.name}: ${copy.profiles.credentialStored}`);
      return profile;
    });
  };

  const handleGetProfileApiKey = useCallback(async (profileId: string) => {
    const result = await api.getProfileApiKey(profileId);
    if (result.exists) {
      setProfiles((current) => {
        let changed = false;
        const next = current.map((profile) => {
          if (profile.id !== result.profileId || profile.credentialStored) return profile;
          changed = true;
          return { ...profile, credentialStored: true };
        });
        return changed ? next : current;
      });
    }
    return result;
  }, []);

  const handlePreviewProfileApply = (profileId: string, hostIds: string[]) => {
    const section = activeSection;
    return runSectionOperation(section, () => api.previewProfileApply(profileId, hostIds));
  };

  const handleApplyProfile = async (profileId: string, hostIds: string[]) => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const result = await api.applyProfile(profileId, hostIds);
      if (result.tasks.length > 0) setTasks((current) => mergeTaskRunsForUi(normalizeTaskRunsForUi(result.tasks), current));
      const profileName =
        result.profiles.find((profile) => profile.id === profileId)?.name ??
        profiles.find((profile) => profile.id === profileId)?.name ??
        profileId;
      if (result.profiles.length > 0 || result.hosts.length > 0) {
        if (result.profiles.length > 0) setProfiles(result.profiles);
        if (result.hosts.length > 0) setHosts(result.hosts);
        setNotice(`${profileName}: ${copy.profiles.applySelected}`);
        void refreshLatestCodex(true);
        return result;
      }
      const appliedAt = copy.common.justNow;
      const normalizeHostKey = (value: string | null | undefined) => value?.trim().toLowerCase() ?? "";
      const appliedHostKeys = new Set<string>();
      const successfulResults = result.results.filter((entry) => entry.status === "success" || entry.status === "no-change");
      for (const item of successfulResults) {
        for (const key of [item.hostId, item.hostAlias]) {
          const normalizedKey = normalizeHostKey(key);
          if (normalizedKey) appliedHostKeys.add(normalizedKey);
        }
      }
      const appliedHostIds = new Set<string>();
      for (const host of hosts) {
        if (appliedHostKeys.has(normalizeHostKey(host.id)) || appliedHostKeys.has(normalizeHostKey(host.hostAlias))) {
          appliedHostIds.add(host.id);
        }
      }
      for (const item of successfulResults) {
        const resultKeys = [normalizeHostKey(item.hostId), normalizeHostKey(item.hostAlias)].filter(Boolean);
        const hasCurrentHostMatch = hosts.some(
          (host) => resultKeys.includes(normalizeHostKey(host.id)) || resultKeys.includes(normalizeHostKey(host.hostAlias))
        );
        if (!hasCurrentHostMatch && item.hostId) {
          appliedHostIds.add(item.hostId);
        }
      }
      setProfiles((current) =>
        current.map((profile) => {
          const hostIdsWithoutApplied = profile.hostIds.filter(
            (hostId) => !appliedHostIds.has(hostId) && !appliedHostKeys.has(normalizeHostKey(hostId))
          );
          if (profile.id !== profileId) {
            return hostIdsWithoutApplied.length === profile.hostIds.length ? profile : { ...profile, hostIds: hostIdsWithoutApplied };
          }
          const nextHostIds = Array.from(new Set([...hostIdsWithoutApplied, ...appliedHostIds]));
          return nextHostIds.length === profile.hostIds.length && nextHostIds.every((hostId, index) => hostId === profile.hostIds[index])
            ? profile
            : { ...profile, hostIds: nextHostIds };
        })
      );
      setHosts((current) =>
        current.map((host) =>
          appliedHostIds.has(host.id) ||
          appliedHostKeys.has(normalizeHostKey(host.id)) ||
          appliedHostKeys.has(normalizeHostKey(host.hostAlias))
            ? {
                ...host,
                profileId,
                apiConfigName: profileName,
                apiConfigSource: "profile",
                profileAppliedAt: appliedAt,
                profileAppliedSource: "CodexHub"
              }
            : host
        )
      );
      setNotice(`${profileName}: ${copy.profiles.applySelected}`);
      void refreshLatestCodex(true);
      return result;
    });
  };

  const handleDetectCcSwitchProfiles = async () => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const detection = await api.detectCcSwitchProfiles();
      setNotice(
        detection.detected ? copy.profiles.ccSwitchFound(detection.importExport.profiles.length) : copy.profiles.ccSwitchNone
      );
      return detection;
    });
  };

  const handleImportCcSwitchProfiles = async (detection: CcSwitchDetection) => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      const result = await api.importCcSwitchProfiles(detection);
      const nextProfiles = await api.listProfiles();
      setProfiles(nextProfiles);
      setNotice(copy.profiles.importReady(result.profiles.length));
      return result;
    });
  };

  const persistSettings = async (nextSettings: AppSettings): Promise<boolean> => {
    const normalized = normalizeSettings(nextSettings);
    if (settingsSaveBusyRef.current) {
      setPendingSettings(normalized);
      return false;
    }

    settingsSaveBusyRef.current = true;
    setSettingsSaving(true);
    setSettingsSaveError(null);
    setPendingSettings(normalized);
    try {
      const result = await api.saveSettings(normalized);
      setSettings(result.settings);
      applyAppSettings(result.settings);
      setPendingSettings(null);
      setNotice(copy.settings.settingsSaved);
      return true;
    } catch (error) {
      const detail = formatError(error);
      setSettingsSaveError(detail);
      setErrorNotice(copy.settings.settingsSaveFailed, taskIdForError(error));
      return false;
    } finally {
      settingsSaveBusyRef.current = false;
      setSettingsSaving(false);
    }
  };

  const retrySettingsSave = async () => pendingSettings ? persistSettings(pendingSettings) : true;

  const handleChooseCloseButtonBehavior = async (behavior: Exclude<CloseButtonBehavior, "ask">) => {
    setCloseButtonPromptBusy(true);
    setCloseButtonPromptOpen(false);
    try {
      const result = await api.chooseCloseButtonBehavior(behavior);
      const nextSettings = result.settings;
      setSettings(nextSettings);
      applyAppSettings(nextSettings);
    } catch (error) {
      setCloseButtonPromptOpen(true);
      setErrorNotice(formatError(error), taskIdForError(error));
    } finally {
      setCloseButtonPromptBusy(false);
    }
  };

  const handleTitleBarCloseRequest = useCallback(async () => {
    if (settings.closeButtonBehavior === "ask") {
      setCloseButtonPromptOpen(true);
      return;
    }

    try {
      const currentWindow = getCurrentWindow();
      if (settings.closeButtonBehavior === "minimize-to-tray") {
        await currentWindow.hide();
        return;
      }
      await currentWindow.close();
    } catch {
      console.debug("Desktop close behavior is unavailable in this preview.");
    }
  }, [settings.closeButtonBehavior]);

  const ensureSetupGuideEd25519Key = async (detectedStatus?: SshStatus | null) => {
    const status = detectedStatus ?? sshStatus ?? await api.getSshStatus();
    setSshStatus(status);
    if (status.ed25519.privateExists) return status;
    const result = await api.generateEd25519Key();
    setSshStatus(result.status);
    return result.status;
  };

  const handleDismissSetupGuide = async () => {
    setSetupGuideBusy(true);
    try {
      await ensureSetupGuideEd25519Key().catch((error) => {
        setErrorNotice(formatError(error), taskIdForError(error));
      });
      if (await persistSettings({ ...settings, setupGuideDismissed: true })) {
        setSetupGuideOpen(false);
      }
    } finally {
      setSetupGuideBusy(false);
    }
  };

  const handleSetupGuidePreferencesNext = async (preferences: Pick<AppSettings, "theme" | "platformAppearance" | "fontPreset">) => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      if (!(await persistSettings({ ...settings, ...preferences }))) return null;
      setSetupGuideStep("ssh");
      setSetupGuideBusy(true);
      try {
        await waitForNextFrame();
        return await refreshSshDetectionState();
      } finally {
        setSetupGuideBusy(false);
      }
    });
  };

  const handleOpenSetupGuide = async () => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      setSetupGuideBusy(true);
      try {
        setSetupGuideStep("ssh");
        setSetupGuideOpen(true);
        await waitForNextFrame();
        return await refreshSshDetectionState();
      } finally {
        setSetupGuideBusy(false);
      }
    });
  };

  const handleImportLocalSshConfig = async () => {
    const section = activeSection;
    return runSectionOperation(section, async () => {
      setSetupGuideBusy(true);
      try {
        const refreshed = await importLocalSshConfig();
        let keyError: string | null = null;
        await ensureSetupGuideEd25519Key(refreshed.sshStatus).catch((error) => {
          keyError = formatError(error);
        });
        if (!(await persistSettings({ ...settings, setupGuideDismissed: true }))) return refreshed;
        setSetupGuideOpen(false);
        if (keyError) {
          setErrorNotice(keyError);
          return { ...refreshed, ok: false };
        }
        setNotice(copy.setupGuide.imported(refreshed.detectedSshConfigHosts.length));
        return refreshed;
      } finally {
        setSetupGuideBusy(false);
      }
    });
  };

  const renderContent = () => {
    switch (activeSection) {
      case "dashboard":
        return (
          <DashboardView
            copy={copy}
            hostBusy={hostBusy}
            hosts={hosts}
            latestCodexVersion={latestCodexVersion}
            onlineCount={onlineCount}
            appliedProfileCount={appliedProfileCount}
            inventoryStatus={skillInventoryStatus}
            profiles={profiles}
            sshConfigHosts={sshConfigHosts}
            skillPacks={skillPacks}
            tasks={tasks}
            successfulTaskCount={successfulTaskCount}
            profileById={profileById}
            onAddServer={handleAddHost}
            onTestAllSshHosts={() => handleTestAllSshHosts().catch((error) => {
              setErrorNotice(formatError(error), taskIdForError(error));
              return { ok: false };
            })}
          />
        );
      case "hosts":
        return (
          <HostsView
            copy={copy}
            hosts={hosts}
            hostBusy={hostBusy}
            inventoryStatus={skillInventoryStatus}
            latestCodexVersion={latestCodexVersion}
            sshConfigHosts={sshConfigHosts}
            sshStatus={sshStatus}
            addHostOpen={hostModalOpen}
            sshBusy={sshBusy}
            onCloseAddHost={() => setHostModalOpen(false)}
            onConnectSshHost={handleConnectSshHost}
            onDeleteSshConfigHost={handleDeleteSshConfigHost}
            onDetectLocalSshHosts={handleDetectLocalSshHosts}
            onGenerateEd25519Key={handleGenerateEd25519Key}
            onManageCodex={handleRemoteCodexAction}
            onOpenAddHost={handleAddHost}
            onOpenSetupGuide={handleOpenSetupGuide}
            onTestAllSshHosts={() => handleTestAllSshHosts().catch((error) => {
              setErrorNotice(formatError(error), taskIdForError(error));
              return { ok: false };
            })}
            onTestHost={(id) => {
              void handleTestHost(id).catch((error) => setErrorNotice(formatError(error), taskIdForError(error)));
            }}
            onUpdateOutdatedCodexHosts={handleUpdateOutdatedCodexHosts}
          />
        );
      case "profiles":
        return (
          <ProfilesView
            copy={copy}
            hosts={hosts}
            latestCodexVersion={latestCodexVersion}
            profiles={profiles}
            newProfileRequest={newProfileRequest}
            onCreateProfile={handleCreateProfile}
            onDeleteProfile={handleDeleteProfile}
            onDetectCcSwitchProfiles={handleDetectCcSwitchProfiles}
            onDuplicateProfile={handleDuplicateProfile}
            onGetProfileApiKey={handleGetProfileApiKey}
            onImportCcSwitchProfiles={handleImportCcSwitchProfiles}
            onImportProfiles={handleImportProfiles}
            onPreviewProfileApply={handlePreviewProfileApply}
            onRunProfileApply={handleApplyProfile}
            onSetProfileApiKey={handleSetProfileApiKey}
            onUpdateProfile={handleUpdateProfile}
          />
        );
      case "skills":
        return (
          <SkillsView
            copy={copy}
            hosts={hosts}
            inventoryStatus={skillInventoryStatus}
            skillPacks={skillPacks}
            onDeleteLibrarySkill={handleDeleteLibrarySkill}
            onDetectInstalledSkills={handleDetectInstalledSkills}
            onDownloadInstalledSkill={handleDownloadInstalledSkill}
            onDownloadGithubSkill={handleDownloadGithubSkill}
            onGetSkillTargets={handleGetSkillTargets}
            onImportSkillDirectory={handleImportSkillDirectory}
            onInstallSkillTargets={handleInstallSkillTargets}
            onRefreshSkillLibrary={handleRefreshSkillLibrary}
            onUninstallInstalledSkill={handleUninstallInstalledSkill}
            onUninstallSkillTargets={handleUninstallSkillTargets}
            onUpdateLibrarySkillAbout={handleUpdateLibrarySkillAbout}
            onViewTasks={() => setActiveSection("tasks")}
          />
        );
      case "monitor":
        return (
          <MonitorView
            autoRefresh={settings.resourceMonitorAutoRefresh}
            busy={resourceBusy}
            checkedAt={resourceCheckedAt}
            copy={copy}
            error={resourceError}
            hosts={hosts}
            hostOrder={settings.resourceMonitorHostOrder}
            refreshSeconds={settings.resourceMonitorRefreshSeconds}
            settingsSaving={settingsSaving}
            snapshots={resourceSnapshots}
            onAutoRefreshChange={(resourceMonitorAutoRefresh) => persistSettings({ ...settings, resourceMonitorAutoRefresh })}
            onHostOrderChange={(resourceMonitorHostOrder) => persistSettings({ ...settings, resourceMonitorHostOrder })}
            onRefresh={() => refreshResourceMonitor("manual")}
            onRefreshSecondsChange={(resourceMonitorRefreshSeconds) =>
              persistSettings({
                ...settings,
                resourceMonitorRefreshSeconds: normalizeResourceMonitorRefreshSeconds(resourceMonitorRefreshSeconds)
              })
            }
          />
        );
      case "tasks":
        return (
          <TasksView
            copy={copy}
            hasMore={Boolean(taskNextCursor)}
            loadingMore={taskLoadingMore}
            mockMode={apiMode === "mock"}
            requestedTaskId={requestedTaskId}
            tasks={tasks}
            onClearTaskHistory={clearTaskHistory}
            onLoadMore={loadMoreTasks}
            onRequestHandled={() => setRequestedTaskId(null)}
            onTaskViewed={openTaskDetail}
          />
        );
      case "settings":
        return (
          <SettingsView
            appUpdateChecking={appUpdateChecking}
            appUpdateInstalling={appUpdateInstalling}
            appUpdateStatus={appUpdateStatus}
            copy={copy}
            settings={settings}
            settingsSaveError={settingsSaveError}
            settingsSaving={settingsSaving}
            sshStatus={sshStatus}
            onCheckStableUpdate={handleCheckStableUpdate}
            onInstallStableUpdate={handleInstallStableUpdate}
            onCloseButtonBehaviorChange={(closeButtonBehavior) => persistSettings({ ...settings, closeButtonBehavior })}
            onCopyPublicKey={handleCopyPublicKey}
            onFontPresetChange={(fontPreset) => persistSettings({ ...settings, fontPreset })}
            onNetworkProxyModeChange={(networkProxyMode) => persistSettings({ ...settings, networkProxyMode })}
            onNetworkProxyManualRequest={() => setNetworkProxyManualOpen(true)}
            onPlatformAppearanceChange={(platformAppearance) => persistSettings({ ...settings, platformAppearance })}
            onRetrySettings={retrySettingsSave}
            onRefreshSsh={async () => {
              await runSectionOperation("settings", refreshSshState);
            }}
            onSidebarCompletionIndicatorsChange={(sidebarCompletionIndicators) => persistSettings({ ...settings, sidebarCompletionIndicators })}
            onThemeChange={(theme) => persistSettings({ ...settings, theme })}
          />
        );
      default:
        return null;
    }
  };
  const pageActions: CommandBarAction[] = [
    ...(activeSection === "hosts"
      ? [{ id: "add-host", label: copy.common.addServer, kind: "primary" as const, onClick: handleAddHost }]
      : []),
    ...(activeSection === "profiles"
      ? [{
          id: "new-api-config",
          label: copy.profiles.newApiConfig,
          kind: "primary" as const,
          onClick: handleNewProfile
        }]
      : [])
  ];

  return (
    <PlatformAppearanceContext.Provider value={effectivePlatform}>
      <div className="desktopFrame" data-os={runtimePlatform} data-custom-titlebar={usesCustomTitleBar}>
        {usesCustomTitleBar ? <AppTitleBar copy={copy} onCloseRequest={handleTitleBarCloseRequest} /> : null}
        <div className="appShell">
      <aside className="sidebar" aria-label={copy.common.primaryNavigation}>
        <div className="brandBlock">
          <img className="appIcon" src={appLogoUrl} alt="" aria-hidden="true" />
          <div>
            <div className="brandName">CodexHub</div>
          </div>
        </div>

        <nav className="navList">
          {copy.navItems.map((item) => {
            const completionTone = sectionCompletionSignals[item.id];
            return (
              <button
                aria-label={completionTone ? `${item.label}: ${copy.status.task[completionTone === "error" ? "failed" : "success"]}` : item.label}
                className="navItem"
                data-active={activeSection === item.id}
                key={item.id}
                onClick={() => {
                  clearSectionCompletionSignal(item.id);
                  setActiveSection(item.id);
                }}
                type="button"
              >
                <span className="navIcon" aria-hidden="true"><NavIcon id={item.id} /></span>
                <span className="navLabel">{item.label}</span>
                {completionTone ? <span className="navCompletionDot" data-tone={completionTone} aria-hidden="true" /> : null}
              </button>
            );
          })}
        </nav>

        <div className="sidebarFooter">
          <span className="statusDot" data-status={health.mode === "tauri" ? "online" : "unknown"} />
          <div>
            <strong>{apiMode === "mock" ? copy.common.mockMode : copy.common.backendMode}</strong>
          </div>
          {showSidebarStableUpdateButton ? (
            <button
              className="sidebarUpdateButton"
              disabled={!canInstallSidebarStableUpdate}
              type="button"
              onClick={handleSidebarStableUpdate}
            >
              {appUpdateInstalling ? copy.settings.updateInstalling : copy.settings.sidebarInstallStableUpdate}
            </button>
          ) : null}
        </div>
      </aside>

      <main
        className="contentShell"
        onInputCapture={handleContentInteraction}
        onKeyDownCapture={handleContentInteraction}
        onPointerDownCapture={handleContentInteraction}
        onScrollCapture={handleContentInteraction}
        onWheelCapture={handleContentInteraction}
      >
        <StorageHealthCenter copy={copy} health={storageHealth} onChanged={refreshStorageHealth} />
        {activeSection !== "monitor" ? (
          <header className="topBar">
            <div>
              <TitleWithIcon icon={activeSection} level={1}>{selectedCopy.title}</TitleWithIcon>
            </div>
            {pageActions.length > 0 ? <CommandBarActions ariaLabel={selectedCopy.title} className="topActions" actions={pageActions} /> : null}
          </header>
        ) : null}

        {bootstrapError ? (
          <section className="panel" role="alert">
            <TitleWithIcon icon="warning" level={2}>{copy.common.backendUnavailableTitle}</TitleWithIcon>
            <p>{copy.common.backendUnavailableBody}</p>
            <p className="monoText">{bootstrapError}</p>
          </section>
        ) : renderContent()}
      </main>
      {codexUninstallConfirm ? (
        <CodexUninstallConfirmModal
          copy={copy}
          target={codexUninstallConfirm}
          onCancel={() => setCodexUninstallConfirm(null)}
          onConfirm={() => {
            const target = codexUninstallConfirm;
            setCodexUninstallConfirm(null);
            void runRemoteCodexAction(target.hostAlias, "uninstall");
          }}
        />
      ) : null}
      {codexOperationModal ? (
        <CodexOperationModal
          copy={copy}
          operation={codexOperationModal}
          onClose={() => setCodexOperationModal(null)}
          onViewTasks={() => {
            setActiveSection("tasks");
            setCodexOperationModal(null);
          }}
        />
      ) : null}
      {appUpdateFailureTask ? (
        <TaskLogModal
          copy={copy}
          now={Date.now()}
          task={appUpdateFailureTask}
          onClose={() => setAppUpdateFailureTask(null)}
          footer={(
            <>
              <p className="mutedText taskLogModalHint">{copy.settings.updateCheckFailureHint}</p>
              <ModalActions>
                <button
                  className="secondaryButton"
                  type="button"
                  onClick={() => {
                    setActiveSection("tasks");
                    setAppUpdateFailureTask(null);
                  }}
                >
                  {copy.codexOperation.viewTasks}
                </button>
                <button className="primaryButton" type="button" onClick={() => setAppUpdateFailureTask(null)}>
                  {copy.codexOperation.close}
                </button>
              </ModalActions>
            </>
          )}
        />
      ) : null}
      {networkProxyManualOpen ? (
        <NetworkProxyManualModal
          copy={copy}
          initialValue={settings.networkProxyUrl}
          onClose={() => setNetworkProxyManualOpen(false)}
          onSave={async (networkProxyUrl) => {
            if (await persistSettings({ ...settings, networkProxyMode: "manual", networkProxyUrl })) {
              setNetworkProxyManualOpen(false);
            }
          }}
        />
      ) : null}
      {setupGuideOpen ? (
        <SetupGuideModal
          busy={setupGuideBusy}
          copy={copy}
          currentSettings={settings}
          step={setupGuideStep}
          sshConfigHosts={setupGuideSshConfigHosts}
          sshStatus={sshStatus}
          onClose={handleDismissSetupGuide}
          onImport={handleImportLocalSshConfig}
          onPreferencesNext={handleSetupGuidePreferencesNext}
          onSkip={handleDismissSetupGuide}
        />
      ) : null}
      {closeButtonPromptOpen ? (
        <CloseButtonBehaviorPromptModal
          busy={closeButtonPromptBusy}
          copy={copy}
          onCancel={() => setCloseButtonPromptOpen(false)}
          onChoose={handleChooseCloseButtonBehavior}
        />
      ) : null}
        </div>
      </div>
    </PlatformAppearanceContext.Provider>
  );
}

function CloseButtonBehaviorPromptModal({
  busy,
  copy,
  onCancel,
  onChoose
}: {
  busy: boolean;
  copy: UICopy;
  onCancel: () => void;
  onChoose: (behavior: Exclude<CloseButtonBehavior, "ask">) => Promise<void>;
}) {
  return (
    <div className="modalBackdrop" role="presentation">
      <ModalFrame className="setupGuideModal" titleId="close-button-prompt-title">
        <ModalHeader
          titleId="close-button-prompt-title"
          title={copy.closeButtonPrompt.title}
          description={copy.closeButtonPrompt.body}
          icon="warning"
          closeAriaLabel={copy.closeButtonPrompt.cancel}
          closeDisabled={busy}
          onClose={onCancel}
        />

        <div className="setupGuideLanguage">
          <button className="setupGuideLanguageOption" disabled={busy} type="button" onClick={() => void onChoose("exit")}>
            <strong>{copy.closeButtonPrompt.exitTitle}</strong>
            <span>{copy.closeButtonPrompt.exitBody}</span>
          </button>
          <button className="setupGuideLanguageOption" disabled={busy} type="button" onClick={() => void onChoose("minimize-to-tray")}>
            <strong>{copy.closeButtonPrompt.minimizeTitle}</strong>
            <span>{copy.closeButtonPrompt.minimizeBody}</span>
          </button>
        </div>

        <ModalActions className="setupGuideActions" dataHasHosts>
          <button className="secondaryButton" disabled={busy} type="button" onClick={onCancel}>
            {copy.closeButtonPrompt.cancel}
          </button>
        </ModalActions>
      </ModalFrame>
    </div>
  );
}

function SetupGuideModal({
  busy,
  copy,
  currentSettings,
  step,
  sshConfigHosts,
  sshStatus,
  onClose,
  onImport,
  onPreferencesNext,
  onSkip
}: {
  busy: boolean;
  copy: UICopy;
  currentSettings: AppSettings;
  step: SetupGuideStep;
  sshConfigHosts: SshConfigHost[];
  sshStatus: SshStatus | null;
  onClose: () => Promise<unknown>;
  onImport: () => Promise<unknown>;
  onPreferencesNext: (preferences: Pick<AppSettings, "theme" | "platformAppearance" | "fontPreset">) => Promise<unknown>;
  onSkip: () => Promise<unknown>;
}) {
  const [preferenceDraft, setPreferenceDraft] = useState<Pick<AppSettings, "theme" | "platformAppearance" | "fontPreset">>({
    theme: currentSettings.theme,
    platformAppearance: currentSettings.platformAppearance,
    fontPreset: currentSettings.fontPreset
  });
  const visibleHosts = sshConfigHosts.slice(0, 8);
  const hiddenHostCount = Math.max(0, sshConfigHosts.length - visibleHosts.length);
  const hasLocalHosts = sshConfigHosts.length > 0;
  const detectionPending = step === "ssh" && busy && !hasLocalHosts;
  const configPath = sshStatus?.configPath ?? "%USERPROFILE%\\.ssh\\config";
  const ed25519Ready = Boolean(sshStatus?.ed25519.privateExists);
  const preferencesCopy = uiCopy[preferenceDraft.fontPreset === "zh-cn" ? "zh" : "en"];

  useEffect(() => {
    setPreferenceDraft({
      theme: currentSettings.theme,
      platformAppearance: currentSettings.platformAppearance,
      fontPreset: currentSettings.fontPreset
    });
  }, [currentSettings.fontPreset, currentSettings.platformAppearance, currentSettings.theme]);

  useEffect(() => {
    applyAppSettings({ ...currentSettings, ...preferenceDraft });
  }, [currentSettings, preferenceDraft]);

  return (
    <div className="modalBackdrop" role="presentation">
      <ModalFrame className="setupGuideModal" titleId="setup-guide-title">
        {step === "preferences" ? (
          <>
            <ModalHeader
              className="setupGuideHero"
              titleId="setup-guide-title"
              title={preferencesCopy.setupGuide.preferencesTitle}
              description={preferencesCopy.setupGuide.preferencesBody}
              icon="settings"
            />

            <div className="setupGuidePreferences">
              <div className="setupGuidePreferenceRow">
                <span>{preferencesCopy.settings.theme}</span>
                <div className="segmentedControl" role="group" aria-label={preferencesCopy.settings.theme}>
                  {(["system", "light", "dark"] as ThemeChoice[]).map((choice) => (
                    <button
                      data-active={preferenceDraft.theme === choice}
                      key={choice}
                      type="button"
                      onClick={() => setPreferenceDraft((current) => ({ ...current, theme: choice }))}
                    >
                      {preferencesCopy.settings.themeOptions[choice]}
                    </button>
                  ))}
                </div>
              </div>

              <div className="setupGuidePreferenceRow">
                <span>{preferencesCopy.settings.platformAppearance}</span>
                <div className="segmentedControl" role="group" aria-label={preferencesCopy.settings.platformAppearance}>
                  {(["auto", "windows", "macos"] as PlatformAppearance[]).map((choice) => (
                    <button
                      data-active={preferenceDraft.platformAppearance === choice}
                      key={choice}
                      type="button"
                      onClick={() => setPreferenceDraft((current) => ({ ...current, platformAppearance: choice }))}
                    >
                      {preferencesCopy.settings.platformOptions[choice]}
                    </button>
                  ))}
                </div>
              </div>

              <div className="setupGuidePreferenceRow">
                <span>{preferencesCopy.settings.font}</span>
                <div className="segmentedControl" data-options="2" role="group" aria-label={preferencesCopy.settings.font}>
                  {(["english", "zh-cn"] as FontPreset[]).map((preset) => (
                    <button
                      data-active={preferenceDraft.fontPreset === preset}
                      key={preset}
                      type="button"
                      onClick={() => setPreferenceDraft((current) => ({ ...current, fontPreset: preset }))}
                    >
                      {preset === "english" ? preferencesCopy.setupGuide.languageEnglish : preferencesCopy.setupGuide.languageChinese}
                    </button>
                  ))}
                </div>
              </div>
            </div>

            <ModalActions className="setupGuideActions" dataHasHosts>
              <button className="primaryButton" disabled={busy} type="button" onClick={() => void onPreferencesNext(preferenceDraft)}>
                {preferencesCopy.setupGuide.next}
              </button>
            </ModalActions>
          </>
        ) : (
          <>
            <ModalHeader
              className="setupGuideHero"
              titleId="setup-guide-title"
              title={copy.setupGuide.title}
              description={detectionPending ? copy.setupGuide.detecting : hasLocalHosts ? copy.setupGuide.bodyWithHosts(sshConfigHosts.length) : copy.setupGuide.bodyEmpty}
              icon="hosts"
            >
              <code>{copy.setupGuide.detectedPath(configPath)}</code>
            </ModalHeader>

            <article className="setupGuideKeyCard">
              <div>
                <TitleWithIcon icon="key" level={3}>{copy.setupGuide.ed25519KeyTitle}</TitleWithIcon>
                <p>{ed25519Ready ? copy.setupGuide.ed25519ReadyBody : copy.setupGuide.ed25519MissingBody}</p>
              </div>
              <Badge tone={ed25519Ready ? "green" : "gray"}>
                {ed25519Ready ? copy.setupGuide.ed25519Ready : copy.settings.missing}
              </Badge>
            </article>

            {hasLocalHosts ? (
              <div className="setupGuideHostList" role="table" aria-label={copy.hosts.detectedSshHosts}>
                <div className="setupGuideHostRow setupGuideHostHeader" role="row">
                  <span role="columnheader">{copy.hosts.alias}</span>
                  <span role="columnheader">{copy.hosts.hostName}</span>
                  <span role="columnheader">{copy.hosts.user}</span>
                  <span role="columnheader">{copy.setupGuide.source}</span>
                </div>
                {visibleHosts.map((host) => (
                  <div className="setupGuideHostRow" key={`${host.source}-${host.alias}`} role="row">
                    <strong role="cell">{host.alias}</strong>
                    <span role="cell">{host.hostName || host.alias}</span>
                    <span role="cell">{host.user || copy.hosts.unknown}</span>
                    <span role="cell"><Badge tone={host.managed ? "blue" : "gray"}>{sshHostSourceLabel(copy, host)}</Badge></span>
                  </div>
                ))}
                {hiddenHostCount > 0 ? <p className="setupGuideMore">{copy.setupGuide.moreHosts(hiddenHostCount)}</p> : null}
              </div>
            ) : detectionPending ? (
              <EmptyListState copy={copy} message={copy.setupGuide.detecting} variant="hosts" />
            ) : (
              <EmptyListState copy={copy} message={copy.emptyLists.hosts} variant="hosts" />
            )}

            <ModalActions className="setupGuideActions" dataHasHosts={hasLocalHosts}>
              {hasLocalHosts ? (
                <button className="secondaryButton" disabled={busy} type="button" onClick={() => void onSkip()}>
                  {copy.setupGuide.skip}
                </button>
              ) : null}
              {hasLocalHosts ? (
                <button className="primaryButton" disabled={busy} type="button" onClick={() => void onImport()}>
                  {busy ? copy.setupGuide.importing : copy.setupGuide.importLocalConfig}
                </button>
              ) : (
                <button className="primaryButton" disabled={busy} type="button" onClick={() => void onClose()}>
                  {copy.setupGuide.close}
                </button>
              )}
            </ModalActions>
          </>
        )}
      </ModalFrame>
    </div>
  );
}

function EmptyListState({
  action,
  copy,
  message,
  variant
}: {
  action?: ReactNode;
  copy: UICopy;
  message: string;
  variant: "hosts" | "profiles" | "skills" | "tasks";
}) {
  const iconIdByVariant = {
    hosts: "hosts",
    profiles: "profiles",
    skills: "skills",
    tasks: "tasks"
  } satisfies Record<typeof variant, NavIconId>;
  const iconId = iconIdByVariant[variant];

  return (
    <div className="emptyState emptyListState" data-variant={variant}>
      <div className="emptyListIcon" aria-hidden="true">
        <NavIcon id={iconId} />
      </div>
      <p>{message || copy.emptyLists.hosts}</p>
      {action ? <div className="emptyListActions">{action}</div> : null}
    </div>
  );
}

function CodexUninstallConfirmModal({
  copy,
  target,
  onCancel,
  onConfirm
}: {
  copy: UICopy;
  target: CodexUninstallConfirmState;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
      <AlertModalFrame className="taskLogModal simpleDeleteModal" titleId="codex-uninstall-confirm-title" onCancel={onCancel}>
        <ModalHeader
          className="taskLogModalHeader"
          titleId="codex-uninstall-confirm-title"
          title={copy.hosts.uninstallCodexConfirmTitle(target.hostAlias)}
          description={copy.hosts.uninstallCodexConfirmBody}
          icon="delete"
        />
        <ModalActions>
          <button className="secondaryButton" data-alert-cancel type="button" onClick={onCancel}>
            {copy.hosts.cancel}
          </button>
          <button className="primaryButton dangerButton" type="button" onClick={onConfirm}>
            {copy.hosts.confirmUninstallCodex}
          </button>
        </ModalActions>
      </AlertModalFrame>
  );
}

function CodexOperationModal({
  copy,
  operation,
  onClose,
  onViewTasks
}: {
  copy: UICopy;
  operation: CodexOperationModalState;
  onClose: () => void;
  onViewTasks: () => void;
}) {
  const actionLabel = remoteCodexButtonLabel(copy, undefined, operation.action);
  const statusTone = operation.status === "success" ? "green" : operation.status === "failed" ? "red" : "yellow";
  const progressLogs = operation.logs.slice(-20);
  const taskLogs = operation.task?.logs.slice(-6) ?? [];
  const taskLogCount = operation.task?.logs.length ?? 0;
  const logRowsRef = useRef<HTMLDivElement | null>(null);
  const runningLogs = [
    { level: "info" as const, message: copy.codexOperation.started, detail: operation.hostAlias },
    { level: "info" as const, message: copy.codexOperation.waiting, detail: actionLabel },
    {
      level: "warn" as const,
      message: codexOperationHint(copy, operation.action),
      detail: ""
    }
  ];

  useEffect(() => {
    const logRows = logRowsRef.current;
    if (!logRows) return;
    logRows.scrollTop = logRows.scrollHeight;
  }, [operation.logs.length, operation.status, taskLogCount]);

  return (
    <div className="modalBackdrop" role="presentation">
      <ModalFrame className="codexOperationModal" titleId="codex-operation-modal-title">
        <ModalHeader
          className="codexOperationHeader"
          titleId="codex-operation-modal-title"
          title={actionLabel}
          icon="terminal"
          badge={<Badge tone={statusTone}>{copy.codexOperation[operation.status]}</Badge>}
          closeAriaLabel={operation.status === "running" ? copy.codexOperation.hide : copy.codexOperation.close}
          onClose={onClose}
        />

        <div className="codexOperationSummary">
          <span>{copy.codexOperation.summary}</span>
          <strong>{operation.message ?? operation.error ?? (operation.status === "running" ? copy.codexOperation.waiting : copy.codexOperation.noLogs)}</strong>
        </div>

        <div className="codexOperationLog">
          <div className="codexOperationLogTitle">
            <span>{copy.codexOperation.latestLog}</span>
            {operation.status === "running" ? <i aria-hidden="true" /> : null}
          </div>
          <div className="codexOperationLogRows" ref={logRowsRef}>
            {progressLogs.length > 0
              ? progressLogs.map((log, index) => (
                  <div className="codexOperationLogRow" data-level={progressLogLevel(log)} key={`${log.step}-${log.status}-${index}`}>
                    <strong>{progressLogLabel(copy, log)}</strong>
                    <span>{log.message}</span>
                  </div>
                ))
              : operation.status === "running"
                ? runningLogs.map((log, index) => (
                  <div className="codexOperationLogRow" data-level={log.level} key={`${log.message}-${index}`}>
                    <strong>{copy.status.log[log.level]}</strong>
                    <span>{log.message}</span>
                  </div>
                ))
                : taskLogs.length > 0
                  ? taskLogs.map((log) => (
                    <div className="codexOperationLogRow" data-level={log.level} key={log.id}>
                      <strong>{copy.status.log[log.level]}</strong>
                      <span>{log.message}</span>
                    </div>
                  ))
                  : (
                    <div className="codexOperationLogRow" data-level={operation.status === "failed" ? "error" : "info"}>
                      <strong>{operation.status === "failed" ? copy.status.log.error : copy.status.log.info}</strong>
                      <span>{operation.error ?? copy.codexOperation.noLogs}</span>
                    </div>
                  )}
          </div>
        </div>

        <ModalActions className="codexOperationActions">
          {operation.task ? (
            <button className="secondaryButton" type="button" onClick={onViewTasks}>
              {copy.codexOperation.viewTasks}
            </button>
          ) : null}
          <button className="primaryButton" type="button" onClick={onClose}>
            {operation.status === "running" ? copy.codexOperation.hide : copy.codexOperation.close}
          </button>
        </ModalActions>
      </ModalFrame>
    </div>
  );
}

function DashboardView({
  appliedProfileCount,
  copy,
  hostBusy,
  hosts,
  inventoryStatus,
  latestCodexVersion,
  onlineCount,
  profiles,
  profileById,
  sshConfigHosts,
  skillPacks,
  tasks,
  successfulTaskCount,
  onAddServer,
  onTestAllSshHosts
}: {
  appliedProfileCount: number;
  copy: UICopy;
  hostBusy: Record<string, HostBusyAction>;
  hosts: Host[];
  inventoryStatus: SkillInventoryStatus;
  latestCodexVersion: LatestCodexVersion | null;
  onlineCount: number;
  profiles: Profile[];
  profileById: Map<string, Profile>;
  sshConfigHosts: SshConfigHost[];
  skillPacks: SkillPack[];
  tasks: TaskRun[];
  successfulTaskCount: number;
  onAddServer: () => void;
  onTestAllSshHosts: () => Promise<unknown>;
}) {
  const labelFor = (id: SectionId) => copy.navItems.find((item) => item.id === id)?.label ?? id;
  return (
    <div className="pageGrid">
      <section className="summaryStrip" aria-label={copy.dashboard.summaryLabel}>
        <MetricCard label={labelFor("hosts")} value={String(hosts.length)} detailLabel={copy.dashboard.online} detailValue={String(onlineCount)} />
        <MetricCard label={labelFor("profiles")} value={String(profiles.length)} detailLabel={copy.dashboard.applied} detailValue={String(appliedProfileCount)} />
        <MetricCard label={labelFor("skills")} value={String(skillPacks.length)} detailLabel={copy.dashboard.enabled} detailValue={String(skillPacks.filter((pack) => pack.enabled).length)} />
        <MetricCard label={labelFor("tasks")} value={String(tasks.length)} detailLabel={copy.dashboard.success} detailValue={String(successfulTaskCount)} />
      </section>

      <ServerMatrix
        copy={copy}
        hostBusy={hostBusy}
        hosts={hosts}
        inventoryStatus={inventoryStatus}
        latestCodexVersion={latestCodexVersion}
        profileById={profileById}
        sshConfigHosts={sshConfigHosts}
        onAddServer={onAddServer}
        onTestAllSshHosts={onTestAllSshHosts}
      />
    </div>
  );
}

function MetricCard({
  label,
  value,
  detailLabel,
  detailValue
}: {
  label: string;
  value: string;
  detailLabel: string;
  detailValue: string;
}) {
  return (
    <article className="metricCard">
      <div className="metricPrimary">
        <span>{label}</span>
        <strong>{value}</strong>
      </div>
      <div className="metricSecondary">
        <span>{detailLabel}</span>
        <b>{detailValue}</b>
      </div>
    </article>
  );
}

function MonitorView({
  autoRefresh,
  busy,
  checkedAt,
  copy,
  error,
  hosts,
  hostOrder,
  refreshSeconds,
  settingsSaving,
  snapshots,
  onAutoRefreshChange,
  onHostOrderChange,
  onRefresh,
  onRefreshSecondsChange
}: {
  autoRefresh: boolean;
  busy: boolean;
  checkedAt: string | null;
  copy: UICopy;
  error: string | null;
  hosts: Host[];
  hostOrder: string[];
  refreshSeconds: number;
  settingsSaving: boolean;
  snapshots: HostResourceSnapshot[];
  onAutoRefreshChange: (enabled: boolean) => void;
  onHostOrderChange: (hostOrder: string[]) => void;
  onRefresh: () => Promise<HostResourceBatchResult | null>;
  onRefreshSecondsChange: (seconds: number) => void;
}) {
  const [dragState, setDragState] = useState<MonitorDragState | null>(null);
  const cardRefs = useRef<Map<string, HTMLElement>>(new Map());
  const pendingFlipRectsRef = useRef<Map<string, DOMRect> | null>(null);
  const pendingReorderTimerRef = useRef<number | null>(null);
  const pendingReorderSignatureRef = useRef<string | null>(null);
  const autoScrollFrameRef = useRef<number | null>(null);
  const autoScrollSpeedRef = useRef(0);
  const snapshotByAlias = useMemo(
    () => new Map(snapshots.map((snapshot) => [snapshot.hostAlias.toLowerCase(), snapshot])),
    [snapshots]
  );
  const gpuUserColorByUser = useMemo(() => buildMonitorGpuUserColorMap(snapshots), [snapshots]);
  const orderedHosts = useMemo(() => orderMonitorHosts(hosts, hostOrder), [hostOrder, hosts]);
  const displayedHosts = useMemo(
    () => (dragState ? orderMonitorHosts(hosts, dragState.previewOrder) : orderedHosts),
    [dragState, hosts, orderedHosts]
  );
  const ghostHost = dragState ? hosts.find((host) => host.hostAlias === dragState.alias) ?? null : null;
  const ghostSnapshot = ghostHost ? snapshotByAlias.get(ghostHost.hostAlias.toLowerCase()) ?? null : null;

  const registerMonitorCard = useCallback((alias: string, element: HTMLElement | null) => {
    if (element) {
      cardRefs.current.set(alias, element);
    } else {
      cardRefs.current.delete(alias);
    }
  }, []);

  const measureMonitorCards = useCallback(() => measureMonitorCardRects(cardRefs.current), []);

  const scheduleMonitorFlip = useCallback(() => {
    pendingFlipRectsRef.current = measureMonitorCards();
  }, [measureMonitorCards]);

  const clearPendingMonitorReorder = useCallback(() => {
    pendingReorderSignatureRef.current = null;
    if (pendingReorderTimerRef.current !== null) {
      window.clearTimeout(pendingReorderTimerRef.current);
      pendingReorderTimerRef.current = null;
    }
  }, []);

  useLayoutEffect(() => {
    const firstRects = pendingFlipRectsRef.current;
    if (!firstRects) return;
    pendingFlipRectsRef.current = null;
    const cleanupTimers: number[] = [];
    cardRefs.current.forEach((element, alias) => {
      if (dragState?.alias === alias) return;
      const first = firstRects.get(alias);
      if (!first) return;
      const last = element.getBoundingClientRect();
      const deltaX = first.left - last.left;
      const deltaY = first.top - last.top;
      if (Math.abs(deltaX) < 1 && Math.abs(deltaY) < 1) return;
      element.style.transition = "none";
      element.style.transform = `translate(${deltaX}px, ${deltaY}px)`;
      element.style.zIndex = "2";
      requestAnimationFrame(() => {
        element.style.transition = "transform 180ms cubic-bezier(0.22, 1, 0.36, 1)";
        element.style.transform = "";
        cleanupTimers.push(window.setTimeout(() => {
          element.style.transition = "";
          element.style.zIndex = "";
        }, 210));
      });
    });
    return () => {
      cleanupTimers.forEach((timer) => window.clearTimeout(timer));
    };
  }, [displayedHosts, dragState?.alias]);

  const stopMonitorAutoScroll = useCallback(() => {
    autoScrollSpeedRef.current = 0;
    if (autoScrollFrameRef.current !== null) {
      window.cancelAnimationFrame(autoScrollFrameRef.current);
      autoScrollFrameRef.current = null;
    }
  }, []);

  const startMonitorAutoScroll = useCallback(() => {
    if (autoScrollFrameRef.current !== null) return;
    const tick = () => {
      const speed = autoScrollSpeedRef.current;
      if (speed === 0) {
        autoScrollFrameRef.current = null;
        return;
      }
      window.scrollBy(0, speed);
      autoScrollFrameRef.current = window.requestAnimationFrame(tick);
    };
    autoScrollFrameRef.current = window.requestAnimationFrame(tick);
  }, []);

  const updateMonitorAutoScroll = useCallback((clientY: number) => {
    const threshold = Math.max(48, window.innerHeight * 0.1);
    let speed = 0;
    if (clientY < threshold) {
      speed = -Math.ceil((1 - Math.max(0, clientY) / threshold) * 18);
    } else if (clientY > window.innerHeight - threshold) {
      speed = Math.ceil((1 - Math.max(0, window.innerHeight - clientY) / threshold) * 18);
    }
    autoScrollSpeedRef.current = speed;
    if (speed === 0) {
      stopMonitorAutoScroll();
    } else {
      startMonitorAutoScroll();
    }
  }, [startMonitorAutoScroll, stopMonitorAutoScroll]);

  const finishMonitorDrag = useCallback((commit: boolean) => {
    if (dragState && commit && !sameStringArray(dragState.sourceOrder, dragState.previewOrder)) {
      onHostOrderChange(dragState.previewOrder);
    }
    setDragState(null);
    clearPendingMonitorReorder();
    stopMonitorAutoScroll();
  }, [clearPendingMonitorReorder, dragState, onHostOrderChange, stopMonitorAutoScroll]);

  useEffect(() => {
    if (!dragState) return;
    const handlePointerMove = (event: PointerEvent) => {
      if (event.pointerId !== dragState.pointerId) return;
      event.preventDefault();
      updateMonitorAutoScroll(event.clientY);
      const nextOrder = previewMonitorHostOrder(
        dragState.previewOrder,
        dragState.alias,
        event.clientX,
        event.clientY,
        cardRefs.current
      );
      const orderChanged = !sameStringArray(nextOrder, dragState.previewOrder);
      if (!orderChanged) {
        clearPendingMonitorReorder();
      } else {
        const signature = nextOrder.join("\u0000");
        if (pendingReorderSignatureRef.current !== signature) {
          clearPendingMonitorReorder();
          pendingReorderSignatureRef.current = signature;
          pendingReorderTimerRef.current = window.setTimeout(() => {
            pendingReorderTimerRef.current = null;
            pendingReorderSignatureRef.current = null;
            scheduleMonitorFlip();
            setDragState((current) => {
              if (!current || current.pointerId !== event.pointerId) return current;
              return sameStringArray(current.previewOrder, nextOrder) ? current : { ...current, previewOrder: nextOrder };
            });
          }, 500);
        }
      }
      setDragState((current) => {
        if (!current || current.pointerId !== event.pointerId) return current;
        return {
          ...current,
          x: event.clientX,
          y: event.clientY
        };
      });
    };
    const handlePointerUp = (event: PointerEvent) => {
      if (event.pointerId === dragState.pointerId) finishMonitorDrag(true);
    };
    const handlePointerCancel = (event: PointerEvent) => {
      if (event.pointerId === dragState.pointerId) finishMonitorDrag(false);
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") finishMonitorDrag(false);
    };
    const handleBlur = () => finishMonitorDrag(false);
    window.addEventListener("pointermove", handlePointerMove, { passive: false });
    window.addEventListener("pointerup", handlePointerUp);
    window.addEventListener("pointercancel", handlePointerCancel);
    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("blur", handleBlur);
    return () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerUp);
      window.removeEventListener("pointercancel", handlePointerCancel);
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("blur", handleBlur);
    };
  }, [clearPendingMonitorReorder, dragState, finishMonitorDrag, scheduleMonitorFlip, updateMonitorAutoScroll]);

  const handleDragHandlePointerDown = useCallback((alias: string, event: ReactPointerEvent<HTMLButtonElement>) => {
    if (settingsSaving) return;
    if (event.button !== 0 || orderedHosts.length < 2) return;
    const card = cardRefs.current.get(alias);
    if (!card) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    const rect = card.getBoundingClientRect();
    const sourceOrder = orderedHosts.map((host) => host.hostAlias);
    setDragState({
      alias,
      height: rect.height,
      offsetX: event.clientX - rect.left,
      offsetY: event.clientY - rect.top,
      pointerId: event.pointerId,
      previewOrder: sourceOrder,
      sourceOrder,
      width: rect.width,
      x: event.clientX,
      y: event.clientY
    });
  }, [orderedHosts, settingsSaving]);

  return (
    <div className="monitorPage">
      <section className="panel monitorPanel monitorHeroPanel">
        <div className="panelHeader monitorHeader monitorHeroHeader">
          <div>
            <TitleWithIcon icon="monitor" level={1}>{copy.sections.monitor.title}</TitleWithIcon>
            <small className="monitorCheckedAt">{`${copy.monitor.lastUpdated}: ${formatMonitorTimestamp(checkedAt, copy)}`}</small>
          </div>
          <CommandBar ariaLabel={copy.sections.monitor.title} className="topActions monitorActions">
            <button className="primaryButton monitorRefreshButton" disabled={busy || hosts.length === 0} type="button" onClick={() => void onRefresh()}>
              {busy ? copy.monitor.refreshing : copy.monitor.refreshNow}
            </button>
            <div className="monitorAutoRefreshControl">
              <span>{copy.monitor.autoRefresh}</span>
              <button
                className="pillToggle"
                data-enabled={autoRefresh}
                role="switch"
                aria-checked={autoRefresh}
                aria-label={copy.monitor.autoRefresh}
                disabled={settingsSaving}
                type="button"
                onClick={() => onAutoRefreshChange(!autoRefresh)}
              >
                <span className="pillToggleThumb" aria-hidden="true" />
              </button>
              {autoRefresh ? (
                <label className="monitorIntervalCompact">
                  <input
                    aria-label={copy.monitor.refreshEvery}
                    disabled={settingsSaving}
                    max={300}
                    min={15}
                    onChange={(event) => onRefreshSecondsChange(Number(event.currentTarget.value))}
                    type="number"
                    value={refreshSeconds}
                  />
                  <span>{copy.monitor.seconds}</span>
                </label>
              ) : null}
            </div>
          </CommandBar>
        </div>

        {error ? <p className="monitorError">{error}</p> : null}
      </section>

      {hosts.length === 0 ? (
        <section className="panel monitorEmptyPanel">
          <EmptyListState copy={copy} message={copy.monitor.noHostsBody} variant="hosts" />
        </section>
      ) : (
        <div className="monitorBentoGrid">
          {displayedHosts.map((host) => (
            <MonitorHostCard
              copy={copy}
              dragging={dragState?.alias === host.hostAlias}
              host={host}
              key={host.id}
              placeholder={dragState?.alias === host.hostAlias}
              snapshot={snapshotByAlias.get(host.hostAlias.toLowerCase()) ?? null}
              userColorByUser={gpuUserColorByUser}
              onCardElement={registerMonitorCard}
              onDragHandlePointerDown={handleDragHandlePointerDown}
            />
          ))}
        </div>
      )}
      {dragState && ghostHost ? (
        <MonitorHostDragGhost
          copy={copy}
          dragState={dragState}
          host={ghostHost}
          snapshot={ghostSnapshot}
        />
      ) : null}
    </div>
  );
}

function MonitorHostCard({
  copy,
  dragging,
  host,
  placeholder,
  snapshot,
  userColorByUser,
  onCardElement,
  onDragHandlePointerDown
}: {
  copy: UICopy;
  dragging: boolean;
  host: Host;
  placeholder: boolean;
  snapshot: HostResourceSnapshot | null;
  userColorByUser: MonitorGpuUserColorMap;
  onCardElement: (alias: string, element: HTMLElement | null) => void;
  onDragHandlePointerDown: (alias: string, event: ReactPointerEvent<HTMLButtonElement>) => void;
}) {
  const cardRef = useRef<HTMLElement | null>(null);
  const [rowSpan, setRowSpan] = useState(44);
  const cpuPercent = monitorCpuPercent(snapshot?.cpu);
  const memoryPercent = snapshot?.memory?.usedPercent ?? null;
  const gpus = snapshot?.gpus ?? [];
  const gpuCount = gpus.length;
  const gpuMemory = summarizeGpuMemory(gpus, copy);

  const setCardElement = useCallback((element: HTMLElement | null) => {
    cardRef.current = element;
    onCardElement(host.hostAlias, element);
  }, [host.hostAlias, onCardElement]);

  useEffect(() => {
    const element = cardRef.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const updateSpan = () => {
      const grid = element.parentElement;
      if (!grid) return;
      const styles = window.getComputedStyle(grid);
      const rowHeight = Number.parseFloat(styles.gridAutoRows) || 2;
      const rowGap = Number.parseFloat(styles.rowGap) || 0;
      const height = element.getBoundingClientRect().height;
      setRowSpan(Math.max(1, Math.ceil((height + rowGap) / (rowHeight + rowGap))));
    };
    const observer = new ResizeObserver(updateSpan);
    observer.observe(element);
    updateSpan();
    window.addEventListener("resize", updateSpan);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", updateSpan);
    };
  }, [host.id, snapshot]);

  return (
    <article
      className="monitorHostCard"
      data-dragging={dragging}
      data-placeholder={placeholder}
      ref={setCardElement}
      style={{ "--monitor-card-span": rowSpan } as CSSProperties}
    >
      <button
        className="monitorDragHandle"
        type="button"
        aria-label={copy.monitor.dragHandle}
        title={copy.monitor.dragHandle}
        onPointerDown={(event) => onDragHandlePointerDown(host.hostAlias, event)}
      >
        <span aria-hidden="true" />
      </button>
      <header className="monitorHostHeader">
        <div>
          <h3>{host.hostAlias}</h3>
        </div>
        {snapshot ? (
          <Badge tone={resourceStatusTone(snapshot.status)}>{resourceStatusLabel(snapshot.status, copy)}</Badge>
        ) : (
          <Badge tone="gray">{copy.monitor.statusNoSample}</Badge>
        )}
      </header>

      {snapshot?.error ? <small className="monitorCellNote">{snapshot.error}</small> : null}

      <div className="monitorSummaryGrid">
        <MonitorSummaryTile
          copy={copy}
          detail={formatMonitorMetricDetail(copy.monitor.available, formatBytes(snapshot?.memory?.availableBytes, copy), copy)}
          label={copy.monitor.memory}
          meterValue={memoryPercent}
          tone="memory"
          value={formatPercent(memoryPercent, copy)}
        />
        <MonitorSummaryTile
          copy={copy}
          detail={formatMonitorMetricDetail(copy.monitor.load, formatCpuLoadSummary(snapshot?.cpu, copy), copy)}
          label={copy.monitor.cpu}
          meterValue={cpuPercent}
          tone="cpu"
          value={formatPercent(cpuPercent, copy)}
        />
        <MonitorSummaryTile
          copy={copy}
          detail={gpuMemory.detail}
          label={copy.monitor.gpu}
          meterValue={gpuMemory.percent}
          tone={gpuCount > 0 ? "gpu" : "gray"}
          value={String(gpuCount)}
        />
      </div>

      <section className="monitorGpuStack" aria-label={copy.monitor.gpu}>
        {gpus.length > 0 ? (
          gpus.map((gpu, index) => (
            <MonitorGpuBlock
              copy={copy}
              gpu={gpu}
              key={`${gpu.uuid ?? gpu.index ?? index}-${index}`}
              userColorByUser={userColorByUser}
            />
          ))
        ) : (
          <div className="monitorGpuEmpty">
            <Badge tone="gray">{copy.monitor.noGpu}</Badge>
            <span>{snapshot?.gpuTool ? `${copy.monitor.gpuTool}: ${snapshot.gpuTool}` : copy.monitor.noData}</span>
          </div>
        )}
      </section>
    </article>
  );
}

function MonitorSummaryTile({
  copy,
  detail,
  label,
  meterValue,
  tone,
  value
}: {
  copy: UICopy;
  detail: string;
  label: string;
  meterValue?: number | null;
  tone: MonitorMeterTone;
  value: string;
}) {
  return (
    <div className="monitorSummaryTile">
      <div>
        <span>{label}</span>
        <strong>{value}</strong>
      </div>
      {typeof meterValue === "number" || meterValue === null ? <MonitorMeter value={meterValue ?? null} tone={tone} /> : null}
      <small>{detail || copy.hosts.unknown}</small>
    </div>
  );
}

function MonitorGpuBlock({
  copy,
  gpu,
  userColorByUser
}: {
  copy: UICopy;
  gpu: HostResourceSnapshot["gpus"][number];
  userColorByUser: MonitorGpuUserColorMap;
}) {
  const userUsages = aggregateGpuProcessUsers(gpu, userColorByUser);
  return (
    <article className="monitorGpuBlock" data-status={gpu.status}>
      <header className="monitorGpuLineHeader">
        <strong>{`${gpu.index ? `GPU ${gpu.index}` : copy.monitor.gpu} · ${formatGpuName(gpu.name)}`}</strong>
        <span>{formatGpuCoreDetails(gpu, copy)}</span>
      </header>
      <MonitorGpuMeter copy={copy} gpu={gpu} userUsages={userUsages} />
      {gpu.status === "detected" ? (
        <p className="monitorGpuDetected">{copy.monitor.detectedOnly}</p>
      ) : userUsages.length > 0 ? (
        <div className="monitorProcessList">
          {userUsages.map((usage) => (
            <div
              className="monitorProcessRow"
              key={usage.user}
              style={{ "--monitor-user-color": usage.color } as CSSProperties}
              title={`${copy.monitor.usage}: ${formatBytes(usage.usedMemoryBytes, copy)}`}
            >
              <strong>{usage.user}</strong>
              <span>{formatProcessCount(usage.processCount, copy)}</span>
              <span>{formatDuration(usage.elapsedSeconds, copy)}</span>
              <span>{formatBytes(usage.usedMemoryBytes, copy)}</span>
            </div>
          ))}
        </div>
      ) : null}
    </article>
  );
}

function MonitorHostDragGhost({
  copy,
  dragState,
  host,
  snapshot
}: {
  copy: UICopy;
  dragState: MonitorDragState;
  host: Host;
  snapshot: HostResourceSnapshot | null;
}) {
  const cpuPercent = monitorCpuPercent(snapshot?.cpu);
  const memoryPercent = snapshot?.memory?.usedPercent ?? null;
  const gpus = snapshot?.gpus ?? [];
  const gpuMemory = summarizeGpuMemory(gpus, copy);
  return (
    <div
      className="monitorDragGhost"
      style={{
        height: dragState.height,
        left: dragState.x - dragState.offsetX,
        top: dragState.y - dragState.offsetY,
        width: dragState.width
      } as CSSProperties}
    >
      <span className="monitorDragGhostHandle" aria-hidden="true" />
      <header className="monitorHostHeader">
        <h3>{host.hostAlias}</h3>
        {snapshot ? (
          <Badge tone={resourceStatusTone(snapshot.status)}>{resourceStatusLabel(snapshot.status, copy)}</Badge>
        ) : (
          <Badge tone="gray">{copy.monitor.statusNoSample}</Badge>
        )}
      </header>
      <div className="monitorSummaryGrid">
        <MonitorSummaryTile
          copy={copy}
          detail={formatMonitorMetricDetail(copy.monitor.available, formatBytes(snapshot?.memory?.availableBytes, copy), copy)}
          label={copy.monitor.memory}
          meterValue={memoryPercent}
          tone="memory"
          value={formatPercent(memoryPercent, copy)}
        />
        <MonitorSummaryTile
          copy={copy}
          detail={formatMonitorMetricDetail(copy.monitor.load, formatCpuLoadSummary(snapshot?.cpu, copy), copy)}
          label={copy.monitor.cpu}
          meterValue={cpuPercent}
          tone="cpu"
          value={formatPercent(cpuPercent, copy)}
        />
        <MonitorSummaryTile
          copy={copy}
          detail={gpuMemory.detail}
          label={copy.monitor.gpu}
          meterValue={gpuMemory.percent}
          tone={gpus.length > 0 ? "gpu" : "gray"}
          value={String(gpus.length)}
        />
      </div>
    </div>
  );
}

function MonitorGpuMeter({
  copy,
  gpu,
  userUsages
}: {
  copy: UICopy;
  gpu: HostResourceSnapshot["gpus"][number];
  userUsages: MonitorGpuUserUsage[];
}) {
  const denominator = gpu.memoryTotalBytes || userUsages.reduce((sum, usage) => sum + usage.usedMemoryBytes, 0);
  if (userUsages.length === 0 || denominator <= 0) {
    return <MonitorMeter value={gpu.utilizationPercent} tone="gpu" />;
  }
  return (
    <div className="monitorMeter monitorSegmentedMeter" aria-label={copy.monitor.gpuProcesses}>
      {userUsages.map((usage) => {
        const width = Math.min(100, Math.max(0, usage.usedMemoryBytes / denominator * 100));
        return (
          <span
            key={usage.user}
            style={{
              width: `${width > 0 && width < 1 ? 1 : width}%`,
              background: usage.color
            } as CSSProperties}
            title={`${usage.user}: ${formatBytes(usage.usedMemoryBytes, copy)}`}
          />
        );
      })}
    </div>
  );
}

function MonitorMeter({ value, tone }: { value: number | null; tone: MonitorMeterTone }) {
  const width = typeof value === "number" ? `${Math.min(100, Math.max(0, value))}%` : "0%";
  return (
    <div className="monitorMeter" data-tone={tone}>
      <span style={{ width } as CSSProperties} />
    </div>
  );
}

function ServerMatrix({
  copy,
  hostBusy,
  hosts,
  inventoryStatus,
  latestCodexVersion,
  profileById,
  sshConfigHosts,
  onAddServer,
  onTestAllSshHosts
}: {
  copy: UICopy;
  hostBusy: Record<string, HostBusyAction>;
  hosts: Host[];
  inventoryStatus: SkillInventoryStatus;
  latestCodexVersion: LatestCodexVersion | null;
  profileById: Map<string, Profile>;
  sshConfigHosts: SshConfigHost[];
  onAddServer: () => void;
  onTestAllSshHosts: () => Promise<unknown>;
}) {
  const anyHostBusy = sshConfigHosts.some((host) => Boolean(hostBusy[host.alias]));
  const testingAll = sshConfigHosts.length > 0 && sshConfigHosts.every((host) => hostBusy[host.alias] === "test");
  const hostInventoryByAlias = new Map(inventoryStatus.hostInventories.map((inventory) => [inventory.hostAlias.toLowerCase(), inventory]));
  const skillCounts = hosts.map((host) => dashboardHostSkillCount(host, hostInventoryByAlias.get(host.hostAlias.toLowerCase())));

  return (
    <section className="panel spanWide">
      <div className="panelHeader matrixHeader">
        <TitleWithIcon icon="hosts" level={2}>{copy.dashboard.serverMatrix}</TitleWithIcon>
        <CommandBar ariaLabel={copy.dashboard.serverMatrix} className="topActions">
          <button className="primaryButton" disabled={sshConfigHosts.length === 0 || anyHostBusy} type="button" onClick={() => void onTestAllSshHosts()}>
            {testingAll ? copy.hosts.testingAll : copy.hosts.refreshDetected}
          </button>
        </CommandBar>
      </div>

      {hosts.length === 0 ? (
        <div className="emptyState matrixEmptyState">
          <div className="matrixEmptyIcon" aria-hidden="true"><NavIcon id="hosts" /></div>
          <h3>{copy.dashboard.noHosts}</h3>
          <p>{copy.dashboard.noHostsBody}</p>
          <button className="primaryButton" type="button" onClick={onAddServer}>{copy.common.addServer}</button>
        </div>
      ) : (
        <div className="matrixGrid">
          {hosts.map((host) => {
            const codexStatus = hostCodexStatus(copy, host, undefined, hosts, latestCodexVersion);
            const systemLabel = hostSystemLabel(host, copy);
            const inventory = hostInventoryByAlias.get(host.hostAlias.toLowerCase());
            const skillCount = dashboardHostSkillCount(host, inventory);
            const skillCountLabel = typeof skillCount === "number" ? String(skillCount) : copy.hosts.unknown;
            const skillCountBadgeTone = dashboardSkillCountTone(skillCount, skillCounts);
            return (
            <article className="hostCard" key={host.id}>
              <div className="hostHeader">
                <div>
                  <TitleWithIcon icon="hosts" level={3}>{host.name}</TitleWithIcon>
                  <p>{formatEndpoint(host)}</p>
                </div>
                <StatusBadge copy={copy} status={host.status} />
              </div>

              <dl className="hostMeta">
                <div>
                  <dt>{copy.hosts.source}</dt>
                  <dd><Badge tone={host.source === "managed" ? "blue" : "gray"}>{hostSourceLabel(copy, host)}</Badge></dd>
                </div>
                <div>
                  <dt>{copy.dashboard.system}</dt>
                  <dd><Badge tone={knownValueTone(host.os, copy)}>{systemLabel}</Badge></dd>
                </div>
                <div>
                  <dt>{copy.hosts.codex}</dt>
                  <dd><Badge tone={codexStatus.tone}>{codexStatus.label}</Badge></dd>
                </div>
                <div>
                  <dt>{copy.hosts.configExists}</dt>
                  <dd><HostApiConfigBadge copy={copy} host={host} profileById={profileById} /></dd>
                </div>
                <div>
                  <dt>{copy.hosts.latency}</dt>
                  <dd><Badge tone={latencyTone(host.latencyMs, hosts)}>{formatLatency(host.latencyMs, copy)}</Badge></dd>
                </div>
                <div>
                  <dt>{copy.hosts.skills}</dt>
                  <dd><Badge tone={skillCountBadgeTone}>{skillCountLabel}</Badge></dd>
                </div>
              </dl>
            </article>
            );
          })}
        </div>
      )}
    </section>
  );
}

function HostsView({
  addHostOpen,
  copy,
  hostBusy,
  hosts,
  inventoryStatus,
  latestCodexVersion,
  sshBusy,
  sshConfigHosts,
  sshStatus,
  onCloseAddHost,
  onConnectSshHost,
  onDeleteSshConfigHost,
  onDetectLocalSshHosts,
  onGenerateEd25519Key,
  onManageCodex,
  onOpenAddHost,
  onOpenSetupGuide,
  onTestAllSshHosts,
  onTestHost,
  onUpdateOutdatedCodexHosts
}: {
  addHostOpen: boolean;
  copy: UICopy;
  hostBusy: Record<string, HostBusyAction>;
  hosts: Host[];
  inventoryStatus: SkillInventoryStatus;
  latestCodexVersion: LatestCodexVersion | null;
  sshBusy: boolean;
  sshConfigHosts: SshConfigHost[];
  sshStatus: SshStatus | null;
  onCloseAddHost: () => void;
  onConnectSshHost: (draft: SshHostDraft, password: string, requestId: string, onProgress: (event: SshBootstrapProgressEvent) => void) => Promise<SshBootstrapResult>;
  onDeleteSshConfigHost: (alias: string) => Promise<SshConfigDeleteResult>;
  onDetectLocalSshHosts: () => Promise<unknown>;
  onGenerateEd25519Key: () => Promise<unknown>;
  onManageCodex: (id: string, action: RemoteCodexAction) => void;
  onOpenAddHost: () => void;
  onOpenSetupGuide: () => Promise<unknown>;
  onTestAllSshHosts: () => Promise<unknown>;
  onTestHost: (id: string) => void;
  onUpdateOutdatedCodexHosts: (aliases: string[]) => Promise<unknown>;
}) {
  const reportActionError = useActionErrorReporter(copy);
  const identityFile = sshStatus?.ed25519.privateExists ? sshStatus.ed25519.privatePath : "";
  const hostByAlias = useMemo(
    () => new Map(hosts.map((host) => [host.hostAlias.toLowerCase(), host])),
    [hosts]
  );
  const [selectedHostAlias, setSelectedHostAlias] = useState<string | null>(sshConfigHosts[0]?.alias ?? hosts[0]?.hostAlias ?? null);
  const [editingDraft, setEditingDraft] = useState<SshHostDraft | null>(null);
  const [deleteHostAlias, setDeleteHostAlias] = useState<string | null>(null);
  const [deleteHostBusy, setDeleteHostBusy] = useState(false);
  const [detectHostsBusy, setDetectHostsBusy] = useState(false);
  const selectedHost =
    selectedHostAlias ? hostByAlias.get(selectedHostAlias.toLowerCase()) ?? hosts.find((host) => host.hostAlias === selectedHostAlias) ?? null : hosts[0] ?? null;
  const anyHostBusy = sshConfigHosts.some((host) => Boolean(hostBusy[host.alias]));
  const testingAll = sshConfigHosts.length > 0 && sshConfigHosts.every((host) => hostBusy[host.alias] === "test");
  const outdatedCodexAliases = sshConfigHosts.flatMap((sshHost) => {
    const host = hostByAlias.get(sshHost.alias.toLowerCase()) ?? null;
    return host && isHostCodexTested(host) && host.codexInstalled && isCodexVersionBehind(host.codexVersion, latestCodexVersion?.version) ? [sshHost.alias] : [];
  });
  const updatingOutdated = sshConfigHosts.some((host) => hostBusy[host.alias] === "update");

  useEffect(() => {
    if (!selectedHostAlias && sshConfigHosts[0]) {
      setSelectedHostAlias(sshConfigHosts[0].alias);
      return;
    }
    if (selectedHostAlias && sshConfigHosts.length > 0 && !sshConfigHosts.some((host) => host.alias === selectedHostAlias)) {
      setSelectedHostAlias(sshConfigHosts[0].alias);
    }
  }, [sshConfigHosts, selectedHostAlias]);

  useEffect(() => {
    if (!addHostOpen) setEditingDraft(null);
  }, [addHostOpen]);

  const handleEdit = (host: SshConfigHost) => {
    setEditingDraft({
      alias: host.alias,
      hostName: host.hostName,
      port: host.port,
      user: host.user,
      identityFile: host.identityFile || identityFile
    });
    onOpenAddHost();
  };

  const handleDelete = async () => {
    if (!deleteHostAlias) return;
    setDeleteHostBusy(true);
    try {
      await onDeleteSshConfigHost(deleteHostAlias);
      setDeleteHostAlias(null);
    } finally {
      setDeleteHostBusy(false);
    }
  };

  const handleDetectLocalHosts = async () => {
    setDetectHostsBusy(true);
    try {
      await onDetectLocalSshHosts();
    } finally {
      setDetectHostsBusy(false);
    }
  };

  return (
    <div className="hostsGrid">
      <SshHostModal
        copy={copy}
        defaultIdentityFile={identityFile}
        initialDraft={editingDraft}
        open={addHostOpen}
        sshBusy={sshBusy}
        sshStatus={sshStatus}
        onClose={onCloseAddHost}
        onConnect={onConnectSshHost}
        onGenerateEd25519Key={onGenerateEd25519Key}
      />

      <section className="panel spanWide">
        <div className="panelHeader">
          <div>
            <TitleWithIcon icon="hosts" level={2}>{copy.hosts.detectedSshHosts}</TitleWithIcon>
          </div>
          <CommandBar ariaLabel={copy.hosts.detectedSshHosts} className="topActions">
            <button className="secondaryButton" disabled={detectHostsBusy || anyHostBusy} type="button" onClick={() => void handleDetectLocalHosts().catch(reportActionError)}>
              {detectHostsBusy ? copy.setupGuide.detecting : copy.hosts.detect}
            </button>
            <button className="secondaryButton" disabled={sshConfigHosts.length === 0 || anyHostBusy} type="button" onClick={() => void onTestAllSshHosts()}>
              {testingAll ? copy.hosts.testingAll : copy.hosts.refreshDetected}
            </button>
            <button className="primaryButton" disabled={outdatedCodexAliases.length === 0 || anyHostBusy} type="button" onClick={() => void onUpdateOutdatedCodexHosts(outdatedCodexAliases)}>
              {updatingOutdated ? copy.hosts.updatingOutdatedCodex : copy.hosts.updateOutdatedCodex}
            </button>
          </CommandBar>
        </div>

        {sshConfigHosts.length === 0 ? (
          <EmptyListState
            action={<button className="primaryButton" type="button" onClick={() => void onOpenSetupGuide()}>{copy.hosts.detectLocalConfig}</button>}
            copy={copy}
            message={copy.emptyLists.hosts}
            variant="hosts"
          />
        ) : (
          <div className="tableWrap">
            <table className="sshHostsTable">
              <thead>
                <tr>
                  <th className="sshHostsAliasCol">{copy.hosts.alias}</th>
                  <th className="sshHostsSourceCol">{copy.hosts.source}</th>
                  <th className="sshHostsAddressCol">{copy.hosts.hostName}</th>
                  <th className="sshHostsPortCol">{copy.hosts.port}</th>
                  <th className="sshHostsUserCol">{copy.hosts.user}</th>
                  <th className="sshHostsVersionCol">{copy.hosts.codexVersion}</th>
                  <th className="sshHostsLatestVersionCol">{copy.hosts.latestCodexVersion}</th>
                  <th className="sshHostsActionsCol">{copy.hosts.actions}</th>
                  <th className="sshHostsCodexCol">{copy.hosts.codex}</th>
                </tr>
              </thead>
              <tbody>
                {sshConfigHosts.map((sshHost) => {
                  const host = hostByAlias.get(sshHost.alias.toLowerCase()) ?? null;
                  const busy = hostBusy[sshHost.alias];
                  const codexStatus = hostCodexStatus(copy, host, busy, hosts, latestCodexVersion);
                  const latestStatus = latestCodexStatus(copy, latestCodexVersion);
                  const codexTested = isHostCodexTested(host);
                  const installDisabled = Boolean(busy) || !codexTested || Boolean(host?.codexInstalled);
                  const updateDisabled = Boolean(busy) || !codexTested || !host?.codexInstalled || !isCodexVersionBehind(host.codexVersion, latestCodexVersion?.version);
                  const uninstallDisabled = Boolean(busy) || !codexTested || !host?.codexInstalled;

                  return (
                    <tr className="selectableRow" data-selected={selectedHostAlias === sshHost.alias} key={sshHost.alias} onClick={() => setSelectedHostAlias(sshHost.alias)}>
                      <td className="sshHostsAliasCol"><strong>{sshHost.alias}</strong></td>
                      <td className="sshHostsSourceCol"><Badge tone={sshHost.managed ? "blue" : "gray"}>{sshHostSourceLabel(copy, sshHost)}</Badge></td>
                      <td className="sshHostsAddressCol">{sshHost.hostName}</td>
                      <td className="sshHostsPortCol">{sshHost.port}</td>
                      <td className="sshHostsUserCol">{sshHost.user}</td>
                      <td className="sshHostsVersionCol"><Badge tone={codexStatus.tone}>{codexStatus.label}</Badge></td>
                      <td className="sshHostsLatestVersionCol"><Badge tone={latestStatus.tone} title={latestStatus.title}>{latestStatus.label}</Badge></td>
                      <td className="sshHostsActionsCol">
                        <CommandGroup className="tableActions sshHostsActionGroup">
                          <button className="miniButton" disabled={Boolean(busy)} type="button" onClick={(event) => { event.stopPropagation(); onTestHost(sshHost.alias); }}>
                            {busy === "test" ? copy.hosts.testing : copy.hosts.test}
                          </button>
                          <button className="miniButton" disabled={Boolean(busy)} type="button" onClick={(event) => { event.stopPropagation(); handleEdit(sshHost); }}>{copy.hosts.edit}</button>
                          <button className="miniButton danger" disabled={Boolean(busy)} type="button" onClick={(event) => { event.stopPropagation(); setDeleteHostAlias(sshHost.alias); }}>{copy.hosts.delete}</button>
                        </CommandGroup>
                      </td>
                      <td className="sshHostsCodexCol">
                        <CommandGroup className="tableActions sshHostsActionGroup">
                          <button className="miniButton" disabled={installDisabled} type="button" onClick={(event) => { event.stopPropagation(); onManageCodex(sshHost.alias, "install"); }}>
                            {remoteCodexButtonLabel(copy, busy, "install")}
                          </button>
                          <button className="miniButton" disabled={updateDisabled} type="button" onClick={(event) => { event.stopPropagation(); onManageCodex(sshHost.alias, "update"); }}>
                            {remoteCodexButtonLabel(copy, busy, "update")}
                          </button>
                          <button className="miniButton danger" disabled={uninstallDisabled} type="button" onClick={(event) => { event.stopPropagation(); onManageCodex(sshHost.alias, "uninstall"); }}>
                            {remoteCodexButtonLabel(copy, busy, "uninstall")}
                          </button>
                        </CommandGroup>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <HostDetailsPanel copy={copy} host={selectedHost} hosts={hosts} inventoryStatus={inventoryStatus} latestCodexVersion={latestCodexVersion} />
      {deleteHostAlias ? (
        <SimpleDeleteConfirmModal
          busy={deleteHostBusy}
          body={copy.hosts.deleteConfirm(deleteHostAlias)}
          copy={copy}
          title={`${copy.hosts.delete}: ${deleteHostAlias}`}
          onClose={() => setDeleteHostAlias(null)}
          onDelete={() => void handleDelete().catch(reportActionError)}
        />
      ) : null}
    </div>
  );
}

function SshHostModal({
  copy,
  defaultIdentityFile,
  initialDraft,
  open,
  sshBusy,
  sshStatus,
  onClose,
  onConnect,
  onGenerateEd25519Key
}: {
  copy: UICopy;
  defaultIdentityFile: string;
  initialDraft: SshHostDraft | null;
  open: boolean;
  sshBusy: boolean;
  sshStatus: SshStatus | null;
  onClose: () => void;
  onConnect: (draft: SshHostDraft, password: string, requestId: string, onProgress: (event: SshBootstrapProgressEvent) => void) => Promise<SshBootstrapResult>;
  onGenerateEd25519Key: () => Promise<unknown>;
}) {
  const [draft, setDraft] = useState<SshHostDraft>(() => initialDraft ?? emptySshHostDraft(defaultIdentityFile));
  const [password, setPassword] = useState("");
  const [passwordVisible, setPasswordVisible] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [message, setMessage] = useState("");
  const [messageTone, setMessageTone] = useState<"info" | "success" | "error">("info");
  const [confirmCloseOpen, setConfirmCloseOpen] = useState(false);
  const [showProgress, setShowProgress] = useState(false);
  const [steps, setSteps] = useState(() => createInitialBootstrapSteps(copy));
  const hasIdentityFile = Boolean(defaultIdentityFile);
  const canGenerateKey = Boolean(sshStatus?.sshKeygenAvailable && !sshStatus.ed25519.privateExists && !sshStatus.ed25519.publicExists);
  const canConnect = Boolean(draft.alias.trim() && draft.hostName.trim() && draft.port > 0 && draft.user.trim() && password && hasIdentityFile && !connecting);

  useEffect(() => {
    if (!open) return;
    const nextDraft = initialDraft ?? emptySshHostDraft(defaultIdentityFile);
    setDraft({ ...nextDraft, identityFile: nextDraft.identityFile || defaultIdentityFile });
    setPassword("");
    setPasswordVisible(false);
    setConnecting(false);
    setMessage("");
    setMessageTone("info");
    setConfirmCloseOpen(false);
    setShowProgress(false);
    setSteps(createInitialBootstrapSteps(copy));
  }, [copy, defaultIdentityFile, initialDraft, open]);

  useEffect(() => {
    setDraft((current) => ({ ...current, identityFile: current.identityFile || defaultIdentityFile }));
  }, [defaultIdentityFile]);

  if (!open) return null;

  const updateDraft = (key: keyof SshHostDraft, value: string | number) => {
    setDraft((current) => ({ ...current, [key]: value }));
  };

  const closeModal = () => {
    if (connecting) {
      setConfirmCloseOpen(true);
      return;
    }
    onClose();
  };

  const handleGenerateKey = async () => {
    try {
      await onGenerateEd25519Key();
      setMessage(copy.hosts.keyCreated);
      setMessageTone("success");
    } catch (error) {
      setMessage(formatError(error));
      setMessageTone("error");
    }
  };

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!hasIdentityFile) {
      setMessage(copy.hosts.keyMissing);
      setMessageTone("error");
      return;
    }
    const requestId = `bootstrap-${Date.now()}-${draft.alias || draft.hostName}`;
    setConnecting(true);
    setShowProgress(true);
    setSteps(createInitialBootstrapSteps(copy));
    setMessage(copy.hosts.connectingProgress);
    setMessageTone("info");
    try {
      const result = await onConnect({ ...draft, identityFile: defaultIdentityFile }, password, requestId, (progress) => {
        setSteps((current) => updateBootstrapStep(current, progress));
      });
      if (!result.ok) {
        const detail = result.message || copy.hosts.connectionFailed;
        setMessage(detail);
        setMessageTone("error");
        setSteps((current) => markBootstrapFailureIfNeeded(current, detail, copy));
        return;
      }
      setMessage(copy.hosts.connectionSuccess);
      setMessageTone("success");
    } catch (error) {
      const detail = formatError(error);
      setMessage(detail);
      setMessageTone("error");
      setSteps((current) => markBootstrapFailureIfNeeded(current, detail, copy));
    } finally {
      setConnecting(false);
    }
  };
  const editing = Boolean(initialDraft);

  return (
    <div className="modalBackdrop" role="presentation">
      <ModalFrame className="sshHostModal" titleId="ssh-host-modal-title">
        <ModalHeader
          className="modalHero"
          titleId="ssh-host-modal-title"
          title={editing ? copy.hosts.edit : copy.hosts.addCodexHubHost}
          icon="hosts"
          closeAriaLabel={copy.setupGuide.close}
          onClose={closeModal}
        />

        {message ? <p className="sshHostMessage" data-tone={messageTone} role={messageTone === "error" ? "alert" : "status"}>{message}</p> : null}

        <form className="modalForm" onSubmit={handleSubmit}>
          <label className="fieldGroup">
            <span>{copy.hosts.hostAlias}</span>
            <input disabled={connecting} readOnly={editing} value={draft.alias} onChange={(event) => updateDraft("alias", event.target.value)} placeholder="HostAlias" required />
          </label>
          <label className="fieldGroup">
            <span>{copy.hosts.hostName}</span>
            <input disabled={connecting} value={draft.hostName} onChange={(event) => updateDraft("hostName", event.target.value)} placeholder="127.0.0.1" required />
          </label>
          <label className="fieldGroup">
            <span>{copy.hosts.port}</span>
            <input disabled={connecting} min={1} max={65535} type="number" value={draft.port} onChange={(event) => updateDraft("port", Number(event.target.value))} required />
          </label>
          <label className="fieldGroup">
            <span>{copy.hosts.user}</span>
            <input disabled={connecting} value={draft.user} onChange={(event) => updateDraft("user", event.target.value)} placeholder="Username" required />
          </label>
          <label className="fieldGroup">
            <span>{copy.hosts.bootstrapPassword}</span>
            <div className="passwordInputWrap">
              <input
                autoComplete="new-password"
                disabled={connecting}
                type={passwordVisible ? "text" : "password"}
                value={password}
                onChange={(event) => setPassword(event.target.value)}
                placeholder="Password"
                required
              />
              <button
                aria-label={passwordVisible ? copy.hosts.hidePassword : copy.hosts.showPassword}
                aria-pressed={passwordVisible}
                className="credentialVisibilityButton"
                title={passwordVisible ? copy.hosts.hidePassword : copy.hosts.showPassword}
                type="button"
                onClick={() => setPasswordVisible((current) => !current)}
              >
                <CredentialVisibilityIcon visible={passwordVisible} />
              </button>
            </div>
          </label>
          <div className="fieldGroup identityRow" data-has-action={!hasIdentityFile}>
            <span>IDFile</span>
            <input readOnly value={hasIdentityFile ? copy.hosts.identityDetected : copy.hosts.identityMissing} />
            {!hasIdentityFile ? (
              <button className="secondaryButton" disabled={!canGenerateKey || sshBusy || connecting} type="button" onClick={() => void handleGenerateKey()}>
                {sshBusy ? copy.settings.generating : copy.hosts.generateKey}
              </button>
            ) : null}
          </div>

          <ModalActions>
            <button className="primaryButton" disabled={!canConnect} type="submit">{connecting ? copy.hosts.writing : copy.hosts.writeSshConfig}</button>
          </ModalActions>
        </form>

        {showProgress ? <BootstrapProgressLog copy={copy} steps={steps} /> : null}
      </ModalFrame>
      <ConfirmDialog
        copy={{
          title: copy.hosts.closeWhileConnectingTitle,
          body: copy.hosts.closeWhileConnectingBody,
          cancel: copy.hosts.cancel,
          confirm: copy.hosts.closeWhileConnectingConfirm
        }}
        open={confirmCloseOpen}
        onCancel={() => setConfirmCloseOpen(false)}
        onConfirm={onClose}
      />
    </div>
  );
}

type BootstrapStepState = {
  step: SshBootstrapStep;
  label: string;
  status: SshBootstrapStepStatus;
  message: string;
  detail: string;
  stdout: string;
  stderr: string;
};

function createInitialBootstrapSteps(copy: UICopy): BootstrapStepState[] {
  return [
    { step: "password_login", label: copy.hosts.bootstrapSteps.password_login, status: "pending", message: copy.hosts.waitingToStart, detail: "", stdout: "", stderr: "" },
    { step: "install_public_key", label: copy.hosts.bootstrapSteps.install_public_key, status: "pending", message: copy.hosts.waitingToStart, detail: "", stdout: "", stderr: "" },
    { step: "set_permissions", label: copy.hosts.bootstrapSteps.set_permissions, status: "pending", message: copy.hosts.waitingToStart, detail: "", stdout: "", stderr: "" },
    { step: "verify_alias_login", label: copy.hosts.bootstrapSteps.verify_alias_login, status: "pending", message: copy.hosts.waitingToStart, detail: "", stdout: "", stderr: "" }
  ];
}

function updateBootstrapStep(steps: BootstrapStepState[], progress: SshBootstrapProgressEvent): BootstrapStepState[] {
  return steps.map((step) =>
    step.step === progress.step
      ? {
          ...step,
          status: progress.status,
          message: progress.message,
          detail: progress.detail ?? "",
          stdout: progress.stdout ?? "",
          stderr: progress.stderr ?? ""
        }
      : step
  );
}

function markBootstrapFailureIfNeeded(steps: BootstrapStepState[], detail: string, copy: UICopy): BootstrapStepState[] {
  if (steps.some((step) => step.status === "failed")) return steps;
  const firstRunning = steps.find((step) => step.status === "running")?.step ?? "password_login";
  return steps.map((step) => (step.step === firstRunning ? { ...step, status: "failed", message: copy.hosts.connectionFailed, detail } : step));
}

function BootstrapProgressLog({ copy, steps }: { copy: UICopy; steps: BootstrapStepState[] }) {
  return (
    <section className="bootstrapLogCard">
      <div className="bootstrapLogHeader">
        <strong>{copy.hosts.progressTitle}</strong>
        <span>{copy.hosts.progressSubtitle}</span>
      </div>
      <div className="bootstrapStepList">
        {steps.map((step) => (
          <article className="bootstrapStep" data-status={step.status} key={step.step}>
            <div className="bootstrapStepMain">
              <div>
                <strong>{step.label}</strong>
                <span>{step.message}</span>
              </div>
              <StepStatusIcon status={step.status} />
            </div>
            {step.status === "failed" ? (
              <div className="bootstrapFailureDetail">
                <strong>{copy.hosts.failureDetails}</strong>
                <pre>{step.stderr || step.detail || step.stdout || copy.hosts.noFailureDetails}</pre>
              </div>
            ) : null}
          </article>
        ))}
      </div>
    </section>
  );
}

function StepStatusIcon({ status }: { status: SshBootstrapStepStatus }) {
  if (status === "success") return <span className="stepIcon success">✓</span>;
  if (status === "failed") return <span className="stepIcon failed">×</span>;
  if (status === "running") return <span className="stepIcon running" aria-label="running" />;
  return <span className="stepIcon pending" />;
}
function HostDetailsPanel({
  copy,
  host,
  hosts,
  inventoryStatus,
  latestCodexVersion
}: {
  copy: UICopy;
  host: Host | null;
  hosts: Host[];
  inventoryStatus: SkillInventoryStatus;
  latestCodexVersion: LatestCodexVersion | null;
}) {
  const codexStatus = hostCodexStatus(copy, host, undefined, hosts, latestCodexVersion);
  const codexInstalledStatus = hostCodexInstalledStatus(copy, host);
  const hostInventoryByAlias = new Map(inventoryStatus.hostInventories.map((inventory) => [inventory.hostAlias.toLowerCase(), inventory]));
  const skillCounts = hosts.map((item) => dashboardHostSkillCount(item, hostInventoryByAlias.get(item.hostAlias.toLowerCase())));
  const currentSkillCount = host ? dashboardHostSkillCount(host, hostInventoryByAlias.get(host.hostAlias.toLowerCase())) : null;

  return (
    <section className="panel spanWide">
      <div className="panelHeader">
        <div>
          <TitleWithIcon icon="hosts" level={2}>{copy.hosts.detailsTitle(host?.hostAlias ?? copy.hosts.unknown)}</TitleWithIcon>
        </div>
        <div className="calloutMeta largeStatus">
          {host ? <StatusBadge copy={copy} status={host.status} /> : <Badge tone="gray">{copy.hosts.unknown}</Badge>}
        </div>
      </div>

      <dl className="detailGrid">
        <div>
          <dt>{copy.hosts.sshStatus}</dt>
          <dd>
            {host ? <StatusBadge copy={copy} status={host.status} /> : <HostDetailValueBadge label={copy.hosts.unknown} tone="gray" />}
          </dd>
        </div>
        <div>
          <dt>{copy.hosts.os}</dt>
          <dd><HostDetailValueBadge label={knownHostValue(host?.os, copy)} tone={knownValueTone(host?.os, copy)} /></dd>
        </div>
        <div>
          <dt>{copy.hosts.arch}</dt>
          <dd><HostDetailValueBadge label={knownHostValue(host?.arch, copy)} tone={archTone(host?.arch, copy)} /></dd>
        </div>
        <div>
          <dt>{copy.hosts.shell}</dt>
          <dd><HostDetailValueBadge label={knownHostValue(host?.shell, copy)} tone={knownValueTone(host?.shell, copy)} /></dd>
        </div>
        <div>
          <dt>{copy.hosts.latency}</dt>
          <dd><HostDetailValueBadge label={formatLatency(host?.latencyMs, copy)} tone={latencyTone(host?.latencyMs, hosts)} /></dd>
        </div>
        <div>
          <dt>{copy.hosts.codexInstalled}</dt>
          <dd><HostDetailValueBadge label={codexInstalledStatus.label} tone={codexInstalledStatus.tone} /></dd>
        </div>
        <div>
          <dt>{copy.hosts.codexVersion}</dt>
          <dd><HostDetailValueBadge label={codexStatus.label} tone={codexStatus.tone} /></dd>
        </div>
        <div>
          <dt>{copy.hosts.configExists}</dt>
          <dd>{host ? <HostApiConfigBadge copy={copy} host={host} /> : <HostDetailValueBadge label={copy.hosts.unknown} tone="gray" />}</dd>
        </div>
        <div>
          <dt>{copy.hosts.skillsCount}</dt>
          <dd><HostDetailValueBadge label={typeof currentSkillCount === "number" ? String(currentSkillCount) : copy.hosts.unknown} tone={dashboardSkillCountTone(currentSkillCount, skillCounts)} /></dd>
        </div>
      </dl>
    </section>
  );
}

function profileApplyEligibleHostIds(profile: Profile | null, hosts: Host[], hostIds: string[]) {
  if (!profile) return [];
  const requested = new Set(hostIds);
  // Only hosts not already confirmed on this API config should be sent to apply.
  return hosts.filter((host) => requested.has(host.id) && !profileMatchesConfirmedHostApiConfig(profile, host)).map((host) => host.id);
}

function profileApplyHostDisplayName(host: Host) {
  return host.name === host.hostAlias ? host.name : `${host.name} (${host.hostAlias})`;
}

function ProfilesView({
  copy,
  hosts,
  latestCodexVersion,
  profiles,
  newProfileRequest,
  onCreateProfile,
  onDeleteProfile,
  onDetectCcSwitchProfiles,
  onDuplicateProfile,
  onGetProfileApiKey,
  onImportCcSwitchProfiles,
  onImportProfiles,
  onPreviewProfileApply,
  onRunProfileApply,
  onSetProfileApiKey,
  onUpdateProfile
}: {
  copy: UICopy;
  hosts: Host[];
  latestCodexVersion: LatestCodexVersion | null;
  profiles: Profile[];
  newProfileRequest: number;
  onCreateProfile: (draft: ProfileDraft) => Promise<Profile>;
  onDeleteProfile: (id: string) => Promise<DeleteOperationResult>;
  onDetectCcSwitchProfiles: () => Promise<CcSwitchDetection>;
  onDuplicateProfile: (id: string) => Promise<Profile>;
  onGetProfileApiKey: (profileId: string) => Promise<ProfileApiKeyResult>;
  onImportCcSwitchProfiles: (detection: CcSwitchDetection) => Promise<ProfileImportExport>;
  onImportProfiles: (bundle: ProfileImportExport) => Promise<ProfileImportExport>;
  onPreviewProfileApply: (profileId: string, hostIds: string[]) => Promise<ProfileApplyPreview>;
  onRunProfileApply: (profileId: string, hostIds: string[]) => Promise<ProfileApplyBatchResult>;
  onSetProfileApiKey: (profileId: string, apiKey: string) => Promise<Profile>;
  onUpdateProfile: (id: string, patch: ProfilePatch) => Promise<Profile>;
}) {
  const reportActionError = useActionErrorReporter(copy);
  const importInputRef = useRef<HTMLInputElement | null>(null);
  const [selectedProfileId, setSelectedProfileId] = useState<string | null>(profiles[0]?.id ?? null);
  const selectedProfile = profiles.find((profile) => profile.id === selectedProfileId) ?? profiles[0] ?? null;
  const [selectedHostIds, setSelectedHostIds] = useState<string[]>([]);
  const [preview, setPreview] = useState<ProfileApplyPreview | null>(null);
  const [applyResult, setApplyResult] = useState<ProfileApplyBatchResult | null>(null);
  const [profileApplyOperation, setProfileApplyOperation] = useState<ProfileApplyOperationModalState | null>(null);
  const [profileApplyRunningHostIds, setProfileApplyRunningHostIds] = useState<string[]>([]);
  const [profileEditorOpen, setProfileEditorOpen] = useState(false);
  const [editingProfileId, setEditingProfileId] = useState<string | null>(null);
  const [hostPickerProfileId, setHostPickerProfileId] = useState<string | null>(null);
  const [deleteProfileId, setDeleteProfileId] = useState<string | null>(null);
  const [previewModalOpen, setPreviewModalOpen] = useState(false);
  const lastNewProfileRequestRef = useRef(newProfileRequest);
  const [ccDetection, setCcDetection] = useState<CcSwitchDetection | null>(null);
  const [ccDetectionError, setCcDetectionError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const editingProfile = editingProfileId ? profiles.find((profile) => profile.id === editingProfileId) ?? null : null;
  const hostPickerProfile = hostPickerProfileId ? profiles.find((profile) => profile.id === hostPickerProfileId) ?? null : null;
  const deleteProfile = deleteProfileId ? profiles.find((profile) => profile.id === deleteProfileId) ?? null : null;
  const profileById = useMemo(() => new Map(profiles.map((profile) => [profile.id, profile])), [profiles]);
  const appliedHostCountByProfileId = useMemo(() => {
    const counts = new Map<string, Set<string>>();
    const addHost = (profileId: string, hostId: string) => {
      const profileHosts = counts.get(profileId) ?? new Set<string>();
      profileHosts.add(hostId);
      counts.set(profileId, profileHosts);
    };
    for (const host of hosts) {
      for (const profile of profiles) {
        if (profileMatchesConfirmedHostApiConfig(profile, host)) {
          addHost(profile.id, host.id);
        }
      }
    }
    return new Map(Array.from(counts.entries()).map(([profileId, profileHosts]) => [profileId, profileHosts.size]));
  }, [hosts, profiles]);
  const selectedApplyHostIds = useMemo(
    () => profileApplyEligibleHostIds(selectedProfile, hosts, selectedHostIds),
    [hosts, selectedHostIds, selectedProfile]
  );
  const profileApplyRunningHostIdSet = useMemo(() => new Set(profileApplyRunningHostIds), [profileApplyRunningHostIds]);

  useEffect(() => {
    if (!selectedProfileId && profiles[0]) setSelectedProfileId(profiles[0].id);
    if (selectedProfileId && !profiles.some((profile) => profile.id === selectedProfileId)) {
      setSelectedProfileId(profiles[0]?.id ?? null);
    }
  }, [profiles, selectedProfileId]);

  useEffect(() => {
    setSelectedHostIds([]);
    setPreview(null);
    setApplyResult(null);
  }, [selectedProfile?.id]);

  useEffect(() => {
    if (newProfileRequest === lastNewProfileRequestRef.current) return;
    lastNewProfileRequestRef.current = newProfileRequest;
    if (newProfileRequest <= 0) return;
    setEditingProfileId(null);
    setProfileEditorOpen(true);
  }, [newProfileRequest]);

  const runBusy = async <T,>(label: string, action: () => Promise<T>) => {
    setBusy(label);
    try {
      return await action();
    } finally {
      setBusy(null);
    }
  };

  const handleSaveProfile = async (profile: Profile | null, draft: ProfileDraft) => {
    const saved = profile
      ? await runBusy("save", () => onUpdateProfile(profile.id, draft))
      : await runBusy("create", () => onCreateProfile(draft));
    setSelectedProfileId(saved.id);
    return saved;
  };

  const handleDelete = async (profile: Profile) => {
    await runBusy("delete", () => onDeleteProfile(profile.id));
    setDeleteProfileId(null);
  };

  const handleDuplicate = async (profile: Profile) => {
    const duplicate = await runBusy("duplicate", () => onDuplicateProfile(profile.id));
    setSelectedProfileId(duplicate.id);
  };

  const handleImportFile = async (file: File | undefined) => {
    if (!file) return;
    setCcDetectionError(null);
    const text = await file.text();
    const parsed = JSON.parse(text) as ProfileImportExport | Profile[];
    const bundle: ProfileImportExport = Array.isArray(parsed)
      ? { schemaVersion: 1, exportedAt: new Date().toISOString(), profiles: parsed }
      : parsed;
    const imported = await runBusy("import", () => onImportProfiles(bundle));
    setSelectedProfileId(imported.profiles[0]?.id ?? selectedProfileId);
    if (importInputRef.current) importInputRef.current.value = "";
  };

  const handleDetectCcSwitch = async () => {
    setCcDetectionError(null);
    try {
      const detection = await runBusy("detect", onDetectCcSwitchProfiles);
      setCcDetection(detection);
    } catch (error) {
      setCcDetection(null);
      setCcDetectionError(formatError(error));
    }
  };

  const handleImportDetected = async () => {
    if (!ccDetection) return;
    setCcDetectionError(null);
    const imported = await runBusy("import-cc", () => onImportCcSwitchProfiles(ccDetection));
    setSelectedProfileId(imported.profiles[0]?.id ?? selectedProfileId);
    setCcDetection(null);
  };

  const handleStoreCredential = async (profileId: string, apiKey: string) => {
    const updated = await runBusy("store-key", () => onSetProfileApiKey(profileId, apiKey));
    setSelectedProfileId(updated.id);
    return updated;
  };

  const handlePreview = async (hostIds = selectedHostIds) => {
    if (!selectedProfile || hostIds.length === 0) return;
    setSelectedHostIds(hostIds);
    setPreviewModalOpen(true);
    setPreview(null);
    setApplyResult(null);
    const result = await runBusy("preview", () => onPreviewProfileApply(selectedProfile.id, hostIds));
    setPreview(result);
  };

  const runProfileApply = async (profile: Profile, hostIds: string[]) => {
    const targetHostIds = profileApplyEligibleHostIds(profile, hosts, hostIds);
    if (targetHostIds.length === 0) return null;
    const targetHostIdSet = new Set(targetHostIds);
    const targetHosts = hosts.filter((host) => targetHostIdSet.has(host.id));
    const requestId = `profile-apply-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    const hostNames = targetHosts.map(profileApplyHostDisplayName);
    setSelectedProfileId(profile.id);
    setSelectedHostIds(targetHostIds);
    setApplyResult(null);
    setProfileApplyRunningHostIds((current) => Array.from(new Set([...current, ...targetHostIds])));
    setHostPickerProfileId((current) => (current === profile.id ? null : current));
    setPreviewModalOpen(false);
    setProfileApplyOperation({
      requestId,
      profileName: profile.name,
      hostNames,
      status: "running",
      tasks: [],
      message: copy.profiles.applyOperationStarted(profile.name, targetHostIds.length),
      logs: [
        { level: "info", message: copy.profiles.applyOperationStarted(profile.name, targetHostIds.length) },
        { level: "info", message: copy.profiles.applyOperationWaiting }
      ]
    });
    await waitForNextFrame();
    try {
      const result = await onRunProfileApply(profile.id, targetHostIds);
      setApplyResult(result);
      setPreview((current) => (current && current.profileId === profile.id ? { ...current, hostResults: result.results } : current));
      const completedCount = result.results.filter((row) => row.status === "success" || row.status === "no-change").length;
      const failedMessage = result.results.find((row) => row.status === "failed")?.message;
      const resultLogs = result.results.map((row) => ({
        level: row.status === "failed" ? ("error" as const) : ("info" as const),
        message: `${row.hostName}: ${row.message}`
      }));
      setProfileApplyOperation((current) =>
        current?.requestId === requestId
          ? {
              ...current,
              status: result.ok ? "success" : "failed",
              tasks: result.tasks,
              message: result.ok
                ? copy.profiles.applyOperationSuccess(profile.name, completedCount)
                : failedMessage ?? copy.profiles.applyOperationFailed,
              logs: [...current.logs, ...resultLogs].slice(-80)
            }
          : current
      );
      return result;
    } catch (error) {
      const errorMessage = formatError(error);
      setProfileApplyOperation((current) =>
        current?.requestId === requestId
          ? {
              ...current,
              status: "failed",
              error: errorMessage,
              logs: [...current.logs, { level: "error" as const, message: errorMessage }].slice(-80)
            }
          : current
      );
      return null;
    } finally {
      setProfileApplyRunningHostIds((current) => current.filter((hostId) => !targetHostIdSet.has(hostId)));
    }
  };

  const handleApply = async (hostIds = selectedHostIds) => {
    if (!selectedProfile || hostIds.length === 0) return null;
    return runProfileApply(selectedProfile, hostIds);
  };

  const previewWithResults = preview && applyResult ? { ...preview, hostResults: applyResult.results } : preview;
  const ccSwitchStatus = busy === "detect"
    ? copy.profiles.ccSwitchChecking
    : ccDetectionError ?? (ccDetection
      ? ccDetection.detected
        ? copy.profiles.ccSwitchFound(ccDetection.importExport.profiles.length)
        : copy.profiles.ccSwitchNone
      : "");
  const canImportCcSwitchDetection = Boolean(ccDetection?.detected);

  return (
    <div className="profilesStack">
      <section className="panel spanWide">
        <div className="panelHeader">
          <div>
            <TitleWithIcon icon="profiles" level={2}>{copy.profiles.library}</TitleWithIcon>
          </div>
          <CommandBar ariaLabel={copy.profiles.library} className="topActions profileLibraryActions">
            <button className="secondaryButton" type="button" onClick={() => importInputRef.current?.click()}>{copy.profiles.import}</button>
            <button
              className={`${canImportCcSwitchDetection ? "primaryButton" : "secondaryButton"} ccSwitchActionButton`}
              disabled={canImportCcSwitchDetection ? busy === "import-cc" : busy === "detect"}
              type="button"
              onClick={() => void (canImportCcSwitchDetection ? handleImportDetected() : handleDetectCcSwitch())}
            >
              {canImportCcSwitchDetection ? copy.profiles.importDetected : copy.profiles.detectCcSwitch}
            </button>
            <input
              ref={importInputRef}
              hidden
              type="file"
              accept="application/json,.json"
              onChange={(event) => void handleImportFile(event.currentTarget.files?.[0])}
            />
          </CommandBar>
        </div>

        {ccSwitchStatus ? (
          <div className="profileCcSwitchStatus" role="status">
            <Badge tone={ccDetectionError ? "red" : ccDetection?.detected ? "green" : busy === "detect" ? "blue" : "gray"}>
              cc-switch
            </Badge>
            <span>{ccSwitchStatus}</span>
            {ccDetection?.sourcePath ? <code>{ccDetection.sourcePath}</code> : null}
          </div>
        ) : null}

        {profiles.length === 0 ? (
          <EmptyListState copy={copy} message={copy.emptyLists.profiles} variant="profiles" />
        ) : (
          <div className="tableWrap">
            <table className="profilesTable profileTable">
              <thead>
                <tr>
                  <th>{copy.profiles.name}</th>
                  <th>{copy.profiles.model}</th>
                  <th>{copy.profiles.provider}</th>
                  <th>{copy.profiles.apiKey}</th>
                  <th>{copy.profiles.hosts}</th>
                  <th>{copy.profiles.actions}</th>
                  <th>{copy.profiles.applyColumn}</th>
                </tr>
              </thead>
              <tbody>
                {profiles.map((profile) => (
                  <tr
                    className="selectableRow"
                    data-selected={selectedProfile?.id === profile.id}
                    key={profile.id}
                    onClick={() => setSelectedProfileId(profile.id)}
                  >
                    <td><strong>{profile.name}</strong></td>
                    <td>{profile.model}</td>
                    <td>{profile.provider}</td>
                    <td><ProfileStorageBadge copy={copy} profile={profile} /></td>
                    <td>{appliedHostCountByProfileId.get(profile.id) ?? 0}</td>
                    <td>
                      <CommandGroup className="profileRowActions" onClick={(event) => event.stopPropagation()}>
                        <button className="miniButton" type="button" onClick={() => {
                          setEditingProfileId(profile.id);
                          setProfileEditorOpen(true);
                        }}>{copy.profiles.edit}</button>
                        <button className="miniButton" disabled={busy === "duplicate"} type="button" onClick={() => void handleDuplicate(profile)}>{copy.profiles.duplicate}</button>
                        <button className="miniButton danger" disabled={busy === "delete"} type="button" onClick={() => setDeleteProfileId(profile.id)}>{copy.profiles.delete}</button>
                      </CommandGroup>
                    </td>
                    <td>
                      <button
                        className="miniButton"
                        disabled={hosts.length === 0}
                        type="button"
                        onClick={(event) => {
                          event.stopPropagation();
                          setSelectedProfileId(profile.id);
                          setHostPickerProfileId(profile.id);
                        }}
                      >
                        {copy.profiles.selectHosts}
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <section className="panel spanWide profileApplyPanel">
        <div className="panelHeader compact">
          <div>
            <TitleWithIcon icon="profiles" level={2}>{copy.profiles.applyConfig}</TitleWithIcon>
          </div>
        </div>

        {hosts.length === 0 ? (
          <EmptyListState copy={copy} message={copy.emptyLists.profileHosts} variant="hosts" />
        ) : (
          <div className="tableWrap">
            <table className="sshHostsTable profileApplyTable">
              <thead>
                <tr>
                  <th className="sshHostsAliasCol">{copy.hosts.alias}</th>
                  <th className="sshHostsSourceCol">{copy.hosts.source}</th>
                  <th className="sshHostsAddressCol">{copy.hosts.hostName}</th>
                  <th className="sshHostsVersionCol">{copy.hosts.codexVersion}</th>
                  <th className="profileApplyConfigCol">{copy.profiles.apiConfig}</th>
                  <th className="sshHostsActionsCol">{copy.hosts.actions}</th>
                </tr>
              </thead>
              <tbody>
                {hosts.map((host) => {
                  const alreadyApplied = selectedProfile ? profileMatchesConfirmedHostApiConfig(selectedProfile, host) : false;
                  const codexStatus = hostCodexStatus(copy, host, undefined, hosts, latestCodexVersion);
                  return (
                    <tr
                      key={host.id}
                    >
                      <td className="sshHostsAliasCol">
                        <strong>{host.hostAlias}</strong>
                      </td>
                      <td className="sshHostsSourceCol"><Badge tone={host.source === "managed" ? "blue" : "gray"}>{hostSourceLabel(copy, host)}</Badge></td>
                      <td className="sshHostsAddressCol">{host.address}</td>
                      <td className="sshHostsVersionCol"><Badge tone={codexStatus.tone}>{codexStatus.label}</Badge></td>
                      <td className="profileApplyConfigCol"><HostApiConfigBadge copy={copy} host={host} profileById={profileById} /></td>
                      <td className="sshHostsActionsCol">
                        <CommandGroup className="tableActions sshHostsActionGroup">
                          <button className="miniButton" disabled={!selectedProfile || busy === "preview"} type="button" onClick={(event) => { event.stopPropagation(); void handlePreview([host.id]); }}>
                            {copy.profiles.previewApply}
                          </button>
                          <button className="miniButton" disabled={!selectedProfile || alreadyApplied || profileApplyRunningHostIdSet.has(host.id)} type="button" onClick={(event) => { event.stopPropagation(); void handleApply([host.id]); }}>
                            {copy.profiles.applyOne}
                          </button>
                        </CommandGroup>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <ProfileEditModal
        busy={busy}
        copy={copy}
        open={profileEditorOpen}
        profile={editingProfile}
        onClose={() => setProfileEditorOpen(false)}
        onGetCredential={onGetProfileApiKey}
        onSave={handleSaveProfile}
        onStoreCredential={handleStoreCredential}
      />

      <ProfileHostSelectModal
        applyingHostIds={profileApplyRunningHostIds}
        copy={copy}
        hosts={hosts}
        open={Boolean(hostPickerProfile)}
        profile={hostPickerProfile}
        profileById={profileById}
        onApply={(profile, hostIds) => runProfileApply(profile, hostIds)}
        onClose={() => setHostPickerProfileId(null)}
      />

      <ProfileApplyPreviewModal
        busy={busy}
        copy={copy}
        open={previewModalOpen}
        preview={previewWithResults}
        selectedCount={selectedApplyHostIds.length}
        onApplySelected={() => void handleApply()}
        onClose={() => setPreviewModalOpen(false)}
      />
      {profileApplyOperation ? (
        <ProfileApplyOperationModal
          copy={copy}
          operation={profileApplyOperation}
          onClose={() => setProfileApplyOperation(null)}
        />
      ) : null}
      {deleteProfile ? (
        <SimpleDeleteConfirmModal
          busy={busy === "delete"}
          body={copy.profiles.deleteConfirm(deleteProfile.name)}
          copy={copy}
          title={`${copy.profiles.delete}: ${deleteProfile.name}`}
          onClose={() => setDeleteProfileId(null)}
          onDelete={() => void handleDelete(deleteProfile).catch(reportActionError)}
        />
      ) : null}
    </div>
  );
}

function ProfileHostSelectModal({
  applyingHostIds,
  copy,
  hosts,
  open,
  profile,
  profileById,
  onApply,
  onClose
}: {
  applyingHostIds: string[];
  copy: UICopy;
  hosts: Host[];
  open: boolean;
  profile: Profile | null;
  profileById: Map<string, Profile>;
  onApply: (profile: Profile, hostIds: string[]) => Promise<ProfileApplyBatchResult | null>;
  onClose: () => void;
}) {
  const [selectedHostIds, setSelectedHostIds] = useState<string[]>([]);
  const applyingHostIdSet = useMemo(() => new Set(applyingHostIds), [applyingHostIds]);
  const eligibleHosts = useMemo(
    () => profile ? hosts.filter((host) => !profileMatchesConfirmedHostApiConfig(profile, host) && !applyingHostIdSet.has(host.id)) : [],
    [applyingHostIdSet, hosts, profile]
  );
  const eligibleHostIds = useMemo(() => eligibleHosts.map((host) => host.id), [eligibleHosts]);
  const eligibleHostIdSet = useMemo(() => new Set(eligibleHostIds), [eligibleHostIds]);
  const selectedEligibleHostIds = useMemo(
    () => selectedHostIds.filter((hostId) => eligibleHostIdSet.has(hostId)),
    [eligibleHostIdSet, selectedHostIds]
  );

  useEffect(() => {
    if (!open || !profile) return;
    setSelectedHostIds([]);
  }, [open, profile?.id]);

  if (!open || !profile) return null;

  const toggleHost = (host: Host) => {
    if (profileMatchesConfirmedHostApiConfig(profile, host) || applyingHostIdSet.has(host.id)) return;
    const hostId = host.id;
    setSelectedHostIds((current) => (current.includes(hostId) ? current.filter((id) => id !== hostId) : [...current, hostId]));
  };

  const handleApply = () => {
    if (selectedEligibleHostIds.length === 0) return;
    void onApply(profile, selectedEligibleHostIds);
    onClose();
  };

  return (
    <div className="modalBackdrop" role="presentation">
      <ModalFrame className="sshHostModal profileHostSelectModal ProfileHostSelectModal" titleId="profile-host-select-title">
        <ModalHeader
          className="modalHero"
          titleId="profile-host-select-title"
          title={copy.profiles.selectHosts}
          icon="profiles"
          closeAriaLabel={copy.setupGuide.close}
          onClose={onClose}
        />

        <div className="profileHostSelectList">
          {hosts.map((host) => {
            const selected = selectedHostIds.includes(host.id);
            const alreadyApplied = profileMatchesConfirmedHostApiConfig(profile, host);
            const applying = applyingHostIdSet.has(host.id);
            const disabled = alreadyApplied || applying;
            return (
              <label className="profileHostSelectRow" data-disabled={disabled} data-selected={selected} key={host.id}>
                <input checked={selected} disabled={disabled} type="checkbox" onChange={() => toggleHost(host)} />
                <strong>{host.name}</strong>
                <span className="profileHostSelectStatus">
                  <HostApiConfigBadge copy={copy} host={host} profileById={profileById} />
                  {alreadyApplied ? <Badge tone="green">{copy.profiles.alreadyApplied}</Badge> : null}
                </span>
              </label>
            );
          })}
        </div>

        <ModalActions className="profileHostSelectActions">
          <button className="secondaryButton" disabled={eligibleHosts.length === 0} type="button" onClick={() => {
            setSelectedHostIds(eligibleHostIds);
          }}>
            {copy.profiles.selectAll}
          </button>
          <button className="primaryButton" disabled={selectedEligibleHostIds.length === 0} type="button" onClick={handleApply}>
            {copy.profiles.applySelected}
          </button>
        </ModalActions>
      </ModalFrame>
    </div>
  );
}

function ProfileApplyOperationModal({
  copy,
  operation,
  onClose
}: {
  copy: UICopy;
  operation: ProfileApplyOperationModalState;
  onClose: () => void;
}) {
  const statusTone = operation.status === "success" ? "green" : operation.status === "failed" ? "red" : "yellow";
  const logRows = operation.logs.slice(-24);
  const logRowsRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const logRowsElement = logRowsRef.current;
    if (!logRowsElement) return;
    logRowsElement.scrollTop = logRowsElement.scrollHeight;
  }, [operation.logs.length, operation.status]);

  return (
    <div className="modalBackdrop" role="presentation">
      <ModalFrame className="codexOperationModal profileApplyOperationModal ProfileApplyOperationModal" titleId="profile-apply-operation-title">
        <ModalHeader
          className="codexOperationHeader"
          titleId="profile-apply-operation-title"
          title={copy.profiles.applyOperationTitle}
          icon="profiles"
          badge={<Badge tone={statusTone}>{copy.codexOperation[operation.status]}</Badge>}
          closeAriaLabel={operation.status === "running" ? copy.codexOperation.hide : copy.codexOperation.close}
          onClose={onClose}
        />

        <div className="codexOperationSummary">
          <span>{copy.codexOperation.summary}</span>
          <strong>{operation.message ?? operation.error ?? copy.profiles.applyOperationWaiting}</strong>
          <small>{operation.hostNames.join(", ")}</small>
        </div>

        <div className="codexOperationLog">
          <div className="codexOperationLogTitle">
            <span>{copy.codexOperation.latestLog}</span>
            {operation.status === "running" ? <i aria-hidden="true" /> : null}
          </div>
          <div className="codexOperationLogRows" ref={logRowsRef}>
            {logRows.length > 0 ? logRows.map((log, index) => (
              <div className="codexOperationLogRow" data-level={log.level} key={`${log.message}-${index}`}>
                <strong>{copy.status.log[log.level]}</strong>
                <span>{log.message}</span>
              </div>
            )) : (
              <div className="codexOperationLogRow" data-level={operation.status === "failed" ? "error" : "info"}>
                <strong>{operation.status === "failed" ? copy.status.log.error : copy.status.log.info}</strong>
                <span>{operation.error ?? copy.profiles.applyOperationWaiting}</span>
              </div>
            )}
          </div>
        </div>

        <ModalActions className="codexOperationActions">
          <button className="secondaryButton" type="button" onClick={onClose}>
            {operation.status === "running" ? copy.codexOperation.hide : copy.codexOperation.close}
          </button>
        </ModalActions>
      </ModalFrame>
    </div>
  );
}

function ProfileEditModal({
  busy,
  copy,
  open,
  profile,
  onClose,
  onGetCredential,
  onSave,
  onStoreCredential
}: {
  busy: string | null;
  copy: UICopy;
  open: boolean;
  profile: Profile | null;
  onClose: () => void;
  onGetCredential: (profileId: string) => Promise<ProfileApiKeyResult>;
  onSave: (profile: Profile | null, draft: ProfileDraft) => Promise<Profile>;
  onStoreCredential: (profileId: string, apiKey: string) => Promise<Profile>;
}) {
  const [draft, setDraft] = useState<ProfileDraft>(() => profileToDraft(profile));
  const [credentialInput, setCredentialInput] = useState("");
  const [credentialVisible, setCredentialVisible] = useState(false);
  const [credentialLoaded, setCredentialLoaded] = useState(false);
  const [credentialLoading, setCredentialLoading] = useState(false);
  const [credentialError, setCredentialError] = useState<string | null>(null);
  const canLoadStoredCredential = Boolean(profile?.credentialStored || profile?.source === "cc-switch");

  useEffect(() => {
    if (!open) return;
    setDraft(profileToDraft(profile));
    setCredentialInput("");
    setCredentialVisible(false);
    setCredentialLoaded(false);
    setCredentialLoading(false);
    setCredentialError(null);
  }, [open, profile]);

  if (!open) return null;

  const updateDraft = <K extends keyof ProfileDraft>(key: K, value: ProfileDraft[K]) => {
    setDraft((current) => ({ ...current, [key]: value }));
  };

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const saved = await onSave(profile, profile ? draft : profileDraftWithCreateDefaults(draft));
    if (credentialInput.trim() && !credentialLoaded) {
      await onStoreCredential(saved.id, credentialInput.trim());
    }
    onClose();
  };

  const handleToggleCredentialVisible = async () => {
    if (credentialLoading) return;
    if (credentialVisible) {
      setCredentialVisible(false);
      return;
    }
    // Read the stored value only after an explicit reveal action; never place it in app caches.
    if (profile && canLoadStoredCredential && !credentialLoaded && !credentialInput.trim()) {
      setCredentialLoading(true);
      setCredentialError(null);
      try {
        const result = await onGetCredential(profile.id);
        if (!result.apiKey) {
          setCredentialError(copy.profiles.apiKeyMissing);
          return;
        }
        setCredentialInput(result.apiKey);
        setCredentialLoaded(true);
      } catch (error) {
        setCredentialError(formatError(error));
        return;
      } finally {
        setCredentialLoading(false);
      }
    }
    setCredentialVisible(true);
  };

  return (
    <div className="modalBackdrop" role="presentation">
      <ModalFrame className="sshHostModal profileEditModal ProfileEditModal" titleId="profile-edit-modal-title">
        <ModalHeader
          className="modalHero"
          titleId="profile-edit-modal-title"
          title={profile ? copy.profiles.editor : copy.profiles.newProfile}
          icon="profiles"
          closeAriaLabel={copy.setupGuide.close}
          onClose={onClose}
        />

        <form className="modalForm profileModalForm" onSubmit={handleSubmit}>
          <label className="fieldGroup">
            <span>{copy.profiles.name}</span>
            <input value={draft.name} onChange={(event) => updateDraft("name", event.target.value)} placeholder={copy.profiles.namePlaceholder} required />
          </label>
          <div className="fieldGroup">
            <span>{copy.profiles.model}</span>
            <ProfileModelCombobox value={draft.model} placeholder={copy.profiles.modelPlaceholder} onChange={(value) => updateDraft("model", value)} />
          </div>
          <label className="fieldGroup">
            <span>{copy.profiles.provider}</span>
            <input value={draft.provider} onChange={(event) => updateDraft("provider", event.target.value)} placeholder={copy.profiles.providerPlaceholder} />
          </label>
          <label className="fieldGroup">
            <span>{copy.profiles.baseUrl}</span>
            <input value={draft.baseUrl} onChange={(event) => updateDraft("baseUrl", event.target.value)} placeholder={copy.profiles.baseUrlPlaceholder} />
          </label>
          <div className="profileCredentialRow">
            <div className="profileCredentialLabel">
              <span>{copy.profiles.apiKey}</span>
            </div>
            <div className="passwordInputWrap profileCredentialInputWrap">
              <input
                autoComplete="new-password"
                placeholder={canLoadStoredCredential && !credentialLoaded ? copy.profiles.apiKeyStoredPlaceholder : copy.profiles.apiKeyPlaceholder}
                type={credentialVisible ? "text" : "password"}
                value={credentialInput}
                onChange={(event) => {
                  setCredentialInput(event.target.value);
                  setCredentialLoaded(false);
                  setCredentialError(null);
                }}
              />
              <button
                aria-label={credentialVisible ? copy.profiles.hideApiKey : copy.profiles.showApiKey}
                aria-pressed={credentialVisible}
                className="credentialVisibilityButton"
                disabled={credentialLoading}
                title={credentialVisible ? copy.profiles.hideApiKey : copy.profiles.showApiKey}
                type="button"
                onClick={() => void handleToggleCredentialVisible()}
              >
                <CredentialVisibilityIcon visible={credentialVisible} />
              </button>
            </div>
            {credentialLoading || credentialError ? (
              <p className="profileCredentialHint">{credentialLoading ? copy.profiles.apiKeyLoading : credentialError}</p>
            ) : null}
          </div>
          <label className="fieldGroup">
            <span>{copy.profiles.modelReasoningEffort}</span>
            <select value={draft.modelReasoningEffort} onChange={(event) => updateDraft("modelReasoningEffort", event.target.value)}>
              {REASONING_EFFORT_OPTIONS.map((item) => <option key={item} value={item}>{item}</option>)}
            </select>
          </label>
          <label className="fieldGroup">
            <span>{copy.profiles.planModeReasoningEffort}</span>
            <select value={draft.planModeReasoningEffort} onChange={(event) => updateDraft("planModeReasoningEffort", event.target.value)}>
              {REASONING_EFFORT_OPTIONS.map((item) => <option key={item} value={item}>{item}</option>)}
            </select>
          </label>
          <div className="fieldGroup profileFastModeField">
            <span>{copy.profiles.fastMode}</span>
            <div className="profileFastModeSegment" role="radiogroup" aria-label={copy.profiles.fastMode}>
              <button
                aria-pressed={draft.fastMode}
                className="profileFastModeOption"
                data-selected={draft.fastMode}
                type="button"
                onClick={() => updateDraft("fastMode", true)}
              >
                {copy.hosts.yes}
              </button>
              <button
                aria-pressed={!draft.fastMode}
                className="profileFastModeOption"
                data-selected={!draft.fastMode}
                type="button"
                onClick={() => updateDraft("fastMode", false)}
              >
                {copy.hosts.no}
              </button>
            </div>
          </div>

          <details className="profileAdvancedDetails">
            <summary>{copy.profiles.advanced}</summary>
            <div className="profileAdvancedGrid">
              <label className="fieldGroup">
                <span>{copy.profiles.apiKeyEnvVar}</span>
                <input value={draft.apiKeyEnvVar} onChange={(event) => updateDraft("apiKeyEnvVar", event.target.value)} />
              </label>
              <label className="fieldGroup">
                <span>{copy.profiles.serviceTier}</span>
                <input value={draft.serviceTier} onChange={(event) => updateDraft("serviceTier", event.target.value)} />
              </label>
              <label className="fieldGroup">
                <span>{copy.profiles.description}</span>
                <input value={draft.description} onChange={(event) => updateDraft("description", event.target.value)} />
              </label>
              <label className="fieldGroup fieldWide">
                <span>{copy.profiles.extraToml}</span>
                <textarea value={draft.extraToml} onChange={(event) => updateDraft("extraToml", event.target.value)} rows={5} />
              </label>
            </div>
          </details>

          <ModalActions>
            <button className="primaryButton" disabled={!draft.name.trim() || busy === "save" || busy === "create"} type="submit">
              {busy === "save" || busy === "create" ? copy.profiles.saving : profile ? copy.profiles.save : copy.profiles.create}
            </button>
          </ModalActions>
        </form>
      </ModalFrame>
    </div>
  );
}

function CredentialVisibilityIcon({ visible }: { visible: boolean }) {
  return (
    <svg
      aria-hidden="true"
      className="credentialEyeIcon"
      data-visible={visible}
      fill="none"
      height="18"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth="2"
      viewBox="0 0 24 24"
      width="18"
    >
      {visible ? (
        <>
          <path d="M2.1 12.3c2.2-4.5 5.5-6.8 9.9-6.8s7.7 2.3 9.9 6.8c-2.2 4.1-5.5 6.2-9.9 6.2s-7.7-2.1-9.9-6.2Z" />
          <circle cx="12" cy="12" r="3" />
        </>
      ) : (
        <>
          <path d="M3 3l18 18" />
          <path d="M10.6 10.6A3 3 0 0 0 12 15a3 3 0 0 0 2.4-4.8" />
          <path d="M9.9 5.8A10.8 10.8 0 0 1 12 5.5c4.4 0 7.7 2.3 9.9 6.8a13.2 13.2 0 0 1-3.1 3.9" />
          <path d="M6.1 7.6a14.3 14.3 0 0 0-4 4.7c2.2 4.1 5.5 6.2 9.9 6.2 1.5 0 2.9-.3 4.1-.8" />
        </>
      )}
    </svg>
  );
}

function ProfileModelCombobox({ value, placeholder, onChange }: { value: string; placeholder?: string; onChange: (value: string) => void }) {
  const [open, setOpen] = useState(false);
  const query = value.trim().toLowerCase();
  const options = useMemo(
    () => CODEX_MODEL_OPTIONS.filter((model) => !query || model.toLowerCase().includes(query)),
    [query]
  );

  return (
    <div className="profileModelCombobox">
      <input
        aria-autocomplete="list"
        aria-expanded={open}
        placeholder={placeholder}
        role="combobox"
        value={value}
        onBlur={() => window.setTimeout(() => setOpen(false), 90)}
        onChange={(event) => {
          onChange(event.target.value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
      />
      {open && options.length > 0 ? (
        <div className="profileModelOptions" role="listbox">
          {options.map((model) => (
            <button
              className="profileModelOption"
              data-selected={model === value}
              key={model}
              role="option"
              type="button"
              onClick={() => {
                onChange(model);
                setOpen(false);
              }}
              onMouseDown={(event) => event.preventDefault()}
            >
              {model}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function ProfileApplyPreviewModal({
  busy,
  copy,
  open,
  preview,
  selectedCount,
  onApplySelected,
  onClose
}: {
  busy: string | null;
  copy: UICopy;
  open: boolean;
  preview: ProfileApplyPreview | null;
  selectedCount: number;
  onApplySelected: () => void;
  onClose: () => void;
}) {
  if (!open) return null;

  return (
    <div className="modalBackdrop" role="presentation">
      <ModalFrame className="sshHostModal profileApplyPreviewModal ProfileApplyPreviewModal" titleId="profile-apply-preview-modal-title">
        <ModalHeader
          className="modalHero"
          titleId="profile-apply-preview-modal-title"
          title={copy.profiles.preview}
          icon="preview"
          closeAriaLabel={copy.setupGuide.close}
          onClose={onClose}
        />

        <div className="profilePreviewSection">
          <div className="profileSubhead">
            <strong>{copy.profiles.targetFiles}</strong>
          </div>
          <div className="profileTargetBubble">
            {preview?.targetFiles.length ? (
              <>
                <div className="profileTargetFileHeader">
                  <span>{copy.profiles.targetHost}</span>
                  <span>{copy.profiles.targetPath}</span>
                </div>
                {preview.targetFiles.map((target) => (
                  <div className="profileTargetFileRow" key={target.hostId}>
                    <strong>{target.hostName}</strong>
                    <code>{target.path}</code>
                  </div>
                ))}
              </>
            ) : (
              <p className="mutedText">{copy.profiles.noPreview}</p>
            )}
          </div>
        </div>

        <div className="profilePreviewSection configPreviewBox">
          <div className="profileSubhead">
            <strong>{copy.profiles.renderedToml}</strong>
          </div>
          <pre>{preview?.renderedToml ?? copy.profiles.noPreview}</pre>
        </div>

        <ModalActions>
          <button className="primaryButton" disabled={!preview || selectedCount === 0 || busy === "apply"} type="button" onClick={onApplySelected}>
            {copy.profiles.applySelected}
          </button>
        </ModalActions>
      </ModalFrame>
    </div>
  );
}

function profileToDraft(profile: Profile | null): ProfileDraft {
  return {
    name: profile?.name ?? "",
    description: profile?.description ?? "",
    model: profile?.model ?? "",
    provider: profile?.provider ?? "",
    baseUrl: profile?.baseUrl ?? "",
    apiKeyEnvVar: profile?.apiKeyEnvVar ?? DEFAULT_PROFILE_API_KEY_ENV_VAR,
    modelReasoningEffort: profile?.modelReasoningEffort ?? "medium",
    planModeReasoningEffort: profile?.planModeReasoningEffort ?? "high",
    fastMode: profile?.fastMode ?? false,
    serviceTier: profile?.serviceTier ?? "auto",
    approvalPolicy: profile?.approvalPolicy ?? "on-request",
    sandboxMode: profile?.sandboxMode ?? "workspace-write",
    extraToml: profile?.extraToml ?? "",
    hostIds: profile?.hostIds ?? []
  };
}

function profileDraftWithCreateDefaults(draft: ProfileDraft): ProfileDraft {
  return {
    ...draft,
    model: draft.model.trim() || DEFAULT_PROFILE_MODEL,
    provider: draft.provider.trim() || DEFAULT_PROFILE_PROVIDER,
    baseUrl: draft.baseUrl.trim() || DEFAULT_PROFILE_BASE_URL,
    apiKeyEnvVar: draft.apiKeyEnvVar.trim() || DEFAULT_PROFILE_API_KEY_ENV_VAR
  };
}

export function SkillsView({
  copy,
  hosts,
  inventoryStatus,
  skillPacks,
  onDeleteLibrarySkill,
  onDetectInstalledSkills,
  onDownloadInstalledSkill,
  onDownloadGithubSkill,
  onGetSkillTargets,
  onImportSkillDirectory,
  onInstallSkillTargets,
  onRefreshSkillLibrary,
  onUninstallInstalledSkill,
  onUninstallSkillTargets,
  onUpdateLibrarySkillAbout,
  onViewTasks
}: {
  copy: UICopy;
  hosts: Host[];
  inventoryStatus: SkillInventoryStatus;
  skillPacks: SkillPack[];
  onDeleteLibrarySkill: (skillId: string, uninstallFirst: boolean) => Promise<SkillTargetOperationResult>;
  onDetectInstalledSkills: (includeHosts: boolean) => Promise<SkillDetectionResult>;
  onDownloadInstalledSkill: (request: InstalledSkillRequest) => Promise<InstalledSkillDownloadResult>;
  onDownloadGithubSkill: (repoUrl: string) => Promise<SkillImportResult>;
  onGetSkillTargets: (skillId: string) => Promise<SkillTargetsResult>;
  onImportSkillDirectory: () => Promise<SkillImportResult | null>;
  onInstallSkillTargets: (skillId: string, targets: SkillTargetRequest[]) => Promise<SkillTargetOperationResult>;
  onRefreshSkillLibrary: () => Promise<unknown>;
  onUninstallInstalledSkill: (request: InstalledSkillRequest) => Promise<SkillTargetOperationResult>;
  onUninstallSkillTargets: (skillId: string, targets: SkillTargetRequest[]) => Promise<SkillTargetOperationResult>;
  onUpdateLibrarySkillAbout: (skillId: string, about: string) => Promise<SkillPack | null>;
  onViewTasks: () => void;
}) {
  const reportActionError = useActionErrorReporter(copy);
  const { notify } = useFeedback();
  const [downloadOpen, setDownloadOpen] = useState(false);
  const [firstScanOpen, setFirstScanOpen] = useState(false);
  const [previewSkill, setPreviewSkill] = useState<SkillPack | null>(null);
  const [targetMode, setTargetMode] = useState<"install" | "uninstall" | null>(null);
  const [targetSkill, setTargetSkill] = useState<SkillPack | null>(null);
  const [targetResult, setTargetResult] = useState<SkillTargetsResult | null>(null);
  const [selectedTargetKeys, setSelectedTargetKeys] = useState<string[]>([]);
  const [deleteSkill, setDeleteSkill] = useState<SkillPack | null>(null);
  const [previewInstalledSkill, setPreviewInstalledSkill] = useState<InstalledSkillPreview | null>(null);
  const [downloadInstalledSkill, setDownloadInstalledSkill] = useState<InstalledSkillPreview | null>(null);
  const [uninstallInstalledSkill, setUninstallInstalledSkill] = useState<InstalledSkillPreview | null>(null);
  const [installedSkillOperation, setInstalledSkillOperation] = useState<InstalledSkillOperationModalState | null>(null);
  const [message, setMessage] = useState<{ text: string; tone: "success" | "warning" | "error" } | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const installedSkillRows = useMemo(
    () => buildInstalledSkillRows(copy, hosts, inventoryStatus),
    [copy, hosts, inventoryStatus]
  );
  const installedSkillNames = useMemo(
    () => Array.from(new Set(installedSkillRows.flatMap((row) => row.skills.map((skill) => skill.skillName)))).sort((left, right) => left.localeCompare(right)),
    [installedSkillRows]
  );

  const openInstalledPreview = (row: InstalledSkillLibraryRow, skill: InstalledSkillTagInfo) => {
    setPreviewInstalledSkill({
      ...skill,
      targetLabel: row.alias,
      source: row.source,
      sourceTone: row.sourceTone,
      hostIp: row.hostIp,
      localSkill: findLocalSkillForInstalled(skillPacks, skill.skillName)
    });
  };

  const beginInstalledSkillOperation = (skill: InstalledSkillPreview, action: "download" | "uninstall") => {
    const id = `installed-skill-${action}-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    setInstalledSkillOperation({
      id,
      action,
      skillName: skill.skillName,
      targetLabel: skill.targetLabel,
      path: skill.path,
      status: "running",
      tasks: [],
      message: action === "download"
        ? copy.skills.downloadInstalledStarted(skill.skillName)
        : copy.skills.uninstallInstalledStarted(skill.skillName, skill.targetLabel)
    });
    return id;
  };

  const submitInstalledDownload = async () => {
    const skill = downloadInstalledSkill;
    if (!skill) return;
    setDownloadInstalledSkill(null);
    setPreviewInstalledSkill(null);
    const operationId = beginInstalledSkillOperation(skill, "download");
    try {
      const result = await runBusy(`download-installed-${skill.targetType}-${skill.skillName}`, () =>
        onDownloadInstalledSkill(installedSkillRequest(skill))
      );
      setInstalledSkillOperation((current) =>
        current?.id === operationId
          ? {
              ...current,
              status: "success",
              tasks: result.tasks,
              message: result.message || copy.skills.downloaded
            }
          : current
      );
      setMessage({ text: result.message || copy.skills.downloaded, tone: "success" });
    } catch (error) {
      const detail = formatError(error);
      setInstalledSkillOperation((current) =>
        current?.id === operationId
          ? {
              ...current,
              status: "failed",
              error: detail
            }
          : current
      );
    }
  };

  const submitInstalledUninstall = async () => {
    const skill = uninstallInstalledSkill;
    if (!skill) return;
    setUninstallInstalledSkill(null);
    setPreviewInstalledSkill(null);
    const operationId = beginInstalledSkillOperation(skill, "uninstall");
    try {
      const result = await runBusy(`uninstall-installed-${skill.targetType}-${skill.skillName}`, () =>
        onUninstallInstalledSkill(installedSkillRequest(skill))
      );
      const message =
        result.message === "uninstall-success"
          ? copy.skills.uninstallSuccess
          : result.message === "uninstall-partial-failure"
            ? copy.skills.uninstallPartialFailure
            : result.message;
      setInstalledSkillOperation((current) =>
        current?.id === operationId
          ? {
              ...current,
              status: result.ok ? "success" : "failed",
              tasks: result.tasks,
              message
            }
          : current
      );
      setMessage({ text: message, tone: result.ok ? "success" : "error" });
    } catch (error) {
      const detail = formatError(error);
      setInstalledSkillOperation((current) =>
        current?.id === operationId
          ? {
              ...current,
              status: "failed",
              error: detail
            }
          : current
      );
    }
  };

  const runBusy = async <T,>(key: string, action: () => Promise<T>) => {
    setBusy(key);
    setMessage(null);
    try {
      const result = await action();
      return result;
    } catch (error) {
      const detail = formatError(error);
      setMessage({ text: detail, tone: "error" });
      throw error;
    } finally {
      setBusy(null);
    }
  };

  const handleDetect = async (includeHosts: boolean) => {
    await runBusy("detect", () => onDetectInstalledSkills(includeHosts));
    setMessage({ text: copy.skills.detected, tone: "success" });
  };

  const handleDetectClick = () => {
    void handleDetect(true).catch(reportActionError);
  };

  const handleImport = async () => {
    const result = await runBusy("import", onImportSkillDirectory);
    if (result) setMessage({ text: result.message, tone: "success" });
  };

  const handleRefresh = async () => {
    await runBusy("refresh", onRefreshSkillLibrary);
    setMessage({ text: copy.skills.refreshed, tone: "success" });
  };

  const handleDownload = async (repoUrl: string) => {
    await runBusy("download", () => onDownloadGithubSkill(repoUrl));
    setDownloadOpen(false);
    setMessage({ text: copy.skills.downloaded, tone: "success" });
  };

  const openDownload = () => {
    setDownloadOpen(true);
    notify({ title: copy.skills.downloadTitle, message: copy.skills.downloadBody, placement: "global", tone: "info" });
  };

  const openInstalledDownload = (skill: InstalledSkillPreview) => {
    setDownloadInstalledSkill(skill);
    notify({
      title: copy.skills.downloadInstalledTitle,
      message: copy.skills.downloadInstalledBody(skill.skillName),
      placement: "global",
      tone: "info"
    });
  };

  const openTargets = async (skill: SkillPack, mode: "install" | "uninstall") => {
    setTargetSkill(skill);
    setTargetMode(mode);
    setTargetResult(null);
    setSelectedTargetKeys([]);
    try {
      const result = await runBusy(`targets-${skill.id}`, () => onGetSkillTargets(skill.id));
      setTargetResult(result);
      setSelectedTargetKeys([]);
    } catch (error) {
      reportActionError(error);
      setTargetMode(null);
      setTargetSkill(null);
    }
  };

  const submitTargets = async () => {
    if (!targetSkill || !targetMode || !targetResult) return;
    const requests = targetResult.targets
      .filter((target) => selectedTargetKeys.includes(skillTargetKey(target)))
      .map(skillTargetRequest);
    const result = await runBusy(`${targetMode}-${targetSkill.id}`, () =>
      targetMode === "install"
        ? onInstallSkillTargets(targetSkill.id, requests)
        : onUninstallSkillTargets(targetSkill.id, requests)
    );
    setTargetMode(null);
    setTargetSkill(null);
    setTargetResult(null);
    setSelectedTargetKeys([]);
    setMessage({
      text: result.message === "install-success"
        ? copy.skills.installSuccess
        : result.message === "install-partial-failure"
          ? copy.skills.installPartialFailure
          : result.message === "uninstall-success"
            ? copy.skills.uninstallSuccess
            : result.message === "uninstall-partial-failure"
              ? copy.skills.uninstallPartialFailure
          : result.message,
      tone: result.ok ? "success" : "error"
    });
  };

  const selectAllTargets = () => {
    if (!targetMode || !targetResult) return;
    const selectable = targetResult.targets
      .filter((target) => (targetMode === "install" ? target.canInstall : target.canUninstall))
      .map(skillTargetKey);
    setSelectedTargetKeys(selectable);
  };

  const submitDelete = async (uninstallFirst: boolean) => {
    if (!deleteSkill) return;
    const result = await runBusy(`delete-${deleteSkill.id}`, () => onDeleteLibrarySkill(deleteSkill.id, uninstallFirst));
    setDeleteSkill(null);
    setMessage({ text: result.message, tone: result.ok ? "success" : "error" });
  };

  const toggleTarget = (target: SkillTarget) => {
    const key = skillTargetKey(target);
    setSelectedTargetKeys((current) =>
      current.includes(key) ? current.filter((item) => item !== key) : [...current, key]
    );
  };

  return (
    <div className="skillsStack">
      <section className="panel spanWide">
        <div className="panelHeader compact">
          <TitleWithIcon icon="skills" level={2}>{copy.skills.library}</TitleWithIcon>
          <CommandBar ariaLabel={copy.skills.library} className="skillLibraryActions">
            <button className="secondaryButton" disabled={busy === "detect"} type="button" onClick={handleDetectClick}>
              {busy === "detect" ? copy.skills.detecting : copy.skills.detect}
            </button>
            <button className="secondaryButton" disabled={busy === "refresh"} type="button" onClick={() => void handleRefresh().catch(reportActionError)}>
              {busy === "refresh" ? copy.skills.refreshing : copy.skills.refresh}
            </button>
            <button className="secondaryButton" disabled={busy === "import"} type="button" onClick={() => void handleImport().catch(reportActionError)}>
              {copy.skills.importDirectory}
            </button>
            <button className="primaryButton" disabled={busy === "download"} type="button" onClick={openDownload}>
              {copy.skills.download}
            </button>
          </CommandBar>
        </div>

        {skillPacks.length === 0 ? (
          <EmptyListState copy={copy} message={copy.emptyLists.skills} variant="skills" />
        ) : (
          <div className="tableWrap">
            <table className="skillsTable">
              <thead>
                <tr>
                  <th>{copy.skills.skill}</th>
                  <th>{copy.skills.source}</th>
                  <th>{copy.skills.addedAt}</th>
                  <th>{copy.skills.applications}</th>
                  <th>{copy.skills.actions}</th>
                </tr>
              </thead>
              <tbody>
                {skillPacks.map((skill) => (
                  <tr key={skill.id}>
                    <td>
                      <strong>{skill.name}</strong>
                    </td>
                    <td>
                      <Badge tone={skill.sourceType === "github" ? "blue" : "gray"}>
                        {skill.sourceType === "github" ? copy.skills.sourceGithub : copy.skills.sourceLocal}
                      </Badge>
                    </td>
                    <td>{skill.addedAt || "-"}</td>
                    <td>
                      <div className="skillApplicationTags">
                        {skill.applications.length > 0 ? (
                          skill.applications.map((application) => (
                            <Badge
                              key={`${application.targetType}-${application.hostAlias ?? "local"}`}
                              tone={application.hasSkillMd ? "green" : "yellow"}
                              title={application.path}
                            >
                              {skillApplicationLabel(application, copy)}
                            </Badge>
                          ))
                        ) : (
                          <Badge tone="gray">{copy.skills.unapplied}</Badge>
                        )}
                      </div>
                    </td>
                    <td>
                      <CommandGroup className="skillRowActions">
                        <button className="miniButton" disabled={Boolean(busy)} type="button" onClick={() => setPreviewSkill(skill)}>
                          {copy.skills.preview}
                        </button>
                        <button className="miniButton" disabled={Boolean(busy)} type="button" onClick={() => void openTargets(skill, "install")}>
                          {copy.skills.install}
                        </button>
                        <button className="miniButton" disabled={Boolean(busy) || skill.applications.length === 0} type="button" onClick={() => void openTargets(skill, "uninstall")}>
                          {copy.skills.uninstall}
                        </button>
                        <button className="miniButton danger" disabled={Boolean(busy)} type="button" onClick={() => setDeleteSkill(skill)}>
                          {copy.skills.delete}
                        </button>
                      </CommandGroup>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {message ? <p className="skillMessage" data-tone={message.tone} role={message.tone === "error" ? "alert" : "status"}>{message.text}</p> : null}
      </section>

      <section className="panel spanWide">
        <div className="panelHeader compact">
          <TitleWithIcon icon="install" level={2}>{copy.skills.installedLibrary}</TitleWithIcon>
        </div>
        <div className="tableWrap">
          <table className="installedSkillsTable">
            <thead>
              <tr>
                <th>{copy.hosts.alias}</th>
                <th>{copy.hosts.source}</th>
                <th>{copy.skills.hostIp}</th>
                <th>{copy.skills.installedSkills}</th>
              </tr>
            </thead>
            <tbody>
              {installedSkillRows.map((row) => (
                <tr key={row.key}>
                  <td><strong>{row.alias}</strong></td>
                  <td><Badge tone={row.sourceTone}>{row.source}</Badge></td>
                  <td>{row.hostIp}</td>
                  <td>
                    <div className="installedSkillTags">
                      {row.skills.length > 0 ? (
                        row.skills.map((skill) => (
                          <button
                            className="installedSkillTag"
                            key={skill.key}
                            style={installedSkillTagStyle(skill.skillName, installedSkillNames)}
                            title={skill.path}
                            type="button"
                            onClick={() => openInstalledPreview(row, skill)}
                          >
                            {skill.skillName}
                          </button>
                        ))
                      ) : row.unknownSkillCount ? (
                        <Badge tone="blue">{`${copy.hosts.skillsCount}: ${row.unknownSkillCount}`}</Badge>
                      ) : (
                        <Badge tone="gray">{copy.skills.noInstalledSkills}</Badge>
                      )}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      {firstScanOpen ? (
        <SkillFirstScanModal
          busy={busy === "detect"}
          copy={copy}
          onClose={() => setFirstScanOpen(false)}
          onConfirm={() => void handleDetect(true).then(() => setFirstScanOpen(false)).catch(reportActionError)}
        />
      ) : null}
      {downloadOpen ? (
        <SkillDownloadModal
          busy={busy === "download"}
          copy={copy}
          onClose={() => setDownloadOpen(false)}
          onDownload={(repoUrl) => void handleDownload(repoUrl).catch(reportActionError)}
        />
      ) : null}
      {previewSkill ? (
        <SkillPreviewModal
          copy={copy}
          skill={previewSkill}
          onClose={() => setPreviewSkill(null)}
          onSaveAbout={async (about) => {
            const updated = await onUpdateLibrarySkillAbout(previewSkill.id, about);
            if (updated) setPreviewSkill(updated);
          }}
        />
      ) : null}
      {previewInstalledSkill ? (
        <InstalledSkillPreviewModal
          busy={Boolean(busy)}
          copy={copy}
          skill={previewInstalledSkill}
          onClose={() => setPreviewInstalledSkill(null)}
          onDownload={() => openInstalledDownload(previewInstalledSkill)}
          onSaveAbout={async (about) => {
            if (!previewInstalledSkill.localSkill) return;
            const updated = await onUpdateLibrarySkillAbout(previewInstalledSkill.localSkill.id, about);
            if (updated) {
              setPreviewInstalledSkill((current) =>
                current && current.skillName === previewInstalledSkill.skillName
                  ? { ...current, localSkill: updated }
                  : current
              );
            }
          }}
          onUninstall={() => setUninstallInstalledSkill(previewInstalledSkill)}
        />
      ) : null}
      {downloadInstalledSkill ? (
        <SkillInstalledConfirmModal
          busy={Boolean(busy)}
          confirmLabel={copy.skills.downloadInstalledAction}
          copy={copy}
          title={copy.skills.downloadInstalledTitle}
          body={copy.skills.downloadInstalledBody(downloadInstalledSkill.skillName)}
          onClose={() => setDownloadInstalledSkill(null)}
          onConfirm={() => void submitInstalledDownload().catch(reportActionError)}
        />
      ) : null}
      {uninstallInstalledSkill ? (
        <SkillInstalledConfirmModal
          busy={Boolean(busy)}
          confirmLabel={copy.skills.uninstallInstalledAction}
          copy={copy}
          danger
          title={copy.skills.uninstallInstalledTitle}
          body={copy.skills.uninstallInstalledBody(uninstallInstalledSkill.skillName, uninstallInstalledSkill.targetLabel)}
          onClose={() => setUninstallInstalledSkill(null)}
          onConfirm={() => void submitInstalledUninstall().catch(reportActionError)}
        />
      ) : null}
      {installedSkillOperation ? (
        <InstalledSkillOperationModal
          copy={copy}
          operation={installedSkillOperation}
          onClose={() => setInstalledSkillOperation(null)}
          onViewTasks={() => {
            setInstalledSkillOperation(null);
            onViewTasks();
          }}
        />
      ) : null}
      {targetMode && targetSkill ? (
        <SkillTargetsModal
          busy={Boolean(busy)}
          copy={copy}
          mode={targetMode}
          selectedKeys={selectedTargetKeys}
          skill={targetSkill}
          targets={targetResult?.targets ?? []}
          onClose={() => {
            setTargetMode(null);
            setTargetSkill(null);
            setTargetResult(null);
            setSelectedTargetKeys([]);
          }}
          onSelectAll={selectAllTargets}
          onSubmit={() => void submitTargets().catch(reportActionError)}
          onToggle={toggleTarget}
        />
      ) : null}
      {deleteSkill ? (
        <SkillDeleteModal
          busy={Boolean(busy)}
          copy={copy}
          skill={deleteSkill}
          onClose={() => setDeleteSkill(null)}
          onDelete={(uninstallFirst) => void submitDelete(uninstallFirst).catch(reportActionError)}
        />
      ) : null}
    </div>
  );
}

type InstalledSkillLibraryRow = {
  key: string;
  alias: string;
  source: string;
  sourceTone: BadgeTone;
  hostIp: string;
  skills: InstalledSkillTagInfo[];
  unknownSkillCount?: number;
};

type InstalledSkillTagInfo = {
  key: string;
  targetType: "local" | "host";
  hostAlias: string | null;
  skillName: string;
  path: string;
  hasSkillMd: boolean;
  status: string;
  description: string;
};

type InstalledSkillPreview = InstalledSkillTagInfo & {
  targetLabel: string;
  source: string;
  sourceTone: BadgeTone;
  hostIp: string;
  localSkill: SkillPack | null;
};

type InstalledSkillOperationModalState = {
  id: string;
  action: "download" | "uninstall";
  skillName: string;
  targetLabel: string;
  path: string;
  status: CodexOperationModalStatus;
  tasks: TaskRun[];
  message?: string;
  error?: string;
};

function installedSkillTags(skills: RemoteSkill[], targetType: "local" | "host", hostAlias: string | null): InstalledSkillTagInfo[] {
  return skills
    .filter((skill) => skill.hasSkillMd !== false && skill.name.trim())
    .map((skill) => ({
      key: `${targetType}:${hostAlias ?? "local"}:${skill.path}:${skill.name}`,
      targetType,
      hostAlias,
      skillName: skill.name.trim(),
      path: skill.path,
      hasSkillMd: skill.hasSkillMd,
      status: skill.status,
      description: skill.description?.trim() ?? ""
    }))
    .sort((left, right) => left.skillName.localeCompare(right.skillName));
}

function normalizedSkillLookupName(value: string) {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

function findLocalSkillForInstalled(skillPacks: SkillPack[], skillName: string) {
  const normalized = normalizedSkillLookupName(skillName);
  const lower = skillName.toLowerCase();
  return (
    skillPacks.find((skill) => skill.id.toLowerCase() === normalized || skill.name.toLowerCase() === lower) ??
    null
  );
}

function installedSkillRequest(skill: InstalledSkillTagInfo): InstalledSkillRequest {
  return {
    targetType: skill.targetType,
    hostAlias: skill.hostAlias,
    skillName: skill.skillName,
    path: skill.path
  };
}

function buildInstalledSkillRows(copy: UICopy, hosts: Host[], status: SkillInventoryStatus): InstalledSkillLibraryRow[] {
  const hostInventoryByAlias = new Map(status.hostInventories.map((inventory) => [inventory.hostAlias.toLowerCase(), inventory]));
  return [
    {
      key: "local",
      alias: copy.skills.localMachine,
      source: copy.skills.localMachine,
      sourceTone: "green",
      hostIp: "127.0.0.1",
      skills: installedSkillTags(status.localSkills, "local", null)
    },
    ...hosts.map((host) => {
      const inventory = hostInventoryByAlias.get(host.hostAlias.toLowerCase());
      return {
        key: `host-${host.hostAlias}`,
        alias: host.hostAlias,
        source: hostSourceLabel(copy, host),
        sourceTone: host.source === "managed" ? "blue" as const : "gray" as const,
        hostIp: host.address || "-",
        skills: inventory?.ok ? installedSkillTags(inventory.skills, "host", host.hostAlias) : [],
        unknownSkillCount: !inventory?.ok && typeof host.skillsCount === "number" && host.skillsCount > 0
          ? host.skillsCount
          : undefined
      };
    })
  ];
}

function installedSkillColor(skillName: string, allSkillNames: string[]) {
  if (allSkillNames.length <= 1) return "hsl(205 74% 42%)";
  const index = Math.max(0, allSkillNames.indexOf(skillName));
  const hue = Math.round((index / Math.max(1, allSkillNames.length - 1)) * 310);
  return `hsl(${hue} 72% 42%)`;
}

function installedSkillTagStyle(skillName: string, allSkillNames: string[]): CSSProperties {
  return {
    "--skill-color": installedSkillColor(skillName, allSkillNames)
  } as CSSProperties;
}

function skillTargetKey(target: Pick<SkillTarget, "targetType" | "hostAlias" | "label">) {
  const identity = target.targetType === "local" ? "local" : target.hostAlias ?? target.label;
  return `${target.targetType}:${identity}`;
}

function skillTargetRequest(target: SkillTarget): SkillTargetRequest {
  return {
    targetType: target.targetType,
    hostAlias: target.targetType === "host" ? target.hostAlias : null
  };
}

function skillApplicationLabel(application: SkillApplication, copy: UICopy) {
  if (application.targetType === "local") return copy.skills.localMachine;
  return application.hostAlias || application.label || copy.hosts.unknown;
}

function skillTargetStatusLabel(target: SkillTarget, copy: UICopy) {
  if (target.installed) return copy.skills.installedTarget;
  if (target.canInstall || target.canUninstall) return copy.skills.available;
  return copy.skills.unavailableTarget;
}

function skillTargetTone(target: SkillTarget): BadgeTone {
  if (target.installed) return "green";
  if (target.canInstall || target.canUninstall) return "blue";
  return target.status === "offline" || target.status === "error" || target.status === "failed" ? "red" : "gray";
}

function SkillFirstScanModal({
  busy,
  copy,
  onClose,
  onConfirm
}: {
  busy: boolean;
  copy: UICopy;
  onClose: () => void;
  onConfirm: () => void;
}) {
  return (
    <div className="modalBackdrop" role="presentation">
      <ModalFrame className="taskLogModal skillModal" titleId="skill-first-scan-title">
        <ModalHeader
          className="taskLogModalHeader"
          titleId="skill-first-scan-title"
          title={copy.skills.firstScanTitle}
          description={copy.skills.firstScanBody}
          icon="scan"
          closeAriaLabel={copy.hosts.cancel}
          closeDisabled={busy}
          onClose={onClose}
        />
        <ModalActions>
          <button className="secondaryButton" disabled={busy} type="button" onClick={onClose}>
            {copy.hosts.cancel}
          </button>
          <button className="primaryButton" disabled={busy} type="button" onClick={onConfirm}>
            {busy ? copy.skills.detecting : copy.skills.firstScanAction}
          </button>
        </ModalActions>
      </ModalFrame>
    </div>
  );
}

function SkillDownloadModal({
  busy,
  copy,
  onClose,
  onDownload
}: {
  busy: boolean;
  copy: UICopy;
  onClose: () => void;
  onDownload: (repoUrl: string) => void;
}) {
  const [repoUrl, setRepoUrl] = useState("");
  const canSubmit = repoUrl.trim().length > 0 && !busy;

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!canSubmit) return;
    onDownload(repoUrl.trim());
  };

  return (
    <div className="modalBackdrop" role="presentation">
      <ModalFrame className="taskLogModal skillModal" titleId="skill-download-title">
        <ModalHeader
          className="taskLogModalHeader"
          titleId="skill-download-title"
          title={copy.skills.downloadTitle}
          icon="download"
          closeAriaLabel={copy.hosts.cancel}
          closeDisabled={busy}
          onClose={onClose}
        />
        <form className="skillDownloadForm" onSubmit={handleSubmit}>
          <label className="fieldGroup">
            <span>{copy.skills.githubUrl}</span>
            <input
              autoFocus
              disabled={busy}
              inputMode="url"
              onChange={(event) => setRepoUrl(event.target.value)}
              placeholder="https://github.com/owner/repo"
              required
              type="url"
              value={repoUrl}
            />
          </label>
          <ModalActions>
            <button className="secondaryButton" disabled={busy} type="button" onClick={onClose}>
              {copy.hosts.cancel}
            </button>
            <button className="primaryButton" disabled={!canSubmit} type="submit">
              {busy ? copy.common.loading : copy.skills.downloadAction}
            </button>
          </ModalActions>
        </form>
      </ModalFrame>
    </div>
  );
}

function NetworkProxyManualModal({
  copy,
  initialValue,
  onClose,
  onSave
}: {
  copy: UICopy;
  initialValue: string;
  onClose: () => void;
  onSave: (proxyPort: string) => void;
}) {
  const [proxyPort, setProxyPort] = useState(initialValue.trim() || "7890");
  const canSubmit = proxyPort.trim().length > 0;

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!canSubmit) return;
    onSave(proxyPort.trim());
  };

  return (
    <div className="modalBackdrop" role="presentation">
      <ModalFrame className="taskLogModal skillModal" titleId="network-proxy-manual-title">
        <ModalHeader
          className="taskLogModalHeader"
          titleId="network-proxy-manual-title"
          title={copy.settings.networkProxyManualTitle}
          icon="network"
          closeAriaLabel={copy.hosts.cancel}
          onClose={onClose}
        />
        <form className="skillDownloadForm networkProxyManualForm" onSubmit={handleSubmit}>
          <label className="fieldGroup">
            <span>{copy.settings.networkProxyPort}</span>
            <input
              autoFocus
              inputMode="text"
              onChange={(event) => setProxyPort(event.target.value)}
              placeholder={copy.settings.networkProxyPortPlaceholder}
              required
              value={proxyPort}
            />
          </label>
          <ModalActions>
            <button className="secondaryButton" type="button" onClick={onClose}>
              {copy.hosts.cancel}
            </button>
            <button className="primaryButton" disabled={!canSubmit} type="submit">
              {copy.settings.networkProxySave}
            </button>
          </ModalActions>
        </form>
      </ModalFrame>
    </div>
  );
}

function SkillPreviewModal({
  copy,
  skill,
  onClose,
  onSaveAbout
}: {
  copy: UICopy;
  skill: SkillPack;
  onClose: () => void;
  onSaveAbout: (about: string) => Promise<void>;
}) {
  const skillDescription = skill.description?.trim() ?? "";
  const about = skillDescription || skill.about?.trim() || copy.skills.aboutFallback;
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(about);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setDraft(about);
    setEditing(false);
    setError(null);
  }, [about, skill.id]);

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    try {
      await onSaveAbout(draft);
      setEditing(false);
    } catch (saveError) {
      setError(formatError(saveError));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="modalBackdrop" role="presentation">
      <ModalFrame className="taskLogModal skillModal skillPreviewModal" titleId="skill-preview-title">
        <ModalHeader
          className="taskLogModalHeader"
          titleId="skill-preview-title"
          title={skill.name}
          icon="skills"
          closeAriaLabel={copy.setupGuide.close}
          onClose={onClose}
          badge={(
            <Badge tone={skill.sourceType === "github" ? "blue" : "gray"}>
              {skill.sourceType === "github" ? copy.skills.sourceGithub : copy.skills.sourceLocal}
            </Badge>
          )}
        />
        <div className="taskLogModalMeta skillPreviewMeta">
          <div>
            <span>{copy.skills.addedAt}</span>
            <strong>{skill.addedAt || "-"}</strong>
          </div>
          <div>
            <span>{copy.skills.path}</span>
            <strong>{skill.managedPath || skill.originalPath || "-"}</strong>
          </div>
        </div>
        <section className="skillPreviewDetails">
          <span>{copy.skills.details}</span>
          {editing ? (
            <textarea
              autoFocus
              disabled={saving}
              onChange={(event) => setDraft(event.target.value)}
              value={draft}
            />
          ) : (
            <p>{about}</p>
          )}
        </section>
        {error ? <p className="skillMessage">{error}</p> : null}
        <ModalActions>
          {editing ? (
            <>
              <button className="secondaryButton" disabled={saving} type="button" onClick={() => {
                setDraft(about);
                setEditing(false);
                setError(null);
              }}>
                {copy.hosts.cancel}
              </button>
              <button className="primaryButton" disabled={saving} type="button" onClick={() => void handleSave()}>
                {copy.skills.save}
              </button>
            </>
          ) : (
            <button className="primaryButton" type="button" onClick={() => setEditing(true)}>
              {copy.skills.edit}
            </button>
          )}
        </ModalActions>
      </ModalFrame>
    </div>
  );
}

function InstalledSkillPreviewModal({
  busy,
  copy,
  skill,
  onClose,
  onDownload,
  onSaveAbout,
  onUninstall
}: {
  busy: boolean;
  copy: UICopy;
  skill: InstalledSkillPreview;
  onClose: () => void;
  onDownload: () => void;
  onSaveAbout: (about: string) => Promise<void>;
  onUninstall: () => void;
}) {
  const localSkill = skill.localSkill;
  const about = localSkill
    ? localSkill.description?.trim() || localSkill.about?.trim() || copy.skills.aboutFallback
    : skill.description?.trim() || copy.skills.aboutFallback;
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(about);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setDraft(about);
    setEditing(false);
    setError(null);
  }, [about, skill.skillName, localSkill?.id]);

  const handleSave = async () => {
    if (!localSkill) return;
    setSaving(true);
    setError(null);
    try {
      await onSaveAbout(draft);
      setEditing(false);
    } catch (saveError) {
      setError(formatError(saveError));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="modalBackdrop" role="presentation">
      <ModalFrame className="taskLogModal skillModal skillPreviewModal" titleId="installed-skill-preview-title">
        <ModalHeader
          className="taskLogModalHeader"
          titleId="installed-skill-preview-title"
          title={skill.skillName}
          icon="skills"
          closeAriaLabel={copy.setupGuide.close}
          closeDisabled={busy || saving}
          onClose={onClose}
        />
        <div className="taskLogModalMeta skillPreviewMeta installedSkillPreviewMeta">
          <div>
            <span>{copy.skills.installedPreviewTarget}</span>
            <strong>{skill.targetLabel}</strong>
          </div>
          <div>
            <span>{copy.skills.installedPreviewLocalLibrary}</span>
            <strong>{localSkill ? localSkill.name : copy.skills.installedPreviewNotDownloaded}</strong>
          </div>
          <div>
            <span>{copy.skills.path}</span>
            <strong>{skill.path || "-"}</strong>
          </div>
        </div>
        <section className="skillPreviewDetails">
          <span>{copy.skills.details}</span>
          {editing ? (
            <textarea
              autoFocus
              disabled={saving}
              onChange={(event) => setDraft(event.target.value)}
              value={draft}
            />
          ) : (
            <p>{about}</p>
          )}
        </section>
        {error ? <p className="skillMessage">{error}</p> : null}
        <ModalActions className="skillPreviewActions">
          {editing ? (
            <>
              <button className="secondaryButton" disabled={saving} type="button" onClick={() => {
                setDraft(about);
                setEditing(false);
                setError(null);
              }}>
                {copy.hosts.cancel}
              </button>
              <button className="primaryButton" disabled={saving} type="button" onClick={() => void handleSave()}>
                {copy.skills.save}
              </button>
            </>
          ) : (
            <>
              {localSkill ? (
                <button className="secondaryButton" disabled={busy} type="button" onClick={() => setEditing(true)}>
                  {copy.skills.edit}
                </button>
              ) : null}
              <button className="primaryButton" disabled={busy || Boolean(localSkill)} type="button" onClick={onDownload}>
                {localSkill ? copy.skills.downloadUnavailableLocalExists : copy.skills.download}
              </button>
              <button className="primaryButton dangerButton" disabled={busy} type="button" onClick={onUninstall}>
                {copy.skills.uninstall}
              </button>
            </>
          )}
        </ModalActions>
      </ModalFrame>
    </div>
  );
}

function SkillInstalledConfirmModal({
  body,
  busy,
  confirmLabel,
  copy,
  danger = false,
  title,
  onClose,
  onConfirm
}: {
  body: string;
  busy: boolean;
  confirmLabel: string;
  copy: UICopy;
  danger?: boolean;
  title: string;
  onClose: () => void;
  onConfirm: () => void;
}) {
  return (
      <AlertModalFrame busy={busy} className="taskLogModal skillModal" titleId="installed-skill-confirm-title" onCancel={onClose}>
        <ModalHeader
          className="taskLogModalHeader"
          titleId="installed-skill-confirm-title"
          title={title}
          description={body}
          icon={danger ? "delete" : "skills"}
          closeAriaLabel={copy.hosts.cancel}
          closeDisabled={busy}
          onClose={onClose}
        />
        <ModalActions>
          <button className="secondaryButton" data-alert-cancel disabled={busy} type="button" onClick={onClose}>
            {copy.hosts.cancel}
          </button>
          <button className={danger ? "primaryButton dangerButton" : "primaryButton"} disabled={busy} type="button" onClick={onConfirm}>
            {confirmLabel}
          </button>
        </ModalActions>
      </AlertModalFrame>
  );
}

function InstalledSkillOperationModal({
  copy,
  operation,
  onClose,
  onViewTasks
}: {
  copy: UICopy;
  operation: InstalledSkillOperationModalState;
  onClose: () => void;
  onViewTasks: () => void;
}) {
  const logRowsRef = useRef<HTMLDivElement | null>(null);
  const statusTone = operation.status === "success" ? "green" : operation.status === "failed" ? "red" : "blue";
  const title = operation.action === "download" ? copy.skills.downloadInstalledTitle : copy.skills.uninstallInstalledTitle;
  const taskLogs = operation.tasks.flatMap((task) =>
    task.logs.map((log) => ({
      level: log.level,
      message: log.message || task.summary
    }))
  );
  const fallbackLogs = [
    {
      level: operation.status === "failed" ? "error" as const : "info" as const,
      message: operation.error ?? operation.message ?? copy.skills.operationWaiting
    }
  ];
  const rows = taskLogs.length > 0 ? taskLogs : fallbackLogs;

  useEffect(() => {
    if (logRowsRef.current) {
      logRowsRef.current.scrollTop = logRowsRef.current.scrollHeight;
    }
  }, [rows.length, operation.status]);

  return (
    <div className="modalBackdrop" role="presentation">
      <ModalFrame className="codexOperationModal skillOperationModal" titleId="installed-skill-operation-title">
        <ModalHeader
          className="codexOperationHeader"
          titleId="installed-skill-operation-title"
          title={title}
          icon={operation.action === "download" ? "download" : "delete"}
          badge={<Badge tone={statusTone}>{copy.codexOperation[operation.status]}</Badge>}
          closeAriaLabel={operation.status === "running" ? copy.codexOperation.hide : copy.codexOperation.close}
          onClose={onClose}
        />
        <div className="codexOperationSummary">
          <span>{copy.codexOperation.summary}</span>
          <strong>{operation.message ?? operation.error ?? copy.skills.operationWaiting}</strong>
          <small>{`${operation.skillName} · ${operation.targetLabel}`}</small>
        </div>
        <div className="codexOperationLog">
          <div className="codexOperationLogTitle">
            <span>{copy.codexOperation.latestLog}</span>
            {operation.status === "running" ? <i aria-hidden="true" /> : null}
          </div>
          <div className="codexOperationLogRows" ref={logRowsRef}>
            {rows.map((log, index) => (
              <div className="codexOperationLogRow" data-level={log.level} key={`${log.message}-${index}`}>
                <strong>{copy.status.log[log.level]}</strong>
                <span>{log.message}</span>
              </div>
            ))}
          </div>
        </div>
        <ModalActions className="codexOperationActions">
          {operation.tasks.length > 0 ? (
            <button className="secondaryButton" type="button" onClick={onViewTasks}>
              {copy.codexOperation.viewTasks}
            </button>
          ) : null}
          <button className="primaryButton" type="button" onClick={onClose}>
            {operation.status === "running" ? copy.codexOperation.hide : copy.codexOperation.close}
          </button>
        </ModalActions>
      </ModalFrame>
    </div>
  );
}

function SkillTargetsModal({
  busy,
  copy,
  mode,
  selectedKeys,
  skill,
  targets,
  onClose,
  onSelectAll,
  onSubmit,
  onToggle
}: {
  busy: boolean;
  copy: UICopy;
  mode: "install" | "uninstall";
  selectedKeys: string[];
  skill: SkillPack;
  targets: SkillTarget[];
  onClose: () => void;
  onSelectAll: () => void;
  onSubmit: () => void;
  onToggle: (target: SkillTarget) => void;
}) {
  const actionLabel = mode === "install" ? copy.skills.install : copy.skills.uninstall;
  const hint = mode === "install" ? copy.skills.installHint : copy.skills.uninstallHint;
  const selectedCount = selectedKeys.length;
  const hasTargets = targets.length > 0;
  const selectableCount = targets.filter((target) => (mode === "install" ? target.canInstall : target.canUninstall)).length;

  return (
      <AlertModalFrame busy={busy} className="taskLogModal skillModal" titleId="skill-targets-title" onCancel={onClose}>
        <ModalHeader
          className="taskLogModalHeader"
          titleId="skill-targets-title"
          title={`${actionLabel}: ${skill.name}`}
          description={hint}
          icon={mode === "install" ? "install" : "delete"}
          closeAriaLabel={copy.hosts.cancel}
          closeDisabled={busy}
          onClose={onClose}
        />
        <div className="skillTargetList">
          {hasTargets ? (
            targets.map((target) => {
              const key = skillTargetKey(target);
              const selectable = mode === "install" ? target.canInstall : target.canUninstall;
              const detailText = mode === "install" ? target.message : target.path || target.message;
              const secondaryText = mode === "install" ? "" : target.path && target.message ? target.message : "";
              return (
                <label className="skillTargetRow" data-disabled={!selectable} key={key}>
                  <input
                    checked={selectedKeys.includes(key)}
                    disabled={!selectable || busy}
                    onChange={() => onToggle(target)}
                    type="checkbox"
                  />
                  <div>
                    <strong>{target.targetType === "local" ? copy.skills.localMachine : target.label}</strong>
                    {detailText ? <span>{detailText}</span> : null}
                    {secondaryText ? <small>{secondaryText}</small> : null}
                  </div>
                  <Badge tone={skillTargetTone(target)}>{skillTargetStatusLabel(target, copy)}</Badge>
                </label>
              );
            })
          ) : (
            <EmptyListState copy={copy} message={busy ? copy.common.loading : copy.skills.noTargets} variant="skills" />
          )}
        </div>
        <ModalActions>
          <button className="secondaryButton" data-alert-cancel disabled={busy} type="button" onClick={onClose}>
            {copy.hosts.cancel}
          </button>
          <button className="secondaryButton" disabled={busy || selectableCount === 0} type="button" onClick={onSelectAll}>
            {copy.skills.selectAll}
          </button>
          <button className="primaryButton" disabled={busy || selectedCount === 0} type="button" onClick={onSubmit}>
            {actionLabel}
          </button>
        </ModalActions>
      </AlertModalFrame>
  );
}

function SimpleDeleteConfirmModal({
  body,
  busy,
  copy,
  title,
  onClose,
  onDelete
}: {
  body: string;
  busy: boolean;
  copy: UICopy;
  title: string;
  onClose: () => void;
  onDelete: () => void;
}) {
  return (
      <AlertModalFrame busy={busy} className="taskLogModal simpleDeleteModal" titleId="simple-delete-title" onCancel={onClose}>
        <ModalHeader
          className="taskLogModalHeader"
          titleId="simple-delete-title"
          title={title}
          description={body}
          icon="delete"
          closeAriaLabel={copy.hosts.cancel}
          closeDisabled={busy}
          onClose={onClose}
        />
        <ModalActions>
          <button className="secondaryButton" data-alert-cancel disabled={busy} type="button" onClick={onClose}>
            {copy.hosts.cancel}
          </button>
          <button className="primaryButton dangerButton" disabled={busy} type="button" onClick={onDelete}>
            {copy.hosts.delete}
          </button>
        </ModalActions>
      </AlertModalFrame>
  );
}

function SkillDeleteModal({
  busy,
  copy,
  skill,
  onClose,
  onDelete
}: {
  busy: boolean;
  copy: UICopy;
  skill: SkillPack;
  onClose: () => void;
  onDelete: (uninstallFirst: boolean) => void;
}) {
  return (
      <AlertModalFrame busy={busy} className="taskLogModal skillModal" titleId="skill-delete-title" onCancel={onClose}>
        <ModalHeader
          className="taskLogModalHeader"
          titleId="skill-delete-title"
          title={`${copy.skills.deleteTitle}: ${skill.name}`}
          description={copy.skills.deleteBody}
          icon="delete"
          closeAriaLabel={copy.hosts.cancel}
          closeDisabled={busy}
          onClose={onClose}
        />
        <ModalActions className="skillDeleteActions">
          <button className="secondaryButton" data-alert-cancel disabled={busy} type="button" onClick={onClose}>
            {copy.hosts.cancel}
          </button>
          <button className="secondaryButton dangerButton" disabled={busy} type="button" onClick={() => onDelete(false)}>
            {copy.skills.directDelete}
          </button>
          <button className="primaryButton" disabled={busy} type="button" onClick={() => onDelete(true)}>
            {copy.skills.uninstallAndDelete}
          </button>
        </ModalActions>
      </AlertModalFrame>
  );
}

export function TasksView({
  copy,
  hasMore,
  loadingMore,
  mockMode,
  requestedTaskId,
  tasks,
  onClearTaskHistory,
  onLoadMore,
  onRequestHandled,
  onTaskViewed
}: {
  copy: UICopy;
  hasMore: boolean;
  loadingMore: boolean;
  mockMode: boolean;
  requestedTaskId: string | null;
  tasks: TaskRun[];
  onClearTaskHistory: () => Promise<number>;
  onLoadMore: () => Promise<void>;
  onRequestHandled: () => void;
  onTaskViewed: (taskId: string) => void;
}) {
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [clearHistoryOpen, setClearHistoryOpen] = useState(false);
  const [clearingHistory, setClearingHistory] = useState(false);
  const [nowTick, setNowTick] = useState(() => Date.now());
  const reportActionError = useActionErrorReporter(copy);
  const selectedTask = selectedTaskId ? tasks.find((task) => task.id === selectedTaskId) ?? null : null;
  const completedTaskCount = tasks.filter((task) => task.status !== "queued" && task.status !== "running").length;

  useEffect(() => {
    const timer = window.setInterval(() => setNowTick(Date.now()), 30_000);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    if (selectedTaskId && !tasks.some((task) => task.id === selectedTaskId)) setSelectedTaskId(null);
  }, [selectedTaskId, tasks]);

  useEffect(() => {
    if (!requestedTaskId) return;
    if (tasks.some((task) => task.id === requestedTaskId)) setSelectedTaskId(requestedTaskId);
    onRequestHandled();
  }, [onRequestHandled, requestedTaskId, tasks]);

  const clearCompletedTaskHistory = async () => {
    setClearingHistory(true);
    try {
      await onClearTaskHistory();
      setClearHistoryOpen(false);
    } catch (error) {
      reportActionError(error);
    } finally {
      setClearingHistory(false);
    }
  };

  return (
    <div className="tasksGrid">
      <section className="panel spanWide">
        <div className="panelHeader compact">
          <div>
            <TitleWithIcon icon="tasks" level={2}>{copy.tasks.taskHistory}</TitleWithIcon>
          </div>
          <button
            className="secondaryButton dangerButton"
            disabled={clearingHistory || completedTaskCount === 0}
            type="button"
            onClick={() => setClearHistoryOpen(true)}
          >
            {clearingHistory ? copy.tasks.clearingHistory : copy.tasks.clearHistory}
          </button>
        </div>
        {tasks.length === 0 ? (
          <EmptyListState copy={copy} message={copy.emptyLists.tasks} variant="tasks" />
        ) : (
          <div className="tableWrap taskTableWrap">
            <table className="tasksTable">
              <thead>
                <tr>
                  <th>{copy.tasks.action}</th>
                  <th>{copy.tasks.host}</th>
                  <th>{copy.tasks.status}</th>
                  <th>{copy.tasks.started}</th>
                  <th>{copy.tasks.summary}</th>
                  <th className="taskDetailsCol">{copy.tasks.details}</th>
                </tr>
              </thead>
              <tbody>
                {tasks.map((task) => (
                  <tr key={task.id}>
                    <td><strong>{localizeTaskAction(task.action, copy)}</strong></td>
                    <td>{task.hostName}</td>
                    <td><TaskStatusBadge copy={copy} status={task.status} /></td>
                    <td>{formatTaskTimestamp(task, copy, nowTick)}</td>
                    <td>{localizeTaskSummary(task, copy)}</td>
                    <td className="taskDetailsCol">
                      <button className="miniButton" type="button" onClick={() => {
                        setSelectedTaskId(task.id);
                        onTaskViewed(task.id);
                      }}>{copy.tasks.logs}</button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {hasMore ? (
          <div className="taskHistoryMore">
            <button className="secondaryButton" disabled={loadingMore} type="button" onClick={() => void onLoadMore()}>
              {loadingMore ? copy.tasks.loadingMore : copy.tasks.loadMore}
            </button>
          </div>
        ) : null}
      </section>
      {selectedTask ? (
        <TaskLogModal
          copy={copy}
          now={nowTick}
          task={selectedTask}
          onClose={() => setSelectedTaskId(null)}
        />
      ) : null}
      <ConfirmDialog
        busy={clearingHistory}
        copy={{
          title: copy.tasks.clearHistoryTitle,
          body: mockMode ? copy.tasks.clearHistoryMockBody : copy.tasks.clearHistoryBody,
          cancel: copy.hosts.cancel,
          confirm: clearingHistory ? copy.tasks.clearingHistory : copy.tasks.clearHistoryConfirm
        }}
        open={clearHistoryOpen}
        onCancel={() => setClearHistoryOpen(false)}
        onConfirm={() => void clearCompletedTaskHistory()}
      />
    </div>
  );
}

function TaskLogModal({
  copy,
  now,
  task,
  onClose,
  footer
}: {
  copy: UICopy;
  now: number;
  task: TaskRun;
  onClose: () => void;
  footer?: ReactNode;
}) {
  const statusTone = task.status === "success" ? "green" : task.status === "failed" ? "red" : task.status === "running" ? "yellow" : "gray";
  return (
    <div className="modalBackdrop" role="presentation">
      <ModalFrame className="codexOperationModal taskLogDetailModal" titleId="task-log-modal-title">
        <ModalHeader
          className="codexOperationHeader"
          titleId="task-log-modal-title"
          title={localizeTaskAction(task.action, copy)}
          icon="tasks"
          badge={<Badge tone={statusTone}>{copy.status.task[task.status]}</Badge>}
          closeAriaLabel={copy.codexOperation.close}
          onClose={onClose}
        />
        <div className="codexOperationSummary taskLogSummary">
          <span>{copy.codexOperation.summary}</span>
          <strong>{localizeTaskSummary(task, copy)}</strong>
          <small>{`${copy.tasks.host}: ${task.hostName} | ${copy.tasks.started}: ${formatTaskTimestamp(task, copy, now)}`}</small>
        </div>
        <div className="codexOperationLog">
          <div className="codexOperationLogTitle">
            <span>{copy.codexOperation.latestLog}</span>
            {task.status === "running" ? <i aria-hidden="true" /> : null}
          </div>
          <div className="codexOperationLogRows taskLogFlowRows">
            {task.logs.length > 0 ? task.logs.map((log) => (
              <details className="taskLogFlowRow" data-level={log.level} key={log.id} open={task.status === "failed" && log.level === "error"}>
                <summary className="codexOperationLogRow" data-level={log.level}>
                  <strong>{copy.status.log[log.level]}</strong>
                  <span>{log.message}</span>
                </summary>
                <div className="taskLogFlowDetails">
                  <div className="taskLogMetaGrid">
                    <div>
                      <span>{copy.tasks.command}</span>
                      <code>{log.command ?? "-"}</code>
                    </div>
                    <div>
                      <span>{copy.tasks.exitCode}</span>
                      <code>{log.exitCode ?? "-"}</code>
                    </div>
                    <div>
                      <span>{copy.tasks.duration}</span>
                      <code>{typeof log.durationMs === "number" ? `${log.durationMs} ms` : "-"}</code>
                    </div>
                    <div>
                      <span>{copy.tasks.timedOut}</span>
                      <code>{log.timedOut ? copy.hosts.yes : copy.hosts.no}</code>
                    </div>
                  </div>
                  <div className="taskLogStreamGrid">
                    <div>
                      <span>{copy.tasks.stdout}</span>
                      <pre>{log.stdout || copy.tasks.noOutput}</pre>
                    </div>
                    <div>
                      <span>{copy.tasks.stderr}</span>
                      <pre>{log.stderr || copy.tasks.noOutput}</pre>
                    </div>
                  </div>
                </div>
              </details>
            )) : (
              <div className="codexOperationLogRow" data-level="info">
                <strong>{copy.status.log.info}</strong>
                <span>{copy.tasks.noLogs}</span>
              </div>
            )}
          </div>
        </div>
        {footer}
      </ModalFrame>
    </div>
  );
}

function SettingsView({
  appUpdateChecking,
  appUpdateInstalling,
  appUpdateStatus,
  copy,
  settings,
  settingsSaveError,
  settingsSaving,
  sshStatus,
  onCheckStableUpdate,
  onInstallStableUpdate,
  onCloseButtonBehaviorChange,
  onCopyPublicKey,
  onFontPresetChange,
  onNetworkProxyModeChange,
  onNetworkProxyManualRequest,
  onPlatformAppearanceChange,
  onRefreshSsh,
  onRetrySettings,
  onSidebarCompletionIndicatorsChange,
  onThemeChange
}: {
  appUpdateChecking: boolean;
  appUpdateInstalling: boolean;
  appUpdateStatus: AppUpdateStatus;
  copy: UICopy;
  settings: AppSettings;
  settingsSaveError: string | null;
  settingsSaving: boolean;
  sshStatus: SshStatus | null;
  onCheckStableUpdate: () => Promise<unknown>;
  onInstallStableUpdate: () => Promise<unknown>;
  onCloseButtonBehaviorChange: (behavior: CloseButtonBehavior) => void;
  onCopyPublicKey: (publicKey: string) => Promise<boolean>;
  onFontPresetChange: (fontPreset: FontPreset) => void;
  onNetworkProxyModeChange: (mode: NetworkProxyMode) => void;
  onNetworkProxyManualRequest: () => void;
  onPlatformAppearanceChange: (platformAppearance: PlatformAppearance) => void;
  onRefreshSsh: () => Promise<unknown>;
  onRetrySettings: () => Promise<boolean>;
  onSidebarCompletionIndicatorsChange: (enabled: boolean) => void;
  onThemeChange: (theme: ThemeChoice) => void;
}) {
  const publicKey = sshStatus?.ed25519.publicKey ?? "";
  const appUpdateBusy = appUpdateChecking || appUpdateInstalling;
  const canCheckStableUpdate = appUpdateStatus.channel === "stable" && !appUpdateBusy;
  const canInstallStableUpdate =
    appUpdateStatus.channel === "stable" && appUpdateStatus.configured && appUpdateStatus.state === "available" && !appUpdateBusy;
  const appLatestVersionLabel = appUpdateLatestVersionLabel(appUpdateStatus, copy);
  const [publicKeyCopied, setPublicKeyCopied] = useState(false);

  useEffect(() => {
    setPublicKeyCopied(false);
  }, [publicKey]);

  const handleCopyPublicKey = async () => {
    if (!publicKey) return;
    const copied = await onCopyPublicKey(publicKey);
    setPublicKeyCopied(copied);
  };

  const handleNetworkProxyChoice = (choice: NetworkProxyMode) => {
    if (choice === "manual") {
      onNetworkProxyManualRequest();
      return;
    }
    onNetworkProxyModeChange(choice);
  };

  return (
    <div className="settingsGrid">
      {settingsSaveError || settingsSaving ? (
        <section className="panel spanWide" role={settingsSaveError ? "alert" : "status"} aria-live={settingsSaveError ? undefined : "polite"}>
          <div className="panelHeader compact">
            <div>
              <TitleWithIcon icon={settingsSaveError ? "warning" : "settings"} level={2}>
                {settingsSaveError ? copy.settings.settingsSaveFailed : copy.settings.settingsSaving}
              </TitleWithIcon>
              {settingsSaveError ? <p className="mutedText">{settingsSaveError}</p> : null}
            </div>
            {settingsSaveError ? (
              <button className="secondaryButton" disabled={settingsSaving} type="button" onClick={() => void onRetrySettings()}>
                {copy.settings.settingsRetry}
              </button>
            ) : null}
          </div>
        </section>
      ) : null}
      <section className="panel spanWide">
        <div className="panelHeader compact">
          <div>
            <TitleWithIcon icon="settings" level={2}>{copy.settings.appearance}</TitleWithIcon>
          </div>
        </div>
        <div className="settingsRows dividedSettingsRows appearanceRows">
          <div className="settingControlRow" data-divider="true">
            <span>{copy.settings.theme}</span>
            <div className="segmentedControl" role="group" aria-label={copy.settings.theme}>
              {(["system", "light", "dark"] as ThemeChoice[]).map((choice) => (
                <button data-active={settings.theme === choice} disabled={settingsSaving} key={choice} onClick={() => onThemeChange(choice)} type="button">
                  {copy.settings.themeOptions[choice]}
                </button>
              ))}
            </div>
          </div>

          <div className="settingControlRow">
            <span>{copy.settings.platformAppearance}</span>
            <div className="segmentedControl" role="group" aria-label={copy.settings.platformAppearance}>
              {(["auto", "windows", "macos"] as PlatformAppearance[]).map((choice) => (
                <button data-active={settings.platformAppearance === choice} disabled={settingsSaving} key={choice} onClick={() => onPlatformAppearanceChange(choice)} type="button">
                  {copy.settings.platformOptions[choice]}
                </button>
              ))}
            </div>
          </div>

          <div className="settingControlRow">
            <span>{copy.settings.font}</span>
            <div className="segmentedControl" data-options="2" role="group" aria-label={copy.settings.font}>
              {(Object.keys(fontPresets) as FontPreset[]).map((preset) => (
                <button data-active={settings.fontPreset === preset} disabled={settingsSaving} key={preset} onClick={() => onFontPresetChange(preset)} type="button">
                  {fontPresets[preset].label}
                </button>
              ))}
            </div>
          </div>

          <div className="settingControlRow" data-divider="true">
            <span>{copy.settings.sidebarCompletionIndicators}</span>
            <button
              className="pillToggle"
              data-enabled={settings.sidebarCompletionIndicators}
              role="switch"
              aria-checked={settings.sidebarCompletionIndicators}
              aria-label={copy.settings.sidebarCompletionIndicators}
              disabled={settingsSaving}
              type="button"
              onClick={() => onSidebarCompletionIndicatorsChange(!settings.sidebarCompletionIndicators)}
            >
              <span className="pillToggleThumb" aria-hidden="true" />
            </button>
          </div>
        </div>
      </section>

      <section className="panel spanWide">
        <div className="panelHeader compact">
          <div>
            <TitleWithIcon icon="key" level={2}>{copy.settings.localSsh}</TitleWithIcon>
          </div>
          <CommandBar ariaLabel={copy.settings.localSsh} className="topActions">
            <button className="secondaryButton" type="button" onClick={() => void onRefreshSsh()}>{copy.settings.refresh}</button>
            <button className="primaryButton copyPublicKeyButton" data-success={publicKeyCopied} disabled={!publicKey} type="button" onClick={() => void handleCopyPublicKey()}>
              {publicKeyCopied ? copy.settings.copyPublicKeySuccess : copy.settings.copyPublicKey}
            </button>
          </CommandBar>
        </div>

        <div className="keyStatusGrid">
          <KeyStatusCard copy={copy} keyInfo={sshStatus?.ed25519} title="Ed25519" />
        </div>
      </section>

      <section className="panel spanWide appUpdatePanel">
        <div className="panelHeader compact">
          <div>
            <TitleWithIcon icon="update" level={2}>{copy.settings.appUpdates}</TitleWithIcon>
            <p className="appUpdateSchedule">{copy.settings.dailyUpdateCheck}</p>
          </div>
          <CommandBar ariaLabel={copy.settings.appUpdates} className="topActions">
            <button className="secondaryButton" disabled={!canCheckStableUpdate} type="button" onClick={() => void onCheckStableUpdate()}>
              {appUpdateChecking ? copy.settings.updateChecking : copy.settings.checkStableUpdate}
            </button>
            <button className="primaryButton" disabled={!canInstallStableUpdate} type="button" onClick={() => void onInstallStableUpdate()}>
              {appUpdateInstalling ? copy.settings.updateInstalling : copy.settings.installStableUpdate}
            </button>
          </CommandBar>
        </div>

        <div className="tableWrap versionInfoTableWrap">
          <table className="sshHostsTable versionInfoTable">
            <thead>
              <tr>
                <th>{copy.settings.softwareName}</th>
                <th>{copy.settings.currentVersion}</th>
                <th>{copy.settings.installedAt}</th>
                <th>{copy.settings.latestVersion}</th>
                <th>{copy.settings.updatedAt}</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td><strong>{appUpdateStatus.softwareName}</strong></td>
                <td>
                  <Badge tone={appVersionTone(appUpdateStatus.currentVersion, appUpdateStatus.latestVersion)}>
                    {appUpdateStatus.currentVersion}
                  </Badge>
                </td>
                <td>{appUpdateStatus.installedAt ?? copy.settings.unknown}</td>
                <td>
                  <Badge tone={appLatestVersionTone(appUpdateStatus)} title={appUpdateStatus.message}>
                    {appLatestVersionLabel}
                  </Badge>
                </td>
                <td>{appUpdateStatus.checkedAt ?? copy.settings.notChecked}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>

      <section className="panel spanWide">
        <div className="panelHeader compact">
          <div>
            <TitleWithIcon icon="close" level={2}>{copy.settings.closeButton}</TitleWithIcon>
          </div>
        </div>
        <div className="settingsRows dividedSettingsRows">
          <div className="settingControlRow" data-divider="true">
            <span>{copy.settings.closeButtonBehavior}</span>
            <div className="segmentedControl" role="group" aria-label={copy.settings.closeButtonBehavior}>
              {(["ask", "exit", "minimize-to-tray"] as CloseButtonBehavior[]).map((choice) => (
                <button
                  data-active={settings.closeButtonBehavior === choice}
                  disabled={settingsSaving}
                  key={choice}
                  onClick={() => onCloseButtonBehaviorChange(choice)}
                  type="button"
                >
                  {copy.settings.closeButtonOptions[choice]}
                </button>
              ))}
            </div>
          </div>

          <div className="settingControlRow">
            <span>{copy.settings.networkProxy}</span>
            <div className="segmentedControl networkProxyControl" role="group" aria-label={copy.settings.networkProxy}>
              {(["auto", "direct", "manual"] as NetworkProxyMode[]).map((choice) => (
                <button
                  data-active={settings.networkProxyMode === choice}
                  disabled={settingsSaving}
                  key={choice}
                  onClick={() => handleNetworkProxyChoice(choice)}
                  type="button"
                >
                  {copy.settings.networkProxyOptions[choice]}
                </button>
              ))}
            </div>
          </div>
        </div>
      </section>
    </div>
  );
}

function KeyStatusCard({ copy, keyInfo, title }: { copy: UICopy; keyInfo: SshKeyInfo | undefined; title: string }) {
  return (
    <article className="keyStatusCard">
      <div className="hostHeader">
        <TitleWithIcon icon="key" level={3}>{title}</TitleWithIcon>
        <Badge tone={keyInfo?.privateExists ? "green" : "gray"}>{keyInfo?.privateExists ? copy.settings.privateFound : copy.settings.missing}</Badge>
      </div>
      <div className="keyStatusBadges">
        <Badge tone={keyInfo?.publicExists ? "green" : "gray"}>{keyInfo?.publicExists ? copy.settings.publicKey : copy.settings.noPublicKey}</Badge>
      </div>
    </article>
  );
}

function Badge({ children, tone, title }: { children: ReactNode; tone: BadgeTone; title?: string | null }) {
  return <span className="badge" data-tone={tone} title={title ?? undefined}>{children}</span>;
}

function StatusBadge({ copy, status }: { copy: UICopy; status: HostStatus }) {
  return <Badge tone={hostStatusTone(status)}>{copy.status.host[status]}</Badge>;
}

function HostDetailValueBadge({ label, tone }: { label: string; tone: BadgeTone }) {
  return <Badge tone={tone}>{label}</Badge>;
}

function hostStatusTone(status: HostStatus): BadgeTone {
  if (status === "online") return "green";
  if (status === "offline") return "red";
  if (status === "testing") return "yellow";
  return "gray";
}

function HostApiConfigBadge({ copy, host, profileById }: { copy: UICopy; host: Host; profileById?: Map<string, Profile> }) {
  if (!hostApiConfigChecked(host)) {
    return <Badge tone="gray">{copy.profiles.unknownApiConfig}</Badge>;
  }
  const source = host.apiConfigSource ?? (host.configExists === false ? "none" : "unknown");
  if (source === "none" || host.configExists === false) {
    return <Badge tone="gray">{copy.profiles.noApiConfig}</Badge>;
  }
  if (source === "unknown") {
    return <Badge tone="yellow">{copy.profiles.unknownApiConfig}</Badge>;
  }
  const label = host.apiConfigName
    ?? (source === "profile" && host.profileId ? profileById?.get(host.profileId)?.name ?? host.profileId : null)
    ?? copy.profiles.unknownApiConfig;
  const envTitle = host.apiKeyEnvVar ? copy.hosts.apiEnvStatusTitle(host.apiKeyEnvVar) : undefined;
  if (host.apiKeyEnvPresent === false) {
    return <Badge tone="red" title={envTitle}>{`${label} · ${copy.hosts.apiEnvMissing}`}</Badge>;
  }
  if (host.apiKeyEnvPresent === true) {
    return <Badge tone={source === "cc-switch" ? "green" : "blue"} title={envTitle}>{label}</Badge>;
  }
  return <Badge tone={source === "cc-switch" ? "yellow" : "blue"} title={envTitle}>{label}</Badge>;
}

function hostApiConfigChecked(host: Host) {
  return isHostCodexTested(host) && (host.configExists !== null || Boolean(host.apiConfigSource || host.apiConfigName));
}

function profileMatchesConfirmedHostApiConfig(profile: Profile, host: Host) {
  if (!hostApiConfigChecked(host)) return false;
  const source = host.apiConfigSource ?? (host.configExists === false ? "none" : "unknown");
  if (source !== "profile" && source !== "cc-switch") return false;
  const profileIdMatches = host.profileId === profile.id;
  const profileNameMatches = normalizeApiConfigName(host.apiConfigName) === normalizeApiConfigName(profile.name);
  if (source === "cc-switch") {
    return profile.source === "cc-switch" && (profileIdMatches || profileNameMatches);
  }
  return profileIdMatches || profileNameMatches;
}

function normalizeApiConfigName(value: string | null | undefined) {
  return value?.trim().toLowerCase() ?? "";
}

function ProfileStorageBadge({ copy, profile }: { copy: UICopy; profile: Profile }) {
  const isThirdPartyImport = profile.source === "cc-switch";
  return (
    <Badge tone={isThirdPartyImport ? "green" : "blue"}>
      {isThirdPartyImport ? copy.profiles.thirdPartyImport : copy.profiles.localStorageLabel}
    </Badge>
  );
}

function normalizeTaskRunsForUi(tasks: TaskRun[]) {
  const receivedAt = new Date().toISOString();
  return tasks.slice(0, MAX_VISIBLE_TASKS).map((task) => normalizeTaskRunForUi(task, receivedAt));
}

function mergeTaskRunsForUi(primary: TaskRun[], secondary: TaskRun[]) {
  const seen = new Set<string>();
  return [...primary, ...secondary].filter((task) => {
    if (seen.has(task.id)) return false;
    seen.add(task.id);
    return true;
  }).slice(0, MAX_VISIBLE_TASKS);
}

function normalizeTaskRunForUi(task: TaskRun, receivedAt = new Date().toISOString()): TaskRun {
  const hasTimestamp = Boolean(taskTimestampMillis(task));
  const startedAt = hasTimestamp || !isNowTimeLabel(task.startedAt) ? task.startedAt : receivedAt;
  const endedAt = hasTimestamp || !task.endedAt || !isNowTimeLabel(task.endedAt) ? task.endedAt : receivedAt;
  const logs = task.logs.map((log) => (isNowTimeLabel(log.timestamp) ? { ...log, timestamp: receivedAt } : log));
  return { ...task, startedAt, endedAt, logs };
}

function latestAppUpdateTask(tasks: TaskRun[]) {
  return tasks.find((task) => task.action === "Check app update") ?? null;
}

function latestAppInstallTask(tasks: TaskRun[]) {
  return tasks.find((task) => task.action === "Install app update") ?? null;
}

function TaskStatusBadge({ copy, status }: { copy: UICopy; status: TaskStatus }) {
  const tone: BadgeTone = status === "success" ? "green" : status === "failed" ? "red" : status === "running" ? "yellow" : "gray";
  return <Badge tone={tone}>{copy.status.task[status]}</Badge>;
}

function localizeTaskAction(action: string, copy: UICopy) {
  const labels = copy.tasks.actionLabels as Record<string, string>;
  return labels[action] ?? (copy.common.locale === "zh-CN" ? copy.tasks.unknownAction : action);
}

function localizeTaskSummary(task: TaskRun, copy: UICopy) {
  if (copy.common.locale !== "zh-CN") return task.summary;
  const summaries = copy.tasks.summaryByStatus as Record<TaskStatus, (action: string) => string>;
  return summaries[task.status](localizeTaskAction(task.action, copy));
}

function formatTaskTimestamp(task: TaskRun, copy: UICopy, now = Date.now()) {
  const timestamp = taskTimestampMillis(task);
  if (!timestamp) return task.startedAt || "-";
  const date = new Date(timestamp);
  const deltaMs = Math.max(0, now - timestamp);
  const zh = copy.common.locale === "zh-CN";
  if (deltaMs < 60_000) return copy.common.justNow;
  if (deltaMs < 60 * 60_000) {
    const minutes = Math.max(1, Math.floor(deltaMs / 60_000));
    return copy.tasks.minutesAgo(minutes);
  }
  const sameYear = new Date(now).getFullYear() === date.getFullYear();
  return new Intl.DateTimeFormat(zh ? "zh-CN" : "en-US", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    ...(sameYear ? {} : { year: "numeric" })
  }).format(date);
}

function taskTimestampMillis(task: TaskRun) {
  const direct = parseTaskTimeValue(task.startedAt);
  if (direct) return direct;
  if (isNowTimeLabel(task.startedAt)) {
    return parseTaskIdTimestamp(task.id) ?? parseTaskTimeValue(task.logs[0]?.timestamp);
  }
  return parseTaskTimeValue(task.logs[0]?.timestamp);
}

function parseTaskTimeValue(value: string | undefined) {
  const normalized = value?.trim();
  if (!normalized || isNowTimeLabel(normalized)) return null;
  if (/^\d{12,}$/.test(normalized)) return Number(normalized);
  const parsed = Date.parse(normalized);
  return Number.isNaN(parsed) ? null : parsed;
}

function isNowTimeLabel(value: string | undefined) {
  return value?.trim().toLowerCase() === "now";
}

function parseTaskIdTimestamp(id: string) {
  const match = id.match(/(\d{12,})$/);
  return match ? Number(match[1]) : null;
}

function hostCodexStatus(
  copy: UICopy,
  host: Host | null,
  busy: HostBusyAction | undefined,
  hosts: Host[],
  latestCodexVersion: LatestCodexVersion | null
): { label: string; tone: BadgeTone } {
  if (!host) return { label: copy.profiles.notChecked, tone: "gray" };
  if (busy === "test") return { label: copy.hosts.testing, tone: "yellow" };
  if (busy === "check-version") return { label: copy.hosts.checkingVersion, tone: "yellow" };
  if (busy === "install") return { label: copy.hosts.installingCodex, tone: "yellow" };
  if (busy === "update") return { label: copy.hosts.updatingCodex, tone: "yellow" };
  if (!isHostCodexTested(host)) return { label: copy.hosts.unknown, tone: "gray" };
  if (!host.codexInstalled) {
    return { label: copy.profiles.notInstalled, tone: "gray" };
  }
  if (host.codexCommandAvailable === false) {
    return { label: copy.hosts.codexPathMissing, tone: "yellow" };
  }
  const label = formatCodexVersionLabel(host.codexVersion, copy.profiles.notChecked);
  return { label, tone: codexVersionTone(host.codexVersion, hosts, latestCodexVersion?.version) };
}

function hostCodexInstalledStatus(copy: UICopy, host: Host | null): { label: string; tone: BadgeTone } {
  if (!host || !isHostCodexTested(host)) return { label: copy.profiles.notChecked, tone: "gray" };
  return { label: formatBoolean(host.codexInstalled, copy), tone: booleanTone(host.codexInstalled) };
}

function isHostCodexTested(host: Host | null) {
  if (!host || host.status !== "online") return false;
  const version = host.codexVersion.trim().toLowerCase();
  return Boolean(version && version !== "pending" && version !== "unknown");
}

function latestCodexStatus(copy: UICopy, latestCodexVersion: LatestCodexVersion | null): { label: string; tone: BadgeTone; title?: string | null } {
  if (latestCodexVersion?.version) {
    return {
      label: formatCodexVersionLabel(latestCodexVersion.version, latestCodexVersion.version),
      tone: "green",
      title: latestCodexVersion.error
    };
  }
  return { label: copy.hosts.latestCodexUnknown, tone: "gray", title: latestCodexVersion?.error };
}

function formatCodexVersionLabel(value: string | null | undefined, fallback: string) {
  const parsed = parseCodexVersion(value);
  if (parsed) return parsed.label;
  const normalized = value?.trim();
  return normalized || fallback;
}

function parseCodexVersion(value: string | null | undefined): { label: string; parts: number[] } | null {
  const normalized = value?.trim();
  if (!normalized) return null;
  const match = normalized.match(/(?:^|[\s@/:-])v?(\d+(?:\.\d+){1,3})(?:[-+][0-9A-Za-z._-]+)?/i);
  if (!match) return null;
  const label = match[1];
  return {
    label,
    parts: label.split(".").map((part) => Number.parseInt(part, 10))
  };
}

function compareVersionParts(left: number[], right: number[]) {
  const length = Math.max(left.length, right.length);
  for (let index = 0; index < length; index += 1) {
    const leftPart = left[index] ?? 0;
    const rightPart = right[index] ?? 0;
    if (leftPart !== rightPart) return leftPart - rightPart;
  }
  return 0;
}

function isCodexVersionBehind(current: string | null | undefined, latest: string | null | undefined) {
  const currentVersion = parseCodexVersion(current);
  const latestVersion = parseCodexVersion(latest);
  return Boolean(currentVersion && latestVersion && compareVersionParts(currentVersion.parts, latestVersion.parts) < 0);
}

function appVersionTone(current: string | null | undefined, latest: string | null | undefined): BadgeTone {
  return isCodexVersionBehind(current, latest) ? "red" : "green";
}

function appUpdateLatestVersionLabel(status: AppUpdateStatus, copy: UICopy) {
  if (status.latestVersion) return status.latestVersion;
  if (status.state === "error" && status.checkedAt) return copy.settings.checkFailed;
  if (status.state === "pending-configuration") return copy.settings.pendingConfiguration;
  return copy.settings.notChecked;
}

function appLatestVersionTone(status: AppUpdateStatus): BadgeTone {
  if (parseCodexVersion(status.latestVersion)) return "green";
  if (status.state === "error" && status.checkedAt) return "red";
  if (status.state === "pending-configuration") return "yellow";
  return "gray";
}

function codexVersionTone(current: string | null | undefined, hosts: Host[], latest: string | null | undefined): BadgeTone {
  const currentVersion = parseCodexVersion(current);
  if (!currentVersion) return "gray";
  const latestVersion = parseCodexVersion(latest);
  if (latestVersion) {
    if (compareVersionParts(currentVersion.parts, latestVersion.parts) >= 0) return "green";
    const versions = uniqueSortedCodexVersions([
      ...hosts.filter((host) => host.codexInstalled).map((host) => host.codexVersion),
      latest
    ]);
    return isLowestCodexVersion(currentVersion.parts, versions) ? "red" : "yellow";
  }
  const versions = uniqueSortedCodexVersions(hosts.filter((host) => host.codexInstalled).map((host) => host.codexVersion));
  if (versions.length <= 1) return "green";
  if (isHighestCodexVersion(currentVersion.parts, versions)) return "green";
  if (isLowestCodexVersion(currentVersion.parts, versions)) return "red";
  return "yellow";
}

function uniqueSortedCodexVersions(values: Array<string | null | undefined>) {
  const parsed = values
    .map(parseCodexVersion)
    .filter((version): version is { label: string; parts: number[] } => Boolean(version));
  const unique = new Map<string, number[]>();
  for (const version of parsed) {
    unique.set(version.parts.join("."), version.parts);
  }
  return Array.from(unique.values()).sort(compareVersionParts);
}

function isLowestCodexVersion(parts: number[], versions: number[][]) {
  return versions.length > 0 && compareVersionParts(parts, versions[0]) === 0;
}

function isHighestCodexVersion(parts: number[], versions: number[][]) {
  return versions.length > 0 && compareVersionParts(parts, versions[versions.length - 1]) === 0;
}

function sshHostSourceLabel(copy: UICopy, host: SshConfigHost) {
  return host.managed || host.source === "managed" ? copy.hosts.codexhubManaged : copy.hosts.localSource;
}

function knownHostValue(value: string | null | undefined, copy: UICopy) {
  const normalized = value?.trim();
  return normalized && normalized.toLowerCase() !== "unknown" ? normalized : copy.hosts.unknown;
}

function hostSystemLabel(host: Host, copy: UICopy) {
  const os = knownHostValue(host.os, copy);
  const arch = knownHostValue(host.arch, copy);
  if (arch === copy.hosts.unknown) return os;
  if (os === copy.hosts.unknown) return arch;
  return `${os} / ${arch}`;
}

function knownValueTone(value: string | null | undefined, copy: UICopy): BadgeTone {
  return knownHostValue(value, copy) === copy.hosts.unknown ? "gray" : "blue";
}

function latencyTone(value: number | null | undefined, hosts: Host[]): BadgeTone {
  if (typeof value !== "number") return "gray";
  const values = hosts
    .map((host) => host.latencyMs)
    .filter((latency): latency is number => typeof latency === "number");
  if (values.length <= 1) return "green";
  const min = Math.min(...values);
  const max = Math.max(...values);
  if (min === max) return "green";
  const relative = (value - min) / (max - min);
  if (relative <= 0.34) return "green";
  if (relative >= 0.67) return "red";
  return "yellow";
}

function dashboardHostSkillCount(
  host: Host,
  inventory: SkillInventoryStatus["hostInventories"][number] | undefined
): number | null {
  if (inventory?.ok) return installedSkillTags(inventory.skills, "host", inventory.hostAlias).length;
  return typeof host.skillsCount === "number" ? host.skillsCount : null;
}

function dashboardSkillCountTone(value: number | null | undefined, counts: Array<number | null>): BadgeTone {
  if (typeof value !== "number") return "gray";
  const values = counts.filter((count): count is number => typeof count === "number");
  if (values.length === 0) return "gray";
  const min = Math.min(...values);
  const max = Math.max(...values);
  if (max <= 0) return "gray";
  if (values.length <= 1 || min === max) return "green";
  const relative = (value - min) / (max - min);
  if (relative <= 0.34) return "red";
  if (relative >= 0.67) return "green";
  return "yellow";
}

function booleanTone(value: boolean | null | undefined): BadgeTone {
  return value ? "green" : "gray";
}

function archTone(value: string | null | undefined, copy: UICopy): BadgeTone {
  const arch = knownHostValue(value, copy).toLowerCase();
  if (arch === copy.hosts.unknown.toLowerCase()) return "gray";
  if (arch.includes("aarch64") || arch.includes("arm64") || arch.includes("arm")) return "green";
  if (arch.includes("x86_64") || arch.includes("amd64")) return "blue";
  return "yellow";
}

function hostSourceLabel(copy: UICopy, host: Host) {
  if (host.source === "managed") return copy.hosts.codexhubManaged;
  if (host.source === "local") return copy.hosts.localSource;
  return host.source || copy.hosts.unknown;
}

function remoteCodexButtonLabel(copy: UICopy, busy: HostBusyAction | undefined, action: RemoteCodexAction) {
  if (busy === action) {
    if (action === "check-version") return copy.hosts.checkingVersion;
    if (action === "install") return copy.hosts.installingCodex;
    if (action === "uninstall") return copy.hosts.uninstallingCodex;
    return copy.hosts.updatingCodex;
  }
  if (action === "check-version") return copy.hosts.checkVersion;
  if (action === "install") return copy.hosts.installCodex;
  if (action === "uninstall") return copy.hosts.uninstallCodex;
  return copy.hosts.updateCodex;
}

function codexOperationHint(copy: UICopy, action: RemoteCodexAction) {
  if (action === "install") return copy.codexOperation.installHint;
  if (action === "uninstall") return copy.codexOperation.uninstallHint;
  return copy.codexOperation.updateHint;
}

function progressLogLevel(log: RemoteCodexProgressEvent): "info" | "warn" | "error" {
  if (log.status === "failed" || log.status === "stderr") return "error";
  if (log.status === "heartbeat") return "warn";
  return "info";
}

function progressLogLabel(copy: UICopy, log: RemoteCodexProgressEvent) {
  return copy.status.log[progressLogLevel(log)];
}

function waitForNextFrame() {
  return new Promise<void>((resolve) => {
    window.requestAnimationFrame(() => resolve());
  });
}

function formatEndpoint(host: Host) {
  const user = host.username ? `${host.username}@` : "";
  return `${user}${host.address}:${host.port}`;
}

function formatBoolean(value: boolean, copy: UICopy) {
  return value ? copy.hosts.yes : copy.hosts.no;
}

function formatLatency(value: number | null | undefined, copy: UICopy) {
  return typeof value === "number" ? `${value} ms` : copy.hosts.unknown;
}

function resourceStatusTone(status: HostResourceSnapshot["status"]): BadgeTone {
  if (status === "ok") return "green";
  if (status === "partial") return "yellow";
  return "red";
}

function resourceStatusLabel(status: HostResourceSnapshot["status"], copy: UICopy) {
  if (status === "ok") return copy.monitor.statusOnline;
  if (status === "partial") return copy.monitor.statusPartial;
  return copy.monitor.statusFailed;
}

function monitorCpuPercent(cpu: HostResourceSnapshot["cpu"] | null | undefined) {
  if (typeof cpu?.usagePercent === "number") return cpu.usagePercent;
  if (typeof cpu?.load1 === "number" && typeof cpu.cores === "number" && cpu.cores > 0) {
    return Math.min(100, Math.max(0, cpu.load1 / cpu.cores * 100));
  }
  return null;
}

function summarizeGpuMemory(gpus: HostResourceSnapshot["gpus"], copy: UICopy) {
  if (gpus.length === 0) return { detail: copy.monitor.noGpu, percent: null };
  let usedBytes = 0;
  let totalBytes = 0;
  let hasUsed = false;
  let hasTotal = false;
  for (const gpu of gpus) {
    if (typeof gpu.memoryUsedBytes === "number") {
      usedBytes += gpu.memoryUsedBytes;
      hasUsed = true;
    }
    if (typeof gpu.memoryTotalBytes === "number") {
      totalBytes += gpu.memoryTotalBytes;
      hasTotal = true;
    }
  }
  if (!hasTotal) {
    return { detail: copy.monitor.detectedOnly, percent: 0 };
  }
  const used = hasUsed ? usedBytes : 0;
  return {
    detail: `${formatGpuMemoryBytes(used, copy)} / ${formatGpuMemoryBytes(totalBytes, copy)}`,
    percent: totalBytes > 0 ? used / totalBytes * 100 : 0
  };
}

function formatGpuMemoryBytes(value: number | null | undefined, copy: UICopy) {
  if (value === 0) return "0 MB";
  return formatBytes(value, copy);
}

function formatPercent(value: number | null | undefined, copy: UICopy) {
  return typeof value === "number" ? `${value.toFixed(1)}%` : copy.hosts.unknown;
}

function formatCompactPercent(value: number | null | undefined, copy: UICopy) {
  if (typeof value !== "number") return copy.hosts.unknown;
  return `${Number.isInteger(value) ? value.toFixed(0) : value.toFixed(1)}%`;
}

function formatCpuLoadSummary(cpu: HostResourceSnapshot["cpu"] | null | undefined, copy: UICopy) {
  const load = typeof cpu?.load1 === "number" ? cpu.load1.toFixed(1) : copy.hosts.unknown;
  const cores = typeof cpu?.cores === "number" ? String(cpu.cores) : copy.hosts.unknown;
  return `${load} / ${cores}`;
}

function formatMonitorMetricDetail(label: string, value: string, copy: UICopy) {
  return `${label}${copy.common.keyValueSeparator}${value}`;
}

function formatBytes(value: number | null | undefined, copy: UICopy) {
  if (typeof value !== "number") return copy.hosts.unknown;
  const units = ["B", "KB", "MB", "GB", "TB"];
  let next = value;
  let unit = 0;
  while (next >= 1024 && unit < units.length - 1) {
    next /= 1024;
    unit += 1;
  }
  return `${next >= 10 || unit === 0 ? next.toFixed(0) : next.toFixed(1)} ${units[unit]}`;
}

function isZhCopy(copy: UICopy) {
  return copy.common.locale === "zh-CN";
}

function formatMonitorTimestamp(value: string | null, copy: UICopy) {
  if (!value) return copy.monitor.never;
  const timestamp = Date.parse(value);
  if (!Number.isFinite(timestamp)) return value;
  const zh = isZhCopy(copy);
  return new Intl.DateTimeFormat(zh ? "zh-CN" : "en-US", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit"
  }).format(new Date(timestamp));
}

function formatGpuName(name: string) {
  const compact = name
    .replace(/\bNVIDIA\b/gi, "")
    .replace(/\bGeForce\b/gi, "")
    .replace(/\bGraphics\b/gi, "")
    .replace(/\bPCIe\b/gi, "")
    .replace(/\bGPU\b/gi, "")
    .replace(/\s+/g, " ")
    .trim();
  const match = compact.match(/\b(RTX\s?\d{4}|A\d{3,4}|GB\d+|H\d{3}|L\d{2,3})\b/i);
  return (match?.[1] ?? (compact || name)).replace(/\s+/g, " ");
}

function formatGpuCoreDetails(gpu: HostResourceSnapshot["gpus"][number], copy: UICopy) {
  if (gpu.status === "detected") return copy.monitor.detectedOnly;
  const memory = summarizeGpuMemory([gpu], copy).detail;
  return [
    formatCompactPercent(gpu.utilizationPercent, copy),
    formatWatts(gpu.powerWatts, copy),
    memory
  ].join(" · ");
}

function formatWatts(value: number | null | undefined, copy: UICopy) {
  return typeof value === "number" ? `${value.toFixed(0)} W` : copy.hosts.unknown;
}

function formatDuration(value: number | null | undefined, copy: UICopy) {
  if (typeof value !== "number") return copy.hosts.unknown;
  if (value >= 3600) {
    const hours = value / 3600;
    return `${hours >= 10 ? hours.toFixed(0) : hours.toFixed(1)} h`;
  }
  if (value >= 60) return `${Math.round(value / 60)} min`;
  return `${Math.max(0, Math.round(value))} s`;
}

function formatProcessCount(value: number, copy: UICopy) {
  return copy.monitor.processCountShort(value);
}

const MONITOR_UNKNOWN_GPU_USER = "unknown";
const MONITOR_UNKNOWN_GPU_USER_COLOR = "#7c3aed";
const MONITOR_GPU_USER_COLORS = [
  "#0891b2",
  "#2563eb",
  "#0f9f6e",
  "#b7791f",
  "#c2417f",
  "#dc2626",
  "#4f46e5",
  "#047857",
  "#ca8a04",
  "#be185d",
  "#0d9488",
  "#9333ea",
  "#ea580c",
  "#16a34a",
  "#0284c7",
  "#9f1239",
  "#52525b"
] as const;

function aggregateGpuProcessUsers(
  gpu: HostResourceSnapshot["gpus"][number],
  userColorByUser: MonitorGpuUserColorMap
): MonitorGpuUserUsage[] {
  const users = new Map<string, Omit<MonitorGpuUserUsage, "color">>();
  for (const process of gpu.processes ?? []) {
    const user = normalizeMonitorGpuUser(process.user);
    const current = users.get(user) ?? {
      user,
      processCount: 0,
      elapsedSeconds: null,
      usedMemoryBytes: 0
    };
    current.processCount += 1;
    current.usedMemoryBytes += process.usedMemoryBytes ?? 0;
    if (typeof process.elapsedSeconds === "number") {
      current.elapsedSeconds = Math.max(current.elapsedSeconds ?? 0, process.elapsedSeconds);
    }
    users.set(user, current);
  }
  return Array.from(users.values())
    .sort((left, right) => right.usedMemoryBytes - left.usedMemoryBytes || left.user.localeCompare(right.user))
    .map((usage) => ({ ...usage, color: monitorGpuUserColor(usage.user, userColorByUser) }));
}

function buildMonitorGpuUserColorMap(snapshots: HostResourceSnapshot[]): MonitorGpuUserColorMap {
  const users = new Set<string>();
  for (const snapshot of snapshots) {
    for (const gpu of snapshot.gpus) {
      for (const process of gpu.processes ?? []) {
        users.add(normalizeMonitorGpuUser(process.user));
      }
    }
  }
  return assignMonitorGpuUserColors(Array.from(users));
}

function assignMonitorGpuUserColors(users: string[]): MonitorGpuUserColorMap {
  const colorsByUser = new Map<string, string>();
  const takenColorIndexes = new Set<number>();
  const sortedUsers = users
    .filter((user) => user !== MONITOR_UNKNOWN_GPU_USER)
    .sort((left, right) => left.localeCompare(right));

  // Build one visible-snapshot-wide map so different GPU cards reuse the same
  // color for a user while avoiding palette collisions whenever colors remain.
  for (const user of sortedUsers) {
    const preferredIndex = monitorGpuUserHash(user) % MONITOR_GPU_USER_COLORS.length;
    let color: string | null = null;
    for (let offset = 0; offset < MONITOR_GPU_USER_COLORS.length; offset += 1) {
      const candidateIndex = (preferredIndex + offset) % MONITOR_GPU_USER_COLORS.length;
      if (!takenColorIndexes.has(candidateIndex)) {
        color = MONITOR_GPU_USER_COLORS[candidateIndex];
        takenColorIndexes.add(candidateIndex);
        break;
      }
    }
    colorsByUser.set(user, color ?? monitorGeneratedGpuUserColor(user));
  }
  if (users.includes(MONITOR_UNKNOWN_GPU_USER)) {
    colorsByUser.set(MONITOR_UNKNOWN_GPU_USER, MONITOR_UNKNOWN_GPU_USER_COLOR);
  }
  return colorsByUser;
}

function monitorGpuUserColor(user: string, userColorByUser: MonitorGpuUserColorMap) {
  return userColorByUser.get(user) ?? (user === MONITOR_UNKNOWN_GPU_USER ? MONITOR_UNKNOWN_GPU_USER_COLOR : monitorGeneratedGpuUserColor(user));
}

function monitorGeneratedGpuUserColor(user: string) {
  const hue = monitorGpuUserHash(user) % 360;
  return `hsl(${hue} 64% 42%)`;
}

function monitorGpuUserHash(user: string) {
  let hash = 0;
  for (const char of user) hash = (hash * 31 + char.charCodeAt(0)) >>> 0;
  return hash;
}

function normalizeMonitorGpuUser(user: string | null | undefined) {
  return user?.trim() || MONITOR_UNKNOWN_GPU_USER;
}

function sameStringArray(left: string[], right: string[]) {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}

function measureMonitorCardRects(cardRefs: Map<string, HTMLElement>) {
  const rects = new Map<string, DOMRect>();
  cardRefs.forEach((element, alias) => {
    rects.set(alias, element.getBoundingClientRect());
  });
  return rects;
}

function previewMonitorHostOrder(
  order: string[],
  draggedAlias: string,
  pointerX: number,
  pointerY: number,
  cardRefs: Map<string, HTMLElement>
) {
  const baseOrder = order.filter((alias) => alias !== draggedAlias);
  const candidates = baseOrder
    .map((alias) => {
      const rect = cardRefs.get(alias)?.getBoundingClientRect();
      if (!rect) return null;
      const centerX = rect.left + rect.width / 2;
      const centerY = rect.top + rect.height / 2;
      return {
        alias,
        centerX,
        centerY,
        distance: Math.hypot(pointerX - centerX, pointerY - centerY)
      };
    })
    .filter((entry): entry is { alias: string; centerX: number; centerY: number; distance: number } => Boolean(entry))
    .sort((left, right) => left.distance - right.distance);
  const nearest = candidates[0];
  if (!nearest) return order;
  const verticalIntent = Math.abs(pointerY - nearest.centerY) >= Math.abs(pointerX - nearest.centerX);
  const insertAfter = verticalIntent ? pointerY > nearest.centerY : pointerX > nearest.centerX;
  const insertIndex = baseOrder.indexOf(nearest.alias) + (insertAfter ? 1 : 0);
  const nextOrder = [...baseOrder];
  nextOrder.splice(insertIndex, 0, draggedAlias);
  return nextOrder;
}

function orderMonitorHosts(hosts: Host[], hostOrder: string[]) {
  const byAlias = new Map(hosts.map((host) => [host.hostAlias.toLowerCase(), host]));
  const used = new Set<string>();
  const ordered: Host[] = [];
  for (const alias of hostOrder) {
    const key = alias.toLowerCase();
    const host = byAlias.get(key);
    if (host && !used.has(key)) {
      ordered.push(host);
      used.add(key);
    }
  }
  for (const host of hosts) {
    const key = host.hostAlias.toLowerCase();
    if (!used.has(key)) ordered.push(host);
  }
  return ordered;
}

function emptySshHostDraft(identityFile: string): SshHostDraft {
  return {
    alias: "",
    hostName: "",
    port: 22,
    user: "",
    identityFile
  };
}

function formatError(error: unknown) {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "Operation failed.";
}

function taskIdForError(error: unknown) {
  return parseApiError(error)?.taskId ?? undefined;
}

export default App;
