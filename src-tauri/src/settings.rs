use super::storage;
use super::{AppPaths, TaskStore};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[cfg(test)]
use std::fs::{self, OpenOptions};
#[cfg(test)]
use std::io::{ErrorKind, Write};
#[cfg(test)]
use std::path::{Path, PathBuf};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "ThemeChoiceDto")]
pub(crate) enum ThemeChoice {
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "PlatformAppearanceDto")]
pub(crate) enum PlatformAppearance {
    Auto,
    Windows,
    Macos,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(rename = "CloseButtonBehaviorDto")]
pub(crate) enum CloseButtonBehavior {
    Ask,
    Exit,
    MinimizeToTray,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(rename = "NetworkProxyModeDto")]
pub(crate) enum NetworkProxyMode {
    Auto,
    Direct,
    Manual,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "AppSettingsDto")]
pub(crate) struct AppSettings {
    pub(crate) theme: ThemeChoice,
    #[ts(type = "\"english\" | \"zh-cn\"")]
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
    pub(crate) resource_monitor_auto_refresh: bool,
    #[serde(default)]
    pub(crate) resource_monitor_host_order: Vec<String>,
    #[serde(default = "default_resource_monitor_refresh_seconds")]
    pub(crate) resource_monitor_refresh_seconds: u16,
    #[serde(default = "default_true")]
    pub(crate) sidebar_completion_indicators: bool,
    #[serde(default = "default_true")]
    pub(crate) host_operation_log_popups: bool,
    #[serde(default)]
    pub(crate) setup_guide_dismissed: bool,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "SettingsSaveResultDto")]
pub(crate) struct SettingsSaveResult {
    pub(crate) settings: AppSettings,
    pub(crate) changed: bool,
    pub(crate) backup_path: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::System,
            font_preset: FontPreset::ZhCn,
            platform_appearance: PlatformAppearance::Auto,
            close_button_behavior: CloseButtonBehavior::Ask,
            network_proxy_mode: NetworkProxyMode::Auto,
            network_proxy_url: String::new(),
            resource_monitor_auto_refresh: true,
            resource_monitor_host_order: Vec::new(),
            resource_monitor_refresh_seconds: 60,
            sidebar_completion_indicators: true,
            host_operation_log_popups: true,
            setup_guide_dismissed: false,
        }
    }
}

impl AppSettings {
    fn normalized(mut self) -> Self {
        self.network_proxy_url = self.network_proxy_url.trim().to_string();
        self.resource_monitor_refresh_seconds =
            self.resource_monitor_refresh_seconds.clamp(15, 300);
        let mut seen = HashSet::new();
        self.resource_monitor_host_order = self
            .resource_monitor_host_order
            .into_iter()
            .map(|alias| alias.trim().to_string())
            .filter(|alias| !alias.is_empty() && seen.insert(alias.clone()))
            .collect();
        self
    }
}

pub(crate) fn read_settings(paths: &AppPaths) -> Result<AppSettings, String> {
    storage::load_document(paths, "settings", "settings.json", AppSettings::default())
        .map(|document| document.data.normalized())
}

pub(crate) fn write_settings(
    paths: &AppPaths,
    task_store: &TaskStore,
    settings: &AppSettings,
) -> Result<SettingsSaveResult, String> {
    let normalized = settings.clone().normalized();
    let existing = read_settings(paths)?;
    if existing == normalized {
        return Ok(SettingsSaveResult {
            settings: normalized,
            changed: false,
            backup_path: None,
        });
    }
    let backup_path =
        storage::save_document(paths, task_store, "settings", "settings.json", &normalized)?;
    Ok(SettingsSaveResult {
        settings: normalized,
        changed: true,
        backup_path,
    })
}

#[cfg(test)]
fn read_settings_at(path: &Path) -> Result<AppSettings, String> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(AppSettings::default()),
        Err(error) => return Err(format!("Could not read settings.json: {error}")),
    };
    serde_json::from_str::<AppSettings>(&content)
        .map(AppSettings::normalized)
        .map_err(|error| format!("settings.json is invalid and was not overwritten: {error}"))
}

