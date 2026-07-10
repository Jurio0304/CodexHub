use serde::Serialize;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use ts_rs::TS;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "RuntimePlatformDto")]
pub enum RuntimePlatform {
    Windows,
    MacOS,
    Linux,
}

pub fn get_platform() -> RuntimePlatform {
    if cfg!(target_os = "windows") {
        RuntimePlatform::Windows
    } else if cfg!(target_os = "macos") {
        RuntimePlatform::MacOS
    } else {
        RuntimePlatform::Linux
    }
}

pub fn is_windows() -> bool {
    get_platform() == RuntimePlatform::Windows
}

#[allow(dead_code)]
pub fn is_macos() -> bool {
    get_platform() == RuntimePlatform::MacOS
}

#[allow(dead_code)]
pub fn is_linux() -> bool {
    get_platform() == RuntimePlatform::Linux
}

pub fn get_home_dir() -> Result<PathBuf, String> {
    let platform = get_platform();
    let home = match platform {
        RuntimePlatform::Windows => env::var_os("USERPROFILE")
            .or_else(|| env::var_os("HOME"))
            .map(PathBuf::from),
        RuntimePlatform::MacOS | RuntimePlatform::Linux => env::var_os("HOME")
            .or_else(|| env::var_os("USERPROFILE"))
            .map(PathBuf::from),
    };
    home.filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| {
            format!(
                "{} is not set; cannot locate the user home directory.",
                home_env_label(platform)
            )
        })
}

pub fn get_ssh_dir() -> Result<PathBuf, String> {
    Ok(get_ssh_dir_for_home(&get_home_dir()?))
}

pub fn get_ssh_config_path() -> Result<PathBuf, String> {
    Ok(get_ssh_dir()?.join("config"))
}

#[allow(dead_code)]
pub fn get_default_ssh_key_path() -> Result<PathBuf, String> {
    Ok(get_default_ssh_key_path_for_home(&get_home_dir()?))
}

#[allow(dead_code)]
pub fn get_codex_config_path() -> Result<PathBuf, String> {
    Ok(get_codex_config_path_for_home(&get_home_dir()?))
}

#[allow(dead_code)]
pub fn get_codex_skills_path() -> Result<PathBuf, String> {
    Ok(get_codex_skills_path_for_home(&get_home_dir()?))
}

