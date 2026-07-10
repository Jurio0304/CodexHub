use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Manager};

#[cfg(test)]
static TEST_PATH_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub(crate) struct AppPaths {
    config_dir: PathBuf,
    cache_dir: PathBuf,
    database_path: PathBuf,
}

impl AppPaths {
    pub(crate) fn resolve(app: &AppHandle) -> Result<Self, String> {
        let config_dir = app
            .path()
            .app_config_dir()
            .map_err(|error| format!("Could not resolve the app config directory: {error}"))?;
        let cache_dir = app
            .path()
            .app_cache_dir()
            .map_err(|error| format!("Could not resolve the app cache directory: {error}"))?;
        let database_path = config_dir.join("codexhub.db");
        Ok(Self {
            config_dir,
            cache_dir,
            database_path,
        })
    }

    #[cfg(test)]
    pub(crate) fn for_tests() -> Self {
        let root = std::env::temp_dir().join(format!(
            "codexhub-state-test-{}-{}",
            std::process::id(),
            TEST_PATH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        Self::for_test_root(root)
    }

    #[cfg(test)]
    pub(crate) fn for_test_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            config_dir: root.join("config"),
            cache_dir: root.join("cache"),
            database_path: root.join("config").join("codexhub.db"),
        }
    }

    pub(crate) fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub(crate) fn config_file(&self, file_name: &str) -> PathBuf {
        self.config_dir.join(file_name)
    }

    pub(crate) fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    pub(crate) fn cache_file(&self, file_name: &str) -> PathBuf {
        self.cache_dir.join(file_name)
    }

    pub(crate) fn backup_directory(&self, store: &str) -> PathBuf {
        self.config_dir.join("backups").join(store)
    }

    pub(crate) fn ensure_resolved(&self) -> Result<(), String> {
        if self.config_dir.as_os_str().is_empty()
            || self.cache_dir.as_os_str().is_empty()
            || self.database_path.as_os_str().is_empty()
        {
            return Err("Application storage paths are unavailable.".into());
        }
        Ok(())
    }
}
