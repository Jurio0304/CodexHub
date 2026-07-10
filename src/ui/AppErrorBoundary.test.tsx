import { render, screen, waitFor } from "@testing-library/react";
import { expect, test, vi } from "vitest";

const mocks = vi.hoisted(() => ({ recordFrontendError: vi.fn(() => Promise.resolve()) }));

vi.mock("../api", () => ({
  api: { recordFrontendError: mocks.recordFrontendError }
}));

import { AppErrorBoundary } from "./AppErrorBoundary";

function BrokenView(): never {
  throw new Error("token=must-not-render");
}

test("render failures show a sanitized fallback and record no raw exception text", async () => {
  const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
  try {
    render(<AppErrorBoundary><BrokenView /></AppErrorBoundary>);
    expect(screen.getByRole("alert")).toBeVisible();
    expect(document.body).not.toHaveTextContent("must-not-render");
    await waitFor(() => expect(mocks.recordFrontendError).toHaveBeenCalledWith("React render failure."));
    expect(JSON.stringify(mocks.recordFrontendError.mock.calls)).not.toContain("must-not-render");
  } finally {
    consoleError.mockRestore();
  }
});
