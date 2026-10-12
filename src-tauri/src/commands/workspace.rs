//! Tauri boundary for the Workspace resource owner.
//!
//! Browser payloads contain aliases, opaque entry references and one-shot local
//! grants only.  Private keys, arbitrary local paths and terminal bytes never
//! enter durable task logs.

use crate::tasks::{TaskLog, TaskLogLevel, TaskStatus, TaskStep, TaskStepStatus};
use crate::workspace::transfer_io::{TransferAuditSink, TransferAuditStage, TransferAuditStatus};
use crate::workspace::types::*;
use crate::{jobs, AppServices, AppState, Host};
use chrono::Local;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;

struct WorkspaceTransferJobAudit {
    services: Arc<AppServices>,
}

/// Cancellation may occur before a worker starts, while a conflict waits, or
/// inside a stream. Keep the Job Manager row terminal in every case without
/// writing transfer payloads or local paths into the task log.
fn record_transfer_cancelled(services: &Arc<AppServices>, transfer: &TransferDto) {
    let Some(task_id) = transfer.task_id.as_deref() else {
        return;
    };
    WorkspaceTransferJobAudit {
        services: services.clone(),
    }
    .record(
        task_id,
        TransferAuditStage::Transfer,
        TransferAuditStatus::Cancelled,
    );
}

impl TransferAuditSink for WorkspaceTransferJobAudit {
    fn record(&self, task_id: &str, stage: TransferAuditStage, status: TransferAuditStatus) {
        let Ok(Some(task)) = self.services.task_store.get(task_id) else {
            return;
        };
        let (step_id, sequence, summary) = transfer_task_step(stage);
        let mut step = task
            .steps
            .into_iter()
            .find(|item| item.step_id == step_id)
            .unwrap_or(TaskStep {
                task_run_id: task_id.to_string(),
                step_id: step_id.to_string(),
                sequence,
                status: TaskStepStatus::Pending,
                summary: summary.to_string(),
                started_at: None,
                ended_at: None,
            });
        let now = Local::now().to_rfc3339();
        step.status = match status {
            TransferAuditStatus::Running => TaskStepStatus::Running,
            TransferAuditStatus::Success => TaskStepStatus::Success,
            TransferAuditStatus::Failed => TaskStepStatus::Failed,
            TransferAuditStatus::Skipped | TransferAuditStatus::Cancelled => {
                TaskStepStatus::Skipped
            }
        };
        if step.started_at.is_none() {
            step.started_at = Some(now.clone());
        }
        if !matches!(status, TransferAuditStatus::Running) {
            step.ended_at = Some(now);
        }
        let _ = jobs::persist_step_update(
            &self.services.task_store,
            self.services.task_event_sink.as_ref(),
            task_id,
            &step,
            None,
            Some(summary),
        );

        let final_status = match (stage, status) {
            (_, TransferAuditStatus::Failed) => Some(TaskStatus::Failed),
            (_, TransferAuditStatus::Cancelled) => Some(TaskStatus::Cancelled),
            (TransferAuditStage::Commit, TransferAuditStatus::Success) => Some(TaskStatus::Success),
            _ => None,
        };
        if let Some(status) = final_status {
            if let Ok(Some(mut current)) = self.services.task_store.get(task_id) {
                current.status = status;
                current.ended_at = Some(Local::now().to_rfc3339());
                current.summary = summary.to_string();
                let _ = jobs::persist_task(
                    &self.services.task_store,
                    self.services.task_event_sink.as_ref(),
                    &current,
                );
            }
        }
    }
}

fn transfer_task_step(stage: TransferAuditStage) -> (&'static str, u32, &'static str) {
    match stage {
        TransferAuditStage::Preflight => (
            "preflight",
            1,
            "Validating transfer source and destination.",
        ),
        TransferAuditStage::Transfer => ("transfer", 2, "Transferring file contents."),
        TransferAuditStage::Verify => ("verify", 3, "Verifying transferred content."),
        TransferAuditStage::Commit => ("commit", 4, "Committing verified transfer."),
    }
}

fn workspace_transfer_steps(task_id: &str) -> Vec<TaskStep> {
    [
        TransferAuditStage::Preflight,
        TransferAuditStage::Transfer,
        TransferAuditStage::Verify,
        TransferAuditStage::Commit,
    ]
    .into_iter()
    .map(|stage| {
        let (step_id, sequence, summary) = transfer_task_step(stage);
        TaskStep {
            task_run_id: task_id.to_string(),
            step_id: step_id.to_string(),
            sequence,
            status: TaskStepStatus::Pending,
            summary: summary.to_string(),
            started_at: None,
            ended_at: None,
        }
    })
    .collect()
}

fn workspace_file_operation_steps(task_id: &str) -> Vec<TaskStep> {
    [
        ("preflight", 1, "Revalidating the confirmed file operation."),
        ("backup", 2, "Creating the recoverable backup journal."),
        ("commit", 3, "Applying the confirmed file operation."),
    ]
    .into_iter()
    .map(|(step_id, sequence, summary)| TaskStep {
        task_run_id: task_id.to_string(),
        step_id: step_id.to_string(),
        sequence,
        status: TaskStepStatus::Pending,
        summary: summary.to_string(),
        started_at: None,
        ended_at: None,
    })
    .collect()
}

#[derive(Clone, Copy)]
enum WorkspaceFilesTaskOperation {
    Connect,
    ReadDirectory,
}

impl WorkspaceFilesTaskOperation {
    fn action(self) -> &'static str {
        match self {
            Self::Connect => "Connect Workspace Files",
            Self::ReadDirectory => "Read Workspace directory",
        }
    }

    fn step(self) -> (&'static str, &'static str) {
        match self {
            Self::Connect => ("connect", "Opening the SFTP Files session."),
            Self::ReadDirectory => ("read", "Reading the requested directory."),
        }
    }

    fn completed_message(self) -> &'static str {
        match self {
            Self::Connect => "Workspace Files connection completed.",
            Self::ReadDirectory => "Workspace directory read completed.",
        }
    }

    fn failed_message(self, error_code: &str) -> String {
        match self {
            Self::Connect => format!("Workspace Files connection failed ({error_code})."),
            Self::ReadDirectory => format!("Workspace directory read failed ({error_code})."),
        }
    }
}

