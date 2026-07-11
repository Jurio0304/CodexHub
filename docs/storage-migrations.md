# Storage, Migration, And Recovery

Date: 2026-07-10

CodexHub keeps human-recoverable low-frequency data in JSON and searchable operational history in SQLite. Neither store contains SSH private keys, one-time passwords, API keys, tokens, or passphrases.

## Responsibilities

| Store | Responsibility |
| --- | --- |
| Versioned JSON | Settings, Hosts, Profiles, and Skill metadata |
| SQLite | Task runs/logs, acknowledgement, schema history, operation journal, backup metadata |
| App cache | Codex latest-version cache, skill inventory, clones, downloads, and staging |
| Managed directories | Skill contents and retained recovery backups |
| OS credential store | Profile API key values only |
| SSH/SFTP/SCP | Remote files with preview, confirmation, timestamped backup, idempotency, and recovery |

Tauri resolves config/cache paths once during setup. Resolution failure makes storage unavailable; there is no relative `.codexhub` or Mock fallback. `CODEX_HOME` must resolve to an absolute path before local Skill writes.

## JSON Schema And Migration

Durable JSON v1 is:

```json
{
  "schemaVersion": 1,
  "updatedAt": "RFC3339 timestamp",
  "data": {}
}
```

Legacy arrays/objects are v0 and remain readable. The compact storage banner groups every affected store into one review flow. A write to v0 is locked until the user:

1. previews a plan containing store, path, source SHA-256, source/target schema, and backup directory;
2. explicitly confirms the plan;
3. has the source fingerprint rechecked;
4. receives a timestamped backup;
5. has staged JSON flushed, parsed, and atomically replaced.

Repeated application of an already completed plan is a no-op. A changed fingerprint returns `storage-migration-stale` and creates neither write nor backup. Corrupt targets never silently load a backup.

Operations that require Host/Profile persistence or profile credential metadata preflight the related stores before starting SSH or changing OS credentials. Known migration blockers therefore fail before external work begins. Multi-host SSH probes still run concurrently; their linked Host/Profile read-modify-write windows are serialized so one completed probe cannot overwrite another probe's state.

The SQLite task store uses schema v3 for task-history retention. It keeps at most 100 task records, exports recyclable completed tasks and their full logs to a validated JSON archive, moves the archive to the operating system recycle bin, and then deletes the corresponding rows. Running and queued tasks remain in SQLite; task tombstones prevent delayed writes from recreating recycled records.

## Atomic Writes And Backups

All JSON is staged in the target directory. Windows replaces existing files with `ReplaceFileW` and creates new files with write-through `MoveFileExW`; the staging handle is closed before replacement. Unix uses same-directory rename and synchronizes the parent directory. Unchanged data creates no backup.

Backups are retained indefinitely. Recovery previews choose the latest validated managed backup and bind it to a SHA-256 token. Restore first backs up the current target, then atomically replaces it and records `restored_at`. Repeating the same restore does not create another backup.

Legacy `skills-inventory.json` and `codex-latest.json` are copied into app cache and checksum-verified. The config copy is retained for later user-reviewed cleanup.

## Related Writes And Compensation

Host/Profile link changes stage and validate every JSON document before the first replacement. `operation_journal` is written first, each existing file is backed up, and replacements occur under one process-wide write lock. If a later replacement fails, committed files are restored in reverse order. Failed compensation leaves the journal in `recovery-required`, which appears in storage health after restart.

OS credential changes use the same compensation principle: read the previous credential, apply the new/delete operation, persist non-secret profile metadata, and restore the previous credential if metadata persistence fails. cc-switch batch import snapshots every affected credential before the first write, rolls back earlier entries in reverse order if any later write fails, and commits profile JSON only after the complete credential batch succeeds. A rollback failure is a redacted `partial-failure` and remains visible as a durable task.

## SQLite

SQLite v1 contains `schema_migrations`, `task_runs`, `task_logs`, `operation_journal`, and `storage_backups`. It enables foreign keys, WAL, `synchronous=FULL`, and a five-second busy timeout. Task updates preserve their first RFC3339 `started_at`; startup converts queued/running tasks to `interrupted`.

Before a future SQLite schema upgrade, CodexHub checkpoints WAL, creates a consistent `VACUUM INTO` backup, validates it with `quick_check`, and then migrates transactionally. The first v1 initialization creates no meaningless backup.

Task storage is a safety gate. A durable write or live operation cannot start if the queued/running transition cannot be persisted, and final persistence failure is returned instead of reporting success.

## Domain Boundaries

Public Tauri command names remain stable. Every Tauri entry point now lives under `commands/` and forwards to a service use case. Storage and credential compensation lives in services; event and credential integrations live in adapters; `ssh.rs`, `resource_monitor.rs`, and `updater.rs` remain compatibility adapters. `lib.rs` only wires modules, while further adapter extraction can continue without changing desktop/mock contracts.
