use crate::adapters::{emit_task_update, TaskEventSink};
use crate::contracts::{redact_error_text, TaskEvent};
use crate::ssh;
use crate::storage::TaskStore;
use crate::tasks::{TaskLog, TaskLogLevel, TaskRun, TaskStatus};
use chrono::Local;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TASK_ID_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Persists every state transition before notifying the UI. Event delivery is
/// diagnostic-only; SQLite remains the durable source of truth.
pub(crate) fn persist_task(
    store: &TaskStore,
    event_sink: Option<&TaskEventSink>,
    task: &TaskRun,
) -> Result<(), String> {
    let mut task = sanitize_task(task);
    if let Some(existing) = store.get(&task.id)? {
        let mut merged = task.logs;
        for log in existing.logs {
            if !merged.iter().any(|current| current.id == log.id) {
                merged.push(log);
            }
        }
        merged.sort_by(|left, right| left.timestamp.cmp(&right.timestamp));
        task.logs = merged;
    }
    store.upsert(&task)?;
    emit_task_update(
        event_sink,
        TaskEvent {
            task_id: task.id.clone(),
            status: task.status.clone(),
            summary: task.summary.clone(),
            updated_at: now(),
        },
    );
    Ok(())
}

/// Appends one sanitized progress line and publishes the running task update
/// immediately. Final task writes merge these lines instead of replacing them.
pub(crate) fn append_message(
    store: &TaskStore,
    event_sink: Option<&TaskEventSink>,
    task_id: &str,
    level: TaskLogLevel,
    message: &str,
) -> Result<(), String> {
    let mut task = store
        .get(task_id)?
        .ok_or_else(|| format!("Task {task_id} was not found for progress logging."))?;
    let safe_message = redact_error_text(message);
    task.summary = safe_message.clone();
    task.logs.push(TaskLog {
        id: format!("{task_id}-progress-{}", timestamp_millis()),
        task_run_id: task_id.to_string(),
        level,
        timestamp: now(),
        message: safe_message,
        command: None,
        stdout: None,
        stderr: None,
        exit_code: None,
        duration_ms: None,
        timed_out: None,
    });
    persist_task(store, event_sink, &task)
}

pub(crate) fn begin_task(
    store: &TaskStore,
    event_sink: Option<&TaskEventSink>,
    task_id: &str,
    host_id: &str,
    host_name: &str,
    action: &str,
) -> Result<TaskRun, String> {
    let mut task = TaskRun {
        id: task_id.to_string(),
        host_id: host_id.to_string(),
        host_name: host_name.to_string(),
        action: action.to_string(),
        status: TaskStatus::Queued,
        started_at: now(),
        ended_at: None,
        summary: format!("{action} queued."),
        logs: vec![log(task_id, 1, TaskLogLevel::Info, "Operation queued.")],
    };
    persist_task(store, event_sink, &task)?;
    task.status = TaskStatus::Running;
    task.summary = format!("{action} is running.");
    task.logs
        .push(log(task_id, 2, TaskLogLevel::Info, "Operation started."));
    persist_task(store, event_sink, &task)?;
    Ok(task)
}

pub(crate) fn run_local_operation<T, F>(
    store: &TaskStore,
    event_sink: Option<&TaskEventSink>,
    action: &str,
    domain: &str,
    operation: F,
) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String>,
{
    run_observed_operation(store, event_sink, action, domain, operation, |_| {
        (TaskStatus::Success, format!("{action} completed."))
    })
}

/// Persists queued/running before the adapter is called. A domain can mark a
/// returned compatibility DTO as partial/failed without discarding that DTO.
pub(crate) fn run_observed_operation<T, F, S>(
    store: &TaskStore,
    event_sink: Option<&TaskEventSink>,
    action: &str,
    domain: &str,
    operation: F,
    summarize: S,
) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String>,
    S: FnOnce(&T) -> (TaskStatus, String),
{
    let task_id = format!("task-local-{}-{}", slug(domain), timestamp_millis());
    let mut task = begin_task(
        store,
        event_sink,
        &task_id,
        &format!("local-{domain}"),
        domain,
        action,
    )?;

    match operation() {
        Ok(value) => {
            let (status, summary) = summarize(&value);
            let level = if matches!(&status, TaskStatus::Success) {
                TaskLogLevel::Info
            } else {
                TaskLogLevel::Error
            };
            task.status = status;
            task.ended_at = Some(now());
            task.summary = summary.clone();
            task.logs.push(log(&task_id, 3, level, &summary));
            persist_task(store, event_sink, &task)?;
            Ok(value)
        }
        Err(error) => {
            let safe_error = redact_error_text(&error);
            task.status = TaskStatus::Failed;
            task.ended_at = Some(now());
            task.summary = safe_error.clone();
            task.logs
                .push(log(&task_id, 3, TaskLogLevel::Error, &safe_error));
            persist_task(store, event_sink, &task)?;
            Err(task_error(&task_id, &safe_error))
        }
    }
}

/// Compatibility envelope for legacy string-returning commands. Both backend
/// and frontend parsers recover the durable task id from this sanitized value.
pub(crate) fn task_error(task_id: &str, message: &str) -> String {
    format!("task-error:{task_id}:{}", redact_error_text(message))
}

