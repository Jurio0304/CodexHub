import { useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { api, fallbackHealth } from "./api";
import type {
  Health,
  Host,
  HostStatus,
  Profile,
  SkillPack,
  SshBootstrapProgressEvent,
  SshBootstrapResult,
  SshBootstrapStep,
  SshBootstrapStepStatus,
  SshConfigHost,
  SshHostDraft,
  SshKeyInfo,
  SshStatus,
  TaskRun,
  TaskStatus
} from "./models";
import { applyAppSettings, fontPresets, loadLocalSettings } from "./settings";
import type { AppSettings, FontPreset, ThemeChoice } from "./settings";

type SectionId = "dashboard" | "hosts" | "profiles" | "skills" | "tasks" | "settings";
type Locale = "en" | "zh";
type HostBusyAction = "test" | "probe" | "bootstrap";

const uiCopy = {
  en: {
    navItems: [
      { id: "dashboard", label: "Dashboard", description: "Overview" },
      { id: "hosts", label: "Hosts", description: "SSH targets" },
      { id: "profiles", label: "Profiles", description: "Codex TOML" },
      { id: "skills", label: "Skills", description: "Skill packs" },
      { id: "tasks", label: "Tasks", description: "Runs and logs" },
      { id: "settings", label: "Settings", description: "App options" }
    ] satisfies Array<{ id: SectionId; label: string; description: string }>,
    sections: {
      dashboard: {
        title: "Control plane",
        eyebrow: "Dashboard",
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
      backendMode: "backend mode",
      desktopMvp: "Desktop MVP",
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
      summaryLabel: "Dashboard summary",
      online: "online",
      mockPresets: "mock presets",
      enabled: "enabled",
      active: "active",
      backendContract: "Backend contract",
      sshManagementWired: "SSH management wired",
      wrapper: "wrapper",
      serverMatrix: "Server Matrix",
      mockHosts: "Mock hosts",
      matrixBody: "Inventory cards show the shape of the future SSH connection matrix.",
      noHosts: "No hosts yet",
      noHostsBody: "Add the first SSH target to populate the server matrix.",
      noSkillPacks: "No skill packs",
      recentTasks: "Recent tasks",
      activity: "Activity",
      viewAll: "View all"
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
      deleteConfirm: (alias: string) => `Delete CodexHub-managed Host ${alias} from SSH config?`,
      deletedHost: (alias: string) => `Deleted Host ${alias}.`,
      hostAlias: "Host Alias",
      hostName: "HostName",
      port: "Port",
      user: "User",
      identityFile: "IdentityFile",
      bootstrapPassword: "One-time password",
      bootstrapPasswordHelp: "Optional. Used once to log in, append your public key to ~/.ssh/authorized_keys, then test key login.",
      cancel: "Cancel",
      saving: "Saving...",
      writeSshConfig: "Connect",
      reset: "Reset",
      codexhubManaged: "CodexHub managed",
      sshHostBlocks: "SSH Host blocks",
      repeatedSaves: "Repeated saves update the same alias instead of appending duplicates.",
      newHost: "New Host",
      noManagedHosts: "No managed SSH hosts",
      noManagedHostsBody: "Click Add Server in the Hosts header to connect and create the first CodexHub-managed block in SSH config.",
      detectedSshHosts: "Detected SSH hosts",
      detectedSshHostsBody: "CodexHub scans local SSH config in read-only mode and imports safe HostAlias entries into the inventory.",
      refreshDetected: "Refresh detected hosts",
      readOnlySource: "read-only",
      source: "Source",
      probe: "Probe",
      probing: "Probing...",
      bootstrapping: "Bootstrapping...",
      testing: "Testing...",
      details: "Host details",
      detailsBody: "Connection status and remote Codex readiness from the latest test or probe.",
      sshStatus: "SSH status",
      arch: "Arch",
      shell: "Shell",
      pathLocalBin: "PATH has ~/.local/bin",
      codexInstalled: "Codex installed",
      codexVersion: "Codex version",
      configExists: "Config exists",
      skillsCount: "Skills count",
      yes: "Yes",
      no: "No",
      unknown: "Unknown",
      mockInventory: "Mock inventory",
      existingHosts: "Existing app hosts",
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
      latency: "Latency"
    },
    profiles: {
      hosts: "hosts",
      approval: "Approval",
      sandbox: "Sandbox",
      updated: "Updated",
      applyOnline: "Apply to online hosts"
    },
    skills: {
      source: "Source",
      skills: "Skills",
      updated: "Updated",
      enabled: "enabled",
      disabled: "disabled"
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
        "Probe remote system": "Probe remote system"
      }
    },
    settings: {
      appearance: "Appearance",
      theme: "Theme",
      font: "Font",
      runtime: "Runtime",
      backend: "Backend",
      app: "App",
      remoteWrapper: "Remote wrapper",
      sshConfig: "SSH config",
      desktopBackendRequired: "desktop backend required",
      localSsh: "Local SSH",
      sshKeyStatus: "SSH key status",
      sshKeyBody: "Private key files are checked by existence only. CodexHub never reads or displays private key content.",
      refresh: "Refresh",
      generating: "Generating...",
      generateEd25519: "Generate Ed25519",
      publicKey: "Public key",
      readyToCopy: "Ready to copy",
      noPublicKey: "No public key detected",
      copyPublicKey: "Copy Public Key",
      publicKeyEmpty: "Generate or add an SSH public key to show it here.",
      commandReservations: "Command reservations",
      commandSurface: "Tauri command surface",
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
      { id: "dashboard", label: "仪表盘", description: "总览" },
      { id: "hosts", label: "主机", description: "SSH 目标" },
      { id: "profiles", label: "配置", description: "Codex TOML" },
      { id: "skills", label: "技能", description: "技能包" },
      { id: "tasks", label: "任务", description: "运行与日志" },
      { id: "settings", label: "设置", description: "应用选项" }
    ] satisfies Array<{ id: SectionId; label: string; description: string }>,
    sections: {
      dashboard: {
        title: "控制面板",
        eyebrow: "仪表盘",
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
      desktopMvp: "桌面 MVP",
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
      summaryLabel: "仪表盘概览",
      online: "在线",
      mockPresets: "mock 预设",
      enabled: "已启用",
      active: "活跃",
      backendContract: "后端约定",
      sshManagementWired: "SSH 管理已接入",
      wrapper: "包装器",
      serverMatrix: "服务器矩阵",
      mockHosts: "Mock 主机",
      matrixBody: "清单卡片用于展示 SSH 连接矩阵的结构。",
      noHosts: "还没有主机",
      noHostsBody: "添加第一个 SSH 目标后会填充服务器矩阵。",
      noSkillPacks: "无技能包",
      recentTasks: "最近任务",
      activity: "活动",
      viewAll: "查看全部"
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
      deleteConfirm: (alias: string) => `确定要从 SSH config 删除 CodexHub 管理的 Host ${alias} 吗？`,
      deletedHost: (alias: string) => `已删除 Host ${alias}。`,
      hostAlias: "Host 别名",
      hostName: "HostName",
      port: "端口",
      user: "用户",
      identityFile: "IdentityFile",
      bootstrapPassword: "一次性密码",
      bootstrapPasswordHelp: "可选。仅用于首次登录，把本地公钥追加到远端 ~/.ssh/authorized_keys，然后测试密钥登录。",
      cancel: "取消",
      saving: "保存中...",
      writeSshConfig: "连接",
      reset: "重置",
      codexhubManaged: "CodexHub 管理",
      sshHostBlocks: "SSH Host 块",
      repeatedSaves: "重复保存会更新同一别名，不会追加重复块。",
      newHost: "新建 Host",
      noManagedHosts: "没有受管理的 SSH 主机",
      noManagedHostsBody: "点击主机页右上角“添加服务器”，连接成功后会创建第一个 CodexHub 管理块。",
      detectedSshHosts: "已检测 SSH hosts",
      detectedSshHostsBody: "CodexHub 只读扫描本地 SSH config，并把安全的 HostAlias 自动加入清单。",
      refreshDetected: "刷新检测",
      readOnlySource: "只读",
      source: "来源",
      probe: "探测",
      probing: "探测中...",
      bootstrapping: "连接中...",
      testing: "测试中...",
      details: "主机详情",
      detailsBody: "展示最近一次测试或探测得到的连接状态与远端 Codex 就绪度。",
      sshStatus: "SSH 状态",
      arch: "架构",
      shell: "Shell",
      pathLocalBin: "PATH 含 ~/.local/bin",
      codexInstalled: "Codex 已安装",
      codexVersion: "Codex 版本",
      configExists: "Config 存在",
      skillsCount: "Skills 数量",
      yes: "是",
      no: "否",
      unknown: "未知",
      mockInventory: "Mock 清单",
      existingHosts: "现有应用主机",
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
      latency: "延迟"
    },
    profiles: {
      hosts: "台主机",
      approval: "审批",
      sandbox: "沙箱",
      updated: "更新于",
      applyOnline: "应用到在线主机"
    },
    skills: {
      source: "来源",
      skills: "技能",
      updated: "更新于",
      enabled: "已启用",
      disabled: "已禁用"
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
        "Probe remote system": "探测远端系统"
      }
    },
    settings: {
      appearance: "外观",
      theme: "主题",
      font: "字体",
      runtime: "运行时",
      backend: "后端",
      app: "应用",
      remoteWrapper: "远程包装器",
      sshConfig: "SSH 配置",
      desktopBackendRequired: "需要桌面后端",
      localSsh: "本地 SSH",
      sshKeyStatus: "SSH 密钥状态",
      sshKeyBody: "仅检查私钥文件是否存在。CodexHub 从不读取或显示私钥内容。",
      refresh: "刷新",
      generating: "生成中...",
      generateEd25519: "生成 Ed25519",
      publicKey: "公钥",
      readyToCopy: "可复制",
      noPublicKey: "未检测到公钥",
      copyPublicKey: "复制公钥",
      publicKeyEmpty: "生成或添加 SSH 公钥后会显示在这里。",
      commandReservations: "命令预留",
      commandSurface: "Tauri 命令接口",
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

function App() {
  const [activeSection, setActiveSection] = useState<SectionId>("dashboard");
  const [settings, setSettings] = useState<AppSettings>(() => loadLocalSettings());
  const [health, setHealth] = useState<Health>(fallbackHealth);
  const [hosts, setHosts] = useState<Host[]>([]);
  const [profiles, setProfiles] = useState<Profile[]>([]);
  const [skillPacks, setSkillPacks] = useState<SkillPack[]>([]);
  const [tasks, setTasks] = useState<TaskRun[]>([]);
  const [sshStatus, setSshStatus] = useState<SshStatus | null>(null);
  const [sshConfigHosts, setSshConfigHosts] = useState<SshConfigHost[]>([]);
  const [loading, setLoading] = useState(true);
  const [sshBusy, setSshBusy] = useState(false);
  const [hostBusy, setHostBusy] = useState<Record<string, HostBusyAction>>({});
  const [hostModalOpen, setHostModalOpen] = useState(false);
  const [notice, setNotice] = useState<string>(uiCopy.en.notices.default);

  const locale: Locale = settings.fontPreset === "zh-cn" ? "zh" : "en";
  const copy = uiCopy[locale];

  const refreshSshState = async () => {
    const [nextSshStatus, nextSshConfigHosts, nextHosts] = await Promise.all([
      api.getSshStatus(),
      api.listSshConfigHosts(),
      api.refreshDiscoveredHosts()
    ]);
    setSshStatus(nextSshStatus);
    setSshConfigHosts(nextSshConfigHosts);
    setHosts(nextHosts);
  };

  useEffect(() => {
    let mounted = true;

    Promise.all([
      api.getSettings(),
      api.getHealth(),
      api.listHosts(),
      api.listProfiles(),
      api.listSkillPacks(),
      api.listTasks(),
      api.getSshStatus(),
      api.listSshConfigHosts()
    ])
      .then(([nextSettings, nextHealth, nextHosts, nextProfiles, nextSkillPacks, nextTasks, nextSshStatus, nextSshConfigHosts]) => {
        if (!mounted) return;
        setSettings(nextSettings);
        setHealth(nextHealth);
        setHosts(nextHosts);
        setProfiles(nextProfiles);
        setSkillPacks(nextSkillPacks);
        setTasks(nextTasks);
        setSshStatus(nextSshStatus);
        setSshConfigHosts(nextSshConfigHosts);
      })
      .finally(() => {
        if (mounted) setLoading(false);
      });

    return () => {
      mounted = false;
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
  const activeTasks = tasks.filter((task) => task.status === "queued" || task.status === "running").length;

  const profileById = useMemo(() => new Map(profiles.map((profile) => [profile.id, profile])), [profiles]);
  const skillPackById = useMemo(() => new Map(skillPacks.map((pack) => [pack.id, pack])), [skillPacks]);

  const handleAddHost = () => {
    setActiveSection("hosts");
    setHostModalOpen(true);
    setNotice(copy.notices.addHost);
  };

  const handleDeleteHost = async (id: string) => {
    const host = hosts.find((item) => item.id === id);
    const removed = await api.deleteHost(id);
    if (!removed) return;
    setHosts((current) => current.filter((item) => item.id !== id));
    setNotice(copy.notices.mockHostRemoved(host?.name ?? copy.common.host));
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
      setTasks((current) => [result.task, ...current]);
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
    setNotice(result.backupPath ? `${result.message} Backup: ${result.backupPath}` : result.message);
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
    } catch {
      setNotice(copy.notices.copyFailed);
    }
  };

  const handleRefreshDetectedHosts = async () => {
    await refreshSshState();
    setNotice(copy.hosts.detectedSshHostsBody);
  };

  const handleTestHost = async (idOrAlias: string) => {
    const target = hosts.find((host) => host.id === idOrAlias || host.hostAlias === idOrAlias);
    const hostAlias = target?.hostAlias ?? idOrAlias;
    setHostBusy((current) => ({ ...current, [hostAlias]: "test" }));
    setHosts((current) => current.map((host) => (host.hostAlias === hostAlias ? { ...host, status: "testing" } : host)));

    const result = await api.sshCheck(hostAlias);
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
    setTasks((current) => [result.task, ...current]);
    setNotice(`${target?.name ?? hostAlias}: ${result.message}`);
    setHostBusy((current) => {
      const next = { ...current };
      delete next[hostAlias];
      return next;
    });
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
      setTasks((current) => [result.task, ...current]);
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

  const handleProbeHost = async (idOrAlias: string) => {
    const target = hosts.find((host) => host.id === idOrAlias || host.hostAlias === idOrAlias);
    const hostAlias = target?.hostAlias ?? idOrAlias;
    setHostBusy((current) => ({ ...current, [hostAlias]: "probe" }));
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
              skillsExists: result.skillsExists,
              skillsCount: result.skillsCount,
              lastSeen: result.sshStatus === "online" ? copy.common.justNow : host.lastSeen
            }
          : host
      )
    );
    setTasks((current) => [result.task, ...current]);
    setNotice(`${target?.name ?? hostAlias}: ${result.task.summary}`);
    setHostBusy((current) => {
      const next = { ...current };
      delete next[hostAlias];
      return next;
    });
  };

  const handleApplyProfile = async (profileId: string) => {
    const targetHostIds = hosts.filter((host) => host.status === "online").map((host) => host.id);
    const hostIds = targetHostIds.length > 0 ? targetHostIds : hosts.slice(0, 1).map((host) => host.id);
    if (hostIds.length === 0) {
      setNotice(copy.notices.addHostBeforeProfile);
      return;
    }

    const task = await api.applyProfile(profileId, hostIds);
    setTasks((current) => [task, ...current]);
    setHosts((current) => current.map((host) => (hostIds.includes(host.id) ? { ...host, profileId } : host)));
    setActiveSection("tasks");
    setNotice(task.summary);
  };

  const persistSettings = (nextSettings: AppSettings) => {
    setSettings(nextSettings);
    applyAppSettings(nextSettings);
    void api.saveSettings(nextSettings).then(setSettings);
  };

  const renderContent = () => {
    switch (activeSection) {
      case "dashboard":
        return (
          <DashboardView
            activeTasks={activeTasks}
            copy={copy}
            health={health}
            hostBusy={hostBusy}
            hosts={hosts}
            loading={loading}
            notice={notice}
            onlineCount={onlineCount}
            profiles={profiles}
            skillPacks={skillPacks}
            tasks={tasks}
            profileById={profileById}
            skillPackById={skillPackById}
            onProbeHost={handleProbeHost}
            onSelectSection={setActiveSection}
            onTestHost={handleTestHost}
          />
        );
      case "hosts":
        return (
          <HostsView
            copy={copy}
            hosts={hosts}
            hostBusy={hostBusy}
            profileById={profileById}
            skillPackById={skillPackById}
            sshConfigHosts={sshConfigHosts}
            sshStatus={sshStatus}
            addHostOpen={hostModalOpen}
            sshBusy={sshBusy}
            onCloseAddHost={() => setHostModalOpen(false)}
            onConnectSshHost={handleConnectSshHost}
            onDeleteHost={handleDeleteHost}
            onDeleteSshConfigHost={handleDeleteSshConfigHost}
            onGenerateEd25519Key={handleGenerateEd25519Key}
            onOpenAddHost={handleAddHost}
            onProbeHost={handleProbeHost}
            onRefreshDetectedHosts={handleRefreshDetectedHosts}
            onTestHost={handleTestHost}
          />
        );
      case "profiles":
        return <ProfilesView copy={copy} hosts={hosts} profiles={profiles} onApplyProfile={handleApplyProfile} />;
      case "skills":
        return <SkillsView copy={copy} skillPacks={skillPacks} />;
      case "tasks":
        return <TasksView copy={copy} tasks={tasks} />;
      case "settings":
        return (
          <SettingsView
            copy={copy}
            health={health}
            settings={settings}
            sshBusy={sshBusy}
            sshStatus={sshStatus}
            onCopyPublicKey={handleCopyPublicKey}
            onFontPresetChange={(fontPreset) => persistSettings({ ...settings, fontPreset })}
            onGenerateEd25519Key={handleGenerateEd25519Key}
            onRefreshSsh={refreshSshState}
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
          <div className="appIcon" aria-hidden="true">CH</div>
          <div>
            <div className="brandName">CodexHub</div>
            <div className="brandSubtle">{copy.common.desktopMvp}</div>
          </div>
        </div>

        <nav className="navList">
          {copy.navItems.map((item) => (
            <button className="navItem" data-active={activeSection === item.id} key={item.id} onClick={() => setActiveSection(item.id)} type="button">
              <span>{item.label}</span>
              <small>{item.description}</small>
            </button>
          ))}
        </nav>

        <div className="sidebarFooter">
          <span className="statusDot" data-status={health.mode === "tauri" ? "online" : "unknown"} />
          <div>
            <strong>{health.mode}</strong>
            <span>{copy.common.backendMode}</span>
          </div>
        </div>
      </aside>

      <main className="contentShell">
        <header className="topBar">
          <div>
            <div className="eyebrow">{selectedCopy.eyebrow}</div>
            <h1>{selectedCopy.title}</h1>
            <p>{selectedCopy.body}</p>
          </div>
          <div className="topActions">
            <Badge tone={health.mode === "tauri" ? "green" : "gray"}>{health.mode}</Badge>
            {activeSection === "hosts" ? (
              <button className="primaryButton" type="button" onClick={handleAddHost}>{copy.common.addServer}</button>
            ) : null}
          </div>
        </header>

        {renderContent()}
      </main>
    </div>
  );
}

function DashboardView({
  activeTasks,
  copy,
  health,
  hostBusy,
  hosts,
  loading,
  notice,
  onlineCount,
  profiles,
  profileById,
  skillPackById,
  skillPacks,
  tasks,
  onProbeHost,
  onSelectSection,
  onTestHost
}: {
  activeTasks: number;
  copy: UICopy;
  health: Health;
  hostBusy: Record<string, HostBusyAction>;
  hosts: Host[];
  loading: boolean;
  notice: string;
  onlineCount: number;
  profiles: Profile[];
  profileById: Map<string, Profile>;
  skillPackById: Map<string, SkillPack>;
  skillPacks: SkillPack[];
  tasks: TaskRun[];
  onProbeHost: (id: string) => void;
  onSelectSection: (section: SectionId) => void;
  onTestHost: (id: string) => void;
}) {
  return (
    <div className="pageGrid">
      <section className="summaryStrip" aria-label={copy.dashboard.summaryLabel}>
        <MetricCard label={copy.navItems[1].label} value={String(hosts.length)} detail={`${onlineCount} ${copy.dashboard.online}`} />
        <MetricCard label={copy.navItems[2].label} value={String(profiles.length)} detail={copy.dashboard.mockPresets} />
        <MetricCard label={copy.navItems[3].label} value={String(skillPacks.length)} detail={`${skillPacks.filter((pack) => pack.enabled).length} ${copy.dashboard.enabled}`} />
        <MetricCard label={copy.navItems[4].label} value={String(tasks.length)} detail={`${activeTasks} ${copy.dashboard.active}`} />
      </section>

      <section className="panel calloutPanel">
        <div>
          <div className="eyebrow">{copy.dashboard.backendContract}</div>
          <h2>{copy.dashboard.sshManagementWired}</h2>
          <p>{notice}</p>
        </div>
        <div className="calloutMeta">
          <Badge tone={health.remoteWrapperRequired ? "yellow" : "green"}>{copy.dashboard.wrapper} {health.remoteWrapperRequired ? copy.common.required : copy.common.notRequired}</Badge>
          <Badge tone={loading ? "yellow" : "green"}>{loading ? copy.common.loading : copy.common.ready}</Badge>
        </div>
      </section>

      <ServerMatrix copy={copy} hostBusy={hostBusy} hosts={hosts} profileById={profileById} skillPackById={skillPackById} onProbeHost={onProbeHost} onTestHost={onTestHost} />
      <RecentTasks copy={copy} tasks={tasks} onViewAll={() => onSelectSection("tasks")} />
    </div>
  );
}

function MetricCard({ label, value, detail }: { label: string; value: string; detail: string }) {
  return (
    <article className="metricCard">
      <span>{label}</span>
      <strong>{value}</strong>
      <small>{detail}</small>
    </article>
  );
}

function ServerMatrix({
  copy,
  hostBusy,
  hosts,
  profileById,
  skillPackById,
  onProbeHost,
  onTestHost
}: {
  copy: UICopy;
  hostBusy: Record<string, HostBusyAction>;
  hosts: Host[];
  profileById: Map<string, Profile>;
  skillPackById: Map<string, SkillPack>;
  onProbeHost: (id: string) => void;
  onTestHost: (id: string) => void;
}) {
  return (
    <section className="panel spanWide">
      <div className="panelHeader">
        <div>
          <div className="eyebrow">{copy.dashboard.serverMatrix}</div>
          <h2>{copy.dashboard.mockHosts}</h2>
          <p>{copy.dashboard.matrixBody}</p>
        </div>
      </div>

      {hosts.length === 0 ? (
        <div className="emptyState">
          <div className="emptyIcon" aria-hidden="true" />
          <h3>{copy.dashboard.noHosts}</h3>
          <p>{copy.dashboard.noHostsBody}</p>
        </div>
      ) : (
        <div className="matrixGrid">
          {hosts.map((host) => (
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
                  <dt>{copy.hosts.os}</dt>
                  <dd>{host.os}{host.arch && host.arch !== "Unknown" ? ` / ${host.arch}` : ""}</dd>
                </div>
                <div>
                  <dt>{copy.hosts.codex}</dt>
                  <dd>{formatBoolean(host.codexInstalled, copy)} · {host.codexVersion}</dd>
                </div>
                <div>
                  <dt>{copy.hosts.profile}</dt>
                  <dd>{host.profileId ? profileById.get(host.profileId)?.name ?? host.profileId : copy.common.unassigned}</dd>
                </div>
                <div>
                  <dt>{copy.hosts.latency}</dt>
                  <dd>{host.latencyMs ? `${host.latencyMs} ms` : "-"}</dd>
                </div>
              </dl>

              <div className="tagRow" aria-label={`${host.name} tags`}>
                {host.tags.map((tag) => <span key={tag}>{tag}</span>)}
              </div>

              <div className="skillLine">
                {host.skillPackIds.length > 0 ? host.skillPackIds.map((id) => skillPackById.get(id)?.name ?? id).join(", ") : copy.dashboard.noSkillPacks}
              </div>

              <div className="hostCardActions">
                <button className="tertiaryButton" disabled={Boolean(hostBusy[host.hostAlias])} type="button" onClick={() => onTestHost(host.hostAlias)}>
                  {hostBusy[host.hostAlias] === "test" ? copy.hosts.testing : copy.hosts.testSsh}
                </button>
                <button className="tertiaryButton" disabled={Boolean(hostBusy[host.hostAlias])} type="button" onClick={() => onProbeHost(host.hostAlias)}>
                  {hostBusy[host.hostAlias] === "probe" ? copy.hosts.probing : copy.hosts.probe}
                </button>
              </div>
            </article>
          ))}
        </div>
      )}
    </section>
  );
}

function RecentTasks({ copy, tasks, onViewAll }: { copy: UICopy; tasks: TaskRun[]; onViewAll: () => void }) {
  return (
    <section className="panel">
      <div className="panelHeader compact">
        <div>
          <div className="eyebrow">{copy.dashboard.recentTasks}</div>
          <h2>{copy.dashboard.activity}</h2>
        </div>
        <button className="linkButton" type="button" onClick={onViewAll}>{copy.dashboard.viewAll}</button>
      </div>

      <div className="taskList">
        {tasks.slice(0, 4).map((task) => (
          <article className="taskItem" key={task.id}>
            <div>
              <strong>{localizeTaskAction(task.action, copy)}</strong>
              <span>{task.hostName}</span>
            </div>
            <TaskStatusBadge copy={copy} status={task.status} />
          </article>
        ))}
      </div>
    </section>
  );
}

function HostsView({
  addHostOpen,
  copy,
  hostBusy,
  hosts,
  profileById,
  skillPackById,
  sshBusy,
  sshConfigHosts,
  sshStatus,
  onCloseAddHost,
  onConnectSshHost,
  onDeleteHost,
  onDeleteSshConfigHost,
  onGenerateEd25519Key,
  onOpenAddHost,
  onProbeHost,
  onRefreshDetectedHosts,
  onTestHost
}: {
  addHostOpen: boolean;
  copy: UICopy;
  hostBusy: Record<string, HostBusyAction>;
  hosts: Host[];
  profileById: Map<string, Profile>;
  skillPackById: Map<string, SkillPack>;
  sshBusy: boolean;
  sshConfigHosts: SshConfigHost[];
  sshStatus: SshStatus | null;
  onCloseAddHost: () => void;
  onConnectSshHost: (draft: SshHostDraft, password: string, requestId: string, onProgress: (event: SshBootstrapProgressEvent) => void) => Promise<SshBootstrapResult>;
  onDeleteHost: (id: string) => void;
  onDeleteSshConfigHost: (alias: string) => Promise<void>;
  onGenerateEd25519Key: () => Promise<void>;
  onOpenAddHost: () => void;
  onProbeHost: (id: string) => void;
  onRefreshDetectedHosts: () => Promise<void>;
  onTestHost: (id: string) => void;
}) {
  const identityFile = sshStatus?.ed25519.privateExists ? sshStatus.ed25519.privatePath : "";
  const [selectedHostAlias, setSelectedHostAlias] = useState<string | null>(hosts[0]?.hostAlias ?? null);
  const [editingDraft, setEditingDraft] = useState<SshHostDraft | null>(null);
  const selectedHost = hosts.find((host) => host.hostAlias === selectedHostAlias) ?? hosts[0] ?? null;

  useEffect(() => {
    if (!selectedHostAlias && hosts[0]) setSelectedHostAlias(hosts[0].hostAlias);
  }, [hosts, selectedHostAlias]);

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

  const handleDelete = async (alias: string) => {
    const confirmed = window.confirm(copy.hosts.deleteConfirm(alias));
    if (!confirmed) return;
    await onDeleteSshConfigHost(alias);
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
            <div className="eyebrow">{copy.hosts.codexhubManaged}</div>
            <h2>{copy.hosts.detectedSshHosts}</h2>
            <p>{copy.hosts.detectedSshHostsBody}</p>
          </div>
          <button className="secondaryButton" type="button" onClick={() => void onRefreshDetectedHosts()}>{copy.hosts.refreshDetected}</button>
        </div>

        {sshConfigHosts.length === 0 ? (
          <div className="emptyState">
            <div className="emptyIcon" aria-hidden="true" />
            <h3>{copy.hosts.noManagedHosts}</h3>
            <p>{copy.hosts.noManagedHostsBody}</p>
          </div>
        ) : (
          <div className="tableWrap">
            <table>
              <thead>
                <tr>
                  <th>{copy.hosts.alias}</th>
                  <th>{copy.hosts.source}</th>
                  <th>{copy.hosts.hostName}</th>
                  <th>{copy.hosts.port}</th>
                  <th>{copy.hosts.user}</th>
                  <th>{copy.hosts.identityFile}</th>
                  <th>{copy.hosts.actions}</th>
                </tr>
              </thead>
              <tbody>
                {sshConfigHosts.map((host) => (
                  <tr className="selectableRow" data-selected={selectedHost?.hostAlias === host.alias} key={host.alias} onClick={() => setSelectedHostAlias(host.alias)}>
                    <td><strong>{host.alias}</strong></td>
                    <td><Badge tone={host.managed ? "blue" : "gray"}>{host.managed ? copy.hosts.codexhubManaged : copy.hosts.readOnlySource}</Badge></td>
                    <td>{host.hostName}</td>
                    <td>{host.port}</td>
                    <td>{host.user}</td>
                    <td><code>{host.identityFile || "-"}</code></td>
                    <td className="tableActions">
                      <button className="miniButton" disabled={Boolean(hostBusy[host.alias])} type="button" onClick={(event) => { event.stopPropagation(); onTestHost(host.alias); }}>
                        {hostBusy[host.alias] === "test" ? copy.hosts.testing : copy.hosts.test}
                      </button>
                      <button className="miniButton" disabled={Boolean(hostBusy[host.alias])} type="button" onClick={(event) => { event.stopPropagation(); onProbeHost(host.alias); }}>
                        {hostBusy[host.alias] === "probe" ? copy.hosts.probing : copy.hosts.probe}
                      </button>
                      {host.managed ? (
                        <>
                          <button className="miniButton" type="button" onClick={(event) => { event.stopPropagation(); handleEdit(host); }}>{copy.hosts.edit}</button>
                          <button className="miniButton danger" type="button" onClick={(event) => { event.stopPropagation(); void handleDelete(host.alias); }}>{copy.hosts.delete}</button>
                        </>
                      ) : null}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <HostDetailsPanel copy={copy} host={selectedHost} />

      <section className="panel spanWide">
        <div className="panelHeader">
          <div>
            <div className="eyebrow">{copy.hosts.mockInventory}</div>
            <h2>{copy.hosts.existingHosts}</h2>
            <p>{copy.hosts.mockInventoryBody}</p>
          </div>
        </div>

        <div className="tableWrap">
          <table>
            <thead>
              <tr>
                <th>{copy.hosts.name}</th>
                <th>{copy.hosts.source}</th>
                <th>{copy.hosts.endpoint}</th>
                <th>{copy.hosts.status}</th>
                <th>{copy.hosts.profile}</th>
                <th>{copy.hosts.skills}</th>
                <th>{copy.hosts.lastSeen}</th>
                <th>{copy.hosts.actions}</th>
              </tr>
            </thead>
            <tbody>
              {hosts.map((host) => (
                <tr key={host.id}>
                  <td>
                    <strong>{host.name}</strong>
                    <span>{host.hostAlias} / {host.os}</span>
                  </td>
                  <td><Badge tone={host.source === "mock" ? "gray" : host.source === "managed" ? "blue" : "green"}>{host.source}</Badge></td>
                  <td>{formatEndpoint(host)}</td>
                  <td><StatusBadge copy={copy} status={host.status} /></td>
                  <td>{host.profileId ? profileById.get(host.profileId)?.name ?? host.profileId : copy.common.unassigned}</td>
                  <td>{host.skillPackIds.map((id) => skillPackById.get(id)?.name ?? id).join(", ") || "-"}</td>
                  <td>{host.lastSeen}</td>
                  <td className="tableActions">
                    <button className="miniButton" disabled={Boolean(hostBusy[host.hostAlias])} type="button" onClick={() => onTestHost(host.hostAlias)}>
                      {hostBusy[host.hostAlias] === "test" ? copy.hosts.testing : copy.hosts.test}
                    </button>
                    <button className="miniButton" disabled={Boolean(hostBusy[host.hostAlias])} type="button" onClick={() => onProbeHost(host.hostAlias)}>
                      {hostBusy[host.hostAlias] === "probe" ? copy.hosts.probing : copy.hosts.probe}
                    </button>
                    <button className="miniButton danger" type="button" onClick={() => onDeleteHost(host.id)}>{copy.hosts.delete}</button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
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

  return (
    <div className="modalBackdrop" role="presentation">
      <div className="sshHostModal" role="dialog" aria-modal="true" aria-labelledby="ssh-host-modal-title">
        <button className="modalCloseButton" type="button" onClick={closeModal} aria-label="Close">×</button>
        <div className="modalHero">
          <h2 id="ssh-host-modal-title">新增 SSH Host</h2>
        </div>

        <form className="modalForm" onSubmit={handleSubmit}>
          <label className="fieldGroup">
            <span>{copy.hosts.hostAlias}</span>
            <input disabled={connecting} value={draft.alias} onChange={(event) => updateDraft("alias", event.target.value)} placeholder="HostAlias" required />
          </label>
          <label className="fieldGroup">
            <span>Host IP</span>
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
            <input autoComplete="new-password" disabled={connecting} type="password" value={password} onChange={(event) => setPassword(event.target.value)} placeholder="Password" required />
          </label>
          <div className="fieldGroup identityRow">
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
function HostDetailsPanel({ copy, host }: { copy: UICopy; host: Host | null }) {
  return (
    <section className="panel spanWide">
      <div className="panelHeader">
        <div>
          <div className="eyebrow">{copy.hosts.details}</div>
          <h2>{host?.name ?? copy.hosts.unknown}</h2>
          <p>{copy.hosts.detailsBody}</p>
        </div>
        {host ? <StatusBadge copy={copy} status={host.status} /> : <Badge tone="gray">{copy.hosts.unknown}</Badge>}
      </div>

      <dl className="detailGrid">
        <div>
          <dt>{copy.hosts.sshStatus}</dt>
          <dd>{host ? copy.status.host[host.status] : copy.hosts.unknown}</dd>
        </div>
        <div>
          <dt>{copy.hosts.os}</dt>
          <dd>{host?.os ?? copy.hosts.unknown}</dd>
        </div>
        <div>
          <dt>{copy.hosts.arch}</dt>
          <dd>{host?.arch ?? copy.hosts.unknown}</dd>
        </div>
        <div>
          <dt>{copy.hosts.shell}</dt>
          <dd>{host?.shell ?? copy.hosts.unknown}</dd>
        </div>
        <div>
          <dt>{copy.hosts.pathLocalBin}</dt>
          <dd>{host ? formatNullableBoolean(host.pathHasLocalBin, copy) : copy.hosts.unknown}</dd>
        </div>
        <div>
          <dt>{copy.hosts.codexInstalled}</dt>
          <dd>{host ? formatBoolean(host.codexInstalled, copy) : copy.hosts.unknown}</dd>
        </div>
        <div>
          <dt>{copy.hosts.codexVersion}</dt>
          <dd>{host?.codexVersion ?? copy.hosts.unknown}</dd>
        </div>
        <div>
          <dt>{copy.hosts.configExists}</dt>
          <dd>{host ? formatNullableBoolean(host.configExists, copy) : copy.hosts.unknown}</dd>
        </div>
        <div>
          <dt>{copy.hosts.skillsCount}</dt>
          <dd>{host?.skillsCount ?? copy.hosts.unknown}</dd>
        </div>
      </dl>
    </section>
  );
}

function ProfilesView({ copy, hosts, profiles, onApplyProfile }: { copy: UICopy; hosts: Host[]; profiles: Profile[]; onApplyProfile: (profileId: string) => void }) {
  return (
    <div className="cardGrid">
      {profiles.map((profile) => {
        const assignedHosts = hosts.filter((host) => host.profileId === profile.id);

        return (
          <article className="panel profileCard" key={profile.id}>
            <div className="panelHeader compact">
              <div>
                <div className="eyebrow">{profile.model}</div>
                <h2>{profile.name}</h2>
              </div>
              <Badge tone="blue">{assignedHosts.length} {copy.profiles.hosts}</Badge>
            </div>
            <p>{profile.description}</p>
            <dl className="settingsList">
              <div>
                <dt>{copy.profiles.approval}</dt>
                <dd>{profile.approvalPolicy}</dd>
              </div>
              <div>
                <dt>{copy.profiles.sandbox}</dt>
                <dd>{profile.sandboxMode}</dd>
              </div>
              <div>
                <dt>{copy.profiles.updated}</dt>
                <dd>{profile.updatedAt}</dd>
              </div>
            </dl>
            <button className="secondaryButton fullWidth" type="button" onClick={() => onApplyProfile(profile.id)}>{copy.profiles.applyOnline}</button>
          </article>
        );
      })}
    </div>
  );
}

function SkillsView({ copy, skillPacks }: { copy: UICopy; skillPacks: SkillPack[] }) {
  return (
    <div className="cardGrid">
      {skillPacks.map((pack) => (
        <article className="panel skillCard" key={pack.id}>
          <div className="panelHeader compact">
            <div>
              <div className="eyebrow">v{pack.version}</div>
              <h2>{pack.name}</h2>
            </div>
            <Badge tone={pack.enabled ? "green" : "gray"}>{pack.enabled ? copy.skills.enabled : copy.skills.disabled}</Badge>
          </div>
          <p>{pack.description}</p>
          <dl className="settingsList">
            <div>
              <dt>{copy.skills.source}</dt>
              <dd>{pack.source}</dd>
            </div>
            <div>
              <dt>{copy.skills.skills}</dt>
              <dd>{pack.skillCount}</dd>
            </div>
            <div>
              <dt>{copy.skills.updated}</dt>
              <dd>{pack.updatedAt}</dd>
            </div>
          </dl>
        </article>
      ))}
    </div>
  );
}

function TasksView({ copy, tasks }: { copy: UICopy; tasks: TaskRun[] }) {
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(tasks[0]?.id ?? null);
  const selectedTask = tasks.find((task) => task.id === selectedTaskId) ?? tasks[0];

  useEffect(() => {
    if (!selectedTaskId && tasks[0]) setSelectedTaskId(tasks[0].id);
  }, [selectedTaskId, tasks]);

  return (
    <div className="tasksGrid">
      <section className="panel spanWide">
        <div className="panelHeader">
          <div>
            <div className="eyebrow">{copy.tasks.runs}</div>
            <h2>{copy.tasks.taskHistory}</h2>
            <p>{copy.tasks.body}</p>
          </div>
        </div>
        <div className="tableWrap">
          <table>
            <thead>
              <tr>
                <th>{copy.tasks.action}</th>
                <th>{copy.tasks.host}</th>
                <th>{copy.tasks.status}</th>
                <th>{copy.tasks.started}</th>
                <th>{copy.tasks.summary}</th>
              </tr>
            </thead>
            <tbody>
              {tasks.map((task) => (
                <tr className="selectableRow" data-selected={selectedTask?.id === task.id} key={task.id} onClick={() => setSelectedTaskId(task.id)}>
                  <td><strong>{localizeTaskAction(task.action, copy)}</strong></td>
                  <td>{task.hostName}</td>
                  <td><TaskStatusBadge copy={copy} status={task.status} /></td>
                  <td>{task.startedAt}</td>
                  <td>{task.summary}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      <section className="panel logPanel">
        <div className="panelHeader compact">
          <div>
            <div className="eyebrow">{copy.tasks.taskLog}</div>
            <h2>{selectedTask?.id ?? copy.tasks.noTask}</h2>
          </div>
        </div>
        <div className="logList">
          {selectedTask?.logs.map((log) => (
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
          )) ?? <p className="mutedText">{copy.tasks.noLogs}</p>}
        </div>
      </section>
    </div>
  );
}

function SettingsView({
  copy,
  health,
  settings,
  sshBusy,
  sshStatus,
  onCopyPublicKey,
  onFontPresetChange,
  onGenerateEd25519Key,
  onRefreshSsh,
  onThemeChange
}: {
  copy: UICopy;
  health: Health;
  settings: AppSettings;
  sshBusy: boolean;
  sshStatus: SshStatus | null;
  onCopyPublicKey: (publicKey: string) => Promise<void>;
  onFontPresetChange: (fontPreset: FontPreset) => void;
  onGenerateEd25519Key: () => Promise<void>;
  onRefreshSsh: () => Promise<void>;
  onThemeChange: (theme: ThemeChoice) => void;
}) {
  const commands = [
    "get_ssh_status",
    "generate_ed25519_key",
    "list_ssh_config_hosts",
    "upsert_ssh_config_host",
    "delete_ssh_config_host",
    "list_hosts",
    "refresh_discovered_hosts",
    "add_host",
    "update_host",
    "delete_host",
    "test_ssh_connection",
    "ssh_check",
    "bootstrap_ssh_host",
    "bootstrap_existing_ssh_host",
    "remote_probe_codex",
    "list_profiles",
    "apply_profile",
    "list_tasks"
  ];
  const publicKey = sshStatus?.ed25519.publicKey ?? sshStatus?.rsa.publicKey ?? "";
  const canGenerateEd25519 = Boolean(sshStatus?.sshKeygenAvailable && !sshStatus.ed25519.privateExists && !sshStatus.ed25519.publicExists);

  return (
    <div className="settingsGrid">
      <section className="panel">
        <div className="panelHeader compact">
          <div>
            <div className="eyebrow">{copy.settings.appearance}</div>
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

          <label className="settingSelectRow">
            <span>{copy.settings.font}</span>
            <select value={settings.fontPreset} onChange={(event) => onFontPresetChange(event.target.value as FontPreset)}>
              {(Object.keys(fontPresets) as FontPreset[]).map((preset) => (
                <option key={preset} value={preset}>
                  {fontPresets[preset].label}
                </option>
              ))}
            </select>
          </label>
        </div>
      </section>

      <section className="panel">
        <div className="panelHeader compact">
          <div>
            <div className="eyebrow">{copy.settings.runtime}</div>
            <h2>{copy.settings.backend}</h2>
          </div>
          <Badge tone={health.mode === "tauri" ? "green" : "gray"}>{health.mode}</Badge>
        </div>
        <dl className="settingsList">
          <div>
            <dt>{copy.settings.app}</dt>
            <dd>{health.app}</dd>
          </div>
          <div>
            <dt>{copy.settings.remoteWrapper}</dt>
            <dd>{health.remoteWrapperRequired ? copy.common.required : copy.common.notRequired}</dd>
          </div>
          <div>
            <dt>{copy.settings.sshConfig}</dt>
            <dd>{sshStatus?.configPath ?? copy.settings.desktopBackendRequired}</dd>
          </div>
        </dl>
      </section>

      <section className="panel spanWide">
        <div className="panelHeader">
          <div>
            <div className="eyebrow">{copy.settings.localSsh}</div>
            <h2>{copy.settings.sshKeyStatus}</h2>
            <p>{copy.settings.sshKeyBody}</p>
          </div>
          <div className="topActions">
            <button className="secondaryButton" type="button" onClick={() => void onRefreshSsh()}>{copy.settings.refresh}</button>
            <button className="primaryButton" disabled={!canGenerateEd25519 || sshBusy} type="button" onClick={() => void onGenerateEd25519Key()}>
              {sshBusy ? copy.settings.generating : copy.settings.generateEd25519}
            </button>
          </div>
        </div>

        <div className="keyStatusGrid">
          <KeyStatusCard copy={copy} keyInfo={sshStatus?.ed25519} title="Ed25519" />
          <KeyStatusCard copy={copy} keyInfo={sshStatus?.rsa} title="RSA" />
        </div>

        <div className="publicKeyBox">
          <div className="panelHeader compact">
            <div>
              <div className="eyebrow">{copy.settings.publicKey}</div>
              <h2>{publicKey ? copy.settings.readyToCopy : copy.settings.noPublicKey}</h2>
            </div>
            <button className="secondaryButton" disabled={!publicKey} type="button" onClick={() => void onCopyPublicKey(publicKey)}>
              {copy.settings.copyPublicKey}
            </button>
          </div>
          <pre>{publicKey || copy.settings.publicKeyEmpty}</pre>
        </div>
      </section>

      <section className="panel spanWide">
        <div className="panelHeader compact">
          <div>
            <div className="eyebrow">{copy.settings.commandReservations}</div>
            <h2>{copy.settings.commandSurface}</h2>
          </div>
        </div>
        <div className="commandGrid">
          {commands.map((command) => <code key={command}>{command}</code>)}
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
      <dl className="settingsList">
        <div>
          <dt>{copy.settings.privatePath}</dt>
          <dd>{keyInfo?.privatePath ?? copy.settings.unknown}</dd>
        </div>
        <div>
          <dt>{copy.settings.publicPath}</dt>
          <dd>{keyInfo?.publicPath ?? copy.settings.unknown}</dd>
        </div>
        <div>
          <dt>{copy.settings.publicKey}</dt>
          <dd>{keyInfo?.publicExists ? copy.settings.available : copy.settings.missing}</dd>
        </div>
      </dl>
    </article>
  );
}

function Badge({ children, tone }: { children: ReactNode; tone: "green" | "yellow" | "red" | "blue" | "gray" }) {
  return <span className="badge" data-tone={tone}>{children}</span>;
}

function StatusBadge({ copy, status }: { copy: UICopy; status: HostStatus }) {
  const tone = status === "online" ? "green" : status === "offline" ? "red" : status === "testing" ? "yellow" : "gray";
  return <Badge tone={tone}>{copy.status.host[status]}</Badge>;
}

function TaskStatusBadge({ copy, status }: { copy: UICopy; status: TaskStatus }) {
  const tone = status === "success" ? "green" : status === "failed" ? "red" : status === "running" ? "yellow" : "gray";
  return <Badge tone={tone}>{copy.status.task[status]}</Badge>;
}

function localizeTaskAction(action: string, copy: UICopy) {
  const labels = copy.tasks.actionLabels as Record<string, string>;
  return labels[action] ?? action;
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
