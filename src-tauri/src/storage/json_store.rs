use chrono::Local;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use ts_rs::TS;

use super::{AppPaths, TaskStore};

pub(super) const CURRENT_JSON_SCHEMA_VERSION: u16 = 1;
static JSON_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
const DURABLE_STORES: [(&str, &str); 4] = [
    ("settings", "settings.json"),
    ("hosts", "hosts.json"),
    ("profiles", "profiles.json"),
    ("skills", "skills.json"),
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct VersionedDocument<T> {
    pub(super) schema_version: u16,
    pub(super) updated_at: String,
    pub(super) data: T,
}

#[derive(Clone, Debug)]
pub(crate) struct LoadedDocument<T> {
    pub(crate) data: T,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum StorageState {
    Missing,
    Current,
    MigrationRequired,
    RecoveryRequired,
    Corrupt,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StorageHealth {
    pub(crate) store: String,
    pub(crate) path: String,
    pub(crate) state: StorageState,
    pub(crate) schema_version: Option<u16>,
    pub(crate) current_schema_version: u16,
    pub(crate) source_sha256: Option<String>,
    pub(crate) latest_backup_path: Option<String>,
    pub(crate) message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StorageMigrationPlan {
    pub(crate) token: String,
    pub(crate) store: String,
    pub(crate) path: String,
    pub(crate) source_sha256: String,
    pub(crate) from_schema_version: u16,
    pub(crate) to_schema_version: u16,
    pub(crate) backup_directory: String,
    pub(crate) message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StorageRestorePlan {
    pub(crate) token: String,
    pub(crate) store: String,
    pub(crate) target_path: String,
    pub(crate) backup_path: String,
    pub(crate) backup_sha256: String,
    pub(crate) message: String,
}

pub(crate) fn load_document<T>(
    paths: &AppPaths,
    store: &str,
    file_name: &str,
    default: T,
) -> Result<LoadedDocument<T>, String>
where
    T: DeserializeOwned,
{
    let path = config_file_path(paths, file_name);
    load_document_at(&path, store, default)
}

fn load_document_at<T>(path: &Path, store: &str, default: T) -> Result<LoadedDocument<T>, String>
where
    T: DeserializeOwned,
{
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LoadedDocument { data: default })
        }
        Err(error) => {
            return Err(format!(
                "Could not read {store} storage {}: {error}",
                path.display()
            ))
        }
    };
    let value = serde_json::from_slice::<Value>(&bytes).map_err(|error| {
        format!(
            "{store} storage {} is invalid JSON: {error}",
            path.display()
        )
    })?;
    if value.get("schemaVersion").is_some() && value.get("data").is_some() {
        let document = serde_json::from_value::<VersionedDocument<T>>(value).map_err(|error| {
            format!(
                "Could not decode versioned {store} storage {}: {error}",
                path.display()
            )
        })?;
        if document.schema_version != CURRENT_JSON_SCHEMA_VERSION {
            return Err(format!(
                "{store} storage schema {} is not supported; expected {}.",
                document.schema_version, CURRENT_JSON_SCHEMA_VERSION
            ));
        }
        return Ok(LoadedDocument {
            data: document.data,
        });
    }
    serde_json::from_value::<T>(value)
        .map(|data| LoadedDocument { data })
        .map_err(|error| {
            format!(
                "Could not decode legacy {store} storage {}: {error}",
                path.display()
            )
        })
}

pub(crate) fn save_document<T>(
    paths: &AppPaths,
    task_store: &TaskStore,
    store: &str,
    file_name: &str,
    data: &T,
) -> Result<Option<String>, String>
where
    T: Serialize + ?Sized,
{
    let _write_guard = acquire_write_lock()?;
    let path = config_file_path(paths, file_name);
    let existing = if path.exists() {
        let bytes = fs::read(&path).map_err(|error| {
            format!("Could not read {store} storage {}: {error}", path.display())
        })?;
        let value = serde_json::from_slice::<Value>(&bytes).map_err(|error| {
            format!(
                "{store} storage {} is invalid JSON: {error}",
                path.display()
            )
        })?;
        if value.get("schemaVersion").is_none() || value.get("data").is_none() {
            return Err(format!(
                "storage-migration-required:{store}: Preview and confirm the local data migration before writing."
            ));
        }
        let schema_version = value
            .get("schemaVersion")
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("Could not decode versioned {store} storage schema."))?;
        if schema_version != u64::from(CURRENT_JSON_SCHEMA_VERSION) {
            return Err(format!(
                "storage-migration-required:{store}: Storage schema {} must be migrated before writing.",
                schema_version
            ));
        }
        Some(
            serde_json::to_vec(value.get("data").expect("versioned data checked above"))
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    let next_data = serde_json::to_vec(data).map_err(|error| error.to_string())?;
    if existing.as_deref() == Some(next_data.as_slice()) {
        return Ok(None);
    }
    let document = VersionedDocument {
        schema_version: CURRENT_JSON_SCHEMA_VERSION,
        updated_at: Local::now().to_rfc3339(),
        data,
    };
    let bytes = serde_json::to_vec_pretty(&document)
        .map_err(|error| format!("Could not serialize {store} storage: {error}"))?;
    let backup = if path.exists() {
        Some(create_backup(paths, task_store, store, &path)?)
    } else {
        None
    };
    atomic_replace(&path, &bytes)?;
    Ok(backup.map(|path| path.to_string_lossy().to_string()))
}

/// Reads rebuildable JSON from the platform cache directory. A legacy copy in
/// the config directory is copied and verified once, but intentionally kept in
/// place until a later user-confirmed cleanup window.
pub(crate) fn load_cache_document<T>(paths: &AppPaths, file_name: &str) -> Result<Option<T>, String>
where
    T: DeserializeOwned,
{
    let path = cache_file_path(paths, file_name);
    if !path.exists() {
        copy_legacy_cache_if_needed::<T>(paths, file_name, &path)?;
    }
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Could not read cache {}: {error}", path.display())),
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| format!("Cache {} is invalid JSON: {error}", path.display()))
}

pub(crate) fn save_cache_document<T>(
    paths: &AppPaths,
    file_name: &str,
    data: &T,
) -> Result<(), String>
where
    T: Serialize + ?Sized,
{
    let _write_guard = acquire_write_lock()?;
    let path = cache_file_path(paths, file_name);
    let bytes = serde_json::to_vec_pretty(data)
        .map_err(|error| format!("Could not serialize cache {file_name}: {error}"))?;
    if fs::read(&path).ok().as_deref() == Some(bytes.as_slice()) {
        return Ok(());
    }
    atomic_replace(&path, &bytes)
}

pub(crate) fn list_store_health(paths: &AppPaths) -> Result<Vec<StorageHealth>, String> {
    DURABLE_STORES
        .iter()
        .map(|(store, file_name)| inspect_store(paths, store, file_name))
        .collect()
}

/// Fails before external work starts when a durable store cannot accept writes.
pub(crate) fn ensure_stores_current(paths: &AppPaths, stores: &[&str]) -> Result<(), String> {
    for store in stores {
        let file_name = file_name_for_store(store)?;
        let health = inspect_store(paths, store, file_name)?;
        match health.state {
            StorageState::Missing | StorageState::Current => {}
            StorageState::MigrationRequired => {
                return Err(format!(
                    "storage-migration-required:{store}: Preview and confirm the local data migration before writing."
                ))
            }
            StorageState::RecoveryRequired => {
                return Err(format!(
                    "storage-recovery-required:{store}: Resolve the interrupted local data operation before writing."
                ))
            }
            StorageState::Corrupt => {
                return Err(format!(
                    "storage-corrupt:{store}: Preview and confirm recovery before writing."
                ))
            }
        }
    }
    Ok(())
}

pub(crate) fn inspect_store(
    paths: &AppPaths,
    store: &str,
    file_name: &str,
) -> Result<StorageHealth, String> {
    let path = config_file_path(paths, file_name);
    let latest_backup_path =
        latest_backup(paths, store)?.map(|path| path.to_string_lossy().to_string());
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(StorageHealth {
                store: store.into(),
                path: path.to_string_lossy().to_string(),
                state: StorageState::Missing,
                schema_version: None,
                current_schema_version: CURRENT_JSON_SCHEMA_VERSION,
                source_sha256: None,
                latest_backup_path,
                message: "No local data file exists yet.".into(),
            })
        }
        Err(error) => return Err(format!("Could not inspect {}: {error}", path.display())),
    };
    let source_sha256 = sha256_hex(&bytes);
    let value = match serde_json::from_slice::<Value>(&bytes) {
        Ok(value) => value,
        Err(error) => {
            return Ok(StorageHealth {
                store: store.into(),
                path: path.to_string_lossy().to_string(),
                state: StorageState::Corrupt,
                schema_version: None,
                current_schema_version: CURRENT_JSON_SCHEMA_VERSION,
                source_sha256: Some(source_sha256),
                latest_backup_path,
                message: format!("Local data is invalid JSON: {error}"),
            })
        }
    };
    let schema_version = value
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .map(|value| value as u16);
    let current =
        schema_version == Some(CURRENT_JSON_SCHEMA_VERSION) && value.get("data").is_some();
    Ok(StorageHealth {
        store: store.into(),
        path: path.to_string_lossy().to_string(),
        state: if current {
            StorageState::Current
        } else {
            StorageState::MigrationRequired
        },
        schema_version: schema_version.or(Some(0)),
        current_schema_version: CURRENT_JSON_SCHEMA_VERSION,
        source_sha256: Some(source_sha256),
        latest_backup_path,
        message: if current {
            "Local data schema is current.".into()
        } else {
            "Legacy local data can be read, but writes require a confirmed migration.".into()
        },
    })
}

