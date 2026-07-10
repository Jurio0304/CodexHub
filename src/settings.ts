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
  sidebarCompletionIndicators: true,
  setupGuideDismissed: false
};

const windowsUiFont = '"Microsoft YaHei UI", "Microsoft YaHei", "Segoe UI Variable", "Segoe UI", "PingFang SC", "Noto Sans CJK SC", system-ui, sans-serif';
const windowsMonoFont = '"Cascadia Mono", "Cascadia Code", "Consolas", "Microsoft YaHei UI", monospace';
const macosUiFont = 'system-ui, -apple-system, BlinkMacSystemFont, "PingFang SC", "Noto Sans CJK SC", "Helvetica Neue", sans-serif';
const macosMonoFont = '"SF Mono", "Menlo", "Monaco", "Cascadia Mono", "Consolas", monospace';

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
    sidebarCompletionIndicators: candidate.sidebarCompletionIndicators !== false,
    setupGuideDismissed: candidate.setupGuideDismissed === true
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
