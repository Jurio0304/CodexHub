use crate::platform;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use russh::client;
use russh::keys::ssh_key;
use russh::{ChannelMsg, Disconnect, MethodKind, MethodSet};
use tokio::runtime::Builder;

const MANAGED_START_PREFIX: &str = "# >>> CodexHub managed host:";
const MANAGED_END_PREFIX: &str = "# <<< CodexHub managed host:";
const DEFAULT_TIMEOUT_MS: u64 = 10_000;
const MIN_TIMEOUT_MS: u64 = 1_000;
const MAX_TIMEOUT_MS: u64 = 120_000;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
const AUTHORIZED_KEYS_INSTALL_SCRIPT: &str = "umask 077; mkdir -p \"$HOME/.ssh\" && touch \"$HOME/.ssh/authorized_keys\" && IFS= read -r key && if grep -qxF \"$key\" \"$HOME/.ssh/authorized_keys\" 2>/dev/null; then printf 'authorized_keys already contains key\\n'; else printf '%s\\n' \"$key\" >> \"$HOME/.ssh/authorized_keys\" && printf 'authorized_keys updated\\n'; fi";
const AUTHORIZED_KEYS_PERMISSIONS_SCRIPT: &str = "chmod 700 \"$HOME/.ssh\" && chmod 600 \"$HOME/.ssh/authorized_keys\" && printf 'permissions set\\n'";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshKeyInfo {
    pub key_type: String,
    pub private_path: String,
    pub public_path: String,
    pub private_exists: bool,
    pub public_exists: bool,
    pub public_key: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshStatus {
    pub ssh_dir: String,
    pub config_path: String,
    pub ssh_keygen_available: bool,
    pub preferred_identity_file: String,
    pub ed25519: SshKeyInfo,
    pub rsa: SshKeyInfo,
}

#[derive(Clone, Deserialize, Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SshHostDraft {
    pub alias: String,
    pub host_name: String,
    pub port: u16,
    pub user: String,
    pub identity_file: String,
}