/// Files tasks contain only the saved host identity, operation and stable code.
/// SFTP errors can contain remote paths, so they never enter the task payload.
fn begin_workspace_files_task(
    store: &crate::storage::TaskStore,
    event_sink: Option<&crate::adapters::TaskEventSink>,
    host_id: &str,
    host_name: &str,
    operation: WorkspaceFilesTaskOperation,
) -> Result<String, String> {
    let task_id = format!("task-workspace-files-{}", Uuid::new_v4());
    let mut task = jobs::begin_task(
        store,
        event_sink,
        &task_id,
        host_id,
        host_name,
        operation.action(),
    )?;
    let (step_id, summary) = operation.step();
    let now = Local::now().to_rfc3339();
    task.steps = vec![TaskStep {
        task_run_id: task.id.clone(),
        step_id: step_id.to_string(),
        sequence: 1,
        status: TaskStepStatus::Running,
        summary: summary.to_string(),
        started_at: Some(now),
        ended_at: None,
    }];
    task.summary = summary.to_string();
    jobs::persist_task(store, event_sink, &task)?;
    Ok(task.id)
}

fn settle_workspace_files_task(
    store: &crate::storage::TaskStore,
    event_sink: Option<&crate::adapters::TaskEventSink>,
    task_id: &str,
    operation: WorkspaceFilesTaskOperation,
    error_code: Option<&str>,
) -> Result<(), String> {
    let mut task = store
        .get(task_id)?
        .ok_or_else(|| format!("Workspace Files task {task_id} is unavailable."))?;
    let now = Local::now().to_rfc3339();
    let failed = error_code.is_some();
    let (step_id, _) = operation.step();
    let summary = error_code
        .map(|code| operation.failed_message(code))
        .unwrap_or_else(|| operation.completed_message().to_string());
    for step in &mut task.steps {
        step.status = if failed {
            TaskStepStatus::Failed
        } else {
            TaskStepStatus::Success
        };
        step.started_at.get_or_insert_with(|| now.clone());
        step.ended_at = Some(now.clone());
    }
    task.status = if failed {
        TaskStatus::Failed
    } else {
        TaskStatus::Success
    };
    task.ended_at = Some(now.clone());
    task.summary = summary.clone();
    task.logs.push(TaskLog {
        id: jobs::task_log_id(task_id, task.logs.len() + 1),
        task_run_id: task_id.to_string(),
        step_id: Some(step_id.to_string()),
        level: if failed {
            TaskLogLevel::Error
        } else {
            TaskLogLevel::Info
        },
        timestamp: now,
        message: summary,
        command: None,
        stdout: None,
        stderr: None,
        exit_code: None,
        duration_ms: None,
        timed_out: None,
    });
    jobs::persist_task(store, event_sink, &task)
}

fn workspace_files_error_code(error: &crate::workspace::error::WorkspaceError) -> &str {
    if !error.code.is_empty()
        && error
            .code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        &error.code
    } else {
        "workspace-files-error"
    }
}

/// Only transport-classified failures may replace the shared SFTP subsystem.
/// Permission, path and encoding errors keep the original session intact.
fn should_reconnect_workspace_files(error: &crate::workspace::error::WorkspaceError) -> bool {
    error.retryable
}

/// File mutations are durable recovery operations, so their Job Manager row
/// is created before the confirmation token can trigger a remote write.
fn begin_workspace_file_task_identity(
    state: &AppState,
    target_id: &str,
    target_name: &str,
    action: &str,
) -> Result<String, String> {
    let task_id = format!("task-workspace-file-{}", Uuid::new_v4());
    let mut task = jobs::begin_task(
        &state.task_store,
        state.task_event_sink.as_ref(),
        &task_id,
        target_id,
        target_name,
        action,
    )?;
    task.steps = workspace_file_operation_steps(&task.id);
    jobs::persist_task(&state.task_store, state.task_event_sink.as_ref(), &task)?;
    Ok(task.id)
}

fn settle_workspace_file_task(state: &AppState, task_id: &str, success: bool, summary: &str) {
    let Ok(Some(mut task)) = state.task_store.get(task_id) else {
        return;
    };
    let now = Local::now().to_rfc3339();
    for step in &mut task.steps {
        if step.started_at.is_none() {
            step.started_at = Some(now.clone());
        }
        step.ended_at = Some(now.clone());
        step.status = if success {
            TaskStepStatus::Success
        } else if step.step_id == "commit" {
            TaskStepStatus::Failed
        } else {
            TaskStepStatus::Skipped
        };
    }
    task.status = if success {
        TaskStatus::Success
    } else {
        TaskStatus::Failed
    };
    task.ended_at = Some(now);
    task.summary = summary.to_string();
    let _ = jobs::persist_task(&state.task_store, state.task_event_sink.as_ref(), &task);
}

async fn open_recovery_files(
    state: &AppState,
    workspace: &crate::workspace::WorkspaceManager,
    recovery: &RecoveryDto,
) -> Result<(String, String, String), String> {
    if recovery.host_id == "local" && recovery.host_alias.is_empty() {
        let session = workspace
            .files
            .open(OpenFilesRequest {
                local: true,
                host_id: "local".into(),
                host_name: "Local files".into(),
                host_alias: String::new(),
            })
            .await
            .map_err(|error| error.to_string())?
            .session;
        return Ok((
            "local".into(),
            "Local files".into(),
            session.file_session_id,
        ));
    }
    let host = host_for_alias(state, &recovery.host_alias)?;
    if host.id != recovery.host_id {
        return Err(
            "recovery-host-unavailable: The saved host identity changed; recovery is blocked."
                .into(),
        );
    }
    let session = workspace
        .files
        .open(OpenFilesRequest {
            local: false,
            host_id: host.id.clone(),
            host_name: host.name.clone(),
            host_alias: host.host_alias.clone(),
        })
        .await
        .map_err(|error| error.to_string())?
        .session;
    Ok((host.id, host.name, session.file_session_id))
}

