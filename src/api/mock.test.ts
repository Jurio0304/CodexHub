import { expect, test, vi } from "vitest";
import type { HostOperationProgressEvent, HostResourceProgressEvent } from "../models";
import { mockApi } from "./mock";

test("Mock profile apply mirrors each remote reload mode without exposing process command lines", async () => {
  const profile = (await mockApi.listProfiles())[0] ?? await mockApi.createProfile({
    name: "Mock reload profile",
    description: "Profile apply reload test",
    model: "gpt-5-codex",
    provider: "openai",
    baseUrl: "https://api.openai.com/v1",
    apiKeyEnvVar: "OPENAI_API_KEY",
    modelReasoningEffort: "medium",
    planModeReasoningEffort: "high",
    fastMode: false,
    serviceTier: "auto",
    approvalPolicy: "on-request",
    sandboxMode: "workspace-write",
    extraToml: "",
    hostIds: []
  });
  const host = (await mockApi.listHosts())[0] ?? await mockApi.addHost({
    name: "Mock reload host",
    address: "mock-reload-host",
    port: 22,
    username: "codex",
    authMethod: "ssh-key",
    tags: []
  });

  const preview = await mockApi.previewProfileApply(profile.id, [host.id]);
  expect(preview.hostResults[0]?.reload).toMatchObject({
    mode: "app-services",
    status: "not-requested"
  });

  const appServices = await mockApi.applyProfile(profile.id, [host.id], {
    remoteCodexReloadMode: "app-services"
  });
  expect(appServices).toMatchObject({ ok: true, outcome: "success" });
  expect(appServices.results[0]?.reload).toMatchObject({
    mode: "app-services",
    status: "reconnected",
    targetedCount: 1,
    stoppedCount: 1,
    preservedCliCount: 1,
    replacementObserved: true
  });
  expect(appServices.tasks[0]?.steps.map((step) => step.stepId)).toEqual([
    "profile-apply",
    "remote-codex-reload"
  ]);

  const noReload = await mockApi.applyProfile(profile.id, [host.id], {
    remoteCodexReloadMode: "none"
  });
  expect(noReload.results[0]?.reload.status).toBe("not-requested");

  const allCodex = await mockApi.applyProfile(profile.id, [host.id], {
    remoteCodexReloadMode: "all-codex"
  });
  expect(allCodex.results[0]?.reload).toMatchObject({
    mode: "all-codex",
    stoppedCount: 2,
    preservedCliCount: 0
  });
  expect(JSON.stringify([appServices, noReload, allCodex])).not.toMatch(/pkill|killall|codex app-server|api[_-]?key\s*[:=]/i);
});

test("Mock task pagination, acknowledgement, and update events match the desktop contract", async () => {
  await mockApi.clearTaskHistory();
  const onUpdate = vi.fn();
  const dispose = await mockApi.onTaskUpdated(onUpdate);
  const created = [];
  for (const message of ["first failure", "second failure", "third failure"]) {
    created.push(await mockApi.recordFrontendError(message));
  }

  expect(onUpdate).toHaveBeenCalledTimes(3);
  expect(onUpdate).toHaveBeenLastCalledWith(expect.objectContaining({ taskId: created[2].id, status: "failed" }));

  const firstPage = await mockApi.queryTasks({ limit: 2, cursor: null });
  expect(firstPage.items.map((task) => task.id)).toEqual([created[2].id, created[1].id]);
  expect(firstPage.nextCursor).toBe(created[1].id);
  expect(firstPage.unacknowledgedTaskIds).toEqual(expect.arrayContaining(created.map((task) => task.id)));

  const secondPage = await mockApi.queryTasks({ limit: 2, cursor: firstPage.nextCursor });
  expect(secondPage.items[0]?.id).toBe(created[0].id);
  expect(await mockApi.acknowledgeTask(created[2].id)).toBe(true);
  const acknowledged = await mockApi.queryTasks({ limit: 10, cursor: null });
  expect(acknowledged.unacknowledgedTaskIds).not.toContain(created[2].id);
  expect(await mockApi.clearTaskHistory()).toBe(3);
  expect(await mockApi.getTask(created[2].id)).toBeNull();

  for (let index = 0; index < 105; index += 1) {
    await mockApi.recordFrontendError(`retention-${index}`);
  }
  const retained = await mockApi.queryTasks({ limit: 200, cursor: null });
  expect(retained.items).toHaveLength(100);
  expect(retained.nextCursor).toBeNull();
  await mockApi.clearTaskHistory();

  dispose();
});

