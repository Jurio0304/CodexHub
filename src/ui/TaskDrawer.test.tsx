import axe from "axe-core";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { TaskDrawer } from "./TaskDrawer";
import type { TaskRun } from "../generated/rust-contracts";

const failedTask: TaskRun = {
  id: "task-1",
  hostId: "local",
  hostName: "Local",
  action: "Save settings",
  status: "failed",
  startedAt: "2026-07-10T00:00:00Z",
  endedAt: "2026-07-10T00:00:01Z",
  summary: "Failed",
  logs: []
};

test("task drawer exposes unacknowledged work and opens its task", async () => {
  const user = userEvent.setup();
  const onOpenTask = vi.fn();
  render(
    <TaskDrawer
      copy={{ open: "Tasks", title: "Activity", description: "Recent tasks", close: "Close", viewAll: "View all", empty: "Empty", details: "Details", attentionCount: (count) => `${count} need attention` }}
      tasks={[failedTask]}
      unacknowledgedTaskIds={new Set([failedTask.id])}
      statusLabel={(task) => task.status}
      onOpenTask={onOpenTask}
      onViewAll={() => {}}
    />
  );
  const trigger = screen.getByRole("button", { name: "Tasks" });
  expect(trigger).toHaveTextContent("1");
  await user.click(trigger);
  expect(screen.getByRole("dialog", { name: "Activity" })).toBeVisible();
  expect((await axe.run(document.body, { rules: { "color-contrast": { enabled: false } } })).violations).toEqual([]);
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("dialog", { name: "Activity" })).not.toBeInTheDocument();
  expect(trigger).toHaveFocus();
  await user.click(trigger);
  await user.click(screen.getByRole("button", { name: /Save settings/u }));
  expect(onOpenTask).toHaveBeenCalledWith("task-1");
});