pub(crate) fn preview_migration(
    paths: &AppPaths,
    store: &str,
) -> Result<StorageMigrationPlan, String> {
    let file_name = file_name_for_store(store)?;
    let health = inspect_store(paths, store, file_name)?;
    if !matches!(health.state, StorageState::MigrationRequired) {
        return Err(format!("{store} storage does not require migration."));
    }
    let source_sha256 = health
        .source_sha256
        .ok_or_else(|| format!("{store} storage has no migration fingerprint."))?;
    let backup_directory = backup_directory(paths, store);
    Ok(StorageMigrationPlan {
        token: migration_token(store, &source_sha256),
        store: store.into(),
        path: health.path,
        source_sha256,
        from_schema_version: health.schema_version.unwrap_or(0),
        to_schema_version: CURRENT_JSON_SCHEMA_VERSION,
        backup_directory: backup_directory.to_string_lossy().to_string(),
        message: format!(
            "Back up and migrate {store} local data to schema {CURRENT_JSON_SCHEMA_VERSION}."
        ),
    })
}

pub(crate) fn apply_migration(
    paths: &AppPaths,
    task_store: &TaskStore,
    plan: &StorageMigrationPlan,
) -> Result<StorageHealth, String> {
    let _write_guard = acquire_write_lock()?;
    let file_name = file_name_for_store(&plan.store)?;
    let path = config_file_path(paths, file_name);
    let bytes = fs::read(&path)
        .map_err(|error| format!("Could not read {} for migration: {error}", path.display()))?;
    let value = serde_json::from_slice::<Value>(&bytes)
        .map_err(|error| format!("Could not parse legacy {} data: {error}", plan.store))?;
    if value.get("schemaVersion").is_some() && value.get("data").is_some() {
        return inspect_store(paths, &plan.store, file_name);
    }
    let current_sha = sha256_hex(&bytes);
    if current_sha != plan.source_sha256
        || plan.token != migration_token(&plan.store, &plan.source_sha256)
    {
        return Err("storage-migration-stale: Local data changed after preview; create a new migration preview.".into());
    }
    create_backup(paths, task_store, &plan.store, &path)?;
    let document = VersionedDocument {
        schema_version: CURRENT_JSON_SCHEMA_VERSION,
        updated_at: Local::now().to_rfc3339(),
        data: value,
    };
    let migrated = serde_json::to_vec_pretty(&document)
        .map_err(|error| format!("Could not serialize migrated {} data: {error}", plan.store))?;
    atomic_replace(&path, &migrated)?;
    inspect_store(paths, &plan.store, file_name)
}