test("Mock automatic resource polling stays out of task history", async () => {
  await mockApi.clearTaskHistory();

  await mockApi.sampleHostResources(["mock-gpu-01"], 8000, false);
  expect((await mockApi.queryTasks({ limit: 10, cursor: null })).items).toHaveLength(0);

  await mockApi.sampleHostResources(["mock-gpu-01"], 8000, true);
  const recorded = await mockApi.queryTasks({ limit: 10, cursor: null });
  expect(recorded.items).toHaveLength(1);
  expect(recorded.items[0]).toEqual(expect.objectContaining({ action: "Sample host resources" }));
  expect(recorded.items[0]?.logs.map((log) => log.level)).toEqual(["info", "info"]);

  await mockApi.clearTaskHistory();
});

test("Mock resource sampling emits one progressive snapshot per completed host", async () => {
  const events: HostResourceProgressEvent[] = [];
  const aliases = ["mock-gpu-01", "mock-cpu-01", "mock-gpu-02"];

  const result = await mockApi.sampleHostResources(
    aliases,
    8000,
    false,
    "mock-resource-request",
    (event) => events.push(event)
  );

  expect(events.map((event) => event.requestId)).toEqual(aliases.map(() => "mock-resource-request"));
  expect(events.map((event) => event.snapshot.hostAlias)).toEqual(aliases);
  expect(result.snapshots.map((snapshot) => snapshot.hostAlias)).toEqual(aliases);
});

test("Mock recorded resource sampling keeps one typed log per host plus a warning summary", async () => {
  await mockApi.clearTaskHistory();
  await mockApi.sampleHostResources(["mock-gpu-01", "mock-cpu-01", "mock-timeout-host"], 10000, true);

  const task = (await mockApi.queryTasks({ limit: 10, cursor: null })).items[0];
  expect(task?.logs.map((log) => log.level)).toEqual(["info", "warn", "error", "warn"]);
  expect(task?.logs.slice(0, 3).map((log) => log.message)).toEqual([
    expect.stringContaining("Host mock-gpu-01: resource sampling succeeded"),
    expect.stringContaining("Host mock-cpu-01: resource sampling completed with partial data"),
    expect.stringContaining("Host mock-timeout-host: resource sampling failed")
  ]);
  expect(task?.logs[task.logs.length - 1]?.message).toBe("Sampled 3 host(s): 1 partial, 1 failed.");

  await mockApi.clearTaskHistory();
});

test("Mock resource timeout reports SSH offline at the ten-second boundary", async () => {
  const result = await mockApi.sampleHostResources(["mock-timeout-host"]);
  expect(result.snapshots[0]).toEqual(expect.objectContaining({
    status: "failed",
    sshStatus: "offline",
    timedOut: true,
    latencyMs: null
  }));
});

test("Mock Codex maintenance emits the ordered fallback chain without making a recovered task fail", async () => {
  const events: HostOperationProgressEvent[] = [];
  const result = await mockApi.remoteManageCodex(
    "mock-stage-host",
    "install",
    120000,
    "mock-install-request",
    (event) => events.push(event)
  );

  expect(result.ok).toBe(true);
  expect(result.task.steps.map((step) => [step.stepId, step.status])).toEqual([
    ["preparation", "success"],
    ["official-installer", "failed"],
    ["remote-native-mirror", "failed"],
    ["remote-npm-mirror", "success"],
    ["local-upload", "skipped"],
    ["runtime-reconcile", "success"],
    ["final-verification", "success"],
    ["release-cleanup", "success"]
  ]);
  expect(events.filter((event) => event.step.status === "running").map((event) => event.step.stepId)).toEqual([
    "preparation",
    "official-installer",
    "remote-native-mirror",
    "remote-npm-mirror",
    "runtime-reconcile",
    "final-verification",
    "release-cleanup"
  ]);

  const updated = await mockApi.remoteManageCodex("mock-stage-host", "update");
  expect(updated.ok).toBe(true);
  expect(updated.task.steps.find((step) => step.stepId === "release-cleanup")?.summary)
    .toContain("staged update backup mock-update-backup");

  const failed = await mockApi.remoteManageCodex("mock-fail-host", "update");
  expect(failed.ok).toBe(false);
  expect(failed.task.status).toBe("failed");
  expect(failed.task.steps.find((step) => step.stepId === "final-verification")?.status).toBe("failed");

  const uninstallEvents: HostOperationProgressEvent[] = [];
  const failedUninstall = await mockApi.remoteManageCodex(
    "mock-fail-uninstall",
    "uninstall",
    120000,
    "mock-uninstall-request",
    (event) => uninstallEvents.push(event)
  );
  expect(failedUninstall.ok).toBe(false);
  expect(failedUninstall.task.status).toBe("failed");
  expect(failedUninstall.task.steps.map((step) => [step.stepId, step.status])).toEqual([
    ["preparation", "success"],
    ["uninstall", "failed"],
    ["final-verification", "failed"]
  ]);
  expect(failedUninstall.beforeVersion).toBe("codex-cli 0.31.0");
  expect(failedUninstall.afterVersion).toBe(failedUninstall.beforeVersion);
  expect(failedUninstall.codexPath).toBe("$HOME/.local/bin/codex");
  expect(failedUninstall.codexCommandAvailable).toBe(true);
  expect(uninstallEvents.filter((event) => event.step.status !== "running").map((event) => [event.step.stepId, event.step.status])).toEqual([
    ["preparation", "success"],
    ["uninstall", "failed"],
    ["final-verification", "failed"]
  ]);
});

