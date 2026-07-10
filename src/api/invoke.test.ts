import { describe, expect, it } from "vitest";
import { parseApiError } from "./invoke";

describe("parseApiError", () => {
  it("links a legacy durable failure envelope to its task", () => {
    expect(parseApiError("task-error:task-local-settings-123:Could not save settings.")).toEqual({
      code: "operation-failed",
      message: "Could not save settings.",
      retryable: true,
      taskId: "task-local-settings-123",
      recoveryId: null
    });
  });
});
