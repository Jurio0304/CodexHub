import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import type { CodexHubApi } from "../api";
import type { InitialAppData } from "../app/bootstrap";
import type { Host, RemoteProbeBatchItem, RemoteProbeBatchResult } from "../models";

const mocks = vi.hoisted(() => ({
  loadInitialAppData: vi.fn<() => Promise<InitialAppData>>(),
  listHosts: vi.fn<CodexHubApi["listHosts"]>(),
  listSshConfigHosts: vi.fn<CodexHubApi["listSshConfigHosts"]>(),
  refreshLatestCodexVersion: vi.fn<CodexHubApi["refreshLatestCodexVersion"]>(),
  remoteProbeCodex: vi.fn<CodexHubApi["remoteProbeCodex"]>(),
  batchRemoteProbeCodex: vi.fn<CodexHubApi["batchRemoteProbeCodex"]>()
}));

vi.mock("../app/bootstrap", () => ({ loadInitialAppData: mocks.loadInitialAppData }));
vi.mock("../api", async (importOriginal) => {
  const original = await importOriginal<typeof import("../api")>();
  const { mockApi } = await import("../api/mock");
  return {
    ...original,
    apiMode: "mock",
    api: {
      ...mockApi,
      listHosts: mocks.listHosts,
      listSshConfigHosts: mocks.listSshConfigHosts,
      refreshLatestCodexVersion: mocks.refreshLatestCodexVersion,
      remoteProbeCodex: mocks.remoteProbeCodex,
      batchRemoteProbeCodex: mocks.batchRemoteProbeCodex
    }
  };
});

import App, { uiCopy } from "../App";
import { fallbackAppUpdateStatus, fallbackHealth, fallbackLatestCodexVersion } from "../api/fallbacks";
import { defaultSettings } from "../settings";
import { FeedbackProvider } from "./feedback";

function host(hostAlias: string): Host {
  return {
    id: `host-${hostAlias}`,
    name: hostAlias,
    hostAlias,
    source: "managed",
    address: `${hostAlias}.example.test`,
    port: 22,
    username: "codex",
    authMethod: "ssh-key",
    status: "online",
    os: "Debian 12",
    arch: "x86_64",
    shell: "/bin/sh",
    path: "/usr/bin",
    pathHasLocalBin: true,
    codexCommandAvailable: true,
    codexInstalled: true,
    codexVersion: "codex-cli 0.1.0",
    configExists: false,
    apiConfigName: null,
    apiConfigSource: null,
    apiKeyEnvVar: null,
    apiKeyEnvPresent: null,
    skillsExists: false,
    skillsCount: 0,
    profileId: null,
    skillPackIds: [],
    tags: [],
    lastSeen: "previously",
    latencyMs: 42
  };
}

function completedItem(hostAlias: string): RemoteProbeBatchItem {
  return {
    hostAlias,
    ok: true,
    result: {
      hostAlias,
      sshStatus: "online",
      latencyMs: 12,
      os: "Debian 12",
      arch: "x86_64",
      shell: "/bin/sh",
      path: "/usr/bin",
      pathHasLocalBin: true,
      codexCommandAvailable: true,
      codexInstalled: true,
      codexPath: "/usr/bin/codex",
      codexVersion: "codex-cli 9.9.0",
      configExists: false,
      apiConfigName: "None",
      apiConfigSource: "none",
      apiKeyEnvVar: null,
      apiKeyEnvPresent: null,
      skillsExists: false,
      skillsCount: 0,
      task: {
        id: `probe-${hostAlias}`,
        hostId: `host-${hostAlias}`,
        hostName: hostAlias,
        action: "Probe remote system",
        status: "success",
        startedAt: "2026-10-19T09:00:00Z",
        endedAt: "2026-10-19T09:00:01Z",
        summary: "Host test completed.",
        steps: [],
        logs: []
      }
    }
  };
}

function initialData(showProgressModal: boolean, sampleTimeoutSeconds: number): InitialAppData {
  return {
    settings: {
      ...defaultSettings,
      fontPreset: "english",
      personalInfoMasking: false,
      setupGuideDismissed: true,
      resourceMonitorSampleTimeoutSeconds: sampleTimeoutSeconds,
      hostOperationLogPopups: showProgressModal
    },
    health: fallbackHealth,
    appUpdateStatus: fallbackAppUpdateStatus,
    hosts: [host("alpha"), host("beta")],
    profiles: [],
    skillPacks: [],
    skillInventoryStatus: { firstHostScanCompleted: false, localSkillRoot: "", localSkills: [], hostInventories: [] },
    tasks: [],
    taskNextCursor: null,
    unacknowledgedTaskIds: []
  };
}

function pendingBatch() {
  let resolve!: (result: RemoteProbeBatchResult) => void;
  let reject!: (error: Error) => void;
  let emit!: (item: RemoteProbeBatchItem) => void;
  let batchRequestId = "";
  const promise = new Promise<RemoteProbeBatchResult>((accept, fail) => {
    resolve = accept;
    reject = fail;
  });
  mocks.batchRemoteProbeCodex.mockImplementation((_aliases, _timeout, requestId, _progress, onItemCompleted) => {
    batchRequestId = requestId!;
    emit = (item) => onItemCompleted?.({ requestId: batchRequestId, item });
    return promise;
  });
  return {
    emit: (item: RemoteProbeBatchItem) => emit(item),
    reject: (error: Error) => reject(error),
    resolve: (results: RemoteProbeBatchItem[]) => resolve({
      requestId: batchRequestId,
      latestCodexVersion: { version: "9.9.0", checkedAt: "now", source: "npm", error: null },
      results
    })
  };
}