test("Mock host probe starts four independent groups before any group completes", async () => {
  const events: HostOperationProgressEvent[] = [];
  await mockApi.remoteProbeCodex(
    "mock-parallel-probe",
    10000,
    "mock-probe-request",
    (event) => events.push(event)
  );

  const sshCompleted = events.findIndex((event) => event.step.stepId === "ssh-check" && event.step.status === "success");
  const groupEvents = events.slice(sshCompleted + 1, sshCompleted + 5);
  expect(groupEvents.map((event) => [event.step.stepId, event.step.status])).toEqual([
    ["system", "running"],
    ["codex", "running"],
    ["api", "running"],
    ["skills", "running"]
  ]);

  const failed = await mockApi.remoteProbeCodex("mock-fail-probe");
  expect(failed.task.status).toBe("failed");
  expect(failed.task.steps.slice(1).every((step) => step.status === "skipped")).toBe(true);
});

test("Mock batch host probe emits each completed item before the ordered batch resolves", async () => {
  const aliases = Array.from({ length: 8 }, (_, index) => `mock-probe-${index + 1}`);
  const completed: string[] = [];
  let resolved = false;
  const promise = mockApi.batchRemoteProbeCodex(
    aliases,
    10000,
    "mock-probe-batch-request",
    undefined,
    (event) => {
      expect(resolved).toBe(false);
      expect(event.requestId).toBe("mock-probe-batch-request");
      completed.push(event.item.hostAlias);
    }
  );
  const result = await promise;
  resolved = true;

  expect(completed).toHaveLength(aliases.length);
  expect(new Set(completed)).toEqual(new Set(aliases));
  expect(result.results.map((item) => item.hostAlias)).toEqual(aliases);
});

test("Mock batch update uses a six-host sliding pool and preserves input order", async () => {
  const aliases = Array.from({ length: 8 }, (_, index) => `mock-batch-${index + 1}`);
  const events: HostOperationProgressEvent[] = [];
  const active = new Set<string>();
  let maxActive = 0;
  const result = await mockApi.batchRemoteUpdateCodex(
    aliases,
    120000,
    "mock-batch-request",
    (event) => {
      events.push(event);
      if (event.step.stepId === "preparation" && event.step.status === "running") {
        active.add(event.hostAlias);
        maxActive = Math.max(maxActive, active.size);
      }
      if (event.step.stepId === "final-verification" && event.step.status === "success") {
        active.delete(event.hostAlias);
      }
    }
  );

  expect(maxActive).toBe(6);
  expect(result.results.map((item) => item.hostAlias)).toEqual(aliases);
  const seventhStarted = events.findIndex((event) => event.hostAlias === aliases[6] && event.step.stepId === "preparation" && event.step.status === "running");
  const firstFinished = events.findIndex((event) => aliases.slice(0, 6).includes(event.hostAlias) && event.step.stepId === "final-verification" && event.step.status === "success");
  expect(seventhStarted).toBeGreaterThan(firstFinished);
});
