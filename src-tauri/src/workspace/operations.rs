use super::error::{WorkspaceError, WorkspaceResult};
use super::files::FileSessions;
use super::remote_path;
use super::types::*;
use chrono::{Duration, Local};
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

const TOKEN_TTL_SECONDS: i64 = 60;

#[derive(Clone)]
struct Prepared {
    file_session_id: String,
    host_id: String,
    host_alias: String,
    kind: FileOperationKind,
    source: RemoteFileEntry,
    staging: Option<RemoteFileEntry>,
    destination: Option<String>,
    destination_fingerprint: Option<String>,
    recovery_path: String,
    expires_at: chrono::DateTime<Local>,
}

/// Recovery persistence is deliberately metadata-only. It must not receive
/// terminal output, file bodies, credentials, or local path grants.
pub trait RecoveryPersistence: Send + Sync {
    fn upsert(&self, recovery: &RecoveryDto) -> Result<(), String>;
    fn get(&self, recovery_id: &str) -> Result<Option<RecoveryDto>, String>;
    fn list(&self) -> Result<Vec<RecoveryDto>, String>;
}
pub struct FileOperations {
    prepared: Mutex<HashMap<String, Prepared>>,
    purge_prepared: Mutex<HashMap<String, (String, chrono::DateTime<Local>)>>,
    persistence: Box<dyn RecoveryPersistence>,
}

impl FileOperations {
    pub fn new(persistence: Box<dyn RecoveryPersistence>) -> Self {
        Self {
            prepared: Mutex::new(HashMap::new()),
            purge_prepared: Mutex::new(HashMap::new()),
            persistence,
        }
    }
    pub fn list(&self) -> WorkspaceResult<Vec<RecoveryDto>> {
        self.persistence
            .list()
            .map_err(|error| WorkspaceError::new("recovery-storage-unavailable", error))
    }

    /// Moves clipboard capabilities between Files sessions. All destinations
    /// are copied successfully before any source is removed. Source removal
    /// uses the same recovery journal as an explicit delete, then purges the
    /// temporary recovery after the destination is known to exist.
    pub async fn move_entries(
        &self,
        files: &FileSessions,
        request: CopyFileEntriesRequest,
    ) -> WorkspaceResult<Vec<RemoteFileEntry>> {
        if request.source_entry_refs.is_empty() || request.source_entry_refs.len() > 100 {
            return Err(WorkspaceError::new(
                "invalid-move-selection",
                "Move between Files windows requires between 1 and 100 entries.",
            ));
        }

        let destination_root = files
            .canonicalize_cwd(
                &request.destination_file_session_id,
                &request.destination_path,
            )
            .await?;
        let same_session = request.source_file_session_id == request.destination_file_session_id;
        let mut decisions = Vec::with_capacity(request.source_entry_refs.len());
        let mut movable_refs = Vec::with_capacity(request.source_entry_refs.len());
        for source_ref in &request.source_entry_refs {
            let source = files
                .operation_stat(&request.source_file_session_id, source_ref)
                .await?;
            if same_session && remote_path::parent(&source.path)? == destination_root {
                decisions.push(Some(source));
            } else {
                decisions.push(None);
                movable_refs.push(source_ref.clone());
            }
        }

        let copied = if movable_refs.is_empty() {
            Vec::new()
        } else {
            files
                .copy_entries(CopyFileEntriesRequest {
                    source_file_session_id: request.source_file_session_id.clone(),
                    destination_file_session_id: request.destination_file_session_id,
                    source_entry_refs: movable_refs.clone(),
                    destination_path: destination_root,
                })
                .await?
        };

        for source_ref in movable_refs {
            let prepared = self
                .prepare(
                    files,
                    PrepareFileOperationRequest {
                        file_session_id: request.source_file_session_id.clone(),
                        kind: FileOperationKind::Delete,
                        source_entry_ref: source_ref,
                        destination_path: None,
                        staging_entry_ref: None,
                    },
                )
                .await?;
            let deleted = self
                .confirm(
                    files,
                    ConfirmFileOperationRequest {
                        operation_token: prepared.operation_token,
                    },
                    None,
                )
                .await?;
            // The source has already been removed. If permanent cleanup is
            // unavailable, retain its recovery instead of reporting a failed
            // move that the user might retry and duplicate at the destination.
            if let Ok(purge) = self.prepare_purge(RecoveryIdentityRequest {
                recovery_id: deleted.recovery_id,
            }) {
                let _ = self
                    .purge(
                        files,
                        PurgeRecoveryRequest {
                            purge_token: purge.purge_token,
                        },
                        &request.source_file_session_id,
                        None,
                    )
                    .await;
            }
        }

        let mut copied = copied.into_iter();
        Ok(decisions
            .into_iter()
            .map(|unchanged| unchanged.unwrap_or_else(|| copied.next().expect("copied entry")))
            .collect())
    }

