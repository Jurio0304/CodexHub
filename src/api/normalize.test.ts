import { describe, expect, it } from "vitest";
import type { ProfileApplyBatchResultDto } from "../generated/rust-contracts";
import { normalizeProfileApplyOptions, normalizeProfileApplyResult } from "./normalize";

describe("normalizeProfileApplyResult", () => {
  it("falls back unknown requested modes to no process reload", () => {
    expect(normalizeProfileApplyOptions({ remoteCodexReloadMode: "future-mode" })).toEqual({
      remoteCodexReloadMode: "none"
    });
    expect(normalizeProfileApplyOptions(undefined)).toEqual({ remoteCodexReloadMode: "none" });
  });

  it("degrades unknown apply and reload enums to explicit failure states", () => {
    const wireResult = {
      profileId: "profile-1",
      ok: true,
      outcome: "future-outcome",
      results: [
        {
          hostId: "host-1",
          hostName: "Host 1",
          hostAlias: "host-1",
          status: "future-status",
          targetPath: "~/.codex/config.toml",
          backupPath: null,
          message: "Protocol drift",
          reload: {
            mode: "future-mode",
            status: "future-reload-status",
            targetedCount: -2,
            stoppedCount: -1,
            preservedCliCount: -5,
            replacementObserved: false,
            message: "Unknown reload result"
          },
          task: null
        }
      ],
      tasks: [],
      profiles: [],
      hosts: []
    } as unknown as ProfileApplyBatchResultDto;

    const normalized = normalizeProfileApplyResult(wireResult);

    expect(normalized.ok).toBe(false);
    expect(normalized.outcome).toBe("failed");
    expect(normalized.results[0]).toMatchObject({
      status: "failed",
      reload: {
        mode: "none",
        status: "failed",
        targetedCount: 0,
        stoppedCount: 0,
        preservedCliCount: 0,
        replacementObserved: false
      }
    });
  });

  it("derives ok only from the normalized success outcome", () => {
    const result = normalizeProfileApplyResult({
      profileId: "profile-1",
      ok: true,
      outcome: "manual-reconnect",
      results: [],
      tasks: [],
      profiles: [],
      hosts: []
    });

    expect(result.outcome).toBe("manual-reconnect");
    expect(result.ok).toBe(false);
  });
});
