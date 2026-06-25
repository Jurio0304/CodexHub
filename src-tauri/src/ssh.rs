use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const MANAGED_START_PREFIX: &str = "# >>> CodexHub managed host:";
const MANAGED_END_PREFIX: &str = "# <<< CodexHub managed host:";

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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManagedBlock {
    alias: String,
    range: Range<usize>,
    host: SshConfigHost,
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
        return Err("ssh-keygen was not found on PATH. Install Windows OpenSSH Client first.".into());
    }

    fs::create_dir_all(&ssh_dir).map_err(|error| format!("Failed to create .ssh directory: {error}"))?;
    let comment = format!(
        "codexhub@{}",
        env::var("COMPUTERNAME").unwrap_or_else(|_| "windows".into())
    );
    let output = Command::new("ssh-keygen")
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

pub fn list_ssh_config_hosts() -> Result<Vec<SshConfigHost>, String> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read SSH config {}: {error}", path_string(&path)))?;
    parse_managed_hosts(&content)
}

pub fn upsert_ssh_config_host(draft: SshHostDraft) -> Result<SshConfigWriteResult, String> {
    let draft = normalize_draft(draft)?;
    let path = config_path()?;
    let (existing, existed) = read_optional_config(&path)?;
    let next = upsert_managed_host_block(&existing, &draft)?;

    if next == existing {
        return Ok(SshConfigWriteResult {
            changed: false,
            action: "unchanged".into(),
            config_path: path_string(&path),
            backup_path: None,
            host: Some(host_from_draft(&draft)),
            message: format!("Host {} is already up to date.", draft.alias),
        });
    }

    let backup_path = write_config_with_backup(&path, &next, existed)?;
    let action = if find_managed_host_block(&existing, &draft.alias)?.is_some() {
        "updated"
    } else {
        "added"
    };

    Ok(SshConfigWriteResult {
        changed: true,
        action: action.into(),
        config_path: path_string(&path),
        backup_path: backup_path.as_ref().map(|item| path_string(item)),
        host: Some(host_from_draft(&draft)),
        message: format!("Host {} was {} in SSH config.", draft.alias, action),
    })
}

pub fn delete_ssh_config_host(alias: String) -> Result<SshConfigWriteResult, String> {
    let alias = normalize_alias(&alias)?;
    let path = config_path()?;
    let (existing, existed) = read_optional_config(&path)?;
    let next = delete_managed_host_block(&existing, &alias)?;

    if next == existing {
        return Ok(SshConfigWriteResult {
            changed: false,
            action: "unchanged".into(),
            config_path: path_string(&path),
            backup_path: None,
            host: None,
            message: format!("No CodexHub-managed Host {alias} was found."),
        });
    }

    let backup_path = write_config_with_backup(&path, &next, existed)?;
    Ok(SshConfigWriteResult {
        changed: true,
        action: "deleted".into(),
        config_path: path_string(&path),
        backup_path: backup_path.as_ref().map(|item| path_string(item)),
        host: None,
        message: format!("Deleted CodexHub-managed Host {alias}."),
    })
}

fn ssh_dir() -> Result<PathBuf, String> {
    let profile = env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .ok_or_else(|| "USERPROFILE is not set; cannot locate the Windows .ssh directory.".to_string())?;
    Ok(profile.join(".ssh"))
}

fn config_path() -> Result<PathBuf, String> {
    Ok(ssh_dir()?.join("config"))
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
    #[cfg(windows)]
    {
        Command::new("where")
            .arg(command)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    #[cfg(not(windows))]
    {
        Command::new("sh")
            .arg("-c")
            .arg(format!("command -v {command}"))
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
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

fn write_config_with_backup(path: &Path, content: &str, existed: bool) -> Result<Option<PathBuf>, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("Failed to create .ssh directory: {error}"))?;
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
        alias: normalize_alias(&draft.alias)?,
        host_name: normalize_required("HostName", &draft.host_name)?,
        port: normalize_port(draft.port)?,
        user: normalize_required("User", &draft.user)?,
        identity_file: normalize_required("IdentityFile", &draft.identity_file)?,
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
    }
}

fn parse_managed_hosts(content: &str) -> Result<Vec<SshConfigHost>, String> {
    Ok(scan_managed_blocks(content)?
        .into_iter()
        .map(|block| block.host)
        .collect())
}