#[derive(Clone, Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SshConfigHost {
    pub alias: String,
    pub host_name: String,
    pub port: u16,
    pub user: String,
    pub identity_file: String,
    pub managed: bool,
    pub source: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshConfigWriteResult {
    pub changed: bool,
    pub action: String,
    pub config_path: String,
    pub backup_path: Option<String>,
    pub host: Option<SshConfigHost>,
    pub message: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshKeyGenerationResult {
    pub private_path: String,
    pub public_path: String,
    pub status: SshStatus,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalKeyPair {
    pub private_path: PathBuf,
    pub public_path: PathBuf,
    pub public_key: String,
    pub generated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshCommandOutput {
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub timed_out: bool,
}

impl SshCommandOutput {
    pub fn success(&self) -> bool {
        self.exit_code == Some(0) && !self.timed_out
    }
}

fn process_command(program: &str) -> Command {
    let mut command = Command::new(program);
    configure_process_window(&mut command);
    command
}

#[cfg(windows)]
fn configure_process_window(command: &mut Command) {
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn configure_process_window(_command: &mut Command) {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasswordBootstrapStep {
    PasswordLogin,
    InstallPublicKey,
    SetPermissions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManagedBlock {
    alias: String,
    range: Range<usize>,
    host: SshConfigHost,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LocalHostBlock {
    aliases: Vec<String>,
    range: Range<usize>,
    host_name: String,
    port: u16,
    user: String,
    identity_file: String,
}

struct LocalHostBlockBuilder {
    aliases: Vec<String>,
    start: usize,
    host_name: String,
    port: u16,
    user: String,
    identity_file: String,
}

pub fn get_ssh_status() -> Result<SshStatus, String> {
    let ssh_dir = ssh_dir()?;
    let config_path = ssh_dir.join("config");
    let ed25519_private = ssh_dir.join("id_ed25519");
    let ed25519_public = ssh_dir.join("id_ed25519.pub");
    let rsa_private = ssh_dir.join("id_rsa");
    let rsa_public = ssh_dir.join("id_rsa.pub");

    let ed25519 = key_info("ed25519", &ed25519_private, &ed25519_public);
    let rsa = key_info("rsa", &rsa_private, &rsa_public);
    let preferred_identity_file = if ed25519.private_exists {
        path_string(&ed25519_private)
    } else if rsa.private_exists {
        path_string(&rsa_private)
    } else {
        path_string(&ed25519_private)
    };

    Ok(SshStatus {
        ssh_dir: path_string(&ssh_dir),
        config_path: path_string(&config_path),
        ssh_keygen_available: command_available("ssh-keygen"),
        preferred_identity_file,
        ed25519,
        rsa,
    })
}

pub fn generate_ed25519_key() -> Result<SshKeyGenerationResult, String> {
    let ssh_dir = ssh_dir()?;
    let private_path = ssh_dir.join("id_ed25519");
    let public_path = ssh_dir.join("id_ed25519.pub");

    if private_path.exists() || public_path.exists() {
        return Err(format!(
            "Refusing to overwrite existing Ed25519 key files: {} or {}",
            path_string(&private_path),
            path_string(&public_path)
        ));
    }

    if !command_available("ssh-keygen") {
        return Err(ssh_keygen_missing_message());
    }

    fs::create_dir_all(&ssh_dir)
        .map_err(|error| format!("Failed to create .ssh directory: {error}"))?;
    set_ssh_dir_permissions(&ssh_dir)?;
    let comment = key_comment();
    let output = process_command("ssh-keygen")
        .arg("-t")
        .arg("ed25519")
        .arg("-f")
        .arg(&private_path)
        .arg("-N")
        .arg("")
        .arg("-C")
        .arg(comment)
        .output()
        .map_err(|error| format!("Failed to run ssh-keygen: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Err(format!("ssh-keygen failed: {detail}"));
    }

    Ok(SshKeyGenerationResult {
        private_path: path_string(&private_path),
        public_path: path_string(&public_path),
        status: get_ssh_status()?,
        message: format!("Generated Ed25519 key at {}", path_string(&private_path)),
    })
}

pub fn ensure_identity_keypair(identity_file: &str) -> Result<LocalKeyPair, String> {
    let private_path = expand_local_path(identity_file)?;
    let public_path = public_key_path_for(&private_path);
    let private_exists = private_path.exists();
    let public_exists = public_path.exists();

    match (private_exists, public_exists) {
        (true, true) => Ok(LocalKeyPair {
            public_key: read_public_key(&public_path)?,
            private_path,
            public_path,
            generated: false,
        }),
        (false, false) => {
            generate_keypair_at(&private_path)?;
            Ok(LocalKeyPair {
                public_key: read_public_key(&public_path)?,
                private_path,
                public_path,
                generated: true,
            })
        }
        (true, false) => {
            derive_public_key(&private_path, &public_path)?;
            Ok(LocalKeyPair {
                public_key: read_public_key(&public_path)?,
                private_path,
                public_path,
                generated: false,
            })
        }
        (false, true) => Err(format!(
            "Public key exists but private key is missing: {}",
            path_string(&private_path)
        )),
    }
}

pub fn prepare_ssh_config_host(draft: SshHostDraft) -> Result<SshConfigHost, String> {
    let draft = normalize_draft(draft)?;
    let path = config_path()?;
    let (existing, _) = read_optional_config(&path)?;
    if find_local_host_block(&existing, &draft.alias)?.is_some() {
        return Ok(local_host_from_draft(&draft));
    }
    Ok(host_from_draft(&draft))
}

pub fn run_password_bootstrap_steps<F, G>(
    draft: &SshHostDraft,
    password: &str,
    public_key: &str,
    timeout_ms: u64,
    mut on_started: F,
    mut on_finished: G,
) -> Vec<(PasswordBootstrapStep, SshCommandOutput)>
where
    F: FnMut(PasswordBootstrapStep),
    G: FnMut(PasswordBootstrapStep, &SshCommandOutput),
{
    let timeout_ms = normalize_timeout_ms(Some(timeout_ms));
    let runtime = match Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            return vec![(
                PasswordBootstrapStep::PasswordLogin,
                failed_output(
                    login_display_command(draft),
                    format!("Failed to start async SSH runtime: {error}"),
                    0,
                    false,
                    password,
                ),
            )]
        }
    };

    runtime.block_on(async move {
        run_password_bootstrap_steps_async(
            draft,
            password,
            public_key,
            timeout_ms,
            &mut on_started,
            &mut on_finished,
        )
        .await
    })
}

async fn run_password_bootstrap_steps_async<F, G>(
    draft: &SshHostDraft,
    password: &str,
    public_key: &str,
    timeout_ms: u64,
    on_started: &mut F,
    on_finished: &mut G,
) -> Vec<(PasswordBootstrapStep, SshCommandOutput)>
where
    F: FnMut(PasswordBootstrapStep),
    G: FnMut(PasswordBootstrapStep, &SshCommandOutput),
{
    let mut outputs = Vec::new();

    on_started(PasswordBootstrapStep::PasswordLogin);
    let login = password_login(draft, password, timeout_ms).await;
    let login_output = login.output.clone();
    on_finished(PasswordBootstrapStep::PasswordLogin, &login_output);
    outputs.push((PasswordBootstrapStep::PasswordLogin, login_output.clone()));
    if !login_output.success() {
        return outputs;
    }
    let Some(mut session) = login.session else {
        return outputs;
    };

    on_started(PasswordBootstrapStep::InstallPublicKey);
    let install_output = run_remote_command(
        &mut session,
        install_display_command(draft),
        AUTHORIZED_KEYS_INSTALL_SCRIPT,
        Some(format!("{}\n", public_key.trim())),
        timeout_ms,
        password,
    )
    .await;
    on_finished(PasswordBootstrapStep::InstallPublicKey, &install_output);
    outputs.push((
        PasswordBootstrapStep::InstallPublicKey,
        install_output.clone(),
    ));
    if !install_output.success() {
        let _ = session
            .disconnect(Disconnect::ByApplication, "", "English")
            .await;
        return outputs;
    }

    on_started(PasswordBootstrapStep::SetPermissions);
    let permissions_output = run_remote_command(
        &mut session,
        permissions_display_command(draft),
        AUTHORIZED_KEYS_PERMISSIONS_SCRIPT,
        None,
        timeout_ms,
        password,
    )
    .await;
    on_finished(PasswordBootstrapStep::SetPermissions, &permissions_output);
    outputs.push((
        PasswordBootstrapStep::SetPermissions,
        permissions_output.clone(),
    ));

    let _ = session
        .disconnect(Disconnect::ByApplication, "", "English")
        .await;
    outputs
}

struct PasswordLoginResult {
    session: Option<client::Handle<AcceptAllServerKey>>,
    output: SshCommandOutput,
}

#[derive(Clone)]
struct AcceptAllServerKey;

impl client::Handler for AcceptAllServerKey {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

async fn password_login(
    draft: &SshHostDraft,
    password: &str,
    timeout_ms: u64,
) -> PasswordLoginResult {
    let start = Instant::now();
    let command = login_display_command(draft);
    let timeout = Duration::from_millis(timeout_ms);
    let future = async {
        let config = client::Config {
            inactivity_timeout: Some(timeout),
            ..Default::default()
        };
        let mut session = client::connect(
            Arc::new(config),
            (draft.host_name.as_str(), draft.port),
            AcceptAllServerKey,
        )
        .await
        .map_err(|error| format!("SSH connection failed: {error}"))?;

        authenticate_with_one_time_password(&mut session, &draft.user, password).await?;

        Ok::<client::Handle<AcceptAllServerKey>, String>(session)
    };

    match tokio::time::timeout(timeout, future).await {
        Ok(Ok(session)) => PasswordLoginResult {
            session: Some(session),
            output: SshCommandOutput {
                command,
                stdout: "password authentication succeeded".into(),
                stderr: String::new(),
                exit_code: Some(0),
                duration_ms: duration_ms(start),
                timed_out: false,
            },
        },
        Ok(Err(error)) => PasswordLoginResult {
            session: None,
            output: failed_output(command, error, duration_ms(start), false, password),
        },
        Err(_) => PasswordLoginResult {
            session: None,
            output: failed_output(
                command,
                format!("SSH password login timed out after {timeout_ms} ms."),
                duration_ms(start),
                true,
                password,
            ),
        },
    }
}

async fn authenticate_with_one_time_password(
    session: &mut client::Handle<AcceptAllServerKey>,
    user: &str,
    password: &str,
) -> Result<(), String> {
    let mut failures = Vec::new();
    let advertised_methods = match session.authenticate_none(user.to_string()).await {
        Ok(client::AuthResult::Success) => return Ok(()),
        Ok(client::AuthResult::Failure {
            remaining_methods, ..
        }) => Some(remaining_methods),
        Err(error) => {
            failures.push(format!("method discovery failed: {error}"));
            None
        }
    };

    if allows_auth_method(advertised_methods.as_ref(), MethodKind::KeyboardInteractive) {
        match keyboard_interactive_auth(session, user, password).await {
            Ok(true) => return Ok(()),
            Ok(false) => failures.push("keyboard-interactive rejected".into()),
            Err(error) => failures.push(error),
        }
    }

    if allows_auth_method(advertised_methods.as_ref(), MethodKind::Password) {
        match session
            .authenticate_password(user.to_string(), password.to_string())
            .await
        {
            Ok(client::AuthResult::Success) => return Ok(()),
            Ok(client::AuthResult::Failure {
                remaining_methods, ..
            }) => failures.push(format!(
                "password rejected; remaining methods: {}",
                auth_methods_label(&remaining_methods)
            )),
            Err(error) => failures.push(format!("password authentication failed: {error}")),
        }
    }

    let advertised = advertised_methods
        .as_ref()
        .map(auth_methods_label)
        .unwrap_or_else(|| "unknown".into());
    if failures.is_empty() {
        failures.push(
            "server did not advertise password or keyboard-interactive authentication".into(),
        );
    }
    Err(format!(
        "Permission denied. Server advertised authentication methods: {advertised}. Attempts: {}",
        failures.join("; ")
    ))
}

async fn keyboard_interactive_auth(
    session: &mut client::Handle<AcceptAllServerKey>,
    user: &str,
    password: &str,
) -> Result<bool, String> {
    let mut response = session
        .authenticate_keyboard_interactive_start(user.to_string(), Some("password".to_string()))
        .await
        .map_err(|error| format!("Keyboard-interactive authentication failed: {error}"))?;

    for _ in 0..3 {
        match response {
            client::KeyboardInteractiveAuthResponse::Success => return Ok(true),
            client::KeyboardInteractiveAuthResponse::Failure {
                remaining_methods, ..
            } => {
                return Err(format!(
                    "Keyboard-interactive authentication was rejected; remaining methods: {}",
                    auth_methods_label(&remaining_methods)
                ))
            }
            client::KeyboardInteractiveAuthResponse::InfoRequest { prompts, .. } => {
                let prompt_count = prompts.len();
                response = session
                    .authenticate_keyboard_interactive_respond(
                        prompts
                            .iter()
                            .map(|prompt| {
                                keyboard_interactive_response(prompt, password, prompt_count)
                            })
                            .collect(),
                    )
                    .await
                    .map_err(|error| {
                        format!("Keyboard-interactive authentication failed: {error}")
                    })?;
            }
        }
    }

    Ok(false)
}

fn allows_auth_method(methods: Option<&MethodSet>, method: MethodKind) -> bool {
    methods.map_or(true, |methods| methods.iter().any(|item| *item == method))
}

fn auth_methods_label(methods: &MethodSet) -> String {
    if methods.is_empty() {
        return "none".into();
    }
    methods
        .iter()
        .map(|method| match method {
            MethodKind::None => "none",
            MethodKind::Password => "password",
            MethodKind::PublicKey => "publickey",
            MethodKind::HostBased => "hostbased",
            MethodKind::KeyboardInteractive => "keyboard-interactive",
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn keyboard_interactive_response(
    prompt: &client::Prompt,
    password: &str,
    prompt_count: usize,
) -> String {
    let lower = prompt.prompt.to_ascii_lowercase();
    if lower.contains("password") || (!prompt.echo && prompt_count == 1) {
        password.to_string()
    } else {
        String::new()
    }
}

async fn run_remote_command(
    session: &mut client::Handle<AcceptAllServerKey>,
    display_command: String,
    command: &str,
    stdin_input: Option<String>,
    timeout_ms: u64,
    password: &str,
) -> SshCommandOutput {
    let start = Instant::now();
    let timeout = Duration::from_millis(timeout_ms);
    let future = async {
        let mut channel = session
            .channel_open_session()
            .await
            .map_err(|error| format!("Failed to open SSH session channel: {error}"))?;
        channel
            .exec(true, command)
            .await
            .map_err(|error| format!("Failed to execute remote command: {error}"))?;
        if let Some(input) = stdin_input {
            channel
                .data_bytes(input.into_bytes())
                .await
                .map_err(|error| format!("Failed to write remote stdin: {error}"))?;
            channel
                .eof()
                .await
                .map_err(|error| format!("Failed to close remote stdin: {error}"))?;
        }

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_code = None;

        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { data } => stdout.extend_from_slice(&data),
                ChannelMsg::ExtendedData { data, .. } => stderr.extend_from_slice(&data),
                ChannelMsg::ExitStatus { exit_status } => {
                    exit_code = Some(i32::try_from(exit_status).unwrap_or(i32::MAX));
                }
                _ => {}
            }
        }

        Ok::<(Vec<u8>, Vec<u8>, Option<i32>), String>((stdout, stderr, exit_code))
    };

    match tokio::time::timeout(timeout, future).await {
        Ok(Ok((stdout, stderr, exit_code))) => SshCommandOutput {
            command: display_command,
            stdout: redact_password(
                &redact_sensitive(&String::from_utf8_lossy(&stdout)),
                password,
            ),
            stderr: redact_password(
                &redact_sensitive(&String::from_utf8_lossy(&stderr)),
                password,
            ),
            exit_code,
            duration_ms: duration_ms(start),
            timed_out: false,
        },
        Ok(Err(error)) => {
            failed_output(display_command, error, duration_ms(start), false, password)
        }
        Err(_) => failed_output(
            display_command,
            format!("Remote command timed out after {timeout_ms} ms."),
            duration_ms(start),
            true,
            password,
        ),
    }
}

fn failed_output(
    command: String,
    message: String,
    duration_ms: u64,
    timed_out: bool,
    password: &str,
) -> SshCommandOutput {
    SshCommandOutput {
        command,
        stdout: String::new(),
        stderr: redact_password(&redact_sensitive(&message), password),
        exit_code: None,
        duration_ms,
        timed_out,
    }
}

fn login_display_command(draft: &SshHostDraft) -> String {
    format!(
        "ssh password login {}@{}:{}",
        draft.user, draft.host_name, draft.port
    )
}

fn install_display_command(draft: &SshHostDraft) -> String {
    format!(
        "ssh password bootstrap {}@{}:{} install authorized_keys",
        draft.user, draft.host_name, draft.port
    )
}

fn permissions_display_command(draft: &SshHostDraft) -> String {
    format!(
        "ssh password bootstrap {}@{}:{} chmod .ssh",
        draft.user, draft.host_name, draft.port
    )
}

pub fn list_ssh_config_hosts() -> Result<Vec<SshConfigHost>, String> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read SSH config {}: {error}", path_string(&path)))?;
    parse_all_ssh_config_hosts(&content)
}

pub fn upsert_ssh_config_host(draft: SshHostDraft) -> Result<SshConfigWriteResult, String> {
    let draft = normalize_draft(draft)?;
    let path = config_path()?;
    let (existing, existed) = read_optional_config(&path)?;
    let managed_exists = find_managed_host_block(&existing, &draft.alias)?.is_some();
    let local_exists = find_local_host_block(&existing, &draft.alias)?.is_some();
    let next = if managed_exists {
        upsert_managed_host_block(&existing, &draft)?
    } else if local_exists {
        upsert_local_host_block(&existing, &draft)?
    } else {
        upsert_managed_host_block(&existing, &draft)?
    };

    if next == existing {
        let host = if local_exists {
            local_host_from_draft(&draft)
        } else {
            host_from_draft(&draft)
        };
        return Ok(SshConfigWriteResult {
            changed: false,
            action: "unchanged".into(),
            config_path: path_string(&path),
            backup_path: None,
            host: Some(host),
            message: format!("Host {} is already up to date.", draft.alias),
        });
    }

    let backup_path = write_config_with_backup(&path, &next, existed)?;
    let (action, host, source_label) = if managed_exists {
        ("updated", host_from_draft(&draft), "CodexHub-managed")
    } else if local_exists {
        ("local_updated", local_host_from_draft(&draft), "local")
    } else {
        ("added", host_from_draft(&draft), "CodexHub-managed")
    };

    Ok(SshConfigWriteResult {
        changed: true,
        action: action.into(),
        config_path: path_string(&path),
        backup_path: backup_path.as_ref().map(|item| path_string(item)),
        host: Some(host),
        message: format!(
            "Host {} was updated in the {source_label} SSH config block.",
            draft.alias
        ),
    })
}

pub fn delete_ssh_config_host(alias: String) -> Result<SshConfigWriteResult, String> {
    let alias = normalize_alias(&alias)?;
    let path = config_path()?;
    let (existing, existed) = read_optional_config(&path)?;
    let managed_exists = find_managed_host_block(&existing, &alias)?.is_some();
    let local_exists = find_local_host_block(&existing, &alias)?.is_some();
    let next = if managed_exists {
        delete_managed_host_block(&existing, &alias)?
    } else if local_exists {
        delete_local_host_block(&existing, &alias)?
    } else {
        existing.clone()
    };

    if next == existing {
        return Ok(SshConfigWriteResult {
            changed: false,
            action: "unchanged".into(),
            config_path: path_string(&path),
            backup_path: None,
            host: None,
            message: format!("No SSH config Host {alias} was found."),
        });
    }

    let backup_path = write_config_with_backup(&path, &next, existed)?;
    let (action, source_label) = if managed_exists {
        ("deleted", "CodexHub-managed")
    } else {
        ("local_deleted", "local")
    };
    Ok(SshConfigWriteResult {
        changed: true,
        action: action.into(),
        config_path: path_string(&path),
        backup_path: backup_path.as_ref().map(|item| path_string(item)),
        host: None,
        message: format!("Deleted Host {alias} from the {source_label} SSH config block."),
    })
}

pub fn normalize_timeout_ms(timeout_ms: Option<u64>) -> u64 {
    timeout_ms
        .unwrap_or(DEFAULT_TIMEOUT_MS)
        .clamp(MIN_TIMEOUT_MS, MAX_TIMEOUT_MS)
}

pub fn validate_ssh_alias(alias: &str) -> Result<String, String> {
    let alias = normalize_alias(alias)?;
    if alias.starts_with('-') {
        return Err(
            "Host Alias cannot start with '-' because it would be parsed as an ssh option.".into(),
        );
    }
    if alias
        .chars()
        .any(|ch| matches!(ch, '*' | '?' | '[' | ']' | '!' | '/' | '\\'))
    {
        return Err(
            "Host Alias must be a concrete SSH alias without wildcards or path separators.".into(),
        );
    }
    if !alias
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '@' | ':'))
    {
        return Err("Host Alias contains characters CodexHub will not pass to ssh.".into());
    }
    Ok(alias)
}

pub fn run_ssh_echo_ok(host_alias: &str, timeout_ms: u64) -> Result<SshCommandOutput, String> {
    run_ssh_command(
        host_alias,
        vec!["echo".into(), "ok".into()],
        timeout_ms,
        "echo ok",
        vec![("StrictHostKeyChecking".into(), "accept-new".into())],
    )
}

pub fn run_ssh_script(
    host_alias: &str,
    script: &str,
    timeout_ms: u64,
) -> Result<SshCommandOutput, String> {
    // Windows OpenSSH can lose shell quotes when an entire script is passed as
    // the remote command argument. Send scripts through stdin so POSIX shell
    // parsing happens only on the remote side.
    let timeout_ms = normalize_timeout_ms(Some(timeout_ms));
    let extra_options = Vec::new();
    let (host_alias, connect_timeout_secs, args) = build_ssh_args(
        host_alias,
        vec!["sh".into(), "-s".into()],
        timeout_ms,
        extra_options.clone(),
    )?;
    let command = format!(
        "ssh -o BatchMode=yes -o NumberOfPasswordPrompts=0 -o ConnectTimeout={}{} {} sh -s",
        connect_timeout_secs,
        display_extra_options(&extra_options),
        host_alias
    );
    let stdin_input = script_stdin(script);
    run_process_with_timeout_input_env(
        "ssh",
        &args,
        &command,
        timeout_ms,
        stdin_input.as_ref(),
        &[],
    )
}

pub fn run_ssh_script_streaming<F>(
    host_alias: &str,
    script: &str,
    timeout_ms: u64,
    on_event: F,
) -> Result<SshCommandOutput, String>
where
    F: FnMut(ProcessStreamEvent),
{
    let timeout_ms = normalize_timeout_ms(Some(timeout_ms));
    let extra_options = Vec::new();
    let (host_alias, connect_timeout_secs, args) = build_ssh_args(
        host_alias,
        vec!["sh".into(), "-s".into()],
        timeout_ms,
        extra_options.clone(),
    )?;
    let command = format!(
        "ssh -o BatchMode=yes -o NumberOfPasswordPrompts=0 -o ConnectTimeout={}{} {} sh -s",
        connect_timeout_secs,
        display_extra_options(&extra_options),
        host_alias
    );
    let stdin_input = script_stdin(script);
    run_process_with_timeout_streaming(
        "ssh",
        &args,
        &command,
        timeout_ms,
        stdin_input.as_ref(),
        &[],
        on_event,
    )
}

fn script_stdin(script: &str) -> Cow<'_, str> {
    if script.ends_with('\n') {
        Cow::Borrowed(script)
    } else {
        Cow::Owned(format!("{script}\n"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessStreamKind {
    Stdout,
    Stderr,
    Heartbeat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessStreamEvent {
    pub kind: ProcessStreamKind,
    pub line: String,
    pub elapsed_ms: u64,
}

pub fn run_local_process(
    program: &str,
    args: &[String],
    display_command: &str,
    timeout_ms: u64,
) -> Result<SshCommandOutput, String> {
    run_process_with_timeout(program, args, display_command, timeout_ms)
}

pub fn run_local_process_streaming<F>(
    program: &str,
    args: &[String],
    display_command: &str,
    timeout_ms: u64,
    on_event: F,
) -> Result<SshCommandOutput, String>
where
    F: FnMut(ProcessStreamEvent),
{
    run_process_with_timeout_streaming(
        program,
        args,
        display_command,
        timeout_ms,
        "",
        &[],
        on_event,
    )
}

pub fn upload_file(
    host_alias: &str,
    local_path: &Path,
    remote_path: &str,
    timeout_ms: u64,
) -> Result<SshCommandOutput, String> {
    let host_alias = validate_ssh_alias(host_alias)?;
    if !remote_path
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-'))
    {
        return Err("Remote upload path contains characters CodexHub will not pass to scp.".into());
    }

    let timeout_ms = normalize_timeout_ms(Some(timeout_ms));
    let connect_timeout_secs = ((timeout_ms + 999) / 1000).clamp(1, 120).to_string();
    let remote_target = format!("{host_alias}:{remote_path}");
    let args = vec![
        "-q".into(),
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        "NumberOfPasswordPrompts=0".into(),
        "-o".into(),
        format!("ConnectTimeout={connect_timeout_secs}"),
        local_path.to_string_lossy().to_string(),
        remote_target.clone(),
    ];
    let command = format!(
        "scp -q -o BatchMode=yes -o NumberOfPasswordPrompts=0 -o ConnectTimeout={} {} {}",
        connect_timeout_secs,
        path_string(local_path),
        remote_target
    );

    run_process_with_timeout("scp", &args, &command, timeout_ms)
}

pub fn download_file(
    host_alias: &str,
    remote_path: &str,
    local_path: &Path,
    timeout_ms: u64,
) -> Result<SshCommandOutput, String> {
    let host_alias = validate_ssh_alias(host_alias)?;
    if !remote_path
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-'))
    {
        return Err(
            "Remote download path contains characters CodexHub will not pass to scp.".into(),
        );
    }

    let timeout_ms = normalize_timeout_ms(Some(timeout_ms));
    let connect_timeout_secs = ((timeout_ms + 999) / 1000).clamp(1, 120).to_string();
    let remote_target = format!("{host_alias}:{remote_path}");
    let args = vec![
        "-q".into(),
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        "NumberOfPasswordPrompts=0".into(),
        "-o".into(),
        format!("ConnectTimeout={connect_timeout_secs}"),
        remote_target.clone(),
        local_path.to_string_lossy().to_string(),
    ];
    let command = format!(
        "scp -q -o BatchMode=yes -o NumberOfPasswordPrompts=0 -o ConnectTimeout={} {} {}",
        connect_timeout_secs,
        remote_target,
        path_string(local_path)
    );

    run_process_with_timeout("scp", &args, &command, timeout_ms)
}

pub fn upload_file_streaming<F>(
    host_alias: &str,
    local_path: &Path,
    remote_path: &str,
    timeout_ms: u64,
    on_event: F,
) -> Result<SshCommandOutput, String>
where
    F: FnMut(ProcessStreamEvent),
{
    let host_alias = validate_ssh_alias(host_alias)?;
    if !remote_path
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-'))
    {
        return Err("Remote upload path contains characters CodexHub will not pass to scp.".into());
    }

    let timeout_ms = normalize_timeout_ms(Some(timeout_ms));
    let connect_timeout_secs = ((timeout_ms + 999) / 1000).clamp(1, 120).to_string();
    let remote_target = format!("{host_alias}:{remote_path}");
    let args = vec![
        "-q".into(),
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        "NumberOfPasswordPrompts=0".into(),
        "-o".into(),
        format!("ConnectTimeout={connect_timeout_secs}"),
        local_path.to_string_lossy().to_string(),
        remote_target.clone(),
    ];
    let command = format!(
        "scp -q -o BatchMode=yes -o NumberOfPasswordPrompts=0 -o ConnectTimeout={} {} {}",
        connect_timeout_secs,
        path_string(local_path),
        remote_target
    );

    run_process_with_timeout_streaming("scp", &args, &command, timeout_ms, "", &[], on_event)
}

fn run_ssh_command(
    host_alias: &str,
    remote_args: Vec<String>,
    timeout_ms: u64,
    display_remote_command: &str,
    extra_options: Vec<(String, String)>,
) -> Result<SshCommandOutput, String> {
    let timeout_ms = normalize_timeout_ms(Some(timeout_ms));
    let (host_alias, connect_timeout_secs, args) =
        build_ssh_args(host_alias, remote_args, timeout_ms, extra_options.clone())?;

    let command = format!(
        "ssh -o BatchMode=yes -o NumberOfPasswordPrompts=0 -o ConnectTimeout={}{} {} {}",
        connect_timeout_secs,
        display_extra_options(&extra_options),
        host_alias,
        display_remote_command
    );
    run_process_with_timeout("ssh", &args, &command, timeout_ms)
}

fn build_ssh_args(
    host_alias: &str,
    remote_args: Vec<String>,
    timeout_ms: u64,
    extra_options: Vec<(String, String)>,
) -> Result<(String, String, Vec<String>), String> {
    let host_alias = validate_ssh_alias(host_alias)?;
    let connect_timeout_secs = ((timeout_ms + 999) / 1000).clamp(1, 120).to_string();
    let mut args = vec![
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        "NumberOfPasswordPrompts=0".to_string(),
        "-o".to_string(),
        format!("ConnectTimeout={connect_timeout_secs}"),
        host_alias.clone(),
    ];
    for (key, value) in extra_options {
        args.insert(args.len() - 1, "-o".to_string());
        args.insert(args.len() - 1, format!("{key}={value}"));
    }
    args.extend(remote_args);

    Ok((host_alias, connect_timeout_secs, args))
}

fn display_extra_options(extra_options: &[(String, String)]) -> String {
    extra_options
        .iter()
        .map(|(key, value)| format!(" -o {key}={value}"))
        .collect::<String>()
}

fn run_process_with_timeout(
    program: &str,
    args: &[String],
    display_command: &str,
    timeout_ms: u64,
) -> Result<SshCommandOutput, String> {
    run_process_with_timeout_input_env(program, args, display_command, timeout_ms, "", &[])
}

enum ProcessReaderMessage {
    Line(ProcessStreamKind, String),
    Done,
}

fn run_process_with_timeout_streaming<F>(
    program: &str,
    args: &[String],
    display_command: &str,
    timeout_ms: u64,
    stdin_input: &str,
    envs: &[(&str, String)],
    mut on_event: F,
) -> Result<SshCommandOutput, String>
where
    F: FnMut(ProcessStreamEvent),
{
    let start = Instant::now();
    let mut child = process_command(program)
        .args(args)
        .envs(envs.iter().map(|(key, value)| (*key, value)))
        .stdin(if stdin_input.is_empty() {
            Stdio::null()
        } else {
            Stdio::piped()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Failed to start {program}: {error}"))?;

    if !stdin_input.is_empty() {
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(stdin_input.as_bytes())
                .map_err(|error| format!("Failed to write stdin for {program}: {error}"))?;
        }
    }

    let (sender, receiver) = mpsc::channel::<ProcessReaderMessage>();
    if let Some(stdout) = child.stdout.take() {
        spawn_stream_reader(stdout, ProcessStreamKind::Stdout, sender.clone());
    } else {
        let _ = sender.send(ProcessReaderMessage::Done);
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_stream_reader(stderr, ProcessStreamKind::Stderr, sender.clone());
    } else {
        let _ = sender.send(ProcessReaderMessage::Done);
    }
    drop(sender);

    let timeout = Duration::from_millis(timeout_ms);
    let heartbeat_interval = Duration::from_secs(4);
    let mut last_heartbeat = Instant::now();
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut exit_code = None;
    let mut timed_out = false;
    let mut readers_done = 0usize;

    loop {
        while let Ok(message) = receiver.try_recv() {
            match message {
                ProcessReaderMessage::Line(kind, line) => {
                    let redacted = redact_sensitive(&line);
                    match kind {
                        ProcessStreamKind::Stdout => {
                            stdout.push_str(&redacted);
                            stdout.push('\n');
                        }
                        ProcessStreamKind::Stderr => {
                            stderr.push_str(&redacted);
                            stderr.push('\n');
                        }
                        ProcessStreamKind::Heartbeat => {}
                    }
                    on_event(ProcessStreamEvent {
                        kind,
                        line: redacted,
                        elapsed_ms: duration_ms(start),
                    });
                    last_heartbeat = Instant::now();
                }
                ProcessReaderMessage::Done => readers_done += 1,
            }
        }

        if exit_code.is_none() {
            if let Some(status) = child
                .try_wait()
                .map_err(|error| format!("Failed to poll {program}: {error}"))?
            {
                exit_code = status.code();
            }
        }

        if exit_code.is_some() && readers_done >= 2 {
            break;
        }

        if exit_code.is_none() && start.elapsed() >= timeout {
            timed_out = true;
            let _ = child.kill();
            let status = child.wait().map_err(|error| {
                format!("Failed to collect timed-out {program} output: {error}")
            })?;
            exit_code = status.code();
        }

        if exit_code.is_none() && last_heartbeat.elapsed() >= heartbeat_interval {
            on_event(ProcessStreamEvent {
                kind: ProcessStreamKind::Heartbeat,
                line: format!("Still running after {} ms.", duration_ms(start)),
                elapsed_ms: duration_ms(start),
            });
            last_heartbeat = Instant::now();
        }

        if readers_done < 2 {
            match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(message) => match message {
                    ProcessReaderMessage::Line(kind, line) => {
                        let redacted = redact_sensitive(&line);
                        match kind {
                            ProcessStreamKind::Stdout => {
                                stdout.push_str(&redacted);
                                stdout.push('\n');
                            }
                            ProcessStreamKind::Stderr => {
                                stderr.push_str(&redacted);
                                stderr.push('\n');
                            }
                            ProcessStreamKind::Heartbeat => {}
                        }
                        on_event(ProcessStreamEvent {
                            kind,
                            line: redacted,
                            elapsed_ms: duration_ms(start),
                        });
                        last_heartbeat = Instant::now();
                    }
                    ProcessReaderMessage::Done => readers_done += 1,
                },
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => readers_done = 2,
            }
        } else {
            thread::sleep(Duration::from_millis(25));
        }
    }

    Ok(SshCommandOutput {
        command: display_command.to_string(),
        stdout: stdout.trim_end_matches('\n').to_string(),
        stderr: stderr.trim_end_matches('\n').to_string(),
        exit_code,
        duration_ms: duration_ms(start),
        timed_out,
    })
}

fn spawn_stream_reader<R>(
    reader: R,
    kind: ProcessStreamKind,
    sender: mpsc::Sender<ProcessReaderMessage>,
) where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let trimmed = line.trim_end_matches(['\r', '\n']).to_string();
                    if sender
                        .send(ProcessReaderMessage::Line(kind, trimmed))
                        .is_err()
                    {
                        return;
                    }
                }
                Err(error) => {
                    let _ = sender.send(ProcessReaderMessage::Line(
                        ProcessStreamKind::Stderr,
                        format!("Failed to read process output: {error}"),
                    ));
                    break;
                }
            }
        }
        let _ = sender.send(ProcessReaderMessage::Done);
    });
}

fn run_process_with_timeout_input_env(
    program: &str,
    args: &[String],
    display_command: &str,
    timeout_ms: u64,
    stdin_input: &str,
    envs: &[(&str, String)],
) -> Result<SshCommandOutput, String> {
    let start = Instant::now();
    let mut child = process_command(program)
        .args(args)
        .envs(envs.iter().map(|(key, value)| (*key, value)))
        .stdin(if stdin_input.is_empty() {
            Stdio::null()
        } else {
            Stdio::piped()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Failed to start {program}: {error}"))?;

    if !stdin_input.is_empty() {
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(stdin_input.as_bytes())
                .map_err(|error| format!("Failed to write stdin for {program}: {error}"))?;
        }
    }

    let timeout = Duration::from_millis(timeout_ms);
    loop {
        if child
            .try_wait()
            .map_err(|error| format!("Failed to poll {program}: {error}"))?
            .is_some()
        {
            let output = child
                .wait_with_output()
                .map_err(|error| format!("Failed to collect {program} output: {error}"))?;
            return Ok(SshCommandOutput {
                command: display_command.to_string(),
                stdout: redact_sensitive(&String::from_utf8_lossy(&output.stdout)),
                stderr: redact_sensitive(&String::from_utf8_lossy(&output.stderr)),
                exit_code: output.status.code(),
                duration_ms: duration_ms(start),
                timed_out: false,
            });
        }

        if start.elapsed() >= timeout {
            let _ = child.kill();
            let output = child.wait_with_output().map_err(|error| {
                format!("Failed to collect timed-out {program} output: {error}")
            })?;
            return Ok(SshCommandOutput {
                command: display_command.to_string(),
                stdout: redact_sensitive(&String::from_utf8_lossy(&output.stdout)),
                stderr: redact_sensitive(&String::from_utf8_lossy(&output.stderr)),
                exit_code: output.status.code(),
                duration_ms: duration_ms(start),
                timed_out: true,
            });
        }

        thread::sleep(Duration::from_millis(25));
    }
}

pub fn redact_sensitive(input: &str) -> String {
    let mut lines = Vec::new();
    let mut in_private_key = false;

    for line in input.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("begin") && lower.contains("private key") {
            lines.push("[redacted private key material]".into());
            in_private_key = true;
            continue;
        }
        if in_private_key {
            if lower.contains("end") && lower.contains("private key") {
                in_private_key = false;
            }
            continue;
        }
        lines.push(redact_sensitive_line(line));
    }

    lines.join("\n")
}

fn redact_password(input: &str, password: &str) -> String {
    if password.is_empty() {
        input.to_string()
    } else {
        input.replace(password, "[redacted]")
    }
}

fn redact_sensitive_line(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    if lower.contains("private key") {
        return "[redacted private key material]".into();
    }

    let mut next = map_whitespace_separated_tokens(line, redact_sensitive_token);

    for key in [
        "password",
        "passphrase",
        "token",
        "secret",
        "api_key",
        "apikey",
    ] {
        next = redact_key_value(&next, key);
    }

    next
}

fn map_whitespace_separated_tokens<F>(line: &str, mut map_token: F) -> String
where
    F: FnMut(&str) -> String,
{
    let mut output = String::with_capacity(line.len());
    let mut token = String::new();
    for ch in line.chars() {
        if ch.is_whitespace() {
            if !token.is_empty() {
                output.push_str(&map_token(&token));
                token.clear();
            }
            output.push(ch);
        } else {
            token.push(ch);
        }
    }
    if !token.is_empty() {
        output.push_str(&map_token(&token));
    }
    output
}

fn redact_sensitive_token(token: &str) -> String {
    let trimmed = token.trim_matches(|ch: char| matches!(ch, '"' | '\'' | ',' | ';'));
    if trimmed.starts_with("sk-")
        || trimmed.starts_with("ghp_")
        || trimmed.starts_with("github_pat_")
        || trimmed.starts_with("xoxb-")
    {
        token.replace(trimmed, "[redacted]")
    } else {
        token.to_string()
    }
}

fn redact_key_value(line: &str, key: &str) -> String {
    map_whitespace_separated_tokens(line, |part| {
        let lower = part.to_ascii_lowercase();
        let prefix_eq = format!("{key}=");
        let prefix_colon = format!("{key}:");
        if lower.starts_with(&prefix_eq) {
            format!("{}=[redacted]", &part[..key.len()])
        } else if lower.starts_with(&prefix_colon) {
            format!("{}:[redacted]", &part[..key.len()])
        } else {
            part.to_string()
        }
    })
}

fn ssh_dir() -> Result<PathBuf, String> {
    platform::get_ssh_dir()
}

fn config_path() -> Result<PathBuf, String> {
    platform::get_ssh_config_path()
}

fn key_info(key_type: &str, private_path: &Path, public_path: &Path) -> SshKeyInfo {
    let public_exists = public_path.exists();
    SshKeyInfo {
        key_type: key_type.into(),
        private_path: path_string(private_path),
        public_path: path_string(public_path),
        private_exists: private_path.exists(),
        public_exists,
        public_key: if public_exists {
            fs::read_to_string(public_path)
                .ok()
                .map(|content| content.trim().to_string())
                .filter(|content| !content.is_empty())
        } else {
            None
        },
    }
}

fn command_available(command: &str) -> bool {
    platform::command_available(command)
}

fn ssh_keygen_missing_message() -> String {
    if platform::is_windows() {
        "ssh-keygen was not found on PATH. Install Windows OpenSSH Client first.".into()
    } else {
        "ssh-keygen was not found on PATH. Install OpenSSH client tools first.".into()
    }
}

fn key_comment() -> String {
    if platform::is_windows() {
        format!(
            "codexhub@{}",
            env::var("COMPUTERNAME").unwrap_or_else(|_| "windows".into())
        )
    } else {
        "codexhub".into()
    }
}

#[cfg(unix)]
fn set_ssh_dir_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::metadata(path)
        .map_err(|error| format!("Failed to inspect SSH directory permissions: {error}"))?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)
        .map_err(|error| format!("Failed to set SSH directory permissions: {error}"))
}

#[cfg(not(unix))]
fn set_ssh_dir_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn expand_local_path(input: &str) -> Result<PathBuf, String> {
    platform::expand_home_path(input)
        .map_err(|error| format!("{error} Cannot expand IdentityFile."))
}

fn public_key_path_for(private_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.pub", private_path.to_string_lossy()))
}

fn read_public_key(public_path: &Path) -> Result<String, String> {
    fs::read_to_string(public_path)
        .map(|content| content.trim().to_string())
        .map_err(|error| {
            format!(
                "Failed to read public key {}: {error}",
                path_string(public_path)
            )
        })
        .and_then(|content| {
            if content.is_empty() {
                Err(format!("Public key is empty: {}", path_string(public_path)))
            } else {
                Ok(content)
            }
        })
}

fn generate_keypair_at(private_path: &Path) -> Result<(), String> {
    if !command_available("ssh-keygen") {
        return Err(ssh_keygen_missing_message());
    }
    if let Some(parent) = private_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create SSH key directory: {error}"))?;
        set_ssh_dir_permissions(parent)?;
    }
    let comment = key_comment();
    let output = process_command("ssh-keygen")
        .arg("-t")
        .arg("ed25519")
        .arg("-f")
        .arg(private_path)
        .arg("-N")
        .arg("")
        .arg("-C")
        .arg(comment)
        .output()
        .map_err(|error| format!("Failed to run ssh-keygen: {error}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!("ssh-keygen failed: {}", process_detail(&output)))
    }
}

fn derive_public_key(private_path: &Path, public_path: &Path) -> Result<(), String> {
    if !command_available("ssh-keygen") {
        return Err(ssh_keygen_missing_message());
    }
    let output = process_command("ssh-keygen")
        .arg("-y")
        .arg("-f")
        .arg(private_path)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("Failed to derive public key: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Could not derive public key from {}: {}",
            path_string(private_path),
            process_detail(&output)
        ));
    }
    let public_key = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if public_key.is_empty() {
        return Err(format!(
            "ssh-keygen returned an empty public key for {}",
            path_string(private_path)
        ));
    }
    fs::write(public_path, format!("{public_key}\n")).map_err(|error| {
        format!(
            "Failed to write derived public key {}: {error}",
            path_string(public_path)
        )
    })
}

