use crate::tasks::{TaskLog, TaskLogLevel, TaskRun, TaskStatus, TaskStep, TaskStepStatus};
use chrono::Local;
use rusqlite::{params, Connection, OptionalExtension, Params, Transaction};
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

const CURRENT_TASK_SCHEMA_VERSION: i64 = 4;
const MAX_TASK_HISTORY: usize = 100;
static BACKUP_ID_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static TASK_ARCHIVE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// SQLite is the durable source of truth for task history. The mutex serializes
/// persistence and the bounded local recycle-bin handoff, never SSH or network work.
pub(crate) struct TaskStore {
    connection: Mutex<Connection>,
    unavailable_reason: Option<String>,
    recycle_staging_dir: PathBuf,
}

impl TaskStore {
    pub(crate) fn open(path: &Path) -> Result<Self, String> {
        let parent = path
            .parent()
            .ok_or_else(|| "Task database path has no parent directory.".to_string())?;
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create the task database directory: {error}"))?;
        let mut connection = Connection::open(path)
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
        Self::configure(&mut connection)?;
        let store = Self {
            connection: Mutex::new(connection),
            unavailable_reason: None,
            recycle_staging_dir: parent.join(".task-history-recycle-staging"),
        };
        store.mark_interrupted()?;
        store.enforce_task_retention()?;
        Ok(store)
    }

    #[cfg(test)]
    pub(crate) fn in_memory() -> Self {
        let mut connection = Connection::open_in_memory().expect("open task database in memory");
        Self::configure(&mut connection).expect("configure task database in memory");
        Self {
            connection: Mutex::new(connection),
            unavailable_reason: None,
            recycle_staging_dir: test_recycle_staging_dir(),
        }
    }

    pub(crate) fn unavailable(reason: String) -> Self {
        Self {
            connection: Mutex::new(
                Connection::open_in_memory().expect("open disabled task database handle"),
            ),
            unavailable_reason: Some(reason),
            recycle_staging_dir: test_recycle_staging_dir(),
        }
    }

    fn ensure_available(&self) -> Result<(), String> {
        match self.unavailable_reason.as_ref() {
            Some(reason) => Err(format!("Persistent task storage is unavailable: {reason}")),
            None => Ok(()),
        }
    }