pub(crate) fn preview_restore(paths: &AppPaths, store: &str) -> Result<StorageRestorePlan, String> {
    let file_name = file_name_for_store(store)?;
    let target = config_file_path(paths, file_name);
    let backup = latest_backup(paths, store)?
        .ok_or_else(|| format!("No recovery backup is available for {store}."))?;
    let bytes = fs::read(&backup).map_err(|error| {
        format!(
            "Could not read recovery backup {}: {error}",
            backup.display()
        )
    })?;
    serde_json::from_slice::<Value>(&bytes).map_err(|error| {
        format!(
            "Recovery backup {} is invalid JSON: {error}",
            backup.display()
        )
    })?;
    let backup_sha256 = sha256_hex(&bytes);
    Ok(StorageRestorePlan {
        token: restore_token(store, &backup_sha256),
        store: store.into(),
        target_path: target.to_string_lossy().to_string(),
        backup_path: backup.to_string_lossy().to_string(),
        backup_sha256,
        message: format!(
            "Restore the latest validated {store} backup after preserving the current file."
        ),
    })
}

pub(crate) fn restore_backup(
    paths: &AppPaths,
    task_store: &TaskStore,
    plan: &StorageRestorePlan,
) -> Result<StorageHealth, String> {
    let _write_guard = acquire_write_lock()?;
    let file_name = file_name_for_store(&plan.store)?;
    let target = config_file_path(paths, file_name);
    if target.to_string_lossy() != plan.target_path {
        return Err("storage-restore-stale: The recovery target no longer matches.".into());
    }
    let backup_root = backup_directory(paths, &plan.store);
    let backup = PathBuf::from(&plan.backup_path);
    let canonical_root = fs::canonicalize(&backup_root)
        .map_err(|error| format!("Could not resolve the recovery directory: {error}"))?;
    let canonical_backup = fs::canonicalize(&backup)
        .map_err(|error| format!("Could not resolve the recovery backup: {error}"))?;
    if !canonical_backup.starts_with(&canonical_root) {
        return Err("storage-restore-invalid-path: Recovery backup is outside the managed backup directory.".into());
    }
    let bytes = fs::read(&canonical_backup)
        .map_err(|error| format!("Could not read the recovery backup: {error}"))?;
    let sha = sha256_hex(&bytes);
    if sha != plan.backup_sha256 || plan.token != restore_token(&plan.store, &sha) {
        return Err("storage-restore-stale: Recovery backup changed after preview.".into());
    }
    serde_json::from_slice::<Value>(&bytes)
        .map_err(|error| format!("Recovery backup is invalid JSON: {error}"))?;
    if target.exists() {
        let current = fs::read(&target)
            .map_err(|error| format!("Could not read the current recovery target: {error}"))?;
        if current == bytes {
            task_store.mark_backup_restored(&canonical_backup)?;
            return inspect_store(paths, &plan.store, file_name);
        }
        create_backup(paths, task_store, &plan.store, &target)?;
    }
    atomic_replace(&target, &bytes)?;
    task_store.mark_backup_restored(&canonical_backup)?;
    inspect_store(paths, &plan.store, file_name)
}

