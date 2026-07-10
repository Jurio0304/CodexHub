use crate::adapters::ProfileCredentialAdapter;
use crate::contracts::redact_error_text;

/// Keeps OS credential state and non-secret JSON metadata consistent. The
/// secret value is never formatted into errors, task summaries, or journals.
pub(crate) fn apply_with_metadata<T, A, F>(
    adapter: &A,
    profile_id: &str,
    desired: Option<&str>,
    persist_metadata: F,
) -> Result<T, String>
where
    A: ProfileCredentialAdapter,
    F: FnOnce() -> Result<T, String>,
{
    let previous = adapter.load(profile_id)?;
    apply(adapter, profile_id, desired)?;
    match persist_metadata() {
        Ok(value) => Ok(value),
        Err(metadata_error) => match apply(adapter, profile_id, previous.as_deref()) {
            Ok(()) => Err(format!(
                "Profile credential metadata failed to persist; the credential store was restored: {metadata_error}"
            )),
            Err(rollback_error) => Err(format!(
                "partial-failure: Profile credential metadata failed to persist ({metadata_error}) and credential rollback failed ({rollback_error})."
            )),
        },
    }
}

/// Applies a credential batch before committing its non-secret metadata. Every
/// prior credential value is captured first so failures can be compensated in
/// reverse order without ever placing secret values in an error message.
pub(crate) fn apply_batch_with_metadata<T, A, F>(
    adapter: &A,
    desired: &[(String, String)],
    persist_metadata: F,
) -> Result<T, String>
where
    A: ProfileCredentialAdapter,
    F: FnOnce() -> Result<T, String>,
{
    let mut snapshots = Vec::with_capacity(desired.len());
    for (profile_id, _) in desired {
        if snapshots
            .iter()
            .any(|(existing_id, _)| existing_id == profile_id)
        {
            return Err(format!(
                "Invalid credential batch: duplicate profile id {profile_id}."
            ));
        }
        snapshots.push((profile_id.clone(), adapter.load(profile_id)?));
    }

    let mut applied = 0usize;
    for (profile_id, value) in desired {
        if let Err(error) = adapter.store(profile_id, value) {
            return Err(batch_failure(
                "Credential batch write failed",
                &error,
                rollback_batch(adapter, &snapshots[..applied]),
            ));
        }
        applied += 1;
    }

    match persist_metadata() {
        Ok(value) => Ok(value),
        Err(error) => Err(batch_failure(
            "Credential batch metadata failed to persist",
            &error,
            rollback_batch(adapter, &snapshots),
        )),
    }
}

fn rollback_batch<A: ProfileCredentialAdapter>(
    adapter: &A,
    snapshots: &[(String, Option<String>)],
) -> Vec<String> {
    let mut failures = Vec::new();
    for (profile_id, previous) in snapshots.iter().rev() {
        if let Err(error) = apply(adapter, profile_id, previous.as_deref()) {
            failures.push(format!("{profile_id}: {}", redact_error_text(&error)));
        }
    }
    failures
}

fn batch_failure(prefix: &str, error: &str, rollback_failures: Vec<String>) -> String {
    let error = redact_error_text(error);
    if rollback_failures.is_empty() {
        format!("{prefix}; all changed credentials were restored: {error}")
    } else {
        format!(
            "partial-failure: {prefix} ({error}); credential rollback failed for {}.",
            rollback_failures.join(", ")
        )
    }
}

