//! Workspace owns interactive SSH resources.  Commands are deliberately kept
//! outside this module so the manager can also be shut down from app exit.

mod background_process;
pub mod error;
pub mod events;
pub mod files;
pub mod local_files;
pub mod operations;
mod remote_hash;
pub(crate) mod remote_path;
pub mod terminal;
pub mod transfer_io;
pub mod transfers;
pub mod types;
mod vscode;

use crate::workspace::error::{WorkspaceError, WorkspaceResult};
use crate::workspace::events::WorkspaceEventSink;
use crate::workspace::files::FileSessions;
use crate::workspace::operations::{FileOperations, RecoveryPersistence};
use crate::workspace::terminal::{TerminalAuditSink, TerminalSessions};
use crate::workspace::transfer_io::{
    LocalRecoveryPersistence, SqliteLocalRecoveryCommitter, TransferIo,
};
use crate::workspace::transfers::{TransferPersistence, TransferQueue};
use std::sync::Arc;

/// The single owner of live Workspace resources.  It never reads SSH keys:
/// OpenSSH receives only the user-selected alias and resolves its own config.
pub struct WorkspaceManager {
    pub terminals: TerminalSessions,
    pub files: FileSessions,
    pub transfers: TransferQueue,
    /// Transfer finalization shares the same recovery journal as Files.
    pub operations: Arc<FileOperations>,
    pub transfer_io: TransferIo,
    pub local_recoveries: Arc<SqliteLocalRecoveryCommitter>,
}

impl WorkspaceManager {
    pub fn new(
        transfer_persistence: Arc<dyn TransferPersistence>,
        recovery_persistence: Box<dyn RecoveryPersistence>,
        local_recovery_persistence: Arc<dyn LocalRecoveryPersistence>,
        event_sink: Option<WorkspaceEventSink>,
        terminal_audit: Option<Arc<dyn TerminalAuditSink>>,
    ) -> WorkspaceResult<Self> {
        let operations = Arc::new(FileOperations::new(recovery_persistence));
        let local_recoveries = Arc::new(SqliteLocalRecoveryCommitter::new(
            local_recovery_persistence,
        ));
        Ok(Self {
            terminals: TerminalSessions::new(event_sink.clone(), terminal_audit),
            files: FileSessions::new(event_sink.clone()),
            transfers: TransferQueue::new(transfer_persistence, event_sink)?,
            transfer_io: TransferIo::with_committers(operations.clone(), local_recoveries.clone()),
            operations,
            local_recoveries,
        })
    }

    /// Exit closes PTYs first and then kills sftp subprocesses.  There is no
    /// background daemon and no resource survives a true application quit.
    pub async fn shutdown(&self) -> WorkspaceResult<()> {
        self.terminals.shutdown()?;
        self.files
            .shutdown()
            .await
            .map_err(|error| WorkspaceError::new("workspace-shutdown-failed", error))
    }
}
