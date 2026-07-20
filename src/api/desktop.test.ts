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
