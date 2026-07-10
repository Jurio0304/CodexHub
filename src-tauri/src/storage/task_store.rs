use crate::tasks::{TaskLog, TaskLogLevel, TaskRun, TaskStatus};
use chrono::Local;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

const CURRENT_TASK_SCHEMA_VERSION: i64 = 1;
static BACKUP_ID_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// SQLite is the durable source of truth for task history. The mutex is held only
/// for short SQL transactions and never across SSH, network, or filesystem work.
pub(crate) struct TaskStore {
    connection: Mutex<Connection>,
    unavailable_reason: Option<String>,
}

impl TaskStore {
    pub(crate) fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!("Could not create the task database directory: {error}")
            })?;
        }
        let connection = Connection::open(path)
            .map_err(|error| format!("Could not open the task database: {error}"))?;
        let schema_version = connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .map_err(|error| format!("Could not inspect the task database schema: {error}"))?;
        if schema_version > CURRENT_TASK_SCHEMA_VERSION {
            return Err(format!(
                "Task database schema {schema_version} is newer than supported schema {CURRENT_TASK_SCHEMA_VERSION}."
            ));
        }
        if schema_version > 0 && schema_version < CURRENT_TASK_SCHEMA_VERSION {
            backup_before_schema_upgrade(&connection, path, schema_version)?;
        }
        Self::configure(&connection)?;
        let store = Self {
            connection: Mutex::new(connection),
            unavailable_reason: None,
        };
        store.mark_interrupted()?;
        Ok(store)
    }

    #[cfg(test)]
    pub(crate) fn in_memory() -> Self {
        let connection = Connection::open_in_memory().expect("open task database in memory");
        Self::configure(&connection).expect("configure task database in memory");
        Self {
            connection: Mutex::new(connection),
            unavailable_reason: None,
        }
    }

    pub(crate) fn unavailable(reason: String) -> Self {
        Self {
            connection: Mutex::new(
                Connection::open_in_memory().expect("open disabled task database handle"),
            ),
            unavailable_reason: Some(reason),
        }
    }

    fn ensure_available(&self) -> Result<(), String> {
        match self.unavailable_reason.as_ref() {
            Some(reason) => Err(format!("Persistent task storage is unavailable: {reason}")),
            None => Ok(()),
        }
    }

    fn configure(connection: &Connection) -> Result<(), String> {
        connection
            .execute_batch(
                r#"
                PRAGMA foreign_keys = ON;
                PRAGMA journal_mode = WAL;
                PRAGMA synchronous = FULL;
                PRAGMA busy_timeout = 5000;
                CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    checksum TEXT NOT NULL,
                    applied_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS task_runs (
                    id TEXT PRIMARY KEY,
                    host_id TEXT NOT NULL,
                    host_name TEXT NOT NULL,
                    action TEXT NOT NULL,
                    status TEXT NOT NULL,
                    started_at TEXT NOT NULL,
                    ended_at TEXT,
                    summary TEXT NOT NULL,
                    acknowledged_at TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_task_runs_started_at
                    ON task_runs(started_at DESC);
                CREATE INDEX IF NOT EXISTS idx_task_runs_status
                    ON task_runs(status, started_at DESC);
                CREATE TABLE IF NOT EXISTS task_logs (
                    id TEXT PRIMARY KEY,
                    task_run_id TEXT NOT NULL REFERENCES task_runs(id) ON DELETE CASCADE,
                    sequence INTEGER NOT NULL,
                    level TEXT NOT NULL,
                    timestamp TEXT NOT NULL,
                    message TEXT NOT NULL,
                    command TEXT,
                    stdout TEXT,
                    stderr TEXT,
                    exit_code INTEGER,
                    duration_ms INTEGER,
                    timed_out INTEGER,
                    UNIQUE(task_run_id, sequence)
                );
                CREATE TABLE IF NOT EXISTS operation_journal (
                    operation_id TEXT PRIMARY KEY,
                    kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    payload_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS storage_backups (
                    backup_id TEXT PRIMARY KEY,
                    store_name TEXT NOT NULL,
                    path TEXT NOT NULL,
                    sha256 TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    restored_at TEXT
                );
                PRAGMA user_version = 1;
                "#,
            )
            .map_err(|error| format!("Could not initialize the task database: {error}"))?;
        connection
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version, name, checksum, applied_at) VALUES(1, 'initial-task-store', 'v1', ?1)",
                [Local::now().to_rfc3339()],
            )
            .map_err(|error| format!("Could not record the task schema migration: {error}"))?;
        Ok(())
    }

    pub(crate) fn upsert(&self, task: &TaskRun) -> Result<(), String> {
        self.ensure_available()?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "Task database mutex was poisoned.".to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("Could not start the task transaction: {error}"))?;
        upsert_run(&transaction, task)?;
        transaction
            .execute("DELETE FROM task_logs WHERE task_run_id = ?1", [&task.id])
            .map_err(|error| format!("Could not replace task logs: {error}"))?;
        for (sequence, log) in task.logs.iter().enumerate() {
            insert_log(&transaction, sequence, log)?;
        }
        transaction
            .commit()
            .map_err(|error| format!("Could not commit the task transaction: {error}"))
    }

    pub(crate) fn list(&self, limit: usize) -> Result<Vec<TaskRun>, String> {
        self.list_page(limit, None)
    }

    pub(crate) fn list_page(
        &self,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<Vec<TaskRun>, String> {
        self.ensure_available()?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| "Task database mutex was poisoned.".to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT id, host_id, host_name, action, status, started_at, ended_at, summary
                 FROM task_runs
                 WHERE ?2 IS NULL
                    OR started_at < (SELECT started_at FROM task_runs WHERE id = ?2)
                    OR (
                      started_at = (SELECT started_at FROM task_runs WHERE id = ?2)
                      AND rowid < (SELECT rowid FROM task_runs WHERE id = ?2)
                    )
                 ORDER BY started_at DESC, rowid DESC LIMIT ?1",
            )
            .map_err(|error| format!("Could not prepare the task query: {error}"))?;
        let rows = statement
            .query_map(params![limit as i64, cursor], |row| {
                Ok(TaskRun {
                    id: row.get(0)?,
                    host_id: row.get(1)?,
                    host_name: row.get(2)?,
                    action: row.get(3)?,
                    status: parse_status(&row.get::<_, String>(4)?),
                    started_at: row.get(5)?,
                    ended_at: row.get(6)?,
                    summary: row.get(7)?,
                    logs: Vec::new(),
                })
            })
            .map_err(|error| format!("Could not query task history: {error}"))?;
        let mut tasks = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Could not decode task history: {error}"))?;
        for task in &mut tasks {
            task.logs = load_logs(&connection, &task.id)?;
        }
        Ok(tasks)
    }

    pub(crate) fn list_unacknowledged_failures(&self, limit: usize) -> Result<Vec<String>, String> {
        self.ensure_available()?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| "Task database mutex was poisoned.".to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT id FROM task_runs
                 WHERE acknowledged_at IS NULL AND status IN ('failed', 'interrupted')
                 ORDER BY started_at DESC, rowid DESC LIMIT ?1",
            )
            .map_err(|error| format!("Could not prepare unacknowledged task query: {error}"))?;
        let task_ids = statement
            .query_map([limit as i64], |row| row.get(0))
            .map_err(|error| format!("Could not query unacknowledged tasks: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Could not decode unacknowledged tasks: {error}"))?;
        Ok(task_ids)
    }

    pub(crate) fn get(&self, task_id: &str) -> Result<Option<TaskRun>, String> {
        self.ensure_available()?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| "Task database mutex was poisoned.".to_string())?;
        let mut task = connection
            .query_row(
                "SELECT id, host_id, host_name, action, status, started_at, ended_at, summary FROM task_runs WHERE id = ?1",
                [task_id],
                |row| {
                    Ok(TaskRun {
                        id: row.get(0)?,
                        host_id: row.get(1)?,
                        host_name: row.get(2)?,
                        action: row.get(3)?,
                        status: parse_status(&row.get::<_, String>(4)?),
                        started_at: row.get(5)?,
                        ended_at: row.get(6)?,
                        summary: row.get(7)?,
                        logs: Vec::new(),
                    })
                },
            )
            .optional()
            .map_err(|error| format!("Could not read task {task_id}: {error}"))?;
        if let Some(task) = &mut task {
            task.logs = load_logs(&connection, task_id)?;
        }
        Ok(task)
    }

    pub(crate) fn acknowledge(&self, task_id: &str) -> Result<bool, String> {
        self.ensure_available()?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| "Task database mutex was poisoned.".to_string())?;
        connection
            .execute(
                "UPDATE task_runs SET acknowledged_at = COALESCE(acknowledged_at, ?2) WHERE id = ?1",
                params![task_id, Local::now().to_rfc3339()],
            )
            .map(|changed| changed > 0)
            .map_err(|error| format!("Could not acknowledge task {task_id}: {error}"))
    }

    pub(crate) fn begin_operation(
        &self,
        operation_id: &str,
        kind: &str,
        payload_json: &str,
    ) -> Result<(), String> {
        self.ensure_available()?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| "Task database mutex was poisoned.".to_string())?;
        let now = Local::now().to_rfc3339();
        connection
            .execute(
                "INSERT INTO operation_journal(operation_id, kind, status, payload_json, created_at, updated_at)
                 VALUES(?1, ?2, 'running', ?3, ?4, ?4)
                 ON CONFLICT(operation_id) DO UPDATE SET
                   kind=excluded.kind, status='running', payload_json=excluded.payload_json,
                   updated_at=excluded.updated_at",
                params![operation_id, kind, payload_json, now],
            )
            .map(|_| ())
            .map_err(|error| format!("Could not start operation journal entry: {error}"))
    }

    pub(crate) fn finish_operation(&self, operation_id: &str, status: &str) -> Result<(), String> {
        self.ensure_available()?;
        if !matches!(
            status,
            "completed" | "failed" | "recovered" | "recovery-required"
        ) {
            return Err(format!("Invalid operation journal status: {status}"));
        }
        let connection = self
            .connection
            .lock()
            .map_err(|_| "Task database mutex was poisoned.".to_string())?;
        connection
            .execute(
                "UPDATE operation_journal SET status = ?2, updated_at = ?3 WHERE operation_id = ?1",
                params![operation_id, status, Local::now().to_rfc3339()],
            )
            .and_then(|changed| {
                if changed == 1 {
                    Ok(changed)
                } else {
                    Err(rusqlite::Error::QueryReturnedNoRows)
                }
            })
            .map(|_| ())
            .map_err(|error| format!("Could not finish operation journal entry: {error}"))
    }

    pub(crate) fn pending_operation_ids(&self) -> Result<Vec<String>, String> {
        self.ensure_available()?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| "Task database mutex was poisoned.".to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT operation_id FROM operation_journal WHERE status IN ('running', 'recovery-required') ORDER BY created_at ASC",
            )
            .map_err(|error| format!("Could not prepare pending operation query: {error}"))?;
        let ids = statement
            .query_map([], |row| row.get(0))
            .map_err(|error| format!("Could not query pending operations: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Could not decode pending operations: {error}"))?;
        Ok(ids)
    }

    pub(crate) fn record_backup(
        &self,
        store_name: &str,
        path: &Path,
        sha256: &str,
    ) -> Result<String, String> {
        self.ensure_available()?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| "Task database mutex was poisoned.".to_string())?;
        let created_at = Local::now().to_rfc3339();
        let backup_id = format!(
            "backup-{}-{}-{}",
            store_name,
            Local::now().timestamp_micros(),
            BACKUP_ID_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        connection
            .execute(
                "INSERT INTO storage_backups(backup_id, store_name, path, sha256, created_at, restored_at)
                 VALUES(?1, ?2, ?3, ?4, ?5, NULL)",
                params![
                    backup_id,
                    store_name,
                    path.to_string_lossy(),
                    sha256,
                    created_at
                ],
            )
            .map_err(|error| format!("Could not record storage backup metadata: {error}"))?;
        Ok(backup_id)
    }

    pub(crate) fn mark_backup_restored(&self, path: &Path) -> Result<(), String> {
        self.ensure_available()?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| "Task database mutex was poisoned.".to_string())?;
        connection
            .execute(
                "UPDATE storage_backups SET restored_at = COALESCE(restored_at, ?2) WHERE path = ?1",
                params![path.to_string_lossy(), Local::now().to_rfc3339()],
            )
            .map(|_| ())
            .map_err(|error| format!("Could not mark a storage backup as restored: {error}"))
    }

    fn mark_interrupted(&self) -> Result<(), String> {
        self.ensure_available()?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| "Task database mutex was poisoned.".to_string())?;
        let now = Local::now().to_rfc3339();
        connection
            .execute(
                "UPDATE task_runs
                 SET status = 'interrupted', ended_at = ?1,
                     summary = summary || ' The previous CodexHub process ended before completion.'
                 WHERE status IN ('queued', 'running')",
                [now],
            )
            .map(|_| ())
            .map_err(|error| format!("Could not recover interrupted tasks: {error}"))
    }
}

