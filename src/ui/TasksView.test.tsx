import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { TasksView, uiCopy } from "../App";
import type { TaskRun } from "../models";
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
  logs: [{
    id: "task-1-log-1",
    taskRunId: "task-1",
    level: "info",
    timestamp: "2026-07-11T10:00:01+08:00",
    message: "Saved"
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
  await user.click(within(detail).getByRole("button", { name: "Close" }));

  await user.click(clearButton);
  const confirmation = await screen.findByRole("alertdialog", { name: "Clear task history?" });
  await user.click(within(confirmation).getByRole("button", { name: "Move to recycle bin" }));

  expect(onClearTaskHistory).toHaveBeenCalledTimes(1);
});
