//! Explicit opt-in acceptance against a user-authorized SSH alias; fixtures are retained.

use super::*;
use crate::storage::WorkspaceSqliteRecoveryPersistence;
use crate::workspace::events::{WorkspaceEventSink, TRANSFER_UPDATED_EVENT};
use crate::workspace::files::FileSessions;
use crate::workspace::operations::FileOperations;
use crate::workspace::transfer_io::TransferIo;
use crate::workspace::transfers::TransferQueue;
use crate::workspace::types::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::Instant;

const FILE_BYTES: u64 = 100 * 1024 * 1024;
const PAUSE_AT_BYTES: u64 = 40 * 1024 * 1024;

fn identity(transfer: &TransferDto) -> TransferIdentityRequest {
    TransferIdentityRequest {
        transfer_id: transfer.transfer_id.clone(),
        revision: None,
        file_session_id: None,
        local_grant_id: None,
        restart: false,
    }
}

fn file_sha256(path: &Path) -> String {
    use std::io::Read;
    let mut file = std::fs::File::open(path).unwrap();
    let mut hash = Sha256::new();
    let mut chunk = vec![0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut chunk).unwrap();
        if read == 0 {
            break;
        }
        hash.update(&chunk[..read]);
    }
    format!("{:x}", hash.finalize())
}

fn verification_ms(events: &[Value], id: &str) -> u64 {
    let time = |state: &str| {
        events
            .iter()
            .find(|event| event["transferId"] == id && event["state"] == state)
            .and_then(|event| event["elapsedMs"].as_u64())
            .unwrap()
    };
    time("finalizing") - time("verifying")
}

async fn run_with_resume(
    io: &TransferIo,
    queue: &TransferQueue,
    files: &FileSessions,
    id: &str,
) -> Value {
    let started = Instant::now();
    let first = queue.claim_next().unwrap().unwrap();
    assert_eq!(first.transfer_id, id);
    io.run_claimed(queue, files, first.clone()).await.unwrap();
    let paused = queue
        .list()
        .unwrap()
        .into_iter()
        .find(|row| row.transfer_id == id)
        .unwrap();
    assert_eq!(
        paused.state,
        TransferState::Paused,
        "The fixture must pause before completion."
    );
    assert_eq!(paused.resume_offset, Some(PAUSE_AT_BYTES));
    queue.resume(identity(&paused)).unwrap();
    let resumed = queue.claim_next().unwrap().unwrap();
    io.run_claimed(queue, files, resumed).await.unwrap();
    let completed = queue
        .list()
        .unwrap()
        .into_iter()
        .find(|row| row.transfer_id == id)
        .unwrap();
    assert_eq!(
        completed.state,
        TransferState::Completed,
        "Transfer failed: {:?}",
        completed.error_code
    );
    assert_eq!(completed.bytes, FILE_BYTES);
    assert_eq!(completed.attempt, 2);
    json!({"totalMs": started.elapsed().as_millis() as u64, "resumeOffset": paused.bytes, "bytes": completed.bytes, "attempts": completed.attempt})
}

async fn measure_hash_traffic(alias: &str, remote_file: &str, expected_hash: &str) -> Value {
    let script = hash_script(remote_file, None).unwrap();
    let mut command = Command::new("ssh");
    configure_tokio_command(&mut command);
    command.args([
        "-v",
        "-T",
        "-o",
        "BatchMode=yes",
        "-o",
        "NumberOfPasswordPrompts=0",
        "-o",
        "ConnectTimeout=15",
        alias,
        "sh",
        "-s",
    ]);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let started = Instant::now();
    let mut child = command.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(script.as_bytes()).await.unwrap();
    drop(stdin);
    let output = tokio::time::timeout(Duration::from_secs(30), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        output.status.success(),
        "SSH hash measurement failed with {:?}.",
        output.status.code()
    );
    assert_eq!(parse_digest(&output.stdout).unwrap(), expected_hash);
    assert_eq!(output.stdout.len(), 65);
    // OpenSSH reports counts for this connection only, excluding other apps.
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    let summary = diagnostics
        .lines()
        .find(|line| line.starts_with("Transferred:"))
        .expect("OpenSSH transfer counters");
    let fields = summary.split_whitespace().collect::<Vec<_>>();
    let sent = fields[2].trim_end_matches(',').parse::<u64>().unwrap();
    let received = fields[4].parse::<u64>().unwrap();
    assert!(
        received < 64 * 1024,
        "Verification must not download the 100 MiB file."
    );
    json!({"elapsedMs": started.elapsed().as_millis() as u64, "digestPayloadBytes": output.stdout.len(), "sshSentBytes": sent, "sshReceivedBytes": received})
}

