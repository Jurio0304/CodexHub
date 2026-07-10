import { expect, test, vi } from "vitest";
import { mockApi } from "./mock";

test("Mock task pagination, acknowledgement, and update events match the desktop contract", async () => {
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

  dispose();
});