pub fn detect_codex_binary_path() -> Option<PathBuf> {
    for candidate in codex_binary_candidates_for_current_home() {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    command_path("codex")
}

pub fn codex_binary_candidates_for_current_home() -> Vec<PathBuf> {
    let home = get_home_dir().unwrap_or_else(|_| PathBuf::from("~"));
    codex_binary_candidates_for_home(get_platform(), &home)
}

pub fn codex_binary_candidates_for_home(platform: RuntimePlatform, home: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    match platform {
        RuntimePlatform::MacOS => {
            push_unique(&mut paths, PathBuf::from("/opt/homebrew/bin/codex"));
            push_unique(&mut paths, PathBuf::from("/usr/local/bin/codex"));
            push_unique(&mut paths, home.join(".local/bin/codex"));
        }
        RuntimePlatform::Windows => {
            push_unique(&mut paths, home.join(".local/bin/codex.exe"));
            push_unique(&mut paths, home.join("AppData/Roaming/npm/codex.cmd"));
        }
        RuntimePlatform::Linux => {
            push_unique(&mut paths, home.join(".local/bin/codex"));
            push_unique(&mut paths, home.join(".npm-global/bin/codex"));
        }
    }
    paths
}

pub fn get_ssh_dir_for_home(home: &Path) -> PathBuf {
    home.join(".ssh")
}

#[allow(dead_code)]
pub fn get_default_ssh_key_path_for_home(home: &Path) -> PathBuf {
    get_ssh_dir_for_home(home).join("id_ed25519")
}

#[allow(dead_code)]
pub fn get_codex_config_path_for_home(home: &Path) -> PathBuf {
    home.join(".codex/config.toml")
}

#[allow(dead_code)]
pub fn get_codex_skills_path_for_home(home: &Path) -> PathBuf {
    home.join(".codex/skills")
}

pub fn expand_home_path(input: &str) -> Result<PathBuf, String> {
    expand_home_path_with_home(input, &get_home_dir()?)
}

pub fn expand_home_path_with_home(input: &str, home: &Path) -> Result<PathBuf, String> {
    let value = input.trim();
    if value == "~" {
        return Ok(home.to_path_buf());
    }
    if let Some(rest) = value
        .strip_prefix("~/")
        .or_else(|| value.strip_prefix("~\\"))
    {
        return Ok(home_join_relative(home, rest));
    }
    if let Some(rest) = value.strip_prefix("$HOME/") {
        return Ok(home_join_relative(home, rest));
    }
    if let Some(rest) = value.strip_prefix("${HOME}/") {
        return Ok(home_join_relative(home, rest));
    }
    if let Some(rest) = value.strip_prefix("%USERPROFILE%\\") {
        return Ok(home_join_relative(home, rest));
    }
    if let Some(rest) = value.strip_prefix("%USERPROFILE%/") {
        return Ok(home_join_relative(home, rest));
    }
    if value == "%USERPROFILE%" {
        return Ok(home.to_path_buf());
    }
    Ok(PathBuf::from(value))
}

fn home_join_relative(home: &Path, rest: &str) -> PathBuf {
    let separator = std::path::MAIN_SEPARATOR.to_string();
    let normalized = rest.replace('\\', &separator).replace('/', &separator);
    home.join(normalized)
}

pub fn command_available(command: &str) -> bool {
    command_path(command).is_some()
}

pub fn command_path(command: &str) -> Option<PathBuf> {
    if command.trim().is_empty() {
        return None;
    }
    let output = if cfg!(windows) {
        let mut command_runner = process_command("where");
        command_runner.arg(command).output()
    } else {
        let mut command_runner = process_command("sh");
        command_runner
            .arg("-c")
            .arg("command -v \"$1\"")
            .arg("sh")
            .arg(command)
            .output()
    }
    .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(PathBuf::from)
}

pub fn run_version_command(program: &Path) -> Option<String> {
    let output = version_command(program)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .or_else(|| {
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .next()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
        })
}

fn version_command(program: &Path) -> Command {
    #[cfg(windows)]
    {
        let extension = program
            .extension()
            .and_then(|item| item.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(extension.as_str(), "cmd" | "bat") {
            let mut command = process_command("cmd");
            command.arg("/C").arg(program).arg("--version");
            return command;
        }
    }

    let mut command = process_command(program);
    command.arg("--version");
    command
}

fn process_command<P: AsRef<std::ffi::OsStr>>(program: P) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

fn home_env_label(platform: RuntimePlatform) -> &'static str {
    match platform {
        RuntimePlatform::Windows => "USERPROFILE",
        RuntimePlatform::MacOS | RuntimePlatform::Linux => "HOME",
    }
}

fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|item| item == &path) {
        paths.push(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_paths_follow_codexhub_contract() {
        let home = Path::new("/Users/codexhub");

        assert_eq!(
            get_ssh_dir_for_home(home),
            PathBuf::from("/Users/codexhub/.ssh")
        );
        assert_eq!(
            get_default_ssh_key_path_for_home(home),
            PathBuf::from("/Users/codexhub/.ssh/id_ed25519")
        );
        assert_eq!(
            get_codex_config_path_for_home(home),
            PathBuf::from("/Users/codexhub/.codex/config.toml")
        );
        assert_eq!(
            get_codex_skills_path_for_home(home),
            PathBuf::from("/Users/codexhub/.codex/skills")
        );
    }

    #[test]
    fn macos_codex_binary_candidates_keep_required_order() {
        let home = Path::new("/Users/codexhub");
        let candidates = codex_binary_candidates_for_home(RuntimePlatform::MacOS, home);

        assert_eq!(
            candidates,
            vec![
                PathBuf::from("/opt/homebrew/bin/codex"),
                PathBuf::from("/usr/local/bin/codex"),
                PathBuf::from("/Users/codexhub/.local/bin/codex")
            ]
        );
    }

    #[test]
    fn expands_home_tokens_without_requiring_process_env() {
        let home = Path::new("/Users/codexhub");

        assert_eq!(
            expand_home_path_with_home("~/.ssh/id_ed25519", home).expect("expand tilde"),
            PathBuf::from("/Users/codexhub/.ssh/id_ed25519")
        );
        assert_eq!(
            expand_home_path_with_home("$HOME/.codex/config.toml", home).expect("expand $HOME"),
            PathBuf::from("/Users/codexhub/.codex/config.toml")
        );
        assert_eq!(
            expand_home_path_with_home("%USERPROFILE%\\.ssh\\id_ed25519", home)
                .expect("expand userprofile"),
            PathBuf::from("/Users/codexhub/.ssh/id_ed25519")
        );
    }

    #[test]
    fn platform_flags_are_mutually_exclusive() {
        let true_count = [is_windows(), is_macos(), is_linux()]
            .into_iter()
            .filter(|value| *value)
            .count();

        assert_eq!(true_count, 1);
    }
}
