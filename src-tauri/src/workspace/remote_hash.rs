//! Transfer verification reads file content on the server and returns only a digest.

#[cfg(test)]
#[path = "transfer_live_tests.rs"]
mod live_tests;

use super::background_process::configure_tokio_command;
use super::error::{WorkspaceError, WorkspaceResult};
use super::remote_path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

const MAX_HASH_OUTPUT_BYTES: u64 = 128;
const HASH_BASE_TIMEOUT_SECONDS: u64 = 120;
const HASH_BYTES_PER_SECOND: u64 = 1024 * 1024;
const HASH_MAX_TIMEOUT_SECONDS: u64 = 24 * 60 * 60;

pub(crate) fn cancelled() -> WorkspaceError {
    WorkspaceError::new(
        "transfer-verify-cancelled",
        "Transfer verification was paused or cancelled.",
    )
}

pub(crate) async fn sha256(
    alias: &str,
    path: &str,
    length: Option<u64>,
    file_size: u64,
    cancel: &CancellationToken,
) -> WorkspaceResult<String> {
    let alias = crate::ssh::validate_ssh_alias(alias).map_err(|_| {
        WorkspaceError::new("invalid-host-alias", "Invalid verification host alias.")
    })?;
    let script = hash_script(path, length)?;
    let mut command = Command::new("ssh");
    configure_tokio_command(&mut command);
    // Keep the Files session's OpenSSH alias, agent, proxy and host-key policy.
    command.args([
        "-T",
        "-o",
        "BatchMode=yes",
        "-o",
        "NumberOfPasswordPrompts=0",
        "-o",
        "ConnectTimeout=15",
        "-o",
        "ServerAliveInterval=15",
        "-o",
        "ServerAliveCountMax=3",
        &alias,
        "sh",
        "-s",
    ]);
    run_hash_command(
        command,
        &script,
        hash_timeout(length.unwrap_or(file_size)),
        cancel,
    )
    .await
}

fn hash_timeout(bytes: u64) -> Duration {
    Duration::from_secs(
        HASH_BASE_TIMEOUT_SECONDS
            .saturating_add(bytes.div_ceil(HASH_BYTES_PER_SECOND))
            .min(HASH_MAX_TIMEOUT_SECONDS),
    )
}

async fn run_hash_command(
    mut command: Command,
    script: &str,
    timeout: Duration,
    cancel: &CancellationToken,
) -> WorkspaceResult<String> {
    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|_| {
        WorkspaceError::retryable(
            "transfer-verify-ssh-failed",
            "Could not start SSH verification.",
        )
    })?;
    let result = {
        let work = async {
            let mut stdin = child.stdin.take().ok_or_else(|| {
                WorkspaceError::new(
                    "transfer-verify-failed",
                    "Verification stdin is unavailable.",
                )
            })?;
            let stdout = child.stdout.take().ok_or_else(|| {
                WorkspaceError::new(
                    "transfer-verify-failed",
                    "Verification stdout is unavailable.",
                )
            })?;
            stdin.write_all(script.as_bytes()).await.map_err(|_| {
                WorkspaceError::retryable(
                    "transfer-verify-ssh-failed",
                    "Could not send the verification script.",
                )
            })?;
            stdin.shutdown().await.map_err(|_| {
                WorkspaceError::retryable(
                    "transfer-verify-ssh-failed",
                    "Could not finish sending the verification script.",
                )
            })?;
            drop(stdin);
            // Bound stdout even if a server prints a banner or an unexpected payload.
            let mut output = Vec::new();
            stdout
                .take(MAX_HASH_OUTPUT_BYTES + 1)
                .read_to_end(&mut output)
                .await
                .map_err(|_| {
                    WorkspaceError::retryable(
                        "transfer-verify-ssh-failed",
                        "Could not read the verification result.",
                    )
                })?;
            if output.len() as u64 > MAX_HASH_OUTPUT_BYTES {
                return Err(invalid_digest());
            }
            let status = child.wait().await.map_err(|_| {
                WorkspaceError::retryable(
                    "transfer-verify-ssh-failed",
                    "Could not wait for SSH verification.",
                )
            })?;
            if !status.success() {
                return Err(match status.code() {
                    Some(71) => WorkspaceError::new(
                        "transfer-verify-unavailable",
                        "Remote verification requires sha256sum, shasum or openssl; prefix verification also requires head and wc.",
                    ),
                    Some(73) => WorkspaceError::new(
                        "transfer-prefix-short",
                        "The remote file is shorter than the saved transfer offset.",
                    ),
                    Some(74) => WorkspaceError::new(
                        "transfer-verify-not-file",
                        "Remote verification requires a regular non-link file.",
                    ),
                    Some(255) => WorkspaceError::retryable(
                        "transfer-verify-ssh-failed",
                        "SSH could not run remote transfer verification.",
                    ),
                    _ => WorkspaceError::new(
                        "transfer-verify-failed",
                        "Remote SHA-256 calculation failed; the partial file is retained.",
                    ),
                });
            }
            parse_digest(&output)
        };
        tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(cancelled()),
            result = tokio::time::timeout(timeout, work) => result.unwrap_or_else(|_| Err(
                WorkspaceError::retryable("transfer-verify-timeout", "Remote SHA-256 calculation timed out; the partial file is retained.")
            )),
        }
    };
    if result.is_err() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    result
}

