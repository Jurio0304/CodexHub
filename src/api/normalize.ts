import type {
  Host,
  Profile,
  ProfileApplyBatchResult,
  ProfileApplyHostResult,
  ProfileApplyOptions,
  ProfileApplyOutcome,
  ProfileApplyPreview,
  RemoteCodexReloadMode,
  RemoteCodexReloadResult,
  RemoteCodexReloadStatus
} from "../models";
import type {
  HostDto,
  ProfileApplyBatchResultDto,
  ProfileApplyPreviewDto,
  ProfileDto
} from "../generated/rust-contracts";

function nowStamp() {
  return new Date().toISOString();
}

export function normalizeProfile(profile: Profile | ProfileDto): Profile {
  return {
    ...profile,
    provider: profile.provider || "openai",
    baseUrl: profile.baseUrl || "https://api.openai.com/v1",
    apiKeyEnvVar: profile.apiKeyEnvVar || "OPENAI_API_KEY",
    modelReasoningEffort: profile.modelReasoningEffort || "medium",
    planModeReasoningEffort: profile.planModeReasoningEffort || "high",
    serviceTier: profile.serviceTier || "auto",
    extraToml: profile.extraToml || "",
    createdAt: profile.createdAt || profile.updatedAt || nowStamp(),
    updatedAt: profile.updatedAt || nowStamp(),
    source: profile.source || "imported",
    credentialStored: Boolean(profile.credentialStored),
    hostIds: profile.hostIds ?? []
  };
}

export function normalizeHost(host: Host | HostDto): Host {
  return { ...host };
}

const profileApplyStatuses = new Set(["pending", "success", "failed", "no-change"] as const);
const profileApplyOutcomes = new Set<ProfileApplyOutcome>(["success", "partial", "manual-reconnect", "failed"]);
const remoteCodexReloadModes = new Set<RemoteCodexReloadMode>(["none", "app-services", "all-codex"]);
const remoteCodexReloadStatuses = new Set<RemoteCodexReloadStatus>([
  "not-requested",
  "skipped",
  "not-running",
  "reloaded",
  "reconnected",
  "manual-required",
  "failed"
]);

export function normalizeProfileApplyOptions(value: unknown): ProfileApplyOptions {
  const options = value && typeof value === "object" ? value as Partial<ProfileApplyOptions> : {};
  return {
    remoteCodexReloadMode: remoteCodexReloadModes.has(options.remoteCodexReloadMode as RemoteCodexReloadMode)
      ? options.remoteCodexReloadMode as RemoteCodexReloadMode
      // Protocol drift must never expand into an unexpected remote process termination.
      : "none"
  };
}

function normalizeRemoteCodexReloadResult(value: unknown): RemoteCodexReloadResult {
  const reload = value && typeof value === "object" ? value as Partial<RemoteCodexReloadResult> : {};
  const mode = remoteCodexReloadModes.has(reload.mode as RemoteCodexReloadMode)
    ? reload.mode as RemoteCodexReloadMode
    : "none";
  const status = remoteCodexReloadStatuses.has(reload.status as RemoteCodexReloadStatus)
    ? reload.status as RemoteCodexReloadStatus
    : "failed";
  return {
    mode,
    status,
    targetedCount: Number.isFinite(reload.targetedCount) ? Math.max(0, Number(reload.targetedCount)) : 0,
    stoppedCount: Number.isFinite(reload.stoppedCount) ? Math.max(0, Number(reload.stoppedCount)) : 0,
    preservedCliCount: Number.isFinite(reload.preservedCliCount) ? Math.max(0, Number(reload.preservedCliCount)) : 0,
    replacementObserved: reload.replacementObserved === true,
    message: typeof reload.message === "string" ? reload.message : ""
  };
}

function normalizeProfileApplyHostResult(item: ProfileApplyHostResult | Record<string, unknown>): ProfileApplyHostResult {
  return {
    ...item,
    hostId: typeof item.hostId === "string" ? item.hostId : "",
    hostName: typeof item.hostName === "string" ? item.hostName : "",
    hostAlias: typeof item.hostAlias === "string" ? item.hostAlias : "",
    status: profileApplyStatuses.has(item.status as "pending")
      ? item.status as "pending" | "success" | "failed" | "no-change"
      : "failed",
    targetPath: typeof item.targetPath === "string" ? item.targetPath : "",
    backupPath: typeof item.backupPath === "string" ? item.backupPath : null,
    message: typeof item.message === "string" ? item.message : "",
    reload: normalizeRemoteCodexReloadResult(item.reload),
    task: item.task && typeof item.task === "object" ? item.task as ProfileApplyHostResult["task"] : undefined
  };
}

export function normalizeProfileApplyResult(
  result: ProfileApplyBatchResult | ProfileApplyBatchResultDto
): ProfileApplyBatchResult {
  const outcome = profileApplyOutcomes.has(result.outcome as ProfileApplyOutcome)
    ? result.outcome as ProfileApplyOutcome
    : "failed";
  return {
    ...result,
    ok: outcome === "success",
    outcome,
    results: result.results.map((item) => normalizeProfileApplyHostResult(item)),
    profiles: (result.profiles ?? []).map(normalizeProfile),
    hosts: (result.hosts ?? []).map(normalizeHost)
  };
}

export function normalizeProfileApplyPreview(result: ProfileApplyPreviewDto): ProfileApplyPreview {
  return {
    ...result,
    warnings: result.warnings ?? [],
    hostResults: result.hostResults.map((item) => normalizeProfileApplyHostResult(item))
  };
}