fn manager(state: &AppState) -> Result<&crate::workspace::WorkspaceManager, String> {
    state
        .workspace
        .as_ref()
        .map_err(|error| format!("desktop-backend-required: {error}"))
}

fn host_for_alias(state: &AppState, host_alias: &str) -> Result<Host, String> {
    let hosts = state
        .hosts
        .lock()
        .map_err(|_| "Host inventory is unavailable.".to_string())?;
    hosts
        .iter()
        .find(|host| host.host_alias == host_alias)
        .cloned()
        .ok_or_else(|| format!("Unknown SSH host alias: {host_alias}"))
}

fn assert_terminal_host(state: &AppState, request: &OpenTerminalRequest) -> Result<(), String> {
    let host = host_for_alias(state, &request.host_alias)?;
    if host.id != request.host_id || host.name != request.host_name {
        return Err("Workspace terminal host identity no longer matches the saved host.".into());
    }
    Ok(())
}

fn assert_files_host(state: &AppState, request: &OpenFilesRequest) -> Result<Option<Host>, String> {
    if request.local {
        if request.host_id != "local" || !request.host_alias.is_empty() {
            return Err("Workspace local Files identity is invalid.".into());
        }
        return Ok(None);
    }
    let host = host_for_alias(state, &request.host_alias)?;
    if host.id != request.host_id || host.name != request.host_name {
        return Err("Workspace Files host identity no longer matches the saved host.".into());
    }
    Ok(Some(host))
}