/// Future schema upgrades checkpoint WAL first and use SQLite's own consistent
/// snapshot mechanism. The v1 bootstrap does not create an unnecessary backup.
fn backup_before_schema_upgrade(
    connection: &Connection,
    database_path: &Path,
    from_version: i64,
) -> Result<PathBuf, String> {
    connection
        .execute_batch("PRAGMA wal_checkpoint(FULL);")
        .map_err(|error| {
            format!("Could not checkpoint the task database before upgrade: {error}")
        })?;
    let parent = database_path
        .parent()
        .ok_or_else(|| "Task database path has no parent directory.".to_string())?;
    let backup = parent.join(format!(
        "codexhub-schema-v{from_version}-{}.sqlite",
        Local::now().format("%Y%m%d-%H%M%S-%3f")
    ));
    connection
        .execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])
        .map_err(|error| format!("Could not create the task database upgrade backup: {error}"))?;
    let check = Connection::open(&backup)
        .and_then(|backup_connection| {
            backup_connection.query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
        })
        .map_err(|error| format!("Could not validate the task database upgrade backup: {error}"))?;
    if check != "ok" {
        return Err(format!(
            "Task database upgrade backup failed integrity validation: {check}"
        ));
    }
    Ok(backup)
}

fn upsert_run(transaction: &Transaction<'_>, task: &TaskRun) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO task_runs(id, host_id, host_name, action, status, started_at, ended_at, summary)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
               host_id=excluded.host_id, host_name=excluded.host_name, action=excluded.action,
               status=excluded.status, ended_at=excluded.ended_at,
               summary=excluded.summary",
            params![
                task.id,
                task.host_id,
                task.host_name,
                task.action,
                status_label(&task.status),
                task.started_at,
                task.ended_at,
                task.summary
            ],
        )
        .map(|_| ())
        .map_err(|error| format!("Could not persist task {}: {error}", task.id))
}

