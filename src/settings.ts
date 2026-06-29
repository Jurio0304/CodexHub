export type ThemeChoice = "system" | "light" | "dark";
export type FontPreset = "english" | "zh-cn";

export type AppSettings = {
  theme: ThemeChoice;
  fontPreset: FontPreset;
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
  setupGuideDismissed: false
};

const uiFont = '"Microsoft YaHei UI", "Microsoft YaHei", "Segoe UI Variable", "Segoe UI", "PingFang SC", "Noto Sans CJK SC", system-ui, sans-serif';
const monoFont = '"Cascadia Mono", "Cascadia Code", "Consolas", "Microsoft YaHei UI", monospace';

export const fontPresets: Record<FontPreset, FontPresetDefinition> = {
  english: {
    label: "English",
    fontUi: uiFont,
    fontMono: monoFont
  },
  "zh-cn": {
    label: "简体中文",
    fontUi: uiFont,
    fontMono: monoFont
  }
};

const themeValues: ThemeChoice[] = ["system", "light", "dark"];

function normalizeFontPreset(value: unknown): FontPreset {
  return value === "zh-cn" ? "zh-cn" : "english";
}

export function normalizeSettings(value: unknown): AppSettings {
  if (!value || typeof value !== "object") return defaultSettings;

  const candidate = value as Partial<AppSettings>;
  return {
    theme: themeValues.includes(candidate.theme as ThemeChoice) ? (candidate.theme as ThemeChoice) : defaultSettings.theme,
    fontPreset: normalizeFontPreset(candidate.fontPreset),
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

export function applyFontPreset(fontPreset: FontPreset) {
  const preset = fontPresets[fontPreset] ?? fontPresets.english;
  const root = document.documentElement;
  root.style.setProperty("--font-ui", preset.fontUi);
  root.style.setProperty("--font-mono", preset.fontMono);
  root.setAttribute("lang", fontPreset === "zh-cn" ? "zh-CN" : "en");
}

export function applyAppSettings(settings: AppSettings) {
  const normalized = normalizeSettings(settings);
  applyThemeChoice(normalized.theme);
  applyFontPreset(normalized.fontPreset);
}