    pub async fn prepare(
        &self,
        files: &FileSessions,
        request: PrepareFileOperationRequest,
    ) -> WorkspaceResult<PreparedFileOperation> {
        let source = files
            .operation_stat(&request.file_session_id, &request.source_entry_ref)
            .await?;
        if !source.writable_name || protected(&source.path) {
            return Err(WorkspaceError::new(
                "protected-file",
                "This remote file cannot be changed by Workspace.",
            ));
        }
        let staging = match request.staging_entry_ref {
            Some(entry_ref) => Some(
                files
                    .operation_stat(&request.file_session_id, &entry_ref)
                    .await?,
            ),
            None => None,
        };
        let mut destination = request
            .destination_path
            .map(|path| normalize_destination(&source.path, &path))
            .transpose()?;
        match request.kind {
            FileOperationKind::Delete => {
                if destination.is_some() || staging.is_some() {
                    return Err(WorkspaceError::new(
                        "invalid-file-operation",
                        "Delete accepts only the source entry.",
                    ));
                }
            }
            FileOperationKind::Rename => {
                if destination.is_none() {
                    return Err(WorkspaceError::new(
                        "missing-destination",
                        "Rename requires a destination path.",
                    ));
                }
            }
            FileOperationKind::Overwrite => {
                if destination.is_none() || staging.is_none() {
                    return Err(WorkspaceError::new(
                        "invalid-file-operation",
                        "Overwrite requires destination and a verified staging entry.",
                    ));
                }
            }
        }
        // Snapshot the existing target before issuing a confirmation token.
        // Confirm repeats this lstat before it creates any backup directory.
        let destination_fingerprint = if request.kind == FileOperationKind::Overwrite {
            let snapshot = files
                .operation_snapshot_overwrite_destination(
                    &request.file_session_id,
                    destination
                        .as_deref()
                        .expect("validated overwrite destination"),
                )
                .await?;
            destination = Some(snapshot.path);
            Some(snapshot.fingerprint)
        } else {
            None
        };
        let recovery_id = format!("recovery-{}", Uuid::new_v4());
        let target = destination.as_deref().unwrap_or(&source.path);
        let parent = remote_path::parent(target)?;
        let backup_root = remote_path::join(parent, ".codexhub-workspace-backups")?;
        let recovery_path = remote_path::join(&backup_root, &recovery_id)?;
        let expires_at = Local::now() + Duration::seconds(TOKEN_TTL_SECONDS);
        let token = format!("fileop-{}", Uuid::new_v4());
        let host_alias = files.operation_host_alias(&request.file_session_id)?;
        let host_id = files.operation_host_id(&request.file_session_id)?;
        let prepared = Prepared {
            file_session_id: request.file_session_id,
            host_id,
            host_alias: host_alias.clone(),
            kind: request.kind,
            source: source.clone(),
            staging,
            destination: destination.clone(),
            destination_fingerprint,
            recovery_path: recovery_path.clone(),
            expires_at,
        };
        self.prepared
            .lock()
            .map_err(lock_error)?
            .insert(token.clone(), prepared);
        let source_path = source.path.clone();
        let requires_destination_backup = destination
            .as_ref()
            .is_some_and(|target| target != &source_path);
        Ok(PreparedFileOperation {
            operation_token: token,
            kind: request.kind,
            host_alias,
            source_path,
            destination_path: destination.clone(),
            recovery_path,
            expires_at: expires_at.to_rfc3339(),
            requires_destination_backup,
        })
    }