    fn configure(connection: &mut Connection) -> Result<(), String> {
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
                CREATE TABLE IF NOT EXISTS task_recycle_tombstones (
                    task_run_id TEXT PRIMARY KEY,
                    recycled_at TEXT NOT NULL,
                    recycle_reason TEXT NOT NULL
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
                "#,
            )
            .map_err(|error| format!("Could not initialize the task database: {error}"))?;
        connection
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version, name, checksum, applied_at) VALUES(1, 'initial-task-store', 'v1', ?1)",
                [Local::now().to_rfc3339()],
            )
            .map_err(|error| format!("Could not record the task schema migration: {error}"))?;
        connection
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version, name, checksum, applied_at) VALUES(3, 'task-history-system-recycle', 'v3', ?1)",
                [Local::now().to_rfc3339()],
            )
            .map_err(|error| format!("Could not record the task log recycle-bin migration: {error}"))?;
        migrate_task_steps_v4(connection)?;
        Ok(())
    }

    /// Updates one step without replacing sibling steps, which keeps parallel
    /// host probes from losing each other's progress.
    pub(crate) fn update_step(
        &self,
        task_id: &str,
        step: &TaskStep,
        log: Option<&TaskLog>,
        task_summary: Option<&str>,
    ) -> Result<(), String> {
        self.ensure_available()?;
        if step.task_run_id != task_id {
            return Err(format!(
                "Task step {} belongs to a different task.",
                step.step_id
            ));
        }
        if let Some(log) = log {
            if log.task_run_id != task_id || log.step_id.as_deref() != Some(step.step_id.as_str()) {
                return Err(format!(
                    "Task log {} does not belong to step {}.",
                    log.id, step.step_id
                ));
            }
        }

        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "Task database mutex was poisoned.".to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("Could not start the task step transaction: {error}"))?;
        if task_was_recycled(&transaction, task_id)? {
            return Ok(());
        }
        let task_exists = transaction
            .query_row("SELECT 1 FROM task_runs WHERE id = ?1", [task_id], |_| {
                Ok(())
            })
            .optional()
            .map_err(|error| format!("Could not inspect task {task_id}: {error}"))?
            .is_some();
        if !task_exists {
            return Err(format!("Task {task_id} was not found for step progress."));
        }
        upsert_step(&transaction, step)?;
        if let Some(log) = log {
            upsert_incremental_log(&transaction, log)?;
        }
        if let Some(summary) = task_summary {
            transaction
                .execute(
                    "UPDATE task_runs SET summary = ?2 WHERE id = ?1",
                    params![task_id, summary],
                )
                .map_err(|error| format!("Could not update task {task_id} summary: {error}"))?;
        }
        transaction
            .commit()
            .map_err(|error| format!("Could not commit the task step transaction: {error}"))
    }

    pub(crate) fn upsert(&self, task: &TaskRun) -> Result<(), String> {
        self.ensure_available()?;
        if task.steps.iter().any(|step| step.task_run_id != task.id) {
            return Err(format!(
                "Task {} contains a step for another task.",
                task.id
            ));
        }
        if task.logs.iter().any(|log| log.task_run_id != task.id) {
            return Err(format!("Task {} contains a log for another task.", task.id));
        }
        if let Some(log) = task.logs.iter().find(|log| {
            log.step_id
                .as_ref()
                .is_some_and(|step_id| !task.steps.iter().any(|step| step.step_id == *step_id))
        }) {
            return Err(format!(
                "Task log {} references a step missing from task {}.",
                log.id, task.id
            ));
        }
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "Task database mutex was poisoned.".to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| format!("Could not start the task transaction: {error}"))?;
        if task_was_recycled(&transaction, &task.id)? {
            return Ok(());
        }
        upsert_run(&transaction, task)?;
        transaction
            .execute("DELETE FROM task_logs WHERE task_run_id = ?1", [&task.id])
            .map_err(|error| format!("Could not replace task logs: {error}"))?;
        transaction
            .execute("DELETE FROM task_steps WHERE task_run_id = ?1", [&task.id])
            .map_err(|error| format!("Could not replace task steps: {error}"))?;
        for step in &task.steps {
            insert_step(&transaction, step)?;
        }
        for (sequence, log) in task.logs.iter().enumerate() {
            insert_log(&transaction, sequence, log)?;
        }
        transaction
            .commit()
            .map_err(|error| format!("Could not commit the task transaction: {error}"))?;
        drop(connection);
        self.enforce_task_retention()
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
                    steps: Vec::new(),
                    logs: Vec::new(),
                })
            })
            .map_err(|error| format!("Could not query task history: {error}"))?;
        let mut tasks = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Could not decode task history: {error}"))?;
        for task in &mut tasks {
            task.steps = load_steps(&connection, &task.id)?;
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
        load_task_from_connection(&connection, task_id)
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

    /// Archives every completed task to the OS recycle bin before deleting it.
    pub(crate) fn clear_history(&self) -> Result<usize, String> {
        self.ensure_available()?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "Task database mutex was poisoned.".to_string())?;
        let task_ids = completed_task_ids(&connection, None)?;
        archive_and_delete_tasks(
            &mut connection,
            &self.recycle_staging_dir,
            &task_ids,
            "manual-clear",
        )
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
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "Task database mutex was poisoned.".to_string())?;
        let now = Local::now().to_rfc3339();
        let transaction = connection
            .transaction()
            .map_err(|error| format!("Could not start interrupted task recovery: {error}"))?;
        // Step state is recovered with the parent task so the UI never keeps a
        // stale spinner after CodexHub restarts.
        transaction
            .execute(
                "UPDATE task_steps
                 SET status = CASE status
                       WHEN 'running' THEN 'failed'
                       WHEN 'pending' THEN 'skipped'
                       ELSE status
                     END,
                     ended_at = CASE
                       WHEN status IN ('running', 'pending') THEN ?1
                       ELSE ended_at
                     END,
                     summary = CASE status
                       WHEN 'running' THEN summary || ' CodexHub exited before this step completed.'
                       WHEN 'pending' THEN summary || ' Skipped because CodexHub exited before execution.'
                       ELSE summary
                     END
                 WHERE status IN ('running', 'pending')
                   AND task_run_id IN (
                     SELECT id FROM task_runs WHERE status IN ('queued', 'running')
                   )",
                [&now],
            )
            .map_err(|error| format!("Could not recover interrupted task steps: {error}"))?;
        transaction
            .execute(
                "UPDATE task_runs
                 SET status = 'interrupted', ended_at = ?1,
                     summary = summary || ' The previous CodexHub process ended before completion.'
                 WHERE status IN ('queued', 'running')",
                [&now],
            )
            .map_err(|error| format!("Could not recover interrupted tasks: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("Could not commit interrupted task recovery: {error}"))
    }

    fn enforce_task_retention(&self) -> Result<(), String> {
        self.ensure_available()?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| "Task database mutex was poisoned.".to_string())?;
        let total = connection
            .query_row("SELECT COUNT(*) FROM task_runs", [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|error| format!("Could not count task history: {error}"))?
            .max(0) as usize;
        if total <= MAX_TASK_HISTORY {
            return Ok(());
        }
        let task_ids = completed_task_ids(&connection, Some(total - MAX_TASK_HISTORY))?;
        archive_and_delete_tasks(
            &mut connection,
            &self.recycle_staging_dir,
            &task_ids,
            "retention",
        )
        .map(|_| ())
    }
}

