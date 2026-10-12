import { render, screen } from "@testing-library/react";
import { describe, expect, test } from "vitest";
import { MonitorHostCard, uiCopy } from "../App";
import type { Host, HostResourceSnapshot } from "../models";

const host: Host = {
  id: "host-lab", name: "lab", hostAlias: "lab", source: "managed",
  address: "lab", port: 22, username: "codex", authMethod: "ssh-key",
  status: "online", os: "Linux", arch: "x86_64", shell: "/bin/sh", path: null,
  pathHasLocalBin: null, codexCommandAvailable: null, codexInstalled: false,
  codexVersion: "Not installed", configExists: false, apiConfigName: null, apiConfigSource: null,
  apiKeyEnvVar: null, apiKeyEnvPresent: null, skillsExists: false, skillsCount: 0,
  profileId: null, skillPackIds: [], tags: [], lastSeen: "now", latencyMs: 12
};

function snapshot(availableBytes = 1024 ** 3): HostResourceSnapshot {
  const sampledAt = "2026-10-12T08:00:00Z";
  return {
    hostAlias: "lab", status: "ok", sshStatus: "online", timedOut: false,
    sampledAt, latencyMs: 12, error: null,
    cpu: { usagePercent: 20, load1: 0.4, load5: null, load15: null, cores: 4, model: null },
    memory: { totalBytes: 8 * 1024 ** 3, availableBytes: 4 * 1024 ** 3, usedPercent: 50 },
    storage: {
      totalBytes: 4 * 1024 ** 3, availableBytes,
      usedPercent: (1 - availableBytes / (4 * 1024 ** 3)) * 100,
      filesystems: [
        { totalBytes: 1024 ** 3, availableBytes: availableBytes / 4,
          source: "/dev/sda3", filesystemType: "ext4", mountPoints: ["/"] },
        { totalBytes: 3 * 1024 ** 3, availableBytes: availableBytes * 3 / 4,
          source: "/dev/mapper/home", filesystemType: "xfs", mountPoints: ["/training data"] }
      ],
      sampledAt
    },
    gpuTool: "none", gpus: []
  };
}

function renderCard(sample: HostResourceSnapshot | null, copy: typeof uiCopy.en | typeof uiCopy.zh = uiCopy.en) {
  return render(
    <MonitorHostCard
      copy={copy} dragging={false} host={host} placeholder={false} refreshing={false}
      snapshot={sample} userColorByUser={new Map()}
      onCardElement={() => {}} onDragHandlePointerDown={() => {}}
    />
  );
}

describe("host resource summary", () => {
  test("orders memory, storage, CPU, GPU and keeps storage capacity compact", () => {
    const { container } = renderCard(snapshot());
    const tiles = Array.from(container.querySelectorAll(".monitorSummaryGrid > .monitorSummaryTile"));
    expect(tiles.map((tile) => tile.querySelector("span")?.textContent)).toEqual(["Memory", "Storage", "CPU", "GPU"]);
    const storage = tiles[1]!;
    expect(storage.querySelector("small")).toHaveTextContent("Available: 1.0 GB / 4.0 GB");
    expect(storage.querySelector("strong")).toHaveTextContent("75.0%");
    expect(storage.querySelector(".monitorMeter")).toHaveAttribute("data-tone", "storage");
    expect(storage.querySelector(".monitorMeter span")).toHaveStyle({ width: "75%" });
    expect(storage).toHaveAttribute("title", expect.stringContaining("/training data"));
    expect(storage).toHaveAttribute("title", expect.stringContaining("/: 256 MB / 1.0 GB"));
    expect(storage).not.toHaveAttribute("data-stale");
  });

  test("marks a retained reading and clears that indication after a fresh sample", () => {
    const stale = snapshot();
    stale.sampledAt = "2026-10-12T08:01:00Z";
    const { container, rerender } = renderCard(stale);
    const storage = container.querySelector(".monitorSummaryGrid > :nth-child(2)");
    expect(storage).toHaveAttribute("data-stale", "true");
    expect(storage).toHaveAttribute("title", expect.stringContaining("Last storage sample"));
    rerender(
      <MonitorHostCard
        copy={uiCopy.en} dragging={false} host={host} placeholder={false} refreshing={false}
        snapshot={snapshot()} userColorByUser={new Map()}
        onCardElement={() => {}} onDragHandlePointerDown={() => {}}
      />
    );
    expect(storage).not.toHaveAttribute("data-stale");
  });

  test("shows exhausted capacity in Chinese and keeps unknown samples distinct from zero", () => {
    const { container, unmount } = renderCard(snapshot(0), uiCopy.zh);
    expect(screen.getByText("存储")).toBeInTheDocument();
    const storage = container.querySelector(".monitorSummaryGrid > :nth-child(2)");
    expect(storage?.querySelector("small")).toHaveTextContent("0 B / 4.0 GB");
    expect(storage?.querySelector(".monitorMeter span")).toHaveStyle({ width: "100%" });
    unmount();
    const unknown = renderCard(null).container.querySelector(".monitorSummaryGrid > :nth-child(2)");
    expect(unknown?.querySelector("small")).toHaveTextContent(uiCopy.en.hosts.unknown);
    expect(unknown).not.toHaveTextContent("0 B");
  });
});
