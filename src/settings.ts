export type ThemeChoice = "system" | "light" | "dark";
export type FontPreset = "system" | "chinese" | "english" | "cross-platform";

export type AppSettings = {
  theme: ThemeChoice;
  fontPreset: FontPreset;
};

type FontPresetDefinition = {
  label: string;
  description: string;
  fontUi: string;
  fontMono: string;
};

export const settingsStorageKey = "codexhub.settings.v1";

export const defaultSettings: AppSettings = {
  theme: "system",
  fontPreset: "system"
};

const systemUi = '"Segoe UI Variable", "Segoe UI", "Microsoft YaHei UI", "Microsoft YaHei", system-ui, sans-serif';
const systemMono = '"Cascadia Mono", "Cascadia Code", "Consolas", monospace';
const chineseOptimizedUi = '"Microsoft YaHei UI", "Microsoft YaHei", "Segoe UI Variable", "Segoe UI", "PingFang SC", "Noto Sans CJK SC", system-ui, sans-serif';
const chineseOptimizedMono = '"Cascadia Mono", "Cascadia Code", "Consolas", "Microsoft YaHei UI", monospace';
const englishOptimizedUi = '"Segoe UI Variable", "Segoe UI", "Aptos", system-ui, sans-serif';
const crossPlatformUi = '"Segoe UI Variable", "Segoe UI", "San Francisco", "Helvetica Neue", "PingFang SC", "Noto Sans", system-ui, sans-serif';

export const fontPresets: Record<FontPreset, FontPresetDefinition> = {
  system: {
    label: "System Default",
    description: "Follow the Windows system UI stack with a safe monospace default.",
    fontUi: systemUi,
    fontMono: systemMono
  },
  chinese: {
    label: "Chinese Optimized",
    description: "Prioritize Microsoft YaHei for clearer Simplified Chinese labels.",
    fontUi: chineseOptimizedUi,
    fontMono: chineseOptimizedMono
  },
  english: {
    label: "English Optimized",
    description: "Favor Segoe UI Variable and Aptos for English-heavy workflows.",
    fontUi: englishOptimizedUi,
    fontMono: systemMono
  },
  "cross-platform": {
    label: "Cross Platform",
    description: "Use a broader fallback stack for machines that move between platforms.",
    fontUi: crossPlatformUi,
    fontMono: systemMono
  }
};

const themeValues: ThemeChoice[] = ["system", "light", "dark"];

function normalizeFontPreset(value: unknown): FontPreset {
  if (value === "chinese" || value === "english" || value === "cross-platform") return value;
  if (value === "zh-cn") return "chinese";
  return "system";
}

export function normalizeSettings(value: unknown): AppSettings {
  if (!value || typeof value !== "object") return defaultSettings;

  const candidate = value as Partial<AppSettings>;
  return {
    theme: themeValues.includes(candidate.theme as ThemeChoice) ? (candidate.theme as ThemeChoice) : defaultSettings.theme,
    fontPreset: normalizeFontPreset(candidate.fontPreset)
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

export function applyFontPreset(fontPreset: FontPreset) {
  const preset = fontPresets[fontPreset] ?? fontPresets.system;
  const root = document.documentElement;
  root.style.setProperty("--font-ui", preset.fontUi);
  root.style.setProperty("--font-mono", preset.fontMono);
  root.setAttribute("lang", fontPreset === "chinese" ? "zh-CN" : "en");
}

export function applyAppSettings(settings: AppSettings) {
  const normalized = normalizeSettings(settings);
  applyThemeChoice(normalized.theme);
  applyFontPreset(normalized.fontPreset);
}
