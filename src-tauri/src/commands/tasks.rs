use crate::contracts::{ApiResult, TaskPage, TaskQuery};
use crate::tasks::{TaskLogLevel, TaskRun, TaskStatus};
use crate::{
    basic_log, ensure_task_storage_healthy, jobs, redact_error_text, timestamp_label,
    timestamp_millis, AppState,
};
use tauri::State;

#[tauri::command]
pub(crate) fn list_tasks(state: State<'_, AppState>) -> Result<Vec<TaskRun>, String> {
    ensure_task_storage_healthy(&state)?;
    state.task_store.list(500)
}

#[tauri::command]
pub(crate) fn query_tasks(
    state: State<'_, AppState>,
    query: Option<TaskQuery>,
) -> ApiResult<TaskPage> {
    ensure_task_storage_healthy(&state)?;
    let query = query.unwrap_or(TaskQuery {
        limit: None,
        cursor: None,
    });
    let limit = query.limit.unwrap_or(50).clamp(1, 100) as usize;
    let mut items = state
        .task_store
        .list_page(limit + 1, query.cursor.as_deref())?;
    let next_cursor = if items.len() > limit {
        items.truncate(limit);
        items.last().map(|task| task.id.clone())
    } else {
        None
    };
    let unacknowledged_task_ids = state.task_store.list_unacknowledged_failures(500)?;
    Ok(TaskPage {
        items,
        next_cursor,
        unacknowledged_task_ids,
    })
}

#[tauri::command]
pub(crate) fn get_task(state: State<'_, AppState>, task_id: String) -> ApiResult<Option<TaskRun>> {
    ensure_task_storage_healthy(&state)?;
    state.task_store.get(task_id.trim()).map_err(Into::into)
}

#[tauri::command]
pub(crate) fn acknowledge_task(state: State<'_, AppState>, task_id: String) -> ApiResult<bool> {
    ensure_task_storage_healthy(&state)?;
    state
        .task_store
        .acknowledge(task_id.trim())
        .map_err(Into::into)
}

#[tauri::command]
pub(crate) fn clear_task_history(state: State<'_, AppState>) -> ApiResult<u64> {
    ensure_task_storage_healthy(&state)?;
    state
        .task_store
        .clear_history()
        .map(|count| count as u64)
        .map_err(Into::into)
}

#[tauri::command]
pub(crate) fn record_frontend_error(
    state: State<'_, AppState>,
    message: String,
) -> ApiResult<TaskRun> {
    ensure_task_storage_healthy(&state)?;
    let safe_message = redact_error_text(&message)
        .chars()
        .take(512)
        .collect::<String>();
    let task_id = format!("task-frontend-error-{}", timestamp_millis());
    let task = TaskRun {
        id: task_id.clone(),
        host_id: "local-ui".into(),
        host_name: "CodexHub UI".into(),
        action: "Frontend error".into(),
        status: TaskStatus::Failed,
        started_at: timestamp_label(),
        ended_at: Some(timestamp_label()),
        summary: safe_message,
        steps: Vec::new(),
        logs: vec![basic_log(
            &task_id,
            1,
            TaskLogLevel::Error,
            "A sanitized frontend error reached the application error boundary.",
        )],
    };
    jobs::persist_task(&state.task_store, state.task_event_sink.as_ref(), &task)?;
    Ok(task)
}
