import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { applyRemoteCodexResultToHost, TasksView, uiCopy } from "../App";
import type { Host, RemoteCodexMaintenanceResult, TaskRun } from "../models";
import { FeedbackProvider } from "./feedback";

const task: TaskRun = {
  id: "task-1",
  hostId: "local",
  hostName: "Local",
  action: "Save settings",
  status: "success",
  startedAt: "2026-07-11T10:00:00+08:00",
  endedAt: "2026-07-11T10:00:01+08:00",
  summary: "Saved",
  steps: [],
  logs: [{
    id: "task-1-log-1",
    taskRunId: "task-1",
    level: "info",
    timestamp: "2026-07-11T10:00:01+08:00",
    message: "Saved",
    command: "save_settings",
    stdout: "saved settings",
    stderr: ""
  }]
};

test("task page header clears all completed history while the detail dialog has no clear action", async () => {
  const user = userEvent.setup();
  const onClearTaskHistory = vi.fn(async () => 1);
  render(
    <FeedbackProvider>
      <TasksView
        copy={uiCopy.en}
        hasMore={false}
        loadingMore={false}
        mockMode={false}
        requestedTaskId={null}
        tasks={[task]}
        onClearTaskHistory={onClearTaskHistory}
        onLoadMore={async () => undefined}
        onRequestHandled={() => undefined}
        onTaskViewed={() => undefined}
      />
    </FeedbackProvider>
  );

  const clearButton = screen.getByRole("button", { name: "Clear all" });
  expect(clearButton.closest(".panelHeader")).toBeInTheDocument();

  await user.click(screen.getByRole("button", { name: "Logs" }));
  const detail = await screen.findByRole("dialog", { name: "Save settings" });
  expect(within(detail).queryByRole("button", { name: "Clear all" })).not.toBeInTheDocument();
  expect(within(detail).getByText("Historical details")).toBeInTheDocument();
  expect(within(detail).queryByText("Saved", { selector: ".codexOperationLogRow span" })).not.toBeInTheDocument();
  await user.click(within(detail).getByRole("button", { name: /Historical details/ }));
  const summary = within(detail).getByText("Saved", { selector: ".codexOperationLogRow span" }).closest("summary");
  expect(summary).not.toBeNull();
  const logDetails = summary!.closest("details");
  expect(logDetails).not.toHaveAttribute("open");
  await user.click(summary!);
  expect(logDetails).toHaveAttribute("open");
  expect(within(detail).getByText("save_settings")).toBeInTheDocument();
  await user.click(within(detail).getByRole("button", { name: "Close" }));

  await user.click(clearButton);
  const confirmation = await screen.findByRole("alertdialog", { name: "Clear task history?" });
  await user.click(within(confirmation).getByRole("button", { name: "Move to recycle bin" }));

  expect(onClearTaskHistory).toHaveBeenCalledTimes(1);
});

test("failed uninstall with unknown remote state preserves the confirmed host Codex state", () => {
  const host: Host = {
    id: "host-1",
    name: "GPU host",
    hostAlias: "gpu-host",
    source: "managed",
    address: "gpu-host",
    port: 22,
    username: "codex",
    authMethod: "ssh-key",
    status: "testing",
    os: "Linux",
    arch: "x86_64",
    shell: "bash",
    path: "/usr/bin:/home/codex/.local/bin",
    pathHasLocalBin: true,
    codexCommandAvailable: true,
    codexInstalled: true,
    codexVersion: "codex-cli 0.31.0",
    configExists: true,
    skillsExists: true,
    skillsCount: 2,
    profileId: null,
    skillPackIds: [],
    tags: [],
    lastSeen: "before failure",
    latencyMs: 12
  };
  const failed: RemoteCodexMaintenanceResult = {
    hostAlias: host.hostAlias,
    ok: false,
    action: "uninstall",
    beforeVersion: null,
    afterVersion: null,
    codexPath: null,
    codexCommandAvailable: false,
    installMethod: null,
    pathChanged: false,
    shellConfigPath: null,
    backupPath: null,
    message: "SSH check failed before uninstall.",
    task: { ...task, id: "uninstall-failed", action: "Uninstall Codex", status: "failed" }
  };

  const preserved = applyRemoteCodexResultToHost(host, failed, "uninstall", "just now");
  expect(preserved).toEqual(expect.objectContaining({
    status: "offline",
    codexInstalled: true,
    codexVersion: host.codexVersion,
    pathHasLocalBin: true,
    codexCommandAvailable: true,
    lastSeen: host.lastSeen
  }));

  const removed = applyRemoteCodexResultToHost(host, { ...failed, ok: true, message: "Uninstall completed." }, "uninstall", "just now");
  expect(removed).toEqual(expect.objectContaining({
    codexInstalled: false,
    codexVersion: "not installed",
    codexCommandAvailable: false
  }));
});
