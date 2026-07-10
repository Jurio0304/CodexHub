use crate::storage::{save_related_documents, JsonStoreUpdate, RelatedWriteResult};
use crate::{AppState, Host, Profile};

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
