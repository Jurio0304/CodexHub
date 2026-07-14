import { describe, expect, test } from "vitest";
import type { HostResourceSnapshot } from "../models";
import {
  mergeHostResourceSnapshot,
  resolveMonitorHostIndicatorState,
  shouldRecordResourceSample
} from "../App";

function snapshot(hostAlias: string, usagePercent: number): HostResourceSnapshot {
  return {
    hostAlias,
    status: "ok",
    sshStatus: "online",
    timedOut: false,
    sampledAt: "2026-07-13T21:00:00+08:00",
    latencyMs: usagePercent,
    error: null,
    cpu: { usagePercent, load1: null, load5: null, load15: null, cores: null, model: null },
    memory: null,
    gpuTool: "none",
    gpus: []
  };
}

describe("resource monitor task policy", () => {
  test("records initial and manual refreshes but not scheduled polling", () => {
    expect(shouldRecordResourceSample("initial")).toBe(true);
    expect(shouldRecordResourceSample("manual")).toBe(true);
    expect(shouldRecordResourceSample("auto")).toBe(false);
  });

  test("merges each completed host immediately without changing host order", () => {
    const current = [snapshot("alpha", 1), snapshot("beta", 2)];
    const afterBeta = mergeHostResourceSnapshot(current, snapshot("beta", 20), ["alpha", "beta", "gamma"]);
    expect(afterBeta.map((item) => [item.hostAlias, item.cpu?.usagePercent])).toEqual([
      ["alpha", 1],
      ["beta", 20]
    ]);

    const afterGamma = mergeHostResourceSnapshot(afterBeta, snapshot("gamma", 30), ["alpha", "beta", "gamma"]);
    expect(afterGamma.map((item) => item.hostAlias)).toEqual(["alpha", "beta", "gamma"]);
  });
});

describe("resource monitor host indicator", () => {
  test("shows refreshing ahead of the previous snapshot status", () => {
    expect(resolveMonitorHostIndicatorState("ok", true)).toBe("refreshing");
    expect(resolveMonitorHostIndicatorState("failed", true)).toBe("refreshing");
  });

  test.each([
    ["ok", "ok"],
    ["partial", "partial"],
    ["failed", "failed"],
    [null, "no-sample"]
  ] as const)("maps %s to %s while idle", (status, expected) => {
    expect(resolveMonitorHostIndicatorState(status, false)).toBe(expected);
  });
});
