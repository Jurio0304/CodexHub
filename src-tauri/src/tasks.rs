use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TaskStatus {
    Queued,
    Running,
    Success,
    Failed,
    Interrupted,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TaskStepStatus {
    Pending,
    Running,
    Success,
    Failed,
    Skipped,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskStep {
    pub(crate) task_run_id: String,
    pub(crate) step_id: String,
    pub(crate) sequence: u32,
    pub(crate) status: TaskStepStatus,
    pub(crate) summary: String,
    pub(crate) started_at: Option<String>,
    pub(crate) ended_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TaskLogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskLog {
    pub(crate) id: String,
    pub(crate) task_run_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) step_id: Option<String>,
    pub(crate) level: TaskLogLevel,
    pub(crate) timestamp: String,
    pub(crate) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub(crate) duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) timed_out: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskRun {
    pub(crate) id: String,
    pub(crate) host_id: String,
    pub(crate) host_name: String,
    pub(crate) action: String,
    pub(crate) status: TaskStatus,
    pub(crate) started_at: String,
    pub(crate) ended_at: Option<String>,
    pub(crate) summary: String,
    #[serde(default)]
    pub(crate) steps: Vec<TaskStep>,
    pub(crate) logs: Vec<TaskLog>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_task_json_defaults_steps_and_log_step_id() {
        let legacy = serde_json::json!({
            "id": "legacy-task",
            "hostId": "host-1",
            "hostName": "Host 1",
            "action": "Legacy action",
            "status": "success",
            "startedAt": "2026-07-01T00:00:00+08:00",
            "endedAt": "2026-07-01T00:00:01+08:00",
            "summary": "Completed.",
            "logs": [{
                "id": "legacy-log",
                "taskRunId": "legacy-task",
                "level": "info",
                "timestamp": "2026-07-01T00:00:01+08:00",
                "message": "Completed."
            }]
        });

        let task: TaskRun = serde_json::from_value(legacy).expect("decode legacy task JSON");
        assert!(task.steps.is_empty());
        assert_eq!(task.logs.len(), 1);
        assert!(task.logs[0].step_id.is_none());
    }

    #[test]
    fn task_step_statuses_use_stable_wire_labels() {
        let labels = [
            (TaskStepStatus::Pending, "pending"),
            (TaskStepStatus::Running, "running"),
            (TaskStepStatus::Success, "success"),
            (TaskStepStatus::Failed, "failed"),
            (TaskStepStatus::Skipped, "skipped"),
        ];
        for (status, expected) in labels {
            assert_eq!(serde_json::to_value(status).unwrap(), expected);
        }
    }
}