    /// Prepare a regular-file replacement from an editor staging entry. The
    /// destination fingerprint is captured before the confirmation token is
    /// returned, and `confirm` rechecks it before the atomic no-replace swap.
    pub async fn prepare_overwrite(
        &self,
        files: &FileSessions,
        file_session_id: &str,
        destination_entry_ref: &str,
        staging_entry_ref: &str,
    ) -> WorkspaceResult<PreparedFileOperation> {
        let destination = files
            .operation_stat(file_session_id, destination_entry_ref)
            .await?
            .path;
        self.prepare(
            files,
            PrepareFileOperationRequest {
                file_session_id: file_session_id.to_owned(),
                kind: FileOperationKind::Overwrite,
                source_entry_ref: destination_entry_ref.to_owned(),
                destination_path: Some(destination),
                staging_entry_ref: Some(staging_entry_ref.to_owned()),
            },
        )
        .await
    }
    /// Gives the command boundary enough stable context to start an auditable
    /// Job Manager task before the one-shot confirmation token is consumed.
    pub fn prepared_host(&self, operation_token: &str) -> WorkspaceResult<(String, String)> {
        let prepared = self
            .prepared
            .lock()
            .map_err(lock_error)?
            .get(operation_token)
            .cloned()
            .ok_or_else(|| {
                WorkspaceError::new(
                    "invalid-operation-token",
                    "Prepare this file operation again.",
                )
            })?;
        Ok((prepared.host_id, prepared.host_alias))
    }

    pub fn get(&self, recovery_id: &str) -> WorkspaceResult<RecoveryDto> {
        self.persistence
            .get(recovery_id)
            .map_err(|error| WorkspaceError::new("recovery-storage-unavailable", error))?
            .ok_or_else(|| WorkspaceError::new("recovery-not-found", "Recovery record not found."))
    }