fn invalid_digest() -> WorkspaceError {
    WorkspaceError::new(
        "transfer-verify-invalid-digest",
        "Remote verification did not return exactly one SHA-256 digest.",
    )
}

fn parse_digest(output: &[u8]) -> WorkspaceResult<String> {
    let digest = output
        .strip_suffix(b"\r\n")
        .or_else(|| output.strip_suffix(b"\n"))
        .unwrap_or(output);
    if digest.len() != 64 || !digest.iter().all(u8::is_ascii_hexdigit) {
        return Err(invalid_digest());
    }
    Ok(String::from_utf8_lossy(digest).to_ascii_lowercase())
}

fn hash_script(path: &str, length: Option<u64>) -> WorkspaceResult<String> {
    remote_path::validate_absolute(path)?;
    if length.is_some_and(|value| value > i64::MAX as u64) {
        return Err(WorkspaceError::new(
            "invalid-resume-offset",
            "The verification prefix is too large.",
        ));
    }
    let quoted_path = format!("'{}'", path.replace('\'', "'\\''"));
    let prefix = length.map(|value| value.to_string()).unwrap_or_default();
    Ok(format!(
        "set -uf\nLC_ALL=C\nexport LC_ALL\nch_path={quoted_path}\nch_length='{prefix}'\n{}",
        r#"
[ -f "$ch_path" ] && [ ! -L "$ch_path" ] || exit 74
if command -v sha256sum >/dev/null 2>&1; then
  ch_tool=sha256sum
elif command -v shasum >/dev/null 2>&1; then
  ch_tool=shasum
elif command -v openssl >/dev/null 2>&1; then
  ch_tool=openssl
else
  exit 71
fi

# Hash stdin so filenames never appear in the digest output.
ch_hash_stream() {
  case "$ch_tool" in
    sha256sum) ch_raw=$(sha256sum) || return 1 ;;
    shasum) ch_raw=$(shasum -a 256) || return 1 ;;
    openssl) ch_raw=$(openssl dgst -sha256) || return 1 ;;
  esac
  set -- $ch_raw
  [ "$#" -gt 0 ] || return 1
  if [ "$ch_tool" = openssl ]; then
    for ch_word do ch_digest=$ch_word; done
  else
    ch_digest=$1
  fi
  printf 'hash:%s\n' "$ch_digest"
}