fn process_detail(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return redact_sensitive(&stderr);
    }
    redact_sensitive(String::from_utf8_lossy(&output.stdout).trim())
}

fn read_optional_config(path: &Path) -> Result<(String, bool), String> {
    if path.exists() {
        fs::read_to_string(path)
            .map(|content| (content, true))
            .map_err(|error| format!("Failed to read SSH config {}: {error}", path_string(path)))
    } else {
        Ok((String::new(), false))
    }
}

fn write_config_with_backup(
    path: &Path,
    content: &str,
    existed: bool,
) -> Result<Option<PathBuf>, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create .ssh directory: {error}"))?;
    }

    let backup_path = if existed {
        let backup = path.with_file_name(format!("config.bak.{}", timestamp_millis()));
        fs::copy(path, &backup).map_err(|error| {
            format!(
                "Failed to create SSH config backup {}: {error}",
                path_string(&backup)
            )
        })?;
        Some(backup)
    } else {
        None
    };

    fs::write(path, content)
        .map_err(|error| format!("Failed to write SSH config {}: {error}", path_string(path)))?;
    Ok(backup_path)
}

fn normalize_draft(draft: SshHostDraft) -> Result<SshHostDraft, String> {
    Ok(SshHostDraft {
        alias: validate_ssh_alias(&draft.alias)?,
        host_name: normalize_config_value("HostName", &draft.host_name)?,
        port: normalize_port(draft.port)?,
        user: normalize_config_value("User", &draft.user)?,
        identity_file: normalize_config_value("IdentityFile", &draft.identity_file)?,
    })
}

