use crate::contracts::{redact_error_text, TaskEvent};
use std::sync::Arc;

pub(crate) type TaskEventSink = Arc<dyn Fn(TaskEvent) -> Result<(), String> + Send + Sync>;

/// Event delivery is a UI notification adapter. SQLite remains authoritative,
/// so an emit failure is logged with redaction and never rolls back the task.
pub(crate) fn emit_task_update(sink: Option<&TaskEventSink>, event: TaskEvent) {
    let Some(emit) = sink else {
        return;
    };
    if let Err(error) = emit(event) {
        eprintln!(
            "Could not emit sanitized task update: {}",
            redact_error_text(&error)
        );
    }
}
