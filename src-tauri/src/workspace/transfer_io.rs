//! Real transfer I/O is kept separate from commands.  Commands can only hand
//! this module opaque local grants and fresh remote entry references.

use super::error::{WorkspaceError, WorkspaceResult};
use super::files::{FileSessions, TransferFingerprint, TransferStreamStop};
use super::operations::FileOperations;
use super::transfers::TransferQueue;
use super::types::{
    ConfirmFileOperationRequest, ConflictStrategy, FileOperationKind, LocalPathGrantDto,
    LocalTransferRecoveryDto, LocalTransferRecoveryState, PrepareFileOperationRequest, TransferDto,
    TransferState,
};
use super::{remote_hash, remote_path};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::io::AsyncReadExt;
use tokio::time::{sleep, Duration as TokioDuration};
use uuid::Uuid;

const GRANT_TTL_SECONDS: i64 = 60;
const MAX_NETWORK_RETRIES: u8 = 3;
const FINGERPRINT_PREFIX_BYTES: u64 = 64 * 1024;
const LOCAL_HASH_CHUNK_BYTES: usize = 128 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GrantKind {
    UploadFile,
    DownloadDirectory,
}

#[derive(Clone, Debug)]
struct LocalGrant {
    path: PathBuf,
    kind: GrantKind,
    expires_at: DateTime<Utc>,
}

/// One-shot local authority.  Paths stay in Rust memory and are consumed when
/// a transfer is bound; queued state contains only the opaque grant id.
#[derive(Default)]
pub struct LocalGrantStore {
    values: Mutex<HashMap<String, LocalGrant>>,
}

impl LocalGrantStore {
    pub async fn grant_upload_file(&self, path: PathBuf) -> WorkspaceResult<LocalPathGrantDto> {
        // Do not resolve a selected link into an upload source. The later
        // canonical path is safe only after this link check has succeeded.
        let metadata = tokio::fs::symlink_metadata(&path)
            .await
            .map_err(|e| WorkspaceError::new("local-source-unavailable", e.to_string()))?;
        if metadata.file_type().is_symlink() {
            return Err(WorkspaceError::new(
                "upload-source-symlink-not-supported",
                "Upload sources must not be symbolic links.",
            ));
        }
        if metadata.is_dir() {
            return Err(WorkspaceError::new(
                "directory-upload-not-supported",
                "Directory uploads are not available yet. Select regular files; no partial directory tree was queued.",
            ));
        }
        if !metadata.is_file() {
            return Err(WorkspaceError::new(
                "upload-source-not-file",
                "Upload sources must be regular files.",
            ));
        }
        self.insert(path, GrantKind::UploadFile, Some(metadata.len()))
    }

    pub async fn grant_download_directory(
        &self,
        path: PathBuf,
    ) -> WorkspaceResult<LocalPathGrantDto> {
        let metadata = tokio::fs::symlink_metadata(&path)
            .await
            .map_err(|e| WorkspaceError::new("download-target-unavailable", e.to_string()))?;
        if metadata.file_type().is_symlink() {
            return Err(WorkspaceError::new(
                "download-target-symlink-not-supported",
                "Download targets must not be symbolic links.",
            ));
        }
        if !metadata.is_dir() {
            return Err(WorkspaceError::new(
                "download-target-not-directory",
                "Downloads require a selected directory.",
            ));
        }
        self.insert(path, GrantKind::DownloadDirectory, None)
    }

    fn insert(
        &self,
        path: PathBuf,
        kind: GrantKind,
        total_bytes: Option<u64>,
    ) -> WorkspaceResult<LocalPathGrantDto> {
        let canonical = std::fs::canonicalize(&path)
            .map_err(|e| WorkspaceError::new("local-path-unavailable", e.to_string()))?;
        if canonical.to_str().is_none() {
            return Err(WorkspaceError::new(
                "unsupported-local-path-encoding",
                "The selected local path cannot be safely persisted.",
            ));
        }
        let grant_id = format!("grant-{}", Uuid::new_v4());
        let expires_at = Utc::now() + Duration::seconds(GRANT_TTL_SECONDS);
        self.values.lock().map_err(lock_error)?.insert(
            grant_id.clone(),
            LocalGrant {
                path: canonical.clone(),
                kind,
                expires_at,
            },
        );
        Ok(LocalPathGrantDto {
            grant_id,
            display_name: canonical
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or("selected item")
                .to_string(),
            is_directory: kind == GrantKind::DownloadDirectory,
            total_bytes,
            expires_at: expires_at.to_rfc3339(),
        })
    }

    fn consume(&self, id: &str, kind: GrantKind) -> WorkspaceResult<PathBuf> {
        let grant = self
            .values
            .lock()
            .map_err(lock_error)?
            .remove(id)
            .ok_or_else(|| {
                WorkspaceError::new("local-grant-invalid", "Select the local path again.")
            })?;
        if grant.expires_at < Utc::now() {
            return Err(WorkspaceError::new(
                "local-grant-expired",
                "The local path authorization expired; select it again.",
            ));
        }
        if grant.kind != kind {
            return Err(WorkspaceError::new(
                "local-grant-scope-invalid",
                "The local path authorization cannot be used for this transfer.",
            ));
        }
        Ok(grant.path)
    }

    /// Exposes only a safe basename while the authorized local path remains
    /// private. Queue creation uses it for the eventual remote target name.
    pub fn upload_file_name(&self, id: &str) -> WorkspaceResult<String> {
        let values = self.values.lock().map_err(lock_error)?;
        let grant = values.get(id).ok_or_else(|| {
            WorkspaceError::new("local-grant-invalid", "Select the local path again.")
        })?;
        if grant.expires_at < Utc::now() {
            return Err(WorkspaceError::new(
                "local-grant-expired",
                "The local path authorization expired; select it again.",
            ));
        }
        if grant.kind != GrantKind::UploadFile {
            return Err(WorkspaceError::new(
                "local-grant-scope-invalid",
                "The local path authorization cannot be used for this transfer.",
            ));
        }
        grant
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty() && *name != "." && *name != "..")
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                WorkspaceError::new(
                    "unsupported-file-encoding",
                    "The local source name cannot be safely written remotely.",
                )
            })
    }
}

