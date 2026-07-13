import { describe, expect, it } from "vitest";
import { resolveApiMode } from "./runtime";

describe("resolveApiMode", () => {
  it("selects Mock only for the explicit mock build mode", () => {
    expect(resolveApiMode("mock")).toBe("mock");
    expect(resolveApiMode("desktop")).toBe("desktop");
    expect(resolveApiMode("production")).toBe("desktop");
    expect(resolveApiMode(undefined)).toBe("desktop");
  });
});
