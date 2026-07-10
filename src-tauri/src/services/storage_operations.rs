use crate::storage::{
    self, AppPaths, StorageHealth, StorageMigrationPlan, StorageRestorePlan, TaskStore,
};

/// Applies a fingerprinted migration while keeping the cross-storage journal
/// authoritative if the process stops between backup and replacement.
pub(crate) fn migrate(
    paths: &AppPaths,
    task_store: &TaskStore,
    plan: &StorageMigrationPlan,
) -> Result<StorageHealth, String> {
    run_journaled(
        task_store,
        format!("storage-migration:{}", plan.token),
        "storage-migration",
        plan,
        || storage::apply_migration(paths, task_store, plan),
        "completed",
    )
}

/// Restores only the backup approved by the preview token. The storage layer
/// preserves the current file first and treats a repeated restore as no-op.
pub(crate) fn restore(
    paths: &AppPaths,
    task_store: &TaskStore,
    plan: &StorageRestorePlan,
) -> Result<StorageHealth, String> {
    run_journaled(
        task_store,
        format!("storage-restore:{}", plan.token),
        "storage-restore",
        plan,
        || storage::restore_backup(paths, task_store, plan),
        "recovered",
    )
}

fn run_journaled<T, P, F>(
    task_store: &TaskStore,
    operation_id: String,
    kind: &str,
    plan: &P,
    operation: F,
    success_status: &str,
) -> Result<T, String>
where
    P: serde::Serialize,
    F: FnOnce() -> Result<T, String>,
{
    let payload = serde_json::to_string(plan)
        .map_err(|error| format!("Could not serialize {kind} journal: {error}"))?;
    task_store.begin_operation(&operation_id, kind, &payload)?;
    match operation() {
        Ok(value) => {
            task_store.finish_operation(&operation_id, success_status)?;
            Ok(value)
        }
        Err(operation_error) => {
            if let Err(journal_error) = task_store.finish_operation(&operation_id, "failed") {
                return Err(format!(
                    "{operation_error} The operation journal could not be finalized: {journal_error}"
                ));
            }
            Err(operation_error)
        }
    }
}