    pub async fn confirm(
        &self,
        files: &FileSessions,
        request: ConfirmFileOperationRequest,
        task_id: Option<String>,
    ) -> WorkspaceResult<FileOperationResult> {
        let prepared = self
            .prepared
            .lock()
            .map_err(lock_error)?
            .remove(&request.operation_token)
            .ok_or_else(|| {
                WorkspaceError::new(
                    "invalid-operation-token",
                    "Prepare this file operation again.",
                )
            })?;
        if Local::now() > prepared.expires_at {
            return Err(WorkspaceError::new(
                "expired-operation-token",
                "The file-operation preview expired; prepare it again.",
            ));
        }
        files
            .operation_stat(&prepared.file_session_id, &prepared.source.entry_ref)
            .await?;
        if let Some(staging) = &prepared.staging {
            files
                .operation_stat(&prepared.file_session_id, &staging.entry_ref)
                .await?;
        }
        if prepared.kind == FileOperationKind::Overwrite {
            let destination = prepared
                .destination
                .as_deref()
                .expect("validated destination");
            let current = files
                .operation_snapshot_overwrite_destination(&prepared.file_session_id, destination)
                .await?;
            ensure_destination_fingerprint(
                prepared.destination_fingerprint.as_deref(),
                &current.fingerprint,
            )?;
        }
        let backup_path = format!("{}/payload", prepared.recovery_path);
        let recovery_id = prepared
            .recovery_path
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_string();
        // The prepared journal is durable before any remote directory or file
        // is touched. A crash after a successful move therefore leaves a
        // discoverable recovery path rather than an untracked mutation.
        let mut recovery = RecoveryDto {
            recovery_id: recovery_id.clone(),
            host_id: prepared.host_id.clone(),
            host_alias: prepared.host_alias.clone(),
            kind: prepared.kind,
            original_path: if prepared.kind == FileOperationKind::Overwrite {
                prepared.destination.clone().expect("validated destination")
            } else {
                prepared.source.path.clone()
            },
            current_path: match prepared.kind {
                FileOperationKind::Rename => prepared.destination.clone(),
                FileOperationKind::Overwrite => prepared.destination.clone(),
                FileOperationKind::Delete => None,
            },
            backup_path: Some(if prepared.kind == FileOperationKind::Rename {
                prepared.recovery_path.clone()
            } else {
                backup_path.clone()
            }),
            state: RecoveryState::Prepared,
            task_id,
            reason: None,
            created_at: Local::now().to_rfc3339(),
            restored_at: None,
            purged_at: None,
        };
        self.persistence
            .upsert(&recovery)
            .map_err(|e| WorkspaceError::new("recovery-storage-unavailable", e))?;

        let mutation = match prepared.kind {
            FileOperationKind::Delete => {
                match files
                    .operation_create_recovery_dir(
                        &prepared.file_session_id,
                        &prepared.recovery_path,
                    )
                    .await
                {
                    Ok(()) => {
                        files
                            .operation_rename_no_replace(
                                &prepared.file_session_id,
                                &prepared.source.path,
                                &backup_path,
                            )
                            .await
                    }
                    Err(error) => Err(error),
                }
            }
            FileOperationKind::Rename => {
                let destination = prepared.destination.clone().expect("validated destination");
                if files
                    .operation_exists(&prepared.file_session_id, &destination)
                    .await?
                {
                    Err(WorkspaceError::new(
                        "destination-exists",
                        "Rename destination exists; use the overwrite confirmation flow.",
                    ))
                } else {
                    match files
                        .operation_create_recovery_dir(
                            &prepared.file_session_id,
                            &prepared.recovery_path,
                        )
                        .await
                    {
                        Ok(()) => {
                            files
                                .operation_rename_no_replace(
                                    &prepared.file_session_id,
                                    &prepared.source.path,
                                    &destination,
                                )
                                .await
                        }
                        Err(error) => Err(error),
                    }
                }
            }
            FileOperationKind::Overwrite => {
                let destination = prepared.destination.clone().expect("validated destination");
                if !files
                    .operation_exists(&prepared.file_session_id, &destination)
                    .await?
                {
                    Err(WorkspaceError::new(
                        "destination-missing",
                        "Overwrite destination changed; prepare it again.",
                    ))
                } else {
                    let staging = prepared.staging.as_ref().expect("validated staging");
                    let result = files
                        .operation_create_recovery_dir(
                            &prepared.file_session_id,
                            &prepared.recovery_path,
                        )
                        .await;
                    if let Err(error) = result {
                        Err(error)
                    } else if let Err(error) = files
                        .operation_rename_no_replace(
                            &prepared.file_session_id,
                            &destination,
                            &backup_path,
                        )
                        .await
                    {
                        Err(error)
                    } else if let Err(error) = files
                        .operation_rename_no_replace(
                            &prepared.file_session_id,
                            &staging.path,
                            &destination,
                        )
                        .await
                    {
                        // The old destination remains in the backup root. Roll
                        // it back only through the same no-replace primitive.
                        let _ = files
                            .operation_rename_no_replace(
                                &prepared.file_session_id,
                                &backup_path,
                                &destination,
                            )
                            .await;
                        Err(error)
                    } else {
                        Ok(())
                    }
                }
            }
        };
        if let Err(error) = mutation {
            // Keep a prepared journal for an ambiguous mid-operation failure.
            // It contains no file content and lets a later recovery action
            // recheck the actual remote payload before moving anything again.
            recovery.reason = Some(error.code.clone());
            let _ = self.persistence.upsert(&recovery);
            return Err(error);
        }
        recovery.state = RecoveryState::Available;
        self.persistence
            .upsert(&recovery)
            .map_err(|e| WorkspaceError::new("recovery-storage-unavailable", e))?;
        Ok(FileOperationResult {
            recovery_id: recovery.recovery_id.clone(),
            task_id: recovery.task_id.clone(),
            recovery,
        })
    }
    pub async fn restore(
        &self,
        files: &FileSessions,
        request: RecoveryIdentityRequest,
        file_session_id: &str,
        task_id: Option<String>,
    ) -> WorkspaceResult<FileOperationResult> {
        let mut recovery = self.get(&request.recovery_id)?;
        assert_recovery_session(files, file_session_id, &recovery)?;
        if !matches!(
            recovery.state,
            RecoveryState::Prepared | RecoveryState::Available
        ) {
            return Err(WorkspaceError::new(
                "recovery-unavailable",
                "This recovery is no longer available.",
            ));
        }
        let prior_state = recovery.state;
        recovery.state = RecoveryState::Restoring;
        recovery.task_id = task_id;
        recovery.reason = None;
        self.persistence
            .upsert(&recovery)
            .map_err(|e| WorkspaceError::new("recovery-storage-unavailable", e))?;
        let source = match recovery.kind {
            FileOperationKind::Rename => recovery.current_path.clone().ok_or_else(|| {
                WorkspaceError::new(
                    "recovery-unavailable",
                    "Rename recovery has no current path.",
                )
            })?,
            _ => recovery.backup_path.clone().ok_or_else(|| {
                WorkspaceError::new(
                    "recovery-unavailable",
                    "This operation has no backup payload.",
                )
            })?,
        };
        let restore = if files
            .operation_exists(file_session_id, &recovery.original_path)
            .await?
        {
            Err(WorkspaceError::new(
                "restore-destination-exists",
                "Restore destination is occupied.",
            ))
        } else if !files.operation_exists(file_session_id, &source).await? {
            Err(WorkspaceError::new(
                "recovery-payload-missing",
                "Recovery payload changed or is missing.",
            ))
        } else {
            files
                .operation_rename_no_replace(file_session_id, &source, &recovery.original_path)
                .await
        };
        if let Err(error) = restore {
            recovery.state = prior_state;
            recovery.reason = Some(error.code.clone());
            let _ = self.persistence.upsert(&recovery);
            return Err(error);
        }
        recovery.state = RecoveryState::Restored;
        recovery.restored_at = Some(Local::now().to_rfc3339());
        self.persistence
            .upsert(&recovery)
            .map_err(|e| WorkspaceError::new("recovery-storage-unavailable", e))?;
        Ok(FileOperationResult {
            recovery_id: recovery.recovery_id.clone(),
            task_id: recovery.task_id.clone(),
            recovery,
        })
    }
    pub fn prepare_purge(
        &self,
        request: RecoveryIdentityRequest,
    ) -> WorkspaceResult<PreparedRecoveryPurge> {
        let recovery = self.get(&request.recovery_id)?;
        if recovery.state != RecoveryState::Available {
            return Err(WorkspaceError::new(
                "recovery-unavailable",
                "Only a verified available recovery can be permanently removed.",
            ));
        }
        let token = format!("purge-{}", Uuid::new_v4());
        let expires = Local::now() + Duration::seconds(TOKEN_TTL_SECONDS);
        self.purge_prepared
            .lock()
            .map_err(lock_error)?
            .insert(token.clone(), (recovery.recovery_id.clone(), expires));
        Ok(PreparedRecoveryPurge {
            purge_token: token,
            recovery,
            expires_at: expires.to_rfc3339(),
        })
    }