fn insert_log(transaction: &Transaction<'_>, sequence: usize, log: &TaskLog) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO task_logs(id, task_run_id, sequence, level, timestamp, message, command, stdout, stderr, exit_code, duration_ms, timed_out)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                log.id,
                log.task_run_id,
                sequence as i64,
                level_label(&log.level),
                log.timestamp,
                log.message,
                log.command,
                log.stdout,
                log.stderr,
                log.exit_code,
                log.duration_ms.map(|value| value as i64),
                log.timed_out.map(i64::from)
            ],
        )
        .map(|_| ())
        .map_err(|error| format!("Could not persist task log {}: {error}", log.id))
}

fn load_logs(connection: &Connection, task_id: &str) -> Result<Vec<TaskLog>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, task_run_id, level, timestamp, message, command, stdout, stderr, exit_code, duration_ms, timed_out
             FROM task_logs WHERE task_run_id = ?1 ORDER BY sequence ASC",
        )
        .map_err(|error| format!("Could not prepare task log query: {error}"))?;
    let logs = statement
        .query_map([task_id], |row| {
            Ok(TaskLog {
                id: row.get(0)?,
                task_run_id: row.get(1)?,
                level: parse_level(&row.get::<_, String>(2)?),
                timestamp: row.get(3)?,
                message: row.get(4)?,
                command: row.get(5)?,
                stdout: row.get(6)?,
                stderr: row.get(7)?,
                exit_code: row.get(8)?,
                duration_ms: row.get::<_, Option<i64>>(9)?.map(|value| value as u64),
                timed_out: row.get::<_, Option<i64>>(10)?.map(|value| value != 0),
            })
        })
        .map_err(|error| format!("Could not query logs for task {task_id}: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Could not decode logs for task {task_id}: {error}"))?;
    Ok(logs)
}