pub(super) fn config_file_path(paths: &AppPaths, file_name: &str) -> PathBuf {
    paths.config_file(file_name)
}

fn cache_file_path(paths: &AppPaths, file_name: &str) -> PathBuf {
    paths.cache_file(file_name)
}

fn copy_legacy_cache_if_needed<T>(
    paths: &AppPaths,
    file_name: &str,
    target: &Path,
) -> Result<(), String>
where
    T: DeserializeOwned,
{
    let legacy = config_file_path(paths, file_name);
    copy_legacy_cache_at::<T>(&legacy, target)
}

fn copy_legacy_cache_at<T>(legacy: &Path, target: &Path) -> Result<(), String>
where
    T: DeserializeOwned,
{
    let bytes = match fs::read(&legacy) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "Could not read legacy cache {}: {error}",
                legacy.display()
            ))
        }
    };
    serde_json::from_slice::<T>(&bytes).map_err(|error| {
        format!(
            "Legacy cache {} is invalid JSON and was not copied: {error}",
            legacy.display()
        )
    })?;
    let _write_guard = acquire_write_lock()?;
    if target.exists() {
        return Ok(());
    }
    atomic_replace(target, &bytes)?;
    let copied = fs::read(target).map_err(|error| {
        format!(
            "Could not verify copied cache {}: {error}",
            target.display()
        )
    })?;
    if sha256_hex(&copied) != sha256_hex(&bytes) {
        return Err(format!(
            "Copied cache {} failed SHA-256 verification.",
            target.display()
        ));
    }
    Ok(())
}