    /// Resolves a second-confirmation token without consuming it, so the Tauri
    /// boundary can reopen the matching host session and start its audit task.
    pub fn prepared_purge_recovery(&self, purge_token: &str) -> WorkspaceResult<RecoveryDto> {
        let (recovery_id, expiry) = self
            .purge_prepared
            .lock()
            .map_err(lock_error)?
            .get(purge_token)
            .cloned()
            .ok_or_else(|| {
                WorkspaceError::new("invalid-purge-token", "Prepare permanent deletion again.")
            })?;
        if Local::now() > expiry {
            return Err(WorkspaceError::new(
                "expired-purge-token",
                "Permanent deletion confirmation expired.",
            ));
        }
        self.get(&recovery_id)
    }

    pub async fn purge(
        &self,
        files: &FileSessions,
        request: PurgeRecoveryRequest,
        file_session_id: &str,
        task_id: Option<String>,
    ) -> WorkspaceResult<RecoveryDto> {
        let (recovery_id, expiry) = self
            .purge_prepared
            .lock()
            .map_err(lock_error)?
            .remove(&request.purge_token)
            .ok_or_else(|| {
                WorkspaceError::new("invalid-purge-token", "Prepare permanent deletion again.")
            })?;
        if Local::now() > expiry {
            return Err(WorkspaceError::new(
                "expired-purge-token",
                "Permanent deletion confirmation expired.",
            ));
        }
        let mut recovery = self.get(&recovery_id)?;
        assert_recovery_session(files, file_session_id, &recovery)?;
        if recovery.state != RecoveryState::Available {
            return Err(WorkspaceError::new(
                "recovery-unavailable",
                "Only a verified available recovery can be permanently removed.",
            ));
        }
        let backup = recovery.backup_path.as_deref().ok_or_else(|| {
            WorkspaceError::new(
                "recovery-unavailable",
                "Recovery backup path is unavailable.",
            )
        })?;
        let root = match recovery.kind {
            FileOperationKind::Rename => backup,
            _ => remote_path::parent(backup)?,
        };
        recovery.state = RecoveryState::PurgePrepared;
        recovery.task_id = task_id;
        recovery.reason = None;
        self.persistence
            .upsert(&recovery)
            .map_err(|e| WorkspaceError::new("recovery-storage-unavailable", e))?;
        if let Err(error) = files
            .operation_purge_recovery(file_session_id, root, &recovery.recovery_id)
            .await
        {
            recovery.state = RecoveryState::Available;
            recovery.reason = Some(error.code.clone());
            let _ = self.persistence.upsert(&recovery);
            return Err(error);
        }
        recovery.state = RecoveryState::Purged;
        recovery.purged_at = Some(Local::now().to_rfc3339());
        self.persistence
            .upsert(&recovery)
            .map_err(|e| WorkspaceError::new("recovery-storage-unavailable", e))?;
        Ok(recovery)
    }
}