fn upsert_managed_host_block(content: &str, draft: &SshHostDraft) -> Result<String, String> {
    if unmanaged_aliases(content)?
        .iter()
        .any(|alias| alias.eq_ignore_ascii_case(&draft.alias))
    {
        return Err(format!(
            "Host {} already exists in an unmanaged SSH config block. CodexHub will not overwrite it.",
            draft.alias
        ));
    }

    let rendered = render_managed_block(draft);
    if let Some(block) = find_managed_host_block(content, &draft.alias)? {
        let lines = split_lines_inclusive(content);
        let mut next = String::new();
        next.push_str(&lines[..block.range.start].concat());
        next.push_str(&rendered);
        next.push_str(&lines[block.range.end..].concat());
        Ok(next)
    } else {
        Ok(append_managed_block(content, &rendered))
    }
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

fn find_managed_host_block(content: &str, alias: &str) -> Result<Option<ManagedBlock>, String> {
    Ok(scan_managed_blocks(content)?
        .into_iter()
        .find(|block| block.alias.eq_ignore_ascii_case(alias)))
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
                .find(|line_index| trim_line(&lines[*line_index]).trim().starts_with(MANAGED_END_PREFIX))
                .ok_or_else(|| format!("Malformed CodexHub managed Host block for {alias}: missing end marker."))?;
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

fn parse_host_from_lines(lines: &[String], managed_alias: Option<String>) -> Result<SshConfigHost, String> {
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
                    alias = value.split_whitespace().next().unwrap_or_default().to_string();
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
    })
}

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

fn render_managed_block(draft: &SshHostDraft) -> String {
    format!(
        "{MANAGED_START_PREFIX} {alias}\nHost {alias}\n    HostName {host_name}\n    Port {port}\n    User {user}\n    IdentityFile {identity_file}\n{MANAGED_END_PREFIX} {alias}\n",
        alias = draft.alias,
        host_name = draft.host_name,
        port = draft.port,
        user = draft.user,
        identity_file = draft.identity_file
    )
}

fn append_managed_block(content: &str, block: &str) -> String {
    if content.is_empty() {
        return block.into();
    }

    let mut next = content.to_string();
    if !next.ends_with('\n') {
        next.push('\n');
    }
    next.push('\n');
    next.push_str(block);
    next
}

fn split_lines_inclusive(content: &str) -> Vec<String> {
    content.split_inclusive('\n').map(str::to_string).collect()
}

fn trim_line(line: &str) -> &str {
    line.trim_end_matches(['\r', '\n'])
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

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(alias: &str) -> SshHostDraft {
        SshHostDraft {
            alias: alias.into(),
            host_name: "example.com".into(),
            port: 2222,
            user: "codex".into(),
            identity_file: r"C:\Users\PC\.ssh\id_ed25519".into(),
        }
    }

    #[test]
    fn parser_returns_managed_hosts_and_preserves_unmanaged_blocks() {
        let content = "Host github.com\n    HostName github.com\n\n# >>> CodexHub managed host: lab\nHost lab\n    HostName 10.0.0.5\n    Port 22\n    User jurio\n    IdentityFile C:\\Users\\PC\\.ssh\\id_ed25519\n# <<< CodexHub managed host: lab\n";

        let hosts = parse_managed_hosts(content).expect("parse hosts");

        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].alias, "lab");
        assert_eq!(hosts[0].host_name, "10.0.0.5");
        assert_eq!(hosts[0].identity_file, r"C:\Users\PC\.ssh\id_ed25519");
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
    fn update_managed_block_in_place() {
        let first = upsert_managed_host_block("Host github.com\n    User git\n", &draft("lab")).expect("first");
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
    fn reject_unmanaged_duplicate_alias() {
        let content = "Host lab\n    HostName unmanaged.example\n";
        let error = upsert_managed_host_block(content, &draft("lab")).expect_err("must reject");

        assert!(error.contains("unmanaged SSH config block"));
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
    fn unmanaged_alias_detection_ignores_managed_blocks() {
        let content = upsert_managed_host_block("Host *.example.com other\n    User test\n", &draft("lab"))
            .expect("add");
        let aliases = unmanaged_aliases(&content).expect("aliases");

        assert!(aliases.contains(&"*.example.com".to_string()));
        assert!(aliases.contains(&"other".to_string()));
        assert!(!aliases.contains(&"lab".to_string()));
    }

    #[test]
    fn writer_backs_up_existing_config_before_mutation() {
        let root = env::temp_dir().join(format!("codexhub-ssh-test-{}", timestamp_millis()));
        fs::create_dir_all(&root).expect("create temp root");
        let config = root.join("config");
        fs::write(&config, "Host old\n    HostName old.example\n").expect("write config");

        let backup = write_config_with_backup(&config, "Host new\n    HostName new.example\n", true)
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
}
