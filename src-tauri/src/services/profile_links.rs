use crate::storage::{save_related_documents, JsonStoreUpdate, RelatedWriteResult};
use crate::{AppState, Host, Profile};
use std::sync::MutexGuard;

/// Keeps the read/modify/write window for linked Host/Profile state atomic.
pub(crate) fn acquire_write_lock(state: &AppState) -> Result<MutexGuard<'_, ()>, String> {
    state
        .host_profile_write_lock
        .lock()
        .map_err(|_| "Host/Profile write mutex was poisoned.".to_string())
}

/// Persists the bidirectional Host/Profile link as one journaled operation and
/// publishes the new in-memory snapshots only after both files commit.
pub(crate) fn save(
    state: &AppState,
    operation_id: &str,
    profiles: Vec<Profile>,
    hosts: Vec<Host>,
) -> Result<RelatedWriteResult, String> {
    let result = save_related_documents(
        &state.paths,
        &state.task_store,
        operation_id,
        vec![
            JsonStoreUpdate::new("profiles", "profiles.json", &profiles)?,
            JsonStoreUpdate::new("hosts", "hosts.json", &hosts)?,
        ],
    )?;
    *state.profiles.lock().expect("profiles mutex poisoned") = profiles;
    *state.hosts.lock().expect("hosts mutex poisoned") = hosts;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::TaskStore;
    use std::sync::{mpsc, Arc};
    use std::thread;
    use std::time::Duration;

    #[test]
    fn host_profile_write_lock_serializes_related_state_updates() {
        let state = Arc::new(AppState::new(TaskStore::in_memory()));
        let first_guard = acquire_write_lock(&state).expect("acquire first related write lock");
        let (entered_tx, entered_rx) = mpsc::channel();
        let next_state = Arc::clone(&state);
        let worker = thread::spawn(move || {
            let _guard =
                acquire_write_lock(&next_state).expect("acquire second related write lock");
            entered_tx.send(()).expect("signal second writer");
        });

        assert!(entered_rx.recv_timeout(Duration::from_millis(100)).is_err());
        drop(first_guard);
        entered_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("second writer should continue after the first commit");
        worker.join().expect("join related write worker");
    }
}