/// A recovery can outlive its old SFTP process. Bind it to a newly opened
/// Files session only after both durable host id and alias agree.
fn assert_recovery_session(
    files: &FileSessions,
    file_session_id: &str,
    recovery: &RecoveryDto,
) -> WorkspaceResult<()> {
    files.assert_host(file_session_id, &recovery.host_id)?;
    if files.operation_host_alias(file_session_id)? == recovery.host_alias {
        return Ok(());
    }
    Err(WorkspaceError::new(
        "recovery-host-mismatch",
        "The reopened Files session does not match this recovery host.",
    ))
}

fn normalize_destination(source: &str, destination: &str) -> WorkspaceResult<String> {
    if destination.trim().is_empty() || remote_path::validate_absolute(destination).is_err() {
        return Err(WorkspaceError::new(
            "invalid-destination",
            "Destination must be an absolute normalized Workspace path.",
        ));
    }
    if destination.contains('\0') || protected(destination) {
        return Err(WorkspaceError::new(
            "invalid-destination",
            "Destination path is not permitted.",
        ));
    }
    remote_path::parent(source).map_err(|_| {
        WorkspaceError::new("invalid-source-path", "Source path has no safe parent.")
    })?;
    Ok(destination.into())
}
fn protected(path: &str) -> bool {
    path.contains("/.ssh/")
        || path.ends_with("/.ssh")
        || path.contains("credential")
        || path.ends_with("/.codex-hub/env")
}
fn lock_error<T>(_: std::sync::PoisonError<T>) -> WorkspaceError {
    WorkspaceError::new(
        "workspace-lock-poisoned",
        "Workspace recovery state is unavailable.",
    )
}