fn normalize_alias(alias: &str) -> Result<String, String> {
    let alias = alias.trim();
    if alias.is_empty() {
        return Err("Host Alias is required.".into());
    }
    if alias.chars().any(char::is_whitespace) {
        return Err("Host Alias must be a single SSH config token without whitespace.".into());
    }
    Ok(alias.into())
}

fn normalize_required(label: &str, value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{label} is required."))
    } else {
        Ok(value.into())
    }
}

fn normalize_config_value(label: &str, value: &str) -> Result<String, String> {
    let value = normalize_required(label, value)?;
    if value.contains('\r') || value.contains('\n') {
        Err(format!("{label} cannot contain line breaks."))
    } else {
        Ok(value)
    }
}

fn normalize_port(port: u16) -> Result<u16, String> {
    if port == 0 {
        Err("Port must be between 1 and 65535.".into())
    } else {
        Ok(port)
    }
}

fn host_from_draft(draft: &SshHostDraft) -> SshConfigHost {
    SshConfigHost {
        alias: draft.alias.clone(),
        host_name: draft.host_name.clone(),
        port: draft.port,
        user: draft.user.clone(),
        identity_file: draft.identity_file.clone(),
        managed: true,
        source: "managed".into(),
    }
}

fn local_host_from_draft(draft: &SshHostDraft) -> SshConfigHost {
    SshConfigHost {
        alias: draft.alias.clone(),
        host_name: draft.host_name.clone(),
        port: draft.port,
        user: draft.user.clone(),
        identity_file: draft.identity_file.clone(),
        managed: false,
        source: "local".into(),
    }
}