#[derive(Clone, Debug)]
enum TransferPlan {
    Upload {
        file_session_id: String,
        local_source: PathBuf,
        source_fingerprint: LocalFingerprint,
    },
    Download {
        file_session_id: String,
        remote_source: String,
        remote_fingerprint: TransferFingerprint,
        local_directory: PathBuf,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct LocalFingerprint {
    size: u64,
    modified_nanos: Option<u128>,
    prefix_sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum DurableTransferFingerprint {
    Upload(LocalFingerprint),
    Download(TransferFingerprint),
}

/// The runner owns no SSH configuration.  It receives an existing SFTP Files
/// session, so OpenSSH still resolves the user's alias, Include and agent.
/// Main integration supplies this only when it can durably record the backup
/// before moving a user-selected local destination.  This keeps the worker
/// from introducing a hidden local deletion/replacement path.
pub trait LocalRecoveryCommitter: Send + Sync {
    fn replace_with_backup(
        &self,
        transfer: &TransferDto,
        partial: &Path,
        destination: &Path,
    ) -> WorkspaceResult<()>;
}

pub trait LocalRecoveryPersistence: Send + Sync {
    fn create(&self, recovery: &LocalRecoveryRecord) -> Result<(), String>;
    fn get(&self, recovery_id: &str) -> Result<Option<LocalRecoveryRecord>, String>;
    fn list(&self) -> Result<Vec<LocalRecoveryRecord>, String>;
    fn update(&self, recovery: &LocalRecoveryRecord) -> Result<(), String>;
}

#[derive(Clone, Debug)]
pub struct LocalRecoveryRecord {
    pub recovery_id: String,
    pub transfer_id: String,
    pub destination_path: PathBuf,
    pub backup_path: PathBuf,
    pub state: String,
    pub created_at: String,
    pub restored_at: Option<String>,
    pub purged_at: Option<String>,
}

impl LocalRecoveryRecord {
    pub fn dto(&self) -> LocalTransferRecoveryDto {
        LocalTransferRecoveryDto {
            recovery_id: self.recovery_id.clone(),
            transfer_id: self.transfer_id.clone(),
            destination_name: local_display_name(&self.destination_path),
            backup_name: local_display_name(&self.backup_path),
            state: match self.state.as_str() {
                "prepared" => LocalTransferRecoveryState::Prepared,
                "available" => LocalTransferRecoveryState::Available,
                "restored" => LocalTransferRecoveryState::Restored,
                "purged" => LocalTransferRecoveryState::Purged,
                // A damaged journal never gets an optimistic UI state.
                _ => LocalTransferRecoveryState::Failed,
            },
            created_at: self.created_at.clone(),
            restored_at: self.restored_at.clone(),
            purged_at: self.purged_at.clone(),
        }
    }
}

/// Desktop-only local recovery. It records `prepared` durably before moving a
/// selected destination, and restores it immediately if publishing fails.
pub struct SqliteLocalRecoveryCommitter {
    persistence: Arc<dyn LocalRecoveryPersistence>,
    purge_tokens: Mutex<HashMap<String, (String, DateTime<Utc>)>>,
}

impl SqliteLocalRecoveryCommitter {
    pub fn new(persistence: Arc<dyn LocalRecoveryPersistence>) -> Self {
        Self {
            persistence,
            purge_tokens: Mutex::new(HashMap::new()),
        }
    }
    pub fn list(&self) -> WorkspaceResult<Vec<LocalTransferRecoveryDto>> {
        self.persistence
            .list()
            .map(|values| values.into_iter().map(|value| value.dto()).collect())
            .map_err(|error| WorkspaceError::new("local-recovery-storage-unavailable", error))
    }
    pub fn restore(&self, recovery_id: &str) -> WorkspaceResult<LocalTransferRecoveryDto> {
        let mut recovery = self.get_available(recovery_id)?;
        verify_local_recovery_payload(&recovery)?;
        // `Path::exists` treats a dangling symlink as absent.  Restore must
        // never replace any existing filesystem object at the destination.
        match std::fs::symlink_metadata(&recovery.destination_path) {
            Ok(_) => {
                return Err(WorkspaceError::new(
                    "restore-destination-exists",
                    "The local restore destination is occupied.",
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(WorkspaceError::new(
                    "local-recovery-restore-check-failed",
                    error.to_string(),
                ));
            }
        }
        std::fs::rename(&recovery.backup_path, &recovery.destination_path).map_err(|error| {
            WorkspaceError::new("local-recovery-restore-failed", error.to_string())
        })?;
        recovery.state = "restored".into();
        recovery.restored_at = Some(Utc::now().to_rfc3339());
        self.persistence
            .update(&recovery)
            .map_err(|error| WorkspaceError::new("local-recovery-storage-unavailable", error))?;
        Ok(recovery.dto())
    }
    pub fn prepare_purge(
        &self,
        recovery_id: &str,
    ) -> WorkspaceResult<(String, LocalTransferRecoveryDto, DateTime<Utc>)> {
        let recovery = self.get_available(recovery_id)?;
        verify_local_recovery_payload(&recovery)?;
        let token = format!("local-purge-{}", Uuid::new_v4());
        let expiry = Utc::now() + Duration::seconds(GRANT_TTL_SECONDS);
        self.purge_tokens
            .lock()
            .map_err(lock_error)?
            .insert(token.clone(), (recovery_id.to_string(), expiry));
        Ok((token, recovery.dto(), expiry))
    }
    pub fn purge(&self, token: &str) -> WorkspaceResult<()> {
        let (recovery_id, expiry) = self
            .purge_tokens
            .lock()
            .map_err(lock_error)?
            .remove(token)
            .ok_or_else(|| {
                WorkspaceError::new(
                    "invalid-local-purge-token",
                    "Prepare permanent deletion again.",
                )
            })?;
        if Utc::now() > expiry {
            return Err(WorkspaceError::new(
                "expired-local-purge-token",
                "Permanent deletion confirmation expired.",
            ));
        }
        let mut recovery = self.get_available(&recovery_id)?;
        let root = verify_local_recovery_payload(&recovery)?;
        std::fs::remove_dir_all(root).map_err(|error| {
            WorkspaceError::new("local-recovery-purge-failed", error.to_string())
        })?;
        recovery.state = "purged".into();
        recovery.purged_at = Some(Utc::now().to_rfc3339());
        self.persistence
            .update(&recovery)
            .map_err(|error| WorkspaceError::new("local-recovery-storage-unavailable", error))
    }
    fn get_available(&self, recovery_id: &str) -> WorkspaceResult<LocalRecoveryRecord> {
        self.persistence
            .get(recovery_id)
            .map_err(|error| WorkspaceError::new("local-recovery-storage-unavailable", error))?
            .filter(|value| value.state == "available")
            .ok_or_else(|| {
                WorkspaceError::new(
                    "local-recovery-not-available",
                    "Local recovery is not available.",
                )
            })
    }
}

impl LocalRecoveryCommitter for SqliteLocalRecoveryCommitter {
    fn replace_with_backup(
        &self,
        transfer: &TransferDto,
        partial: &Path,
        destination: &Path,
    ) -> WorkspaceResult<()> {
        let metadata = std::fs::symlink_metadata(destination)
            .map_err(|error| WorkspaceError::new("local-destination-stale", error.to_string()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(WorkspaceError::new(
                "local-destination-unsafe",
                "Only regular local files can be replaced.",
            ));
        }
        let parent = destination.parent().ok_or_else(|| {
            WorkspaceError::new(
                "local-destination-invalid",
                "Local destination has no parent.",
            )
        })?;
        let recovery_id = format!("local-recovery-{}", Uuid::new_v4());
        let root = parent
            .join(".codexhub-workspace-backups")
            .join(&recovery_id);
        let backup = root.join("payload");
        std::fs::create_dir_all(&root).map_err(|error| {
            WorkspaceError::new("local-recovery-create-failed", error.to_string())
        })?;
        ensure_local_backup_root(&root, &recovery_id)?;
        let partial_metadata = std::fs::symlink_metadata(partial).map_err(|error| {
            WorkspaceError::new("local-commit-partial-unavailable", error.to_string())
        })?;
        if partial_metadata.file_type().is_symlink() || !partial_metadata.is_file() {
            return Err(WorkspaceError::new(
                "local-commit-partial-unsafe",
                "The verified local download staging file is unavailable.",
            ));
        }
        let mut recovery = LocalRecoveryRecord {
            recovery_id: recovery_id.clone(),
            transfer_id: transfer.transfer_id.clone(),
            destination_path: destination.to_path_buf(),
            backup_path: backup.clone(),
            state: "prepared".into(),
            created_at: Utc::now().to_rfc3339(),
            restored_at: None,
            purged_at: None,
        };
        self.persistence
            .create(&recovery)
            .map_err(|error| WorkspaceError::new("local-recovery-storage-unavailable", error))?;
        if let Err(error) = std::fs::rename(destination, &backup) {
            recovery.state = "failed".into();
            self.persistence
                .update(&recovery)
                .map_err(|storage_error| {
                    WorkspaceError::new("local-recovery-storage-unavailable", storage_error)
                })?;
            return Err(WorkspaceError::new(
                "local-recovery-move-failed",
                error.to_string(),
            ));
        }
        if let Err(error) = std::fs::hard_link(partial, destination) {
            let rollback = std::fs::rename(&backup, destination);
            recovery.state = "failed".into();
            self.persistence
                .update(&recovery)
                .map_err(|storage_error| {
                    WorkspaceError::new("local-recovery-storage-unavailable", storage_error)
                })?;
            if rollback.is_err() {
                return Err(WorkspaceError::new(
                    "local-recovery-rollback-failed",
                    error.to_string(),
                ));
            }
            return Err(WorkspaceError::new(
                "local-commit-failed",
                error.to_string(),
            ));
        }
        recovery.state = "available".into();
        self.persistence
            .update(&recovery)
            .map_err(|error| WorkspaceError::new("local-recovery-storage-unavailable", error))?;
        // The replacement is now safely published through a hard link and
        // its original target has a durable recovery journal. Remove only the
        // app-owned staging name, never the replacement or its backup.
        let _ = std::fs::remove_file(partial);
        Ok(())
    }
}

/// Commands bridge this small interface to Job Manager.  It deliberately
/// contains no file names, bytes, terminal output, or credentials; the task
/// remains an audit trail while high-frequency progress stays in the queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferAuditStage {
    Preflight,
    Transfer,
    Verify,
    Commit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferAuditStatus {
    Running,
    Success,
    Failed,
    Skipped,
    Cancelled,
}

pub trait TransferAuditSink: Send + Sync {
    fn record(&self, task_id: &str, stage: TransferAuditStage, status: TransferAuditStatus);
}

pub struct TransferIo {
    pub local_grants: LocalGrantStore,
    plans: Mutex<HashMap<String, TransferPlan>>,
    /// FileOperations owns recovery journalling and performs the TOCTOU
    /// re-check immediately before replacing an existing remote file.
    file_operations: Option<Arc<FileOperations>>,
    local_recovery: Option<Arc<dyn LocalRecoveryCommitter>>,
    audit: Mutex<Option<Arc<dyn TransferAuditSink>>>,
}

impl Default for TransferIo {
    fn default() -> Self {
        Self {
            local_grants: LocalGrantStore::default(),
            plans: Mutex::new(HashMap::new()),
            file_operations: None,
            local_recovery: None,
            audit: Mutex::new(None),
        }
    }
}

impl TransferIo {
    pub fn with_file_operations(file_operations: Arc<FileOperations>) -> Self {
        Self {
            local_grants: LocalGrantStore::default(),
            plans: Mutex::new(HashMap::new()),
            file_operations: Some(file_operations),
            local_recovery: None,
            audit: Mutex::new(None),
        }
    }

    pub fn with_committers(
        file_operations: Arc<FileOperations>,
        local_recovery: Arc<dyn LocalRecoveryCommitter>,
    ) -> Self {
        Self {
            local_grants: LocalGrantStore::default(),
            plans: Mutex::new(HashMap::new()),
            file_operations: Some(file_operations),
            local_recovery: Some(local_recovery),
            audit: Mutex::new(None),
        }
    }

    pub fn with_audit(self, audit: Arc<dyn TransferAuditSink>) -> Self {
        self.set_audit(audit);
        self
    }

    pub fn set_audit(&self, audit: Arc<dyn TransferAuditSink>) {
        if let Ok(mut current) = self.audit.lock() {
            *current = Some(audit);
        }
    }

    pub fn is_bound(&self, transfer_id: &str) -> WorkspaceResult<bool> {
        Ok(self
            .plans
            .lock()
            .map_err(lock_error)?
            .contains_key(transfer_id))
    }

    pub fn consume_download_directory(&self, grant_id: &str) -> WorkspaceResult<PathBuf> {
        self.local_grants
            .consume(grant_id, GrantKind::DownloadDirectory)
    }
    pub async fn bind_upload(
        &self,
        queue: &TransferQueue,
        transfer: &TransferDto,
        file_session_id: String,
    ) -> WorkspaceResult<()> {
        let local_source = self
            .local_grants
            .consume(&transfer.source_ref, GrantKind::UploadFile)?;
        let source_fingerprint = local_fingerprint(&local_source).await?;
        let durable_fingerprint = encode_durable_fingerprint(DurableTransferFingerprint::Upload(
            source_fingerprint.clone(),
        ))?;
        let staging = upload_staging_path(transfer);
        queue.configure_resume(
            &transfer.transfer_id,
            source_fingerprint.size,
            durable_fingerprint,
            staging,
        )?;
        self.plans.lock().map_err(lock_error)?.insert(
            transfer.transfer_id.clone(),
            TransferPlan::Upload {
                file_session_id,
                local_source,
                source_fingerprint,
            },
        );
        Ok(())
    }

    pub async fn bind_download(
        &self,
        queue: &TransferQueue,
        transfer: &TransferDto,
        files: &FileSessions,
        file_session_id: String,
        local_directory: PathBuf,
    ) -> WorkspaceResult<()> {
        let remote_fingerprint = files
            .transfer_fingerprint(&file_session_id, &transfer.source_ref)
            .await?;
        let partial = download_partial_path(&local_directory, &transfer.transfer_id)?;
        queue.configure_resume(
            &transfer.transfer_id,
            remote_fingerprint.size,
            encode_durable_fingerprint(DurableTransferFingerprint::Download(
                remote_fingerprint.clone(),
            ))?,
            path_to_private_string(&partial)?,
        )?;
        self.plans.lock().map_err(lock_error)?.insert(
            transfer.transfer_id.clone(),
            TransferPlan::Download {
                file_session_id,
                remote_source: transfer.source_ref.clone(),
                remote_fingerprint,
                local_directory,
            },
        );
        Ok(())
    }

    /// A queued row survives restart, whereas its path authority and live
    /// SFTP handles intentionally do not. Resume therefore accepts a fresh
    /// one-shot native grant and binds it only after durable fingerprints and
    /// parent boundaries match the original transfer.
    pub async fn bind_after_restart(
        &self,
        transfer: &TransferDto,
        files: &FileSessions,
        file_session_id: String,
        local_grant_id: &str,
    ) -> WorkspaceResult<()> {
        if self
            .plans
            .lock()
            .map_err(lock_error)?
            .contains_key(&transfer.transfer_id)
        {
            return Ok(());
        }
        let durable = transfer
            .durable_source_fingerprint
            .as_deref()
            .ok_or_else(|| {
                WorkspaceError::new(
                    "transfer-reauthorization-required",
                    "This transfer has no durable source fingerprint; select the source again and restart it.",
                )
            })
            .and_then(decode_durable_fingerprint)?;
        match (transfer.direction, durable) {
            (
                super::types::TransferDirection::Upload,
                DurableTransferFingerprint::Upload(expected),
            ) => {
                let local_source = self
                    .local_grants
                    .consume(local_grant_id, GrantKind::UploadFile)?;
                let current = local_fingerprint(&local_source).await?;
                if current != expected {
                    return Err(WorkspaceError::new(
                        "source-fingerprint-changed",
                        "The newly selected upload source does not match the interrupted transfer.",
                    ));
                }
                self.plans.lock().map_err(lock_error)?.insert(
                    transfer.transfer_id.clone(),
                    TransferPlan::Upload {
                        file_session_id,
                        local_source,
                        source_fingerprint: current,
                    },
                );
                Ok(())
            }
            (
                super::types::TransferDirection::Download,
                DurableTransferFingerprint::Download(expected),
            ) => {
                let local_directory = self
                    .local_grants
                    .consume(local_grant_id, GrantKind::DownloadDirectory)?;
                let partial = private_partial_path(transfer, &local_directory)?;
                ensure_partial_parent(&partial, &local_directory)?;
                let current = files
                    .transfer_fingerprint(&file_session_id, &transfer.source_ref)
                    .await?;
                if current != expected {
                    return Err(WorkspaceError::new(
                        "source-fingerprint-changed",
                        "The remote download source changed while the transfer was interrupted.",
                    ));
                }
                self.plans.lock().map_err(lock_error)?.insert(
                    transfer.transfer_id.clone(),
                    TransferPlan::Download {
                        file_session_id,
                        remote_source: transfer.source_ref.clone(),
                        remote_fingerprint: current,
                        local_directory,
                    },
                );
                Ok(())
            }
            _ => Err(WorkspaceError::new(
                "transfer-reauthorization-required",
                "The interrupted transfer does not match its durable direction and fingerprint.",
            )),
        }
    }

    /// Runs one already-claimed transfer.  The caller can schedule this after
    /// `claim_next`; no synthetic progress is ever emitted.
    pub async fn run_claimed(
        &self,
        queue: &TransferQueue,
        files: &FileSessions,
        transfer: TransferDto,
    ) -> WorkspaceResult<()> {
        let plan = self
            .plans
            .lock()
            .map_err(lock_error)?
            .get(&transfer.transfer_id)
            .cloned()
            .ok_or_else(|| {
                WorkspaceError::new(
                    "transfer-plan-unavailable",
                    "This transfer needs fresh local authorization before it can run.",
                )
            })?;
        let token = queue.worker_token(&transfer.transfer_id)?;
        let result = match plan {
            TransferPlan::Upload {
                file_session_id,
                local_source,
                source_fingerprint,
            } => {
                self.run_upload(
                    queue,
                    files,
                    &transfer,
                    &file_session_id,
                    &local_source,
                    &source_fingerprint,
                    &token,
                )
                .await
            }
            TransferPlan::Download {
                file_session_id,
                remote_source,
                remote_fingerprint,
                local_directory,
            } => {
                self.run_download(
                    queue,
                    files,
                    &transfer,
                    &file_session_id,
                    &remote_source,
                    &remote_fingerprint,
                    &local_directory,
                    &token,
                )
                .await
            }
        };
        if let Err(error) = &result {
            // Commands deliberately do not await workers.  Every unexpected
            // worker error therefore has to converge durable queue state here
            // instead of leaving a transfer permanently `running`.
            let current = queue.list().ok().and_then(|items| {
                items
                    .into_iter()
                    .find(|item| item.transfer_id == transfer.transfer_id)
            });
            match current {
                Some(item) if item.state == TransferState::Pausing => {
                    let _ = queue.mark_paused(&transfer.transfer_id, item.bytes);
                }
                Some(item)
                    if matches!(
                        item.state,
                        TransferState::Cancelled | TransferState::Completed | TransferState::Failed
                    ) => {}
                None => {}
                Some(item) => {
                    let _ = queue.mark_failed(
                        &transfer.transfer_id,
                        error.code.as_str(),
                        item.bytes > 0,
                        (item.bytes > 0).then_some(item.bytes),
                    );
                }
            }
        }
        queue.release_worker(&transfer.transfer_id);
        result
    }

    async fn run_upload(
        &self,
        queue: &TransferQueue,
        files: &FileSessions,
        transfer: &TransferDto,
        file_session_id: &str,
        local_source: &Path,
        expected: &LocalFingerprint,
        token: &tokio_util::sync::CancellationToken,
    ) -> WorkspaceResult<()> {
        if super::files::is_prohibited_path(&transfer.destination_path) {
            return Err(WorkspaceError::new(
                "protected-file",
                "Workspace blocks uploads to protected SSH or credential paths.",
            ));
        }
        self.audit(
            transfer,
            TransferAuditStage::Preflight,
            TransferAuditStatus::Running,
        );
        let current = local_fingerprint(local_source).await?;
        if &current != expected {
            self.audit(
                transfer,
                TransferAuditStage::Preflight,
                TransferAuditStatus::Failed,
            );
            return fail(queue, transfer, "source-fingerprint-changed", false, None);
        }
        if files
            .operation_exists(file_session_id, &transfer.destination_path)
            .await?
        {
            match transfer.conflict_strategy {
                ConflictStrategy::Ask => {
                    self.audit(
                        transfer,
                        TransferAuditStage::Preflight,
                        TransferAuditStatus::Skipped,
                    );
                    queue.mark_waiting_conflict(&transfer.transfer_id)?;
                    return Ok(());
                }
                ConflictStrategy::Skip => {
                    self.audit(
                        transfer,
                        TransferAuditStage::Preflight,
                        TransferAuditStatus::Skipped,
                    );
                    self.cancel_for_strategy(queue, transfer, TransferAuditStage::Preflight)?;
                    return Ok(());
                }
                ConflictStrategy::KeepBoth | ConflictStrategy::ReplaceWithBackup => {}
            }
        }
        self.audit(
            transfer,
            TransferAuditStage::Preflight,
            TransferAuditStatus::Success,
        );
        queue.set_total_and_fingerprint(&transfer.transfer_id, current.size, "source-verified")?;
        let staging = upload_staging_path(transfer);
        let offset = upload_resume_offset(
            files,
            file_session_id,
            local_source,
            &staging,
            expected,
            transfer.resume_offset,
            token,
        )
        .await?;
        queue.set_stream_position(&transfer.transfer_id, offset)?;
        let mut last = offset;
        let mut progress = |bytes| {
            last = bytes;
            queue
                .update_progress(&transfer.transfer_id, bytes, None, None)
                .map(|_| ())
        };
        self.audit(
            transfer,
            TransferAuditStage::Transfer,
            TransferAuditStatus::Running,
        );
        let transfer_result = self
            .retry_upload(
                queue,
                files,
                transfer,
                file_session_id,
                local_source,
                &staging,
                expected,
                offset,
                token,
                &mut progress,
            )
            .await;
        match transfer_result {
            Err(error) => {
                let state = queue
                    .list()?
                    .into_iter()
                    .find(|item| item.transfer_id == transfer.transfer_id)
                    .map(|item| item.state);
                if state == Some(TransferState::Cancelled) {
                    self.audit(
                        transfer,
                        TransferAuditStage::Transfer,
                        TransferAuditStatus::Cancelled,
                    );
                    return Ok(());
                }
                if state == Some(TransferState::Pausing) {
                    return settle_cancel(
                        queue,
                        transfer,
                        queue_offset(queue, &transfer.transfer_id),
                    );
                }
                self.audit(
                    transfer,
                    TransferAuditStage::Transfer,
                    TransferAuditStatus::Failed,
                );
                let offset = queue_offset(queue, &transfer.transfer_id);
                let _ = fail(
                    queue,
                    transfer,
                    error.code.as_str(),
                    offset > 0,
                    (offset > 0).then_some(offset),
                );
                return Err(error);
            }
            Ok(TransferStreamStop::Cancelled) => {
                self.audit(
                    transfer,
                    TransferAuditStage::Transfer,
                    TransferAuditStatus::Cancelled,
                );
                return settle_cancel(queue, transfer, last);
            }
            Ok(TransferStreamStop::Complete) => {}
        }
        self.audit(
            transfer,
            TransferAuditStage::Transfer,
            TransferAuditStatus::Success,
        );
        queue.mark_verifying(&transfer.transfer_id)?;
        self.audit(
            transfer,
            TransferAuditStage::Verify,
            TransferAuditStatus::Running,
        );
        // Transport retries replace the worker token; verification must use
        // the current token so pause/cancel still reaches the SSH hash process.
        let verify_token = queue.worker_token(&transfer.transfer_id)?;
        let verification = async {
            let local_hash = sha256_local(local_source).await?;
            let remote_hash = files
                .transfer_sha256(file_session_id, &staging, &verify_token)
                .await?;
            if local_fingerprint(local_source).await? != current {
                return Err(WorkspaceError::new(
                    "source-fingerprint-changed",
                    "The upload source changed during transfer verification.",
                ));
            }
            Ok(local_hash == remote_hash)
        };
        let verification = tokio::select! {
            biased;
            _ = verify_token.cancelled() => Err(remote_hash::cancelled()),
            result = verification => result,
        };
        if !self.finish_verification(queue, transfer, verification, last)? {
            return Ok(());
        }
        queue.mark_finalizing(&transfer.transfer_id)?;
        self.audit(
            transfer,
            TransferAuditStage::Commit,
            TransferAuditStatus::Running,
        );
        if let Err(error) = self
            .commit_remote_upload(queue, files, transfer, file_session_id, &staging)
            .await
        {
            self.audit(
                transfer,
                TransferAuditStage::Commit,
                TransferAuditStatus::Failed,
            );
            let _ = fail(queue, transfer, error.code.as_str(), true, transfer.total);
            return Err(error);
        }
        self.audit(
            transfer,
            TransferAuditStage::Commit,
            TransferAuditStatus::Success,
        );
        Ok(())
    }

    async fn run_download(
        &self,
        queue: &TransferQueue,
        files: &FileSessions,
        transfer: &TransferDto,
        file_session_id: &str,
        remote_source: &str,
        expected: &TransferFingerprint,
        local_directory: &Path,
        token: &tokio_util::sync::CancellationToken,
    ) -> WorkspaceResult<()> {
        if super::files::is_prohibited_path(remote_source) {
            return Err(WorkspaceError::new(
                "protected-file",
                "Workspace blocks downloads of protected SSH or credential paths.",
            ));
        }
        self.audit(
            transfer,
            TransferAuditStage::Preflight,
            TransferAuditStatus::Running,
        );
        let current = files
            .transfer_fingerprint(file_session_id, remote_source)
            .await?;
        if &current != expected {
            self.audit(
                transfer,
                TransferAuditStage::Preflight,
                TransferAuditStatus::Failed,
            );
            return fail(queue, transfer, "source-fingerprint-changed", false, None);
        }
        let destination = download_destination(local_directory, remote_source)?;
        if destination.exists() {
            match transfer.conflict_strategy {
                ConflictStrategy::Ask => {
                    self.audit(
                        transfer,
                        TransferAuditStage::Preflight,
                        TransferAuditStatus::Skipped,
                    );
                    queue.mark_waiting_conflict(&transfer.transfer_id)?;
                    return Ok(());
                }
                ConflictStrategy::Skip => {
                    self.audit(
                        transfer,
                        TransferAuditStage::Preflight,
                        TransferAuditStatus::Skipped,
                    );
                    self.cancel_for_strategy(queue, transfer, TransferAuditStage::Preflight)?;
                    return Ok(());
                }
                ConflictStrategy::KeepBoth | ConflictStrategy::ReplaceWithBackup => {}
            }
        }
        self.audit(
            transfer,
            TransferAuditStage::Preflight,
            TransferAuditStatus::Success,
        );
        queue.set_total_and_fingerprint(&transfer.transfer_id, current.size, "source-verified")?;
        let partial = private_partial_path(transfer, local_directory)?;
        ensure_partial_parent(&partial, local_directory)?;
        let offset = download_resume_offset(
            files,
            file_session_id,
            remote_source,
            &partial,
            expected,
            transfer.resume_offset,
            token,
        )
        .await?;
        queue.set_stream_position(&transfer.transfer_id, offset)?;
        let mut last = offset;
        let mut progress = |bytes| {
            last = bytes;
            queue
                .update_progress(&transfer.transfer_id, bytes, None, None)
                .map(|_| ())
        };
        self.audit(
            transfer,
            TransferAuditStage::Transfer,
            TransferAuditStatus::Running,
        );
        let transfer_result = self
            .retry_download(
                queue,
                files,
                transfer,
                file_session_id,
                remote_source,
                &partial,
                expected,
                offset,
                token,
                &mut progress,
            )
            .await;
        match transfer_result {
            Err(error) => {
                let state = queue
                    .list()?
                    .into_iter()
                    .find(|item| item.transfer_id == transfer.transfer_id)
                    .map(|item| item.state);
                if state == Some(TransferState::Cancelled) {
                    self.audit(
                        transfer,
                        TransferAuditStage::Transfer,
                        TransferAuditStatus::Cancelled,
                    );
                    return Ok(());
                }
                if state == Some(TransferState::Pausing) {
                    return settle_cancel(
                        queue,
                        transfer,
                        queue_offset(queue, &transfer.transfer_id),
                    );
                }
                self.audit(
                    transfer,
                    TransferAuditStage::Transfer,
                    TransferAuditStatus::Failed,
                );
                let offset = queue_offset(queue, &transfer.transfer_id);
                let _ = fail(
                    queue,
                    transfer,
                    error.code.as_str(),
                    offset > 0,
                    (offset > 0).then_some(offset),
                );
                return Err(error);
            }
            Ok(TransferStreamStop::Cancelled) => {
                self.audit(
                    transfer,
                    TransferAuditStage::Transfer,
                    TransferAuditStatus::Cancelled,
                );
                return settle_cancel(queue, transfer, last);
            }
            Ok(TransferStreamStop::Complete) => {}
        }
        self.audit(
            transfer,
            TransferAuditStage::Transfer,
            TransferAuditStatus::Success,
        );
        queue.mark_verifying(&transfer.transfer_id)?;
        self.audit(
            transfer,
            TransferAuditStage::Verify,
            TransferAuditStatus::Running,
        );
        let verify_token = queue.worker_token(&transfer.transfer_id)?;
        let verification = async {
            let after = files
                .transfer_fingerprint(file_session_id, remote_source)
                .await?;
            if after != current {
                return Err(WorkspaceError::new(
                    "source-fingerprint-changed",
                    "The download source changed before transfer verification.",
                ));
            }
            let local_hash = sha256_local(&partial).await?;
            let remote_hash = files
                .transfer_sha256(file_session_id, remote_source, &verify_token)
                .await?;
            if files
                .transfer_fingerprint(file_session_id, remote_source)
                .await?
                != current
            {
                return Err(WorkspaceError::new(
                    "source-fingerprint-changed",
                    "The download source changed during transfer verification.",
                ));
            }
            Ok(local_hash == remote_hash)
        };
        let verification = tokio::select! {
            biased;
            _ = verify_token.cancelled() => Err(remote_hash::cancelled()),
            result = verification => result,
        };
        if !self.finish_verification(queue, transfer, verification, last)? {
            return Ok(());
        }
        queue.mark_finalizing(&transfer.transfer_id)?;
        self.audit(
            transfer,
            TransferAuditStage::Commit,
            TransferAuditStatus::Running,
        );
        if let Err(error) = self
            .commit_local_download(queue, transfer, &partial, &destination)
            .await
        {
            self.audit(
                transfer,
                TransferAuditStage::Commit,
                TransferAuditStatus::Failed,
            );
            let _ = fail(queue, transfer, error.code.as_str(), true, transfer.total);
            return Err(error);
        }
        self.audit(
            transfer,
            TransferAuditStage::Commit,
            TransferAuditStatus::Success,
        );
        Ok(())
    }

    /// Settle verification before commit, retaining partial bytes on failure
    /// and keeping paused/cancelled work out of the finalization path.
    fn finish_verification(
        &self,
        queue: &TransferQueue,
        transfer: &TransferDto,
        result: WorkspaceResult<bool>,
        offset: u64,
    ) -> WorkspaceResult<bool> {
        let state = queue
            .list()?
            .into_iter()
            .find(|item| item.transfer_id == transfer.transfer_id)
            .map(|item| item.state);
        if matches!(
            state,
            Some(TransferState::Pausing | TransferState::Cancelled)
        ) {
            self.audit(
                transfer,
                TransferAuditStage::Verify,
                if state == Some(TransferState::Cancelled) {
                    TransferAuditStatus::Cancelled
                } else {
                    TransferAuditStatus::Skipped
                },
            );
            settle_cancel(queue, transfer, offset)?;
            return Ok(false);
        }
        match result {
            Ok(true) => {
                self.audit(
                    transfer,
                    TransferAuditStage::Verify,
                    TransferAuditStatus::Success,
                );
                Ok(true)
            }
            Ok(false) => {
                self.audit(
                    transfer,
                    TransferAuditStage::Verify,
                    TransferAuditStatus::Failed,
                );
                fail(
                    queue,
                    transfer,
                    "checksum-mismatch",
                    offset > 0,
                    (offset > 0).then_some(offset),
                )?;
                Ok(false)
            }
            Err(error) => {
                self.audit(
                    transfer,
                    TransferAuditStage::Verify,
                    TransferAuditStatus::Failed,
                );
                fail(
                    queue,
                    transfer,
                    &error.code,
                    offset > 0,
                    (offset > 0).then_some(offset),
                )?;
                Err(error)
            }
        }
    }

    /// Retries only classified transport failures. Every retry reconnects the
    /// SFTP subsystem and verifies the staged prefix before seeking forward.
    async fn retry_upload(
        &self,
        queue: &TransferQueue,
        files: &FileSessions,
        transfer: &TransferDto,
        file_session_id: &str,
        local_source: &Path,
        staging: &str,
        expected: &LocalFingerprint,
        initial_offset: u64,
        initial_token: &tokio_util::sync::CancellationToken,
        progress: &mut (dyn FnMut(u64) -> WorkspaceResult<()> + Send),
    ) -> WorkspaceResult<TransferStreamStop> {
        let mut retry = 0u8;
        let mut token = initial_token.clone();
        let mut offset = initial_offset;
        loop {
            match files
                .transfer_upload(
                    file_session_id,
                    local_source,
                    staging,
                    offset,
                    &token,
                    progress,
                )
                .await
            {
                Ok(result) => return Ok(result),
                Err(error) if error.retryable && retry < MAX_NETWORK_RETRIES => {
                    retry = retry.saturating_add(1);
                    self.wait_for_retry(&token, retry).await?;
                    if local_fingerprint(local_source).await? != *expected {
                        return Err(WorkspaceError::new(
                            "source-fingerprint-changed",
                            "The upload source changed while the transfer was interrupted.",
                        ));
                    }
                    let saved = queue_offset(queue, &transfer.transfer_id);
                    files.reconnect(file_session_id).await?;
                    queue.restart_attempt_preserving_resume(&transfer.transfer_id)?;
                    token = queue.worker_token(&transfer.transfer_id)?;
                    offset = upload_resume_offset(
                        files,
                        file_session_id,
                        local_source,
                        staging,
                        expected,
                        (saved > 0).then_some(saved),
                        &token,
                    )
                    .await?;
                    queue.set_stream_position(&transfer.transfer_id, offset)?;
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn retry_download(
        &self,
        queue: &TransferQueue,
        files: &FileSessions,
        transfer: &TransferDto,
        file_session_id: &str,
        remote_source: &str,
        partial: &Path,
        expected: &TransferFingerprint,
        initial_offset: u64,
        initial_token: &tokio_util::sync::CancellationToken,
        progress: &mut (dyn FnMut(u64) -> WorkspaceResult<()> + Send),
    ) -> WorkspaceResult<TransferStreamStop> {
        let mut retry = 0u8;
        let mut token = initial_token.clone();
        let mut offset = initial_offset;
        loop {
            match files
                .transfer_download(
                    file_session_id,
                    remote_source,
                    partial,
                    offset,
                    &token,
                    progress,
                )
                .await
            {
                Ok(result) => return Ok(result),
                Err(error) if error.retryable && retry < MAX_NETWORK_RETRIES => {
                    retry = retry.saturating_add(1);
                    self.wait_for_retry(&token, retry).await?;
                    let saved = queue_offset(queue, &transfer.transfer_id);
                    files.reconnect(file_session_id).await?;
                    queue.restart_attempt_preserving_resume(&transfer.transfer_id)?;
                    token = queue.worker_token(&transfer.transfer_id)?;
                    let current = files
                        .transfer_fingerprint(file_session_id, remote_source)
                        .await?;
                    if &current != expected {
                        return Err(WorkspaceError::new(
                            "source-fingerprint-changed",
                            "The remote download source changed while the transfer was interrupted.",
                        ));
                    }
                    offset = download_resume_offset(
                        files,
                        file_session_id,
                        remote_source,
                        partial,
                        expected,
                        (saved > 0).then_some(saved),
                        &token,
                    )
                    .await?;
                    queue.set_stream_position(&transfer.transfer_id, offset)?;
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn wait_for_retry(
        &self,
        token: &tokio_util::sync::CancellationToken,
        retry: u8,
    ) -> WorkspaceResult<()> {
        let seconds = retry_delay_seconds(retry);
        tokio::select! {
            _ = token.cancelled() => Err(WorkspaceError::new("transfer-retry-cancelled", "The transfer was cancelled before retry.")),
            _ = sleep(TokioDuration::from_secs(seconds)) => Ok(()),
        }
    }

    fn audit(
        &self,
        transfer: &TransferDto,
        stage: TransferAuditStage,
        status: TransferAuditStatus,
    ) {
        let Some(task_id) = transfer.task_id.as_deref() else {
            return;
        };
        let Ok(audit) = self.audit.lock() else {
            return;
        };
        if let Some(audit) = audit.as_ref() {
            audit.record(task_id, stage, status);
        }
    }

    async fn commit_remote_upload(
        &self,
        queue: &TransferQueue,
        files: &FileSessions,
        transfer: &TransferDto,
        file_session_id: &str,
        staging: &str,
    ) -> WorkspaceResult<()> {
        let operations = self.file_operations.as_ref().ok_or_else(|| {
            WorkspaceError::new(
                "recovery-commit-unavailable",
                "Workspace recovery operations are not initialized.",
            )
        })?;
        let destination_exists = files
            .operation_exists(file_session_id, &transfer.destination_path)
            .await?;
        let strategy = transfer.conflict_strategy;
        if destination_exists && matches!(strategy, ConflictStrategy::Ask | ConflictStrategy::Skip)
        {
            // A new destination appeared after the user had already passed
            // preflight. Finalizing is non-cancellable; retain staging and
            // require a fresh conflict decision rather than changing state.
            return fail(
                queue,
                transfer,
                "destination-conflict-after-verify",
                true,
                transfer.total,
            );
        }
        let staging_entry = files
            .operation_internal_entry(file_session_id, staging)
            .await?;
        let (kind, source_entry_ref, destination_path, staging_entry_ref) =
            if destination_exists && strategy == ConflictStrategy::ReplaceWithBackup {
                let destination_entry = files
                    .operation_internal_entry(file_session_id, &transfer.destination_path)
                    .await?;
                (
                    FileOperationKind::Overwrite,
                    destination_entry.entry_ref,
                    transfer.destination_path.clone(),
                    Some(staging_entry.entry_ref),
                )
            } else {
                let destination_path = if destination_exists {
                    keep_both_remote_path(
                        files,
                        file_session_id,
                        &transfer.destination_path,
                        &transfer.transfer_id,
                    )
                    .await?
                } else {
                    transfer.destination_path.clone()
                };
                (
                    FileOperationKind::Rename,
                    staging_entry.entry_ref,
                    destination_path,
                    None,
                )
            };
        let prepared = operations
            .prepare(
                files,
                PrepareFileOperationRequest {
                    file_session_id: file_session_id.to_string(),
                    kind,
                    source_entry_ref,
                    destination_path: Some(destination_path),
                    staging_entry_ref,
                },
            )
            .await?;
        operations
            .confirm(
                files,
                ConfirmFileOperationRequest {
                    operation_token: prepared.operation_token,
                },
                transfer.task_id.clone(),
            )
            .await?;
        queue.mark_completed(&transfer.transfer_id)?;
        Ok(())
    }

    async fn commit_local_download(
        &self,
        queue: &TransferQueue,
        transfer: &TransferDto,
        partial: &Path,
        destination: &Path,
    ) -> WorkspaceResult<()> {
        let mut target = destination.to_path_buf();
        if target.exists() {
            match transfer.conflict_strategy {
                ConflictStrategy::Ask => {
                    queue.mark_waiting_conflict(&transfer.transfer_id)?;
                    return Ok(());
                }
                ConflictStrategy::Skip => {
                    self.cancel_for_strategy(queue, transfer, TransferAuditStage::Commit)?;
                    return Ok(());
                }
                ConflictStrategy::KeepBoth => {
                    target = keep_both_local_path(destination)?;
                }
                ConflictStrategy::ReplaceWithBackup => {
                    let committer = self.local_recovery.as_ref().ok_or_else(|| {
                        WorkspaceError::new(
                            "local-recovery-commit-required",
                            "Local replacement requires the recovery journal.",
                        )
                    })?;
                    committer.replace_with_backup(transfer, partial, &target)?;
                    queue.mark_completed(&transfer.transfer_id)?;
                    return Ok(());
                }
            }
        }
        // hard_link is an atomic create-if-absent operation on the selected
        // directory's filesystem. It therefore cannot silently replace a
        // file created after the conflict check. Recheck the staging inode so
        // a local race cannot publish a link or special file as a download.
        let partial_metadata = std::fs::symlink_metadata(partial).map_err(|error| {
            WorkspaceError::new("local-commit-partial-unavailable", error.to_string())
        })?;
        if partial_metadata.file_type().is_symlink() || !partial_metadata.is_file() {
            return Err(WorkspaceError::new(
                "local-commit-partial-unsafe",
                "The verified local download staging file is unavailable.",
            ));
        }
        let partial = partial.to_path_buf();
        let partial_for_link = partial.clone();
        let target_for_link = target.clone();
        let created = tokio::task::spawn_blocking(move || {
            std::fs::hard_link(&partial_for_link, &target_for_link)
        })
        .await
        .map_err(|error| WorkspaceError::new("local-commit-failed", error.to_string()))?;
        if let Err(error) = created {
            let code = if error.kind() == std::io::ErrorKind::AlreadyExists {
                "destination-exists"
            } else {
                "local-commit-failed"
            };
            return fail(queue, transfer, code, true, transfer.total);
        }
        queue.mark_completed(&transfer.transfer_id)?;
        // This is an application-owned staging file. The published target is
        // already an independent hard link, so removing the staging name does
        // not remove user data or bypass the recovery path for replacements.
        let _ = tokio::task::spawn_blocking(move || std::fs::remove_file(partial)).await;
        Ok(())
    }

    fn cancel_for_strategy(
        &self,
        queue: &TransferQueue,
        transfer: &TransferDto,
        stage: TransferAuditStage,
    ) -> WorkspaceResult<()> {
        let cancelled = queue.cancel(super::types::TransferIdentityRequest {
            transfer_id: transfer.transfer_id.clone(),
            revision: None,
            file_session_id: None,
            local_grant_id: None,
            restart: false,
        })?;
        self.audit(&cancelled, stage, TransferAuditStatus::Cancelled);
        Ok(())
    }
}

fn settle_cancel(
    queue: &TransferQueue,
    transfer: &TransferDto,
    offset: u64,
) -> WorkspaceResult<()> {
    match queue
        .list()?
        .into_iter()
        .find(|value| value.transfer_id == transfer.transfer_id)
        .map(|value| value.state)
    {
        Some(TransferState::Pausing) => {
            queue.mark_paused(&transfer.transfer_id, offset)?;
            Ok(())
        }
        Some(TransferState::Cancelled) => Ok(()),
        _ => queue
            .mark_interrupted(&transfer.transfer_id, Some(offset))
            .map(|_| ()),
    }
}

fn fail(
    queue: &TransferQueue,
    transfer: &TransferDto,
    code: &str,
    resumable: bool,
    offset: Option<u64>,
) -> WorkspaceResult<()> {
    queue
        .mark_failed(&transfer.transfer_id, code, resumable, offset)
        .map(|_| ())
}

fn download_destination(directory: &Path, remote_source: &str) -> WorkspaceResult<PathBuf> {
    let name = remote_path::file_name(remote_source).map_err(|_| {
        WorkspaceError::new(
            "unsupported-file-encoding",
            "The remote source name cannot be safely written locally.",
        )
    })?;
    Ok(directory.join(name))
}

fn upload_staging_path(transfer: &TransferDto) -> String {
    format!(
        "{}.codexhub-part-{}",
        transfer.destination_path, transfer.transfer_id
    )
}

fn download_partial_path(directory: &Path, transfer_id: &str) -> WorkspaceResult<PathBuf> {
    let path = directory.join(format!(".codexhub-part-{transfer_id}"));
    path_to_private_string(&path)?;
    Ok(path)
}

fn path_to_private_string(path: &Path) -> WorkspaceResult<String> {
    path.to_str().map(str::to_owned).ok_or_else(|| {
        WorkspaceError::new(
            "unsupported-local-path-encoding",
            "The local transfer path is not valid UTF-8.",
        )
    })
}

fn private_partial_path(transfer: &TransferDto, directory: &Path) -> WorkspaceResult<PathBuf> {
    let partial = transfer
        .durable_partial_locator
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or(download_partial_path(directory, &transfer.transfer_id)?);
    let expected_name = format!(".codexhub-part-{}", transfer.transfer_id);
    if partial.file_name().and_then(|value| value.to_str()) != Some(expected_name.as_str()) {
        return Err(WorkspaceError::new(
            "local-partial-invalid",
            "The durable local transfer partial does not match this transfer.",
        ));
    }
    Ok(partial)
}

fn ensure_partial_parent(partial: &Path, directory: &Path) -> WorkspaceResult<()> {
    let parent = partial.parent().ok_or_else(|| {
        WorkspaceError::new(
            "local-partial-invalid",
            "The durable local transfer partial has no parent directory.",
        )
    })?;
    let expected = std::fs::canonicalize(directory)
        .map_err(|error| WorkspaceError::new("download-target-unavailable", error.to_string()))?;
    let actual = std::fs::canonicalize(parent)
        .map_err(|error| WorkspaceError::new("local-partial-invalid", error.to_string()))?;
    if expected != actual {
        return Err(WorkspaceError::new(
            "download-target-mismatch",
            "Select the original download directory to resume this transfer.",
        ));
    }
    Ok(())
}

fn encode_durable_fingerprint(value: DurableTransferFingerprint) -> WorkspaceResult<String> {
    serde_json::to_string(&value).map_err(|error| {
        WorkspaceError::new("transfer-fingerprint-encode-failed", error.to_string())
    })
}

fn decode_durable_fingerprint(value: &str) -> WorkspaceResult<DurableTransferFingerprint> {
    serde_json::from_str(value).map_err(|_| {
        WorkspaceError::new(
            "transfer-reauthorization-required",
            "The saved transfer fingerprint is unavailable; restart with a fresh selection.",
        )
    })
}

fn queue_offset(queue: &TransferQueue, transfer_id: &str) -> u64 {
    queue
        .list()
        .ok()
        .and_then(|items| {
            items
                .into_iter()
                .find(|item| item.transfer_id == transfer_id)
        })
        .map(|item| item.bytes)
        .unwrap_or_default()
}

async fn upload_resume_offset(
    files: &FileSessions,
    file_session_id: &str,
    local_source: &Path,
    staging: &str,
    expected: &LocalFingerprint,
    saved_offset: Option<u64>,
    cancel: &tokio_util::sync::CancellationToken,
) -> WorkspaceResult<u64> {
    let Some(offset) = saved_offset.filter(|offset| *offset > 0 && *offset <= expected.size) else {
        return Ok(0);
    };
    if !files.operation_exists(file_session_id, staging).await? {
        return Ok(0);
    }
    let remote = files.transfer_fingerprint(file_session_id, staging).await?;
    if remote.size != offset {
        return Ok(0);
    }
    let local_hash = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(remote_hash::cancelled()),
        result = sha256_local_prefix(local_source, offset) => result?,
    };
    let remote_hash = files
        .transfer_prefix_sha256(file_session_id, staging, offset, cancel)
        .await?;
    Ok((local_hash == remote_hash)
        .then_some(offset)
        .unwrap_or_default())
}

async fn download_resume_offset(
    files: &FileSessions,
    file_session_id: &str,
    remote_source: &str,
    partial: &Path,
    expected: &TransferFingerprint,
    saved_offset: Option<u64>,
    cancel: &tokio_util::sync::CancellationToken,
) -> WorkspaceResult<u64> {
    let Some(offset) = saved_offset.filter(|offset| *offset > 0 && *offset <= expected.size) else {
        return Ok(0);
    };
    let metadata = match tokio::fs::symlink_metadata(partial).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(WorkspaceError::new(
                "local-partial-unavailable",
                error.to_string(),
            ))
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() != offset {
        return Ok(0);
    }
    let local_hash = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(remote_hash::cancelled()),
        result = sha256_local_prefix(partial, offset) => result?,
    };
    let remote_hash = files
        .transfer_prefix_sha256(file_session_id, remote_source, offset, cancel)
        .await?;
    Ok((local_hash == remote_hash)
        .then_some(offset)
        .unwrap_or_default())
}

async fn keep_both_remote_path(
    files: &FileSessions,
    file_session_id: &str,
    destination: &str,
    transfer_id: &str,
) -> WorkspaceResult<String> {
    let parent = remote_path::parent(destination).map_err(|_| {
        WorkspaceError::new(
            "invalid-destination",
            "The remote destination has no valid parent.",
        )
    })?;
    let name = remote_path::file_name(destination).map_err(|_| {
        WorkspaceError::new(
            "unsupported-file-encoding",
            "The remote destination name is not writable.",
        )
    })?;
    let suffix = transfer_id.strip_prefix("transfer-").unwrap_or(transfer_id);
    let candidate_name = format!("{name}.copy-{}", &suffix[..suffix.len().min(8)]);
    let candidate = remote_path::join(parent, &candidate_name)?;
    if files.operation_exists(file_session_id, &candidate).await? {
        return Err(WorkspaceError::new(
            "keep-both-name-unavailable",
            "Choose a different destination and retry.",
        ));
    }
    Ok(candidate)
}

fn keep_both_local_path(destination: &Path) -> WorkspaceResult<PathBuf> {
    let parent = destination.parent().ok_or_else(|| {
        WorkspaceError::new(
            "invalid-destination",
            "The local destination has no parent.",
        )
    })?;
    let stem = destination
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            WorkspaceError::new(
                "unsupported-file-encoding",
                "The local destination name is not writable.",
            )
        })?;
    let extension = destination
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    for index in 1..=1000 {
        let candidate = parent.join(format!("{stem} ({index}){extension}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(WorkspaceError::new(
        "keep-both-name-unavailable",
        "No available keep-both name was found.",
    ))
}

fn retry_delay_seconds(retry: u8) -> u64 {
    [1u64, 2, 5]
        .get(usize::from(retry.saturating_sub(1)))
        .copied()
        .unwrap_or(5)
}

fn local_display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("local backup")
        .to_string()
}

/// Re-check the persisted recovery boundary before every destructive action.
/// The journal stores only paths that were canonical UTF-8 grants at commit;
/// this also rejects later symlink swaps and a replaced backup payload.
fn verify_local_recovery_payload(recovery: &LocalRecoveryRecord) -> WorkspaceResult<PathBuf> {
    let root = recovery.backup_path.parent().ok_or_else(|| {
        WorkspaceError::new(
            "local-recovery-invalid",
            "Local recovery backup has no parent.",
        )
    })?;
    ensure_local_backup_root(root, &recovery.recovery_id)?;
    if recovery
        .backup_path
        .file_name()
        .and_then(|value| value.to_str())
        != Some("payload")
    {
        return Err(WorkspaceError::new(
            "local-recovery-invalid",
            "Local recovery payload name is invalid.",
        ));
    }
    let destination_name = recovery
        .destination_path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|name| !name.is_empty() && *name != "." && *name != "..")
        .ok_or_else(|| {
            WorkspaceError::new(
                "local-recovery-invalid",
                "Local recovery destination name is invalid.",
            )
        })?;
    let destination_parent = recovery.destination_path.parent().ok_or_else(|| {
        WorkspaceError::new(
            "local-recovery-invalid",
            "Local recovery destination has no parent.",
        )
    })?;
    let destination_parent_metadata = std::fs::symlink_metadata(destination_parent)
        .map_err(|error| WorkspaceError::new("local-recovery-invalid", error.to_string()))?;
    if destination_parent_metadata.file_type().is_symlink() || !destination_parent_metadata.is_dir()
    {
        return Err(WorkspaceError::new(
            "local-recovery-invalid",
            "Local recovery destination parent is not a regular directory.",
        ));
    }
    let backup_parent = root.parent().and_then(Path::parent).ok_or_else(|| {
        WorkspaceError::new("local-recovery-invalid", "Local recovery root is invalid.")
    })?;
    let canonical_destination_parent = std::fs::canonicalize(destination_parent)
        .map_err(|error| WorkspaceError::new("local-recovery-invalid", error.to_string()))?;
    let canonical_backup_parent = std::fs::canonicalize(backup_parent)
        .map_err(|error| WorkspaceError::new("local-recovery-invalid", error.to_string()))?;
    if canonical_destination_parent != canonical_backup_parent {
        return Err(WorkspaceError::new(
            "local-recovery-invalid",
            "Local recovery backup does not belong to the destination directory.",
        ));
    }
    // Keep the checked basename live so a malformed persisted path cannot
    // silently become a directory restore on platform-specific path parsing.
    let _ = destination_name;
    let metadata = std::fs::symlink_metadata(&recovery.backup_path)
        .map_err(|error| WorkspaceError::new("local-recovery-invalid", error.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(WorkspaceError::new(
            "local-recovery-invalid",
            "Local recovery payload is not a regular file.",
        ));
    }
    Ok(root.to_path_buf())
}

fn ensure_local_backup_root(root: &Path, recovery_id: &str) -> WorkspaceResult<()> {
    let backup_directory = root.parent().ok_or_else(|| {
        WorkspaceError::new("local-recovery-invalid", "Local recovery root is invalid.")
    })?;
    let backup_parent = backup_directory.parent().ok_or_else(|| {
        WorkspaceError::new("local-recovery-invalid", "Local recovery root is invalid.")
    })?;
    if root.file_name().and_then(|value| value.to_str()) != Some(recovery_id)
        || root
            .parent()
            .and_then(|value| value.file_name())
            .and_then(|value| value.to_str())
            != Some(".codexhub-workspace-backups")
        || !recovery_id.starts_with("local-recovery-")
    {
        return Err(WorkspaceError::new(
            "local-recovery-invalid",
            "Local recovery path is outside the managed backup root.",
        ));
    }
    let root_metadata = std::fs::symlink_metadata(root)
        .map_err(|error| WorkspaceError::new("local-recovery-invalid", error.to_string()))?;
    let directory_metadata = std::fs::symlink_metadata(backup_directory)
        .map_err(|error| WorkspaceError::new("local-recovery-invalid", error.to_string()))?;
    let parent_metadata = std::fs::symlink_metadata(backup_parent)
        .map_err(|error| WorkspaceError::new("local-recovery-invalid", error.to_string()))?;
    if root_metadata.file_type().is_symlink()
        || !root_metadata.is_dir()
        || directory_metadata.file_type().is_symlink()
        || !directory_metadata.is_dir()
        || parent_metadata.file_type().is_symlink()
        || !parent_metadata.is_dir()
    {
        return Err(WorkspaceError::new(
            "local-recovery-invalid",
            "Local recovery boundary is not a regular directory.",
        ));
    }
    Ok(())
}

async fn local_fingerprint(path: &Path) -> WorkspaceResult<LocalFingerprint> {
    let metadata = tokio::fs::symlink_metadata(path)
        .await
        .map_err(|e| WorkspaceError::new("local-source-unavailable", e.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(WorkspaceError::new(
            "local-source-not-file",
            "The authorized upload source is no longer a regular non-link file.",
        ));
    }
    let modified_nanos = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_nanos());
    Ok(LocalFingerprint {
        size: metadata.len(),
        modified_nanos,
        prefix_sha256: sha256_local_prefix(path, metadata.len().min(FINGERPRINT_PREFIX_BYTES))
            .await?,
    })
}

async fn sha256_local_prefix(path: &Path, length: u64) -> WorkspaceResult<String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|error| WorkspaceError::new("local-source-unavailable", error.to_string()))?;
    let mut remaining = length;
    let mut hash = Sha256::new();
    let mut chunk = vec![0u8; LOCAL_HASH_CHUNK_BYTES];
    while remaining > 0 {
        let limit = remaining.min(chunk.len() as u64) as usize;
        let read = file
            .read(&mut chunk[..limit])
            .await
            .map_err(|error| WorkspaceError::new("local-source-read-failed", error.to_string()))?;
        if read == 0 {
            return Err(WorkspaceError::new(
                "local-prefix-short",
                "The local transfer source is shorter than its verified offset.",
            ));
        }
        remaining = remaining.saturating_sub(read as u64);
        hash.update(&chunk[..read]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

async fn sha256_local(path: &Path) -> WorkspaceResult<String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| WorkspaceError::new("local-source-unavailable", e.to_string()))?;
    let mut hash = Sha256::new();
    let mut chunk = vec![0u8; 128 * 1024];
    loop {
        let read = file
            .read(&mut chunk)
            .await
            .map_err(|e| WorkspaceError::new("local-source-read-failed", e.to_string()))?;
        if read == 0 {
            break;
        }
        hash.update(&chunk[..read]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn lock_error<T>(_: std::sync::PoisonError<T>) -> WorkspaceError {
    WorkspaceError::new(
        "workspace-lock-poisoned",
        "Workspace transfer state is unavailable.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verifying_transfer() -> (TransferQueue, TransferDto) {
        let queue = TransferQueue::in_memory(None);
        let transfer = queue
            .enqueue(super::super::types::EnqueueTransfersRequest {
                file_session_id: "files-test".into(),
                items: vec![super::super::types::TransferDraft {
                    direction: super::super::types::TransferDirection::Upload,
                    host_id: "host-test".into(),
                    host_name: "host-test".into(),
                    host_alias: "host-test".into(),
                    source_ref: "grant-test".into(),
                    local_grant_id: None,
                    destination_path: "/tmp/file".into(),
                    total_bytes: Some(10),
                    conflict_strategy: None,
                }],
            })
            .unwrap()
            .remove(0);
        queue.mark_running(&transfer.transfer_id).unwrap();
        queue
            .update_progress(&transfer.transfer_id, 10, None, None)
            .unwrap();
        queue.mark_verifying(&transfer.transfer_id).unwrap();
        (queue, transfer)
    }

    fn transfer_identity(transfer: &TransferDto) -> super::super::types::TransferIdentityRequest {
        super::super::types::TransferIdentityRequest {
            transfer_id: transfer.transfer_id.clone(),
            revision: None,
            file_session_id: None,
            local_grant_id: None,
            restart: false,
        }
    }

    #[test]
    fn verification_failures_retain_the_partial_offset_and_block_commit() {
        for code in [
            "transfer-verify-unavailable",
            "transfer-verify-timeout",
            "transfer-verify-invalid-digest",
        ] {
            let (queue, transfer) = verifying_transfer();
            let result = TransferIo::default().finish_verification(
                &queue,
                &transfer,
                Err(WorkspaceError::new(code, "Verification failed.")),
                10,
            );
            assert_eq!(result.unwrap_err().code, code);
            let failed = queue.list().unwrap().remove(0);
            assert_eq!(failed.state, TransferState::Failed);
            assert_eq!(failed.error_code.as_deref(), Some(code));
            assert!(failed.resumable);
            assert_eq!(failed.resume_offset, Some(10));
            assert!(queue.mark_finalizing(&transfer.transfer_id).is_err());
        }
        let (queue, transfer) = verifying_transfer();
        assert!(!TransferIo::default()
            .finish_verification(&queue, &transfer, Ok(false), 10)
            .unwrap());
        assert_eq!(
            queue.list().unwrap()[0].error_code.as_deref(),
            Some("checksum-mismatch")
        );
        assert!(queue.mark_finalizing(&transfer.transfer_id).is_err());
    }

    #[test]
    fn pause_and_cancel_during_verification_never_advance_to_commit() {
        let io = TransferIo::default();
        let (queue, transfer) = verifying_transfer();
        queue.pause(transfer_identity(&transfer)).unwrap();
        assert!(!io
            .finish_verification(&queue, &transfer, Ok(true), 10)
            .unwrap());
        let paused = queue.list().unwrap().remove(0);
        assert_eq!(paused.state, TransferState::Paused);
        assert_eq!(paused.resume_offset, Some(10));
        assert!(queue.mark_finalizing(&transfer.transfer_id).is_err());

        let (queue, transfer) = verifying_transfer();
        queue.cancel(transfer_identity(&transfer)).unwrap();
        assert!(!io
            .finish_verification(&queue, &transfer, Err(remote_hash::cancelled()), 10)
            .unwrap());
        assert_eq!(queue.list().unwrap()[0].state, TransferState::Cancelled);
        assert!(queue.mark_finalizing(&transfer.transfer_id).is_err());
    }

    #[test]
    fn download_destination_uses_remote_basename_only() {
        let path =
            download_destination(Path::new("C:/selected"), "/remote/a/bundle.tar.gz").unwrap();
        assert_eq!(path, PathBuf::from("C:/selected").join("bundle.tar.gz"));
        assert!(download_destination(Path::new("C:/selected"), "/remote/..").is_err());
    }

    #[test]
    fn network_retry_backoff_is_bounded() {
        assert_eq!([1u8, 2, 3, 4].map(retry_delay_seconds), [1, 2, 5, 5]);
        assert_eq!(MAX_NETWORK_RETRIES, 3);
    }

    #[test]
    fn durable_upload_fingerprint_round_trips_without_exposing_a_path() {
        let encoded =
            encode_durable_fingerprint(DurableTransferFingerprint::Upload(LocalFingerprint {
                size: 42,
                modified_nanos: Some(7),
                prefix_sha256: "digest".into(),
            }))
            .unwrap();
        let decoded = decode_durable_fingerprint(&encoded).unwrap();
        assert!(
            matches!(decoded, DurableTransferFingerprint::Upload(value) if value.size == 42 && value.prefix_sha256 == "digest")
        );
        assert_eq!(
            decode_durable_fingerprint("not-json").unwrap_err().code,
            "transfer-reauthorization-required"
        );
    }

    #[test]
    fn durable_partial_rejects_a_foreign_file_name() {
        let transfer = TransferDto {
            transfer_id: "transfer-safe".into(),
            batch_id: "batch-safe".into(),
            task_id: None,
            direction: super::super::types::TransferDirection::Download,
            host_id: "host".into(),
            host_name: "host".into(),
            host_alias: "host".into(),
            source_ref: "/remote/report.txt".into(),
            destination_path: "download".into(),
            created_at: "2026-07-30T00:00:00Z".into(),
            updated_at: "2026-07-30T00:00:00Z".into(),
            state: TransferState::Interrupted,
            revision: 1,
            bytes: 4,
            total: Some(8),
            speed: None,
            eta_seconds: None,
            attempt: 1,
            resumable: true,
            resume_offset: Some(4),
            conflict_strategy: ConflictStrategy::Ask,
            conflict_revision: None,
            error_code: Some("interrupted".into()),
            fingerprint_status: Some("source-verified".into()),
            durable_source_fingerprint: None,
            durable_partial_locator: Some("C:/selected/.codexhub-part-other".into()),
        };
        assert_eq!(
            private_partial_path(&transfer, Path::new("C:/selected"))
                .unwrap_err()
                .code,
            "local-partial-invalid"
        );
    }

    #[tokio::test]
    async fn grants_are_opaque_single_use_and_scoped() {
        let root = std::env::temp_dir().join(format!("codexhub-grant-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let source = root.join("source.txt");
        tokio::fs::write(&source, b"ok").await.unwrap();
        let grants = LocalGrantStore::default();
        let grant = grants.grant_upload_file(source).await.unwrap();
        assert!(grant.grant_id.starts_with("grant-"));
        assert!(!grant.display_name.contains(std::path::MAIN_SEPARATOR));
        assert!(grants
            .consume(&grant.grant_id, GrantKind::DownloadDirectory)
            .is_err());
        assert!(grants
            .consume(&grant.grant_id, GrantKind::UploadFile)
            .is_err());
    }

    #[tokio::test]
    async fn directory_upload_is_explicitly_rejected_before_queueing() {
        let grants = LocalGrantStore::default();
        let error = grants
            .grant_upload_file(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
            .await
            .unwrap_err();
        assert_eq!(error.code, "directory-upload-not-supported");
    }

    #[test]
    fn local_recovery_payload_must_stay_with_its_original_parent() {
        let root = std::env::temp_dir().join(format!("codexhub-recovery-{}", Uuid::new_v4()));
        let original_parent = root.join("original");
        let other_parent = root.join("other");
        let recovery_id = format!("local-recovery-{}", Uuid::new_v4());
        let backup_root = original_parent
            .join(".codexhub-workspace-backups")
            .join(&recovery_id);
        std::fs::create_dir_all(&backup_root).unwrap();
        std::fs::create_dir_all(&other_parent).unwrap();
        let payload = backup_root.join("payload");
        std::fs::write(&payload, b"backup").unwrap();
        let record = LocalRecoveryRecord {
            recovery_id,
            transfer_id: "transfer-test".into(),
            destination_path: other_parent.join("report.txt"),
            backup_path: payload,
            state: "available".into(),
            created_at: "test".into(),
            restored_at: None,
            purged_at: None,
        };

        assert_eq!(
            verify_local_recovery_payload(&record).unwrap_err().code,
            "local-recovery-invalid"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn upload_grant_rejects_a_symlink_before_canonicalizing_it() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!("codexhub-link-{}", Uuid::new_v4()));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let source = root.join("source.txt");
        tokio::fs::write(&source, b"ok").await.unwrap();
        let link = root.join("source-link.txt");
        symlink(&source, &link).unwrap();
        let error = LocalGrantStore::default()
            .grant_upload_file(link)
            .await
            .unwrap_err();

        assert_eq!(error.code, "upload-source-symlink-not-supported");
    }
}