if [ -n "$ch_length" ]; then
  command -v head >/dev/null 2>&1 && command -v wc >/dev/null 2>&1 || exit 71
  ch_size=$(wc -c < "$ch_path") || exit 72
  set -- $ch_size
  [ "$#" -eq 1 ] || exit 72
  case "$1" in ''|*[!0-9]*) exit 72 ;; esac
  [ "$1" -ge "$ch_length" ] || exit 73
  # Preserve the producer status separately: POSIX pipelines only report the
  # last command's status, which could otherwise hide a failed prefix read.
  ch_result=$(
    exec 3>&1
    { head -c "$ch_length" < "$ch_path"; printf 'read:%s\n' "$?" >&3; } | ch_hash_stream
  ) || exit 72
  ch_read_ok=no
  ch_hash=
  for ch_record in $ch_result; do
    case "$ch_record" in
      read:0) [ "$ch_read_ok" = no ] || exit 72; ch_read_ok=yes ;;
      hash:*) [ -z "$ch_hash" ] || exit 72; ch_hash=${ch_record#hash:} ;;
      *) exit 72 ;;
    esac
  done
  [ "$ch_read_ok" = yes ] && [ -n "$ch_hash" ] || exit 72
else
  ch_result=$(ch_hash_stream < "$ch_path") || exit 72
  ch_hash=${ch_result#hash:}
fi
printf '%s\n' "$ch_hash"
"#
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::path::PathBuf;

    // Unix CI uses /bin/sh. Windows can exercise the same script with Git's sh.
    fn shell_command() -> Option<Command> {
        #[cfg(unix)]
        let shell = Some(PathBuf::from("/bin/sh"));
        #[cfg(windows)]
        let shell = std::env::var_os("CODEXHUB_TEST_POSIX_SHELL")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::split_paths(&std::env::var_os("PATH")?)
                    .filter_map(|path| path.parent().map(|root| root.join("usr/bin/sh.exe")))
                    .find(|path| path.is_file())
            });
        let Some(shell) = shell else {
            eprintln!("POSIX script test requires sh; set CODEXHUB_TEST_POSIX_SHELL on Windows.");
            return None;
        };
        let mut command = Command::new(shell);
        command.arg("-s").env("PATH", "/usr/bin:/bin:/mingw64/bin");
        configure_tokio_command(&mut command);
        Some(command)
    }

    fn fixture() -> (String, Vec<u8>) {
        let root = std::env::temp_dir().join(format!("codexhub-hash-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("資料 ' $(echo injected) `literal` -file.bin");
        let bytes = (0..(3 * 128 * 1024 + 37))
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        std::fs::write(&path, &bytes).unwrap();
        (path.to_str().unwrap().replace('\\', "/"), bytes)
    }

    async fn run_script(command: Command, script: &str) -> WorkspaceResult<String> {
        run_hash_command(
            command,
            script,
            Duration::from_secs(15),
            &CancellationToken::new(),
        )
        .await
    }

    #[test]
    fn digest_response_rejects_banners_filenames_and_extra_payloads() {
        let digest = "A".repeat(64);
        assert_eq!(
            parse_digest(format!("{digest}\r\n").as_bytes()).unwrap(),
            "a".repeat(64)
        );
        for output in [
            format!("{digest}  /remote/file\n"),
            format!("banner\n{digest}\n"),
            format!("{digest}\n{digest}\n"),
            "z".repeat(64),
            "a".repeat(63),
        ] {
            assert_eq!(
                parse_digest(output.as_bytes()).unwrap_err().code,
                "transfer-verify-invalid-digest"
            );
        }
    }

    #[test]
    fn timeout_scales_with_file_size_without_overflow() {
        assert_eq!(hash_timeout(0), Duration::from_secs(120));
        assert!(hash_timeout(2 * 1024 * 1024 * 1024) > Duration::from_secs(120));
        assert_eq!(
            hash_timeout(u64::MAX),
            Duration::from_secs(HASH_MAX_TIMEOUT_SECONDS)
        );
        assert!(hash_script("/tmp/file", Some(u64::MAX)).is_err());
    }

    #[tokio::test]
    async fn server_script_hashes_binary_files_and_exact_prefixes_with_quoted_paths() {
        let Some(_) = shell_command() else {
            return;
        };
        let (path, bytes) = fixture();
        for length in [
            None,
            Some(0),
            Some(128 * 1024 + 19),
            Some(bytes.len() as u64),
        ] {
            let expected = format!(
                "{:x}",
                Sha256::digest(&bytes[..length.unwrap_or(bytes.len() as u64) as usize])
            );
            let actual = run_script(
                shell_command().unwrap(),
                &hash_script(&path, length).unwrap(),
            )
            .await
            .unwrap();
            assert_eq!(actual, expected, "prefix {length:?}");
        }
    }

    #[tokio::test]
    async fn shasum_and_openssl_fallbacks_preserve_the_same_digest() {
        let Some(_) = shell_command() else {
            return;
        };
        let (path, bytes) = fixture();
        // These wrappers verify each tool's arguments and output format while
        // using the fixture's real SHA-256 implementation as the oracle.
        let shasum = r#"
command() { case "$2" in sha256sum) return 1 ;; *) return 0 ;; esac; }
shasum() { [ "$#" -eq 2 ] && [ "$1" = -a ] && [ "$2" = 256 ] || return 1; /usr/bin/sha256sum; }
"#;
        let openssl = r#"
command() { case "$2" in sha256sum|shasum) return 1 ;; *) return 0 ;; esac; }
openssl() {
  [ "$#" -eq 2 ] && [ "$1" = dgst ] && [ "$2" = -sha256 ] || return 1
  ch_test_raw=$(/usr/bin/sha256sum) || return 1
  printf 'SHA2-256(stdin)= %s\n' "${ch_test_raw%% *}"
}
"#;
        for wrapper in [shasum, openssl] {
            for length in [None, Some(100_003)] {
                let script = format!("{wrapper}{}", hash_script(&path, length).unwrap());
                let actual = run_script(shell_command().unwrap(), &script).await.unwrap();
                let expected = format!(
                    "{:x}",
                    Sha256::digest(&bytes[..length.unwrap_or(bytes.len() as u64) as usize])
                );
                assert_eq!(actual, expected);
            }
        }
    }

    #[tokio::test]
    async fn missing_tools_short_prefixes_and_reader_failures_never_verify() {
        let Some(_) = shell_command() else {
            return;
        };
        let (path, bytes) = fixture();
        let cases = [
            (
                format!(
                    "command() {{ return 1; }}\n{}",
                    hash_script(&path, None).unwrap()
                ),
                "transfer-verify-unavailable",
            ),
            (
                hash_script(&path, Some(bytes.len() as u64 + 1)).unwrap(),
                "transfer-prefix-short",
            ),
            (
                format!(
                    "head() {{ printf partial; return 5; }}\n{}",
                    hash_script(&path, Some(10)).unwrap()
                ),
                "transfer-verify-failed",
            ),
            (
                format!(
                    "sha256sum() {{ printf '%s' {}; return 5; }}\n{}",
                    "a".repeat(64),
                    hash_script(&path, None).unwrap()
                ),
                "transfer-verify-failed",
            ),
            (
                hash_script(&format!("{path}.missing"), None).unwrap(),
                "transfer-verify-not-file",
            ),
        ];
        for (script, expected) in cases {
            assert_eq!(
                run_script(shell_command().unwrap(), &script)
                    .await
                    .unwrap_err()
                    .code,
                expected
            );
        }
    }

    #[tokio::test]
    async fn process_output_is_bounded_and_exit_status_is_required() {
        let Some(_) = shell_command() else {
            return;
        };
        for script in [
            format!("printf '%s' '{}'\n", "a".repeat(256)),
            format!("printf '%s\\n' '{}'\nexit 72\n", "a".repeat(64)),
        ] {
            assert!(run_script(shell_command().unwrap(), &script).await.is_err());
        }
    }

    #[tokio::test]
    async fn stalled_hash_process_can_be_cancelled_or_timed_out() {
        let Some(_) = shell_command() else {
            return;
        };
        let script = "while :; do :; done\n";
        let cancel = CancellationToken::new();
        let pending = run_hash_command(
            shell_command().unwrap(),
            script,
            Duration::from_secs(15),
            &cancel,
        );
        let trigger = async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            cancel.cancel();
        };
        let (result, _) = tokio::join!(pending, trigger);
        assert_eq!(result.unwrap_err().code, "transfer-verify-cancelled");
        let error = run_hash_command(
            shell_command().unwrap(),
            script,
            Duration::from_millis(50),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "transfer-verify-timeout");
    }
}