#[cfg(test)]
fn parse_managed_hosts(content: &str) -> Result<Vec<SshConfigHost>, String> {
    Ok(scan_managed_blocks(content)?
        .into_iter()
        .map(|block| block.host)
        .collect())
}

fn parse_all_ssh_config_hosts(content: &str) -> Result<Vec<SshConfigHost>, String> {
    let managed_blocks = scan_managed_blocks(content)?;
    let managed_ranges: Vec<Range<usize>> = managed_blocks
        .iter()
        .map(|block| block.range.clone())
        .collect();
    let mut hosts = Vec::new();
    let mut seen = HashSet::new();

    for block in managed_blocks {
        if validate_ssh_alias(&block.host.alias).is_ok() {
            seen.insert(block.host.alias.to_ascii_lowercase());
            hosts.push(block.host);
        }
    }

    for block in parse_local_host_blocks(content, &managed_ranges)? {
        for alias in &block.aliases {
            if validate_ssh_alias(alias).is_err() {
                continue;
            }
            if !is_local_ssh_config_import_candidate(alias, &block) {
                continue;
            }
            let key = alias.to_ascii_lowercase();
            if seen.insert(key) {
                hosts.push(SshConfigHost {
                    alias: alias.clone(),
                    host_name: block.host_name.clone(),
                    port: block.port,
                    user: block.user.clone(),
                    identity_file: block.identity_file.clone(),
                    managed: false,
                    source: "local".into(),
                });
            }
        }
    }

    Ok(hosts)
}

