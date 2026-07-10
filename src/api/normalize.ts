import type { Profile, ProfileApplyBatchResult } from "../models";

function nowStamp() {
  return new Date().toISOString();
}

export function normalizeProfile(profile: Profile): Profile {
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

export function normalizeProfileApplyResult(result: ProfileApplyBatchResult): ProfileApplyBatchResult {
  return {
    ...result,
    profiles: (result.profiles ?? []).map(normalizeProfile),
    hosts: result.hosts ?? []
  };
}
