use crate::contracts::ApiResult;
use crate::storage::{self, StorageMigrationPlan, StorageRestorePlan};
use crate::{run_durable_local, services, AppState};
use tauri::State;

#[tauri::command]
pub(crate) fn get_storage_health(
    state: State<'_, AppState>,
) -> ApiResult<Vec<storage::StorageHealth>> {
    let mut health = storage::list_store_health(&state.paths)?;
    let pending_operations = state.task_store.pending_operation_ids()?;
    if !pending_operations.is_empty() {
        health.push(storage::StorageHealth {
            store: "operation-journal".into(),
            path: "codexhub.db".into(),
            state: storage::StorageState::RecoveryRequired,
            schema_version: Some(1),
            current_schema_version: 1,
            source_sha256: None,
            latest_backup_path: None,
            message: format!(
                "{} interrupted storage operation(s) require review in Tasks.",
                pending_operations.len()
            ),
        });
    }
    Ok(health)
}

#[tauri::command]
pub(crate) fn preview_storage_migration(
    state: State<'_, AppState>,
    store: String,
) -> ApiResult<StorageMigrationPlan> {
    storage::preview_migration(&state.paths, store.trim()).map_err(Into::into)
}

#[tauri::command]
pub(crate) fn apply_storage_migration(
    state: State<'_, AppState>,
    plan: StorageMigrationPlan,
) -> ApiResult<storage::StorageHealth> {
    let domain = format!("storage-{}", plan.store);
    run_durable_local(&state, "Migrate local storage", &domain, || {
        services::storage_operations::migrate(&state.paths, &state.task_store, &plan)
    })
    .map_err(Into::into)
}

#[tauri::command]
pub(crate) fn preview_storage_restore(
    state: State<'_, AppState>,
    store: String,
) -> ApiResult<StorageRestorePlan> {
    storage::preview_restore(&state.paths, store.trim()).map_err(Into::into)
}

#[tauri::command]
pub(crate) fn restore_storage_backup(
    state: State<'_, AppState>,
    plan: StorageRestorePlan,
) -> ApiResult<storage::StorageHealth> {
    let domain = format!("storage-{}", plan.store);
    run_durable_local(&state, "Restore local storage", &domain, || {
        services::storage_operations::restore(&state.paths, &state.task_store, &plan)
    })
    .map_err(Into::into)
}