fn parse_local_host_blocks(
    content: &str,
    managed_ranges: &[Range<usize>],
) -> Result<Vec<LocalHostBlock>, String> {
    let lines = split_lines_inclusive(content);
    let mut blocks = Vec::new();
    let mut current: Option<LocalHostBlockBuilder> = None;

    for (index, raw_line) in lines.iter().enumerate() {
        if managed_ranges.iter().any(|range| range.contains(&index)) {
            if let Some(builder) = current.take() {
                push_local_host_block(&mut blocks, builder, index);
            }
            continue;
        }

        let line = trim_line(raw_line).trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((keyword, value)) = split_directive(line) else {
            continue;
        };

        if keyword.eq_ignore_ascii_case("Host") {
            if let Some(builder) = current.take() {
                push_local_host_block(&mut blocks, builder, index);
            }
            current = Some(LocalHostBlockBuilder {
                aliases: value.split_whitespace().map(str::to_string).collect(),
                start: index,
                host_name: String::new(),
                port: 22,
                user: String::new(),
                identity_file: String::new(),
            });
            continue;
        }

        let Some(builder) = current.as_mut() else {
            continue;
        };
        match keyword.to_ascii_lowercase().as_str() {
            "hostname" => builder.host_name = unquote(value).to_string(),
            "port" => {
                if let Ok(parsed_port) = value.parse::<u16>() {
                    builder.port = parsed_port;
                }
            }
            "user" => builder.user = unquote(value).to_string(),
            "identityfile" => builder.identity_file = unquote(value).to_string(),
            _ => {}
        }
    }

    if let Some(builder) = current.take() {
        push_local_host_block(&mut blocks, builder, lines.len());
    }

    Ok(blocks)
}

fn push_local_host_block(
    blocks: &mut Vec<LocalHostBlock>,
    builder: LocalHostBlockBuilder,
    end: usize,
) {
    if builder.aliases.is_empty() {
        return;
    }
    blocks.push(LocalHostBlock {
        aliases: builder.aliases,
        range: builder.start..end,
        host_name: builder.host_name,
        port: builder.port,
        user: builder.user,
        identity_file: builder.identity_file,
    });
}

fn is_local_ssh_config_import_candidate(alias: &str, block: &LocalHostBlock) -> bool {
    let alias = alias.trim().to_ascii_lowercase();
    let host_name = block.host_name.trim().to_ascii_lowercase();
    // Public Git service entries are SSH client shortcuts, not CodexHub workstation candidates.
    !is_public_git_service_host(&alias) && !is_public_git_service_host(&host_name)
}

fn is_public_git_service_host(value: &str) -> bool {
    matches!(
        value,
        "github.com"
            | "ssh.github.com"
            | "gitlab.com"
            | "bitbucket.org"
            | "git.sr.ht"
            | "ssh.dev.azure.com"
            | "vs-ssh.visualstudio.com"
    )
}

fn upsert_managed_host_block(content: &str, draft: &SshHostDraft) -> Result<String, String> {
    let newline = detect_newline(content);
    let rendered = render_managed_block_with_newline(draft, newline);
    if let Some(block) = find_managed_host_block(content, &draft.alias)? {
        let lines = split_lines_inclusive(content);
        let mut next = String::new();
        next.push_str(&lines[..block.range.start].concat());
        next.push_str(&rendered);
        next.push_str(&lines[block.range.end..].concat());
        Ok(next)
    } else {
        Ok(append_managed_block(content, &rendered, newline))
    }
}

fn upsert_local_host_block(content: &str, draft: &SshHostDraft) -> Result<String, String> {
    let block = find_local_host_block(content, &draft.alias)?
        .ok_or_else(|| format!("Host {} was not found in local SSH config.", draft.alias))?;
    let lines = split_lines_inclusive(content);
    let block_lines = &lines[block.range.clone()];
    let mut next = String::new();
    next.push_str(&lines[..block.range.start].concat());
    if block.aliases.len() == 1 {
        next.push_str(&rewrite_local_host_block(
            block_lines,
            &[draft.alias.clone()],
            Some(draft),
        ));
    } else {
        let remaining_aliases = block
            .aliases
            .iter()
            .filter(|alias| !alias.eq_ignore_ascii_case(&draft.alias))
            .cloned()
            .collect::<Vec<_>>();
        next.push_str(&rewrite_local_host_block(
            block_lines,
            &remaining_aliases,
            None,
        ));
        if !next.ends_with('\n') {
            next.push('\n');
        }
        next.push_str(&rewrite_local_host_block(
            block_lines,
            &[draft.alias.clone()],
            Some(draft),
        ));
    }
    next.push_str(&lines[block.range.end..].concat());
    Ok(next)
}

fn delete_managed_host_block(content: &str, alias: &str) -> Result<String, String> {
    if let Some(block) = find_managed_host_block(content, alias)? {
        let lines = split_lines_inclusive(content);
        let mut next = String::new();
        next.push_str(&lines[..block.range.start].concat());
        next.push_str(&lines[block.range.end..].concat());
        Ok(next)
    } else {
        Ok(content.into())
    }
}

fn delete_local_host_block(content: &str, alias: &str) -> Result<String, String> {
    let Some(block) = find_local_host_block(content, alias)? else {
        return Ok(content.into());
    };
    let lines = split_lines_inclusive(content);
    let mut next = String::new();
    next.push_str(&lines[..block.range.start].concat());
    if block.aliases.len() > 1 {
        let remaining_aliases = block
            .aliases
            .iter()
            .filter(|item| !item.eq_ignore_ascii_case(alias))
            .cloned()
            .collect::<Vec<_>>();
        next.push_str(&rewrite_local_host_block(
            &lines[block.range.clone()],
            &remaining_aliases,
            None,
        ));
    }
    next.push_str(&lines[block.range.end..].concat());
    Ok(next)
}

fn find_managed_host_block(content: &str, alias: &str) -> Result<Option<ManagedBlock>, String> {
    Ok(scan_managed_blocks(content)?
        .into_iter()
        .find(|block| block.alias.eq_ignore_ascii_case(alias)))
}

fn find_local_host_block(content: &str, alias: &str) -> Result<Option<LocalHostBlock>, String> {
    let managed_ranges: Vec<Range<usize>> = scan_managed_blocks(content)?
        .into_iter()
        .map(|block| block.range)
        .collect();
    Ok(parse_local_host_blocks(content, &managed_ranges)?
        .into_iter()
        .find(|block| {
            block
                .aliases
                .iter()
                .any(|item| item.eq_ignore_ascii_case(alias))
        }))
}

fn scan_managed_blocks(content: &str) -> Result<Vec<ManagedBlock>, String> {
    let lines = split_lines_inclusive(content);
    let mut blocks = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let trimmed = trim_line(&lines[index]).trim();
        if let Some(alias) = trimmed.strip_prefix(MANAGED_START_PREFIX) {
            let alias = alias.trim().to_string();
            let end = (index + 1..lines.len())
                .find(|line_index| {
                    trim_line(&lines[*line_index])
                        .trim()
                        .starts_with(MANAGED_END_PREFIX)
                })
                .ok_or_else(|| {
                    format!(
                        "Malformed CodexHub managed Host block for {alias}: missing end marker."
                    )
                })?;
            let host = parse_host_from_lines(&lines[index..=end], Some(alias.clone()))?;
            blocks.push(ManagedBlock {
                alias,
                range: index..end + 1,
                host,
            });
            index = end + 1;
            continue;
        }
        index += 1;
    }

    Ok(blocks)
}

fn parse_host_from_lines(
    lines: &[String],
    managed_alias: Option<String>,
) -> Result<SshConfigHost, String> {
    let mut alias = managed_alias.unwrap_or_default();
    let mut host_name = String::new();
    let mut port = 22;
    let mut user = String::new();
    let mut identity_file = String::new();

    for line in lines {
        let line = trim_line(line).trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((keyword, value)) = split_directive(line) else {
            continue;
        };
        match keyword.to_ascii_lowercase().as_str() {
            "host" => {
                if alias.is_empty() {
                    alias = value
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_string();
                }
            }
            "hostname" => host_name = unquote(value).to_string(),
            "port" => {
                port = value
                    .parse::<u16>()
                    .map_err(|_| format!("Invalid Port value in Host {alias}: {value}"))?
            }
            "user" => user = unquote(value).to_string(),
            "identityfile" => identity_file = unquote(value).to_string(),
            _ => {}
        }
    }

    Ok(SshConfigHost {
        alias,
        host_name,
        port,
        user,
        identity_file,
        managed: true,
        source: "managed".into(),
    })
}

