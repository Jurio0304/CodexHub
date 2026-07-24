import { describe, expect, test } from "vitest";
import type { HostResourceSnapshot } from "../models";
import { resolveMonitorGpuMemoryUsage } from "../App";

type Gpu = HostResourceSnapshot["gpus"][number];

function gpu(overrides: Partial<Gpu>): Gpu {
  return {
    vendor: "nvidia",
    index: "0",
    uuid: "GPU-test",
    name: "NVIDIA GB10",
    status: "ok",
    memoryMode: "unknown",
    utilizationPercent: 2,
    memoryUsedBytes: null,
    memoryTotalBytes: null,
    temperatureC: null,
    powerWatts: 13,
    driverVersion: "580.95",
    processes: [],
    ...overrides
  };
}

describe("GPU memory capacity", () => {
  test("uses host memory once as the DGX Spark unified-memory capacity", () => {
    const hostTotal = 125_511_744 * 1024;
    const processUsed = 5_067 * 1024 ** 2;
    const usage = resolveMonitorGpuMemoryUsage(gpu({
      memoryMode: "unified",
      processes: [{
        gpuUuid: "GPU-test",
        pid: 4242,
        name: "python",
        usedMemoryBytes: processUsed,
        user: "wzy",
        elapsedSeconds: 1_800,
        command: "python train.py"
      }]
    }), hostTotal);

    expect(usage.capacityBytes).toBe(hostTotal);
    expect(usage.usedBytes).toBe(processUsed);
    expect(usage.usedBytes / usage.capacityBytes! * 100).toBeCloseTo(4.13, 2);
  });

  test("keeps dedicated GPUs scaled by device-reported VRAM", () => {
    const usage = resolveMonitorGpuMemoryUsage(gpu({
      memoryMode: "dedicated",
      memoryUsedBytes: 6 * 1024 ** 3,
      memoryTotalBytes: 24 * 1024 ** 3
    }), 128 * 1024 ** 3);

    expect(usage.mode).toBe("dedicated");
    expect(usage.capacityBytes).toBe(24 * 1024 ** 3);
    expect(usage.usedBytes).toBe(6 * 1024 ** 3);
  });

  test("does not normalize unknown capacity to the current process sum", () => {
    const processUsed = 5 * 1024 ** 3;
    const usage = resolveMonitorGpuMemoryUsage(gpu({
      processes: [{
        gpuUuid: "GPU-test",
        pid: 4242,
        name: "python",
        usedMemoryBytes: processUsed,
        user: "wzy",
        elapsedSeconds: 1_800,
        command: "python train.py"
      }]
    }), 128 * 1024 ** 3);

    expect(usage.mode).toBe("unknown");
    expect(usage.capacityBytes).toBeNull();
    expect(usage.processUsedBytes).toBe(processUsed);
  });
});
