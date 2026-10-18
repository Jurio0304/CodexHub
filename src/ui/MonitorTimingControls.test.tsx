import type { ComponentProps } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vitest";
import { MonitorTimingControls, uiCopy } from "../App";

function renderControls(overrides: Partial<ComponentProps<typeof MonitorTimingControls>> = {}) {
  const props = {
    autoRefresh: true,
    copy: uiCopy.zh,
    refreshSeconds: 60,
    sampleTimeoutSeconds: 30,
    settingsSaving: false,
    onAutoRefreshChange: vi.fn(),
    onRefreshSecondsChange: vi.fn().mockResolvedValue(true),
    onSampleTimeoutSecondsChange: vi.fn().mockResolvedValue(true),
    ...overrides
  };
  return { ...render(<MonitorTimingControls {...props} />), props };
}

describe("monitor timing controls", () => {
  test("allows clearing and typing a complete refresh interval before saving on blur", async () => {
    const user = userEvent.setup();
    const { props } = renderControls();
    const input = screen.getByRole("spinbutton", { name: "刷新间隔" });

    await user.clear(input);
    expect(input).toHaveValue(null);
    await user.type(input, "120");
    expect(input).toHaveValue(120);
    expect(props.onRefreshSecondsChange).not.toHaveBeenCalled();
    await user.tab();
    expect(props.onRefreshSecondsChange).toHaveBeenCalledTimes(1);
    expect(props.onRefreshSecondsChange).toHaveBeenCalledWith(120);
  });

  test("saves a sixty-second sample timeout once on Enter", async () => {
    const user = userEvent.setup();
    const { props } = renderControls();
    const input = screen.getByRole("spinbutton", { name: "采样超时" });

    await user.clear(input);
    await user.type(input, "60");
    expect(props.onSampleTimeoutSecondsChange).not.toHaveBeenCalled();
    await user.keyboard("{Enter}");
    expect(props.onSampleTimeoutSecondsChange).toHaveBeenCalledTimes(1);
    expect(props.onSampleTimeoutSecondsChange).toHaveBeenCalledWith(60);
    expect(input).not.toHaveFocus();
  });

  test("restores the saved value for Escape or an empty field", async () => {
    const user = userEvent.setup();
    const { props } = renderControls();
    const input = screen.getByRole("spinbutton", { name: "刷新间隔" });

    await user.clear(input);
    await user.type(input, "180");
    await user.keyboard("{Escape}");
    expect(input).toHaveValue(60);
    await user.clear(input);
    await user.tab();
    expect(input).toHaveValue(60);
    expect(props.onRefreshSecondsChange).not.toHaveBeenCalled();
  });

  test("keeps manual sample timeout editable with auto refresh off and caps it on commit", async () => {
    const user = userEvent.setup();
    const { props } = renderControls({ autoRefresh: false });
    expect(screen.getByRole("spinbutton", { name: "刷新间隔" })).toBeDisabled();
    const input = screen.getByRole("spinbutton", { name: "采样超时" });
    expect(input).toBeEnabled();

    await user.clear(input);
    await user.type(input, "90");
    expect(input).toHaveValue(90);
    await user.tab();
    expect(input).toHaveValue(60);
    expect(props.onSampleTimeoutSecondsChange).toHaveBeenCalledTimes(1);
    expect(props.onSampleTimeoutSecondsChange).toHaveBeenCalledWith(60);
  });

  test("restores the persisted value when saving fails", async () => {
    const user = userEvent.setup();
    renderControls({ onSampleTimeoutSecondsChange: vi.fn().mockResolvedValue(false) });
    const input = screen.getByRole("spinbutton", { name: "采样超时" });
    await user.clear(input);
    await user.type(input, "45");
    await user.tab();
    expect(input).toHaveValue(30);
  });
});