#[cfg(test)]
fn unmanaged_aliases(content: &str) -> Result<Vec<String>, String> {
    let lines = split_lines_inclusive(content);
    let managed_ranges: Vec<Range<usize>> = scan_managed_blocks(content)?
        .into_iter()
        .map(|block| block.range)
        .collect();
    let mut aliases = Vec::new();

    'lines: for (index, line) in lines.iter().enumerate() {
        for range in &managed_ranges {
            if range.contains(&index) {
                continue 'lines;
            }
        }

        let line = trim_line(line).trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((keyword, value)) = split_directive(line) else {
            continue;
        };
        if keyword.eq_ignore_ascii_case("Host") {
            aliases.extend(value.split_whitespace().map(str::to_string));
        }
    }

    Ok(aliases)
}

fn rewrite_local_host_block(
    block_lines: &[String],
    aliases: &[String],
    draft: Option<&SshHostDraft>,
) -> String {
    let block_content = block_lines.concat();
    let newline = detect_newline(&block_content);
    let mut next_lines = Vec::new();
    let mut host_line_index = None;

    for line in block_lines {
        let trimmed = trim_line(line).trim();
        let directive = if trimmed.is_empty() || trimmed.starts_with('#') {
            None
        } else {
            split_directive(trimmed)
        };

        match directive {
            Some((keyword, _)) if keyword.eq_ignore_ascii_case("Host") => {
                host_line_index = Some(next_lines.len());
                next_lines.push(format!("Host {}{newline}", aliases.join(" ")));
            }
            Some((keyword, _))
                if draft.is_some()
                    && matches!(
                        keyword.to_ascii_lowercase().as_str(),
                        "hostname" | "port" | "user" | "identityfile"
                    ) => {}
            _ => next_lines.push(line.clone()),
        }
    }

    if let Some(draft) = draft {
        let insert_index = host_line_index.map_or(0, |index| index + 1);
        let mut directives = vec![
            format!("    HostName {}{newline}", draft.host_name),
            format!("    Port {}{newline}", draft.port),
            format!("    User {}{newline}", draft.user),
        ];
        if !draft.identity_file.trim().is_empty() {
            directives.push(format!("    IdentityFile {}{newline}", draft.identity_file));
        }
        for (offset, line) in directives.into_iter().enumerate() {
            next_lines.insert(insert_index + offset, line);
        }
    }

    next_lines.concat()
}

fn render_managed_block_with_newline(draft: &SshHostDraft, newline: &str) -> String {
    format!(
        "{MANAGED_START_PREFIX} {alias}{newline}Host {alias}{newline}    HostName {host_name}{newline}    Port {port}{newline}    User {user}{newline}    IdentityFile {identity_file}{newline}{MANAGED_END_PREFIX} {alias}{newline}",
        alias = draft.alias,
        host_name = draft.host_name,
        port = draft.port,
        user = draft.user,
        identity_file = draft.identity_file,
        newline = newline
    )
}

fn append_managed_block(content: &str, block: &str, newline: &str) -> String {
    if content.is_empty() {
        return block.into();
    }

    let mut next = content.to_string();
    if !next.ends_with('\n') {
        next.push_str(newline);
    }
    next.push_str(newline);
    next.push_str(block);
    next
}

fn split_lines_inclusive(content: &str) -> Vec<String> {
    content.split_inclusive('\n').map(str::to_string).collect()
}

fn trim_line(line: &str) -> &str {
    line.trim_end_matches(['\r', '\n'])
}

