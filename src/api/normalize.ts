import type { Host, Profile, ProfileApplyBatchResult, ProfileApplyPreview } from "../models";
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

export function normalizeProfileApplyResult(
  result: ProfileApplyBatchResult | ProfileApplyBatchResultDto
): ProfileApplyBatchResult {
  const validStatuses = new Set(["pending", "success", "failed", "no-change"] as const);
  return {
    ...result,
    results: result.results.map((item) => ({
      ...item,
      status: validStatuses.has(item.status as "pending")
        ? item.status as "pending" | "success" | "failed" | "no-change"
        : "failed",
      task: item.task ?? undefined
    })),
    profiles: (result.profiles ?? []).map(normalizeProfile),
    hosts: (result.hosts ?? []).map(normalizeHost)
  };
}

export function normalizeProfileApplyPreview(result: ProfileApplyPreviewDto): ProfileApplyPreview {
  const validStatuses = new Set(["pending", "success", "failed", "no-change"] as const);
  return {
    ...result,
    warnings: result.warnings ?? [],
    hostResults: result.hostResults.map((item) => ({
      ...item,
      status: validStatuses.has(item.status as "pending")
        ? item.status as "pending" | "success" | "failed" | "no-change"
        : "failed",
      task: item.task ?? undefined
    }))
  };
}
