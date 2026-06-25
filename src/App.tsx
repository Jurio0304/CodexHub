import { useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { api, fallbackHealth } from "./api";
import type {
  Health,
  Host,
  HostStatus,
  Profile,
  SkillPack,
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

const navItems: Array<{ id: SectionId; label: string; description: string }> = [
  { id: "dashboard", label: "Dashboard", description: "Overview" },
  { id: "hosts", label: "Hosts", description: "SSH targets" },
  { id: "profiles", label: "Profiles", description: "Codex TOML" },
  { id: "skills", label: "Skills", description: "Skill packs" },
  { id: "tasks", label: "Tasks", description: "Runs and logs" },
  { id: "settings", label: "Settings", description: "App options" }
];

const sectionCopy: Record<SectionId, { title: string; eyebrow: string; body: string }> = {
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
};

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
  const [notice, setNotice] = useState("Local SSH key and config management is ready in the desktop backend.");

  const refreshSshState = async () => {
    const [nextSshStatus, nextSshConfigHosts] = await Promise.all([api.getSshStatus(), api.listSshConfigHosts()]);
    setSshStatus(nextSshStatus);
    setSshConfigHosts(nextSshConfigHosts);
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

  const selectedCopy = sectionCopy[activeSection];
  const onlineCount = hosts.filter((host) => host.status === "online").length;
  const activeTasks = tasks.filter((task) => task.status === "queued" || task.status === "running").length;

  const profileById = useMemo(() => new Map(profiles.map((profile) => [profile.id, profile])), [profiles]);
  const skillPackById = useMemo(() => new Map(skillPacks.map((pack) => [pack.id, pack])), [skillPacks]);

  const handleAddHost = () => {
    setActiveSection("hosts");
    setNotice("Fill in the SSH config form. CodexHub will create or update one managed Host block with a backup first.");
  };

  const handleDeleteHost = async (id: string) => {
    const host = hosts.find((item) => item.id === id);
    const removed = await api.deleteHost(id);
    if (!removed) return;
    setHosts((current) => current.filter((item) => item.id !== id));
    setNotice(`${host?.name ?? "Host"} was removed from the mock inventory only.`);
  };

  const handleSaveSshConfigHost = async (draft: SshHostDraft) => {
    const result = await api.upsertSshConfigHost(draft);
    await refreshSshState();
    setNotice(result.backupPath ? `${result.message} Backup: ${result.backupPath}` : result.message);
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
      setNotice("Public key copied to clipboard.");
    } catch {
      setNotice("Could not copy automatically. Select the public key text and copy it manually.");
    }
  };

  const handleTestHost = async (id: string) => {
    const target = hosts.find((host) => host.id === id);
    setHosts((current) => current.map((host) => (host.id === id ? { ...host, status: "testing" } : host)));

    const result = await api.testSshConnection(id);
    setHosts((current) =>
      current.map((host) =>
        host.id === id
          ? {
              ...host,
              status: result.ok ? "online" : "offline",
              latencyMs: result.latencyMs,
              lastSeen: result.ok ? "just now" : host.lastSeen
            }
          : host
      )
    );
    setNotice(`${target?.name ?? "Host"}: ${result.message}`);
  };

  const handleApplyProfile = async (profileId: string) => {
    const targetHostIds = hosts.filter((host) => host.status === "online").map((host) => host.id);
    const hostIds = targetHostIds.length > 0 ? targetHostIds : hosts.slice(0, 1).map((host) => host.id);
    if (hostIds.length === 0) {
      setNotice("Add a host before applying a profile.");
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
            health={health}
            hosts={hosts}
            loading={loading}
            notice={notice}
            onlineCount={onlineCount}
            profiles={profiles}
            skillPacks={skillPacks}
            tasks={tasks}
            profileById={profileById}
            skillPackById={skillPackById}
            onAddHost={handleAddHost}
            onSelectSection={setActiveSection}
            onTestHost={handleTestHost}
          />
        );
      case "hosts":
        return (
          <HostsView
            hosts={hosts}
            profileById={profileById}
            skillPackById={skillPackById}
            sshConfigHosts={sshConfigHosts}
            sshStatus={sshStatus}
            onAddHost={handleAddHost}
            onDeleteHost={handleDeleteHost}
            onDeleteSshConfigHost={handleDeleteSshConfigHost}
            onSaveSshConfigHost={handleSaveSshConfigHost}
            onTestHost={handleTestHost}
          />
        );
      case "profiles":
        return <ProfilesView hosts={hosts} profiles={profiles} onApplyProfile={handleApplyProfile} />;
      case "skills":
        return <SkillsView skillPacks={skillPacks} />;
      case "tasks":
        return <TasksView tasks={tasks} />;
      case "settings":
        return (
          <SettingsView
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
      <aside className="sidebar" aria-label="Primary navigation">
        <div className="brandBlock">
          <div className="appIcon" aria-hidden="true">CH</div>
          <div>
            <div className="brandName">CodexHub</div>
            <div className="brandSubtle">Desktop MVP</div>
          </div>
        </div>

        <nav className="navList">
          {navItems.map((item) => (
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
            <span>backend mode</span>
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
            <button className="primaryButton" type="button" onClick={handleAddHost}>Add Server</button>
          </div>
        </header>

        {renderContent()}
      </main>
    </div>
  );
}

function DashboardView({
  activeTasks,
  health,
  hosts,
  loading,
  notice,
  onlineCount,
  profiles,
  profileById,
  skillPackById,
  skillPacks,
  tasks,
  onAddHost,
  onSelectSection,
  onTestHost
}: {
  activeTasks: number;
  health: Health;
  hosts: Host[];
  loading: boolean;
  notice: string;
  onlineCount: number;
  profiles: Profile[];
  profileById: Map<string, Profile>;
  skillPackById: Map<string, SkillPack>;
  skillPacks: SkillPack[];
  tasks: TaskRun[];
  onAddHost: () => void;
  onSelectSection: (section: SectionId) => void;
  onTestHost: (id: string) => void;
}) {
  return (
    <div className="pageGrid">
      <section className="summaryStrip" aria-label="Dashboard summary">
        <MetricCard label="Hosts" value={String(hosts.length)} detail={`${onlineCount} online`} />
        <MetricCard label="Profiles" value={String(profiles.length)} detail="mock presets" />
        <MetricCard label="Skills" value={String(skillPacks.length)} detail={`${skillPacks.filter((pack) => pack.enabled).length} enabled`} />
        <MetricCard label="Tasks" value={String(tasks.length)} detail={`${activeTasks} active`} />
      </section>

      <section className="panel calloutPanel">
        <div>
          <div className="eyebrow">Backend contract</div>
          <h2>SSH management wired</h2>
          <p>{notice}</p>
        </div>
        <div className="calloutMeta">
          <Badge tone={health.remoteWrapperRequired ? "yellow" : "green"}>wrapper {health.remoteWrapperRequired ? "required" : "not required"}</Badge>
          <Badge tone={loading ? "yellow" : "green"}>{loading ? "loading" : "ready"}</Badge>
        </div>
      </section>

      <ServerMatrix hosts={hosts} profileById={profileById} skillPackById={skillPackById} onAddHost={onAddHost} onTestHost={onTestHost} />
      <RecentTasks tasks={tasks} onViewAll={() => onSelectSection("tasks")} />
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
  hosts,
  profileById,
  skillPackById,
  onAddHost,
  onTestHost
}: {
  hosts: Host[];
  profileById: Map<string, Profile>;
  skillPackById: Map<string, SkillPack>;
  onAddHost: () => void;
  onTestHost: (id: string) => void;
}) {
  return (
    <section className="panel spanWide">
      <div className="panelHeader">
        <div>
          <div className="eyebrow">Server Matrix</div>
          <h2>Mock hosts</h2>
          <p>Inventory cards show the shape of the future SSH connection matrix.</p>
        </div>
        <button className="secondaryButton" type="button" onClick={onAddHost}>Add Server</button>
      </div>

      {hosts.length === 0 ? (
        <div className="emptyState">
          <div className="emptyIcon" aria-hidden="true" />
          <h3>No hosts yet</h3>
          <p>Add the first SSH target to populate the server matrix.</p>
          <button className="primaryButton" type="button" onClick={onAddHost}>Add Server</button>
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
                <StatusBadge status={host.status} />
              </div>

              <dl className="hostMeta">
                <div>
                  <dt>OS</dt>
                  <dd>{host.os}</dd>
                </div>
                <div>
                  <dt>Codex</dt>
                  <dd>{host.codexVersion}</dd>
                </div>
                <div>
                  <dt>Profile</dt>
                  <dd>{host.profileId ? profileById.get(host.profileId)?.name ?? host.profileId : "Unassigned"}</dd>
                </div>
                <div>
                  <dt>Latency</dt>
                  <dd>{host.latencyMs ? `${host.latencyMs} ms` : "-"}</dd>
                </div>
              </dl>

              <div className="tagRow" aria-label={`${host.name} tags`}>
                {host.tags.map((tag) => <span key={tag}>{tag}</span>)}
              </div>

              <div className="skillLine">
                {host.skillPackIds.length > 0 ? host.skillPackIds.map((id) => skillPackById.get(id)?.name ?? id).join(", ") : "No skill packs"}
              </div>

              <button className="tertiaryButton" type="button" onClick={() => onTestHost(host.id)}>Test SSH</button>
            </article>
          ))}
        </div>
      )}
    </section>
  );
}

function RecentTasks({ tasks, onViewAll }: { tasks: TaskRun[]; onViewAll: () => void }) {
  return (
    <section className="panel">
      <div className="panelHeader compact">
        <div>
          <div className="eyebrow">Recent tasks</div>
          <h2>Activity</h2>
        </div>
        <button className="linkButton" type="button" onClick={onViewAll}>View all</button>
      </div>

      <div className="taskList">
        {tasks.slice(0, 4).map((task) => (
          <article className="taskItem" key={task.id}>
            <div>
              <strong>{task.action}</strong>
              <span>{task.hostName}</span>
            </div>
            <TaskStatusBadge status={task.status} />
          </article>
        ))}
      </div>
    </section>
  );
}

function HostsView({
  hosts,
  profileById,
  skillPackById,
  sshConfigHosts,
  sshStatus,
  onAddHost,
  onDeleteHost,
  onDeleteSshConfigHost,
  onSaveSshConfigHost,
  onTestHost
}: {
  hosts: Host[];
  profileById: Map<string, Profile>;
  skillPackById: Map<string, SkillPack>;
  sshConfigHosts: SshConfigHost[];
  sshStatus: SshStatus | null;
  onAddHost: () => void;
  onDeleteHost: (id: string) => void;
  onDeleteSshConfigHost: (alias: string) => Promise<void>;
  onSaveSshConfigHost: (draft: SshHostDraft) => Promise<void>;
  onTestHost: (id: string) => void;
}) {
  const defaultIdentityFile = sshStatus?.preferredIdentityFile ?? "%USERPROFILE%\\.ssh\\id_ed25519";
  const [draft, setDraft] = useState<SshHostDraft>(() => emptySshHostDraft(defaultIdentityFile));
  const [saving, setSaving] = useState(false);
  const [formMessage, setFormMessage] = useState("CodexHub writes only managed Host blocks and backs up existing config first.");

  useEffect(() => {
    setDraft((current) => (current.identityFile ? current : { ...current, identityFile: defaultIdentityFile }));
  }, [defaultIdentityFile]);

  const updateDraft = (key: keyof SshHostDraft, value: string | number) => {
    setDraft((current) => ({ ...current, [key]: value }));
  };

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setSaving(true);
    setFormMessage("Writing SSH config...");
    try {
      await onSaveSshConfigHost(draft);
      setFormMessage(`Saved Host ${draft.alias}.`);
      setDraft(emptySshHostDraft(defaultIdentityFile));
    } catch (error) {
      setFormMessage(formatError(error));
    } finally {
      setSaving(false);
    }
  };

  const handleEdit = (host: SshConfigHost) => {
    setDraft({
      alias: host.alias,
      hostName: host.hostName,
      port: host.port,
      user: host.user,
      identityFile: host.identityFile
    });
    setFormMessage(`Editing managed Host ${host.alias}. Submit with the same alias to update it in place.`);
  };

  const handleDelete = async (alias: string) => {
    const confirmed = window.confirm(`Delete CodexHub-managed Host ${alias} from SSH config?`);
    if (!confirmed) return;
    setSaving(true);
    try {
      await onDeleteSshConfigHost(alias);
      setFormMessage(`Deleted Host ${alias}.`);
    } catch (error) {
      setFormMessage(formatError(error));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="hostsGrid">
      <section className="panel spanWide">
        <div className="panelHeader">
          <div>
            <div className="eyebrow">SSH config manager</div>
            <h2>Add server</h2>
            <p>Writes to {sshStatus?.configPath ?? "%USERPROFILE%\\.ssh\\config"}. User-owned blocks are preserved.</p>
          </div>
          <Badge tone={sshStatus ? "green" : "gray"}>{sshStatus ? "local paths loaded" : "web preview"}</Badge>
        </div>

        <form className="sshForm" onSubmit={handleSubmit}>
          <label className="fieldGroup compactField">
            <span>Host Alias</span>
            <input value={draft.alias} onChange={(event) => updateDraft("alias", event.target.value)} placeholder="lab-box" required />
          </label>
          <label className="fieldGroup compactField">
            <span>HostName</span>
            <input value={draft.hostName} onChange={(event) => updateDraft("hostName", event.target.value)} placeholder="192.168.31.50" required />
          </label>
          <label className="fieldGroup compactField">
            <span>Port</span>
            <input min={1} max={65535} type="number" value={draft.port} onChange={(event) => updateDraft("port", Number(event.target.value))} required />
          </label>
          <label className="fieldGroup compactField">
            <span>User</span>
            <input value={draft.user} onChange={(event) => updateDraft("user", event.target.value)} placeholder="codex" required />
          </label>
          <label className="fieldGroup compactField identityField">
            <span>IdentityFile</span>
            <input value={draft.identityFile} onChange={(event) => updateDraft("identityFile", event.target.value)} required />
          </label>
          <div className="formActions">
            <button className="primaryButton" disabled={saving} type="submit">{saving ? "Saving..." : "Write SSH Config"}</button>
            <button className="secondaryButton" type="button" onClick={() => setDraft(emptySshHostDraft(defaultIdentityFile))}>Reset</button>
          </div>
        </form>
        <p className="mutedText">{formMessage}</p>
      </section>

      <section className="panel spanWide">
        <div className="panelHeader">
          <div>
            <div className="eyebrow">CodexHub managed</div>
            <h2>SSH Host blocks</h2>
            <p>Repeated saves update the same alias instead of appending duplicates.</p>
          </div>
          <button className="secondaryButton" type="button" onClick={onAddHost}>New Host</button>
        </div>

        {sshConfigHosts.length === 0 ? (
          <div className="emptyState">
            <div className="emptyIcon" aria-hidden="true" />
            <h3>No managed SSH hosts</h3>
            <p>Add a server above to create the first CodexHub-managed block in SSH config.</p>
          </div>
        ) : (
          <div className="tableWrap">
            <table>
              <thead>
                <tr>
                  <th>Alias</th>
                  <th>HostName</th>
                  <th>Port</th>
                  <th>User</th>
                  <th>IdentityFile</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                {sshConfigHosts.map((host) => (
                  <tr key={host.alias}>
                    <td><strong>{host.alias}</strong></td>
                    <td>{host.hostName}</td>
                    <td>{host.port}</td>
                    <td>{host.user}</td>
                    <td><code>{host.identityFile}</code></td>
                    <td className="tableActions">
                      <button className="miniButton" type="button" onClick={() => handleEdit(host)}>Edit</button>
                      <button className="miniButton danger" type="button" onClick={() => handleDelete(host.alias)}>Delete</button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <section className="panel spanWide">
        <div className="panelHeader">
          <div>
            <div className="eyebrow">Mock inventory</div>
            <h2>Existing app hosts</h2>
            <p>These rows still exercise list_hosts, delete_host, and test_ssh_connection while real config management lands.</p>
          </div>
        </div>

        <div className="tableWrap">
          <table>
            <thead>
              <tr>
                <th>Name</th>
                <th>Endpoint</th>
                <th>Status</th>
                <th>Profile</th>
                <th>Skills</th>
                <th>Last seen</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {hosts.map((host) => (
                <tr key={host.id}>
                  <td>
                    <strong>{host.name}</strong>
                    <span>{host.os}</span>
                  </td>
                  <td>{formatEndpoint(host)}</td>
                  <td><StatusBadge status={host.status} /></td>
                  <td>{host.profileId ? profileById.get(host.profileId)?.name ?? host.profileId : "Unassigned"}</td>
                  <td>{host.skillPackIds.map((id) => skillPackById.get(id)?.name ?? id).join(", ") || "-"}</td>
                  <td>{host.lastSeen}</td>
                  <td className="tableActions">
                    <button className="miniButton" type="button" onClick={() => onTestHost(host.id)}>Test</button>
                    <button className="miniButton danger" type="button" onClick={() => onDeleteHost(host.id)}>Delete</button>
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

function ProfilesView({ hosts, profiles, onApplyProfile }: { hosts: Host[]; profiles: Profile[]; onApplyProfile: (profileId: string) => void }) {
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
              <Badge tone="blue">{assignedHosts.length} hosts</Badge>
            </div>
            <p>{profile.description}</p>
            <dl className="settingsList">
              <div>
                <dt>Approval</dt>
                <dd>{profile.approvalPolicy}</dd>
              </div>
              <div>
                <dt>Sandbox</dt>
                <dd>{profile.sandboxMode}</dd>
              </div>
              <div>
                <dt>Updated</dt>
                <dd>{profile.updatedAt}</dd>
              </div>
            </dl>
            <button className="secondaryButton fullWidth" type="button" onClick={() => onApplyProfile(profile.id)}>Apply to online hosts</button>
          </article>
        );
      })}
    </div>
  );
}

function SkillsView({ skillPacks }: { skillPacks: SkillPack[] }) {
  return (
    <div className="cardGrid">
      {skillPacks.map((pack) => (
        <article className="panel skillCard" key={pack.id}>
          <div className="panelHeader compact">
            <div>
              <div className="eyebrow">v{pack.version}</div>
              <h2>{pack.name}</h2>
            </div>
            <Badge tone={pack.enabled ? "green" : "gray"}>{pack.enabled ? "enabled" : "disabled"}</Badge>
          </div>
          <p>{pack.description}</p>
          <dl className="settingsList">
            <div>
              <dt>Source</dt>
              <dd>{pack.source}</dd>
            </div>
            <div>
              <dt>Skills</dt>
              <dd>{pack.skillCount}</dd>
            </div>
            <div>
              <dt>Updated</dt>
              <dd>{pack.updatedAt}</dd>
            </div>
          </dl>
        </article>
      ))}
    </div>
  );
}

function TasksView({ tasks }: { tasks: TaskRun[] }) {
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
            <div className="eyebrow">Runs</div>
            <h2>Task history</h2>
            <p>Mock TaskRun rows demonstrate the local task model and future worker queue.</p>
          </div>
        </div>
        <div className="tableWrap">
          <table>
            <thead>
              <tr>
                <th>Action</th>
                <th>Host</th>
                <th>Status</th>
                <th>Started</th>
                <th>Summary</th>
              </tr>
            </thead>
            <tbody>
              {tasks.map((task) => (
                <tr className="selectableRow" data-selected={selectedTask?.id === task.id} key={task.id} onClick={() => setSelectedTaskId(task.id)}>
                  <td><strong>{task.action}</strong></td>
                  <td>{task.hostName}</td>
                  <td><TaskStatusBadge status={task.status} /></td>
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
            <div className="eyebrow">TaskLog</div>
            <h2>{selectedTask?.id ?? "No task"}</h2>
          </div>
        </div>
        <div className="logList">
          {selectedTask?.logs.map((log) => (
            <article className="logLine" data-level={log.level} key={log.id}>
              <span>{log.timestamp}</span>
              <strong>{log.level}</strong>
              <p>{log.message}</p>
            </article>
          )) ?? <p className="mutedText">No logs yet.</p>}
        </div>
      </section>
    </div>
  );
}

function SettingsView({
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
    "add_host",
    "update_host",
    "delete_host",
    "test_ssh_connection",
    "list_profiles",
    "apply_profile",
    "list_tasks"
  ];
  const selectedFontPreset = fontPresets[settings.fontPreset] ?? fontPresets.system;
  const publicKey = sshStatus?.ed25519.publicKey ?? sshStatus?.rsa.publicKey ?? "";
  const canGenerateEd25519 = Boolean(sshStatus?.sshKeygenAvailable && !sshStatus.ed25519.privateExists && !sshStatus.ed25519.publicExists);

  return (
    <div className="settingsGrid">
      <section className="panel">
        <div className="panelHeader compact">
          <div>
            <div className="eyebrow">Appearance</div>
            <h2>Theme</h2>
          </div>
        </div>
        <div className="segmentedControl" role="group" aria-label="Theme">
          {(["system", "light", "dark"] as ThemeChoice[]).map((choice) => (
            <button data-active={settings.theme === choice} key={choice} onClick={() => onThemeChange(choice)} type="button">
              {choice}
            </button>
          ))}
        </div>
        <p className="mutedText">System follows the operating system color scheme. The app uses native-feeling cards and badges in both modes.</p>

        <label className="fieldGroup">
          <span>Font Family</span>
          <select aria-describedby="fontPresetDescription" value={settings.fontPreset} onChange={(event) => onFontPresetChange(event.target.value as FontPreset)}>
            {(Object.keys(fontPresets) as FontPreset[]).map((preset) => (
              <option key={preset} value={preset}>
                {fontPresets[preset].label}
              </option>
            ))}
          </select>
        </label>
        <p className="mutedText" id="fontPresetDescription">{selectedFontPreset.description}</p>
      </section>

      <section className="panel">
        <div className="panelHeader compact">
          <div>
            <div className="eyebrow">Runtime</div>
            <h2>Backend</h2>
          </div>
          <Badge tone={health.mode === "tauri" ? "green" : "gray"}>{health.mode}</Badge>
        </div>
        <dl className="settingsList">
          <div>
            <dt>App</dt>
            <dd>{health.app}</dd>
          </div>
          <div>
            <dt>Remote wrapper</dt>
            <dd>{health.remoteWrapperRequired ? "required" : "not required"}</dd>
          </div>
          <div>
            <dt>SSH config</dt>
            <dd>{sshStatus?.configPath ?? "desktop backend required"}</dd>
          </div>
        </dl>
      </section>

      <section className="panel spanWide">
        <div className="panelHeader">
          <div>
            <div className="eyebrow">Local SSH</div>
            <h2>SSH key status</h2>
            <p>Private key files are checked by existence only. CodexHub never reads or displays private key content.</p>
          </div>
          <div className="topActions">
            <button className="secondaryButton" type="button" onClick={() => void onRefreshSsh()}>Refresh</button>
            <button className="primaryButton" disabled={!canGenerateEd25519 || sshBusy} type="button" onClick={() => void onGenerateEd25519Key()}>
              {sshBusy ? "Generating..." : "Generate Ed25519"}
            </button>
          </div>
        </div>

        <div className="keyStatusGrid">
          <KeyStatusCard keyInfo={sshStatus?.ed25519} title="Ed25519" />
          <KeyStatusCard keyInfo={sshStatus?.rsa} title="RSA" />
        </div>

        <div className="publicKeyBox">
          <div className="panelHeader compact">
            <div>
              <div className="eyebrow">Public key</div>
              <h2>{publicKey ? "Ready to copy" : "No public key detected"}</h2>
            </div>
            <button className="secondaryButton" disabled={!publicKey} type="button" onClick={() => void onCopyPublicKey(publicKey)}>
              Copy Public Key
            </button>
          </div>
          <pre>{publicKey || "Generate or add an SSH public key to show it here."}</pre>
        </div>
      </section>

      <section className="panel spanWide">
        <div className="panelHeader compact">
          <div>
            <div className="eyebrow">Command reservations</div>
            <h2>Tauri command surface</h2>
          </div>
        </div>
        <div className="commandGrid">
          {commands.map((command) => <code key={command}>{command}</code>)}
        </div>
      </section>
    </div>
  );
}

function KeyStatusCard({ keyInfo, title }: { keyInfo: SshKeyInfo | undefined; title: string }) {
  return (
    <article className="keyStatusCard">
      <div className="hostHeader">
        <h3>{title}</h3>
        <Badge tone={keyInfo?.privateExists ? "green" : "gray"}>{keyInfo?.privateExists ? "private found" : "missing"}</Badge>
      </div>
      <dl className="settingsList">
        <div>
          <dt>Private path</dt>
          <dd>{keyInfo?.privatePath ?? "unknown"}</dd>
        </div>
        <div>
          <dt>Public path</dt>
          <dd>{keyInfo?.publicPath ?? "unknown"}</dd>
        </div>
        <div>
          <dt>Public key</dt>
          <dd>{keyInfo?.publicExists ? "available" : "missing"}</dd>
        </div>
      </dl>
    </article>
  );
}

function Badge({ children, tone }: { children: ReactNode; tone: "green" | "yellow" | "red" | "blue" | "gray" }) {
  return <span className="badge" data-tone={tone}>{children}</span>;
}

function StatusBadge({ status }: { status: HostStatus }) {
  const tone = status === "online" ? "green" : status === "offline" ? "red" : status === "testing" ? "yellow" : "gray";
  return <Badge tone={tone}>{status}</Badge>;
}

function TaskStatusBadge({ status }: { status: TaskStatus }) {
  const tone = status === "success" ? "green" : status === "failed" ? "red" : status === "running" ? "yellow" : "gray";
  return <Badge tone={tone}>{status}</Badge>;
}

function formatEndpoint(host: Host) {
  return `${host.username}@${host.address}:${host.port}`;
}

function emptySshHostDraft(identityFile: string): SshHostDraft {
  return {
    alias: "",
    hostName: "",
    port: 22,
    user: "codex",
    identityFile
  };
}

function formatError(error: unknown) {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return "Operation failed.";
}

export default App;