fn detect_newline(content: &str) -> &str {
    if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn split_directive(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.splitn(2, char::is_whitespace);
    let keyword = parts.next()?.trim();
    let value = parts.next().unwrap_or_default().trim();
    if keyword.is_empty() {
        None
    } else {
        Some((keyword, value))
    }
}

fn unquote(value: &str) -> &str {
    let value = value.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn duration_ms(start: Instant) -> u64 {
    start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(alias: &str) -> SshHostDraft {
        SshHostDraft {
            alias: alias.into(),
            host_name: "example.com".into(),
            port: 2222,
            user: "codex".into(),
            identity_file: r"C:\Users\Example User\.ssh\id_ed25519".into(),
        }
    }

    #[test]
    fn parser_returns_managed_hosts_and_preserves_unmanaged_blocks() {
        let content = "Host github.com\n    HostName github.com\n\n# >>> CodexHub managed host: lab\nHost lab\n    HostName 10.0.0.5\n    Port 22\n    User codex\n    IdentityFile C:\\Users\\Example User\\.ssh\\id_ed25519\n# <<< CodexHub managed host: lab\n";

        let hosts = parse_managed_hosts(content).expect("parse hosts");

        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].alias, "lab");
        assert_eq!(hosts[0].host_name, "10.0.0.5");
        assert_eq!(
            hosts[0].identity_file,
            r"C:\Users\Example User\.ssh\id_ed25519"
        );
    }

    #[test]
    fn parser_and_writer_accept_macos_identity_paths() {
        let mut mac_draft = draft("mac-lab");
        mac_draft.identity_file = "~/.ssh/id_ed25519".into();
        let next = upsert_managed_host_block("Host github.com\n    User git\n", &mac_draft)
            .expect("upsert mac host");
        let hosts = parse_managed_hosts(&next).expect("parse mac host");

        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].alias, "mac-lab");
        assert_eq!(hosts[0].identity_file, "~/.ssh/id_ed25519");
        assert!(next.contains("IdentityFile ~/.ssh/id_ed25519"));
        assert!(next.contains("Host github.com\n    User git\n"));
    }

    #[test]
    fn parser_returns_managed_and_importable_local_hosts() {
        let content = "Host github.com *.example.com *\n    HostName github.com\n\n# >>> CodexHub managed host: lab\nHost lab\n    HostName 10.0.0.5\n    Port 22\n    User codex\n    IdentityFile C:\\Users\\Example User\\.ssh\\id_ed25519\n# <<< CodexHub managed host: lab\nHost runner\n    HostName 10.0.0.6\n    User codex\n";

        let hosts = parse_all_ssh_config_hosts(content).expect("parse all hosts");

        assert_eq!(
            hosts
                .iter()
                .map(|host| host.alias.as_str())
                .collect::<Vec<_>>(),
            vec!["lab", "runner"]
        );
        assert_eq!(hosts[0].source, "managed");
        assert_eq!(hosts[1].source, "local");
        assert!(!hosts
            .iter()
            .any(|host| host.alias == "*" || host.alias == "*.example.com"));
    }

    #[test]
    fn parser_filters_public_git_service_local_hosts() {
        let content = "Host github.com\n    HostName github.com\n    User git\n\nHost gitlab\n    HostName gitlab.com\n    User git\n\nHost work-box\n    HostName 192.168.1.12\n    User codex\n";

        let hosts = parse_all_ssh_config_hosts(content).expect("parse all hosts");

        assert_eq!(
            hosts
                .iter()
                .map(|host| host.alias.as_str())
                .collect::<Vec<_>>(),
            vec!["work-box"]
        );
        assert_eq!(hosts[0].host_name, "192.168.1.12");
    }

    #[test]
    fn add_managed_block_to_empty_config() {
        let next = upsert_managed_host_block("", &draft("lab")).expect("upsert");

        assert!(next.contains("Host lab"));
        assert!(next.contains("HostName example.com"));
        assert_eq!(parse_managed_hosts(&next).expect("parse").len(), 1);
    }

    #[test]
    fn repeat_add_is_idempotent() {
        let first = upsert_managed_host_block("", &draft("lab")).expect("first");
        let second = upsert_managed_host_block(&first, &draft("lab")).expect("second");

        assert_eq!(first, second);
        assert_eq!(second.matches("Host lab").count(), 1);
    }

    #[test]
    fn upsert_managed_block_preserves_crlf_and_comments() {
        let content = "# user comment\r\nHost github.com\r\n    User git\r\n";
        let next = upsert_managed_host_block(content, &draft("lab")).expect("upsert");

        assert!(next.starts_with("# user comment\r\nHost github.com\r\n"));
        assert!(next.contains("# >>> CodexHub managed host: lab\r\nHost lab\r\n"));
        assert!(!next.contains("# >>> CodexHub managed host: lab\nHost lab\n"));
    }

    #[test]
    fn update_managed_block_in_place() {
        let first = upsert_managed_host_block("Host github.com\n    User git\n", &draft("lab"))
            .expect("first");
        let mut changed = draft("lab");
        changed.host_name = "10.1.2.3".into();
        changed.port = 2200;
        let second = upsert_managed_host_block(&first, &changed).expect("update");

        assert!(second.contains("Host github.com\n    User git\n"));
        assert!(second.contains("HostName 10.1.2.3"));
        assert!(second.contains("Port 2200"));
        assert_eq!(parse_managed_hosts(&second).expect("parse").len(), 1);
    }

    #[test]
    fn update_local_single_alias_block_in_place() {
        let content = "Host lab\n    HostName old.example\n    ProxyJump bastion\n";
        let mut changed = draft("lab");
        changed.host_name = "10.1.2.3".into();
        changed.port = 2200;
        changed.user = "alice".into();
        let next = upsert_local_host_block(content, &changed).expect("update local");

        assert!(next.contains("Host lab\n"));
        assert!(next.contains("HostName 10.1.2.3"));
        assert!(next.contains("Port 2200"));
        assert!(next.contains("User alice"));
        assert!(next.contains("ProxyJump bastion"));
        assert!(!next.contains("old.example"));
    }

    #[test]
    fn update_local_multi_alias_block_splits_target_alias() {
        let content = "Host lab runner\n    HostName shared.example\n    User shared\n    ProxyJump bastion\n";
        let mut changed = draft("runner");
        changed.host_name = "10.9.8.7".into();
        changed.user = "codex".into();
        let next = upsert_local_host_block(content, &changed).expect("split local");

        assert!(next.contains("Host lab\n"));
        assert!(next.contains("Host runner\n"));
        assert!(next.contains("HostName shared.example"));
        assert!(next.contains("HostName 10.9.8.7"));
        assert!(next.contains("User codex"));
        assert_eq!(next.matches("ProxyJump bastion").count(), 2);
    }

    #[test]
    fn update_local_block_preserves_crlf_comments_and_spacing() {
        let content =
            "Host lab\r\n    # keep this comment\r\n    HostName old.example\r\n    User old\r\n";
        let mut changed = draft("lab");
        changed.host_name = "new.example".into();

        let next = upsert_local_host_block(content, &changed).expect("update local");

        assert!(next.contains("    # keep this comment\r\n"));
        assert!(next.contains("    HostName new.example\r\n"));
        assert!(next.contains("    IdentityFile C:\\Users\\Example User\\.ssh\\id_ed25519\r\n"));
        assert!(!next.contains("old.example"));
    }

    #[test]
    fn delete_only_managed_block() {
        let content = "Host github.com\n    User git\n\n";
        let with_managed = upsert_managed_host_block(content, &draft("lab")).expect("add");
        let deleted = delete_managed_host_block(&with_managed, "lab").expect("delete");

        assert!(deleted.contains("Host github.com"));
        assert!(!deleted.contains("CodexHub managed host: lab"));
    }

    #[test]
    fn delete_local_multi_alias_removes_only_target_alias() {
        let content = "Host lab runner other\n    HostName shared.example\n    User codex\n";
        let next = delete_local_host_block(content, "runner").expect("delete local alias");

        assert!(next.contains("Host lab other\n"));
        assert!(next.contains("HostName shared.example"));
        assert!(!next.contains("runner"));
    }

    #[test]
    fn delete_local_single_alias_removes_entire_block() {
        let content = "Host github.com\n    User git\n\nHost runner\n    HostName 10.0.0.6\n    User codex\n\nHost tail\n    HostName tail.example\n";
        let next = delete_local_host_block(content, "runner").expect("delete local block");

        assert!(next.contains("Host github.com"));
        assert!(next.contains("Host tail"));
        assert!(!next.contains("Host runner"));
        assert!(!next.contains("10.0.0.6"));
    }

    #[test]
    fn unmanaged_alias_detection_ignores_managed_blocks() {
        let content =
            upsert_managed_host_block("Host *.example.com other\n    User test\n", &draft("lab"))
                .expect("add");
        let aliases = unmanaged_aliases(&content).expect("aliases");

        assert!(aliases.contains(&"*.example.com".to_string()));
        assert!(aliases.contains(&"other".to_string()));
        assert!(!aliases.contains(&"lab".to_string()));
    }

    #[test]
    fn ssh_config_values_reject_line_break_injection() {
        let mut bad_host = draft("lab");
        bad_host.host_name = "example.com\nProxyCommand powershell".into();
        assert!(normalize_draft(bad_host)
            .expect_err("hostname injection")
            .contains("HostName cannot contain line breaks"));

        let mut bad_user = draft("lab");
        bad_user.user = "codex\r\nIdentityFile C:\\temp\\other".into();
        assert!(normalize_draft(bad_user)
            .expect_err("user injection")
            .contains("User cannot contain line breaks"));

        let mut bad_identity = draft("lab");
        bad_identity.identity_file = "C:\\keys\\id_ed25519\nProxyJump bad".into();
        assert!(normalize_draft(bad_identity)
            .expect_err("identity injection")
            .contains("IdentityFile cannot contain line breaks"));
    }

    #[test]
    fn writer_backs_up_existing_config_before_mutation() {
        let root = env::temp_dir().join(format!("codexhub-ssh-test-{}", timestamp_millis()));
        fs::create_dir_all(&root).expect("create temp root");
        let config = root.join("config");
        fs::write(&config, "Host old\n    HostName old.example\n").expect("write config");

        let backup =
            write_config_with_backup(&config, "Host new\n    HostName new.example\n", true)
                .expect("write with backup")
                .expect("backup path");

        assert!(backup.exists());
        assert_eq!(
            fs::read_to_string(&backup).expect("read backup"),
            "Host old\n    HostName old.example\n"
        );
        assert_eq!(
            fs::read_to_string(&config).expect("read config"),
            "Host new\n    HostName new.example\n"
        );

        fs::remove_dir_all(root).expect("cleanup temp root");
    }

    #[test]
    fn ssh_alias_validation_rejects_wildcards_options_and_shell_tokens() {
        assert!(validate_ssh_alias("lab-box_01").is_ok());
        assert!(validate_ssh_alias("-oProxyCommand=bad").is_err());
        assert!(validate_ssh_alias("*.example.com").is_err());
        assert!(validate_ssh_alias("lab;rm").is_err());
        assert!(validate_ssh_alias("two words").is_err());
    }

    #[test]
    fn windows_public_key_path_keeps_backslash_paths() {
        let public = public_key_path_for(Path::new(r"C:\Users\Example User\.ssh\id_ed25519"));

        assert_eq!(
            public.to_string_lossy(),
            r"C:\Users\Example User\.ssh\id_ed25519.pub"
        );
    }

    #[test]
    fn keyboard_interactive_password_prompt_receives_password_even_when_echo_is_set() {
        let prompt = client::Prompt {
            prompt: "Password:".into(),
            echo: true,
        };

        assert_eq!(
            keyboard_interactive_response(&prompt, "secret-pass", 1),
            "secret-pass"
        );
    }

    #[test]
    fn keyboard_interactive_single_hidden_prompt_receives_password() {
        let prompt = client::Prompt {
            prompt: "Response:".into(),
            echo: false,
        };

        assert_eq!(
            keyboard_interactive_response(&prompt, "secret-pass", 1),
            "secret-pass"
        );
    }

    #[test]
    fn ssh_script_invocation_uses_stdin_shell() {
        let (_, _, args) =
            build_ssh_args("lab", vec!["sh".into(), "-s".into()], 10_000, Vec::new())
                .expect("build args");

        let tail = args
            .iter()
            .rev()
            .take(2)
            .map(String::as_str)
            .collect::<Vec<_>>();
        assert_eq!(tail, vec!["-s", "sh"]);
        assert!(!args.iter().any(|arg| arg == "uname -m"));
    }

    #[test]
    fn ssh_script_stdin_adds_trailing_newline() {
        assert_eq!(script_stdin("printf ok").as_ref(), "printf ok\n");
        assert_eq!(script_stdin("printf ok\n").as_ref(), "printf ok\n");
    }

    #[test]
    fn accept_new_option_is_before_host_alias() {
        let (_, _, args) = build_ssh_args(
            "lab",
            vec!["echo".into(), "ok".into()],
            10_000,
            vec![("StrictHostKeyChecking".into(), "accept-new".into())],
        )
        .expect("build args");

        let host_index = args
            .iter()
            .position(|arg| arg == "lab")
            .expect("host alias");
        let option_index = args
            .iter()
            .position(|arg| arg == "StrictHostKeyChecking=accept-new")
            .expect("accept-new option");
        assert!(option_index < host_index);
    }

    #[test]
    fn process_timeout_kills_child() {
        #[cfg(windows)]
        let (program, args) = (
            "powershell",
            vec![
                "-NoProfile".to_string(),
                "-Command".to_string(),
                "Start-Sleep -Seconds 2".to_string(),
            ],
        );
        #[cfg(not(windows))]
        let (program, args) = ("sh", vec!["-c".to_string(), "sleep 2".to_string()]);

        let output =
            run_process_with_timeout(program, &args, "sleep", 100).expect("run timeout process");

        assert!(output.timed_out);
    }

    #[test]
    fn process_streaming_emits_stdout_and_stderr_lines() {
        #[cfg(windows)]
        let (program, args) = (
            "powershell",
            vec![
                "-NoProfile".to_string(),
                "-Command".to_string(),
                "Write-Output 'stream-out'; [Console]::Error.WriteLine('stream-err')".to_string(),
            ],
        );
        #[cfg(not(windows))]
        let (program, args) = (
            "sh",
            vec![
                "-c".to_string(),
                "printf 'stream-out\\n'; printf 'stream-err\\n' >&2".to_string(),
            ],
        );

        let mut events = Vec::new();
        let output = run_process_with_timeout_streaming(
            program,
            &args,
            "stream-test",
            5_000,
            "",
            &[],
            |event| events.push((event.kind, event.line)),
        )
        .expect("run streaming process");

        assert!(output.success());
        assert!(output.stdout.contains("stream-out"));
        assert!(output.stderr.contains("stream-err"));
        assert!(events
            .iter()
            .any(|(kind, line)| *kind == ProcessStreamKind::Stdout && line == "stream-out"));
        assert!(events
            .iter()
            .any(|(kind, line)| *kind == ProcessStreamKind::Stderr && line == "stream-err"));
    }

    #[test]
    fn redaction_removes_secret_like_output() {
        let output = redact_sensitive(
            "token=sk-test123 password=hunter2\n-----BEGIN OPENSSH PRIVATE KEY-----\nabc123\n-----END OPENSSH PRIVATE KEY-----\nplain text",
        );

        assert!(output.contains("token=[redacted]"));
        assert!(output.contains("password=[redacted]"));
        assert!(output.contains("[redacted private key material]"));
        assert!(output.contains("plain text"));
        assert!(!output.contains("sk-test123"));
        assert!(!output.contains("hunter2"));
        assert!(!output.contains("abc123"));
    }

    #[test]
    fn redaction_preserves_tab_delimited_markers() {
        let output = redact_sensitive(
            "CODEXHUB_REMOTE_SKILL\timagegen\tyes\tvalid\t/home/me/.codex/skills/.system/imagegen\tGenerate token=sk-test123 helper",
        );

        assert!(output.contains("CODEXHUB_REMOTE_SKILL\timagegen\tyes\tvalid"));
        assert!(output.contains("Generate token=[redacted] helper"));
        assert!(!output.contains("sk-test123"));
    }

    #[test]
    fn redaction_removes_one_time_password_value() {
        let output = redact_password("Permission denied for password s3cret-pass", "s3cret-pass");

        assert!(output.contains("[redacted]"));
        assert!(!output.contains("s3cret-pass"));
    }
}
