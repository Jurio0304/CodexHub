import { beforeEach, expect, test, vi } from "vitest";

const invokeMocks = vi.hoisted(() => ({
  requiredInvoke: vi.fn()
}));

vi.mock("./invoke", () => ({
  assertTauriRuntime: vi.fn(),
  requireHostAlias: (_command: string, alias: string) => alias,
  requiredInvoke: invokeMocks.requiredInvoke
}));

import { desktopApi } from "./desktop";

beforeEach(() => {
  invokeMocks.requiredInvoke.mockReset();
});

test("desktop host tests and resource samples share the default timeout", async () => {
  invokeMocks.requiredInvoke.mockResolvedValue(undefined);
  await desktopApi.remoteProbeCodex("host-a");
  expect(invokeMocks.requiredInvoke).toHaveBeenLastCalledWith("remote_probe_codex", expect.objectContaining({ timeoutMs: 30000 }));

  await desktopApi.batchRemoteProbeCodex(["host-a"]);
  expect(invokeMocks.requiredInvoke).toHaveBeenLastCalledWith("batch_remote_probe_codex", expect.objectContaining({ timeoutMs: 30000 }));

  await desktopApi.sampleHostResources(["host-a"]);
  expect(invokeMocks.requiredInvoke).toHaveBeenLastCalledWith("sample_host_resources", expect.objectContaining({ timeoutMs: 30000 }));
});

test("desktop profile apply forwards explicit remote reload options", async () => {
  invokeMocks.requiredInvoke.mockResolvedValue({
    profileId: "profile-1",
    ok: true,
    outcome: "success",
    results: [],
    tasks: [],
    profiles: [],
    hosts: []
  });

  await desktopApi.applyProfile("profile-1", ["host-1", "host-2"], {
    remoteCodexReloadMode: "all-codex"
  });

  expect(invokeMocks.requiredInvoke).toHaveBeenCalledWith("apply_profile", {
    profileId: "profile-1",
    hostIds: ["host-1", "host-2"],
    options: { remoteCodexReloadMode: "all-codex" }
  });
});

test("desktop batch update keeps process preview and execution as separate commands", async () => {
  invokeMocks.requiredInvoke.mockResolvedValueOnce({ requestId: "batch-1", results: [] });
  await desktopApi.previewBatchRemoteCodexUpdate(["host-a", "host-b"], 120000, "batch-1");
  expect(invokeMocks.requiredInvoke).toHaveBeenLastCalledWith("preview_batch_remote_codex_update", {
    hostAliases: ["host-a", "host-b"],
    timeoutMs: 120000,
    requestId: "batch-1"
  });

  const plans = [{
    hostAlias: "host-a",
    processAction: "force-terminate" as const,
    approvedProcesses: [{
      pid: 42,
      startTime: "1234",
      processName: "codex",
      processKind: "app-server" as const,
      version: "0.145.0",
      releasePath: "/home/u/.codex/packages/standalone/releases/0.145.0"
    }]
  }];
  invokeMocks.requiredInvoke.mockResolvedValueOnce({ requestId: "batch-1", action: "update", results: [] });
  await desktopApi.batchRemoteUpdateCodex(plans, 120000, "batch-1");
  expect(invokeMocks.requiredInvoke).toHaveBeenLastCalledWith("batch_remote_update_codex", {
    plans,
    timeoutMs: 120000,
    requestId: "batch-1"
  });
});
