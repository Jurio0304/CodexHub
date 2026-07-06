use serde::Serialize;

#[derive(Clone, Serialize)]
#[allow(dead_code)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TaskStatus {
    Queued,
    Running,
    Success,
    Failed,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TaskLogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskLog {
    pub(crate) id: String,
    pub(crate) task_run_id: String,
    pub(crate) level: TaskLogLevel,
    pub(crate) timestamp: String,
    pub(crate) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) timed_out: Option<bool>,
}

#[derive(Clone, Serialize)]
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
    pub(crate) logs: Vec<TaskLog>,
}
