import { expect, test, vi } from "vitest";
import { mockApi } from "./mock";

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

  await mockApi.clearTaskHistory();
});