#[cfg(test)]
fn write_settings_at(path: &Path, settings: &AppSettings) -> Result<SettingsSaveResult, String> {
    let normalized = settings.clone().normalized();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create the settings directory: {error}"))?;
    }

    let existing = if path.exists() {
        Some(read_settings_at(path)?)
    } else {
        None
    };
    if existing.as_ref() == Some(&normalized) {
        return Ok(SettingsSaveResult {
            settings: normalized,
            changed: false,
            backup_path: None,
        });
    }

    let content = serde_json::to_string_pretty(&normalized)
        .map_err(|error| format!("Could not serialize settings.json: {error}"))?;
    let temp_path = sidecar_path(path, ".codexhub.tmp");
    let backup_path = sidecar_path(path, ".bak");

    let write_result = (|| -> Result<Option<String>, String> {
        let mut temp_file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temp_path)
            .map_err(|error| format!("Could not create the settings temporary file: {error}"))?;
        temp_file
            .write_all(content.as_bytes())
            .and_then(|_| temp_file.sync_all())
            .map_err(|error| format!("Could not flush the settings temporary file: {error}"))?;

        let backup = if existing.is_some() {
            fs::copy(path, &backup_path)
                .map_err(|error| format!("Could not back up settings.json: {error}"))?;
            Some(backup_path.to_string_lossy().to_string())
        } else {
            None
        };

        fs::rename(&temp_path, path)
            .map_err(|error| format!("Could not atomically replace settings.json: {error}"))?;
        Ok(backup)
    })();

    if write_result.is_err() {
        if let Err(error) = fs::remove_file(&temp_path) {
            eprintln!("Could not clean the settings test staging file: {error}");
        }
    }
    let backup_path = write_result?;
    Ok(SettingsSaveResult {
        settings: normalized,
        changed: true,
        backup_path,
    })
}

#[cfg(test)]
fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{}", path.to_string_lossy(), suffix))
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

fn default_resource_monitor_refresh_seconds() -> u16 {
    60
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "codexhub-settings-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        ))
    }

    #[test]
    fn missing_settings_use_defaults() {
        let path = test_dir("missing").join("settings.json");
        assert_eq!(
            read_settings_at(&path).expect("missing settings should load"),
            AppSettings::default()
        );
    }

    #[test]
    fn invalid_settings_are_reported() {
        let dir = test_dir("invalid");
        fs::create_dir_all(&dir).expect("test directory should be created");
        let path = dir.join("settings.json");
        fs::write(&path, "{invalid").expect("invalid fixture should be written");
        let error = read_settings_at(&path).expect_err("invalid settings should fail");
        assert!(error.contains("invalid and was not overwritten"));
        fs::remove_dir_all(dir).expect("test directory should be removed");
    }

    #[test]
    fn writes_are_idempotent_and_back_up_changes() {
        let dir = test_dir("write");
        let path = dir.join("settings.json");
        let initial = AppSettings::default();
        let first = write_settings_at(&path, &initial).expect("initial settings should save");
        assert!(first.changed);
        assert!(first.backup_path.is_none());

        let unchanged = write_settings_at(&path, &initial).expect("unchanged settings should save");
        assert!(!unchanged.changed);
        assert!(unchanged.backup_path.is_none());

        let mut changed = initial;
        changed.resource_monitor_refresh_seconds = 120;
        changed.host_operation_log_popups = false;
        let saved = write_settings_at(&path, &changed).expect("changed settings should save");
        assert!(saved.changed);
        let backup = saved
            .backup_path
            .expect("changed settings should have a backup");
        assert!(Path::new(&backup).exists());
        assert_eq!(
            read_settings_at(&path).expect("saved settings should load"),
            changed
        );
        fs::remove_dir_all(dir).expect("test directory should be removed");
    }
}
