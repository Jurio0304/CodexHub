import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test } from "vitest";
import { formatDuration, MonitorGpuBlock, sortMonitorGpuProcesses, uiCopy } from "../App";
import type { HostResourceSnapshot } from "../models";
import { createPersonalInfoMasker } from "../personalInfo";
import { PersonalInfoMaskingProvider } from "./PersonalInfoMasking";

type Gpu = HostResourceSnapshot["gpus"][number];

const longProcessName = "python-training-worker-with-a-very-long-readable-process-name";

function gpu(processes: Gpu["processes"] = sampleProcesses): Gpu {
  return {
    vendor: "nvidia",
    index: "0",
    uuid: "GPU-test",
    name: "NVIDIA RTX 4090",
    status: "ok",
    memoryMode: "dedicated",
    utilizationPercent: 72,
    memoryUsedBytes: 10 * 1024 ** 3,
    memoryTotalBytes: 24 * 1024 ** 3,
    temperatureC: 61,
    powerWatts: 220,
    driverVersion: "550.54",
    processes
  };
}

const sampleProcesses: Gpu["processes"] = [
  {
    gpuUuid: "GPU-test",
    pid: 202,
    name: longProcessName,
    cpuUsagePercent: 412.7,
    usedMemoryBytes: 8 * 1024 ** 3,
    user: "amax",
    elapsedSeconds: 7_200,
    command: "python secret_train.py --token should-not-render"
  },
  {
    gpuUuid: "GPU-test",
    pid: 101,
    name: "python-small",
    cpuUsagePercent: 4,
    usedMemoryBytes: 0,
    user: "amax",
    elapsedSeconds: 45,
    command: "python small.py --password should-not-render"
  },
  {
    gpuUuid: "GPU-test",
    pid: null,
    name: "jupyter-lab",
    cpuUsagePercent: null,
    usedMemoryBytes: null,
    user: "jy",
    elapsedSeconds: null,
    command: "jupyter lab --IdentityProvider.token=should-not-render"
  }
];

function renderGpu(sampledAt = "2026-07-27T10:00:00+08:00", processes = sampleProcesses) {
  return render(
    <MonitorGpuBlock
      copy={uiCopy.en}
      gpu={gpu(processes)}
      hostMemoryTotalBytes={128 * 1024 ** 3}
      sampledAt={sampledAt}
      userColorByUser={new Map([
        ["amax", "#2563eb"],
        ["jy", "#0f9f6e"]
      ])}
    />
  );
}

