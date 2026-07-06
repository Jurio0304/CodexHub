use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ThemeChoice {
    System,
    Light,
    Dark,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) enum FontPreset {
    #[serde(
        rename = "english",
        alias = "system",
        alias = "chinese",
        alias = "cross-platform"
    )]
    English,
    #[serde(rename = "zh-cn")]
    ZhCn,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PlatformAppearance {
    Auto,
    Windows,
    Macos,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CloseButtonBehavior {
    Ask,
    Exit,
    MinimizeToTray,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NetworkProxyMode {
    Auto,
    Direct,
    Manual,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppSettings {
    pub(crate) theme: ThemeChoice,
    pub(crate) font_preset: FontPreset,
    #[serde(default = "default_platform_appearance")]
    pub(crate) platform_appearance: PlatformAppearance,
    #[serde(default = "default_close_button_behavior")]
    pub(crate) close_button_behavior: CloseButtonBehavior,
    #[serde(default = "default_network_proxy_mode")]
    pub(crate) network_proxy_mode: NetworkProxyMode,
    #[serde(default)]
    pub(crate) network_proxy_url: String,
    #[serde(default = "default_true")]
    pub(crate) sidebar_completion_indicators: bool,
    #[serde(default)]
    pub(crate) setup_guide_dismissed: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::System,
            font_preset: FontPreset::English,
            platform_appearance: PlatformAppearance::Auto,
            close_button_behavior: CloseButtonBehavior::Ask,
            network_proxy_mode: NetworkProxyMode::Auto,
            network_proxy_url: String::new(),
            sidebar_completion_indicators: true,
            setup_guide_dismissed: false,
        }
    }
}

pub(crate) fn read_settings(app: &AppHandle) -> AppSettings {
    let path = settings_path(app);
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<AppSettings>(&content).ok())
        .unwrap_or_default()
}

pub(crate) fn write_settings(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let path = settings_path(app);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let content = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;
    fs::write(path, content).map_err(|error| error.to_string())
}

fn settings_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("codexhub"))
        .join("settings.json")
}

fn default_platform_appearance() -> PlatformAppearance {
    PlatformAppearance::Auto
}

fn default_close_button_behavior() -> CloseButtonBehavior {
    CloseButtonBehavior::Ask
}

fn default_network_proxy_mode() -> NetworkProxyMode {
    NetworkProxyMode::Auto
}

fn default_true() -> bool {
    true
}
