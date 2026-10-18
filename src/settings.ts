import { getPlatform, isWindows } from "./platform";
import type { RuntimePlatform } from "./platform";
import type {
  AppSettingsDto,
  CloseButtonBehaviorDto,
  NetworkProxyModeDto,
  PlatformAppearanceDto,
  SettingsSaveResultDto,
  ThemeChoiceDto
} from "./generated/rust-contracts";
import type { WorkspaceTerminalPreferences } from "./workspace/types";

export type ThemeChoice = ThemeChoiceDto;
export type FontPreset = "english" | "zh-cn";
export type PlatformAppearance = PlatformAppearanceDto;
export type CloseButtonBehavior = CloseButtonBehaviorDto;
export type NetworkProxyMode = NetworkProxyModeDto;
export type AppSettings = AppSettingsDto;
export type SettingsSaveResult = SettingsSaveResultDto;

type FontPresetDefinition = {
  label: string;
  fontUi: string;
  fontMono: string;
};

export const legacySettingsStorageKey = "codexhub.settings.v1";
export const desktopSettingsCacheKey = "codexhub.desktop-settings-cache.v1";
export const mockSettingsStorageKey = "codexhub.mock-settings.v1";

export const defaultSettings: AppSettings = {
  theme: "system",
  fontPreset: "zh-cn",
  platformAppearance: "auto",
  closeButtonBehavior: "ask",
  networkProxyMode: "auto",
  networkProxyUrl: "",
  resourceMonitorAutoRefresh: true,
  resourceMonitorHostOrder: [],
  resourceMonitorRefreshSeconds: 60,
  resourceMonitorSampleTimeoutSeconds: 30,
  sidebarCompletionIndicators: true,
  hostOperationLogPopups: true,
  personalInfoMasking: true,
  launchAtLogin: false,
  setupGuideDismissed: false,
  workspaceTerminalPreferences: {
    fontFamily: "system-mono",
    fontSize: 14,
    lineHeight: "1.25",
    colorScheme: "dark",
    scrollback: 5000,
    cursorStyle: "block",
    screenReaderMode: false,
    confirmLargePaste: true
  }
};

const windowsUiFont = '"Microsoft YaHei UI", "Microsoft YaHei", "Segoe UI Variable", "Segoe UI", "PingFang SC", "Noto Sans CJK SC", system-ui, sans-serif';
const windowsMonoFont = '"Cascadia Mono", "Cascadia Code", "Consolas", "Microsoft YaHei UI", monospace';
const macosUiFont = 'system-ui, -apple-system, BlinkMacSystemFont, "PingFang SC", "Noto Sans CJK SC", "Helvetica Neue", sans-serif';
const macosMonoFont = '"Cascadia Mono", "Cascadia Code", "SF Mono", "Menlo", "Monaco", "Consolas", "PingFang SC", "Microsoft YaHei UI", monospace';

export const fontPresets: Record<FontPreset, FontPresetDefinition> = {
  english: {
    label: "English",
    fontUi: windowsUiFont,
    fontMono: windowsMonoFont
  },
  "zh-cn": {
    label: "简体中文",
    fontUi: windowsUiFont,
    fontMono: windowsMonoFont
  }
};

const themeValues: ThemeChoice[] = ["system", "light", "dark"];
const platformAppearanceValues: PlatformAppearance[] = ["auto", "windows", "macos"];
const closeButtonBehaviorValues: CloseButtonBehavior[] = ["ask", "exit", "minimize-to-tray"];
const networkProxyModeValues: NetworkProxyMode[] = ["auto", "direct", "manual"];

function normalizeFontPreset(value: unknown): FontPreset {
  return value === "english" ? "english" : "zh-cn";
}

export function normalizeResourceMonitorRefreshSeconds(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.round(Math.min(300, Math.max(15, value)))
    : defaultSettings.resourceMonitorRefreshSeconds;
}

export function normalizeResourceMonitorSampleTimeoutSeconds(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.round(Math.min(60, Math.max(1, value)))
    : defaultSettings.resourceMonitorSampleTimeoutSeconds;
}

function normalizeStringList(value: unknown): string[] {
  return Array.isArray(value)
    ? value
        .filter((item): item is string => typeof item === "string")
        .map((item) => item.trim())
        .filter(Boolean)
    : [];
}

export function normalizeSettings(value: unknown): AppSettings {
  if (!value || typeof value !== "object") return defaultSettings;

  const candidate = value as Partial<AppSettings>;
  return {
    theme: themeValues.includes(candidate.theme as ThemeChoice) ? (candidate.theme as ThemeChoice) : defaultSettings.theme,
    fontPreset: normalizeFontPreset(candidate.fontPreset),
    platformAppearance: platformAppearanceValues.includes(candidate.platformAppearance as PlatformAppearance)
      ? (candidate.platformAppearance as PlatformAppearance)
      : defaultSettings.platformAppearance,
    closeButtonBehavior: closeButtonBehaviorValues.includes(candidate.closeButtonBehavior as CloseButtonBehavior)
      ? (candidate.closeButtonBehavior as CloseButtonBehavior)
      : defaultSettings.closeButtonBehavior,
    networkProxyMode: networkProxyModeValues.includes(candidate.networkProxyMode as NetworkProxyMode)
      ? (candidate.networkProxyMode as NetworkProxyMode)
      : defaultSettings.networkProxyMode,
    networkProxyUrl: typeof candidate.networkProxyUrl === "string" ? candidate.networkProxyUrl.trim() : defaultSettings.networkProxyUrl,
    resourceMonitorAutoRefresh: candidate.resourceMonitorAutoRefresh !== false,
    resourceMonitorHostOrder: normalizeStringList(candidate.resourceMonitorHostOrder),
    resourceMonitorRefreshSeconds: normalizeResourceMonitorRefreshSeconds(candidate.resourceMonitorRefreshSeconds),
    resourceMonitorSampleTimeoutSeconds: normalizeResourceMonitorSampleTimeoutSeconds(candidate.resourceMonitorSampleTimeoutSeconds),
    sidebarCompletionIndicators: candidate.sidebarCompletionIndicators !== false,
    hostOperationLogPopups: candidate.hostOperationLogPopups !== false,
    personalInfoMasking: candidate.personalInfoMasking !== false,
    launchAtLogin: candidate.launchAtLogin === true,
    setupGuideDismissed: candidate.setupGuideDismissed === true,
    workspaceTerminalPreferences: normalizeWorkspaceTerminalPreferences(candidate.workspaceTerminalPreferences)
  };
}