#[tokio::test]
#[ignore = "Requires an explicitly authorized CODEXHUB_LIVE_SSH_ALIAS and isolated fixture directories."]
async fn live_100mib_upload_download_with_resume() {
    let alias = crate::ssh::validate_ssh_alias(
        &std::env::var("CODEXHUB_LIVE_SSH_ALIAS").expect("Explicit SSH test alias"),
    )
    .unwrap();
    let local_root = PathBuf::from(
        std::env::var("CODEXHUB_LIVE_LOCAL_DIR").expect("Isolated local fixture directory"),
    );
    let remote_root =
        std::env::var("CODEXHUB_LIVE_REMOTE_DIR").expect("Isolated remote fixture directory");
    assert!(
        remote_root.starts_with("/tmp/codexhub-transfer-verify.")
            && !remote_root.contains(['\n', '\r', '\'', ' '])
    );
    let source = local_root.join("source-100MiB.bin");
    assert_eq!(std::fs::metadata(&source).unwrap().len(), FILE_BYTES);
    let expected_hash = file_sha256(&source);
    let download_dir = local_root.join("download");
    std::fs::create_dir(&download_dir).unwrap();
    let remote_file = format!("{remote_root}/source-100MiB.bin");

    let started = Instant::now();
    let queue_holder = Arc::new(Mutex::new(None::<Weak<TransferQueue>>));
    let paused_ids = Arc::new(Mutex::new(HashSet::<String>::new()));
    let states = Arc::new(Mutex::new(HashMap::<String, String>::new()));
    let events = Arc::new(Mutex::new(Vec::<Value>::new()));
    let sink: WorkspaceEventSink = {
        let queue_holder = queue_holder.clone();
        let paused_ids = paused_ids.clone();
        let states = states.clone();
        let events = events.clone();
        Arc::new(move |name, event| {
            if name != TRANSFER_UPDATED_EVENT {
                return Ok(());
            }
            let id = event["transferId"].as_str().unwrap();
            let state = event["state"].as_str().unwrap();
            let bytes = event["bytes"].as_u64().unwrap();
            let previous = states.lock().unwrap().insert(id.into(), state.into());
            if previous.as_deref() != Some(state) {
                println!("LIVE stage={state} bytes={bytes}");
                events.lock().unwrap().push(json!({"transferId": id, "state": state, "bytes": bytes, "elapsedMs": started.elapsed().as_millis() as u64}));
            }
            // Pause synchronously at a real progress event, once per direction.
            if state == "running"
                && bytes >= PAUSE_AT_BYTES
                && paused_ids.lock().unwrap().insert(id.into())
            {
                let queue = queue_holder
                    .lock()
                    .unwrap()
                    .as_ref()
                    .and_then(Weak::upgrade)
                    .unwrap();
                queue
                    .pause(TransferIdentityRequest {
                        transfer_id: id.into(),
                        revision: None,
                        file_session_id: None,
                        local_grant_id: None,
                        restart: false,
                    })
                    .map_err(|error| error.to_string())?;
            }
            Ok(())
        })
    };
    let queue = Arc::new(TransferQueue::in_memory(Some(sink)));
    *queue_holder.lock().unwrap() = Some(Arc::downgrade(&queue));
    let files = FileSessions::new(None);
    let session = files
        .open(OpenFilesRequest {
            local: false,
            host_id: "live-test".into(),
            host_name: "live-test".into(),
            host_alias: alias.clone(),
        })
        .await
        .unwrap()
        .session;
    assert!(session.supports_fsync && session.supports_hardlink);
    let operations = Arc::new(FileOperations::new(Box::new(
        WorkspaceSqliteRecoveryPersistence::in_memory(),
    )));
    let io = TransferIo::with_file_operations(operations);

    let grant = io
        .local_grants
        .grant_upload_file(source.clone())
        .await
        .unwrap();
    let upload = queue
        .enqueue(EnqueueTransfersRequest {
            file_session_id: session.file_session_id.clone(),
            items: vec![TransferDraft {
                direction: TransferDirection::Upload,
                host_id: session.host_id.clone(),
                host_name: session.host_name.clone(),
                host_alias: alias.clone(),
                source_ref: grant.grant_id,
                local_grant_id: None,
                destination_path: remote_file.clone(),
                total_bytes: Some(FILE_BYTES),
                conflict_strategy: Some(ConflictStrategy::Ask),
            }],
        })
        .unwrap()
        .remove(0);
    io.bind_upload(&queue, &upload, session.file_session_id.clone())
        .await
        .unwrap();
    let mut upload_report = run_with_resume(&io, &queue, &files, &upload.transfer_id).await;
    assert_eq!(
        files
            .transfer_fingerprint(&session.file_session_id, &remote_file)
            .await
            .unwrap()
            .size,
        FILE_BYTES
    );
    upload_report["verificationMs"] =
        verification_ms(&events.lock().unwrap(), &upload.transfer_id).into();
    println!("LIVE upload={upload_report}");

    let grant = io
        .local_grants
        .grant_download_directory(download_dir.clone())
        .await
        .unwrap();
    let selected_directory = io.consume_download_directory(&grant.grant_id).unwrap();
    let download = queue
        .enqueue(EnqueueTransfersRequest {
            file_session_id: session.file_session_id.clone(),
            items: vec![TransferDraft {
                direction: TransferDirection::Download,
                host_id: session.host_id.clone(),
                host_name: session.host_name.clone(),
                host_alias: alias.clone(),
                source_ref: remote_file.clone(),
                local_grant_id: None,
                destination_path: "download".into(),
                total_bytes: Some(FILE_BYTES),
                conflict_strategy: Some(ConflictStrategy::Ask),
            }],
        })
        .unwrap()
        .remove(0);
    io.bind_download(
        &queue,
        &download,
        &files,
        session.file_session_id.clone(),
        selected_directory,
    )
    .await
    .unwrap();
    let mut download_report = run_with_resume(&io, &queue, &files, &download.transfer_id).await;
    download_report["verificationMs"] =
        verification_ms(&events.lock().unwrap(), &download.transfer_id).into();
    let downloaded = download_dir.join("source-100MiB.bin");
    assert_eq!(std::fs::metadata(&downloaded).unwrap().len(), FILE_BYTES);
    assert_eq!(file_sha256(&downloaded), expected_hash);
    println!("LIVE download={download_report}");

    let traffic = measure_hash_traffic(&alias, &remote_file, &expected_hash).await;
    files.close(&session.file_session_id).await.unwrap();
    let report = json!({
        "fileBytes": FILE_BYTES, "sha256": expected_hash, "upload": upload_report, "download": download_report,
        "verificationTraffic": traffic, "localDirectory": local_root, "remoteDirectory": remote_root,
        "backend": "CodexHub FileSessions + TransferIo", "fixturesRetained": true, "events": *events.lock().unwrap(),
    });
    std::fs::write(
        local_root.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("LIVE RESULT {}", serde_json::to_string(&report).unwrap());
}