/// v4 keeps the migration transactional; `open` creates and validates a
/// SQLite snapshot before this function runs for every existing v1-v3 file.
fn migrate_task_steps_v4(connection: &mut Connection) -> Result<(), String> {
    let schema_version = connection
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .map_err(|error| {
            format!("Could not inspect the task schema before v4 migration: {error}")
        })?;
    if schema_version >= CURRENT_TASK_SCHEMA_VERSION {
        return Ok(());
    }
    let log_has_step_id = table_has_column(connection, "task_logs", "step_id")?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("Could not start the task schema v4 migration: {error}"))?;
    transaction
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS task_steps (
                task_run_id TEXT NOT NULL REFERENCES task_runs(id) ON DELETE CASCADE,
                step_id TEXT NOT NULL,
                sequence INTEGER NOT NULL,
                status TEXT NOT NULL,
                summary TEXT NOT NULL,
                started_at TEXT,
                ended_at TEXT,
                PRIMARY KEY(task_run_id, step_id),
                UNIQUE(task_run_id, sequence)
            );
            CREATE INDEX IF NOT EXISTS idx_task_steps_status
                ON task_steps(task_run_id, status, sequence);
            "#,
        )
        .map_err(|error| format!("Could not create the task step schema: {error}"))?;
    if !log_has_step_id {
        transaction
            .execute_batch("ALTER TABLE task_logs ADD COLUMN step_id TEXT;")
            .map_err(|error| format!("Could not link task logs to steps: {error}"))?;
    }
    transaction
        .execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_task_logs_step_id
             ON task_logs(task_run_id, step_id, sequence);",
        )
        .map_err(|error| format!("Could not index task step logs: {error}"))?;
    transaction
        .execute(
            "INSERT OR IGNORE INTO schema_migrations(version, name, checksum, applied_at) VALUES(4, 'task-steps', 'v4', ?1)",
            [Local::now().to_rfc3339()],
        )
        .map_err(|error| format!("Could not record the task schema v4 migration: {error}"))?;
    transaction
        .execute_batch("PRAGMA user_version = 4;")
        .map_err(|error| format!("Could not finalize the task schema v4 migration: {error}"))?;
    transaction
        .commit()
        .map_err(|error| format!("Could not commit the task schema v4 migration: {error}"))
}