async function startBatch(showProgressModal = false, sampleTimeoutSeconds = defaultSettings.resourceMonitorSampleTimeoutSeconds) {
  mocks.loadInitialAppData.mockResolvedValue(initialData(showProgressModal, sampleTimeoutSeconds));
  const user = userEvent.setup();
  render(<FeedbackProvider><App /></FeedbackProvider>);
  await user.click(await screen.findByRole("button", { name: /^Hosts(?:\s|$)/ }));
  await user.click(await screen.findByRole("button", { name: "Test all" }));
  await waitFor(() => expect(mocks.batchRemoteProbeCodex).toHaveBeenCalledOnce());
  if (showProgressModal) {
    const dialog = screen.getByRole("dialog");
    await user.click(within(dialog).getAllByRole("button", { name: uiCopy.en.codexOperation.hide })[0]!);
  }
  return user;
}

function hostRow(alias: string) {
  return screen.getByRole("row", { name: new RegExp(alias) });
}

beforeEach(() => {
  window.localStorage.clear();
  vi.clearAllMocks();
  mocks.refreshLatestCodexVersion.mockResolvedValue(fallbackLatestCodexVersion);
  mocks.remoteProbeCodex.mockResolvedValue(completedItem("alpha").result!);
  mocks.listHosts.mockResolvedValue([host("alpha"), host("beta")]);
  mocks.listSshConfigHosts.mockResolvedValue(["alpha", "beta"].map((alias) => ({
    alias,
    hostName: `${alias}.example.test`,
    port: 22,
    user: "codex",
    identityFile: "",
    managed: true,
    source: "managed"
  })));
});

test.each([false, true])("settles hosts before the batch resolves with log popups %s", async (showProgressModal) => {
  const batch = pendingBatch();
  const user = await startBatch(showProgressModal);
  expect(hostRow("alpha").querySelector(".sshHostsVersionCol")).toHaveTextContent(uiCopy.en.hosts.testing);

  const alpha = completedItem("alpha");
  await act(async () => batch.emit(alpha));
  expect(within(hostRow("alpha")).getByRole("img", { name: /^online$/i })).toBeInTheDocument();
  expect(hostRow("alpha").querySelector(".sshHostsVersionCol")).toHaveTextContent("9.9.0");
  expect(within(hostRow("alpha")).getByRole("button", { name: "Test" })).toBeDisabled();
  expect(hostRow("beta").querySelector(".sshHostsVersionCol")).toHaveTextContent(uiCopy.en.hosts.testing);
  expect(screen.getByRole("button", { name: uiCopy.en.hosts.testingAll })).toBeDisabled();

  const beta = { hostAlias: "beta", ok: false, error: "Fixture probe failure" };
  await act(async () => batch.emit(beta));
  expect(within(hostRow("beta")).getByRole("img", { name: /^unknown$/i })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: uiCopy.en.hosts.testingAll })).toBeDisabled();

  await user.click(screen.getByRole("button", { name: "Home" }));
  expect(screen.getByRole("button", { name: uiCopy.en.hosts.testingAll })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: /^Hosts(?:\s|$)/ }));

  await act(async () => batch.resolve([alpha, beta]));
  expect(screen.getByRole("button", { name: "Test all" })).toBeEnabled();
  expect(within(hostRow("alpha")).getByRole("button", { name: "Test" })).toBeEnabled();
  expect(hostRow("alpha").querySelector(".sshHostsVersionCol")).toHaveTextContent("9.9.0");
});

test("keeps streamed successes when the remaining batch request fails", async () => {
  const batch = pendingBatch();
  await startBatch();
  await act(async () => batch.emit(completedItem("alpha")));
  await act(async () => batch.reject(new Error("Fixture batch transport failure")));

  expect(hostRow("alpha").querySelector(".sshHostsVersionCol")).toHaveTextContent("9.9.0");
  expect(within(hostRow("alpha")).getByRole("img", { name: /^online$/i })).toBeInTheDocument();
  expect(within(hostRow("beta")).getByRole("img", { name: /^unknown$/i })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Test all" })).toBeEnabled();
});

test("uses the final batch results when completion events are unavailable", async () => {
  const batch = pendingBatch();
  await startBatch();
  await act(async () => batch.resolve([completedItem("alpha"), completedItem("beta")]));

  for (const alias of ["alpha", "beta"]) {
    expect(hostRow(alias).querySelector(".sshHostsVersionCol")).toHaveTextContent("9.9.0");
    expect(within(hostRow(alias)).getByRole("img", { name: /^online$/i })).toBeInTheDocument();
    expect(within(hostRow(alias)).getByRole("button", { name: "Test" })).toBeEnabled();
  }
  expect(screen.getByRole("button", { name: "Test all" })).toBeEnabled();
});

test.each([1, 30, 45, 60])("uses the Monitor's %s-second timeout for batch and individual host tests", async (timeoutSeconds) => {
  const batch = pendingBatch();
  const user = await startBatch(false, timeoutSeconds);
  expect(mocks.batchRemoteProbeCodex).toHaveBeenCalledWith(
    ["alpha", "beta"], timeoutSeconds * 1000, expect.any(String), expect.any(Function), expect.any(Function)
  );
  await act(async () => batch.resolve([completedItem("alpha"), completedItem("beta")]));

  await user.click(within(hostRow("alpha")).getByRole("button", { name: "Test" }));
  expect(mocks.remoteProbeCodex).toHaveBeenCalledWith(
    "alpha", timeoutSeconds * 1000, expect.any(String), expect.any(Function)
  );
  expect(mocks.refreshLatestCodexVersion).toHaveBeenCalledWith(true, timeoutSeconds * 1000);
  await waitFor(() => expect(within(hostRow("alpha")).getByRole("button", { name: "Test" })).toBeEnabled());
});
