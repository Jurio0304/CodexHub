import { render, screen } from "@testing-library/react";
import { describe, expect, test } from "vitest";
import {
  applyHostOperationProgressConnectivityToHosts,
  applyHostResourceConnectivityToHosts,
  applyRemoteProbeBatchResultsToHosts,
  applyRemoteProbeResultToHost,
  CircularStatusIndicator,
  resolveHostStatusIndicatorState
} from "../App";
import type { Host, HostOperationProgressEvent, HostResourceSnapshot, HostStatus, RemoteProbeResult, TaskRun } from "../models";

function host(hostAlias: string): Host {
  return {
    id: `host-${hostAlias}`,
    name: hostAlias,
    hostAlias,
    source: "managed",
    address: hostAlias,
    port: 22,
    username: "codex",
    authMethod: "ssh-key",
    status: "online",
    os: "TrustedOS",
    arch: "trusted-arch",
    shell: "/bin/trusted",
    path: "/trusted/bin",
    pathHasLocalBin: true,
    codexCommandAvailable: true,
    codexInstalled: true,
    codexVersion: "codex-cli 1.0.0",
    configExists: true,
    apiConfigName: "Trusted config",
    apiConfigSource: "profile",
    apiKeyEnvVar: "OPENAI_API_KEY",
    apiKeyEnvPresent: true,
    skillsExists: true,
    skillsCount: 3,
    profileId: "profile-1",
    skillPackIds: [],
    tags: [],
    lastSeen: "previously",
    latencyMs: 42
  };
}

function task(status: TaskRun["status"]): TaskRun {
  return {
    id: `task-${status}`,
    hostId: "host-lab",
    hostName: "lab",
    action: "Probe remote system",
    status,
    startedAt: "2026-07-14T12:00:00+08:00",
    endedAt: "2026-07-14T12:00:01+08:00",
    summary: status,
    steps: [],
    logs: []
  };
}

function probe(hostAlias: string, sshStatus: "online" | "offline", taskStatus: TaskRun["status"]): RemoteProbeResult {
  return {
    hostAlias,
    sshStatus,
    latencyMs: sshStatus === "online" ? 12 : null,
    os: "NewOS",
    arch: "new-arch",
    shell: "/bin/new",
    path: "/new/bin",
    pathHasLocalBin: true,
    codexCommandAvailable: true,
    codexInstalled: true,
    codexPath: "/new/bin/codex",
    codexVersion: "codex-cli 2.0.0",
    configExists: true,
    apiConfigName: "New config",
    apiConfigSource: "profile",
    apiKeyEnvVar: "NEW_API_KEY",
    apiKeyEnvPresent: true,
    skillsExists: true,
    skillsCount: 7,
    task: task(taskStatus)
  };
}

describe("host probe connectivity projection", () => {
  test("an offline result preserves trusted details and last-seen data", () => {
    const current = host("lab");
    const next = applyRemoteProbeResultToHost(current, probe("lab", "offline", "failed"), "just now");

    expect(next.status).toBe("offline");
    expect(next.latencyMs).toBeNull();
    expect(next.os).toBe(current.os);
    expect(next.codexVersion).toBe(current.codexVersion);
    expect(next.shell).toBe(current.shell);
    expect(next.lastSeen).toBe(current.lastSeen);
  });

  test("keeps SSH-reachable hosts online when a secondary probe group fails", () => {
    const next = applyRemoteProbeResultToHost(host("lab"), probe("lab", "online", "failed"), "just now");

    expect(next.status).toBe("online");
    expect(next.os).toBe("NewOS");
    expect(next.lastSeen).toBe("just now");
  });

  test("merges mixed batch results and marks result-less failures unknown", () => {
    const next = applyRemoteProbeBatchResultsToHosts(
      [host("online"), host("offline"), host("internal-error")],
      [
        { hostAlias: "online", ok: true, result: probe("online", "online", "success") },
        { hostAlias: "offline", ok: false, result: probe("offline", "offline", "failed") },
        { hostAlias: "internal-error", ok: false, error: "internal error" }
      ],
      "just now"
    );

    expect(next.map((item) => item.status)).toEqual(["online", "offline", "unknown"]);
    expect(next[2]?.latencyMs).toBeNull();
  });

  test("settles one batch host immediately while the remaining hosts stay testing", () => {
    const current = [host("alpha"), host("beta")].map((item) => ({ ...item, status: "testing" as const }));
    const next = applyRemoteProbeBatchResultsToHosts(
      current,
      [{ hostAlias: "alpha", ok: true, result: probe("alpha", "online", "success") }],
      "just now"
    );

    expect(next.map((item) => item.status)).toEqual(["online", "testing"]);
  });

  test("marks a host offline as soon as SSH progress reports a timeout", () => {
    const event: HostOperationProgressEvent = {
      requestId: "request-1",
      taskId: "task-1",
      hostAlias: "lab",
      operation: "host-test",
      step: {
        taskRunId: "task-1",
        stepId: "system",
        sequence: 1,
        status: "failed",
        summary: "timed out",
        startedAt: "now",
        endedAt: "now"
      },
      log: {
        id: "log-1",
        taskRunId: "task-1",
        stepId: "system",
        level: "error",
        timestamp: "now",
        message: "timed out",
        durationMs: 10000,
        timedOut: true
      }
    };

    const next = applyHostOperationProgressConnectivityToHosts([host("lab")], event);
    expect(next[0]?.status).toBe("offline");
    expect(next[0]?.latencyMs).toBeNull();
  });

  test("projects monitor SSH status without replacing trusted host details", () => {
    const current = host("lab");
    const snapshot: HostResourceSnapshot = {
      hostAlias: "lab",
      status: "failed",
      sshStatus: "offline",
      timedOut: true,
      sampledAt: "now",
      latencyMs: null,
      error: "timeout",
      cpu: null,
      memory: null,
      gpuTool: "none",
      gpus: []
    };
    const offline = applyHostResourceConnectivityToHosts([current], snapshot, "just now");
    expect(offline[0]).toEqual(expect.objectContaining({ status: "offline", os: "TrustedOS", lastSeen: "previously" }));
    expect(applyHostResourceConnectivityToHosts(offline, snapshot, "just now")).toBe(offline);
  });
});

describe("circular host status indicator", () => {
  test.each([
    ["online", "ok"],
    ["offline", "failed"],
    ["testing", "refreshing"],
    ["unknown", "no-sample"]
  ] as const)("maps %s to %s", (status, expected) => {
    expect(resolveHostStatusIndicatorState(status as HostStatus)).toBe(expected);
  });

  test("exposes the localized label without relying on color alone", () => {
    render(<CircularStatusIndicator label="Offline" state="failed" />);

    const indicator = screen.getByRole("img", { name: "Offline" });
    expect(indicator).toHaveAttribute("data-status", "failed");
    expect(indicator).toHaveAttribute("title", "Offline");
  });
});