fn table_has_column(
    connection: &Connection,
    table_name: &str,
    column_name: &str,
) -> Result<bool, String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table_name})"))
        .map_err(|error| format!("Could not inspect table {table_name}: {error}"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| format!("Could not query table {table_name}: {error}"))?;
    for column in columns {
        if column.map_err(|error| format!("Could not decode table {table_name}: {error}"))?
            == column_name
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskHistoryArchive<'a> {
    schema_version: u8,
    exported_at: String,
    reason: &'a str,
    task_count: usize,
    tasks: &'a [TaskRun],
}

fn task_was_recycled(transaction: &Transaction<'_>, task_id: &str) -> Result<bool, String> {
    transaction
        .query_row(
            "SELECT 1 FROM task_recycle_tombstones WHERE task_run_id = ?1",
            [task_id],
            |_| Ok(()),
        )
        .optional()
        .map(|entry| entry.is_some())
        .map_err(|error| format!("Could not inspect task recycle state: {error}"))
}

fn completed_task_ids(
    connection: &Connection,
    limit: Option<usize>,
) -> Result<Vec<String>, String> {
    let base = "SELECT id FROM task_runs
                WHERE status NOT IN ('queued', 'running')
                ORDER BY started_at ASC, rowid ASC";
    match limit {
        Some(limit) => query_task_ids(connection, &format!("{base} LIMIT ?1"), [limit as i64]),
        None => query_task_ids(connection, base, []),
    }
}

fn query_task_ids<P: Params>(
    connection: &Connection,
    sql: &str,
    params: P,
) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| format!("Could not prepare task recycle query: {error}"))?;
    let task_ids = statement
        .query_map(params, |row| row.get(0))
        .map_err(|error| format!("Could not query recyclable tasks: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Could not decode recyclable tasks: {error}"))?;
    Ok(task_ids)
}

fn archive_and_delete_tasks(
    connection: &mut Connection,
    recycle_staging_dir: &Path,
    task_ids: &[String],
    reason: &str,
) -> Result<usize, String> {
    if task_ids.is_empty() {
        return Ok(0);
    }
    let tasks = task_ids
        .iter()
        .map(|task_id| {
            load_task_from_connection(connection, task_id)?
                .ok_or_else(|| format!("Task {task_id} disappeared before it could be recycled."))
        })
        .collect::<Result<Vec<_>, _>>()?;

    stage_archive_in_system_recycle_bin(recycle_staging_dir, reason, &tasks)?;

    let recycled_at = Local::now().to_rfc3339();
    let transaction = connection
        .transaction()
        .map_err(|error| format!("Could not start task history recycle transaction: {error}"))?;
    for task_id in task_ids {
        transaction
            .execute(
                "INSERT OR IGNORE INTO task_recycle_tombstones(task_run_id, recycled_at, recycle_reason)
                 VALUES(?1, ?2, ?3)",
                params![task_id, recycled_at, reason],
            )
            .map_err(|error| format!("Could not record recycled task {task_id}: {error}"))?;
        transaction
            .execute("DELETE FROM task_runs WHERE id = ?1", [task_id])
            .map_err(|error| format!("Could not delete recycled task {task_id}: {error}"))?;
    }
    transaction
        .commit()
        .map_err(|error| format!("Could not commit task history recycling: {error}"))?;
    Ok(tasks.len())
}

fn stage_archive_in_system_recycle_bin(
    recycle_staging_dir: &Path,
    reason: &str,
    tasks: &[TaskRun],
) -> Result<(), String> {
    fs::create_dir_all(recycle_staging_dir)
        .map_err(|error| format!("Could not create task recycle staging directory: {error}"))?;
    let sequence = TASK_ARCHIVE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let archive_path = recycle_staging_dir.join(format!(
        "CodexHub-task-history-{reason}-{}-{sequence}.json",
        Local::now().format("%Y%m%d-%H%M%S-%3f")
    ));
    let bytes = task_archive_bytes(reason, tasks)?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&archive_path)
        .map_err(|error| format!("Could not stage task history archive: {error}"))?;
    if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&archive_path);
        return Err(format!("Could not flush task history archive: {error}"));
    }
    drop(file);

    if let Err(error) = move_file_to_system_recycle_bin(&archive_path) {
        let _ = fs::remove_file(&archive_path);
        return Err(error);
    }
    let _ = fs::remove_dir(recycle_staging_dir);
    Ok(())
}

fn task_archive_bytes(reason: &str, tasks: &[TaskRun]) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(&TaskHistoryArchive {
        schema_version: 1,
        exported_at: Local::now().to_rfc3339(),
        reason,
        task_count: tasks.len(),
        tasks,
    })
    .map_err(|error| format!("Could not serialize task history archive: {error}"))
}

#[cfg(not(test))]
fn move_file_to_system_recycle_bin(path: &Path) -> Result<(), String> {
    trash::delete(path).map_err(|error| {
        format!("Could not move task history archive to the system recycle bin: {error}")
    })
}

