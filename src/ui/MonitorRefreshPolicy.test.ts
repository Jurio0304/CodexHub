import { describe, expect, test } from "vitest";
import { shouldRecordResourceSample } from "../App";

describe("resource monitor task policy", () => {
  test("records initial and manual refreshes but not scheduled polling", () => {
    expect(shouldRecordResourceSample("initial")).toBe(true);
    expect(shouldRecordResourceSample("manual")).toBe(true);
    expect(shouldRecordResourceSample("auto")).toBe(false);
  });
});
