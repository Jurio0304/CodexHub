import { desktopApi } from "./desktop";
import { mockApi } from "./mock";
import { apiMode } from "./runtime";

export const api = apiMode === "mock" ? mockApi : desktopApi;
export { apiMode } from "./runtime";
export type { ApiMode } from "./runtime";
export type {
  CodexHubApi,
  HostOperationProgressHandler,
  RemoteCodexProgressHandler,
  SshBootstrapProgressHandler,
  TaskUpdatedHandler
} from "./contracts";
export {
  fallbackAppUpdateStatus,
  fallbackConnection,
  fallbackHealth,
  fallbackHosts,
  fallbackLatestCodexVersion,
  fallbackLocalCodexStatus,
  fallbackNetworkProxyStatus,
  fallbackProfiles,
  fallbackSkillPacks,
  fallbackSshConfigHosts,
  fallbackSshStatus,
  fallbackTasks
} from "./fallbacks";
export { DesktopCommandError, formatInvokeError, hasTauriRuntime, parseApiError, requiredInvoke } from "./invoke";