#[cfg(test)]
fn move_file_to_system_recycle_bin(path: &Path) -> Result<(), String> {
    fs::remove_file(path)
        .map_err(|error| format!("Could not simulate task history recycling: {error}"))
}

fn test_recycle_staging_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "codexhub-task-recycle-{}-{}",
        std::process::id(),
        TASK_ARCHIVE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))
}

/// Schema upgrades checkpoint WAL first and use SQLite's own consistent snapshot.
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

fn insert_step(transaction: &Transaction<'_>, step: &TaskStep) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO task_steps(task_run_id, step_id, sequence, status, summary, started_at, ended_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                step.task_run_id,
                step.step_id,
                i64::from(step.sequence),
                step_status_label(&step.status),
                step.summary,
                step.started_at,
                step.ended_at
            ],
        )
        .map(|_| ())
        .map_err(|error| {
            format!(
                "Could not persist task step {} for {}: {error}",
                step.step_id, step.task_run_id
            )
        })
}

fn upsert_step(transaction: &Transaction<'_>, step: &TaskStep) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO task_steps(task_run_id, step_id, sequence, status, summary, started_at, ended_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(task_run_id, step_id) DO UPDATE SET
               sequence=excluded.sequence, status=excluded.status, summary=excluded.summary,
               started_at=excluded.started_at, ended_at=excluded.ended_at",
            params![
                step.task_run_id,
                step.step_id,
                i64::from(step.sequence),
                step_status_label(&step.status),
                step.summary,
                step.started_at,
                step.ended_at
            ],
        )
        .map(|_| ())
        .map_err(|error| {
            format!(
                "Could not update task step {} for {}: {error}",
                step.step_id, step.task_run_id
            )
        })
}

fn insert_log(transaction: &Transaction<'_>, sequence: usize, log: &TaskLog) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO task_logs(id, task_run_id, sequence, level, timestamp, message, command, stdout, stderr, exit_code, duration_ms, timed_out, step_id)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
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
                log.timed_out.map(i64::from),
                log.step_id
            ],
        )
        .map(|_| ())
        .map_err(|error| format!("Could not persist task log {}: {error}", log.id))
}

fn upsert_incremental_log(transaction: &Transaction<'_>, log: &TaskLog) -> Result<(), String> {
    let existing_sequence = transaction
        .query_row(
            "SELECT sequence FROM task_logs WHERE id = ?1",
            [&log.id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|error| format!("Could not inspect task log {}: {error}", log.id))?;
    if let Some(sequence) = existing_sequence {
        transaction
            .execute(
                "UPDATE task_logs
                 SET task_run_id=?2, sequence=?3, level=?4, timestamp=?5, message=?6,
                     command=?7, stdout=?8, stderr=?9, exit_code=?10, duration_ms=?11,
                     timed_out=?12, step_id=?13
                 WHERE id=?1",
                params![
                    log.id,
                    log.task_run_id,
                    sequence,
                    level_label(&log.level),
                    log.timestamp,
                    log.message,
                    log.command,
                    log.stdout,
                    log.stderr,
                    log.exit_code,
                    log.duration_ms.map(|value| value as i64),
                    log.timed_out.map(i64::from),
                    log.step_id
                ],
            )
            .map(|_| ())
            .map_err(|error| format!("Could not update task log {}: {error}", log.id))
    } else {
        let next_sequence = transaction
            .query_row(
                "SELECT COALESCE(MAX(sequence), -1) + 1 FROM task_logs WHERE task_run_id = ?1",
                [&log.task_run_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| format!("Could not allocate task log sequence: {error}"))?;
        insert_log(transaction, next_sequence.max(0) as usize, log)
    }
}

fn load_task_from_connection(
    connection: &Connection,
    task_id: &str,
) -> Result<Option<TaskRun>, String> {
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
                    steps: Vec::new(),
                    logs: Vec::new(),
                })
            },
        )
        .optional()
        .map_err(|error| format!("Could not read task {task_id}: {error}"))?;
    if let Some(task) = &mut task {
        task.steps = load_steps(connection, task_id)?;
        task.logs = load_logs(connection, task_id)?;
    }
    Ok(task)
}