pub(super) fn acquire_write_lock() -> Result<MutexGuard<'static, ()>, String> {
    JSON_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "Local JSON write lock was poisoned.".to_string())
}

fn file_name_for_store(store: &str) -> Result<&'static str, String> {
    DURABLE_STORES
        .iter()
        .find_map(|(candidate, file)| (*candidate == store).then_some(*file))
        .ok_or_else(|| format!("Unknown storage domain: {store}"))
}

pub(super) fn backup_directory(paths: &AppPaths, store: &str) -> PathBuf {
    paths.backup_directory(store)
}

pub(super) fn create_backup(
    paths: &AppPaths,
    task_store: &TaskStore,
    store: &str,
    source: &Path,
) -> Result<PathBuf, String> {
    let directory = backup_directory(paths, store);
    fs::create_dir_all(&directory).map_err(|error| {
        format!(
            "Could not create backup directory {}: {error}",
            directory.display()
        )
    })?;
    let timestamp = Local::now().format("%Y%m%d-%H%M%S-%3f");
    let path = directory.join(format!("{store}-{timestamp}.json"));
    fs::copy(source, &path).map_err(|error| {
        format!(
            "Could not back up {} to {}: {error}",
            source.display(),
            path.display()
        )
    })?;
    let bytes = fs::read(&path).map_err(|error| {
        format!(
            "Could not verify backup {} after copying: {error}",
            path.display()
        )
    })?;
    task_store.record_backup(store, &path, &sha256_hex(&bytes))?;
    Ok(path)
}

fn latest_backup(paths: &AppPaths, store: &str) -> Result<Option<PathBuf>, String> {
    let directory = backup_directory(paths, store);
    if !directory.exists() {
        return Ok(None);
    }
    let mut paths = fs::read_dir(&directory)
        .map_err(|error| {
            format!(
                "Could not read backup directory {}: {error}",
                directory.display()
            )
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths.pop())
}

pub(super) fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Storage path {} has no parent directory.", path.display()))?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "Could not create storage directory {}: {error}",
            parent.display()
        )
    })?;
    let temp = parent.join(format!(
        ".{}.codexhub.tmp.{}",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("data"),
        std::process::id()
    ));
    let result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temp)
            .map_err(|error| {
                format!(
                    "Could not create temporary storage file {}: {error}",
                    temp.display()
                )
            })?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| {
                format!(
                    "Could not flush temporary storage file {}: {error}",
                    temp.display()
                )
            })?;
        drop(file);
        let verified = fs::read(&temp).map_err(|error| {
            format!(
                "Could not verify temporary storage file {}: {error}",
                temp.display()
            )
        })?;
        if verified != bytes || serde_json::from_slice::<Value>(&verified).is_err() {
            return Err("Temporary storage file failed checksum or JSON validation.".into());
        }
        replace_path(&temp, path)?;
        sync_parent(parent)?;
        Ok(())
    })();
    if result.is_err() {
        if let Err(error) = fs::remove_file(&temp) {
            eprintln!(
                "Could not clean failed storage temporary file {}: {error}",
                temp.display()
            );
        }
    }
    result
}

