import { describe, expect, it, test } from "vitest";
import { defaultSettings, normalizeSettings, workspaceTerminalPreferences } from "./settings";

describe("resource monitor timing settings", () => {
  test("migrates existing settings to a thirty-second sample timeout", () => {
    expect(normalizeSettings({ resourceMonitorRefreshSeconds: 120 })).toMatchObject({
      resourceMonitorRefreshSeconds: 120,
      resourceMonitorSampleTimeoutSeconds: 30
    });
  });

  test("keeps sampling and refresh budgets independent within their ranges", () => {
    expect(normalizeSettings({ resourceMonitorRefreshSeconds: 120, resourceMonitorSampleTimeoutSeconds: 45 })).toMatchObject({
      resourceMonitorRefreshSeconds: 120,
      resourceMonitorSampleTimeoutSeconds: 45
    });
    expect(normalizeSettings({ resourceMonitorSampleTimeoutSeconds: 90 }).resourceMonitorSampleTimeoutSeconds).toBe(60);
    expect(normalizeSettings({ resourceMonitorSampleTimeoutSeconds: 0 }).resourceMonitorSampleTimeoutSeconds).toBe(1);
  });
});

test("Workspace terminal preferences are persisted safely and clamped", () => {
  const settings = normalizeSettings({
    ...defaultSettings,
    workspaceTerminalPreferences: {
      fontFamily: "jetbrains",
      fontSize: 99,
      lineHeight: "0.2",
      colorScheme: "high-contrast",
      scrollback: 999999,
      cursorStyle: "underline",
      screenReaderMode: true,
      confirmLargePaste: false
    }
  });

  expect(settings.workspaceTerminalPreferences).toEqual({
    fontFamily: "jetbrains",
    fontSize: 24,
    lineHeight: "1.00",
    colorScheme: "high-contrast",
    scrollback: 20000,
    cursorStyle: "underline",
    screenReaderMode: true,
    confirmLargePaste: false
  });
  expect(workspaceTerminalPreferences(settings)).toMatchObject({ lineHeight: 1, confirmLargePaste: false });
});

test("Workspace terminal preferences use safe defaults for an old settings payload", () => {
  const settings = normalizeSettings({ theme: "dark" });
  expect(settings.workspaceTerminalPreferences).toEqual(defaultSettings.workspaceTerminalPreferences);
});

test("legacy follow-app terminal color settings migrate to the independent dark default", () => {
  const settings = normalizeSettings({
    workspaceTerminalPreferences: { ...defaultSettings.workspaceTerminalPreferences, colorScheme: "follow-app" }
  });
  expect(settings.workspaceTerminalPreferences.colorScheme).toBe("dark");
});

describe("personal information masking settings", () => {
  it("is enabled for new and legacy settings", () => {
    expect(defaultSettings.personalInfoMasking).toBe(true);
    expect(normalizeSettings({}).personalInfoMasking).toBe(true);
  });

  it("preserves an explicit opt-out", () => {
    expect(normalizeSettings({ personalInfoMasking: false }).personalInfoMasking).toBe(false);
  });
});

describe("launch at login settings", () => {
  it("defaults to disabled for new and legacy settings", () => {
    expect(defaultSettings.launchAtLogin).toBe(false);
    expect(normalizeSettings({ theme: "dark" }).launchAtLogin).toBe(false);
  });

  it("preserves an explicit opt-in after the desktop backend confirms it", () => {
    expect(normalizeSettings({ launchAtLogin: true }).launchAtLogin).toBe(true);
  });
});
