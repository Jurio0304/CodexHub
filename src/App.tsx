import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { CSSProperties, FormEvent, ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, fallbackHealth } from "./api";
import type {
  DeleteOperationResult,
  Health,
  Host,
  HostStatus,
  LatestCodexVersion,
  LocalCodexStatus,
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
import { applyAppSettings, fontPresets, loadLocalSettings } from "./settings";
import type { AppSettings, FontPreset, PlatformAppearance, ThemeChoice } from "./settings";

type SectionId = "dashboard" | "hosts" | "profiles" | "skills" | "tasks" | "settings";
type Locale = "en" | "zh";
type HostBusyAction = "test" | "bootstrap" | RemoteCodexAction;
type BadgeTone = "green" | "yellow" | "red" | "blue" | "gray";
type SetupGuideStep = "language" | "ssh";
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

const CODEX_MODEL_OPTIONS = ["gpt-5.5", "gpt-5.4", "gpt-5.4-mini", "gpt-5.3-codex", "gpt-5.2", "gpt-5-codex"];
const REASONING_EFFORT_OPTIONS = ["low", "medium", "high", "xhigh"];
const DEFAULT_PROFILE_MODEL = "gpt-5-codex";
const DEFAULT_PROFILE_PROVIDER = "openai";
const DEFAULT_PROFILE_BASE_URL = "https://api.openai.com/v1";
const DEFAULT_PROFILE_API_KEY_ENV_VAR = "OPENAI_API_KEY";
const appLogoUrl = new URL("../figs/app-logo.png", import.meta.url).href;

const uiCopy = {
  en: {
    navItems: [
      { id: "dashboard", label: "Home", icon: "🏠" },
      { id: "hosts", label: "Hosts", icon: "🖥️" },
      { id: "profiles", label: "Profiles", icon: "🧾" },
      { id: "skills", label: "Skills", icon: "🧩" },
      { id: "tasks", label: "Tasks", icon: "✅" },
      { id: "settings", label: "Settings", icon: "⚙️" }
    ] satisfies Array<{ id: SectionId; label: string; icon: string }>,
    sections: {
      dashboard: {
        title: "🏠 Home",
        eyebrow: "Home",
        body: "Mock SSH inventory, profile status, and recent operations for the first CodexHub desktop shell."
      },
      hosts: {
        title: "🖥️ Hosts",
        eyebrow: "Server inventory",
        body: "Add CodexHub-managed SSH config blocks without disturbing user-owned SSH settings."
      },
      profiles: {
        title: "🧾 Profiles",
        eyebrow: "Codex configuration",
        body: "Draft managed profile presets for remote ~/.codex/config.toml files."
      },
      skills: {
        title: "🧩 Skills",
        eyebrow: "Skill packs",
        body: "Review skill bundles that will sync to remote ~/.codex/skills/ directories."
      },
      tasks: {
        title: "✅ Tasks",
        eyebrow: "Task runs",
        body: "Track mock backend commands, generated logs, and pending host operations."
      },
      settings: {
        title: "⚙️ Settings",
        eyebrow: "Preferences",
        body: "Adjust the shell theme, inspect local SSH key status, and copy public keys."
      }
    } satisfies Record<SectionId, { title: string; eyebrow: string; body: string }>,
      common: {
      addServer: "Add Server",
      backendMode: "Backend mode",
      host: "Host",
      justNow: "just now",
      primaryNavigation: "Primary navigation",
      required: "required",
      notRequired: "not required",
      loading: "loading",
      ready: "ready",
      unassigned: "Unassigned"
    },
    notices: {
      default: "Local SSH key and config management is ready in the desktop backend.",
      addHost: "Fill in the SSH config form. CodexHub will create or update one managed Host block with a backup first.",
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
    setupGuide: {
      title: "🧭 Setup Guide",
      languageTitle: "Choose Language",
      languageBody: "Step 1: Please choose your preferred language.",
      languageEnglish: "English",
      languageChinese: "Simplified Chinese",
      next: "Next",
      bodyWithHosts: (count: number) => `${count} local SSH Host entr${count === 1 ? "y" : "ies"} detected. Importing refreshes CodexHub only.`,
      bodyEmpty: "No usable local SSH config was detected. You can add hosts manually in CodexHub.",
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
      details: "Host details",
      detailsTitle: (alias: string) => `🖥️ Host · ${alias}`,
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
      renderedToml: "Rendered TOML",
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
      actionLabels: {
        "Apply profile": "Apply profile",
        "Test SSH connection": "Test SSH connection",
        "Bootstrap SSH key": "Bootstrap SSH key",
        "Sync skill pack": "Sync skill pack",
        "Preview profile": "Preview profile",
        "Probe remote system": "Test remote system",
        "Check Codex version": "Check Codex version",
        "Install Codex": "Install Codex",
        "Update Codex": "Update Codex",
        "List remote skills": "List remote skills",
        "Preview skill install": "Preview skill install",
        "Install skill": "Install skill",
        "Delete SSH Host": "Delete SSH Host",
        "Delete profile": "Delete profile",
        "Delete skill": "Delete skill"
      }
    },
    settings: {
      appearance: "Appearance",
      theme: "Theme",
      platformAppearance: "Platform",
      font: "Font",
      runtime: "Runtime",
      backend: "Backend",
      app: "App",
      remoteWrapper: "Remote wrapper",
      sshConfig: "SSH config",
      desktopBackendRequired: "desktop backend required",
      localSsh: "Local keys",
      localCodexCli: "Local Codex CLI",
      localCodexDetected: "Detected",
      localCodexMissing: "Not detected",
      codexStatus: "Status",
      codexVersion: "Version",
      codexPath: "Path",
      codexSearchPaths: "Search order",
      codexInstallHint: "Install hint",
      sshKeyStatus: "SSH key status",
      sshKeyBody: "Private key files are checked by existence only. CodexHub never reads or displays private key content.",
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
      }
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
        failed: "failed"
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
      { id: "dashboard", label: "主页", icon: "🏠" },
      { id: "hosts", label: "主机", icon: "🖥️" },
      { id: "profiles", label: "配置", icon: "🧾" },
      { id: "skills", label: "技能", icon: "🧩" },
      { id: "tasks", label: "任务", icon: "✅" },
      { id: "settings", label: "设置", icon: "⚙️" }
    ] satisfies Array<{ id: SectionId; label: string; icon: string }>,
    sections: {
      dashboard: {
        title: "🏠 主页",
        eyebrow: "主页",
        body: "用于 CodexHub 桌面壳的 SSH 清单、配置状态和最近操作。"
      },
      hosts: {
        title: "🖥️ 主机",
        eyebrow: "服务器清单",
        body: "添加 CodexHub 管理的 SSH config 块，不影响用户已有 SSH 设置。"
      },
      profiles: {
        title: "🧾 配置",
        eyebrow: "Codex 配置",
        body: "为远端 ~/.codex/config.toml 草拟受管理的配置预设。"
      },
      skills: {
        title: "🧩 技能",
        eyebrow: "技能包",
        body: "查看未来会同步到远程 ~/.codex/skills/ 目录的技能包。"
      },
      tasks: {
        title: "✅ 任务",
        eyebrow: "任务运行",
        body: "跟踪后端命令、生成日志和待处理主机操作。"
      },
      settings: {
        title: "⚙️ 设置",
        eyebrow: "偏好设置",
        body: "调整界面主题，查看本地 SSH 密钥状态，并复制公钥。"
      }
    } satisfies Record<SectionId, { title: string; eyebrow: string; body: string }>,
      common: {
      addServer: "添加服务器",
      backendMode: "后端模式",
      host: "主机",
      justNow: "刚刚",
      primaryNavigation: "主导航",
      required: "需要",
      notRequired: "不需要",
      loading: "加载中",
      ready: "就绪",
      unassigned: "未分配"
    },
    notices: {
      default: "本地 SSH 密钥和配置管理已在桌面后端就绪。",
      addHost: "请填写 SSH config 表单。CodexHub 会先备份，再创建或更新一个受管理的 Host 块。",
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
    setupGuide: {
      title: "🧭 配置向导",
      languageTitle: "选择语言",
      languageBody: "第1步：请选择偏好语言",
      languageEnglish: "英文",
      languageChinese: "简体中文",
      next: "下一步",
      bodyWithHosts: (count: number) => `检测到 ${count} 个本地 SSH Host。导入只刷新 CodexHub，不改写 SSH config。`,
      bodyEmpty: "未检测到本地存在可用的SSH配置，可使用CodexHub手动添加",
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
      details: "主机详情",
      detailsTitle: (alias: string) => `🖥️ 主机 · ${alias}`,
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
      apiKeyLoading: "正在读取 key...",
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
      renderedToml: "渲染 TOML",
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
      actionLabels: {
        "Apply profile": "应用配置",
        "Test SSH connection": "测试 SSH 连接",
        "Bootstrap SSH key": "配置 SSH 密钥",
        "Sync skill pack": "同步技能包",
        "Preview profile": "预览配置",
        "Probe remote system": "测试远端系统",
        "Check Codex version": "检查 Codex 版本",
        "Install Codex": "安装 Codex",
        "Update Codex": "更新 Codex",
        "List remote skills": "列出远端 Skills",
        "Preview skill install": "预览 Skill 安装",
        "Install skill": "安装 Skill",
        "Delete SSH Host": "删除 SSH Host",
        "Delete profile": "删除配置",
        "Delete skill": "删除 Skill"
      }
    },
    settings: {
      appearance: "外观",
      theme: "主题",
      platformAppearance: "平台",
      font: "字体",
      runtime: "运行时",
      backend: "后端",
      app: "应用",
      remoteWrapper: "远程包装器",
      sshConfig: "SSH 配置",
      desktopBackendRequired: "需要桌面后端",
      localSsh: "本地密钥",
      localCodexCli: "本地 Codex CLI",
      localCodexDetected: "已检测到",
      localCodexMissing: "未检测到",
      codexStatus: "状态",
      codexVersion: "版本",
      codexPath: "路径",
      codexSearchPaths: "搜索顺序",
      codexInstallHint: "安装提示",
      sshKeyStatus: "SSH 密钥状态",
      sshKeyBody: "仅检查私钥文件是否存在。CodexHub 从不读取或显示私钥内容。",
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
      }
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
        failed: "失败"
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

function App() {
  const [activeSection, setActiveSection] = useState<SectionId>("dashboard");
  const [settings, setSettings] = useState<AppSettings>(() => loadLocalSettings());
  const [health, setHealth] = useState<Health>(fallbackHealth);
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
  const [sshStatus, setSshStatus] = useState<SshStatus | null>(null);
  const [sshConfigHosts, setSshConfigHosts] = useState<SshConfigHost[]>([]);
  const [latestCodexVersion, setLatestCodexVersion] = useState<LatestCodexVersion | null>(null);
  const [localCodexStatus, setLocalCodexStatus] = useState<LocalCodexStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [sshBusy, setSshBusy] = useState(false);
  const [localCodexBusy, setLocalCodexBusy] = useState(false);
  const [hostBusy, setHostBusy] = useState<Record<string, HostBusyAction>>({});
  const [hostModalOpen, setHostModalOpen] = useState(false);
  const [newProfileRequest, setNewProfileRequest] = useState(0);
  const [codexOperationModal, setCodexOperationModal] = useState<CodexOperationModalState | null>(null);
  const [setupGuideOpen, setSetupGuideOpen] = useState(false);
  const [setupGuideStep, setSetupGuideStep] = useState<SetupGuideStep>("language");
  const [setupGuideSshConfigHosts, setSetupGuideSshConfigHosts] = useState<SshConfigHost[]>([]);
  const [setupGuideBusy, setSetupGuideBusy] = useState(false);
  const [notice, setNotice] = useState<string>(uiCopy.en.notices.default);

  const locale: Locale = settings.fontPreset === "zh-cn" ? "zh" : "en";
  const copy = uiCopy[locale];

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
    try {
      const result = await importLocalSshConfig();
      setNotice(copy.hosts.detectedLocalHosts(result.detectedSshConfigHosts.length));
      return result;
    } catch (error) {
      setNotice(formatError(error));
      throw error;
    }
  };

  const refreshLatestCodex = async (force = false) => {
    const latest = await api.refreshLatestCodexVersion(force);
    setLatestCodexVersion(latest);
    return latest;
  };

  const refreshLocalCodexStatus = async () => {
    setLocalCodexBusy(true);
    try {
      const status = await api.getLocalCodexStatus();
      setLocalCodexStatus(status);
      return status;
    } finally {
      setLocalCodexBusy(false);
    }
  };

  useEffect(() => {
    let mounted = true;

    Promise.all([
      api.getSettings(),
      api.getHealth(),
      api.listHosts(),
      api.listProfiles(),
      api.listSkillPacks(),
      api.getSkillInventoryStatus(),
      api.listTasks(),
      api.getLocalCodexStatus()
    ])
      .then(([nextSettings, nextHealth, nextHosts, nextProfiles, nextSkillPacks, nextSkillInventoryStatus, nextTasks, nextLocalCodexStatus]) => {
        if (!mounted) return;
        setSettings(nextSettings);
        setHealth(nextHealth);
        setHosts(nextHosts);
        setProfiles(nextProfiles);
        setSkillPacks(nextSkillPacks);
        setSkillInventoryStatus(nextSkillInventoryStatus);
        setTasks(normalizeTaskRunsForUi(nextTasks));
        setLocalCodexStatus(nextLocalCodexStatus);
        setSetupGuideStep("language");
        setSetupGuideOpen(!nextSettings.setupGuideDismissed);
        if (nextSettings.setupGuideDismissed || nextHosts.length > 0) {
          void refreshSshState();
        }
      })
      .finally(() => {
        if (mounted) setLoading(false);
      });

    return () => {
      mounted = false;
    };
  }, []);

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
    setNotice(copy.notices.default);
  }, [copy.notices.default]);

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
    setNotice(copy.notices.addHost);
  };

  const handleConnectSshHost = async (
    draft: SshHostDraft,
    password: string,
    requestId: string,
    onProgress: (event: SshBootstrapProgressEvent) => void
  ): Promise<SshBootstrapResult> => {
    const hostAlias = draft.alias || draft.hostName;
    setHostBusy((current) => ({ ...current, [hostAlias]: "bootstrap" }));
    setHosts((current) => current.map((host) => (host.hostAlias === hostAlias ? { ...host, status: "testing" } : host)));

    try {
      const result = await api.connectSshHost(draft, password, requestId, onProgress);
      setTasks((current) => [normalizeTaskRunForUi(result.task), ...current]);
      setNotice(result.message);
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
  };

  const handleDeleteSshConfigHost = async (alias: string) => {
    const result = await api.deleteSshConfigHost(alias);
    await refreshSshState();
    setTasks((current) => [normalizeTaskRunForUi(result.task), ...current]);
    setNotice(result.backupPath ? `${result.message} Backup: ${result.backupPath}` : result.message);
    return result;
  };

  const handleGenerateEd25519Key = async () => {
    setSshBusy(true);
    try {
      const result = await api.generateEd25519Key();
      setSshStatus(result.status);
      setNotice(result.message);
    } catch (error) {
      setNotice(formatError(error));
      throw error;
    } finally {
      setSshBusy(false);
    }
  };

  const handleCopyPublicKey = async (publicKey: string) => {
    try {
      await navigator.clipboard.writeText(publicKey);
      setNotice(copy.notices.publicKeyCopied);
      return true;
    } catch {
      setNotice(copy.notices.copyFailed);
      return false;
    }
  };

  const handleTestHost = async (idOrAlias: string) => {
    const target = hosts.find((host) => host.id === idOrAlias || host.hostAlias === idOrAlias);
    const hostAlias = target?.hostAlias ?? idOrAlias;
    setHostBusy((current) => ({ ...current, [hostAlias]: "test" }));
    setHosts((current) => current.map((host) => (host.hostAlias === hostAlias ? { ...host, status: "testing" } : host)));

    const result = await api.remoteProbeCodex(hostAlias);
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
              codexInstalled: result.codexInstalled,
              codexVersion: result.codexVersion,
              configExists: result.configExists,
              apiConfigName: result.apiConfigName,
              apiConfigSource: result.apiConfigSource,
              skillsExists: result.skillsExists,
              skillsCount: result.skillsCount,
              latencyMs: result.latencyMs,
              lastSeen: result.sshStatus === "online" ? copy.common.justNow : host.lastSeen
            }
          : host
      )
    );
    const [refreshedHosts, refreshedProfiles] = await Promise.all([api.listHosts(), api.listProfiles()]);
    setHosts(refreshedHosts);
    setProfiles(refreshedProfiles);
    setTasks((current) => [normalizeTaskRunForUi(result.task), ...current]);
    setNotice(`${target?.name ?? hostAlias}: ${result.task.summary}`);
    setHostBusy((current) => {
      const next = { ...current };
      delete next[hostAlias];
      return next;
    });
  };

  const handleTestAllSshHosts = async () => {
    await Promise.all(sshConfigHosts.map((host) => handleTestHost(host.alias)));
    setNotice(copy.hosts.testedAll);
  };

  const handleBootstrapExistingHost = async (idOrAlias: string, password: string) => {
    const target = hosts.find((host) => host.id === idOrAlias || host.hostAlias === idOrAlias);
    const hostAlias = target?.hostAlias ?? idOrAlias;
    setHostBusy((current) => ({ ...current, [hostAlias]: "bootstrap" }));
    setHosts((current) => current.map((host) => (host.hostAlias === hostAlias ? { ...host, status: "testing" } : host)));

    try {
      const result = await api.bootstrapExistingSshHost(hostAlias, password);
      await refreshSshState();
      setHosts((current) =>
        current.map((host) =>
          host.hostAlias === result.hostAlias
            ? {
                ...host,
                status: result.ok ? "online" : "offline",
                latencyMs: result.latencyMs,
                lastSeen: result.ok ? copy.common.justNow : host.lastSeen
              }
            : host
        )
      );
      setTasks((current) => [normalizeTaskRunForUi(result.task), ...current]);
      setNotice(`${target?.name ?? hostAlias}: ${result.message}`);
      if (!result.ok) throw new Error(result.message);
      return result.message;
    } finally {
      setHostBusy((current) => {
        const next = { ...current };
        delete next[hostAlias];
        return next;
      });
    }
  };

  const applyRemoteCodexResult = (result: RemoteCodexMaintenanceResult, action: RemoteCodexAction) => {
    const resolvedVersion = result.afterVersion ?? result.beforeVersion;
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
              lastSeen: sshCheckFailed ? host.lastSeen : copy.common.justNow
            }
          : host
      )
    );
    setTasks((current) => [normalizeTaskRunForUi(result.task), ...current]);
  };

  const handleRemoteCodexAction = async (idOrAlias: string, action: RemoteCodexAction) => {
    const target = hosts.find((host) => host.id === idOrAlias || host.hostAlias === idOrAlias);
    const hostAlias = target?.hostAlias ?? idOrAlias;
    const hostName = target?.name ?? hostAlias;
    const showProgressModal = action === "install" || action === "update";
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
      setNotice(`${hostName}: ${result.message}`);
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
    } catch (error) {
      const errorMessage = formatError(error);
      setNotice(`${hostName}: ${errorMessage}`);
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
    } finally {
      setHostBusy((current) => {
        const next = { ...current };
        delete next[hostAlias];
        return next;
      });
    }
  };

  const handleUpdateOutdatedCodexHosts = async (aliases: string[]) => {
    const uniqueAliases = Array.from(new Set(aliases.filter(Boolean)));
    if (uniqueAliases.length === 0) {
      setNotice(copy.hosts.noOutdatedCodex);
      return;
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
      setNotice(copy.hosts.updatedOutdatedCodex(successCount, uniqueAliases.length));
      void refreshLatestCodex(true);
    } finally {
      setHostBusy((current) => {
        const next = { ...current };
        for (const alias of uniqueAliases) delete next[alias];
        return next;
      });
    }
  };

  const replaceProfile = (profile: Profile) => {
    setProfiles((current) => current.map((item) => (item.id === profile.id ? profile : item)));
  };

  const handleCreateProfile = async (draft: ProfileDraft) => {
    const profile = await api.createProfile(draft);
    setProfiles((current) => [...current, profile]);
    setNotice(`${profile.name}: ${copy.profiles.create}`);
    return profile;
  };

  const handleUpdateProfile = async (id: string, patch: ProfilePatch) => {
    const profile = await api.updateProfile(id, patch);
    replaceProfile(profile);
    setNotice(`${profile.name}: ${copy.profiles.save}`);
    return profile;
  };

  const handleDeleteProfile = async (id: string) => {
    const profile = profiles.find((item) => item.id === id);
    const result = await api.deleteProfile(id);
    setTasks((current) => [normalizeTaskRunForUi(result.task), ...current]);
    if (!result.deleted) {
      setNotice(result.message);
      return result;
    }
    setProfiles((current) => current.filter((item) => item.id !== id));
    setHosts((current) => current.map((host) => (host.profileId === id ? { ...host, profileId: null, apiConfigName: null, apiConfigSource: null } : host)));
    setNotice(profile ? `${profile.name}: ${copy.profiles.delete}` : copy.profiles.delete);
    return result;
  };

  const handleDuplicateProfile = async (id: string) => {
    const profile = await api.duplicateProfile(id);
    setProfiles((current) => [...current, profile]);
    setNotice(`${profile.name}: ${copy.profiles.duplicate}`);
    return profile;
  };

  const handleImportProfiles = async (bundle: ProfileImportExport) => {
    const result = await api.importProfiles(bundle);
    setProfiles((current) => [...current, ...result.profiles]);
    setNotice(copy.profiles.importReady(result.profiles.length));
    return result;
  };

  const refreshSkills = async () => {
    const [nextSkills, nextStatus] = await Promise.all([api.listSkillPacks(), api.getSkillInventoryStatus()]);
    setSkillPacks(nextSkills);
    setSkillInventoryStatus(nextStatus);
    return nextSkills;
  };

  const handleRefreshSkillLibrary = async () => {
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
  };

  const applySkillDetectionResult = async (result: SkillDetectionResult) => {
    setSkillPacks(result.skills);
    setSkillInventoryStatus(result.status);
    if (result.tasks.length > 0) setTasks((current) => [...normalizeTaskRunsForUi(result.tasks), ...current]);
    const nextHosts = await api.listHosts();
    setHosts(nextHosts);
    setNotice(result.message);
    return result;
  };

  const applySkillOperationResult = async (result: SkillTargetOperationResult) => {
    setSkillPacks(result.skills);
    if (result.tasks.length > 0) setTasks((current) => [...normalizeTaskRunsForUi(result.tasks), ...current]);
    const nextHosts = await api.listHosts();
    setHosts(nextHosts);
    const nextStatus = await api.getSkillInventoryStatus();
    setSkillInventoryStatus(nextStatus);
    setNotice(
      result.message === "install-success"
        ? copy.skills.installSuccess
        : result.message === "install-partial-failure"
          ? copy.skills.installPartialFailure
          : result.message === "uninstall-success"
            ? copy.skills.uninstallSuccess
            : result.message === "uninstall-partial-failure"
              ? copy.skills.uninstallPartialFailure
          : result.message || copy.skills.operationDone
    );
    return result;
  };

  const handleDetectInstalledSkills = async (includeHosts: boolean) => {
    const result = await api.detectInstalledSkills(includeHosts);
    return applySkillDetectionResult(result);
  };

  const handleImportSkillDirectory = async () => {
    const selected = await open({ directory: true, multiple: false, title: copy.skills.importDirectory });
    const path = Array.isArray(selected) ? selected[0] : selected;
    if (!path) return null;
    const result = await api.importLocalSkill(path);
    await refreshSkills();
    setNotice(result.message || copy.skills.imported(result.imported.length));
    return result;
  };

  const handleDownloadGithubSkill = async (repoUrl: string) => {
    const result = await api.downloadGithubSkill(repoUrl);
    await refreshSkills();
    setNotice(result.message || copy.skills.imported(result.imported.length));
    return result;
  };

  const handleGetSkillTargets = async (skillId: string) => {
    const result = await api.getSkillTargets(skillId);
    if (result.tasks.length > 0) setTasks((current) => [...normalizeTaskRunsForUi(result.tasks), ...current]);
    return result;
  };

  const handleInstallSkillTargets = async (skillId: string, targets: SkillTargetRequest[]) => {
    const result = await api.installSkillTargets(skillId, targets);
    return applySkillOperationResult(result);
  };

  const handleUninstallSkillTargets = async (skillId: string, targets: SkillTargetRequest[]) => {
    const result = await api.uninstallSkillTargets(skillId, targets);
    return applySkillOperationResult(result);
  };

  const handleDeleteLibrarySkill = async (skillId: string, uninstallFirst: boolean) => {
    const result = await api.deleteLibrarySkill(skillId, uninstallFirst);
    return applySkillOperationResult(result);
  };

  const handleUpdateLibrarySkillAbout = async (skillId: string, about: string) => {
    const nextSkills = await api.updateLibrarySkillAbout(skillId, about);
    setSkillPacks(nextSkills);
    const updated = nextSkills.find((skill) => skill.id === skillId);
    setNotice(updated ? `${updated.name}: ${copy.skills.save}` : copy.skills.save);
    return updated ?? null;
  };

  const handleSetProfileApiKey = async (profileId: string, apiKey: string) => {
    const profile = await api.setProfileApiKey(profileId, apiKey);
    replaceProfile(profile);
    setNotice(`${profile.name}: ${copy.profiles.credentialStored}`);
    return profile;
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

  const handlePreviewProfileApply = (profileId: string, hostIds: string[]) => api.previewProfileApply(profileId, hostIds);

  const handleApplyProfile = async (profileId: string, hostIds: string[]) => {
    const result = await api.applyProfile(profileId, hostIds);
    if (result.tasks.length > 0) setTasks((current) => [...normalizeTaskRunsForUi(result.tasks), ...current]);
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
  };

  const handleDetectCcSwitchProfiles = async () => {
    const detection = await api.detectCcSwitchProfiles();
    setNotice(
      detection.detected ? copy.profiles.ccSwitchFound(detection.importExport.profiles.length) : copy.profiles.ccSwitchNone
    );
    return detection;
  };

  const handleImportCcSwitchProfiles = async (detection: CcSwitchDetection) => {
    const result = await api.importCcSwitchProfiles(detection);
    const nextProfiles = await api.listProfiles();
    setProfiles(nextProfiles);
    setNotice(copy.profiles.importReady(result.profiles.length));
    return result;
  };

  const persistSettings = (nextSettings: AppSettings) => {
    setSettings(nextSettings);
    applyAppSettings(nextSettings);
    void api.saveSettings(nextSettings).then(setSettings);
  };

  const handleDismissSetupGuide = () => {
    setSetupGuideOpen(false);
    persistSettings({ ...settings, setupGuideDismissed: true });
  };

  const handleSetupGuideLanguageNext = async (fontPreset: FontPreset) => {
    persistSettings({ ...settings, fontPreset });
    setSetupGuideStep("ssh");
    setSetupGuideBusy(true);
    try {
      await waitForNextFrame();
      await refreshSshDetectionState();
    } finally {
      setSetupGuideBusy(false);
    }
  };

  const handleOpenSetupGuide = async () => {
    setSetupGuideBusy(true);
    try {
      setSetupGuideStep("ssh");
      setSetupGuideOpen(true);
      await waitForNextFrame();
      await refreshSshDetectionState();
    } finally {
      setSetupGuideBusy(false);
    }
  };

  const handleImportLocalSshConfig = async () => {
    setSetupGuideBusy(true);
    try {
      const refreshed = await importLocalSshConfig();
      setSetupGuideOpen(false);
      persistSettings({ ...settings, setupGuideDismissed: true });
      setNotice(copy.setupGuide.imported(refreshed.detectedSshConfigHosts.length));
    } finally {
      setSetupGuideBusy(false);
    }
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
            onTestAllSshHosts={handleTestAllSshHosts}
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
            onTestAllSshHosts={handleTestAllSshHosts}
            onTestHost={handleTestHost}
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
            onDownloadGithubSkill={handleDownloadGithubSkill}
            onGetSkillTargets={handleGetSkillTargets}
            onImportSkillDirectory={handleImportSkillDirectory}
            onInstallSkillTargets={handleInstallSkillTargets}
            onRefreshSkillLibrary={handleRefreshSkillLibrary}
            onUninstallSkillTargets={handleUninstallSkillTargets}
            onUpdateLibrarySkillAbout={handleUpdateLibrarySkillAbout}
          />
        );
      case "tasks":
        return <TasksView copy={copy} tasks={tasks} />;
      case "settings":
        return (
          <SettingsView
            copy={copy}
            localCodexBusy={localCodexBusy}
            localCodexStatus={localCodexStatus}
            settings={settings}
            sshBusy={sshBusy}
            sshStatus={sshStatus}
            onCopyPublicKey={handleCopyPublicKey}
            onFontPresetChange={(fontPreset) => persistSettings({ ...settings, fontPreset })}
            onGenerateEd25519Key={handleGenerateEd25519Key}
            onPlatformAppearanceChange={(platformAppearance) => persistSettings({ ...settings, platformAppearance })}
            onRefreshLocalCodex={() => refreshLocalCodexStatus()}
            onRefreshSsh={async () => {
              await refreshSshState();
            }}
            onThemeChange={(theme) => persistSettings({ ...settings, theme })}
          />
        );
      default:
        return null;
    }
  };

  return (
    <div className="appShell">
      <aside className="sidebar" aria-label={copy.common.primaryNavigation}>
        <div className="brandBlock">
          <img className="appIcon" src={appLogoUrl} alt="" aria-hidden="true" />
          <div>
            <div className="brandName">CodexHub</div>
          </div>
        </div>

        <nav className="navList">
          {copy.navItems.map((item) => (
            <button className="navItem" data-active={activeSection === item.id} key={item.id} onClick={() => setActiveSection(item.id)} type="button">
              <span className="navIcon" aria-hidden="true">{item.icon}</span>
              <span>{item.label}</span>
            </button>
          ))}
        </nav>

        <div className="sidebarFooter">
          <span className="statusDot" data-status={health.mode === "tauri" ? "online" : "unknown"} />
          <div>
            <strong>{copy.common.backendMode}</strong>
          </div>
        </div>
      </aside>

      <main className="contentShell">
        <header className="topBar">
          <div>
            <h1>{selectedCopy.title}</h1>
          </div>
          <div className="topActions">
            {activeSection === "hosts" ? (
              <button className="primaryButton" type="button" onClick={handleAddHost}>{copy.common.addServer}</button>
            ) : null}
            {activeSection === "profiles" ? (
              <button className="primaryButton" type="button" onClick={() => setNewProfileRequest((current) => current + 1)}>{copy.profiles.newApiConfig}</button>
            ) : null}
          </div>
        </header>

        {renderContent()}
      </main>
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
      {setupGuideOpen ? (
        <SetupGuideModal
          busy={setupGuideBusy}
          copy={copy}
          currentFontPreset={settings.fontPreset}
          step={setupGuideStep}
          sshConfigHosts={setupGuideSshConfigHosts}
          sshStatus={sshStatus}
          onClose={handleDismissSetupGuide}
          onImport={handleImportLocalSshConfig}
          onLanguageNext={handleSetupGuideLanguageNext}
          onSkip={handleDismissSetupGuide}
        />
      ) : null}
    </div>
  );
}

