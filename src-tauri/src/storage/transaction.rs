use super::json_store::{
    acquire_write_lock, atomic_replace, backup_directory, config_file_path, create_backup,
    sha256_hex, VersionedDocument, CURRENT_JSON_SCHEMA_VERSION,
};
use super::{AppPaths, TaskStore};
use chrono::Local;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// A domain service can stage several authoritative JSON documents without
/// coupling the storage layer to Host/Profile DTOs.
pub(crate) struct JsonStoreUpdate {
    store: String,
    file_name: String,
    data: Value,
}

impl JsonStoreUpdate {
    pub(crate) fn new<T>(store: &str, file_name: &str, data: &T) -> Result<Self, String>
    where
        T: Serialize + ?Sized,
    {
        Ok(Self {
            store: store.to_string(),
            file_name: file_name.to_string(),
            data: serde_json::to_value(data)
                .map_err(|error| format!("Could not stage {store} storage: {error}"))?,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RelatedWriteResult {
    pub(crate) changed_stores: Vec<String>,
    pub(crate) backup_paths: Vec<String>,
}

struct PreparedWrite {
    store: String,
    path: PathBuf,
    original: Option<Vec<u8>>,
    next: Vec<u8>,
}

/// Commits related JSON stores under one operation journal entry. Every next
/// document is serialized and validated before the first authoritative file is
/// replaced. A failed later replacement restores already-committed stores.
pub(crate) fn save_related_documents(
    paths: &AppPaths,
    task_store: &TaskStore,
    operation_id: &str,
    updates: Vec<JsonStoreUpdate>,
) -> Result<RelatedWriteResult, String> {
    save_related_documents_with_replace(paths, task_store, operation_id, updates, atomic_replace)
}

fn save_related_documents_with_replace<F>(
    paths: &AppPaths,
    task_store: &TaskStore,
    operation_id: &str,
    updates: Vec<JsonStoreUpdate>,
    mut replace: F,
) -> Result<RelatedWriteResult, String>
where
    F: FnMut(&Path, &[u8]) -> Result<(), String>,
{
    let _write_guard = acquire_write_lock()?;
    let mut prepared = Vec::new();
    for update in updates {
        let path = config_file_path(paths, &update.file_name);
        let original = match fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(format!(
                    "Could not read {} storage {}: {error}",
                    update.store,
                    path.display()
                ))
            }
        };
        if let Some(bytes) = original.as_deref() {
            let current = serde_json::from_slice::<Value>(bytes).map_err(|error| {
                format!(
                    "{} storage {} is invalid JSON: {error}",
                    update.store,
                    path.display()
                )
            })?;
            let schema = current
                .get("schemaVersion")
                .and_then(Value::as_u64)
                .ok_or_else(|| {
                    format!(
                        "storage-migration-required:{}: Confirm migration before a related write.",
                        update.store
                    )
                })?;
            if schema != u64::from(CURRENT_JSON_SCHEMA_VERSION) || current.get("data").is_none() {
                return Err(format!(
                    "storage-migration-required:{}: Confirm migration before a related write.",
                    update.store
                ));
            }
            if current.get("data") == Some(&update.data) {
                continue;
            }
        }
        let next = serde_json::to_vec_pretty(&VersionedDocument {
            schema_version: CURRENT_JSON_SCHEMA_VERSION,
            updated_at: Local::now().to_rfc3339(),
            data: update.data,
        })
        .map_err(|error| format!("Could not serialize {} storage: {error}", update.store))?;
        serde_json::from_slice::<Value>(&next)
            .map_err(|error| format!("Staged {} storage is invalid JSON: {error}", update.store))?;
        prepared.push(PreparedWrite {
            store: update.store,
            path,
            original,
            next,
        });
    }
    if prepared.is_empty() {
        return Ok(RelatedWriteResult::default());
    }

    let payload = serde_json::json!({
        "stores": prepared.iter().map(|item| item.store.as_str()).collect::<Vec<_>>()
    });
    task_store.begin_operation(
        operation_id,
        "related-json-write",
        &serde_json::to_string(&payload).map_err(|error| error.to_string())?,
    )?;

    let mut backup_paths = Vec::new();
    let mut committed = Vec::new();
    let commit_result = (|| -> Result<(), String> {
        for item in &prepared {
            if item.original.is_some() {
                let backup = create_backup(paths, task_store, &item.store, &item.path)?;
                backup_paths.push(backup.to_string_lossy().to_string());
            }
        }
        for (index, item) in prepared.iter().enumerate() {
            replace(&item.path, &item.next)?;
            committed.push(index);
        }
        Ok(())
    })();

    if let Err(error) = commit_result {
        let recovery = compensate(paths, task_store, &prepared, &committed);
        let status = if recovery.is_ok() {
            "recovered"
        } else {
            "recovery-required"
        };
        task_store.finish_operation(operation_id, status)?;
        return match recovery {
            Ok(()) => Err(format!(
                "Related storage write failed and previous data was restored: {error}"
            )),
            Err(recovery_error) => Err(format!(
                "Related storage write failed: {error}. Recovery also failed: {recovery_error}"
            )),
        };
    }

    task_store.finish_operation(operation_id, "completed")?;
    Ok(RelatedWriteResult {
        changed_stores: prepared.into_iter().map(|item| item.store).collect(),
        backup_paths,
    })
}

fn compensate(
    paths: &AppPaths,
    task_store: &TaskStore,
    prepared: &[PreparedWrite],
    committed: &[usize],
) -> Result<(), String> {
    for index in committed.iter().rev() {
        let item = &prepared[*index];
        if let Some(original) = item.original.as_deref() {
            atomic_replace(&item.path, original)?;
            continue;
        }
        if item.path.exists() {
            let directory = backup_directory(paths, &item.store);
            fs::create_dir_all(&directory).map_err(|error| {
                format!(
                    "Could not create recovery directory {}: {error}",
                    directory.display()
                )
            })?;
            let preserved = directory.join(format!(
                "{}-rollback-{}.json",
                item.store,
                Local::now().format("%Y%m%d-%H%M%S-%3f")
            ));
            fs::rename(&item.path, &preserved).map_err(|error| {
                format!(
                    "Could not preserve newly-created {} during recovery: {error}",
                    item.path.display()
                )
            })?;
            let bytes = fs::read(&preserved).map_err(|error| {
                format!(
                    "Could not verify recovery file {}: {error}",
                    preserved.display()
                )
            })?;
            task_store.record_backup(&item.store, &preserved, &sha256_hex(&bytes))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{mpsc, Arc};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn test_paths(label: &str) -> AppPaths {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock")
            .as_nanos();
        AppPaths::for_test_root(std::env::temp_dir().join(format!(
            "codexhub-related-write-{label}-{}-{suffix}",
            std::process::id()
        )))
    }

    fn write_document(paths: &AppPaths, file_name: &str, data: Value) {
        let bytes = serde_json::to_vec_pretty(&VersionedDocument {
            schema_version: CURRENT_JSON_SCHEMA_VERSION,
            updated_at: "2026-07-10T00:00:00+08:00".into(),
            data,
        })
        .expect("serialize fixture");
        atomic_replace(&paths.config_file(file_name), &bytes).expect("write fixture");
    }

    fn read_data(paths: &AppPaths, file_name: &str) -> Value {
        let bytes = fs::read(paths.config_file(file_name)).expect("read fixture");
        serde_json::from_slice::<Value>(&bytes).expect("parse fixture")["data"].clone()
    }

    #[test]
    fn second_replace_failure_restores_first_store_and_closes_journal() {
        let paths = test_paths("rollback");
        let task_store = TaskStore::in_memory();
        write_document(&paths, "profiles.json", json!(["profile-before"]));
        write_document(&paths, "hosts.json", json!(["host-before"]));

        let mut replace_count = 0;
        let error = save_related_documents_with_replace(
            &paths,
            &task_store,
            "operation-rollback",
            vec![
                JsonStoreUpdate::new("profiles", "profiles.json", &json!(["profile-after"]))
                    .expect("stage profiles"),
                JsonStoreUpdate::new("hosts", "hosts.json", &json!(["host-after"]))
                    .expect("stage hosts"),
            ],
            |path, bytes| {
                replace_count += 1;
                if replace_count == 2 {
                    return Err("injected second-store replace failure".into());
                }
                atomic_replace(path, bytes)
            },
        )
        .expect_err("second replace must fail");

        assert!(error.contains("previous data was restored"));
        assert_eq!(
            read_data(&paths, "profiles.json"),
            json!(["profile-before"])
        );
        assert_eq!(read_data(&paths, "hosts.json"), json!(["host-before"]));
        assert!(task_store
            .pending_operation_ids()
            .expect("read journal")
            .is_empty());
    }

    #[test]
    fn related_writes_are_serialized_across_threads() {
        let paths = test_paths("serialization");
        write_document(&paths, "settings.json", json!({ "value": "before" }));
        let task_store = Arc::new(TaskStore::in_memory());
        let (entered_tx, entered_rx) = mpsc::channel::<&'static str>();
        let (release_tx, release_rx) = mpsc::channel::<()>();

        let first_paths = paths.clone();
        let first_store = Arc::clone(&task_store);
        let first_entered = entered_tx.clone();
        let first = thread::spawn(move || {
            save_related_documents_with_replace(
                &first_paths,
                &first_store,
                "operation-first",
                vec![JsonStoreUpdate::new(
                    "settings",
                    "settings.json",
                    &json!({ "value": "first" }),
                )
                .expect("stage first write")],
                |path, bytes| {
                    first_entered.send("first").expect("signal first write");
                    release_rx.recv().expect("release first write");
                    atomic_replace(path, bytes)
                },
            )
        });
        assert_eq!(
            entered_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("first writer entered"),
            "first"
        );

        let second_paths = paths.clone();
        let second_store = Arc::clone(&task_store);
        let second_entered = entered_tx.clone();
        let second = thread::spawn(move || {
            save_related_documents_with_replace(
                &second_paths,
                &second_store,
                "operation-second",
                vec![JsonStoreUpdate::new(
                    "settings",
                    "settings.json",
                    &json!({ "value": "second" }),
                )
                .expect("stage second write")],
                |path, bytes| {
                    second_entered.send("second").expect("signal second write");
                    atomic_replace(path, bytes)
                },
            )
        });

        assert!(entered_rx.recv_timeout(Duration::from_millis(100)).is_err());
        release_tx.send(()).expect("release first writer");
        assert_eq!(
            entered_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("second writer entered"),
            "second"
        );
        first
            .join()
            .expect("join first writer")
            .expect("first write");
        second
            .join()
            .expect("join second writer")
            .expect("second write");
        assert_eq!(
            read_data(&paths, "settings.json"),
            json!({ "value": "second" })
        );
    }
}