fn sanitize_task(task: &TaskRun) -> TaskRun {
    let mut safe = task.clone();
    safe.summary = ssh::redact_sensitive(&safe.summary);
    for log in &mut safe.logs {
        log.message = ssh::redact_sensitive(&log.message);
        log.command = log.command.as_deref().map(ssh::redact_sensitive);
        log.stdout = log.stdout.as_deref().map(ssh::redact_sensitive);
        log.stderr = log.stderr.as_deref().map(ssh::redact_sensitive);
    }
    safe
}

fn log(task_id: &str, sequence: usize, level: TaskLogLevel, message: &str) -> TaskLog {
    TaskLog {
        id: format!("{task_id}-log-{sequence}"),
        task_run_id: task_id.to_string(),
        level,
        timestamp: now(),
        message: message.to_string(),
        command: None,
        stdout: None,
        stderr: None,
        exit_code: None,
        duration_ms: None,
        timed_out: None,
    }
}

fn now() -> String {
    Local::now().to_rfc3339()
}

fn timestamp_millis() -> u128 {
    let micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_micros())
        .unwrap_or_default();
    micros
        .saturating_mul(1_000_000)
        .saturating_add(u128::from(TASK_ID_SEQUENCE.fetch_add(1, Ordering::Relaxed)))
}

fn slug(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn local_operations_persist_success_and_redacted_failure() {
        let store = TaskStore::in_memory();
        let value = run_local_operation(&store, None, "Save settings", "settings", || {
            Ok::<_, String>(7)
        })
        .expect("successful operation");
        assert_eq!(value, 7);

        let error = run_local_operation::<(), _>(&store, None, "Save profile", "profiles", || {
            Err("token=do-not-store".into())
        })
        .expect_err("failed operation");
        assert!(error.starts_with("task-error:"));
        assert!(!error.contains("do-not-store"));
        let tasks = store.list(10).expect("task history");
        assert!(tasks
            .iter()
            .any(|task| matches!(task.status, TaskStatus::Success)));
        let failed = tasks
            .iter()
            .find(|task| matches!(task.status, TaskStatus::Failed))
            .expect("failed task");
        assert!(!failed.summary.contains("do-not-store"));
    }

    #[test]
    fn task_persistence_redacts_all_log_surfaces() {
        let store = TaskStore::in_memory();
        let task = TaskRun {
            id: "task-secret".into(),
            host_id: "local".into(),
            host_name: "Local".into(),
            action: "Test".into(),
            status: TaskStatus::Failed,
            started_at: now(),
            ended_at: Some(now()),
            summary: "token=do-not-store".into(),
            logs: vec![TaskLog {
                id: "log-secret".into(),
                task_run_id: "task-secret".into(),
                level: TaskLogLevel::Error,
                timestamp: now(),
                message: "password=do-not-store".into(),
                command: Some("api_key=do-not-store".into()),
                stdout: Some(format!("-----BEGIN OPENSSH {} KEY-----\nsecret", "PRIVATE")),
                stderr: Some("sk-1234567890".into()),
                exit_code: Some(1),
                duration_ms: None,
                timed_out: Some(false),
            }],
        };
        persist_task(&store, None, &task).expect("persist sanitized task");
        let serialized = serde_json::to_string(&store.get("task-secret").unwrap().unwrap())
            .expect("serialize stored task");
        assert!(!serialized.contains("do-not-store"));
        assert!(!serialized.contains("BEGIN OPENSSH PRIVATE KEY"));
        assert!(!serialized.contains("1234567890"));
    }

    #[test]
    fn unavailable_task_storage_blocks_the_operation_before_it_starts() {
        let store = TaskStore::unavailable("injected database failure".into());
        let operation_called = AtomicBool::new(false);

        let error =
            run_local_operation::<(), _>(&store, None, "Write durable state", "test", || {
                operation_called.store(true, Ordering::SeqCst);
                Ok(())
            })
            .expect_err("task storage failure must stop the operation");

        assert!(error.contains("Persistent task storage is unavailable"));
        assert!(!operation_called.load(Ordering::SeqCst));
    }

    #[test]
    fn progress_logs_are_visible_before_completion_and_survive_final_persist() {
        let store = TaskStore::in_memory();
        let mut task = begin_task(
            &store,
            None,
            "task-progress",
            "local-test",
            "Test",
            "Run staged work",
        )
        .expect("begin task");

        append_message(
            &store,
            None,
            &task.id,
            TaskLogLevel::Info,
            "Stage one finished.",
        )
        .expect("append progress");
        let running = store.get(&task.id).unwrap().unwrap();
        assert!(running
            .logs
            .iter()
            .any(|log| log.message == "Stage one finished."));

        task.status = TaskStatus::Success;
        task.ended_at = Some(now());
        task.summary = "Run staged work completed.".into();
        persist_task(&store, None, &task).expect("persist completion");
        let completed = store.get(&task.id).unwrap().unwrap();
        assert!(completed
            .logs
            .iter()
            .any(|log| log.message == "Stage one finished."));
    }
}