fn load_steps(connection: &Connection, task_id: &str) -> Result<Vec<TaskStep>, String> {
    let mut statement = connection
        .prepare(
            "SELECT task_run_id, step_id, sequence, status, summary, started_at, ended_at
             FROM task_steps WHERE task_run_id = ?1 ORDER BY sequence ASC",
        )
        .map_err(|error| format!("Could not prepare task step query: {error}"))?;
    let steps = statement
        .query_map([task_id], |row| {
            Ok(TaskStep {
                task_run_id: row.get(0)?,
                step_id: row.get(1)?,
                sequence: row.get::<_, i64>(2)?.max(0) as u32,
                status: parse_step_status(&row.get::<_, String>(3)?),
                summary: row.get(4)?,
                started_at: row.get(5)?,
                ended_at: row.get(6)?,
            })
        })
        .map_err(|error| format!("Could not query steps for task {task_id}: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Could not decode steps for task {task_id}: {error}"))?;
    Ok(steps)
}

fn load_logs(connection: &Connection, task_id: &str) -> Result<Vec<TaskLog>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, task_run_id, step_id, level, timestamp, message, command, stdout, stderr, exit_code, duration_ms, timed_out
             FROM task_logs WHERE task_run_id = ?1 ORDER BY sequence ASC",
        )
        .map_err(|error| format!("Could not prepare task log query: {error}"))?;
    let logs = statement
        .query_map([task_id], |row| {
            Ok(TaskLog {
                id: row.get(0)?,
                task_run_id: row.get(1)?,
                step_id: row.get(2)?,
                level: parse_level(&row.get::<_, String>(3)?),
                timestamp: row.get(4)?,
                message: row.get(5)?,
                command: row.get(6)?,
                stdout: row.get(7)?,
                stderr: row.get(8)?,
                exit_code: row.get(9)?,
                duration_ms: row.get::<_, Option<i64>>(10)?.map(|value| value as u64),
                timed_out: row.get::<_, Option<i64>>(11)?.map(|value| value != 0),
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

fn step_status_label(status: &TaskStepStatus) -> &'static str {
    match status {
        TaskStepStatus::Pending => "pending",
        TaskStepStatus::Running => "running",
        TaskStepStatus::Success => "success",
        TaskStepStatus::Failed => "failed",
        TaskStepStatus::Skipped => "skipped",
    }
}

fn parse_step_status(value: &str) -> TaskStepStatus {
    match value {
        "running" => TaskStepStatus::Running,
        "success" => TaskStepStatus::Success,
        "failed" => TaskStepStatus::Failed,
        "skipped" => TaskStepStatus::Skipped,
        _ => TaskStepStatus::Pending,
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

    fn task_step(task_id: &str, step_id: &str, sequence: u32, status: TaskStepStatus) -> TaskStep {
        TaskStep {
            task_run_id: task_id.into(),
            step_id: step_id.into(),
            sequence,
            status,
            summary: format!("Safe step {step_id}"),
            started_at: None,
            ended_at: None,
        }
    }

    fn task_log(task_id: &str, index: usize) -> TaskLog {
        TaskLog {
            id: format!("log-{task_id}-{index:03}"),
            task_run_id: task_id.into(),
            step_id: None,
            level: TaskLogLevel::Info,
            timestamp: format!("2026-07-10T00:{:02}:00+08:00", index % 60),
            message: format!("Safe log {index}"),
            command: None,
            stdout: None,
            stderr: None,
            exit_code: Some(0),
            duration_ms: Some(10),
            timed_out: Some(false),
        }
    }

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
            steps: Vec::new(),
            logs: vec![task_log(id, 0)],
        }
    }

    #[test]
    fn task_history_round_trips_and_acknowledges() {
        let store = TaskStore::in_memory();
        let mut run = task("task-1", TaskStatus::Success);
        run.steps = vec![task_step(&run.id, "prepare", 0, TaskStepStatus::Success)];
        run.logs[0].step_id = Some("prepare".into());
        store.upsert(&run).expect("persist task");
        let tasks = store.list(20).expect("list tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].steps.len(), 1);
        assert_eq!(tasks[0].steps[0].step_id, "prepare");
        assert_eq!(tasks[0].logs.len(), 1);
        assert_eq!(tasks[0].logs[0].step_id.as_deref(), Some("prepare"));
        assert!(store.acknowledge("task-1").expect("acknowledge task"));
    }

    #[test]
    fn startup_marks_incomplete_tasks_interrupted() {
        let store = TaskStore::in_memory();
        let mut run = task("task-running", TaskStatus::Running);
        run.steps = vec![
            task_step(&run.id, "running", 0, TaskStepStatus::Running),
            task_step(&run.id, "pending", 1, TaskStepStatus::Pending),
            task_step(&run.id, "complete", 2, TaskStepStatus::Success),
        ];
        store.upsert(&run).expect("persist running task");
        store.mark_interrupted().expect("mark interrupted");
        let recovered = store.get("task-running").expect("get task").unwrap();
        assert!(matches!(recovered.status, TaskStatus::Interrupted));
        assert!(matches!(recovered.steps[0].status, TaskStepStatus::Failed));
        assert!(matches!(recovered.steps[1].status, TaskStepStatus::Skipped));
        assert!(matches!(recovered.steps[2].status, TaskStepStatus::Success));
        assert!(recovered.steps[0].ended_at.is_some());
        assert!(recovered.steps[1].ended_at.is_some());
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
    fn task_history_retention_keeps_latest_hundred_and_recycles_older_tasks() {
        let store = TaskStore::in_memory();
        for index in 0..105 {
            let mut run = task(&format!("task-retention-{index:03}"), TaskStatus::Success);
            run.started_at = format!("2026-07-10T00:00:{index:03}+08:00");
            store.upsert(&run).expect("persist retained task");
        }

        let stored = store.list(200).expect("read retained task history");
        assert_eq!(stored.len(), MAX_TASK_HISTORY);
        assert_eq!(
            stored.last().map(|task| task.id.as_str()),
            Some("task-retention-005")
        );
        assert!(store
            .get("task-retention-004")
            .expect("read recycled task")
            .is_none());

        let connection = store.connection.lock().expect("task database lock");
        let recycled: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM task_recycle_tombstones WHERE recycle_reason = 'retention'",
                [],
                |row| row.get(0),
            )
            .expect("count retained task tombstones");
        assert_eq!(recycled, 5);
    }

    #[test]
    fn clearing_task_history_recycles_completed_tasks_and_keeps_active_tasks() {
        let store = TaskStore::in_memory();
        store
            .upsert(&task("task-clear-success", TaskStatus::Success))
            .expect("persist successful task");
        store
            .upsert(&task("task-clear-failed", TaskStatus::Failed))
            .expect("persist failed task");
        store
            .upsert(&task("task-clear-running", TaskStatus::Running))
            .expect("persist running task");

        assert_eq!(store.clear_history().expect("clear task history"), 2);
        let remaining = store.list(10).expect("read remaining task history");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, "task-clear-running");

        let connection = store.connection.lock().expect("task database lock");
        let recycled: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM task_recycle_tombstones WHERE recycle_reason = 'manual-clear'",
                [],
                |row| row.get(0),
            )
            .expect("count cleared task tombstones");
        assert_eq!(recycled, 2);
    }

    #[test]
    fn recycled_tasks_do_not_return_after_a_stale_task_write() {
        let store = TaskStore::in_memory();
        let run = task("task-clear-stale", TaskStatus::Failed);
        store.upsert(&run).expect("persist task before clear");
        store.clear_history().expect("clear task history");

        store.upsert(&run).expect("persist stale task snapshot");

        assert!(store
            .get(&run.id)
            .expect("read task after stale write")
            .is_none());
    }

    #[test]
    fn task_archive_is_recoverable_json_with_complete_logs() {
        let mut run = task("task-archive", TaskStatus::Failed);
        run.logs = (0..105).map(|index| task_log(&run.id, index)).collect();

        let bytes = task_archive_bytes("manual-clear", &[run]).expect("serialize task archive");
        let archive: serde_json::Value =
            serde_json::from_slice(&bytes).expect("parse task archive");

        assert_eq!(archive["schemaVersion"], 1);
        assert_eq!(archive["reason"], "manual-clear");
        assert_eq!(archive["taskCount"], 1);
        assert_eq!(
            archive["tasks"][0]["logs"].as_array().map(Vec::len),
            Some(105)
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

    fn v3_database_path(label: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "codexhub-task-store-{label}-{}-{}",
            std::process::id(),
            BACKUP_ID_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory).expect("create task store test directory");
        directory.join("tasks.sqlite")
    }

    fn create_v3_database(path: &Path, malformed_logs: bool) {
        let connection = Connection::open(path).expect("create v3 task database");
        connection
            .execute_batch(
                r#"
                PRAGMA foreign_keys = ON;
                CREATE TABLE schema_migrations (
                    version INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    checksum TEXT NOT NULL,
                    applied_at TEXT NOT NULL
                );
                CREATE TABLE task_runs (
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
                PRAGMA user_version = 3;
                "#,
            )
            .expect("create v3 task schema");
        if malformed_logs {
            connection
                .execute_batch(
                    "CREATE TABLE task_logs (
                        id TEXT PRIMARY KEY,
                        task_run_id TEXT NOT NULL REFERENCES task_runs(id) ON DELETE CASCADE,
                        level TEXT NOT NULL
                    );",
                )
                .expect("create malformed v3 task logs");
            return;
        }
        connection
            .execute_batch(
                r#"
                CREATE TABLE task_logs (
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
                INSERT INTO task_runs(
                    id, host_id, host_name, action, status, started_at, ended_at, summary
                ) VALUES(
                    'legacy-task', 'legacy-host', 'Legacy host', 'Legacy action', 'success',
                    '2026-07-01T00:00:00+08:00', '2026-07-01T00:00:01+08:00', 'Legacy summary'
                );
                INSERT INTO task_logs(
                    id, task_run_id, sequence, level, timestamp, message
                ) VALUES(
                    'legacy-log', 'legacy-task', 0, 'info',
                    '2026-07-01T00:00:01+08:00', 'Legacy detail'
                );
                "#,
            )
            .expect("create legacy v3 task data");
    }

    fn schema_backup_paths(database_path: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(database_path.parent().expect("database parent"))
            .expect("read database directory")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("codexhub-schema-v3-"))
            })
            .collect()
    }

    #[test]
    fn v3_migration_creates_validated_backup_and_preserves_legacy_history() {
        let path = v3_database_path("migration");
        create_v3_database(&path, false);

        let store = TaskStore::open(&path).expect("migrate v3 task database");
        let legacy = store
            .get("legacy-task")
            .expect("read legacy task")
            .expect("legacy task exists");
        assert!(legacy.steps.is_empty());
        assert_eq!(legacy.logs.len(), 1);
        assert!(legacy.logs[0].step_id.is_none());

        let connection = store.connection.lock().expect("task database lock");
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("read migrated schema version");
        assert_eq!(version, CURRENT_TASK_SCHEMA_VERSION);
        assert!(table_has_column(&connection, "task_logs", "step_id").unwrap());
        drop(connection);

        let backups = schema_backup_paths(&path);
        assert_eq!(backups.len(), 1);
        let backup = Connection::open(&backups[0]).expect("open schema backup");
        let backup_version: i64 = backup
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("read backup schema version");
        let backup_logs: i64 = backup
            .query_row("SELECT COUNT(*) FROM task_logs", [], |row| row.get(0))
            .expect("read backup task logs");
        assert_eq!(backup_version, 3);
        assert_eq!(backup_logs, 1);
    }

    #[test]
    fn v4_migration_rolls_back_partial_schema_changes() {
        let path = v3_database_path("rollback");
        create_v3_database(&path, true);

        let error = match TaskStore::open(&path) {
            Ok(_) => panic!("malformed schema must fail migration"),
            Err(error) => error,
        };
        assert!(error.contains("Could not index task step logs"));
        let connection = Connection::open(&path).expect("reopen failed migration database");
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("read rolled back schema version");
        let task_steps: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'task_steps'",
                [],
                |row| row.get(0),
            )
            .expect("inspect rolled back task steps");
        assert_eq!(version, 3);
        assert_eq!(task_steps, 0);
        assert!(!table_has_column(&connection, "task_logs", "step_id").unwrap());
        assert_eq!(schema_backup_paths(&path).len(), 1);
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