#[cfg(windows)]
fn replace_path(temp: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, ReplaceFileW, MOVEFILE_WRITE_THROUGH, REPLACEFILE_WRITE_THROUGH,
    };
    let mut temp_wide = temp
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut target_wide = target
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let ok = unsafe {
        if target.exists() {
            ReplaceFileW(
                target_wide.as_mut_ptr(),
                temp_wide.as_mut_ptr(),
                std::ptr::null(),
                REPLACEFILE_WRITE_THROUGH,
                std::ptr::null(),
                std::ptr::null(),
            )
        } else {
            MoveFileExW(
                temp_wide.as_mut_ptr(),
                target_wide.as_mut_ptr(),
                MOVEFILE_WRITE_THROUGH,
            )
        }
    };
    if ok == 0 {
        return Err(format!(
            "Could not atomically replace {}: {}",
            target.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_path(temp: &Path, target: &Path) -> Result<(), String> {
    fs::rename(temp, target)
        .map_err(|error| format!("Could not atomically replace {}: {error}", target.display()))
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> Result<(), String> {
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| {
            format!(
                "Could not sync storage directory {}: {error}",
                parent.display()
            )
        })
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) -> Result<(), String> {
    Ok(())
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn migration_token(store: &str, sha: &str) -> String {
    sha256_hex(format!("migration:{store}:{sha}:{CURRENT_JSON_SCHEMA_VERSION}").as_bytes())
}

fn restore_token(store: &str, sha: &str) -> String {
    sha256_hex(format!("restore:{store}:{sha}").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "codexhub-json-store-{label}-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    fn app_paths(label: &str) -> AppPaths {
        AppPaths::for_test_root(std::env::temp_dir().join(format!(
            "codexhub-json-store-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        )))
    }

    #[test]
    fn reads_legacy_and_versioned_documents() {
        let path = temp_path("read");
        fs::write(&path, r#"["one"]"#).expect("write legacy fixture");
        let legacy = load_document_at(&path, "test", Vec::<String>::new()).expect("load legacy");
        assert_eq!(legacy.data, vec!["one"]);

        let document = VersionedDocument {
            schema_version: CURRENT_JSON_SCHEMA_VERSION,
            updated_at: "now".into(),
            data: vec!["two"],
        };
        fs::write(
            &path,
            serde_json::to_vec(&document).expect("serialize fixture"),
        )
        .expect("write versioned fixture");
        let current = load_document_at(&path, "test", Vec::<String>::new()).expect("load current");
        assert_eq!(current.data, vec!["two"]);
        fs::remove_file(path).expect("remove fixture");
    }

    #[test]
    fn atomic_replace_keeps_valid_json() {
        let path = temp_path("atomic");
        atomic_replace(&path, br#"{"schemaVersion":1,"updatedAt":"now","data":[]}"#)
            .expect("write atomically");
        let value = serde_json::from_slice::<Value>(&fs::read(&path).expect("read result"))
            .expect("valid json");
        assert_eq!(value["schemaVersion"], 1);
        fs::remove_file(path).expect("remove fixture");
    }

    #[test]
    fn legacy_cache_is_copied_and_verified_without_removing_source() {
        let legacy = temp_path("legacy-cache");
        let target = temp_path("current-cache");
        fs::write(&legacy, br#"{"value":"kept"}"#).expect("write legacy cache");

        copy_legacy_cache_at::<Value>(&legacy, &target).expect("copy legacy cache");

        assert!(legacy.exists());
        assert_eq!(
            fs::read(&legacy).expect("legacy bytes"),
            fs::read(&target).expect("target bytes")
        );
        fs::remove_file(legacy).expect("remove legacy fixture");
        fs::remove_file(target).expect("remove target fixture");
    }

    #[test]
    fn invalid_legacy_cache_is_not_copied() {
        let legacy = temp_path("invalid-legacy-cache");
        let target = temp_path("invalid-current-cache");
        fs::write(&legacy, b"{invalid").expect("write invalid legacy cache");

        let error = copy_legacy_cache_at::<Value>(&legacy, &target)
            .expect_err("invalid cache should be rejected");

        assert!(error.contains("invalid JSON"));
        assert!(!target.exists());
        fs::remove_file(legacy).expect("remove legacy fixture");
    }

    #[test]
    fn cache_migration_uses_cache_dir_and_keeps_the_legacy_source() {
        let paths = app_paths("cache-paths");
        let legacy = paths.config_file("skills-inventory.json");
        fs::create_dir_all(paths.config_dir()).expect("create config directory");
        fs::write(&legacy, br#"{"items":["kept"]}"#).expect("write legacy cache");

        let loaded = load_cache_document::<Value>(&paths, "skills-inventory.json")
            .expect("load migrated cache")
            .expect("cache exists");

        assert_eq!(loaded["items"], serde_json::json!(["kept"]));
        assert!(legacy.exists());
        assert!(paths.cache_file("skills-inventory.json").exists());
    }

    #[test]
    fn migration_is_fingerprinted_backed_up_and_idempotent() {
        let paths = app_paths("migration");
        let task_store = TaskStore::in_memory();
        let target = paths.config_file("settings.json");
        fs::create_dir_all(paths.config_dir()).expect("create config directory");
        fs::write(&target, br#"{"theme":"dark"}"#).expect("write legacy settings");

        let plan = preview_migration(&paths, "settings").expect("preview migration");
        let health = apply_migration(&paths, &task_store, &plan).expect("apply migration");
        assert!(matches!(health.state, StorageState::Current));
        let backups_after_first = fs::read_dir(paths.backup_directory("settings"))
            .expect("read backups")
            .count();
        assert_eq!(backups_after_first, 1);

        let repeated = apply_migration(&paths, &task_store, &plan).expect("repeat migration");
        assert!(matches!(repeated.state, StorageState::Current));
        assert_eq!(
            fs::read_dir(paths.backup_directory("settings"))
                .expect("read backups again")
                .count(),
            backups_after_first
        );
    }

    #[test]
    fn write_preflight_rejects_legacy_data_and_accepts_current_or_missing_stores() {
        let paths = app_paths("write-preflight");
        let task_store = TaskStore::in_memory();
        fs::create_dir_all(paths.config_dir()).expect("create config directory");
        fs::write(paths.config_file("profiles.json"), br#"[]"#).expect("write legacy profiles");

        let error = ensure_stores_current(&paths, &["profiles", "hosts"])
            .expect_err("legacy profiles must block related writes");
        assert!(error.contains("storage-migration-required:profiles"));

        let plan = preview_migration(&paths, "profiles").expect("preview profiles migration");
        apply_migration(&paths, &task_store, &plan).expect("migrate profiles");
        ensure_stores_current(&paths, &["profiles", "hosts"])
            .expect("current profiles and missing hosts should be writable");
    }

    #[test]
    fn stale_migration_preview_does_not_write_or_back_up() {
        let paths = app_paths("stale-migration");
        let task_store = TaskStore::in_memory();
        let target = paths.config_file("settings.json");
        fs::create_dir_all(paths.config_dir()).expect("create config directory");
        fs::write(&target, br#"{"theme":"light"}"#).expect("write legacy settings");
        let plan = preview_migration(&paths, "settings").expect("preview migration");
        fs::write(&target, br#"{"theme":"dark"}"#).expect("change source after preview");

        let error =
            apply_migration(&paths, &task_store, &plan).expect_err("stale migration must fail");
        assert!(error.contains("storage-migration-stale"));
        assert!(!paths.backup_directory("settings").exists());
        assert_eq!(
            fs::read(target).expect("read unchanged source"),
            br#"{"theme":"dark"}"#
        );
    }

    #[test]
    fn versioned_writes_skip_unchanged_data_and_backup_changes() {
        let paths = app_paths("save-document");
        let task_store = TaskStore::in_memory();
        let first = serde_json::json!({ "theme": "light" });
        let changed = serde_json::json!({ "theme": "dark" });

        assert!(
            save_document(&paths, &task_store, "settings", "settings.json", &first)
                .expect("initial write")
                .is_none()
        );
        assert!(
            save_document(&paths, &task_store, "settings", "settings.json", &first)
                .expect("unchanged write")
                .is_none()
        );
        let backup = save_document(&paths, &task_store, "settings", "settings.json", &changed)
            .expect("changed write")
            .expect("changed write backup");
        assert!(Path::new(&backup).exists());
    }

    #[test]
    fn corrupt_storage_never_falls_back_to_a_valid_backup() {
        let paths = app_paths("corrupt");
        let target = paths.config_file("settings.json");
        let backup_root = paths.backup_directory("settings");
        fs::create_dir_all(&backup_root).expect("create backup directory");
        fs::write(&target, b"{invalid").expect("write corrupt target");
        fs::write(
            backup_root.join("settings-99991231-235959-999.json"),
            br#"{"schemaVersion":1,"updatedAt":"backup","data":{"theme":"light"}}"#,
        )
        .expect("write valid backup");

        let error = load_document::<Value>(&paths, "settings", "settings.json", Value::Null)
            .expect_err("corrupt target must fail");
        assert!(error.contains("invalid JSON"));
        assert!(matches!(
            inspect_store(&paths, "settings", "settings.json")
                .expect("inspect corrupt target")
                .state,
            StorageState::Corrupt
        ));
    }

    #[test]
    fn restoring_the_same_backup_twice_is_idempotent() {
        let root = std::env::temp_dir().join(format!(
            "codexhub-restore-idempotent-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let paths = AppPaths::for_test_root(root);
        let task_store = TaskStore::in_memory();
        let target = paths.config_file("settings.json");
        let backup_root = paths.backup_directory("settings");
        fs::create_dir_all(&backup_root).expect("create backup root");
        atomic_replace(
            &target,
            br#"{"schemaVersion":1,"updatedAt":"before","data":{"value":"current"}}"#,
        )
        .expect("write current settings");
        let backup = backup_root.join("settings-99991231-235959-999.json");
        atomic_replace(
            &backup,
            br#"{"schemaVersion":1,"updatedAt":"backup","data":{"value":"restored"}}"#,
        )
        .expect("write recovery backup");

        let plan = preview_restore(&paths, "settings").expect("preview restore");
        restore_backup(&paths, &task_store, &plan).expect("first restore");
        let first_count = fs::read_dir(&backup_root)
            .expect("read backup root")
            .count();
        std::thread::sleep(Duration::from_millis(5));
        restore_backup(&paths, &task_store, &plan).expect("repeat restore");
        let second_count = fs::read_dir(&backup_root)
            .expect("read backup root")
            .count();

        assert_eq!(first_count, 2);
        assert_eq!(second_count, first_count);
        assert_eq!(
            fs::read(target).expect("read restored target"),
            fs::read(backup).expect("read backup")
        );
    }
}