function normalizeWorkspaceTerminalPreferences(value: unknown): AppSettings["workspaceTerminalPreferences"] {
  const current = value as Partial<AppSettings["workspaceTerminalPreferences"]> | null;
  const defaults = defaultSettings.workspaceTerminalPreferences;
  const lineHeight = Number(current?.lineHeight);
  return {
    fontFamily: ["system-mono", "cascadia", "jetbrains", "sf-mono"].includes(current?.fontFamily ?? "")
      ? current!.fontFamily!
      : defaults.fontFamily,
    fontSize: typeof current?.fontSize === "number" ? Math.max(12, Math.min(24, Math.round(current.fontSize))) : defaults.fontSize,
    lineHeight: Number.isFinite(lineHeight) ? Math.max(1, Math.min(2, lineHeight)).toFixed(2) : defaults.lineHeight,
    // Legacy follow-app values are deliberately migrated to the independent dark terminal default.
    colorScheme: ["light", "dark", "high-contrast"].includes(current?.colorScheme ?? "")
      ? current!.colorScheme!
      : defaults.colorScheme,
    scrollback: typeof current?.scrollback === "number" ? Math.max(100, Math.min(20000, Math.round(current.scrollback))) : defaults.scrollback,
    cursorStyle: ["block", "bar", "underline"].includes(current?.cursorStyle ?? "")
      ? current!.cursorStyle!
      : defaults.cursorStyle,
    screenReaderMode: current?.screenReaderMode === true,
    confirmLargePaste: current?.confirmLargePaste !== false
  };
}

export function workspaceTerminalPreferences(settings: AppSettings): WorkspaceTerminalPreferences {
  const value = normalizeWorkspaceTerminalPreferences(settings.workspaceTerminalPreferences);
  return {
    fontFamily: value.fontFamily,
    fontSize: value.fontSize,
    lineHeight: Number(value.lineHeight),
    colorScheme: value.colorScheme,
    scrollback: value.scrollback,
    cursorStyle: value.cursorStyle,
    screenReaderMode: value.screenReaderMode,
    confirmLargePaste: value.confirmLargePaste
  };
}

function loadSettingsFromStorage(key: string): AppSettings | null {
  try {
    const raw = window.localStorage.getItem(key);
    return raw ? normalizeSettings(JSON.parse(raw)) : null;
  } catch {
    return null;
  }
}

function saveSettingsToStorage(key: string, settings: AppSettings) {
  try {
    window.localStorage.setItem(key, JSON.stringify(normalizeSettings(settings)));
  } catch {
    // Browser storage is only a render cache in desktop mode.
  }
}

export function loadDesktopSettingsCache(): AppSettings {
  return loadSettingsFromStorage(desktopSettingsCacheKey) ?? defaultSettings;
}

export function saveDesktopSettingsCache(settings: AppSettings) {
  saveSettingsToStorage(desktopSettingsCacheKey, settings);
}

export function loadMockSettings(): AppSettings {
  const current = loadSettingsFromStorage(mockSettingsStorageKey);
  if (current) return current;

  const legacy = loadSettingsFromStorage(legacySettingsStorageKey);
  if (legacy) {
    saveSettingsToStorage(mockSettingsStorageKey, legacy);
    return legacy;
  }
  return defaultSettings;
}

export function saveMockSettings(settings: AppSettings) {
  saveSettingsToStorage(mockSettingsStorageKey, settings);
}

export function applyThemeChoice(theme: ThemeChoice) {
  const root = document.documentElement;
  if (theme === "system") {
    root.removeAttribute("data-theme");
    return;
  }

  root.dataset.theme = theme;
}

export function resolvePlatformAppearance(platformAppearance: PlatformAppearance): RuntimePlatform {
  if (platformAppearance === "windows" || platformAppearance === "macos") return platformAppearance;
  const platform = getPlatform();
  return isWindows(platform) ? "windows" : "macos";
}

export function applyPlatformAppearance(platformAppearance: PlatformAppearance) {
  const root = document.documentElement;
  root.dataset.platform = resolvePlatformAppearance(platformAppearance);
}

export function applyFontPreset(fontPreset: FontPreset, platformAppearance: PlatformAppearance = "auto") {
  const preset = fontPresets[fontPreset] ?? fontPresets.english;
  const effectivePlatform = resolvePlatformAppearance(platformAppearance);
  const root = document.documentElement;
  root.style.setProperty("--font-ui", effectivePlatform === "macos" ? macosUiFont : preset.fontUi);
  root.style.setProperty("--font-mono", effectivePlatform === "macos" ? macosMonoFont : preset.fontMono);
  root.setAttribute("lang", fontPreset === "zh-cn" ? "zh-CN" : "en");
}

export function applyAppSettings(settings: AppSettings) {
  const normalized = normalizeSettings(settings);
  applyThemeChoice(normalized.theme);
  applyPlatformAppearance(normalized.platformAppearance);
  applyFontPreset(normalized.fontPreset, normalized.platformAppearance);
}