function SetupGuideModal({
  busy,
  copy,
  currentFontPreset,
  step,
  sshConfigHosts,
  sshStatus,
  onClose,
  onImport,
  onLanguageNext,
  onSkip
}: {
  busy: boolean;
  copy: UICopy;
  currentFontPreset: FontPreset;
  step: SetupGuideStep;
  sshConfigHosts: SshConfigHost[];
  sshStatus: SshStatus | null;
  onClose: () => void;
  onImport: () => Promise<void>;
  onLanguageNext: (fontPreset: FontPreset) => Promise<void>;
  onSkip: () => void;
}) {
  const [languageDraft, setLanguageDraft] = useState<FontPreset>(currentFontPreset);
  const visibleHosts = sshConfigHosts.slice(0, 8);
  const hiddenHostCount = Math.max(0, sshConfigHosts.length - visibleHosts.length);
  const hasLocalHosts = sshConfigHosts.length > 0;
  const detectionPending = step === "ssh" && busy && !hasLocalHosts;
  const configPath = sshStatus?.configPath ?? "%USERPROFILE%\\.ssh\\config";
  const languageCopy = uiCopy[languageDraft === "zh-cn" ? "zh" : "en"];

  useEffect(() => {
    setLanguageDraft(currentFontPreset);
  }, [currentFontPreset]);

  return (
    <div className="modalBackdrop" role="presentation">
      <section className="setupGuideModal" role="dialog" aria-modal="true" aria-labelledby="setup-guide-title">
        {step === "language" ? (
          <>
            <div className="setupGuideHero">
              <h2 id="setup-guide-title">{languageCopy.setupGuide.title}</h2>
              <p>{languageCopy.setupGuide.languageBody}</p>
            </div>

            <div className="setupGuideLanguage" role="radiogroup" aria-label={languageCopy.setupGuide.languageTitle}>
              <button
                className="setupGuideLanguageOption"
                data-selected={languageDraft === "english"}
                role="radio"
                aria-checked={languageDraft === "english"}
                type="button"
                onClick={() => setLanguageDraft("english")}
              >
                <strong>{languageCopy.setupGuide.languageEnglish}</strong>
                <span>English</span>
              </button>
              <button
                className="setupGuideLanguageOption"
                data-selected={languageDraft === "zh-cn"}
                role="radio"
                aria-checked={languageDraft === "zh-cn"}
                type="button"
                onClick={() => setLanguageDraft("zh-cn")}
              >
                <strong>{languageCopy.setupGuide.languageChinese}</strong>
                <span>简体中文</span>
              </button>
            </div>

            <div className="modalActions setupGuideActions" data-has-hosts="true">
              <button className="primaryButton" disabled={busy} type="button" onClick={() => void onLanguageNext(languageDraft)}>
                {languageCopy.setupGuide.next}
              </button>
            </div>
          </>
        ) : (
          <>
            <div className="setupGuideHero">
              <h2 id="setup-guide-title">{copy.setupGuide.title}</h2>
              <p>{detectionPending ? copy.setupGuide.detecting : hasLocalHosts ? copy.setupGuide.bodyWithHosts(sshConfigHosts.length) : copy.setupGuide.bodyEmpty}</p>
              <code>{copy.setupGuide.detectedPath(configPath)}</code>
            </div>

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

            <div className="modalActions setupGuideActions" data-has-hosts={hasLocalHosts}>
              {hasLocalHosts ? (
                <button className="secondaryButton" disabled={busy} type="button" onClick={onSkip}>
                  {copy.setupGuide.skip}
                </button>
              ) : null}
              {hasLocalHosts ? (
                <button className="primaryButton" disabled={busy} type="button" onClick={() => void onImport()}>
                  {busy ? copy.setupGuide.importing : copy.setupGuide.importLocalConfig}
                </button>
              ) : (
                <button className="primaryButton" disabled={busy} type="button" onClick={onClose}>
                  {copy.setupGuide.close}
                </button>
              )}
            </div>
          </>
        )}
      </section>
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
  const icon = {
    hosts: "🖥️",
    profiles: "🧾",
    skills: "🧩",
    tasks: "✅"
  }[variant];

  return (
    <div className="emptyState emptyListState" data-variant={variant}>
      <div className="emptyListIcon" aria-hidden="true">
        {icon}
      </div>
      <p>{message || copy.emptyLists.hosts}</p>
      {action ? <div className="emptyListActions">{action}</div> : null}
    </div>
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
      message: operation.action === "install" ? copy.codexOperation.installHint : copy.codexOperation.updateHint,
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
      <section className="codexOperationModal" role="dialog" aria-modal="true" aria-labelledby="codex-operation-modal-title">
        <button className="modalCloseButton" type="button" onClick={onClose} aria-label={operation.status === "running" ? copy.codexOperation.hide : copy.codexOperation.close}>
          ×
        </button>
        <div className="codexOperationHeader">
          <div>
            <h2 id="codex-operation-modal-title">🛠️ {actionLabel}</h2>
          </div>
          <Badge tone={statusTone}>{copy.codexOperation[operation.status]}</Badge>
        </div>

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
                    {compactProgressLogDetail(log) ? <code>{compactProgressLogDetail(log)}</code> : null}
                  </div>
                ))
              : operation.status === "running"
                ? runningLogs.map((log, index) => (
                  <div className="codexOperationLogRow" data-level={log.level} key={`${log.message}-${index}`}>
                    <strong>{copy.status.log[log.level]}</strong>
                    <span>{log.message}</span>
                    {log.detail ? <code>{log.detail}</code> : null}
                  </div>
                ))
                : taskLogs.length > 0
                  ? taskLogs.map((log) => (
                    <div className="codexOperationLogRow" data-level={log.level} key={log.id}>
                      <strong>{copy.status.log[log.level]}</strong>
                      <span>{log.message}</span>
                      {compactTaskLogDetail(log) ? <code>{compactTaskLogDetail(log)}</code> : null}
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

        <div className="modalActions codexOperationActions">
          {operation.task ? (
            <button className="secondaryButton" type="button" onClick={onViewTasks}>
              {copy.codexOperation.viewTasks}
            </button>
          ) : null}
          <button className="primaryButton" type="button" onClick={onClose}>
            {operation.status === "running" ? copy.codexOperation.hide : copy.codexOperation.close}
          </button>
        </div>
      </section>
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
  onTestAllSshHosts: () => Promise<void>;
}) {
  return (
    <div className="pageGrid">
      <section className="summaryStrip" aria-label={copy.dashboard.summaryLabel}>
        <MetricCard label={copy.navItems[1].label} value={String(hosts.length)} detailLabel={copy.dashboard.online} detailValue={String(onlineCount)} />
        <MetricCard label={copy.navItems[2].label} value={String(profiles.length)} detailLabel={copy.dashboard.applied} detailValue={String(appliedProfileCount)} />
        <MetricCard label={copy.navItems[3].label} value={String(skillPacks.length)} detailLabel={copy.dashboard.enabled} detailValue={String(skillPacks.filter((pack) => pack.enabled).length)} />
        <MetricCard label={copy.navItems[4].label} value={String(tasks.length)} detailLabel={copy.dashboard.success} detailValue={String(successfulTaskCount)} />
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
  onTestAllSshHosts: () => Promise<void>;
}) {
  const anyHostBusy = sshConfigHosts.some((host) => Boolean(hostBusy[host.alias]));
  const testingAll = sshConfigHosts.length > 0 && sshConfigHosts.every((host) => hostBusy[host.alias] === "test");
  const hostInventoryByAlias = new Map(inventoryStatus.hostInventories.map((inventory) => [inventory.hostAlias.toLowerCase(), inventory]));
  const skillCounts = hosts.map((host) => dashboardHostSkillCount(host, hostInventoryByAlias.get(host.hostAlias.toLowerCase())));

  return (
    <section className="panel spanWide">
      <div className="panelHeader matrixHeader">
        <h2>{copy.dashboard.serverMatrix}</h2>
        <button className="primaryButton" disabled={sshConfigHosts.length === 0 || anyHostBusy} type="button" onClick={() => void onTestAllSshHosts()}>
          {testingAll ? copy.hosts.testingAll : copy.hosts.refreshDetected}
        </button>
      </div>

      {hosts.length === 0 ? (
        <div className="emptyState matrixEmptyState">
          <div className="matrixEmptyIcon" aria-hidden="true">🖥️</div>
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
                  <h3>{host.name}</h3>
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
  onGenerateEd25519Key: () => Promise<void>;
  onManageCodex: (id: string, action: RemoteCodexAction) => void;
  onOpenAddHost: () => void;
  onOpenSetupGuide: () => Promise<void>;
  onTestAllSshHosts: () => Promise<void>;
  onTestHost: (id: string) => void;
  onUpdateOutdatedCodexHosts: (aliases: string[]) => Promise<void>;
}) {
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
            <h2>{copy.hosts.detectedSshHosts}</h2>
          </div>
          <div className="topActions">
            <button className="secondaryButton" disabled={detectHostsBusy || anyHostBusy} type="button" onClick={() => void handleDetectLocalHosts().catch(() => undefined)}>
              {detectHostsBusy ? copy.setupGuide.detecting : copy.hosts.detect}
            </button>
            <button className="secondaryButton" disabled={sshConfigHosts.length === 0 || anyHostBusy} type="button" onClick={() => void onTestAllSshHosts()}>
              {testingAll ? copy.hosts.testingAll : copy.hosts.refreshDetected}
            </button>
            <button className="primaryButton" disabled={outdatedCodexAliases.length === 0 || anyHostBusy} type="button" onClick={() => void onUpdateOutdatedCodexHosts(outdatedCodexAliases)}>
              {updatingOutdated ? copy.hosts.updatingOutdatedCodex : copy.hosts.updateOutdatedCodex}
            </button>
          </div>
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
                        <div className="tableActions sshHostsActionGroup">
                          <button className="miniButton" disabled={Boolean(busy)} type="button" onClick={(event) => { event.stopPropagation(); onTestHost(sshHost.alias); }}>
                            {busy === "test" ? copy.hosts.testing : copy.hosts.test}
                          </button>
                          <button className="miniButton" disabled={Boolean(busy)} type="button" onClick={(event) => { event.stopPropagation(); handleEdit(sshHost); }}>{copy.hosts.edit}</button>
                          <button className="miniButton danger" disabled={Boolean(busy)} type="button" onClick={(event) => { event.stopPropagation(); setDeleteHostAlias(sshHost.alias); }}>{copy.hosts.delete}</button>
                        </div>
                      </td>
                      <td className="sshHostsCodexCol">
                        <div className="tableActions sshHostsActionGroup">
                          <button className="miniButton" disabled={installDisabled} type="button" onClick={(event) => { event.stopPropagation(); onManageCodex(sshHost.alias, "install"); }}>
                            {remoteCodexButtonLabel(copy, busy, "install")}
                          </button>
                          <button className="miniButton" disabled={updateDisabled} type="button" onClick={(event) => { event.stopPropagation(); onManageCodex(sshHost.alias, "update"); }}>
                            {remoteCodexButtonLabel(copy, busy, "update")}
                          </button>
                        </div>
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
          onDelete={() => void handleDelete().catch(() => undefined)}
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
  onGenerateEd25519Key: () => Promise<void>;
}) {
  const [draft, setDraft] = useState<SshHostDraft>(() => initialDraft ?? emptySshHostDraft(defaultIdentityFile));
  const [password, setPassword] = useState("");
  const [passwordVisible, setPasswordVisible] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [message, setMessage] = useState<string>(copy.hosts.formIntro);
  const [showProgress, setShowProgress] = useState(false);
  const [steps, setSteps] = useState(createInitialBootstrapSteps());
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
    setMessage(copy.hosts.formIntro);
    setShowProgress(false);
    setSteps(createInitialBootstrapSteps());
  }, [copy.hosts.formIntro, defaultIdentityFile, initialDraft, open]);

  useEffect(() => {
    setDraft((current) => ({ ...current, identityFile: current.identityFile || defaultIdentityFile }));
  }, [defaultIdentityFile]);

  if (!open) return null;

  const updateDraft = (key: keyof SshHostDraft, value: string | number) => {
    setDraft((current) => ({ ...current, [key]: value }));
  };

  const closeModal = () => {
    if (connecting && !window.confirm("连接仍在进行，确定要关闭窗口吗？")) return;
    onClose();
  };

  const handleGenerateKey = async () => {
    try {
      await onGenerateEd25519Key();
      setMessage("已新建本地 .ssh\\id_ed25519。");
    } catch (error) {
      setMessage(formatError(error));
    }
  };

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!hasIdentityFile) {
      setMessage("未检测到本地 id_ed25519，请先点击新建。");
      return;
    }
    const requestId = `bootstrap-${Date.now()}-${draft.alias || draft.hostName}`;
    setConnecting(true);
    setShowProgress(true);
    setSteps(createInitialBootstrapSteps());
    setMessage("正在连接，请等待四个步骤完成...");
    try {
      const result = await onConnect({ ...draft, identityFile: defaultIdentityFile }, password, requestId, (progress) => {
        setSteps((current) => updateBootstrapStep(current, progress));
      });
      if (!result.ok) {
        const detail = result.message || "连接失败";
        setMessage(detail);
        setSteps((current) => markBootstrapFailureIfNeeded(current, detail));
        return;
      }
      setMessage("成功连接");
    } catch (error) {
      const detail = formatError(error);
      setMessage(detail);
      setSteps((current) => markBootstrapFailureIfNeeded(current, detail));
    } finally {
      setConnecting(false);
    }
  };
  const editing = Boolean(initialDraft);

  return (
    <div className="modalBackdrop" role="presentation">
      <div className="sshHostModal" role="dialog" aria-modal="true" aria-labelledby="ssh-host-modal-title">
        <button className="modalCloseButton" type="button" onClick={closeModal} aria-label="Close">×</button>
        <div className="modalHero">
          <h2 id="ssh-host-modal-title">🖥️ {editing ? copy.hosts.edit : copy.hosts.addCodexHubHost}</h2>
        </div>

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
                aria-label={passwordVisible ? copy.profiles.hideApiKey : copy.profiles.showApiKey}
                aria-pressed={passwordVisible}
                className="credentialVisibilityButton"
                title={passwordVisible ? copy.profiles.hideApiKey : copy.profiles.showApiKey}
                type="button"
                onClick={() => setPasswordVisible((current) => !current)}
              >
                <CredentialVisibilityIcon visible={passwordVisible} />
              </button>
            </div>
          </label>
          <div className="fieldGroup identityRow" data-has-action={!hasIdentityFile}>
            <span>IDFile</span>
            <input readOnly value={hasIdentityFile ? "id_ed25519 detected" : "id_ed25519 not detected"} />
            {!hasIdentityFile ? (
              <button className="secondaryButton" disabled={!canGenerateKey || sshBusy || connecting} type="button" onClick={() => void handleGenerateKey()}>
                {sshBusy ? copy.settings.generating : "新建"}
              </button>
            ) : null}
          </div>

          <div className="modalActions">
            <button className="primaryButton" disabled={!canConnect} type="submit">{connecting ? "连接中..." : "连接"}</button>
          </div>
        </form>

        {showProgress ? <BootstrapProgressLog steps={steps} /> : null}
      </div>
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

function createInitialBootstrapSteps(): BootstrapStepState[] {
  return [
    { step: "password_login", label: "1. 密码登录远端", status: "pending", message: "等待开始", detail: "", stdout: "", stderr: "" },
    { step: "install_public_key", label: "2. 安装本地公钥", status: "pending", message: "等待开始", detail: "", stdout: "", stderr: "" },
    { step: "set_permissions", label: "3. 设置远端权限", status: "pending", message: "等待开始", detail: "", stdout: "", stderr: "" },
    { step: "verify_alias_login", label: "4. ssh Host 别名测试", status: "pending", message: "等待开始", detail: "", stdout: "", stderr: "" }
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

function markBootstrapFailureIfNeeded(steps: BootstrapStepState[], detail: string): BootstrapStepState[] {
  if (steps.some((step) => step.status === "failed")) return steps;
  const firstRunning = steps.find((step) => step.status === "running")?.step ?? "password_login";
  return steps.map((step) => (step.step === firstRunning ? { ...step, status: "failed", message: "连接失败", detail } : step));
}

function BootstrapProgressLog({ steps }: { steps: BootstrapStepState[] }) {
  return (
    <section className="bootstrapLogCard">
      <div className="bootstrapLogHeader">
        <strong>连接进程</strong>
        <span>实时引导日志</span>
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
                <strong>详细失败日志</strong>
                <pre>{step.stderr || step.detail || step.stdout || "未返回详细日志"}</pre>
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
          <h2>{copy.hosts.detailsTitle(host?.hostAlias ?? copy.hosts.unknown)}</h2>
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
  const importInputRef = useRef<HTMLInputElement | null>(null);
  const [selectedProfileId, setSelectedProfileId] = useState<string | null>(profiles[0]?.id ?? null);
  const selectedProfile = profiles.find((profile) => profile.id === selectedProfileId) ?? profiles[0] ?? null;
  const [selectedHostIds, setSelectedHostIds] = useState<string[]>(() => selectedProfile?.hostIds ?? []);
  const [preview, setPreview] = useState<ProfileApplyPreview | null>(null);
  const [applyResult, setApplyResult] = useState<ProfileApplyBatchResult | null>(null);
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
  const selectedAppliedHostIds = useMemo(() => {
    if (!selectedProfile) return [];
    return hosts
      .filter((host) => profileMatchesConfirmedHostApiConfig(selectedProfile, host))
      .map((host) => host.id);
  }, [hosts, selectedProfile]);

  useEffect(() => {
    if (!selectedProfileId && profiles[0]) setSelectedProfileId(profiles[0].id);
    if (selectedProfileId && !profiles.some((profile) => profile.id === selectedProfileId)) {
      setSelectedProfileId(profiles[0]?.id ?? null);
    }
  }, [profiles, selectedProfileId]);

  useEffect(() => {
    setSelectedHostIds(selectedAppliedHostIds);
    setPreview(null);
    setApplyResult(null);
  }, [selectedProfile?.id, selectedAppliedHostIds]);

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

  const clearApplyState = () => {
    setPreview(null);
    setApplyResult(null);
  };

  const handleSelectAllHosts = () => {
    setSelectedHostIds(hosts.map((host) => host.id));
    clearApplyState();
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

  const runProfileApply = async (profile: Profile, hostIds: string[], syncSelection = false) => {
    if (hostIds.length === 0) return null;
    if (syncSelection) {
      setSelectedProfileId(profile.id);
      setSelectedHostIds(hostIds);
    }
    const result = await runBusy("apply", () => onRunProfileApply(profile.id, hostIds));
    if (syncSelection || selectedProfile?.id === profile.id) {
      setApplyResult(result);
      setPreview((current) => (current && current.profileId === profile.id ? { ...current, hostResults: result.results } : current));
    }
    return result;
  };

  const handleApply = async (hostIds = selectedHostIds) => {
    if (!selectedProfile || hostIds.length === 0) return null;
    return runProfileApply(selectedProfile, hostIds, true);
  };

  const toggleHost = (hostId: string) => {
    setSelectedHostIds((current) => (current.includes(hostId) ? current.filter((id) => id !== hostId) : [...current, hostId]));
    clearApplyState();
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
            <h2>{copy.profiles.library}</h2>
          </div>
          <div className="topActions profileLibraryActions">
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
          </div>
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
                      <div className="profileRowActions" onClick={(event) => event.stopPropagation()}>
                        <button className="miniButton" type="button" onClick={() => {
                          setEditingProfileId(profile.id);
                          setProfileEditorOpen(true);
                        }}>{copy.profiles.edit}</button>
                        <button className="miniButton" disabled={busy === "duplicate"} type="button" onClick={() => void handleDuplicate(profile)}>{copy.profiles.duplicate}</button>
                        <button className="miniButton danger" disabled={busy === "delete"} type="button" onClick={() => setDeleteProfileId(profile.id)}>{copy.profiles.delete}</button>
                      </div>
                    </td>
                    <td>
                      <button
                        className="miniButton"
                        disabled={hosts.length === 0 || busy === "apply"}
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
            <h2>{copy.profiles.applyConfig}</h2>
          </div>
          <div className="topActions">
            <button className="secondaryButton" disabled={hosts.length === 0} type="button" onClick={handleSelectAllHosts}>{copy.profiles.selectAll}</button>
            <button className="primaryButton" disabled={!selectedProfile || selectedHostIds.length === 0 || busy === "apply"} type="button" onClick={() => void handleApply()}>{copy.profiles.applySelected}</button>
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
                  const selected = selectedHostIds.includes(host.id);
                  const codexStatus = hostCodexStatus(copy, host, undefined, hosts, latestCodexVersion);
                  return (
                    <tr
                      className="selectableRow"
                      data-selected={selected}
                      key={host.id}
                      onClick={() => toggleHost(host.id)}
                    >
                      <td className="sshHostsAliasCol">
                        <label className="profileHostSelectCell" onClick={(event) => event.stopPropagation()}>
                          <input checked={selected} type="checkbox" onChange={() => toggleHost(host.id)} />
                          <strong>{host.hostAlias}</strong>
                        </label>
                      </td>
                      <td className="sshHostsSourceCol"><Badge tone={host.source === "managed" ? "blue" : "gray"}>{hostSourceLabel(copy, host)}</Badge></td>
                      <td className="sshHostsAddressCol">{host.address}</td>
                      <td className="sshHostsVersionCol"><Badge tone={codexStatus.tone}>{codexStatus.label}</Badge></td>
                      <td className="profileApplyConfigCol"><HostApiConfigBadge copy={copy} host={host} profileById={profileById} /></td>
                      <td className="sshHostsActionsCol">
                        <div className="tableActions sshHostsActionGroup">
                          <button className="miniButton" disabled={!selectedProfile || busy === "preview"} type="button" onClick={(event) => { event.stopPropagation(); void handlePreview([host.id]); }}>
                            {copy.profiles.previewApply}
                          </button>
                          <button className="miniButton" disabled={!selectedProfile || busy === "apply"} type="button" onClick={(event) => { event.stopPropagation(); void handleApply([host.id]); }}>
                            {copy.profiles.applyOne}
                          </button>
                        </div>
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
        busy={busy}
        copy={copy}
        hosts={hosts}
        open={Boolean(hostPickerProfile)}
        profile={hostPickerProfile}
        profileById={profileById}
        onApply={(profile, hostIds) => runProfileApply(profile, hostIds, true)}
        onClose={() => setHostPickerProfileId(null)}
      />

      <ProfileApplyPreviewModal
        busy={busy}
        copy={copy}
        open={previewModalOpen}
        preview={previewWithResults}
        selectedCount={selectedHostIds.length}
        onApplySelected={() => void handleApply()}
        onClose={() => setPreviewModalOpen(false)}
      />
      {deleteProfile ? (
        <SimpleDeleteConfirmModal
          busy={busy === "delete"}
          body={copy.profiles.deleteConfirm(deleteProfile.name)}
          copy={copy}
          title={`${copy.profiles.delete}: ${deleteProfile.name}`}
          onClose={() => setDeleteProfileId(null)}
          onDelete={() => void handleDelete(deleteProfile).catch(() => undefined)}
        />
      ) : null}
    </div>
  );
}

function ProfileHostSelectModal({
  busy,
  copy,
  hosts,
  open,
  profile,
  profileById,
  onApply,
  onClose
}: {
  busy: string | null;
  copy: UICopy;
  hosts: Host[];
  open: boolean;
  profile: Profile | null;
  profileById: Map<string, Profile>;
  onApply: (profile: Profile, hostIds: string[]) => Promise<ProfileApplyBatchResult | null>;
  onClose: () => void;
}) {
  const [selectedHostIds, setSelectedHostIds] = useState<string[]>([]);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    if (!open || !profile) return;
    setSelectedHostIds(
      hosts
        .filter((host) => profileMatchesConfirmedHostApiConfig(profile, host))
        .map((host) => host.id)
    );
    setMessage(null);
  }, [open, profile, hosts]);

  if (!open || !profile) return null;

  const toggleHost = (hostId: string) => {
    setSelectedHostIds((current) => (current.includes(hostId) ? current.filter((id) => id !== hostId) : [...current, hostId]));
    setMessage(null);
  };

  const handleApply = async () => {
    if (selectedHostIds.length === 0 || busy === "apply") return;
    setMessage(null);
    try {
      const result = await onApply(profile, selectedHostIds);
      if (!result) return;
      if (!result.ok) {
        setMessage(result.results.find((row) => row.status === "failed")?.message ?? copy.profiles.operationFailed);
        return;
      }
      setMessage(copy.profiles.applySuccess(profile.name, selectedHostIds.length));
      window.setTimeout(() => onClose(), 650);
    } catch (error) {
      setMessage(formatError(error));
    }
  };

  return (
    <div className="modalBackdrop" role="presentation">
      <div className="sshHostModal profileHostSelectModal ProfileHostSelectModal" role="dialog" aria-modal="true" aria-labelledby="profile-host-select-title">
        <button className="modalCloseButton" disabled={busy === "apply"} type="button" onClick={onClose} aria-label="Close">&times;</button>
        <div className="modalHero">
          <h2 id="profile-host-select-title">🖥️ {copy.profiles.selectHosts}</h2>
        </div>

        <div className="profileHostSelectList">
          {hosts.map((host) => {
            const selected = selectedHostIds.includes(host.id);
            return (
              <label className="profileHostSelectRow" data-selected={selected} key={host.id}>
                <input checked={selected} disabled={busy === "apply"} type="checkbox" onChange={() => toggleHost(host.id)} />
                <strong>{host.name}</strong>
                <HostApiConfigBadge copy={copy} host={host} profileById={profileById} />
              </label>
            );
          })}
        </div>

        {message ? <p className="profileHostSelectMessage" role="status">{message}</p> : null}

        <div className="modalActions profileHostSelectActions">
          <button className="secondaryButton" disabled={hosts.length === 0 || busy === "apply"} type="button" onClick={() => {
            setSelectedHostIds(hosts.map((host) => host.id));
            setMessage(null);
          }}>
            {copy.profiles.selectAll}
          </button>
          <button className="primaryButton" disabled={selectedHostIds.length === 0 || busy === "apply"} type="button" onClick={() => void handleApply()}>
            {copy.profiles.applySelected}
          </button>
        </div>
      </div>
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

  useEffect(() => {
    if (!open || !profile || !canLoadStoredCredential) return;
    let cancelled = false;
    setCredentialLoading(true);
    setCredentialError(null);
    onGetCredential(profile.id)
      .then((result) => {
        if (cancelled) return;
        if (result.apiKey) {
          setCredentialInput(result.apiKey);
          setCredentialLoaded(true);
        } else {
          setCredentialError(copy.profiles.apiKeyMissing);
        }
      })
      .catch((error) => {
        if (!cancelled) setCredentialError(formatError(error));
      })
      .finally(() => {
        if (!cancelled) setCredentialLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [canLoadStoredCredential, copy.profiles.apiKeyMissing, onGetCredential, open, profile?.id]);

  if (!open) return null;

  const updateDraft = <K extends keyof ProfileDraft>(key: K, value: ProfileDraft[K]) => {
    setDraft((current) => ({ ...current, [key]: value }));
  };

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const saved = await onSave(profile, profile ? draft : profileDraftWithCreateDefaults(draft));
    if (credentialInput.trim()) {
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
      <div className="sshHostModal profileEditModal ProfileEditModal" role="dialog" aria-modal="true" aria-labelledby="profile-edit-modal-title">
        <button className="modalCloseButton" type="button" onClick={onClose} aria-label="Close">×</button>
        <div className="modalHero">
          <h2 id="profile-edit-modal-title">🧾 {profile ? copy.profiles.editor : copy.profiles.newProfile}</h2>
        </div>

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

          <div className="modalActions">
            <button className="primaryButton" disabled={!draft.name.trim() || busy === "save" || busy === "create"} type="submit">
              {busy === "save" || busy === "create" ? copy.profiles.saving : profile ? copy.profiles.save : copy.profiles.create}
            </button>
          </div>
        </form>
      </div>
    </div>
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
      <div className="sshHostModal profileApplyPreviewModal ProfileApplyPreviewModal" role="dialog" aria-modal="true" aria-labelledby="profile-apply-preview-modal-title">
        <button className="modalCloseButton" type="button" onClick={onClose} aria-label="Close">×</button>
        <div className="modalHero">
          <h2 id="profile-apply-preview-modal-title">👁️ {copy.profiles.preview}</h2>
        </div>

        <div className="profilePreviewGrid">
          <div>
            <div className="profileSubhead">
              <strong>{copy.profiles.targetFiles}</strong>
              <span>{preview?.targetFiles.length ?? 0}</span>
            </div>
            <div className="profileTargetList">
              {preview?.targetFiles.map((target) => (
                <div key={target.hostId}>
                  <strong>{target.hostName}</strong>
                  <code>{target.path}</code>
                  <span>{target.noChangeExpected ? copy.profiles.noChangeExpected : target.backupExpected ? copy.profiles.backupExpected : "-"}</span>
                </div>
              )) ?? <p className="mutedText">{copy.profiles.noPreview}</p>}
            </div>
          </div>
          <div>
            <div className="profileSubhead">
              <strong>{copy.profiles.perHostStatus}</strong>
              <span>{preview?.hostResults.length ?? 0}</span>
            </div>
            <div className="profileStatusList">
              {preview?.hostResults.map((row) => (
                <div key={row.hostId}>
                  <Badge tone={profileApplyTone(row.status)}>{row.status}</Badge>
                  <strong>{row.hostName}</strong>
                  <span>{row.message}</span>
                </div>
              )) ?? null}
            </div>
          </div>
        </div>

        <div className="configPreviewBox">
          <div className="profileSubhead">
            <strong>{copy.profiles.renderedToml}</strong>
          </div>
          <pre>{preview?.renderedToml ?? copy.profiles.noPreview}</pre>
        </div>

        <div className="modalActions">
          <button className="primaryButton" disabled={!preview || selectedCount === 0 || busy === "apply"} type="button" onClick={onApplySelected}>
            {copy.profiles.applySelected}
          </button>
        </div>
      </div>
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

function profileApplyTone(status: ProfileApplyBatchResult["results"][number]["status"]): "green" | "yellow" | "red" | "blue" | "gray" {
  if (status === "success") return "green";
  if (status === "failed") return "red";
  if (status === "no-change") return "blue";
  return "gray";
}

function SkillsView({
  copy,
  hosts,
  inventoryStatus,
  skillPacks,
  onDeleteLibrarySkill,
  onDetectInstalledSkills,
  onDownloadGithubSkill,
  onGetSkillTargets,
  onImportSkillDirectory,
  onInstallSkillTargets,
  onRefreshSkillLibrary,
  onUninstallSkillTargets,
  onUpdateLibrarySkillAbout
}: {
  copy: UICopy;
  hosts: Host[];
  inventoryStatus: SkillInventoryStatus;
  skillPacks: SkillPack[];
  onDeleteLibrarySkill: (skillId: string, uninstallFirst: boolean) => Promise<SkillTargetOperationResult>;
  onDetectInstalledSkills: (includeHosts: boolean) => Promise<SkillDetectionResult>;
  onDownloadGithubSkill: (repoUrl: string) => Promise<SkillImportResult>;
  onGetSkillTargets: (skillId: string) => Promise<SkillTargetsResult>;
  onImportSkillDirectory: () => Promise<SkillImportResult | null>;
  onInstallSkillTargets: (skillId: string, targets: SkillTargetRequest[]) => Promise<SkillTargetOperationResult>;
  onRefreshSkillLibrary: () => Promise<unknown>;
  onUninstallSkillTargets: (skillId: string, targets: SkillTargetRequest[]) => Promise<SkillTargetOperationResult>;
  onUpdateLibrarySkillAbout: (skillId: string, about: string) => Promise<SkillPack | null>;
}) {
  const [downloadOpen, setDownloadOpen] = useState(false);
  const [firstScanOpen, setFirstScanOpen] = useState(false);
  const [previewSkill, setPreviewSkill] = useState<SkillPack | null>(null);
  const [targetMode, setTargetMode] = useState<"install" | "uninstall" | null>(null);
  const [targetSkill, setTargetSkill] = useState<SkillPack | null>(null);
  const [targetResult, setTargetResult] = useState<SkillTargetsResult | null>(null);
  const [selectedTargetKeys, setSelectedTargetKeys] = useState<string[]>([]);
  const [deleteSkill, setDeleteSkill] = useState<SkillPack | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const installedSkillRows = useMemo(
    () => buildInstalledSkillRows(copy, hosts, inventoryStatus),
    [copy, hosts, inventoryStatus]
  );
  const installedSkillNames = useMemo(
    () => Array.from(new Set(installedSkillRows.flatMap((row) => row.skills))).sort((left, right) => left.localeCompare(right)),
    [installedSkillRows]
  );

  const runBusy = async <T,>(key: string, action: () => Promise<T>) => {
    setBusy(key);
    setMessage(null);
    try {
      const result = await action();
      return result;
    } catch (error) {
      const detail = formatError(error);
      setMessage(detail);
      throw error;
    } finally {
      setBusy(null);
    }
  };

  const handleDetect = async (includeHosts: boolean) => {
    await runBusy("detect", () => onDetectInstalledSkills(includeHosts));
    setMessage(copy.skills.detected);
  };

  const handleDetectClick = () => {
    void handleDetect(true).catch(() => undefined);
  };

  const handleImport = async () => {
    const result = await runBusy("import", onImportSkillDirectory);
    if (result) setMessage(result.message);
  };

  const handleRefresh = async () => {
    await runBusy("refresh", onRefreshSkillLibrary);
    setMessage(copy.skills.refreshed);
  };

  const handleDownload = async (repoUrl: string) => {
    await runBusy("download", () => onDownloadGithubSkill(repoUrl));
    setDownloadOpen(false);
    setMessage(copy.skills.downloaded);
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
    } catch {
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
    setMessage(
      result.message === "install-success"
        ? copy.skills.installSuccess
        : result.message === "install-partial-failure"
          ? copy.skills.installPartialFailure
          : result.message === "uninstall-success"
            ? copy.skills.uninstallSuccess
            : result.message === "uninstall-partial-failure"
              ? copy.skills.uninstallPartialFailure
          : result.message
    );
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
    setMessage(result.message);
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
          <h2>{copy.skills.library}</h2>
          <div className="skillLibraryActions">
            <button className="secondaryButton" disabled={busy === "detect"} type="button" onClick={handleDetectClick}>
              {busy === "detect" ? copy.skills.detecting : copy.skills.detect}
            </button>
            <button className="secondaryButton" disabled={busy === "refresh"} type="button" onClick={() => void handleRefresh().catch(() => undefined)}>
              {busy === "refresh" ? copy.skills.refreshing : copy.skills.refresh}
            </button>
            <button className="secondaryButton" disabled={busy === "import"} type="button" onClick={() => void handleImport().catch(() => undefined)}>
              {copy.skills.importDirectory}
            </button>
            <button className="primaryButton" disabled={busy === "download"} type="button" onClick={() => setDownloadOpen(true)}>
              {copy.skills.download}
            </button>
          </div>
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
                      <div className="skillRowActions">
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
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {message ? <p className="skillMessage">{message}</p> : null}
      </section>

      <section className="panel spanWide">
        <div className="panelHeader compact">
          <h2>{copy.skills.installedLibrary}</h2>
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
                        row.skills.map((skillName) => (
                          <span className="installedSkillTag" key={skillName} style={installedSkillTagStyle(skillName, installedSkillNames)}>
                            {skillName}
                          </span>
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
          onConfirm={() => void handleDetect(true).then(() => setFirstScanOpen(false)).catch(() => undefined)}
        />
      ) : null}
      {downloadOpen ? (
        <SkillDownloadModal
          busy={busy === "download"}
          copy={copy}
          onClose={() => setDownloadOpen(false)}
          onDownload={(repoUrl) => void handleDownload(repoUrl).catch(() => undefined)}
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
          onSubmit={() => void submitTargets().catch(() => undefined)}
          onToggle={toggleTarget}
        />
      ) : null}
      {deleteSkill ? (
        <SkillDeleteModal
          busy={Boolean(busy)}
          copy={copy}
          skill={deleteSkill}
          onClose={() => setDeleteSkill(null)}
          onDelete={(uninstallFirst) => void submitDelete(uninstallFirst).catch(() => undefined)}
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
  skills: string[];
  unknownSkillCount?: number;
};

function installedSkillNames(skills: SkillInventoryStatus["localSkills"]) {
  return Array.from(
    new Set(
      skills
        .filter((skill) => skill.hasSkillMd !== false)
        .map((skill) => skill.name.trim())
        .filter(Boolean)
    )
  ).sort((left, right) => left.localeCompare(right));
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
      skills: installedSkillNames(status.localSkills)
    },
    ...hosts.map((host) => {
      const inventory = hostInventoryByAlias.get(host.hostAlias.toLowerCase());
      return {
        key: `host-${host.hostAlias}`,
        alias: host.hostAlias,
        source: hostSourceLabel(copy, host),
        sourceTone: host.source === "managed" ? "blue" as const : "gray" as const,
        hostIp: host.address || "-",
        skills: inventory?.ok ? installedSkillNames(inventory.skills) : [],
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
      <section className="taskLogModal skillModal" role="dialog" aria-modal="true" aria-labelledby="skill-first-scan-title">
        <button className="modalCloseButton" disabled={busy} type="button" onClick={onClose} aria-label={copy.hosts.cancel}>
          x
        </button>
        <div className="taskLogModalHeader">
          <div>
            <h2 id="skill-first-scan-title">🔎 {copy.skills.firstScanTitle}</h2>
            <p>{copy.skills.firstScanBody}</p>
          </div>
        </div>
        <div className="modalActions">
          <button className="secondaryButton" disabled={busy} type="button" onClick={onClose}>
            {copy.hosts.cancel}
          </button>
          <button className="primaryButton" disabled={busy} type="button" onClick={onConfirm}>
            {busy ? copy.skills.detecting : copy.skills.firstScanAction}
          </button>
        </div>
      </section>
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
      <section className="taskLogModal skillModal" role="dialog" aria-modal="true" aria-labelledby="skill-download-title">
        <button className="modalCloseButton" disabled={busy} type="button" onClick={onClose} aria-label={copy.hosts.cancel}>
          x
        </button>
        <div className="taskLogModalHeader">
          <div>
            <h2 id="skill-download-title">⬇️ {copy.skills.downloadTitle}</h2>
          </div>
        </div>
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
          <div className="modalActions">
            <button className="secondaryButton" disabled={busy} type="button" onClick={onClose}>
              {copy.hosts.cancel}
            </button>
            <button className="primaryButton" disabled={!canSubmit} type="submit">
              {busy ? copy.common.loading : copy.skills.downloadAction}
            </button>
          </div>
        </form>
      </section>
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
      <section className="taskLogModal skillModal skillPreviewModal" role="dialog" aria-modal="true" aria-labelledby="skill-preview-title">
        <button className="modalCloseButton" type="button" onClick={onClose} aria-label={copy.setupGuide.close}>
          x
        </button>
        <div className="taskLogModalHeader">
          <div>
            <h2 id="skill-preview-title">🧩 {skill.name}</h2>
          </div>
          <div className="skillPreviewBadge">
            <Badge tone={skill.sourceType === "github" ? "blue" : "gray"}>
              {skill.sourceType === "github" ? copy.skills.sourceGithub : copy.skills.sourceLocal}
            </Badge>
          </div>
        </div>
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
        <div className="modalActions">
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
        </div>
      </section>
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
    <div className="modalBackdrop" role="presentation">
      <section className="taskLogModal skillModal" role="dialog" aria-modal="true" aria-labelledby="skill-targets-title">
        <button className="modalCloseButton" disabled={busy} type="button" onClick={onClose} aria-label={copy.hosts.cancel}>
          x
        </button>
        <div className="taskLogModalHeader">
          <div>
            <h2 id="skill-targets-title">{`🧩 ${actionLabel}: ${skill.name}`}</h2>
            <p>{hint}</p>
          </div>
        </div>
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
        <div className="modalActions">
          <button className="secondaryButton" disabled={busy || selectableCount === 0} type="button" onClick={onSelectAll}>
            {copy.skills.selectAll}
          </button>
          <button className="primaryButton" disabled={busy || selectedCount === 0} type="button" onClick={onSubmit}>
            {actionLabel}
          </button>
        </div>
      </section>
    </div>
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
    <div className="modalBackdrop" role="presentation">
      <section className="taskLogModal simpleDeleteModal" role="dialog" aria-modal="true" aria-labelledby="simple-delete-title">
        <div className="taskLogModalHeader">
          <div>
            <h2 id="simple-delete-title">{`🗑️ ${title}`}</h2>
            <p>{body}</p>
          </div>
        </div>
        <div className="modalActions">
          <button className="secondaryButton" disabled={busy} type="button" onClick={onClose}>
            {copy.hosts.cancel}
          </button>
          <button className="primaryButton dangerButton" disabled={busy} type="button" onClick={onDelete}>
            {copy.hosts.delete}
          </button>
        </div>
      </section>
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
    <div className="modalBackdrop" role="presentation">
      <section className="taskLogModal skillModal" role="dialog" aria-modal="true" aria-labelledby="skill-delete-title">
        <button className="modalCloseButton" disabled={busy} type="button" onClick={onClose} aria-label={copy.hosts.cancel}>
          x
        </button>
        <div className="taskLogModalHeader">
          <div>
            <h2 id="skill-delete-title">{`🗑️ ${copy.skills.deleteTitle}: ${skill.name}`}</h2>
            <p>{copy.skills.deleteBody}</p>
          </div>
        </div>
        <div className="modalActions skillDeleteActions">
          <button className="secondaryButton" disabled={busy} type="button" onClick={onClose}>
            {copy.hosts.cancel}
          </button>
          <button className="secondaryButton dangerButton" disabled={busy} type="button" onClick={() => onDelete(false)}>
            {copy.skills.directDelete}
          </button>
          <button className="primaryButton" disabled={busy} type="button" onClick={() => onDelete(true)}>
            {copy.skills.uninstallAndDelete}
          </button>
        </div>
      </section>
    </div>
  );
}

function TasksView({ copy, tasks }: { copy: UICopy; tasks: TaskRun[] }) {
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [nowTick, setNowTick] = useState(() => Date.now());
  const selectedTask = selectedTaskId ? tasks.find((task) => task.id === selectedTaskId) ?? null : null;

  useEffect(() => {
    const timer = window.setInterval(() => setNowTick(Date.now()), 30_000);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    if (selectedTaskId && !tasks.some((task) => task.id === selectedTaskId)) setSelectedTaskId(null);
  }, [selectedTaskId, tasks]);

  return (
    <div className="tasksGrid">
      <section className="panel spanWide">
        <div className="panelHeader compact">
          <div>
            <h2>{copy.tasks.taskHistory}</h2>
          </div>
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
                    <td>{task.summary}</td>
                    <td className="taskDetailsCol">
                      <button className="miniButton" type="button" onClick={() => setSelectedTaskId(task.id)}>{copy.tasks.logs}</button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
      {selectedTask ? <TaskLogModal copy={copy} now={nowTick} task={selectedTask} onClose={() => setSelectedTaskId(null)} /> : null}
    </div>
  );
}

function TaskLogModal({ copy, now, task, onClose }: { copy: UICopy; now: number; task: TaskRun; onClose: () => void }) {
  return (
    <div className="modalBackdrop" role="presentation">
      <section className="taskLogModal" role="dialog" aria-modal="true" aria-labelledby="task-log-modal-title">
        <button className="modalCloseButton" type="button" onClick={onClose} aria-label={copy.codexOperation.close}>
          ×
        </button>
        <div className="taskLogModalHeader">
          <div>
            <h2 id="task-log-modal-title">📋 {localizeTaskAction(task.action, copy)}</h2>
            <p>{task.summary}</p>
          </div>
          <TaskStatusBadge copy={copy} status={task.status} />
        </div>
        <div className="taskLogModalMeta">
          <div>
            <span>{copy.tasks.host}</span>
            <strong>{task.hostName}</strong>
          </div>
          <div>
            <span>{copy.tasks.started}</span>
            <strong>{formatTaskTimestamp(task, copy, now)}</strong>
          </div>
        </div>
        <div className="logList taskLogModalList">
          {task.logs.length > 0 ? task.logs.map((log) => (
            <details className="logLine" data-level={log.level} key={log.id}>
              <summary>
                <span>{log.timestamp}</span>
                <strong>{copy.status.log[log.level]}</strong>
                <p>{log.message}</p>
              </summary>
              <div className="logMetaGrid">
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
              <div className="streamGrid">
                <div>
                  <span>{copy.tasks.stdout}</span>
                  <pre>{log.stdout || copy.tasks.noOutput}</pre>
                </div>
                <div>
                  <span>{copy.tasks.stderr}</span>
                  <pre>{log.stderr || copy.tasks.noOutput}</pre>
                </div>
              </div>
            </details>
          )) : <p className="mutedText">{copy.tasks.noLogs}</p>}
        </div>
      </section>
    </div>
  );
}

function SettingsView({
  copy,
  localCodexBusy,
  localCodexStatus,
  settings,
  sshBusy,
  sshStatus,
  onCopyPublicKey,
  onFontPresetChange,
  onGenerateEd25519Key,
  onPlatformAppearanceChange,
  onRefreshLocalCodex,
  onRefreshSsh,
  onThemeChange
}: {
  copy: UICopy;
  localCodexBusy: boolean;
  localCodexStatus: LocalCodexStatus | null;
  settings: AppSettings;
  sshBusy: boolean;
  sshStatus: SshStatus | null;
  onCopyPublicKey: (publicKey: string) => Promise<boolean>;
  onFontPresetChange: (fontPreset: FontPreset) => void;
  onGenerateEd25519Key: () => Promise<void>;
  onPlatformAppearanceChange: (platformAppearance: PlatformAppearance) => void;
  onRefreshLocalCodex: () => Promise<LocalCodexStatus>;
  onRefreshSsh: () => Promise<void>;
  onThemeChange: (theme: ThemeChoice) => void;
}) {
  const publicKey = sshStatus?.ed25519.publicKey ?? sshStatus?.rsa.publicKey ?? "";
  const canGenerateEd25519 = Boolean(sshStatus?.sshKeygenAvailable && !sshStatus.ed25519.privateExists && !sshStatus.ed25519.publicExists);
  const [publicKeyCopied, setPublicKeyCopied] = useState(false);

  useEffect(() => {
    setPublicKeyCopied(false);
  }, [publicKey]);

  const handleCopyPublicKey = async () => {
    if (!publicKey) return;
    const copied = await onCopyPublicKey(publicKey);
    setPublicKeyCopied(copied);
  };

  return (
    <div className="settingsGrid">
      <section className="panel spanWide">
        <div className="panelHeader compact">
          <div>
            <h2>{copy.settings.appearance}</h2>
          </div>
        </div>
        <div className="settingsRows">
          <div className="settingControlRow">
            <span>{copy.settings.theme}</span>
            <div className="segmentedControl" role="group" aria-label={copy.settings.theme}>
              {(["system", "light", "dark"] as ThemeChoice[]).map((choice) => (
                <button data-active={settings.theme === choice} key={choice} onClick={() => onThemeChange(choice)} type="button">
                  {copy.settings.themeOptions[choice]}
                </button>
              ))}
            </div>
          </div>

          <div className="settingControlRow">
            <span>{copy.settings.platformAppearance}</span>
            <div className="segmentedControl" role="group" aria-label={copy.settings.platformAppearance}>
              {(["auto", "windows", "macos"] as PlatformAppearance[]).map((choice) => (
                <button data-active={settings.platformAppearance === choice} key={choice} onClick={() => onPlatformAppearanceChange(choice)} type="button">
                  {copy.settings.platformOptions[choice]}
                </button>
              ))}
            </div>
          </div>

          <div className="settingControlRow">
            <span>{copy.settings.font}</span>
            <div className="segmentedControl" data-options="2" role="group" aria-label={copy.settings.font}>
              {(Object.keys(fontPresets) as FontPreset[]).map((preset) => (
                <button data-active={settings.fontPreset === preset} key={preset} onClick={() => onFontPresetChange(preset)} type="button">
                  {fontPresets[preset].label}
                </button>
              ))}
            </div>
          </div>
        </div>
      </section>

      <section className="panel spanWide">
        <div className="panelHeader compact">
          <div>
            <h2>{copy.settings.localCodexCli}</h2>
          </div>
          <div className="topActions">
            <button className="secondaryButton" disabled={localCodexBusy} type="button" onClick={() => void onRefreshLocalCodex()}>
              {copy.settings.refresh}
            </button>
          </div>
        </div>
        <div className="detailGrid codexCliDetails">
          <div>
            <dt>{copy.settings.codexStatus}</dt>
            <dd>
              <Badge tone={localCodexStatus?.detected ? "green" : "yellow"}>
                {localCodexStatus?.detected ? copy.settings.localCodexDetected : copy.settings.localCodexMissing}
              </Badge>
            </dd>
          </div>
          <div>
            <dt>{copy.settings.codexVersion}</dt>
            <dd>{localCodexStatus?.version ?? copy.settings.unknown}</dd>
          </div>
          <div>
            <dt>{copy.settings.codexPath}</dt>
            <dd className="monospace">{localCodexStatus?.path ?? copy.settings.unknown}</dd>
          </div>
          <div>
            <dt>{copy.settings.codexSearchPaths}</dt>
            <dd className="monospace">{localCodexStatus?.searchPaths.join(" -> ") ?? copy.settings.unknown}</dd>
          </div>
          <div>
            <dt>{copy.settings.codexInstallHint}</dt>
            <dd>{localCodexStatus?.installHint ?? copy.settings.unknown}</dd>
          </div>
        </div>
      </section>

      <section className="panel spanWide">
        <div className="panelHeader compact">
          <div>
            <h2>{copy.settings.localSsh}</h2>
          </div>
          <div className="topActions">
            <button className="secondaryButton" type="button" onClick={() => void onRefreshSsh()}>{copy.settings.refresh}</button>
            <button className="secondaryButton copyPublicKeyButton" data-success={publicKeyCopied} disabled={!publicKey} type="button" onClick={() => void handleCopyPublicKey()}>
              {publicKeyCopied ? copy.settings.copyPublicKeySuccess : copy.settings.copyPublicKey}
            </button>
            <button className="primaryButton" disabled={!canGenerateEd25519 || sshBusy} type="button" onClick={() => void onGenerateEd25519Key()}>
              {sshBusy ? copy.settings.generating : copy.settings.generateEd25519}
            </button>
          </div>
        </div>

        <div className="keyStatusGrid">
          <KeyStatusCard copy={copy} keyInfo={sshStatus?.ed25519} title="Ed25519" />
          <KeyStatusCard copy={copy} keyInfo={sshStatus?.rsa} title="RSA" />
        </div>
      </section>
    </div>
  );
}

function KeyStatusCard({ copy, keyInfo, title }: { copy: UICopy; keyInfo: SshKeyInfo | undefined; title: string }) {
  return (
    <article className="keyStatusCard">
      <div className="hostHeader">
        <h3>{title}</h3>
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
  return <Badge tone={source === "cc-switch" ? "green" : "blue"}>{label}</Badge>;
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
  return tasks.map((task) => normalizeTaskRunForUi(task, receivedAt));
}

function normalizeTaskRunForUi(task: TaskRun, receivedAt = new Date().toISOString()): TaskRun {
  if (taskTimestampMillis(task)) return task;
  const startedAt = isNowTimeLabel(task.startedAt) ? receivedAt : task.startedAt;
  const endedAt = task.endedAt && isNowTimeLabel(task.endedAt) ? receivedAt : task.endedAt;
  const logs = task.logs.map((log) => (isNowTimeLabel(log.timestamp) ? { ...log, timestamp: receivedAt } : log));
  return { ...task, startedAt, endedAt, logs };
}

function TaskStatusBadge({ copy, status }: { copy: UICopy; status: TaskStatus }) {
  const tone: BadgeTone = status === "success" ? "green" : status === "failed" ? "red" : status === "running" ? "yellow" : "gray";
  return <Badge tone={tone}>{copy.status.task[status]}</Badge>;
}

function localizeTaskAction(action: string, copy: UICopy) {
  const labels = copy.tasks.actionLabels as Record<string, string>;
  return labels[action] ?? action;
}

function formatTaskTimestamp(task: TaskRun, copy: UICopy, now = Date.now()) {
  const timestamp = taskTimestampMillis(task);
  if (!timestamp) return task.startedAt || "-";
  const date = new Date(timestamp);
  const deltaMs = Math.max(0, now - timestamp);
  const zh = copy.navItems[0].label === "主页";
  if (deltaMs < 60_000) return zh ? "刚刚" : "just now";
  if (deltaMs < 60 * 60_000) {
    const minutes = Math.max(1, Math.floor(deltaMs / 60_000));
    return zh ? `${minutes} 分钟前` : `${minutes}m ago`;
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
  if (inventory?.ok) return installedSkillNames(inventory.skills).length;
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
    return copy.hosts.updatingCodex;
  }
  if (action === "check-version") return copy.hosts.checkVersion;
  if (action === "install") return copy.hosts.installCodex;
  return copy.hosts.updateCodex;
}

function compactTaskLogDetail(log: TaskRun["logs"][number]) {
  const detail = firstOutputLine(log.stderr) || firstOutputLine(log.stdout);
  if (detail) return detail;
  if (typeof log.exitCode === "number") return `exit ${log.exitCode}`;
  if (typeof log.durationMs === "number") return `${log.durationMs} ms`;
  return "";
}

function progressLogLevel(log: RemoteCodexProgressEvent): "info" | "warn" | "error" {
  if (log.status === "failed" || log.status === "stderr") return "error";
  if (log.status === "heartbeat") return "warn";
  return "info";
}

function progressLogLabel(copy: UICopy, log: RemoteCodexProgressEvent) {
  return copy.status.log[progressLogLevel(log)];
}

function compactProgressLogDetail(log: RemoteCodexProgressEvent) {
  if (log.stderr) return log.stderr;
  if (log.stdout) return log.stdout;
  if (log.detail) return log.detail;
  if (typeof log.exitCode === "number") return `exit ${log.exitCode}`;
  if (typeof log.durationMs === "number") return `${log.durationMs} ms`;
  return log.step;
}

function firstOutputLine(value: string | null | undefined) {
  return value?.trim().split(/\r?\n/).find(Boolean) ?? "";
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

function formatNullableBoolean(value: boolean | null, copy: UICopy) {
  if (value === null) return copy.hosts.unknown;
  return formatBoolean(value, copy);
}

function formatLatency(value: number | null | undefined, copy: UICopy) {
  return typeof value === "number" ? `${value} ms` : copy.hosts.unknown;
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

export default App;
