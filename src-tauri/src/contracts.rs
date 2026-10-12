#[cfg(test)]
use crate::domain::*;
use crate::tasks::{TaskRun, TaskStatus};
#[cfg(test)]
use crate::{
    platform::RuntimePlatform,
    resource_monitor::{
        CpuSnapshot, GpuMemoryMode, GpuProcessSnapshot, GpuSnapshot, GpuStatus, GpuVendor,
        HostResourceBatchResult, HostResourceProgressEvent, HostResourceSnapshot,
        HostResourceSshStatus, HostResourceStatus, MemorySnapshot, StorageFilesystemSnapshot,
        StorageSnapshot,
    },
    settings::{
        AppSettings, CloseButtonBehavior, NetworkProxyMode, PlatformAppearance, SettingsSaveResult,
        ThemeChoice, WorkspaceTerminalColorScheme, WorkspaceTerminalCursorStyle,
        WorkspaceTerminalFontFamily, WorkspaceTerminalPreferences,
    },
    ssh::{
        SshConfigHost, SshConfigWriteResult, SshHostDraft, SshKeyGenerationResult, SshKeyInfo,
        SshStatus,
    },
    storage::{StorageHealth, StorageMigrationPlan, StorageRestorePlan, StorageState},
    tasks::{TaskLog, TaskLogLevel, TaskStep, TaskStepStatus},
    updater::{AppUpdateState, AppUpdateStatus},
    workspace::{
        events::{
            FileSearchState, FileSearchUpdatedEvent, SessionHeartbeatEvent, SessionStateEvent,
            TerminalCwdEvent, TerminalOutputEvent, TransferUpdatedEvent,
        },
        types::{
            AttachTerminalRequest, AttachTerminalResult, CancelFileSearchRequest,
            ConfirmFileOperationRequest, ConflictStrategy, CopyFileEntriesRequest,
            CopyFileEntryRequest, CreateDirectoryRequest, CwdSource, EnqueueTransfersRequest,
            FileOperationKind, FileOperationResult, FilePreview, FileSearchStarted, FileSessionDto,
            FileSortField, FileTargetKind, InitialTerminalDirectory, ListDirectoryRequest,
            ListDirectoryResult, LocalPathGrantDto, LocalTransferRecoveryDto,
            LocalTransferRecoveryIdentityRequest, LocalTransferRecoveryState, OpenFilesRequest,
            OpenFolderInVscodeRequest, OpenTerminalRequest, PrepareFileOperationRequest,
            PreparedFileOperation, PreparedLocalTransferRecoveryPurge, PreparedRecoveryPurge,
            PreviewFileRequest, PreviewKind, PurgeLocalTransferRecoveryRequest,
            PurgeRecoveryRequest, RecoveryDto, RecoveryIdentityRequest, RecoveryState,
            RemoteFileEntry, RemoteFileKind, ResolveTransferConflictRequest, SaveTextFileRequest,
            SortDirection, StartFileSearchRequest, TerminalAckRequest, TerminalIdentityRequest,
            TerminalReplayFrame, TerminalResizeRequest, TerminalSessionDto, TerminalState,
            TerminalWriteRequest, TransferDirection, TransferDraft, TransferDto,
            TransferIdentityRequest, TransferSnapshotDto, TransferState,
            ValidateTerminalCwdRequest, ValidatedCwd,
        },
    },
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ApiErrorCode {
    BackendUnavailable,
    InvalidArguments,
    StorageUnavailable,
    StorageCorrupt,
    MigrationRequired,
    OperationFailed,
    PartialFailure,
    Unexpected,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ApiError {
    pub(crate) code: ApiErrorCode,
    pub(crate) message: String,
    pub(crate) retryable: bool,
    pub(crate) task_id: Option<String>,
    pub(crate) recovery_id: Option<String>,
}

pub(crate) type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    #[cfg(test)]
    pub(crate) fn operation(message: impl Into<String>) -> Self {
        Self {
            code: ApiErrorCode::OperationFailed,
            message: redact_error_text(&message.into()),
            retryable: true,
            task_id: None,
            recovery_id: None,
        }
    }
}

impl From<String> for ApiError {
    fn from(message: String) -> Self {
        let (task_id, message) = parse_task_error(&message);
        let lower = message.to_ascii_lowercase();
        let code = if lower.contains("partial-failure") {
            ApiErrorCode::PartialFailure
        } else if lower.contains("storage-migration") || lower.contains("schema") {
            ApiErrorCode::MigrationRequired
        } else if lower.contains("invalid json") || lower.contains("corrupt") {
            ApiErrorCode::StorageCorrupt
        } else if lower.contains("storage is unavailable") || lower.contains("task database") {
            ApiErrorCode::StorageUnavailable
        } else if lower.contains("backend is unavailable") || lower.contains("tauri runtime") {
            ApiErrorCode::BackendUnavailable
        } else if lower.contains("invalid") || lower.contains("unknown storage domain") {
            ApiErrorCode::InvalidArguments
        } else {
            ApiErrorCode::OperationFailed
        };
        let recovery_id = message
            .split_once("storage-migration-required:")
            .and_then(|(_, value)| value.split([':', ' ', '.']).next())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        Self {
            retryable: !matches!(&code, ApiErrorCode::InvalidArguments),
            code,
            message: redact_error_text(message),
            task_id,
            recovery_id,
        }
    }
}

fn parse_task_error(message: &str) -> (Option<String>, &str) {
    let Some(rest) = message.strip_prefix("task-error:") else {
        return (None, message);
    };
    let Some((task_id, detail)) = rest.split_once(':') else {
        return (None, message);
    };
    let task_id = task_id.trim();
    if task_id.is_empty()
        || !task_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return (None, message);
    }
    (Some(task_id.to_string()), detail.trim())
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, TS)]
#[allow(dead_code)] // Added to write results domain-by-domain during the staged migration.
#[serde(rename_all = "camelCase")]
pub(crate) struct OperationReceipt {
    pub(crate) operation_id: String,
    pub(crate) task_id: Option<String>,
    pub(crate) changed: bool,
    pub(crate) backup_ids: Vec<String>,
    pub(crate) recovery_available: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskQuery {
    #[ts(optional, type = "number | null")]
    pub(crate) limit: Option<u16>,
    #[ts(optional, type = "string | null")]
    pub(crate) cursor: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskPage {
    pub(crate) items: Vec<TaskRun>,
    pub(crate) next_cursor: Option<String>,
    pub(crate) unacknowledged_task_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskEvent {
    pub(crate) task_id: String,
    pub(crate) status: TaskStatus,
    pub(crate) summary: String,
    pub(crate) updated_at: String,
}

/// Redaction is repeated at the contract boundary so structured errors can never
/// accidentally carry credentials even when a lower layer returns raw text.
pub(crate) fn redact_error_text(value: &str) -> String {
    let mut redacted = value.to_string();
    for marker in ["password", "passphrase", "token", "api_key", "api key"] {
        redacted = redact_assignment(&redacted, marker);
    }
    redact_sk_tokens(&redacted)
}

fn redact_assignment(value: &str, marker: &str) -> String {
    let lower = value.to_ascii_lowercase();
    let mut output = value.to_string();
    let mut search_from = 0;
    while let Some(relative) = lower[search_from..].find(marker) {
        let marker_start = search_from + relative;
        let after_marker = marker_start + marker.len();
        let Some(delimiter_relative) = lower[after_marker..].find([':', '=']) else {
            break;
        };
        let delimiter = after_marker + delimiter_relative;
        if delimiter.saturating_sub(after_marker) > 4 {
            search_from = after_marker;
            continue;
        }
        let end = output[delimiter + 1..]
            .find(|character: char| {
                character.is_whitespace() || character == ',' || character == ';'
            })
            .map(|relative| delimiter + 1 + relative)
            .unwrap_or(output.len());
        output.replace_range(delimiter + 1..end, "[REDACTED]");
        search_from = delimiter + "[REDACTED]".len() + 1;
    }
    output
}

fn redact_sk_tokens(value: &str) -> String {
    value
        .split_whitespace()
        .map(|part| {
            if part.starts_with("sk-") && part.len() >= 11 {
                "[REDACTED API KEY]"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn export_bindings() {
        let config = ts_rs::Config::default();
        let declarations = [
            ApiErrorCode::decl(&config),
            ApiError::decl(&config),
            OperationReceipt::decl(&config),
            Health::decl(&config),
            AppUpdateState::decl(&config),
            AppUpdateStatus::decl(&config),
            ThemeChoice::decl(&config),
            PlatformAppearance::decl(&config),
            CloseButtonBehavior::decl(&config),
            NetworkProxyMode::decl(&config),
            WorkspaceTerminalFontFamily::decl(&config),
            WorkspaceTerminalColorScheme::decl(&config),
            WorkspaceTerminalCursorStyle::decl(&config),
            WorkspaceTerminalPreferences::decl(&config),
            AppSettings::decl(&config),
            SettingsSaveResult::decl(&config),
            AuthMethod::decl(&config),
            HostStatus::decl(&config),
            Host::decl(&config),
            HostDraft::decl(&config),
            HostPatch::decl(&config),
            Profile::decl(&config),
            ProfileDraft::decl(&config),
            ProfilePatch::decl(&config),
            TaskStatus::decl(&config),
            TaskStepStatus::decl(&config),
            TaskStep::decl(&config),
            TaskLogLevel::decl(&config),
            TaskLog::decl(&config),
            TaskRun::decl(&config),
            TaskQuery::decl(&config),
            TaskPage::decl(&config),
            TaskEvent::decl(&config),
            ProfileApplyTargetFile::decl(&config),
            RemoteCodexReloadMode::decl(&config),
            ProfileApplyOptions::decl(&config),
            RemoteCodexReloadStatus::decl(&config),
            RemoteCodexReloadResult::decl(&config),
            ProfileApplyOutcome::decl(&config),
            ProfileApplyHostResult::decl(&config),
            ProfileApplyPreview::decl(&config),
            ProfileApplyBatchResult::decl(&config),
            ProfileImportExport::decl(&config),
            ProfileApiKeyResult::decl(&config),
            ProfileImportResult::decl(&config),
            CcSwitchDetection::decl(&config),
            SkillPack::decl(&config),
            SkillApplication::decl(&config),
            SkillImportResult::decl(&config),
            RemoteSkill::decl(&config),
            SkillInventoryStatus::decl(&config),
            HostSkillInventory::decl(&config),
            SkillDetectionResult::decl(&config),
            SkillTargetRequest::decl(&config),
            SkillTarget::decl(&config),
            SkillTargetsResult::decl(&config),
            SkillTargetOperationItem::decl(&config),
            SkillTargetOperationResult::decl(&config),
            InstalledSkillRequest::decl(&config),
            InstalledSkillDownloadResult::decl(&config),
            ConnectionTest::decl(&config),
            SshCheckResult::decl(&config),
            SshKeyInfo::decl(&config),
            SshStatus::decl(&config),
            SshHostDraft::decl(&config),
            SshConfigHost::decl(&config),
            SshConfigWriteResult::decl(&config),
            SshKeyGenerationResult::decl(&config),
            SshBootstrapResult::decl(&config),
            SshConfigDeleteResult::decl(&config),
            DeleteOperationResult::decl(&config),
            SshBootstrapProgressEvent::decl(&config),
            RemoteProbeResult::decl(&config),
            RemoteProbeBatchItem::decl(&config),
            RemoteProbeBatchItemCompletedEvent::decl(&config),
            RemoteProbeBatchResult::decl(&config),
            LatestCodexVersion::decl(&config),
            RemoteCodexAction::decl(&config),
            HostOperationKind::decl(&config),
            HostOperationProgressEvent::decl(&config),
            RemoteCodexMaintenanceResult::decl(&config),
            RemoteCodexProcessKind::decl(&config),
            RemoteCodexProcessIdentity::decl(&config),
            RemoteCodexProcessPreflightItem::decl(&config),
            RemoteCodexProcessPreflightResult::decl(&config),
            RemoteCodexBatchProcessAction::decl(&config),
            RemoteCodexBatchHostPlan::decl(&config),
            RemoteCodexBatchItem::decl(&config),
            RemoteCodexBatchResult::decl(&config),
            RemoteCodexProgressEvent::decl(&config),
            NetworkProxyCandidate::decl(&config),
            NetworkProxyStatus::decl(&config),
            RuntimePlatform::decl(&config),
            LocalCodexStatus::decl(&config),
            HostResourceStatus::decl(&config),
            HostResourceSshStatus::decl(&config),
            CpuSnapshot::decl(&config),
            MemorySnapshot::decl(&config),
            StorageFilesystemSnapshot::decl(&config),
            StorageSnapshot::decl(&config),
            GpuVendor::decl(&config),
            GpuStatus::decl(&config),
            GpuMemoryMode::decl(&config),
            GpuProcessSnapshot::decl(&config),
            GpuSnapshot::decl(&config),
            HostResourceSnapshot::decl(&config),
            HostResourceProgressEvent::decl(&config),
            HostResourceBatchResult::decl(&config),
            StorageState::decl(&config),
            StorageHealth::decl(&config),
            StorageMigrationPlan::decl(&config),
            StorageRestorePlan::decl(&config),
            TerminalState::decl(&config),
            OpenTerminalRequest::decl(&config),
            InitialTerminalDirectory::decl(&config),
            TerminalSessionDto::decl(&config),
            AttachTerminalRequest::decl(&config),
            TerminalReplayFrame::decl(&config),
            AttachTerminalResult::decl(&config),
            TerminalWriteRequest::decl(&config),
            TerminalResizeRequest::decl(&config),
            TerminalAckRequest::decl(&config),
            TerminalIdentityRequest::decl(&config),
            OpenFilesRequest::decl(&config),
            OpenFolderInVscodeRequest::decl(&config),
            FileSessionDto::decl(&config),
            FileTargetKind::decl(&config),
            RemoteFileKind::decl(&config),
            RemoteFileEntry::decl(&config),
            FileSortField::decl(&config),
            SortDirection::decl(&config),
            ListDirectoryRequest::decl(&config),
            ListDirectoryResult::decl(&config),
            StartFileSearchRequest::decl(&config),
            FileSearchStarted::decl(&config),
            CancelFileSearchRequest::decl(&config),
            PreviewFileRequest::decl(&config),
            PreviewKind::decl(&config),
            FilePreview::decl(&config),
            CreateDirectoryRequest::decl(&config),
            CopyFileEntryRequest::decl(&config),
            CopyFileEntriesRequest::decl(&config),
            SaveTextFileRequest::decl(&config),
            CwdSource::decl(&config),
            ValidateTerminalCwdRequest::decl(&config),
            ValidatedCwd::decl(&config),
            FileOperationKind::decl(&config),
            PrepareFileOperationRequest::decl(&config),
            PreparedFileOperation::decl(&config),
            ConfirmFileOperationRequest::decl(&config),
            FileOperationResult::decl(&config),
            RecoveryState::decl(&config),
            RecoveryDto::decl(&config),
            RecoveryIdentityRequest::decl(&config),
            PreparedRecoveryPurge::decl(&config),
            PurgeRecoveryRequest::decl(&config),
            TransferDirection::decl(&config),
            TransferState::decl(&config),
            ConflictStrategy::decl(&config),
            LocalPathGrantDto::decl(&config),
            TransferDraft::decl(&config),
            EnqueueTransfersRequest::decl(&config),
            TransferDto::decl(&config),
            TransferSnapshotDto::decl(&config),
            LocalTransferRecoveryState::decl(&config),
            LocalTransferRecoveryDto::decl(&config),
            LocalTransferRecoveryIdentityRequest::decl(&config),
            PreparedLocalTransferRecoveryPurge::decl(&config),
            PurgeLocalTransferRecoveryRequest::decl(&config),
            TransferIdentityRequest::decl(&config),
            ResolveTransferConflictRequest::decl(&config),
            TerminalOutputEvent::decl(&config),
            SessionStateEvent::decl(&config),
            SessionHeartbeatEvent::decl(&config),
            TerminalCwdEvent::decl(&config),
            TransferUpdatedEvent::decl(&config),
            FileSearchState::decl(&config),
            FileSearchUpdatedEvent::decl(&config),
        ]
        // `ts-rs` preserves whitespace before a doc-comment line break.  Keep
        // the checked-in generated contract clean so `git diff --check` stays
        // a meaningful release gate.
        .map(|declaration| {
            format!("export {declaration}")
                .lines()
                .map(str::trim_end)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .join("\n\n");
        let output = format!(
            "// Generated by `cargo test export_bindings`; do not edit.\n\n{declarations}\n"
        );
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("src")
            .join("generated")
            .join("rust-contracts.ts");
        fs::create_dir_all(path.parent().expect("generated bindings parent"))
            .expect("create generated bindings directory");
        fs::write(path, output).expect("write generated bindings");
    }

    #[test]
    fn structured_errors_redact_sensitive_values() {
        let error = ApiError::operation("password=secret token:abc sk-1234567890");
        assert!(!error.message.contains("secret"));
        assert!(!error.message.contains("1234567890"));
    }

    #[test]
    fn structured_errors_classify_partial_failure_and_recovery_domain() {
        let partial = ApiError::from("partial-failure: metadata rollback failed".to_string());
        assert!(matches!(partial.code, ApiErrorCode::PartialFailure));

        let migration = ApiError::from(
            "storage-migration-required:profiles: Confirm migration before writing.".to_string(),
        );
        assert!(matches!(migration.code, ApiErrorCode::MigrationRequired));
        assert_eq!(migration.recovery_id.as_deref(), Some("profiles"));

        let linked = ApiError::from(
            "task-error:task-local-settings-123:Could not save settings.".to_string(),
        );
        assert_eq!(linked.task_id.as_deref(), Some("task-local-settings-123"));
        assert_eq!(linked.message, "Could not save settings.");
    }
}