describe("GPU process details", () => {
  test("promotes process runtime to days with remaining hours as the highest precision", () => {
    expect(formatDuration(23 * 3600, uiCopy.en)).toBe("23 h");
    expect(formatDuration(24 * 3600, uiCopy.en)).toBe("1d");
    expect(formatDuration(25 * 3600, uiCopy.en)).toBe("1d1h");
    expect(formatDuration((6 * 24 + 13) * 3600, uiCopy.en)).toBe("6d13h");
    expect(formatDuration(7 * 24 * 3600, uiCopy.en)).toBe("7d");
    expect(formatDuration(30 * 24 * 3600, uiCopy.en)).toBe("30d");
  });

  test("expands users independently and renders only safe per-user process fields", async () => {
    const user = userEvent.setup();
    renderGpu();

    const amaxButton = screen.getByRole("button", { name: "Show GPU process details for amax" });
    expect(amaxButton).toHaveAttribute("aria-expanded", "false");
    await user.click(amaxButton);

    const amaxRegion = screen.getByRole("region", { name: "GPU process details for amax" });
    expect(amaxButton).toHaveAttribute("aria-expanded", "true");
    const amaxRows = within(amaxRegion).getAllByRole("listitem");
    expect(amaxRows).toHaveLength(2);
    expect(amaxRows[0]).toHaveTextContent("202");
    expect(amaxRows[0]).toHaveTextContent(longProcessName);
    expect(amaxRows[0]).toHaveTextContent("412.7%");
    expect(amaxRows[0]).toHaveTextContent("8.0 GB");
    expect(Array.from(amaxRegion.querySelector(".monitorProcessDetailsHeader")!.children, (cell) => cell.textContent))
      .toEqual(["PID", "Process name", "Runtime", "CPU load", "Process memory"]);
    expect(amaxRows[0]!.children[2]).toHaveTextContent("2.0 h");
    expect(amaxRows[0]!.children[3]).toHaveTextContent("412.7%");
    expect(amaxRows[1]).toHaveTextContent("101");
    expect(amaxRows[1]!.children[2]).toHaveTextContent("45 s");
    expect(amaxRows[1]).toHaveTextContent("4%");
    expect(amaxRows[1]).toHaveTextContent("0 B");
    expect(amaxRegion).not.toHaveTextContent("jupyter-lab");
    expect(amaxRegion).not.toHaveTextContent("should-not-render");

    const jyButton = screen.getByRole("button", { name: "Show GPU process details for jy" });
    jyButton.focus();
    await user.keyboard("{Enter}");
    const jyRegion = screen.getByRole("region", { name: "GPU process details for jy" });
    expect(jyRegion).toHaveTextContent("jupyter-lab");
    expect(jyRegion).toHaveTextContent("Unknown");
    expect(screen.getByRole("region", { name: "GPU process details for amax" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Hide GPU process details for amax" }));
    expect(screen.queryByRole("region", { name: "GPU process details for amax" })).not.toBeInTheDocument();
    expect(screen.getByRole("region", { name: "GPU process details for jy" })).toBeInTheDocument();
  });

  test("keeps the longest runtime in user summaries and each process runtime in its own row", async () => {
    const user = userEvent.setup();
    renderGpu("runtime-sample", [
      { ...sampleProcesses[0]!, elapsedSeconds: 0 },
      { ...sampleProcesses[1]!, elapsedSeconds: (6 * 24 + 13) * 3600 },
      sampleProcesses[2]!
    ]);

    const amaxButton = screen.getByRole("button", { name: "Show GPU process details for amax" });
    expect(amaxButton).toHaveTextContent("6d13h");
    await user.click(amaxButton);

    const rows = within(screen.getByRole("region", { name: "GPU process details for amax" })).getAllByRole("listitem");
    expect(rows[0]!.children[2]).toHaveTextContent("0 s");
    expect(rows[1]!.children[2]).toHaveTextContent("6d13h");
    await user.click(amaxButton);
    expect(amaxButton).toHaveAttribute("aria-expanded", "false");
    expect(amaxButton).toHaveTextContent("6d13h");

    await user.click(screen.getByRole("button", { name: "Show GPU process details for jy" }));
    const unknownRow = within(screen.getByRole("region", { name: "GPU process details for jy" })).getByRole("listitem");
    expect(unknownRow.children[2]).toHaveTextContent("Unknown");
  });

  test("keeps details open for ordinary rerenders and closes them for a new sample", async () => {
    const user = userEvent.setup();
    const view = renderGpu();
    await user.click(screen.getByRole("button", { name: "Show GPU process details for amax" }));

    view.rerender(
      <MonitorGpuBlock
        copy={uiCopy.en}
        gpu={gpu()}
        hostMemoryTotalBytes={128 * 1024 ** 3}
        sampledAt="2026-07-27T10:00:00+08:00"
        userColorByUser={new Map([["amax", "#2563eb"], ["jy", "#0f9f6e"]])}
      />
    );
    expect(screen.getByRole("region", { name: "GPU process details for amax" })).toBeInTheDocument();

    view.rerender(
      <MonitorGpuBlock
        copy={uiCopy.en}
        gpu={gpu()}
        hostMemoryTotalBytes={128 * 1024 ** 3}
        sampledAt="2026-07-27T10:01:00+08:00"
        userColorByUser={new Map([["amax", "#2563eb"], ["jy", "#0f9f6e"]])}
      />
    );
    await waitFor(() => {
      expect(screen.queryByRole("region", { name: "GPU process details for amax" })).not.toBeInTheDocument();
    });
  });

  test("keeps process details isolated between GPU blocks", async () => {
    const user = userEvent.setup();
    const firstProcesses: Gpu["processes"] = [{ ...sampleProcesses[0]!, pid: 111, name: "gpu-zero-worker" }];
    const secondProcesses: Gpu["processes"] = [{
      ...sampleProcesses[0]!,
      gpuUuid: "GPU-other",
      pid: 999,
      name: "gpu-one-worker"
    }];

    render(
      <>
        <MonitorGpuBlock
          copy={uiCopy.en}
          gpu={gpu(firstProcesses)}
          hostMemoryTotalBytes={128 * 1024 ** 3}
          sampledAt="sample-1"
          userColorByUser={new Map([["amax", "#2563eb"]])}
        />
        <MonitorGpuBlock
          copy={uiCopy.en}
          gpu={{ ...gpu(secondProcesses), index: "1", uuid: "GPU-other" }}
          hostMemoryTotalBytes={128 * 1024 ** 3}
          sampledAt="sample-1"
          userColorByUser={new Map([["amax", "#2563eb"]])}
        />
      </>
    );

    const disclosureButtons = screen.getAllByRole("button", { name: "Show GPU process details for amax" });
    await user.click(disclosureButtons[0]!);
    const openRegion = screen.getByRole("region", { name: "GPU process details for amax" });
    expect(openRegion).toHaveTextContent("111");
    expect(openRegion).toHaveTextContent("gpu-zero-worker");
    expect(openRegion).not.toHaveTextContent("999");
    expect(openRegion).not.toHaveTextContent("gpu-one-worker");
  });

  test("sorts by memory, then PID and name without mutating the snapshot", () => {
    const source: Gpu["processes"] = [
      { ...sampleProcesses[1]!, pid: 200, name: "zeta", usedMemoryBytes: 1024 },
      { ...sampleProcesses[1]!, pid: 100, name: "beta", usedMemoryBytes: 1024 },
      { ...sampleProcesses[1]!, pid: 100, name: "alpha", usedMemoryBytes: 1024 },
      { ...sampleProcesses[2]!, pid: null, name: "unknown-memory", usedMemoryBytes: null }
    ];

    expect(sortMonitorGpuProcesses(source).map((process) => process.name)).toEqual([
      "alpha",
      "beta",
      "zeta",
      "unknown-memory"
    ]);
    expect(source.map((process) => process.name)).toEqual(["zeta", "beta", "alpha", "unknown-memory"]);
  });

  test("does not render disclosure controls when the GPU has no processes", () => {
    renderGpu("2026-07-27T10:00:00+08:00", []);
    expect(screen.queryByRole("button", { name: /GPU process details/ })).not.toBeInTheDocument();
  });

  test("keeps process usernames visible when personal information masking is enabled", () => {
    render(
      <PersonalInfoMaskingProvider value={createPersonalInfoMasker(true, [{ username: "jy", address: "192.0.2.28" }])}>
        <MonitorGpuBlock
          copy={uiCopy.en}
          gpu={gpu()}
          hostMemoryTotalBytes={128 * 1024 ** 3}
          sampledAt="sample-with-masking"
          userColorByUser={new Map([["amax", "#2563eb"], ["jy", "#0f9f6e"]])}
        />
      </PersonalInfoMaskingProvider>
    );

    expect(screen.getByRole("button", { name: "Show GPU process details for jy" })).toHaveTextContent("jy");
    expect(screen.queryByText("j*")).not.toBeInTheDocument();
  });
});