fn apply<A: ProfileCredentialAdapter>(
    adapter: &A,
    profile_id: &str,
    value: Option<&str>,
) -> Result<(), String> {
    match value {
        Some(value) => adapter.store(profile_id, value),
        None => adapter.delete(profile_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;

    struct FakeCredentialAdapter {
        value: RefCell<Option<String>>,
        fail_rollback: Cell<bool>,
        write_count: Cell<usize>,
    }

    impl ProfileCredentialAdapter for FakeCredentialAdapter {
        fn load(&self, _profile_id: &str) -> Result<Option<String>, String> {
            Ok(self.value.borrow().clone())
        }

        fn store(&self, _profile_id: &str, value: &str) -> Result<(), String> {
            let next_count = self.write_count.get() + 1;
            self.write_count.set(next_count);
            if self.fail_rollback.get() && next_count > 1 {
                return Err("injected credential rollback failure".into());
            }
            *self.value.borrow_mut() = Some(value.to_string());
            Ok(())
        }

        fn delete(&self, _profile_id: &str) -> Result<(), String> {
            *self.value.borrow_mut() = None;
            Ok(())
        }
    }

    #[test]
    fn metadata_failure_restores_the_previous_credential() {
        let adapter = FakeCredentialAdapter {
            value: RefCell::new(Some("previous-value".into())),
            fail_rollback: Cell::new(false),
            write_count: Cell::new(0),
        };

        let error = apply_with_metadata(&adapter, "profile-1", Some("next-value"), || {
            Err::<(), _>("injected metadata failure".to_string())
        })
        .expect_err("metadata write must fail");

        assert!(error.contains("credential store was restored"));
        assert_eq!(adapter.value.borrow().as_deref(), Some("previous-value"));
        assert!(!error.contains("next-value"));
        assert!(!error.contains("previous-value"));
    }

    #[test]
    fn rollback_failure_is_reported_as_partial_without_secret_values() {
        let adapter = FakeCredentialAdapter {
            value: RefCell::new(Some("previous-value".into())),
            fail_rollback: Cell::new(true),
            write_count: Cell::new(0),
        };

        let error = apply_with_metadata(&adapter, "profile-1", Some("next-value"), || {
            Err::<(), _>("injected metadata failure".to_string())
        })
        .expect_err("rollback must fail");

        assert!(error.contains("partial-failure"));
        assert!(!error.contains("next-value"));
        assert!(!error.contains("previous-value"));
    }

    struct BatchCredentialAdapter {
        values: RefCell<HashMap<String, String>>,
        writes: Cell<usize>,
        fail_on_write: Cell<Option<usize>>,
    }

    impl ProfileCredentialAdapter for BatchCredentialAdapter {
        fn load(&self, profile_id: &str) -> Result<Option<String>, String> {
            Ok(self.values.borrow().get(profile_id).cloned())
        }

        fn store(&self, profile_id: &str, value: &str) -> Result<(), String> {
            let write = self.writes.get() + 1;
            self.writes.set(write);
            if self.fail_on_write.get() == Some(write) {
                return Err("injected batch write failure".into());
            }
            self.values
                .borrow_mut()
                .insert(profile_id.to_string(), value.to_string());
            Ok(())
        }

        fn delete(&self, profile_id: &str) -> Result<(), String> {
            self.values.borrow_mut().remove(profile_id);
            Ok(())
        }
    }

    #[test]
    fn batch_write_failure_restores_every_earlier_credential() {
        let adapter = BatchCredentialAdapter {
            values: RefCell::new(HashMap::from([
                ("profile-a".into(), "old-a".into()),
                ("profile-b".into(), "old-b".into()),
            ])),
            writes: Cell::new(0),
            fail_on_write: Cell::new(Some(2)),
        };
        let metadata_called = Cell::new(false);

        let error = apply_batch_with_metadata(
            &adapter,
            &[
                ("profile-a".into(), "new-a".into()),
                ("profile-b".into(), "new-b".into()),
            ],
            || {
                metadata_called.set(true);
                Ok::<_, String>(())
            },
        )
        .expect_err("second credential write should fail");

        assert!(!metadata_called.get());
        assert_eq!(adapter.values.borrow().get("profile-a").unwrap(), "old-a");
        assert_eq!(adapter.values.borrow().get("profile-b").unwrap(), "old-b");
        assert!(!error.contains("new-a"));
        assert!(!error.contains("old-a"));
    }

    #[test]
    fn metadata_failure_restores_the_complete_credential_batch() {
        let adapter = BatchCredentialAdapter {
            values: RefCell::new(HashMap::from([
                ("profile-a".into(), "old-a".into()),
                ("profile-b".into(), "old-b".into()),
            ])),
            writes: Cell::new(0),
            fail_on_write: Cell::new(None),
        };

        apply_batch_with_metadata(
            &adapter,
            &[
                ("profile-a".into(), "new-a".into()),
                ("profile-b".into(), "new-b".into()),
            ],
            || Err::<(), _>("injected metadata failure".to_string()),
        )
        .expect_err("metadata write should fail");

        assert_eq!(adapter.values.borrow().get("profile-a").unwrap(), "old-a");
        assert_eq!(adapter.values.borrow().get("profile-b").unwrap(), "old-b");
    }
}