fn ensure_destination_fingerprint(expected: Option<&str>, actual: &str) -> WorkspaceResult<()> {
    if expected == Some(actual) {
        Ok(())
    } else {
        Err(WorkspaceError::new(
            "stale-preview",
            "Overwrite destination changed after it was confirmed; prepare it again.",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MemoryRecoveryPersistence {
        values: Mutex<HashMap<String, RecoveryDto>>,
    }

    impl RecoveryPersistence for MemoryRecoveryPersistence {
        fn upsert(&self, recovery: &RecoveryDto) -> Result<(), String> {
            self.values
                .lock()
                .map_err(|_| "recovery lock poisoned".to_string())?
                .insert(recovery.recovery_id.clone(), recovery.clone());
            Ok(())
        }

        fn get(&self, recovery_id: &str) -> Result<Option<RecoveryDto>, String> {
            Ok(self
                .values
                .lock()
                .map_err(|_| "recovery lock poisoned".to_string())?
                .get(recovery_id)
                .cloned())
        }

        fn list(&self) -> Result<Vec<RecoveryDto>, String> {
            Ok(self
                .values
                .lock()
                .map_err(|_| "recovery lock poisoned".to_string())?
                .values()
                .cloned()
                .collect())
        }
    }

    #[test]
    fn protected_paths_cover_ssh_credentials_and_codex_env() {
        for path in [
            "/home/alice/.ssh",
            "/home/alice/.ssh/id_ed25519",
            "/srv/credential-store/token",
            "/home/alice/.codex-hub/env",
        ] {
            assert!(protected(path), "{path} must remain read-only");
        }
        assert!(!protected("/srv/workspace/readme.md"));
    }

    #[test]
    fn destination_rejects_traversal_sensitive_and_non_absolute_paths() {
        for destination in ["", "relative/file", "\0", "/home/alice/.ssh/config"] {
            assert!(normalize_destination("/srv/source.txt", destination).is_err());
        }
        assert_eq!(
            normalize_destination("/srv/source.txt", "/srv/renamed.txt")
                .expect("safe absolute destination"),
            "/srv/renamed.txt"
        );
        assert_eq!(
            normalize_destination("C:/source.txt", "C:/renamed.txt")
                .expect("safe absolute Windows destination"),
            "C:/renamed.txt"
        );
    }

    #[test]
    fn overwrite_destination_fingerprint_mismatch_refuses_before_mutation() {
        assert!(super::ensure_destination_fingerprint(Some("before"), "after").is_err());
        assert!(super::ensure_destination_fingerprint(None, "after").is_err());
        assert!(super::ensure_destination_fingerprint(Some("same"), "same").is_ok());
    }

    #[tokio::test]
    async fn local_clipboard_move_copies_all_entries_before_removing_sources() {
        let root = std::env::temp_dir().join(format!("codexhub-move-{}", Uuid::new_v4()));
        let source = root.join("source");
        let destination = root.join("destination");
        tokio::fs::create_dir_all(&source).await.unwrap();
        tokio::fs::create_dir_all(&destination).await.unwrap();
        tokio::fs::write(source.join("one.txt"), b"one")
            .await
            .unwrap();
        tokio::fs::write(source.join("two.txt"), b"two")
            .await
            .unwrap();

        let files = FileSessions::new(None);
        let local = files
            .open(OpenFilesRequest {
                local: true,
                host_id: "local".into(),
                host_name: "Local files".into(),
                host_alias: String::new(),
            })
            .await
            .unwrap();
        let source_page = files
            .list_directory(ListDirectoryRequest {
                file_session_id: local.session.file_session_id.clone(),
                path: source.to_string_lossy().replace('\\', "/"),
                snapshot_id: None,
                page_token: None,
                sort: Some(FileSortField::Name),
                direction: Some(SortDirection::Asc),
            })
            .await
            .unwrap();
        let operations = FileOperations::new(Box::new(MemoryRecoveryPersistence::default()));
        let moved = operations
            .move_entries(
                &files,
                CopyFileEntriesRequest {
                    source_file_session_id: local.session.file_session_id.clone(),
                    destination_file_session_id: local.session.file_session_id.clone(),
                    source_entry_refs: source_page
                        .entries
                        .iter()
                        .map(|entry| entry.entry_ref.clone())
                        .collect(),
                    destination_path: destination.to_string_lossy().replace('\\', "/"),
                },
            )
            .await
            .unwrap();

        assert_eq!(moved.len(), 2);
        assert!(!source.join("one.txt").exists());
        assert!(!source.join("two.txt").exists());
        assert_eq!(
            tokio::fs::read(destination.join("one.txt")).await.unwrap(),
            b"one"
        );
        assert_eq!(
            tokio::fs::read(destination.join("two.txt")).await.unwrap(),
            b"two"
        );

        let no_op = operations
            .move_entries(
                &files,
                CopyFileEntriesRequest {
                    source_file_session_id: local.session.file_session_id.clone(),
                    destination_file_session_id: local.session.file_session_id,
                    source_entry_refs: vec![moved[0].entry_ref.clone()],
                    destination_path: destination.to_string_lossy().replace('\\', "/"),
                },
            )
            .await
            .unwrap();
        assert_eq!(no_op[0].path, moved[0].path);
        assert!(!destination.join("one copy.txt").exists());
        assert!(!destination.join("two copy.txt").exists());

        tokio::fs::remove_dir_all(root).await.unwrap();
    }
}
