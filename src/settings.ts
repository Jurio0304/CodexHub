import { getPlatform, isMacOS } from "./platform";
import type { RuntimePlatform } from "./platform";

export type ThemeChoice = "system" | "light" | "dark";
export type FontPreset = "english" | "zh-cn";
export type PlatformAppearance = "auto" | "windows" | "macos";
export type CloseButtonBehavior = "ask" | "exit" | "minimize-to-tray";
export type NetworkProxyMode = "auto" | "direct" | "manual";

export type AppSettings = {
  theme: ThemeChoice;
  fontPreset: FontPreset;
  platformAppearance: PlatformAppearance;
  closeButtonBehavior: CloseButtonBehavior;
  networkProxyMode: NetworkProxyMode;
  networkProxyUrl: string;
  sidebarCompletionIndicators: boolean;
  setupGuideDismissed: boolean;
};

type FontPresetDefinition = {
  label: string;
  fontUi: string;
  fontMono: string;
};

export const settingsStorageKey = "codexhub.settings.v1";

export const defaultSettings: AppSettings = {
  theme: "system",
  fontPreset: "english",
  platformAppearance: "auto",
  closeButtonBehavior: "ask",
  networkProxyMode: "auto",
  networkProxyUrl: "",
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
  return value === "zh-cn" ? "zh-cn" : "english";
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
    sidebarCompletionIndicators: candidate.sidebarCompletionIndicators !== false,
    setupGuideDismissed: candidate.setupGuideDismissed === true
  };
}

export function loadLocalSettings(): AppSettings {
  try {
    const raw = window.localStorage.getItem(settingsStorageKey);
    return raw ? normalizeSettings(JSON.parse(raw)) : defaultSettings;
  } catch {
    return defaultSettings;
  }
}

export function saveLocalSettings(settings: AppSettings) {
  try {
    window.localStorage.setItem(settingsStorageKey, JSON.stringify(normalizeSettings(settings)));
  } catch {
    // Local persistence is best-effort when storage is disabled.
  }
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
  return isMacOS(platform) ? "macos" : "windows";
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