fn status_label(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Queued => "queued",
        TaskStatus::Running => "running",
        TaskStatus::Success => "success",
        TaskStatus::Failed => "failed",
        TaskStatus::Interrupted => "interrupted",
    }
}

fn parse_status(value: &str) -> TaskStatus {
    match value {
        "queued" => TaskStatus::Queued,
        "running" => TaskStatus::Running,
        "success" => TaskStatus::Success,
        "interrupted" => TaskStatus::Interrupted,
        _ => TaskStatus::Failed,
    }
}

fn level_label(level: &TaskLogLevel) -> &'static str {
    match level {
        TaskLogLevel::Info => "info",
        TaskLogLevel::Warn => "warn",
        TaskLogLevel::Error => "error",
    }
}

fn parse_level(value: &str) -> TaskLogLevel {
    match value {
        "warn" => TaskLogLevel::Warn,
        "error" => TaskLogLevel::Error,
        _ => TaskLogLevel::Info,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, status: TaskStatus) -> TaskRun {
        TaskRun {
            id: id.into(),
            host_id: "local".into(),
            host_name: "Local".into(),
            action: "Test task".into(),
            status,
            started_at: "2026-07-10T00:00:00+08:00".into(),
            ended_at: None,
            summary: "Safe summary".into(),
            logs: vec![TaskLog {
                id: format!("log-{id}"),
                task_run_id: id.into(),
                level: TaskLogLevel::Info,
                timestamp: "2026-07-10T00:00:00+08:00".into(),
                message: "Safe log".into(),
                command: None,
                stdout: None,
                stderr: None,
                exit_code: Some(0),
                duration_ms: Some(10),
                timed_out: Some(false),
            }],
        }
    }

    #[test]
    fn task_history_round_trips_and_acknowledges() {
        let store = TaskStore::in_memory();
        store
            .upsert(&task("task-1", TaskStatus::Success))
            .expect("persist task");
        let tasks = store.list(20).expect("list tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].logs.len(), 1);
        assert!(store.acknowledge("task-1").expect("acknowledge task"));
    }

    #[test]
    fn startup_marks_incomplete_tasks_interrupted() {
        let store = TaskStore::in_memory();
        store
            .upsert(&task("task-running", TaskStatus::Running))
            .expect("persist running task");
        store.mark_interrupted().expect("mark interrupted");
        assert!(matches!(
            store.get("task-running").expect("get task").unwrap().status,
            TaskStatus::Interrupted
        ));
    }

    #[test]
    fn task_updates_preserve_the_original_started_at() {
        let store = TaskStore::in_memory();
        let mut run = task("task-started-at", TaskStatus::Running);
        run.started_at = "2026-07-10T00:00:00+08:00".into();
        store.upsert(&run).expect("persist running task");

        run.status = TaskStatus::Success;
        run.started_at = "2026-07-10T01:00:00+08:00".into();
        run.ended_at = Some("2026-07-10T01:00:01+08:00".into());
        store.upsert(&run).expect("finish task");

        assert_eq!(
            store
                .get("task-started-at")
                .expect("read task")
                .expect("task exists")
                .started_at,
            "2026-07-10T00:00:00+08:00"
        );
    }

    #[test]
    fn operation_journal_keeps_interrupted_work_visible() {
        let store = TaskStore::in_memory();
        store
            .begin_operation("operation-1", "storage-migration", "{}")
            .expect("begin operation");
        assert_eq!(
            store.pending_operation_ids().expect("pending operations"),
            vec!["operation-1"]
        );
        store
            .finish_operation("operation-1", "completed")
            .expect("finish operation");
        assert!(store
            .pending_operation_ids()
            .expect("completed operations")
            .is_empty());

        store
            .begin_operation("operation-2", "related-json-write", "{}")
            .expect("begin recoverable operation");
        store
            .finish_operation("operation-2", "recovery-required")
            .expect("mark recovery required");
        assert_eq!(
            store.pending_operation_ids().expect("recovery operations"),
            vec!["operation-2"]
        );
    }

    #[test]
    fn backup_metadata_records_and_marks_restore() {
        let store = TaskStore::in_memory();
        let path = Path::new("C:/CodexHub/backups/settings-1.json");
        let id = store
            .record_backup("settings", path, "abc123")
            .expect("record backup");
        assert!(id.starts_with("backup-settings-"));
        store
            .mark_backup_restored(path)
            .expect("mark backup restored");

        let connection = store.connection.lock().expect("task database lock");
        let restored_at: Option<String> = connection
            .query_row(
                "SELECT restored_at FROM storage_backups WHERE backup_id = ?1",
                [id],
                |row| row.get(0),
            )
            .expect("read backup metadata");
        assert!(restored_at.is_some());
    }
}
