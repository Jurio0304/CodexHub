import { api } from "../api";
import type {
  AppUpdateStatus,
  Health,
  Host,
  Profile,
  SkillInventoryStatus,
  SkillPack,
  TaskRun
} from "../models";
import type { AppSettings } from "../settings";

export type InitialAppData = {
  settings: AppSettings;
  health: Health;
  appUpdateStatus: AppUpdateStatus;
  hosts: Host[];
  profiles: Profile[];
  skillPacks: SkillPack[];
  skillInventoryStatus: SkillInventoryStatus;
  tasks: TaskRun[];
  taskNextCursor: string | null;
  unacknowledgedTaskIds: string[];
};

export async function loadInitialAppData(): Promise<InitialAppData> {
  const [settings, health, appUpdateStatus, hosts, profiles, skillPacks, skillInventoryStatus, taskPage] = await Promise.all([
    api.getSettings(),
    api.getHealth(),
    api.getAppUpdateStatus(),
    api.listHosts(),
    api.listProfiles(),
    api.listSkillPacks(),
    api.getSkillInventoryStatus(),
    api.queryTasks({ limit: 100, cursor: null })
  ]);

  return {
    settings,
    health,
    appUpdateStatus,
    hosts,
    profiles,
    skillPacks,
    skillInventoryStatus,
    tasks: taskPage.items,
    taskNextCursor: taskPage.nextCursor,
    unacknowledgedTaskIds: taskPage.unacknowledgedTaskIds
  };
}