#[tauri::command]
pub(crate) async fn workspace_open_terminal(
    state: State<'_, AppState>,
    mut request: OpenTerminalRequest,
) -> Result<TerminalSessionDto, String> {
    assert_terminal_host(&state, &request)?;
    let workspace = manager(&state)?;
    let initial_directory = match request.initial_directory.take() {
        Some(context) => Some(
            workspace
                .files
                .validate_terminal_initial_directory(
                    &context.file_session_id,
                    &request.host_id,
                    &context.path,
                )
                .await
                .map_err(|error| error.to_string())?,
        ),
        None => None,
    };
    workspace
        .terminals
        .open(request, initial_directory)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn workspace_list_terminal_sessions(
    state: State<'_, AppState>,
) -> Result<Vec<TerminalSessionDto>, String> {
    manager(&state).and_then(|workspace| {
        workspace
            .terminals
            .list()
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub(crate) fn workspace_attach_terminal(
    state: State<'_, AppState>,
    request: AttachTerminalRequest,
) -> Result<AttachTerminalResult, String> {
    manager(&state).and_then(|workspace| {
        workspace
            .terminals
            .attach(request)
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub(crate) fn workspace_terminal_write(
    state: State<'_, AppState>,
    request: TerminalWriteRequest,
) -> Result<(), String> {
    manager(&state).and_then(|workspace| {
        workspace
            .terminals
            .write(request)
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub(crate) fn workspace_terminal_resize(
    state: State<'_, AppState>,
    request: TerminalResizeRequest,
) -> Result<TerminalSessionDto, String> {
    manager(&state).and_then(|workspace| {
        workspace
            .terminals
            .resize(request)
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub(crate) fn workspace_terminal_ack(
    state: State<'_, AppState>,
    request: TerminalAckRequest,
) -> Result<(), String> {
    manager(&state).and_then(|workspace| {
        workspace
            .terminals
            .ack(request)
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub(crate) fn workspace_reconnect_terminal(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<TerminalSessionDto, String> {
    manager(&state).and_then(|workspace| {
        workspace
            .terminals
            .reconnect(&session_id)
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub(crate) fn workspace_close_terminal(
    state: State<'_, AppState>,
    request: TerminalIdentityRequest,
) -> Result<TerminalSessionDto, String> {
    manager(&state).and_then(|workspace| {
        workspace
            .terminals
            .close(request)
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub(crate) fn workspace_list_local_roots(
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    manager(&state)?
        .files
        .local_roots()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) async fn workspace_open_files(
    state: State<'_, AppState>,
    request: OpenFilesRequest,
) -> Result<FileSessionDto, String> {
    let host = assert_files_host(&state, &request)?;
    let workspace = manager(&state)?;
    match workspace.files.open(request).await {
        Ok(opened) if opened.reused => Ok(opened.session),
        Ok(opened) => {
            let (host_id, host_name) = host
                .as_ref()
                .map(|item| (item.id.as_str(), item.name.as_str()))
                .unwrap_or(("local", "Local files"));
            let task_id = begin_workspace_files_task(
                &state.task_store,
                state.task_event_sink.as_ref(),
                host_id,
                host_name,
                WorkspaceFilesTaskOperation::Connect,
            )?;
            settle_workspace_files_task(
                &state.task_store,
                state.task_event_sink.as_ref(),
                &task_id,
                WorkspaceFilesTaskOperation::Connect,
                None,
            )?;
            Ok(opened.session)
        }
        Err(error) => {
            let (host_id, host_name) = host
                .as_ref()
                .map(|item| (item.id.as_str(), item.name.as_str()))
                .unwrap_or(("local", "Local files"));
            let error_code = workspace_files_error_code(&error);
            let safe_message = WorkspaceFilesTaskOperation::Connect.failed_message(error_code);
            let task_id = begin_workspace_files_task(
                &state.task_store,
                state.task_event_sink.as_ref(),
                host_id,
                host_name,
                WorkspaceFilesTaskOperation::Connect,
            )?;
            settle_workspace_files_task(
                &state.task_store,
                state.task_event_sink.as_ref(),
                &task_id,
                WorkspaceFilesTaskOperation::Connect,
                Some(error_code),
            )?;
            Err(jobs::task_error(&task_id, &safe_message))
        }
    }
}

#[tauri::command]
pub(crate) async fn workspace_open_folder_in_vscode(
    state: State<'_, AppState>,
    request: OpenFolderInVscodeRequest,
) -> Result<(), String> {
    manager(&state)?
        .files
        .open_folder_in_vscode(request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) async fn workspace_list_directory(
    state: State<'_, AppState>,
    request: ListDirectoryRequest,
) -> Result<ListDirectoryResult, String> {
    let workspace = manager(&state)?;
    let file_session_id = request.file_session_id.clone();
    let result = match workspace.files.list_directory(request.clone()).await {
        Err(error) if should_reconnect_workspace_files(&error) => {
            // A FileSession is reused per host. Replace a broken transport once
            // before surfacing the directory failure to the user and Job Manager.
            match workspace.files.reconnect(&file_session_id).await {
                Ok(()) => workspace.files.list_directory(request).await,
                Err(reconnect_error) => Err(reconnect_error),
            }
        }
        result => result,
    };
    match result {
        Ok(result) => Ok(result),
        Err(error) => {
            let error_code = workspace_files_error_code(&error);
            // A stale/missing session still receives a path-free local task.
            let (host_id, host_name) = workspace
                .files
                .operation_host_identity(&file_session_id)
                .unwrap_or_else(|_| ("workspace-files".into(), "Workspace Files".into()));
            let task_id = begin_workspace_files_task(
                &state.task_store,
                state.task_event_sink.as_ref(),
                &host_id,
                &host_name,
                WorkspaceFilesTaskOperation::ReadDirectory,
            )?;
            let safe_message =
                WorkspaceFilesTaskOperation::ReadDirectory.failed_message(error_code);
            settle_workspace_files_task(
                &state.task_store,
                state.task_event_sink.as_ref(),
                &task_id,
                WorkspaceFilesTaskOperation::ReadDirectory,
                Some(error_code),
            )?;
            Err(jobs::task_error(&task_id, &safe_message))
        }
    }
}

#[tauri::command]
pub(crate) async fn workspace_start_file_search(
    state: State<'_, AppState>,
    request: StartFileSearchRequest,
) -> Result<FileSearchStarted, String> {
    manager(&state)?
        .files
        .start_search(request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn workspace_cancel_file_search(
    state: State<'_, AppState>,
    request: CancelFileSearchRequest,
) -> Result<(), String> {
    manager(&state).and_then(|workspace| {
        workspace
            .files
            .cancel_search(request)
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub(crate) async fn workspace_preview_file(
    state: State<'_, AppState>,
    request: PreviewFileRequest,
) -> Result<FilePreview, String> {
    manager(&state)?
        .files
        .preview(request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) async fn workspace_create_directory(
    state: State<'_, AppState>,
    request: CreateDirectoryRequest,
) -> Result<RemoteFileEntry, String> {
    manager(&state)?
        .files
        .create_directory(request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) async fn workspace_copy_file_entry(
    state: State<'_, AppState>,
    request: CopyFileEntryRequest,
) -> Result<RemoteFileEntry, String> {
    let workspace = manager(&state)?;
    let (target_id, target_name) = workspace
        .files
        .operation_host_identity(&request.file_session_id)
        .map_err(|error| error.to_string())?;
    let task_id = begin_workspace_file_task_identity(
        &state,
        &target_id,
        &target_name,
        "Copy Workspace file",
    )?;
    let result = workspace
        .files
        .copy_entry(request)
        .await
        .map_err(|error| error.to_string());
    settle_workspace_file_task(
        &state,
        &task_id,
        result.is_ok(),
        if result.is_ok() {
            "Workspace file copy completed."
        } else {
            "Workspace file copy failed without replacing the destination."
        },
    );
    result
}

#[tauri::command]
pub(crate) async fn workspace_copy_file_entries(
    state: State<'_, AppState>,
    request: CopyFileEntriesRequest,
) -> Result<Vec<RemoteFileEntry>, String> {
    manager(&state)?
        .files
        .copy_entries(request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) async fn workspace_move_file_entries(
    state: State<'_, AppState>,
    request: CopyFileEntriesRequest,
) -> Result<Vec<RemoteFileEntry>, String> {
    let workspace = manager(&state)?;
    workspace
        .operations
        .move_entries(&workspace.files, request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) async fn workspace_save_text_file(
    state: State<'_, AppState>,
    request: SaveTextFileRequest,
) -> Result<RemoteFileEntry, String> {
    let workspace = manager(&state)?;
    let current = workspace
        .files
        .operation_stat(&request.file_session_id, &request.entry_ref)
        .await
        .map_err(|error| error.to_string())?;
    if current.kind != RemoteFileKind::File || current.fingerprint != request.expected_fingerprint {
        return Err(
            "stale-edit: The file changed after it was opened; reload before saving.".into(),
        );
    }
    let staging = workspace
        .files
        .write_text_staging(&request.file_session_id, &current.path, &request.text)
        .await
        .map_err(|error| error.to_string())?;
    let prepared = workspace
        .operations
        .prepare_overwrite(
            &workspace.files,
            &request.file_session_id,
            &request.entry_ref,
            &staging.entry_ref,
        )
        .await;
    let prepared = match prepared {
        Ok(value) => value,
        Err(error) => {
            let _ = workspace
                .files
                .remove_staging_file(&request.file_session_id, &staging.path)
                .await;
            return Err(error.to_string());
        }
    };
    let result = workspace
        .operations
        .confirm(
            &workspace.files,
            ConfirmFileOperationRequest {
                operation_token: prepared.operation_token,
            },
            None,
        )
        .await;
    let _ = workspace
        .files
        .remove_staging_file(&request.file_session_id, &staging.path)
        .await;
    match result {
        Ok(_) => workspace
            .files
            .operation_internal_entry(&request.file_session_id, &current.path)
            .await
            .map_err(|error| error.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
pub(crate) async fn workspace_validate_terminal_cwd(
    state: State<'_, AppState>,
    request: ValidateTerminalCwdRequest,
) -> Result<ValidatedCwd, String> {
    let workspace = manager(&state)?;
    let terminal_request = TerminalIdentityRequest {
        session_id: request.session_id,
        generation: request.generation,
    };
    let candidates = workspace
        .terminals
        .cwd_candidates(&terminal_request)
        .map_err(|error| error.to_string())?;
    let terminal_host_id = &candidates[0].host_id;
    workspace
        .files
        .assert_host(&request.file_session_id, terminal_host_id)
        .map_err(|error| error.to_string())?;
    let mut last_error = None;
    for candidate in candidates {
        match workspace
            .files
            .canonicalize_cwd(&request.file_session_id, &candidate.path)
            .await
        {
            Ok(canonical) => {
                return workspace
                    .terminals
                    .record_verified_cwd(terminal_request.clone(), canonical, candidate.source)
                    .map_err(|error| error.to_string());
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error
        .unwrap_or_else(|| {
            crate::workspace::error::WorkspaceError::new(
                "terminal-cwd-unknown",
                "The terminal working directory could not be validated.",
            )
        })
        .to_string())
}

#[tauri::command]
pub(crate) async fn workspace_close_files(
    state: State<'_, AppState>,
    file_session_id: String,
) -> Result<(), String> {
    manager(&state)?
        .files
        .close(&file_session_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) async fn workspace_prepare_file_operation(
    state: State<'_, AppState>,
    request: PrepareFileOperationRequest,
) -> Result<PreparedFileOperation, String> {
    let workspace = manager(&state)?;
    workspace
        .operations
        .prepare(&workspace.files, request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) async fn workspace_confirm_file_operation(
    state: State<'_, AppState>,
    request: ConfirmFileOperationRequest,
) -> Result<FileOperationResult, String> {
    let workspace = manager(&state)?;
    let (prepared_host_id, prepared_alias) = workspace
        .operations
        .prepared_host(&request.operation_token)
        .map_err(|error| error.to_string())?;
    let (target_id, target_name) = if prepared_host_id == "local" && prepared_alias.is_empty() {
        ("local".to_string(), "Local files".to_string())
    } else {
        let host = host_for_alias(&state, &prepared_alias)?;
        if host.id != prepared_host_id {
            return Err("file-operation-host-unavailable: The saved host identity changed; confirm is blocked.".into());
        }
        (host.id, host.name)
    };
    let task_id = begin_workspace_file_task_identity(
        &state,
        &target_id,
        &target_name,
        "Workspace file operation",
    )?;
    let result = workspace
        .operations
        .confirm(&workspace.files, request, Some(task_id.clone()))
        .await
        .map_err(|error| error.to_string());
    settle_workspace_file_task(
        &state,
        &task_id,
        result.is_ok(),
        if result.is_ok() {
            "Workspace file operation completed."
        } else {
            "Workspace file operation failed; recovery journal retained."
        },
    );
    result
}

#[tauri::command]
pub(crate) async fn workspace_restore_recovery(
    state: State<'_, AppState>,
    request: RecoveryIdentityRequest,
) -> Result<FileOperationResult, String> {
    let workspace = manager(&state)?;
    let recovery = workspace
        .operations
        .get(&request.recovery_id)
        .map_err(|error| error.to_string())?;
    let (target_id, target_name, file_session_id) =
        open_recovery_files(&state, workspace, &recovery).await?;
    let task_id = begin_workspace_file_task_identity(
        &state,
        &target_id,
        &target_name,
        "Restore Workspace recovery",
    )?;
    let result = workspace
        .operations
        .restore(
            &workspace.files,
            request,
            &file_session_id,
            Some(task_id.clone()),
        )
        .await
        .map_err(|error| error.to_string());
    settle_workspace_file_task(
        &state,
        &task_id,
        result.is_ok(),
        if result.is_ok() {
            "Workspace recovery restored."
        } else {
            "Workspace recovery restore failed; backup retained."
        },
    );
    result
}

#[tauri::command]
pub(crate) fn workspace_prepare_recovery_purge(
    state: State<'_, AppState>,
    request: RecoveryIdentityRequest,
) -> Result<PreparedRecoveryPurge, String> {
    manager(&state).and_then(|workspace| {
        workspace
            .operations
            .prepare_purge(request)
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub(crate) async fn workspace_purge_recovery(
    state: State<'_, AppState>,
    request: PurgeRecoveryRequest,
) -> Result<RecoveryDto, String> {
    let workspace = manager(&state)?;
    let recovery = workspace
        .operations
        .prepared_purge_recovery(&request.purge_token)
        .map_err(|error| error.to_string())?;
    let (target_id, target_name, file_session_id) =
        open_recovery_files(&state, workspace, &recovery).await?;
    let task_id = begin_workspace_file_task_identity(
        &state,
        &target_id,
        &target_name,
        "Purge Workspace recovery",
    )?;
    let result = workspace
        .operations
        .purge(
            &workspace.files,
            request,
            &file_session_id,
            Some(task_id.clone()),
        )
        .await
        .map_err(|error| error.to_string());
    settle_workspace_file_task(
        &state,
        &task_id,
        result.is_ok(),
        if result.is_ok() {
            "Workspace recovery permanently removed."
        } else {
            "Workspace recovery purge failed; backup retained."
        },
    );
    result
}

#[tauri::command]
pub(crate) fn workspace_restore_local_transfer_recovery(
    state: State<'_, AppState>,
    request: LocalTransferRecoveryIdentityRequest,
) -> Result<LocalTransferRecoveryDto, String> {
    manager(&state)?
        .local_recoveries
        .restore(&request.recovery_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn workspace_prepare_local_transfer_recovery_purge(
    state: State<'_, AppState>,
    request: LocalTransferRecoveryIdentityRequest,
) -> Result<PreparedLocalTransferRecoveryPurge, String> {
    let (purge_token, recovery, expires_at) = manager(&state)?
        .local_recoveries
        .prepare_purge(&request.recovery_id)
        .map_err(|error| error.to_string())?;
    Ok(PreparedLocalTransferRecoveryPurge {
        purge_token,
        recovery,
        expires_at: expires_at.to_rfc3339(),
    })
}

#[tauri::command]
pub(crate) fn workspace_purge_local_transfer_recovery(
    state: State<'_, AppState>,
    request: PurgeLocalTransferRecoveryRequest,
) -> Result<(), String> {
    manager(&state)?
        .local_recoveries
        .purge(&request.purge_token)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) async fn workspace_select_upload_sources(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<LocalPathGrantDto>, String> {
    let paths = select_files(app).await?;
    let workspace = manager(&state)?;
    let mut grants = Vec::with_capacity(paths.len());
    for path in paths {
        grants.push(
            workspace
                .transfer_io
                .local_grants
                .grant_upload_file(path)
                .await
                .map_err(|error| error.to_string())?,
        );
    }
    Ok(grants)
}

#[tauri::command]
pub(crate) async fn workspace_select_download_target(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<LocalPathGrantDto>, String> {
    let Some(path) = select_folder(app).await? else {
        return Ok(None);
    };
    manager(&state)?
        .transfer_io
        .local_grants
        .grant_download_directory(path)
        .await
        .map(Some)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) async fn workspace_enqueue_transfers(
    state: State<'_, AppState>,
    mut request: EnqueueTransfersRequest,
) -> Result<Vec<TransferDto>, String> {
    let services = state.services.clone();
    let workspace = manager(&state)?;
    for draft in &mut request.items {
        if draft.host_id == "local" && draft.host_alias.is_empty() {
            workspace
                .files
                .assert_host(&request.file_session_id, "local")
                .map_err(|error| error.to_string())?;
        } else {
            let host = host_for_alias(&state, &draft.host_alias)?;
            if host.id != draft.host_id || host.name != draft.host_name {
                return Err(
                    "Workspace transfer host identity no longer matches the saved host.".into(),
                );
            }
            workspace
                .files
                .assert_host(&request.file_session_id, &host.id)
                .map_err(|error| error.to_string())?;
        }
        if draft.direction == TransferDirection::Upload {
            let name = workspace
                .transfer_io
                .local_grants
                .upload_file_name(&draft.source_ref)
                .map_err(|error| error.to_string())?;
            let parent = workspace
                .files
                .canonicalize_cwd(&request.file_session_id, &draft.destination_path)
                .await
                .map_err(|error| error.to_string())?;
            draft.destination_path = remote_child_path(&parent, &name)?;
            if crate::workspace::files::is_prohibited_path(&draft.destination_path) {
                return Err(
                    "Workspace blocks uploads to protected SSH or credential paths.".into(),
                );
            }
        } else {
            // Persist the canonical remote source, never the ephemeral entry
            // capability, so an interrupted download can be reauthorized and
            // verified after a desktop restart.
            let entry = workspace
                .files
                .entry(&request.file_session_id, &draft.source_ref)
                .await
                .map_err(|error| error.to_string())?;
            if !entry.writable_name || entry.kind != crate::workspace::types::RemoteFileKind::File {
                return Err("Downloads require a fresh regular-file entry reference.".into());
            }
            if crate::workspace::files::is_prohibited_path(&entry.path) {
                return Err(
                    "Workspace blocks downloads of protected SSH or credential paths.".into(),
                );
            }
            draft.source_ref = entry.path;
        }
    }
    let created = workspace
        .transfers
        .enqueue(request.clone())
        .map_err(|error| error.to_string())?;
    for transfer in &created {
        let mut task = jobs::begin_task(
            &state.task_store,
            state.task_event_sink.as_ref(),
            &format!("task-workspace-transfer-{}", transfer.transfer_id),
            &transfer.host_id,
            &transfer.host_name,
            "Workspace transfer",
        )?;
        task.steps = workspace_transfer_steps(&task.id);
        jobs::persist_task(&state.task_store, state.task_event_sink.as_ref(), &task)?;
        workspace
            .transfers
            .set_task_id(&transfer.transfer_id, task.id)
            .map_err(|error| error.to_string())?;
    }
    // A download directory grant is shared only inside this enqueue batch.
    // Consume it once here, then bind every selected remote file to the same
    // approved local directory without minting a second path capability.
    let mut download_directories = HashMap::new();
    for draft in request
        .items
        .iter()
        .filter(|item| item.direction == TransferDirection::Download)
    {
        let grant_id = draft
            .local_grant_id
            .as_deref()
            .ok_or("Downloads require a local target grant.")?;
        if !download_directories.contains_key(grant_id) {
            let directory = workspace
                .transfer_io
                .consume_download_directory(grant_id)
                .map_err(|error| error.to_string())?;
            download_directories.insert(grant_id.to_string(), directory);
        }
    }
    // Binding consumes upload grants only after durable queue creation. A
    // failed bind becomes an explicit failed transfer, never invented progress.
    for transfer in workspace
        .transfers
        .list()
        .map_err(|error| error.to_string())?
    {
        if !created
            .iter()
            .any(|item| item.transfer_id == transfer.transfer_id)
        {
            continue;
        }
        let draft = request
            .items
            .iter()
            .find(|item| item.host_id == transfer.host_id && item.source_ref == transfer.source_ref)
            .ok_or_else(|| "Workspace transfer draft disappeared.".to_string())?;
        let bind = match transfer.direction {
            TransferDirection::Upload => {
                workspace
                    .transfer_io
                    .bind_upload(
                        &workspace.transfers,
                        &transfer,
                        request.file_session_id.clone(),
                    )
                    .await
            }
            TransferDirection::Download => {
                workspace
                    .transfer_io
                    .bind_download(
                        &workspace.transfers,
                        &transfer,
                        &workspace.files,
                        request.file_session_id.clone(),
                        download_directories
                            .get(
                                draft
                                    .local_grant_id
                                    .as_deref()
                                    .ok_or("Downloads require a local target grant.")?,
                            )
                            .cloned()
                            .ok_or("Downloads require a local target grant.")?,
                    )
                    .await
            }
        };
        if let Err(error) = bind {
            let _ = workspace
                .transfers
                .mark_failed(&transfer.transfer_id, error.code, false, None);
            if let Some(task_id) = transfer.task_id.as_deref() {
                WorkspaceTransferJobAudit {
                    services: services.clone(),
                }
                .record(
                    task_id,
                    TransferAuditStage::Preflight,
                    TransferAuditStatus::Failed,
                );
            }
        }
    }
    let created_ids = created
        .iter()
        .map(|transfer| transfer.transfer_id.as_str())
        .collect::<HashSet<_>>();
    // The enqueue command returns only this request's batch. Durable history
    // remains available through workspace_list_transfers.
    let created = workspace
        .transfers
        .list()
        .map_err(|error| error.to_string())
        .map(|transfers| retain_created_transfers(transfers, &created_ids))?;
    schedule_transfer_workers(services);
    Ok(created)
}

fn retain_created_transfers(
    transfers: Vec<TransferDto>,
    created_ids: &HashSet<&str>,
) -> Vec<TransferDto> {
    transfers
        .into_iter()
        .filter(|transfer| created_ids.contains(transfer.transfer_id.as_str()))
        .collect()
}

fn remote_child_path(parent: &str, name: &str) -> Result<String, String> {
    if name.contains('\\') {
        return Err("The upload source name cannot be used as a remote filename.".into());
    }
    crate::workspace::remote_path::join(parent, name).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn workspace_list_transfers(
    state: State<'_, AppState>,
) -> Result<TransferSnapshotDto, String> {
    let workspace = manager(&state)?;
    Ok(TransferSnapshotDto {
        transfers: workspace
            .transfers
            .list()
            .map_err(|error| error.to_string())?,
        recoveries: workspace
            .operations
            .list()
            .map_err(|error| error.to_string())?,
        local_recoveries: workspace
            .local_recoveries
            .list()
            .map_err(|error| error.to_string())?,
    })
}

#[tauri::command]
pub(crate) fn workspace_pause_transfer(
    state: State<'_, AppState>,
    request: TransferIdentityRequest,
) -> Result<TransferDto, String> {
    let workspace = manager(&state)?;
    assert_transfer_revision(workspace, &request)?;
    workspace
        .transfers
        .pause(request)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) async fn workspace_resume_transfer(
    state: State<'_, AppState>,
    request: TransferIdentityRequest,
) -> Result<TransferDto, String> {
    let services = state.services.clone();
    let workspace = manager(&state)?;
    assert_transfer_revision(workspace, &request)?;
    ensure_transfer_resume_binding(workspace, &request).await?;
    let result = workspace
        .transfers
        .resume(request)
        .map_err(|error| error.to_string())?;
    schedule_transfer_workers(services);
    Ok(result)
}

#[tauri::command]
pub(crate) fn workspace_cancel_transfer(
    state: State<'_, AppState>,
    request: TransferIdentityRequest,
) -> Result<TransferDto, String> {
    let services = state.services.clone();
    let workspace = manager(&state)?;
    assert_transfer_revision(workspace, &request)?;
    let result = workspace
        .transfers
        .cancel(request)
        .map_err(|error| error.to_string())?;
    if result.state == TransferState::Cancelled {
        record_transfer_cancelled(&services, &result);
    }
    Ok(result)
}

#[tauri::command]
pub(crate) async fn workspace_retry_transfer(
    state: State<'_, AppState>,
    request: TransferIdentityRequest,
) -> Result<TransferDto, String> {
    let services = state.services.clone();
    let workspace = manager(&state)?;
    assert_transfer_revision(workspace, &request)?;
    ensure_transfer_resume_binding(workspace, &request).await?;
    let result = workspace
        .transfers
        .retry(request)
        .map_err(|error| error.to_string())?;
    schedule_transfer_workers(services);
    Ok(result)
}

#[tauri::command]
pub(crate) fn workspace_resolve_transfer_conflict(
    state: State<'_, AppState>,
    request: ResolveTransferConflictRequest,
) -> Result<TransferDto, String> {
    let services = state.services.clone();
    let workspace = manager(&state)?;
    let resolution = workspace
        .transfers
        .resolve_conflict(request)
        .map_err(|error| error.to_string())?;
    for transfer in &resolution.affected {
        if transfer.state == TransferState::Cancelled {
            record_transfer_cancelled(&services, transfer);
        }
    }
    schedule_transfer_workers(services);
    Ok(resolution.primary)
}

fn assert_transfer_revision(
    workspace: &crate::workspace::WorkspaceManager,
    request: &TransferIdentityRequest,
) -> Result<(), String> {
    let Some(expected) = request.revision else {
        return Ok(());
    };
    let actual = workspace
        .transfers
        .list()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|item| item.transfer_id == request.transfer_id)
        .ok_or("Transfer not found.")?;
    if actual.revision != expected {
        return Err("stale-transfer: Refresh the transfer queue before acting.".into());
    }
    Ok(())
}

/// Reauthorization is needed only after process memory lost a transfer plan.
/// The caller supplies a fresh opaque native grant; paths never cross Tauri.
async fn ensure_transfer_resume_binding(
    workspace: &crate::workspace::WorkspaceManager,
    request: &TransferIdentityRequest,
) -> Result<(), String> {
    if workspace
        .transfer_io
        .is_bound(&request.transfer_id)
        .map_err(|error| error.to_string())?
    {
        return Ok(());
    }
    let file_session_id = request.file_session_id.as_deref().ok_or(
        "transfer-reauthorization-required: Select the original local source or destination again.",
    )?;
    let local_grant_id = request.local_grant_id.as_deref().ok_or(
        "transfer-reauthorization-required: Select the original local source or destination again.",
    )?;
    let transfer = workspace
        .transfers
        .list()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|item| item.transfer_id == request.transfer_id)
        .ok_or("Transfer not found.")?;
    workspace
        .files
        .assert_host(file_session_id, &transfer.host_id)
        .map_err(|error| error.to_string())?;
    workspace
        .transfer_io
        .bind_after_restart(
            &transfer,
            &workspace.files,
            file_session_id.to_string(),
            local_grant_id,
        )
        .await
        .map_err(|error| error.to_string())
}

fn schedule_transfer_workers(services: Arc<AppServices>) {
    if let Ok(workspace) = services.workspace.as_ref() {
        workspace
            .transfer_io
            .set_audit(Arc::new(WorkspaceTransferJobAudit {
                services: services.clone(),
            }));
    }
    loop {
        let next = match services.workspace.as_ref() {
            Ok(workspace) => match workspace.transfers.claim_next() {
                Ok(value) => value,
                Err(_) => return,
            },
            Err(_) => return,
        };
        let Some(transfer) = next else {
            return;
        };
        let child_services = services.clone();
        tauri::async_runtime::spawn(async move {
            if let Ok(workspace) = child_services.workspace.as_ref() {
                let _ = workspace
                    .transfer_io
                    .run_claimed(&workspace.transfers, &workspace.files, transfer)
                    .await;
            }
            schedule_transfer_workers(child_services);
        });
    }
}

async fn select_files(app: AppHandle) -> Result<Vec<PathBuf>, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_files(move |paths| {
        let _ = sender.send(paths);
    });
    let paths = receiver
        .await
        .map_err(|_| "Native file picker did not return a result.".to_string())?
        .unwrap_or_default();
    paths
        .into_iter()
        .map(|path| {
            path.into_path()
                .map_err(|error| format!("Selected local path is unavailable: {error}"))
        })
        .collect()
}

async fn select_folder(app: AppHandle) -> Result<Option<PathBuf>, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |path| {
        let _ = sender.send(path);
    });
    receiver
        .await
        .map_err(|_| "Native folder picker did not return a result.".to_string())?
        .map(|value| {
            value
                .into_path()
                .map_err(|error| format!("Selected local path is unavailable: {error}"))
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::TaskStore;

    fn transfer_for_enqueue_test(id: &str) -> TransferDto {
        TransferDto {
            transfer_id: id.to_string(),
            batch_id: "batch-test".to_string(),
            task_id: None,
            direction: TransferDirection::Upload,
            host_id: "host-1".to_string(),
            host_name: "Host 1".to_string(),
            host_alias: "host-1".to_string(),
            source_ref: format!("grant-{id}"),
            destination_path: format!("/tmp/{id}"),
            created_at: "2026-08-03T00:00:00Z".to_string(),
            updated_at: "2026-08-03T00:00:00Z".to_string(),
            state: TransferState::Queued,
            revision: 1,
            bytes: 0,
            total: None,
            speed: None,
            eta_seconds: None,
            attempt: 0,
            resumable: false,
            resume_offset: None,
            conflict_strategy: ConflictStrategy::ReplaceWithBackup,
            conflict_revision: None,
            error_code: None,
            fingerprint_status: None,
            durable_source_fingerprint: None,
            durable_partial_locator: None,
        }
    }

    #[test]
    fn enqueue_response_excludes_durable_transfer_history() {
        let history = transfer_for_enqueue_test("history");
        let created = transfer_for_enqueue_test("created");
        let created_ids = HashSet::from(["created"]);

        let response = retain_created_transfers(vec![history, created], &created_ids);

        assert_eq!(response.len(), 1);
        assert_eq!(response[0].transfer_id, "created");
    }

    #[test]
    fn files_connection_task_settles_without_transport_payloads() {
        let store = TaskStore::in_memory();
        let task_id = begin_workspace_files_task(
            &store,
            None,
            "host-files-1",
            "Files host",
            WorkspaceFilesTaskOperation::Connect,
        )
        .expect("begin files task");

        settle_workspace_files_task(
            &store,
            None,
            &task_id,
            WorkspaceFilesTaskOperation::Connect,
            Some("sftp-start-failed"),
        )
        .expect("settle files task");

        let task = store
            .get(&task_id)
            .expect("read task")
            .expect("task exists");
        assert!(matches!(task.status, TaskStatus::Failed));
        assert_eq!(task.action, "Connect Workspace Files");
        assert_eq!(task.host_id, "host-files-1");
        assert!(task.logs.iter().any(|log| {
            log.step_id.as_deref() == Some("connect")
                && log.message == "Workspace Files connection failed (sftp-start-failed)."
        }));
        assert!(task.logs.iter().all(|log| {
            !log.message.contains("/home/")
                && !log.message.contains("password=")
                && log.stdout.is_none()
                && log.stderr.is_none()
        }));
        let envelope = jobs::task_error(
            &task_id,
            "Workspace Files connection failed (sftp-start-failed).",
        );
        assert!(envelope.starts_with(&format!("task-error:{task_id}:")));
        assert!(!envelope.contains("/private/path"));
    }

    #[test]
    fn files_error_code_rejects_non_stable_text() {
        let error = crate::workspace::error::WorkspaceError::new(
            "directory-open-failed /private/path",
            "not used by audit",
        );
        assert_eq!(workspace_files_error_code(&error), "workspace-files-error");
    }

    #[test]
    fn only_retryable_files_errors_replace_the_shared_sftp_session() {
        let transport = crate::workspace::error::WorkspaceError::retryable(
            "directory-read-failed",
            "connection reset",
        );
        let permissions = crate::workspace::error::WorkspaceError::new(
            "directory-open-failed",
            "permission denied",
        );

        assert!(should_reconnect_workspace_files(&transport));
        assert!(!should_reconnect_workspace_files(&permissions));
    }

    #[test]
    fn directory_read_failure_is_task_enveloped_and_path_free() {
        let store = TaskStore::in_memory();
        let task_id = begin_workspace_files_task(
            &store,
            None,
            "host-files-1",
            "Files host",
            WorkspaceFilesTaskOperation::ReadDirectory,
        )
        .expect("begin directory read task");
        let error_code = "directory-open-failed";
        let safe_message = WorkspaceFilesTaskOperation::ReadDirectory.failed_message(error_code);

        settle_workspace_files_task(
            &store,
            None,
            &task_id,
            WorkspaceFilesTaskOperation::ReadDirectory,
            Some(error_code),
        )
        .expect("settle directory read task");

        let envelope = jobs::task_error(&task_id, &safe_message);
        let task = store
            .get(&task_id)
            .expect("read task")
            .expect("task exists");
        assert!(matches!(task.status, TaskStatus::Failed));
        assert_eq!(task.action, "Read Workspace directory");
        assert!(envelope.starts_with(&format!("task-error:{task_id}:")));
        assert!(task
            .logs
            .iter()
            .any(|log| { log.step_id.as_deref() == Some("read") && log.message == safe_message }));
        assert!(!envelope.contains("/private/path"));
        assert!(task.logs.iter().all(|log| {
            !log.message.contains("/private/path")
                && !log.message.contains("ssh: Connection reset")
                && log.stdout.is_none()
                && log.stderr.is_none()
        }));
    }
}
